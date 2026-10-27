"""Serveur voix d'Orchestra : une boucle Pipecat locale, en flux continu.

    micro (interface) ─WebSocket→ VAD Silero → Voxtral Realtime → contexte → Qwen 3.5 9B (Ollama)
                                                   → Qwen3-TTS ─WebSocket→ haut-parleur

Le cerveau du mode vocal partage la mémoire d'Orchestra (memoire/orchestra.md) et
l'historique du chat. Une demande de travail (fichier, agent…) est confiée à la boucle
centrale d'Orchestra par l'outil `confier_a_orchestra` : l'interface la lance comme un
message de chat, et le résultat est relu à voix haute.

Lancement : python -m orchestra_voix.server [--port 8765]
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
import time
from datetime import datetime

from loguru import logger

from pipecat.adapters.llm.schemas.function_schema import FunctionSchema
from pipecat.adapters.llm.schemas.tools_schema import ToolsSchema
from pipecat.audio.vad.silero import SileroVADAnalyzer
from pipecat.audio.vad.vad_analyzer import VADParams
from pipecat.frames.frames import (
    Frame,
    InputTransportMessageFrame,
    InterimTranscriptionFrame,
    LLMFullResponseEndFrame,
    LLMFullResponseStartFrame,
    LLMTextFrame,
    OutputTransportMessageUrgentFrame,
    TranscriptionFrame,
    TTSAudioRawFrame,
    TTSSpeakFrame,
    VADUserStartedSpeakingFrame,
    VADUserStoppedSpeakingFrame,
)
from pipecat.observers.base_observer import BaseObserver, FramePushed
from pipecat.pipeline.pipeline import Pipeline
from pipecat.pipeline.worker import PipelineParams, PipelineWorker
from pipecat.workers.runner import WorkerRunner
from pipecat.processors.aggregators.llm_context import LLMContext
from pipecat.processors.aggregators.llm_response_universal import LLMContextAggregatorPair
from pipecat.processors.audio.vad_processor import VADProcessor
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor
from pipecat.services.llm_service import FunctionCallParams
from pipecat.services.ollama.llm import OLLamaLLMService
from pipecat.transcriptions.language import Language
from pipecat.transports.websocket.server import (
    SingleClientWebsocketServerParams,
    SingleClientWebsocketServerTransport,
)

from . import config as C
from .protocol import OrchestraSerializer

OUT_RATE = 24000


def system_prompt() -> str:
    memoire = ""
    try:
        memoire = C.MEMOIRE.read_text(encoding="utf-8")[-6000:]
    except OSError:
        pass
    return (
        "Tu es Orchestra, l'assistant local de l'utilisateur, en conversation vocale. "
        "Réponds en français, à l'oral : phrases courtes et naturelles, une à trois en général. "
        "Jamais de Markdown, de listes, d'emoji ni de code : tout ce que tu écris est lu à voix haute. "
        "Écris les nombres et les sigles comme on les prononce. "
        "Quand on te demande un travail concret (écrire ou lire un fichier, rédiger un document, "
        "utiliser un agent, chercher quelque chose), appelle l'outil confier_a_orchestra avec la tâche "
        "formulée clairement, puis dis en une phrase que tu t'en occupes.\n\n"
        f"Ce que tu sais de l'utilisateur (ta mémoire) :\n{memoire}"
    )


class LatencyLog:
    """Chronomètre d'un tour de parole, de la fin de parole au premier son de la réponse."""

    def __init__(self, vad_stop_secs: float):
        self.vad_stop = vad_stop_secs
        self.reset()
        C.MESURES.mkdir(parents=True, exist_ok=True)

    def reset(self):
        self.t_stop = self.t_text = self.t_llm = self.t_audio = None
        self.extra: dict[str, float] = {}

    def hook(self, name: str, seconds: float):
        self.extra[name] = round(seconds * 1000)

    def report(self, tts: str, llm: str) -> dict | None:
        if not (self.t_stop and self.t_audio):
            return None
        ms = lambda a, b: round((b - a) * 1000) if a and b else None  # noqa: E731
        r = {
            "type": "latency",
            "at": datetime.now().isoformat(timespec="seconds"),
            "tts": tts,
            "llm": llm,
            "vad_ms": round(self.vad_stop * 1000),
            "stt_ms": ms(self.t_stop, self.t_text),
            "llm_ms": ms(self.t_text, self.t_llm),
            "tts_ms": ms(self.t_llm, self.t_audio),
            # du silence détecté au premier son, + le temps d'attente du VAD
            "total_ms": ms(self.t_stop, self.t_audio) + round(self.vad_stop * 1000),
            **self.extra,
        }
        with open(C.MESURES / f"latence-{datetime.now():%Y-%m-%d}.jsonl", "a", encoding="utf-8") as f:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
        return r


class UIBridge(BaseObserver):
    """Observe le pipeline et envoie à l'interface ce qui s'y passe (texte, état, mesures)."""

    def __init__(self, lat: LatencyLog, tts: str, llm: str):
        super().__init__(observe_every_push=False)
        self.task: PipelineWorker | None = None
        self.lat, self.tts, self.llm = lat, tts, llm
        self._in_bot = False

    async def send(self, msg: dict):
        if self.task:
            await self.task.queue_frame(OutputTransportMessageUrgentFrame(message=msg))

    async def on_push_frame(self, data: FramePushed):
        f = data.frame
        now = time.perf_counter()
        if isinstance(f, VADUserStartedSpeakingFrame):
            self.lat.reset()
            await self.send({"type": "user_start"})
        elif isinstance(f, VADUserStoppedSpeakingFrame):
            self.lat.t_stop = now
            await self.send({"type": "user_stop"})
        elif isinstance(f, InterimTranscriptionFrame):
            await self.send({"type": "user_interim", "text": f.text})
        elif isinstance(f, TranscriptionFrame):
            self.lat.t_text = self.lat.t_text or now
            await self.send({"type": "user_final", "text": f.text})
        elif isinstance(f, LLMFullResponseStartFrame):
            self._in_bot = True
            await self.send({"type": "bot_start"})
        elif isinstance(f, LLMTextFrame) and self._in_bot:
            self.lat.t_llm = self.lat.t_llm or now
            await self.send({"type": "bot_text", "text": f.text})
        elif isinstance(f, LLMFullResponseEndFrame) and self._in_bot:
            self._in_bot = False
            await self.send({"type": "bot_end"})
        elif isinstance(f, TTSAudioRawFrame) and self.lat.t_stop and not self.lat.t_audio:
            self.lat.t_audio = now
            r = self.lat.report(self.tts, self.llm)
            if r:
                logger.info(f"Latence : {r}")
                await self.send(r)


class ClientCommands(FrameProcessor):
    """Commandes envoyées par l'interface : contexte du chat, lecture d'un texte, arrêt."""

    def __init__(self, context: LLMContext, **kw):
        super().__init__(**kw)
        self._ctx = context

    async def process_frame(self, frame: Frame, direction: FrameDirection):
        await super().process_frame(frame, direction)
        if isinstance(frame, InputTransportMessageFrame) and isinstance(frame.message, dict):
            m = frame.message
            kind = m.get("type")
            if kind == "context":
                hist = [
                    {"role": x["role"], "content": str(x["content"])[:2000]}
                    for x in m.get("messages", [])[-16:]
                    if x.get("role") in ("user", "assistant") and x.get("content")
                ]
                self._ctx.set_messages([{"role": "system", "content": system_prompt()}, *hist])
                return
            if kind == "say" and m.get("text"):
                await self.push_frame(TTSSpeakFrame(text=str(m["text"])))
                return
            if kind == "stop":
                await self.broadcast_interruption()
                return
        await self.push_frame(frame, direction)


def build_tts(s: C.VoiceSettings, lat: LatencyLog):
    from .tts_qwen3 import Qwen3TTSService

    ref = (C.REFERENCES / s.qwen3_reference) if s.qwen3_reference else None
    tts = Qwen3TTSService(
        model_path=C.QWEN3_TTS_BASE if ref else C.QWEN3_TTS,
        speaker=s.qwen3_speaker,
        reference_audio=ref,
        reference_text=s.qwen3_reference_text,
        on_latency=lat.hook,
    )
    tts.warmup()
    return tts


async def main(s: C.VoiceSettings):
    C.VOIX.mkdir(parents=True, exist_ok=True)
    logger.remove()
    logger.add(sys.stderr, level="INFO")
    logger.add(C.VOIX / "voix.log", level="DEBUG", rotation="5 MB", retention=3, encoding="utf-8")

    lat = LatencyLog(s.vad_stop_secs)
    bridge = UIBridge(lat, s.tts, s.llm_model)

    from .stt_voxtral import VoxtralRealtimeSTTService

    stt = VoxtralRealtimeSTTService(
        model_path=C.VOXTRAL_GGUF, library=C.transcribe_library(), delay_ms=s.voxtral_delay_ms, on_latency=lat.hook
    )
    tts = build_tts(s, lat)
    llm = OLLamaLLMService(
        base_url=s.ollama_url,
        settings=OLLamaLLMService.Settings(
            model=s.llm_model,
            temperature=0.6,
            max_tokens=400,
            # pas de « réflexion » en mode vocal : on veut le premier mot tout de suite
            extra={"reasoning_effort": "none"},
        ),
    )

    async def confier(params: FunctionCallParams):
        tache = str(params.arguments.get("tache", "")).strip()
        await bridge.send({"type": "delegate", "text": tache})
        await params.result_callback(
            {"resultat": "Tâche confiée à Orchestra, qui annoncera le résultat. Dis-le en une phrase courte."}
        )

    llm.register_function("confier_a_orchestra", confier)
    tools = ToolsSchema(
        standard_tools=[
            FunctionSchema(
                name="confier_a_orchestra",
                description="Confie un travail concret à la boucle centrale d'Orchestra, qui choisit et supervise les agents.",
                properties={"tache": {"type": "string", "description": "La tâche, formulée clairement et complètement."}},
                required=["tache"],
            )
        ]
    )
    context = LLMContext(messages=[{"role": "system", "content": system_prompt()}], tools=tools)
    agg = LLMContextAggregatorPair(context)

    transport = SingleClientWebsocketServerTransport(
        params=SingleClientWebsocketServerParams(
            audio_in_enabled=True,
            audio_in_sample_rate=16000,
            audio_out_enabled=True,
            audio_out_sample_rate=OUT_RATE,
            serializer=OrchestraSerializer(),
        ),
        host=s.host,
        port=s.port,
    )

    pipeline = Pipeline(
        [
            transport.input(),
            ClientCommands(context),
            VADProcessor(vad_analyzer=SileroVADAnalyzer(params=VADParams(stop_secs=s.vad_stop_secs))),
            stt,
            agg.user(),
            llm,
            tts,
            transport.output(),
            agg.assistant(),
        ]
    )
    task = PipelineWorker(
        pipeline,
        params=PipelineParams(
            enable_metrics=True, audio_in_sample_rate=16000, audio_out_sample_rate=OUT_RATE
        ),
        observers=[bridge],
        idle_timeout_secs=None,
    )
    bridge.task = task

    @transport.event_handler("on_client_connected")
    async def _connected(_transport, _ws):
        logger.info("Interface connectée")
        context.set_messages([{"role": "system", "content": system_prompt()}])
        await bridge.send({"type": "ready", "stt": "voxtral-mini-4b-realtime", "llm": s.llm_model, "tts": s.tts})

    logger.info(f"Serveur voix prêt sur ws://{s.host}:{s.port} (TTS : {s.tts}, cerveau : {s.llm_model})")
    await WorkerRunner(handle_sigint=False).run(task)


def cli():
    p = argparse.ArgumentParser(description="Serveur voix d'Orchestra")
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--llm", default="qwen3.5:9b")
    p.add_argument("--speaker", default="Ryan")
    p.add_argument("--reference", default="", help="fichier de voix/references/ à cloner (avec consentement)")
    p.add_argument("--reference-text", default="")
    p.add_argument("--vad-stop", type=float, default=0.35)
    a = p.parse_args()
    s = C.VoiceSettings(
        port=a.port, llm_model=a.llm, qwen3_speaker=a.speaker,
        qwen3_reference=a.reference, qwen3_reference_text=a.reference_text, vad_stop_secs=a.vad_stop,
    )
    asyncio.run(main(s))


if __name__ == "__main__":
    cli()
