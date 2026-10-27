"""Mesure de la latence réelle de chaque brique, sur cette machine.

    python -m orchestra_voix.bench [--tours 5]

1. Voix : Qwen3-TTS synthétise des phrases types ; on mesure le délai du
   premier son (première phrase) et la vitesse (facteur temps réel).
2. Écoute : la phrase synthétisée est rejouée à Voxtral à vitesse réelle, morceau par
   morceau ; on mesure le délai entre la fin de l'audio et le texte final.
3. Cerveau : délai du premier mot de Qwen 3.5 9B via Ollama, sans réflexion.
4. Total estimé = attente du VAD + écoute + cerveau + voix.

Le rapport est écrit dans voix/mesures/rapport-AAAA-MM-JJ.md (et .json).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import statistics
import subprocess
import time
from datetime import datetime

import numpy as np

from . import config as C

PHRASES = [
    "Bonjour, je suis Orchestra. Que puis-je faire pour toi aujourd'hui ?",
    "D'accord, je m'en occupe tout de suite et je te préviens dès que c'est prêt.",
    "Il est quinze heures, et tu as deux rendez-vous cet après-midi.",
]
QUESTIONS = [
    "Bonjour, comment vas-tu ?",
    "Peux-tu me rappeler ce que tu sais de moi en une phrase ?",
    "Quelle est la capitale de l'Australie ?",
]


def vram() -> str:
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.used,memory.total", "--format=csv,noheader"],
            capture_output=True, text=True, timeout=10,
        ).stdout.strip()
        return out
    except Exception as e:
        return f"inconnu ({e})"


def resample(x: np.ndarray, sr: int, to: int) -> np.ndarray:
    if sr == to:
        return x
    n = int(len(x) * to / sr)
    return np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype(np.float32)


def bench_qwen3(tours: int):
    from faster_qwen3_tts import FasterQwen3TTS

    t0 = time.perf_counter()
    m = FasterQwen3TTS.from_pretrained(str(C.QWEN3_TTS), local_files_only=True)
    load = time.perf_counter() - t0
    stream = lambda t: m.generate_custom_voice_streaming(text=t, language="French", speaker="Ryan", chunk_size=4)  # noqa: E731
    for _ in stream("Bonjour, je suis prête."):  # chauffe : capture des graphes CUDA
        pass
    firsts, rtfs, audio = [], [], None
    for _ in range(tours):
        for p in PHRASES:
            t = time.perf_counter()
            parts, sr, first = [], 24000, None
            for chunk, sr, _timing in stream(p):
                first = first or (time.perf_counter() - t)
                parts.append(np.asarray(chunk, dtype=np.float32).reshape(-1))
            dt = time.perf_counter() - t
            w = np.concatenate(parts) if parts else np.zeros(1, dtype=np.float32)
            firsts.append(first or dt)
            rtfs.append(dt / (len(w) / sr))
            audio = audio if audio is not None else (w, sr)
    return {"chargement_s": round(load, 1), "premier_son_ms": med(firsts), "facteur_temps_reel": round(statistics.median(rtfs), 2), "vram": vram()}, audio


def bench_voxtral(audio: tuple[np.ndarray, int], tours: int, delay_ms: int = 480):
    import os

    lib = C.transcribe_library()
    if lib:
        os.environ["TRANSCRIBE_LIBRARY"] = str(lib)
        os.add_dll_directory(str(lib.parent))
    import transcribe_cpp

    t0 = time.perf_counter()
    model = transcribe_cpp.Model(str(C.VOXTRAL_GGUF))
    load = time.perf_counter() - t0
    pcm = resample(audio[0], audio[1], 16000)
    chunk = 320  # 20 ms
    finals, texte = [], ""
    with model.session() as session:
        for _ in range(tours):
            opts = transcribe_cpp.VoxtralRealtimeStreamOptions(num_delay_tokens=max(1, round(delay_ms / 80)))
            with session.stream(family=opts) as st:
                start = time.perf_counter()
                for i in range(0, len(pcm), chunk):
                    st.feed(pcm[i : i + chunk])
                    # rythme réel : on ne donne pas l'audio plus vite qu'on ne parle
                    wait = start + (i + chunk) / 16000 - time.perf_counter()
                    if wait > 0:
                        time.sleep(wait)
                t = time.perf_counter()
                st.feed(np.zeros(int(16000 * 0.3), dtype=np.float32))
                st.finalize()
                snap = st.snapshot()
                finals.append(time.perf_counter() - t)
                texte = snap.text or st.text().display
    return {"chargement_s": round(load, 1), "texte_final_ms": med(finals), "transcription": texte, "vram": vram()}


def bench_llm(model: str, tours: int, url: str = "http://127.0.0.1:11434/v1"):
    import httpx

    ttfts = []
    with httpx.Client(timeout=120) as c:
        c.post(url.replace("/v1", "/api/generate"), json={"model": model, "prompt": "", "keep_alive": "30m"})
        for _ in range(tours):
            for q in QUESTIONS:
                t = time.perf_counter()
                body = {"model": model, "stream": True, "max_tokens": 60, "reasoning_effort": "none",
                        "messages": [{"role": "system", "content": "Réponds en une phrase courte, en français."}, {"role": "user", "content": q}]}
                with c.stream("POST", f"{url}/chat/completions", json=body) as r:
                    for line in r.iter_lines():
                        if not line.startswith("data: ") or line.endswith("[DONE]"):
                            continue
                        d = json.loads(line[6:])
                        delta = d["choices"][0]["delta"] if d.get("choices") else {}
                        if delta.get("content"):
                            ttfts.append(time.perf_counter() - t)
                            break
    return {"modele": model, "premier_mot_ms": med(ttfts), "vram": vram()}


def med(xs):
    return round(statistics.median(xs) * 1000) if xs else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tours", type=int, default=3)
    ap.add_argument("--llm", default="qwen3.5:9b")
    ap.add_argument("--vad-stop", type=float, default=0.35)
    ap.add_argument("--sans-voxtral", action="store_true", help="tant que transcribe.cpp n'est pas compilé")
    a = ap.parse_args()
    r: dict = {"date": datetime.now().isoformat(timespec="seconds"), "vram_depart": vram()}

    print("Cerveau…", flush=True)
    r["cerveau"] = bench_llm(a.llm, a.tours)
    print(r["cerveau"], flush=True)
    print("Qwen3-TTS…", flush=True)
    r["qwen3_tts"], audio = bench_qwen3(a.tours)
    print(r["qwen3_tts"], flush=True)
    if a.sans_voxtral:
        r["voxtral"] = {"texte_final_ms": None, "transcription": "(non mesuré)", "vram": None}
    else:
        print("Voxtral…", flush=True)
        r["voxtral"] = bench_voxtral(audio, a.tours)
    print(r["voxtral"], flush=True)

    vad = round(a.vad_stop * 1000)
    base = vad + (r["voxtral"]["texte_final_ms"] or 0) + (r["cerveau"]["premier_mot_ms"] or 0)
    r["total_estime_ms"] = {
        "qwen3_tts": base + (r["qwen3_tts"]["premier_son_ms"] or 0),
        "detail": f"VAD {vad} + écoute + cerveau + voix",
    }
    C.MESURES.mkdir(parents=True, exist_ok=True)
    day = datetime.now().strftime("%Y-%m-%d")
    (C.MESURES / f"rapport-{day}.json").write_text(json.dumps(r, ensure_ascii=False, indent=2), encoding="utf-8")
    md = rapport_md(r, vad)
    (C.MESURES / f"rapport-{day}.md").write_text(md, encoding="utf-8")
    print(md)


def rapport_md(r: dict, vad: int) -> str:
    q, v, l = r["qwen3_tts"], r["voxtral"], r["cerveau"]
    return f"""# Latence du mode vocal — {r['date']}

| Étape | Mesure |
|---|---|
| Attente de fin de parole (VAD) | {vad} ms |
| Écoute : Voxtral, texte final après la fin de l'audio | {v['texte_final_ms']} ms |
| Cerveau : {l['modele']}, premier mot | {l['premier_mot_ms']} ms |
| Voix : Qwen3-TTS 1.7B, premier son | {q['premier_son_ms']} ms (temps réel × {q['facteur_temps_reel']}) |
| **Total estimé avec Qwen3-TTS** | **{r['total_estime_ms']['qwen3_tts']} ms** |

Transcription de contrôle : « {v['transcription']} »

Mémoire GPU : départ {r['vram_depart']} · après Qwen3-TTS {q.get('vram')} · après Voxtral {v.get('vram')}

Facteur temps réel : temps de calcul divisé par la durée de l'audio (moins de 1 = plus rapide que la parole).
"""


if __name__ == "__main__":
    main()
