"""Connecteur Pipecat pour Voxtral Mini 4B Realtime (Apache 2.0), via transcribe.cpp (MIT).

Voxtral Realtime est un modèle de transcription en flux : on lui donne l'audio au fil de
l'eau pendant que la personne parle, si bien qu'à la fin de la phrase il ne reste presque
rien à calculer. Pipecat ne fournit pas ce connecteur, d'où ce fichier.

Fonctionnement :
- au début de la parole (VAD), on ouvre un flux et on y verse le pré-roulement (≈0,4 s) ;
- pendant la parole, chaque morceau d'audio est versé dans le flux, et le texte provisoire
  est remonté à l'interface (InterimTranscriptionFrame) ;
- à la fin de la parole, on clôt le flux et on pousse la transcription finale.

Tous les appels à transcribe.cpp passent par un seul fil d'exécution dédié, dans l'ordre.
"""

from __future__ import annotations

import asyncio
import collections
import os
import time
from collections.abc import AsyncGenerator
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
from loguru import logger

from pipecat.frames.frames import (
    ErrorFrame,
    Frame,
    InterimTranscriptionFrame,
    TranscriptionFrame,
    VADUserStartedSpeakingFrame,
    VADUserStoppedSpeakingFrame,
)
from pipecat.processors.frame_processor import FrameDirection
from pipecat.services.settings import STTSettings
from pipecat.services.stt_service import STTService
from pipecat.transcriptions.language import Language
from pipecat.utils.time import time_now_iso8601


class VoxtralRealtimeSTTService(STTService):
    def __init__(
        self,
        *,
        model_path: Path,
        library: Path | None = None,
        delay_ms: int = 480,
        interim_every_s: float = 0.3,
        on_latency=None,
        **kwargs,
    ):
        super().__init__(
            settings=STTSettings(model="voxtral-mini-4b-realtime-2602", language=Language.FR),
            **kwargs,
        )
        if library:
            os.environ["TRANSCRIBE_LIBRARY"] = str(library)
            # les DLL de CUDA et de ggml sont à côté de transcribe.dll
            if hasattr(os, "add_dll_directory"):
                os.add_dll_directory(str(Path(library).parent))
        import transcribe_cpp  # noqa: E402  (après TRANSCRIBE_LIBRARY)

        self._tc = transcribe_cpp
        self._pool = ThreadPoolExecutor(max_workers=1, thread_name_prefix="voxtral")
        self._delay_ms = delay_ms
        self._interim_every = interim_every_s
        self._on_latency = on_latency
        t0 = time.perf_counter()
        self._model = transcribe_cpp.Model(str(model_path))
        self._session = self._model.session()
        logger.info(f"Voxtral chargé en {time.perf_counter() - t0:.1f} s ({model_path.name})")

        self._preroll: collections.deque[bytes] = collections.deque()
        self._preroll_bytes = 0
        self._stream_ctx = None
        self._stream = None
        self._speaking = False
        self._last_interim = 0.0
        self._last_text = ""

    def can_generate_metrics(self) -> bool:
        return True

    # ---- appels natifs (toujours dans le fil dédié) ---------------------------------

    def _open(self, preroll: bytes):
        # délai de Voxtral en « jetons » de 80 ms (6 = 480 ms, le réglage par défaut)
        opts = self._tc.VoxtralRealtimeStreamOptions(num_delay_tokens=max(1, round(self._delay_ms / 80)))
        ctx = self._session.stream(family=opts)
        self._stream_ctx = ctx
        self._stream = ctx.__enter__()
        if preroll:
            self._stream.feed(_pcm16_to_f32(preroll))

    def _feed(self, pcm: bytes, want_text: bool) -> str | None:
        if not self._stream:
            return None
        self._stream.feed(_pcm16_to_f32(pcm))
        if want_text:
            return self._stream.text().display.strip()
        return None

    def _close(self) -> str:
        if not self._stream:
            return ""
        try:
            # un peu de silence pour que le dernier mot soit bien émis
            self._stream.feed(np.zeros(int(16000 * 0.3), dtype=np.float32))
            self._stream.finalize()
            snap = self._stream.snapshot()
            text = snap.text or self._stream.text().display
        finally:
            self._stream_ctx.__exit__(None, None, None)
            self._stream = None
            self._stream_ctx = None
        return (text or "").strip()

    async def _call(self, fn, *args):
        return await asyncio.get_running_loop().run_in_executor(self._pool, fn, *args)

    # ---- Pipecat -------------------------------------------------------------------

    async def run_stt(self, audio: bytes) -> AsyncGenerator[Frame | None, None]:
        if not self._speaking:
            # on garde ~0,4 s d'audio : le VAD confirme la parole avec un léger retard
            self._preroll.append(audio)
            self._preroll_bytes += len(audio)
            limit = int(self.sample_rate * 2 * 0.4)
            while self._preroll_bytes > limit and self._preroll:
                self._preroll_bytes -= len(self._preroll.popleft())
            return
        now = time.monotonic()
        want = now - self._last_interim >= self._interim_every
        text = await self._call(self._feed, audio, want)
        if want:
            self._last_interim = now
        if text and text != self._last_text:
            self._last_text = text
            yield InterimTranscriptionFrame(text, self._user_id, time_now_iso8601(), Language.FR)

    async def process_frame(self, frame: Frame, direction: FrameDirection):
        await super().process_frame(frame, direction)
        if isinstance(frame, VADUserStartedSpeakingFrame) and not self._speaking:
            preroll = b"".join(self._preroll)
            self._preroll.clear()
            self._preroll_bytes = 0
            self._last_text = ""
            self._last_interim = time.monotonic()
            await self._call(self._open, preroll)
            self._speaking = True
        elif isinstance(frame, VADUserStoppedSpeakingFrame) and self._speaking:
            self._speaking = False
            t0 = time.perf_counter()
            await self.start_processing_metrics()
            try:
                text = await self._call(self._close)
            except Exception as e:  # le pipeline continue même si une phrase échoue
                await self.push_frame(ErrorFrame(error=f"Voxtral : {e}"))
                return
            await self.stop_processing_metrics()
            dt = time.perf_counter() - t0
            if self._on_latency:
                self._on_latency("stt_final", dt)
            logger.debug(f"Voxtral ({dt * 1000:.0f} ms après la fin de parole) : {text!r}")
            if text:
                await self.push_frame(
                    TranscriptionFrame(text, self._user_id, time_now_iso8601(), Language.FR)
                )

    async def cleanup(self):
        await super().cleanup()
        self._pool.shutdown(wait=False, cancel_futures=True)


def _pcm16_to_f32(pcm: bytes) -> np.ndarray:
    return np.frombuffer(pcm, dtype=np.int16).astype(np.float32) / 32768.0


def _text_of(t) -> str:
    """stream.text() rend un objet avec .committed et .tentative, ou une chaîne."""
    if t is None:
        return ""
    if isinstance(t, str):
        return t.strip()
    committed = getattr(t, "committed", "") or ""
    tentative = getattr(t, "tentative", "") or ""
    return f"{committed}{tentative}".strip()
