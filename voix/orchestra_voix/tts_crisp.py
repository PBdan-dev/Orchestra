"""Connecteur Pipecat pour les voix servies par CrispASR (moteur MIT, GPU par Vulkan).

    USAGE PERSONNEL UNIQUEMENT. Les modèles branchés ici ont des licences non
    commerciales et ne doivent jamais être proposés dans la version commerciale
    d'Orchestra (seul Qwen3-TTS l'est) :
      - Voxtral TTS (Mistral, CC BY-NC 4.0), voix préréglée « fr_female »
      - Pocket TTS (Kyutai, CC-BY 4.0 avec conditions d'usage), voix de référence
        « developpeuse-3 » (enregistrement Kyutai, CC0)

CrispASR génère phrase par phrase (`synthesize_streaming`) : le premier son sort dès que
la première phrase est prête.
"""

from __future__ import annotations

import asyncio
import threading
import time
from collections.abc import AsyncGenerator
from pathlib import Path

import numpy as np
from loguru import logger

from pipecat.audio.utils import create_stream_resampler
from pipecat.frames.frames import ErrorFrame, Frame, TTSAudioRawFrame
from pipecat.services.settings import TTSSettings
from pipecat.services.tts_service import TTSService
from pipecat.transcriptions.language import Language

import re

CHUNK_MS = 40

# Licences, affichées dans l'interface : tout ce qui n'est pas « commercial » est réservé à l'usage personnel.
LICENCES = {
    "voxtral-tts": "CC BY-NC 4.0 · usage personnel",
    "pocket-tts": "CC-BY 4.0 + conditions Kyutai · usage personnel",
}


def split_for_speech(text: str, first_max: int = 60, max_len: int = 160) -> list[str]:
    """Découpe sur la ponctuation ; le premier morceau est court pour réduire le délai."""
    text = re.sub(r"\s+", " ", text).strip()
    if not text:
        return []
    parts = [p.strip() for p in re.split(r"(?<=[,;:.!?…])\s+", text) if p.strip()]
    out: list[str] = []
    buf = ""
    for p in parts:
        limit = first_max if not out else max_len
        if buf and len(buf) + 1 + len(p) > limit:
            out.append(buf)
            buf = p
        else:
            buf = f"{buf} {p}".strip()
    if buf:
        out.append(buf)
    return out


def open_session(model_path: Path, voice: str, language: str = "fr"):
    """Ouvre une session CrispASR de synthèse et choisit la voix. Renvoie (session, moteur)."""
    import crispasr

    name = model_path.name.lower()
    backend = "pocket-tts-fr" if "pocket" in name and "french" in name else None
    session = crispasr.Session(str(model_path), backend=backend, use_gpu=True, n_threads=8)
    engine = session.backend
    if voice:
        tried = []
        for fn in ("set_speaker_name", "set_voice"):
            try:
                getattr(session, fn)(voice)
                break
            except Exception as e:  # selon le moteur, la voix se choisit par nom ou par fichier
                tried.append(f"{fn}: {e}")
        else:
            raise RuntimeError(f"voix {voice!r} refusée par {engine} ({'; '.join(tried)})")
    return session, engine


class CrispTTSService(TTSService):
    """Voix CrispASR servie par un processus séparé (voir tts_worker : conflit de ggml.dll avec l'écoute)."""

    def __init__(self, *, model_path: Path, voice: str, on_latency=None, **kwargs):
        super().__init__(
            push_start_frame=True,
            push_stop_frames=True,
            settings=TTSSettings(model=model_path.name, voice=voice, language=Language.FR),
            **kwargs,
        )
        import subprocess
        import sys

        t0 = time.perf_counter()
        flags = 0x08000000 if sys.platform == "win32" else 0  # pas de fenêtre de console
        self._proc = subprocess.Popen(
            [sys.executable, "-X", "utf8", "-m", "orchestra_voix.tts_worker", str(model_path), voice],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, creationflags=flags,
        )
        ready = self._proc.stdout.readline().decode("utf-8", "replace").split()
        if not ready or ready[0] != "READY":
            raise RuntimeError(f"la voix {model_path.name} n'a pas démarré")
        self._rate = int(ready[1])
        self._engine = ready[2] if len(ready) > 2 else model_path.stem
        self._lock = threading.Lock()
        self._resampler = create_stream_resampler()
        self._on_latency = on_latency
        logger.info(f"{self._engine} chargé et préchauffé en {time.perf_counter() - t0:.1f} s ({model_path.name}, voix {voice}, processus séparé)")

    def can_generate_metrics(self) -> bool:
        return True

    def warmup(self):
        pass  # fait par le processus de synthèse au démarrage

    def set_speaker(self, voice: str):
        self._next_voice = voice

    def _request(self, text: str, emit):
        """Envoie une demande et lit les morceaux jusqu'à la fin (toujours, pour rester synchronisé)."""
        import json
        import struct

        req = {"text": text}
        voice = getattr(self, "_next_voice", None)
        if voice:
            req["voice"] = voice
            self._next_voice = None
        self._proc.stdin.write((json.dumps(req, ensure_ascii=False) + "\n").encode("utf-8"))
        self._proc.stdin.flush()
        rd = self._proc.stdout.read
        while True:
            head = rd(4)
            if len(head) < 4:
                raise RuntimeError("processus de synthèse arrêté")
            (n,) = struct.unpack("<i", head)
            if n == 0:
                return
            if n < 0:
                (ln,) = struct.unpack("<i", rd(4))
                raise RuntimeError(rd(ln).decode("utf-8", "replace"))
            emit(np.frombuffer(rd(n * 4), dtype=np.float32))

    async def run_tts(self, text: str, context_id: str) -> AsyncGenerator[Frame, None]:
        loop = asyncio.get_running_loop()
        q: asyncio.Queue = asyncio.Queue()
        stop = threading.Event()

        def worker():
            try:
                with self._lock:
                    self._request(text, lambda pcm: None if stop.is_set() else loop.call_soon_threadsafe(q.put_nowait, pcm))
            except Exception as e:
                loop.call_soon_threadsafe(q.put_nowait, e)
            finally:
                loop.call_soon_threadsafe(q.put_nowait, None)

        try:
            await self.start_tts_usage_metrics(text)
            t0 = time.perf_counter()
            threading.Thread(target=worker, daemon=True, name="crisp-tts").start()
            first = True
            step = int(self.sample_rate * CHUNK_MS / 1000) * 2
            while (item := await q.get()) is not None:
                if isinstance(item, Exception):
                    yield ErrorFrame(error=f"{self._engine} : {item}")
                    break
                if first:
                    await self.stop_ttfb_metrics()
                    if self._on_latency:
                        self._on_latency("tts_first_audio", time.perf_counter() - t0)
                    first = False
                pcm = (np.clip(item, -1, 1) * 32767).astype(np.int16).tobytes()
                pcm = await self._resampler.resample(pcm, self._rate, self.sample_rate)
                for i in range(0, len(pcm), step):
                    yield TTSAudioRawFrame(audio=pcm[i : i + step], sample_rate=self.sample_rate, num_channels=1, context_id=context_id)
        finally:
            stop.set()  # interruption : les morceaux restants sont lus puis ignorés
            await self.stop_ttfb_metrics()

    async def cleanup(self):
        await super().cleanup()
        try:
            self._proc.kill()
        except Exception:
            pass
