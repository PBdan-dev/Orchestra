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
    def __init__(self, *, model_path: Path, voice: str, on_latency=None, **kwargs):
        super().__init__(
            push_start_frame=True,
            push_stop_frames=True,
            settings=TTSSettings(model=model_path.name, voice=voice, language=Language.FR),
            **kwargs,
        )
        t0 = time.perf_counter()
        self._session, self._engine = open_session(model_path, voice)
        self._rate = int(self._session.output_sample_rate() or 24000)
        self._lock = threading.Lock()
        self._resampler = create_stream_resampler()
        self._on_latency = on_latency
        logger.info(f"{self._engine} chargé en {time.perf_counter() - t0:.1f} s ({model_path.name}, voix {voice})")

    def can_generate_metrics(self) -> bool:
        return True

    def warmup(self):
        t0 = time.perf_counter()
        self._session.synthesize("Bonjour.")
        logger.info(f"{self._engine} préchauffé en {time.perf_counter() - t0:.1f} s")

    def set_speaker(self, voice: str):
        with self._lock:
            for fn in ("set_speaker_name", "set_voice"):
                try:
                    getattr(self._session, fn)(voice)
                    return
                except Exception:
                    continue

    async def run_tts(self, text: str, context_id: str) -> AsyncGenerator[Frame, None]:
        loop = asyncio.get_running_loop()
        q: asyncio.Queue = asyncio.Queue()
        stop = threading.Event()

        def worker():
            try:
                with self._lock:
                    # découpe courte : la première phrase sort vite
                    for piece in split_for_speech(text):
                        if stop.is_set():
                            break
                        pcm = self._session.synthesize(piece)
                        loop.call_soon_threadsafe(q.put_nowait, np.asarray(pcm, dtype=np.float32).reshape(-1))
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
            stop.set()
            await self.stop_ttfb_metrics()
