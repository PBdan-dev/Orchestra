"""Test rapide de Voxtral seul, sur un fichier WAV 16 kHz mono, rejoué à vitesse réelle.

    python -m orchestra_voix.test_voxtral fichier.wav [--backend auto|vulkan|cuda|cpu]
"""

from __future__ import annotations

import argparse
import os
import time
import wave

import numpy as np

from . import config as C


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wav")
    ap.add_argument("--backend", default="auto")
    ap.add_argument("--delay-ms", type=int, default=480)
    a = ap.parse_args()

    lib = C.transcribe_library()
    if lib:
        os.environ["TRANSCRIBE_LIBRARY"] = str(lib)
    import transcribe_cpp

    with wave.open(a.wav) as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1, "WAV 16 kHz mono attendu"
        pcm = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768

    t0 = time.perf_counter()
    model = transcribe_cpp.Model(str(C.VOXTRAL_GGUF), backend=a.backend)
    print(f"chargement : {time.perf_counter() - t0:.1f} s")
    for attr in ("backend", "device", "backend_name"):
        if hasattr(model, attr):
            print(f"{attr} : {getattr(model, attr)}")

    opts = transcribe_cpp.VoxtralRealtimeStreamOptions(num_delay_tokens=max(1, round(a.delay_ms / 80)))
    with model.session() as session:
        for tour in range(2):
            with session.stream(family=opts) as st:
                start = time.perf_counter()
                lag = []
                for i in range(0, len(pcm), 320):
                    t = time.perf_counter()
                    st.feed(pcm[i : i + 320])
                    lag.append(time.perf_counter() - t)
                    wait = start + (i + 320) / 16000 - time.perf_counter()
                    if wait > 0:
                        time.sleep(wait)
                late = time.perf_counter() - (start + len(pcm) / 16000)
                t = time.perf_counter()
                st.feed(np.zeros(int(16000 * 0.3), dtype=np.float32))
                st.finalize()
                snap = st.snapshot()
                fin = time.perf_counter() - t
            print(
                f"tour {tour + 1} : texte final {fin * 1000:.0f} ms après la fin de l'audio "
                f"(retard accumulé {max(0, late) * 1000:.0f} ms, feed max {max(lag) * 1000:.0f} ms) : {snap.text!r}"
            )


if __name__ == "__main__":
    main()
