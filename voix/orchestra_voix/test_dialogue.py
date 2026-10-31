"""Test de dialogue : plusieurs phrases à la suite sur la même connexion, micro ouvert entre les deux.

    python -m orchestra_voix.test_dialogue a.wav b.wav [c.wav…]

Comme un micro réel : l'audio est envoyé en continu (silence entre les phrases), on attend
la fin de la réponse avant la phrase suivante. Affiche le premier son de chaque réponse.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import time
import wave

import numpy as np
import websockets

STEP = 640  # 20 ms à 16 kHz, 16 bits


def load(path):
    with wave.open(path) as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1
        pcm = w.readframes(w.getnframes())
    x = np.abs(np.frombuffer(pcm, dtype=np.int16))
    loud = np.nonzero(x > 800)[0]
    return pcm, (loud[-1] if len(loud) else len(x)) / 16000


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wavs", nargs="+")
    ap.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()

    async with websockets.connect(f"ws://127.0.0.1:{a.port}", max_size=None) as ws:
        state = {"t_end": None, "first": None, "bot_done": asyncio.Event(), "last_audio": 0.0}

        async def recv():
            async for m in ws:
                now = time.perf_counter()
                if isinstance(m, bytes):
                    if state["t_end"] and state["first"] is None:
                        state["first"] = now - state["t_end"]
                    state["last_audio"] = now
                    state["got"] = True
                else:
                    e = json.loads(m)
                    if e["type"] in ("user_final", "delegate", "thinking", "decision", "thought"):
                        print(f"    {e}")
                    if e["type"] == "bot_end":
                        state["bot_done"].set()

        task = asyncio.create_task(recv())
        await asyncio.sleep(0.5)

        async def silence(seconds):
            for _ in range(int(seconds / 0.02)):
                await ws.send(bytes(STEP))
                await asyncio.sleep(0.02)

        await silence(0.5)
        for path in a.wavs:
            pcm, speech_end = load(path)
            state.update(t_end=None, first=None, last_audio=time.perf_counter() + 60)
            state["bot_done"].clear()
            print(f"> {path}")
            start = time.perf_counter()
            for i in range(0, len(pcm), STEP):
                await ws.send(pcm[i : i + STEP])
                wait = start + (i + STEP) / 32000 - time.perf_counter()
                if wait > 0:
                    await asyncio.sleep(wait)
            state["t_end"] = start + speech_end
            # micro ouvert : silence jusqu'à la fin de la réponse (et de sa lecture)
            deadline = time.perf_counter() + 90
            while time.perf_counter() < deadline and not (state["bot_done"].is_set() and state["first"]):
                await silence(0.1)
            # fin de la lecture : plus aucun son reçu depuis 1,5 s (le serveur envoie à vitesse réelle)
            while time.perf_counter() < deadline and time.perf_counter() - state["last_audio"] < 1.5:
                await silence(0.1)
            first = state["first"]
            print(f"    premier son : {f'{first * 1000:.0f} ms' if first else 'AUCUN'}")
        task.cancel()


if __name__ == "__main__":
    asyncio.run(main())
