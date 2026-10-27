"""Compare les voix sur les mêmes phrases : un fichier WAV par voix et par phrase, et le délai.

    python -m orchestra_voix.compare_voix

Écrit dans voix/mesures/comparaison/ :
    qwen3-ryan-1.wav, voxtral-fr_female-1.wav, pocket-developpeuse-1.wav, …
et un résumé comparaison.md (premier son, durée de calcul, facteur temps réel).
"""

from __future__ import annotations

import statistics
import time
import wave

import numpy as np

from . import config as C

PHRASES = [
    "Bonjour ! Je suis Orchestra. Qu'est-ce que je peux faire pour toi aujourd'hui ?",
    "D'accord, je m'en occupe tout de suite, et je te préviens dès que c'est prêt.",
    "Il est quinze heures trente. Tu as deux rendez-vous cet après-midi, dont un avec Claire à seize heures.",
]
OUT = C.MESURES / "comparaison"


def save(path, pcm: np.ndarray, sr: int):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes((np.clip(pcm, -1, 1) * 32767).astype(np.int16).tobytes())


def run_qwen3():
    from faster_qwen3_tts import FasterQwen3TTS

    m = FasterQwen3TTS.from_pretrained(str(C.QWEN3_TTS), local_files_only=True)
    for _ in m.generate_custom_voice_streaming(text="Bonjour.", speaker="Ryan", language="French", chunk_size=2):
        pass
    res = []
    for i, p in enumerate(PHRASES, 1):
        t = time.perf_counter()
        first, parts, sr = None, [], 24000
        for chunk, sr, _ in m.generate_custom_voice_streaming(text=p, speaker="Ryan", language="French", chunk_size=2):
            first = first or time.perf_counter() - t
            parts.append(np.asarray(chunk, dtype=np.float32).reshape(-1))
        dt = time.perf_counter() - t
        pcm = np.concatenate(parts)
        save(OUT / f"qwen3-ryan-{i}.wav", pcm, sr)
        res.append((first, dt, len(pcm) / sr))
    del m
    return res


def run_crisp(model, voice, tag):
    from .tts_crisp import open_session
    from .tts_crisp import split_for_speech

    s, engine = open_session(model, voice)
    sr = int(s.output_sample_rate() or 24000)
    s.synthesize("Bonjour.")  # préchauffage
    res = []
    for i, p in enumerate(PHRASES, 1):
        t = time.perf_counter()
        first, parts = None, []
        for piece in split_for_speech(p):
            parts.append(np.asarray(s.synthesize(piece), dtype=np.float32).reshape(-1))
            first = first or time.perf_counter() - t
        dt = time.perf_counter() - t
        pcm = np.concatenate(parts)
        save(OUT / f"{tag}-{i}.wav", pcm, sr)
        res.append((first, dt, len(pcm) / sr))
    s.close()
    return res


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    voix = C.MODELES / "pocket-tts" / "voix" / "unmute-prod-website" / "developpeuse-3.wav"
    import sys
    only = sys.argv[1:]  # ex. : voxtral pocket
    runs = [
        ("Qwen3-TTS 1.7B, Ryan (commercial)", "qwen3-ryan", run_qwen3),
        ("Voxtral TTS, fr_female (usage personnel)", "voxtral-fr_female",
         lambda: run_crisp(C.MODELES / "voxtral-tts" / "voxtral-4b-tts-q8_0.gguf", "fr_female", "voxtral-fr_female")),
        ("Pocket TTS français, développeuse (usage personnel)", "pocket-developpeuse",
         lambda: run_crisp(C.MODELES / "pocket-tts" / "pocket-tts-french_24l-q8_0.gguf", str(voix), "pocket-developpeuse")),
    ]
    lines = ["# Comparaison des voix", "", "| Voix | Premier son | Temps réel × | Fichiers |", "|---|---|---|---|"]
    for title, tag, fn in runs:
        if only and not any(o in tag for o in only):
            continue
        print(f"> {title}", flush=True)
        try:
            r = fn()
            first = statistics.median(x[0] for x in r) * 1000
            rtf = statistics.median(x[1] / x[2] for x in r)
            print(f"  premier son {first:.0f} ms, temps réel × {rtf:.2f}", flush=True)
            lines.append(f"| {title} | {first:.0f} ms | {rtf:.2f} | `{tag}-1.wav` à `-3.wav` |")
        except Exception as e:
            print(f"  ÉCHEC : {e}", flush=True)
            lines.append(f"| {title} | échec : {e} | | |")
    lines += ["", "Temps réel × : temps de calcul divisé par la durée de l'audio (moins de 1 = plus rapide que la parole).",
              "Phrases :", *[f"{i}. {p}" for i, p in enumerate(PHRASES, 1)]]
    (OUT / "comparaison.md").write_text("\n".join(lines), encoding="utf-8")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
