"""Processus séparé pour les voix CrispASR (Voxtral TTS, Pocket TTS).

CrispASR et transcribe.cpp (l'écoute) embarquent chacun leur propre `ggml.dll` ; sous
Windows, un seul des deux peut être chargé dans un même processus. La synthèse CrispASR
tourne donc ici, à part, et dialogue avec le serveur voix par ses entrées/sorties :

    entrée  : une ligne JSON par demande, {"text": "…"}
    sortie  : d'abord une ligne texte « READY <fréquence> », puis pour chaque demande une suite
              de morceaux [int32 n][n échantillons float32], terminée par n = 0
              (n = -1 : erreur, suivie de [int32 longueur][message utf-8]).

    python -m orchestra_voix.tts_worker <modèle.gguf> <voix>
"""

from __future__ import annotations

import json
import struct
import sys

import numpy as np


def main():
    model, voice = sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else ""
    from pathlib import Path

    from .tts_crisp import open_session, split_for_speech

    out = sys.stdout.buffer
    session, engine = open_session(Path(model), voice)
    rate = int(session.output_sample_rate() or 24000)
    session.synthesize("Bonjour.")  # préchauffage
    out.write(f"READY {rate} {engine}\n".encode())
    out.flush()

    for line in sys.stdin.buffer:
        try:
            req = json.loads(line)
        except ValueError:
            continue
        try:
            if req.get("voice"):
                for fn in ("set_speaker_name", "set_voice"):
                    try:
                        getattr(session, fn)(req["voice"])
                        break
                    except Exception:
                        continue
            for piece in split_for_speech(str(req.get("text", ""))):
                pcm = np.asarray(session.synthesize(piece), dtype=np.float32).reshape(-1)
                out.write(struct.pack("<i", len(pcm)))
                out.write(pcm.tobytes())
                out.flush()
            out.write(struct.pack("<i", 0))
        except Exception as e:
            msg = str(e).encode("utf-8")
            out.write(struct.pack("<i", -1) + struct.pack("<i", len(msg)) + msg)
        out.flush()


if __name__ == "__main__":
    main()
