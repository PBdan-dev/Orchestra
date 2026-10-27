"""Connecteur Pipecat pour Qwen3-TTS (Apache 2.0), en local sur le GPU, en flux.

Pipecat ne fournit pas ce connecteur. Le paquet officiel `qwen-tts` ne sait pas générer en
flux et reste plus lent que la parole sur une RTX 3090 (mesuré : 6,8 s avant le premier son).
On passe donc par `faster-qwen3-tts` (MIT), qui capture le décodage dans des graphes CUDA et
rend l'audio morceau par morceau : le premier son sort après le premier morceau.

Deux modes :
- voix préréglée (modèle CustomVoice, ex. « Ryan ») ;
- voix clonée à partir d'un court enregistrement (modèle Base), uniquement avec le
  consentement explicite de la personne enregistrée.
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

CHUNK_MS = 40


class Qwen3TTSService(TTSService):
    def __init__(
        self,
        *,
        model_path: Path,
        speaker: str = "Ryan",
        reference_audio: Path | None = None,
        reference_text: str = "",
        language: str = "French",
        chunk_steps: int = 4,  # 4 pas à 12 Hz ≈ 333 ms d'audio par morceau
        on_latency=None,
        **kwargs,
    ):
        super().__init__(
            push_start_frame=True,
            push_stop_frames=True,
            settings=TTSSettings(model=model_path.name, voice=speaker, language=Language.FR),
            **kwargs,
        )
        from faster_qwen3_tts import FasterQwen3TTS

        t0 = time.perf_counter()
        self._model = FasterQwen3TTS.from_pretrained(str(model_path), local_files_only=True)
        self._speaker = speaker
        self._language = language
        self._ref = str(reference_audio) if reference_audio else None
        self._ref_text = reference_text
        self._chunk = chunk_steps
        self._lock = threading.Lock()  # une synthèse à la fois sur le GPU
        self._resampler = create_stream_resampler()
        self._on_latency = on_latency
        logger.info(f"Qwen3-TTS chargé en {time.perf_counter() - t0:.1f} s ({model_path.name})")

    def can_generate_metrics(self) -> bool:
        return True

    def _stream(self, text: str):
        if self._ref:
            return self._model.generate_voice_clone_streaming(
                text=text, language=self._language, ref_audio=self._ref, ref_text=self._ref_text,
                chunk_size=self._chunk,
            )
        return self._model.generate_custom_voice_streaming(
            text=text, speaker=self._speaker, language=self._language, chunk_size=self._chunk,
        )

    def warmup(self):
        """Première synthèse à vide : capture les graphes CUDA avant la première réponse."""
        t0 = time.perf_counter()
        for _ in self._stream("Bonjour, je suis prête."):
            pass
        logger.info(f"Qwen3-TTS préchauffé en {time.perf_counter() - t0:.1f} s")

    async def run_tts(self, text: str, context_id: str) -> AsyncGenerator[Frame, None]:
        loop = asyncio.get_running_loop()
        q: asyncio.Queue = asyncio.Queue()
        stop = threading.Event()

        def worker():
            try:
                with self._lock:
                    for audio, sr, _timing in self._stream(text):
                        if stop.is_set():
                            break
                        loop.call_soon_threadsafe(q.put_nowait, (np.asarray(audio, dtype=np.float32).reshape(-1), int(sr)))
            except Exception as e:  # remonté au pipeline sous forme d'ErrorFrame
                loop.call_soon_threadsafe(q.put_nowait, e)
            finally:
                loop.call_soon_threadsafe(q.put_nowait, None)

        try:
            await self.start_tts_usage_metrics(text)
            t0 = time.perf_counter()
            threading.Thread(target=worker, daemon=True, name="qwen3-tts").start()
            first = True
            step = int(self.sample_rate * CHUNK_MS / 1000) * 2
            while (item := await q.get()) is not None:
                if isinstance(item, Exception):
                    yield ErrorFrame(error=f"Qwen3-TTS : {item}")
                    break
                samples, sr = item
                if first:
                    await self.stop_ttfb_metrics()
                    if self._on_latency:
                        self._on_latency("tts_first_audio", time.perf_counter() - t0)
                    first = False
                pcm = (np.clip(samples, -1, 1) * 32767).astype(np.int16).tobytes()
                pcm = await self._resampler.resample(pcm, sr, self.sample_rate)
                for i in range(0, len(pcm), step):
                    yield TTSAudioRawFrame(audio=pcm[i : i + step], sample_rate=self.sample_rate, num_channels=1, context_id=context_id)
        finally:
            stop.set()  # interruption : on arrête la génération au prochain morceau
            await self.stop_ttfb_metrics()
