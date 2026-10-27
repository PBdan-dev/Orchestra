"""Test de bout en bout du serveur voix : joue un WAV comme si c'était le micro.

    python -m orchestra_voix.test_boucle fichier.wav [--port 8765]

Envoie l'audio à vitesse réelle (morceaux de 20 ms), puis du silence, et affiche les
événements reçus, le délai entre la fin de l'audio et le premier son de la réponse, et
enregistre la réponse dans mesures/reponse-test.wav.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import time
import wave

import numpy as np
import websockets

from . import config as C


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wav")
    ap.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()
    with wave.open(a.wav) as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1
        pcm = w.readframes(w.getnframes())
    # fin réelle de la parole (le fichier se termine souvent par du silence)
    x = np.abs(np.frombuffer(pcm, dtype=np.int16))
    loud = np.nonzero(x > 800)[0]
    speech_end = (loud[-1] if len(loud) else len(x)) / 16000
    print(f"parole : {speech_end:.2f} s sur {len(x) / 16000:.2f} s")

    async with websockets.connect(f"ws://127.0.0.1:{a.port}", max_size=None) as ws:
        out = bytearray()
        t_end = None
        t_first = None
        done = asyncio.Event()

        async def recv():
            nonlocal t_first
            async for m in ws:
                now = time.perf_counter()
                if isinstance(m, bytes):
                    if t_end and not t_first:
                        t_first = now
                        print(f"  >> premier son {1000 * (now - t_end):.0f} ms après la fin de l'audio")
                    out.extend(m)
                else:
                    e = json.loads(m)
                    if e["type"] != "bot_text":
                        stamp = f"+{1000 * (now - t_end):.0f} ms" if t_end else "pendant"
                        print(f"  [{stamp}] {e}")
                    if e["type"] == "bot_end":
                        asyncio.get_running_loop().call_later(4, done.set)

        task = asyncio.create_task(recv())
        await asyncio.sleep(1)
        await ws.send(json.dumps({"type": "context", "messages": []}))
        start = time.perf_counter()
        step = 640  # 20 ms
        for i in range(0, len(pcm), step):
            await ws.send(pcm[i : i + step])
            wait = start + (i + step) / 32000 - time.perf_counter()
            if wait > 0:
                await asyncio.sleep(wait)
        t_end = start + speech_end  # mesures depuis la fin réelle de la parole
        # silence : le micro reste ouvert
        for _ in range(400):
            if done.is_set():
                break
            await ws.send(bytes(step))
            await asyncio.sleep(0.02)
        task.cancel()

    if out:
        C.MESURES.mkdir(parents=True, exist_ok=True)
        p = C.MESURES / "reponse-test.wav"
        with wave.open(str(p), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(24000)
            w.writeframes(bytes(out))
        print(f"réponse enregistrée : {p} ({len(out) / 48000:.1f} s)")


if __name__ == "__main__":
    asyncio.run(main())
