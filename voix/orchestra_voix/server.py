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
import os
import sys
import time
from datetime import datetime

import numpy as np
from loguru import logger

from pipecat.adapters.llm.schemas.function_schema import FunctionSchema
from pipecat.adapters.llm.schemas.tools_schema import ToolsSchema
from pipecat.audio.vad.silero import SileroVADAnalyzer
from pipecat.audio.vad.vad_analyzer import VADParams
from pipecat.frames.frames import (
    Frame,
    FunctionCallResultProperties,
    InputAudioRawFrame,
    InputTransportMessageFrame,
    LLMContextFrame,
    LLMMessagesAppendFrame,
    LLMUpdateSettingsFrame,
    InterimTranscriptionFrame,
    LLMFullResponseEndFrame,
    LLMFullResponseStartFrame,
    LLMTextFrame,
    OutputTransportMessageUrgentFrame,
    TranscriptionFrame,
    TTSAudioRawFrame,
    TTSSpeakFrame,
    TTSTextFrame,
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


JOURS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"]
MOIS = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"]


def maintenant() -> str:
    """Date et heure en toutes lettres : sans elles, le modèle invente la date."""
    d = datetime.now()
    return f"{JOURS[d.weekday()]} {d.day} {MOIS[d.month - 1]} {d.year}, il est {d.hour} h {d.minute:02d}"


# Bloc 1 du prompt vocal (lot A) : fixe, sans date ni mémoire, pour que son calcul reste en cache.
BLOC_1 = (
    "Tu es Orchestra, l'assistant local de l'utilisateur, en conversation vocale. "
    "Réponds en français, à l'oral : phrases courtes et naturelles, une à trois en général ; va à l'essentiel. "
    "Jamais de Markdown, de listes, d'emoji ni de code : tout ce que tu écris est lu à voix haute. "
    "Écris les nombres et les sigles comme on les prononce. "
    "Chaque message de l'utilisateur commence par la date et l'heure entre crochets, puis les souvenirs utiles pour cette question : "
    "sers-t'en quand ils sont pertinents, sans les réciter ni répéter la date. "
    "Quand on te demande un travail concret (écrire ou lire un fichier, rédiger un document, "
    "utiliser un agent, chercher quelque chose), appelle l'outil confier_a_orchestra avec la tâche "
    "formulée clairement. "
    "Si une question demande vraiment de réfléchir (calcul, raisonnement en plusieurs étapes, "
    "comparaison délicate, conseil important), appelle l'outil reflechir avec la question complète ; "
    "pour tout le reste, réponds directement sans réfléchir longuement. "
    "Tiens compte de toute la conversation qui précède : ce que l'utilisateur t'a déjà dit reste vrai, "
    "ne le contredis pas et ne lui redemande pas. "
    "N'invente jamais un fait sur l'utilisateur : si tu ne le sais pas, dis-le."
)

# Profil (bloc 2) : sections Utilisateur et Préférences d'orchestra.md, figé à la connexion de l'interface.
PROFIL_MAX_CAR = 1050  # ≈ 300 tokens
_profil: str | None = None


def lire_profil() -> str:
    try:
        texte = C.MEMOIRE.read_text(encoding="utf-8")
    except OSError:
        return ""
    section, faits = "", []
    for ligne in texte.splitlines():
        t = ligne.strip()
        if t.startswith("## "):
            section = t[3:].strip()
        elif t.startswith("- ") and section in ("Utilisateur", "Préférences"):
            faits.append(t)
    out, ecartes = [], 0
    for f in faits:
        if sum(len(x) + 1 for x in out) + len(f) > PROFIL_MAX_CAR:
            ecartes += 1
        else:
            out.append(f)
    if ecartes:
        # jamais de coupe silencieuse
        out.append(f"[… {ecartes} fait(s) du profil non repris ici, faute de place]")
        logger.info(f"Profil vocal : {ecartes} fait(s) non repris (budget de {PROFIL_MAX_CAR} caractères)")
    return "\n".join(out)


# Lot D (décision de PB du 31/10) : comme en chat, le profil (bloc 2) et le résumé de la conversation
# (bloc 3) viennent du cœur d'Orchestra par le message `context` ; les souvenirs utiles sont demandés
# à chaque phrase (D2). La mémoire n'est plus donnée en entier ni coupée à 6 000 caractères.
_resume: str = ""


def nouvelle_session():
    """Début de conversation : profil relu (en attendant celui que le cœur envoie avec `context`)."""
    global _profil, _resume
    _profil = lire_profil()
    _resume = ""


def system_prompt() -> str:
    consignes = ""
    try:
        # bloc 1b : consignes écrites par PB dans le panneau des modèles (voix/consignes.txt)
        consignes = (C.VOIX / "consignes.txt").read_text(encoding="utf-8").strip()
    except OSError:
        pass
    if len(consignes) > 3000:
        logger.info(f"Consignes vocales coupées à 3 000 caractères (sur {len(consignes)})")
        consignes = consignes[:3000] + " [… consignes coupées, trop longues]"
    profil = _profil if _profil is not None else lire_profil()
    texte = (
        BLOC_1
        + "\n\n"
        + (f"Consignes de l'utilisateur sur ta façon de répondre (à respecter en priorité) :\n{consignes}\n\n" if consignes else "")
        + "Ce que tu sais de l'utilisateur :\n"
        + (profil or "(rien pour l'instant)")
    )
    if _resume:
        texte += (
            "\n\nDébut de la conversation (résumé) : les premiers échanges ne sont plus affichés, voici leur résumé, "
            "à jour et fiable.\n<donnees>\n" + _resume + "\n</donnees>"
        )
    return texte


def modele_derive(base: str) -> str:
    """Même règle que `modele_vocal_derive` côté Rust : qwen3.5:9b → qwen3.5:9b-orchestra."""
    if not base or base.endswith("-orchestra"):
        return base
    return f"{base}-orchestra" if ":" in base else f"{base}:latest-orchestra"


async def modele_vocal(s: C.VoiceSettings, base: str) -> str:
    """Modèle dérivé dans Ollama (mêmes poids, num_ctx réduit), créé s'il manque.

    Le point d'accès /v1 utilisé par Pipecat ignore num_ctx (vérifié le 29/10) : sans ce modèle
    dérivé, Ollama prend son contexte par défaut (64k) et la mémoire GPU déborde presque.
    """
    import httpx

    nom = modele_derive(base)
    api = s.ollama_url.removesuffix("/v1")
    try:
        async with httpx.AsyncClient(timeout=120) as c:
            r = await c.post(f"{api}/api/show", json={"model": nom})
            if r.status_code == 200 and f"num_ctx {s.num_ctx}" in " ".join(str(r.json().get("parameters", "")).split()):
                return nom
            r = await c.post(f"{api}/api/create", json={"model": nom, "from": base, "parameters": {"num_ctx": s.num_ctx}, "stream": False})
            r.raise_for_status()
        logger.info(f"Modèle vocal dérivé créé : {nom} (num_ctx {s.num_ctx})")
        return nom
    except Exception as e:
        logger.warning(f"Modèle vocal dérivé impossible ({e}) : j'utilise {base} avec son contexte par défaut")
        return base


RAPPEL_DELAI_S = 0.05  # D2 : au-delà de 50 ms, réponse sans souvenirs
_client_rappel = None  # un seul client, créé une fois (créer un client httpx coûte ≈ 200 ms)


async def souvenirs(texte: str) -> str:
    """Souvenirs utiles pour la phrase, demandés au cœur d'Orchestra (même rappel qu'en chat).
    50 ms au plus en tout ; au-delà ou en cas d'erreur, réponse sans souvenirs, noté au journal."""
    global _client_rappel
    url, jeton = os.environ.get("ORCHESTRA_RAPPEL_URL"), os.environ.get("ORCHESTRA_RAPPEL_JETON")
    if not url or not jeton:
        return "Aucun souvenir pertinent."
    import httpx

    if _client_rappel is None:
        _client_rappel = httpx.AsyncClient(headers={"Authorization": f"Bearer {jeton}"}, trust_env=False)
    t0 = time.perf_counter()
    try:
        r = await asyncio.wait_for(_client_rappel.post(f"{url}/rappel", json={"texte": texte}), RAPPEL_DELAI_S)
        r.raise_for_status()
        logger.debug(f"Souvenirs obtenus en {(time.perf_counter() - t0) * 1000:.0f} ms")
        return str(r.json().get("souvenirs") or "Aucun souvenir pertinent.")
    except Exception as e:
        logger.info(f"Souvenirs non obtenus en {(time.perf_counter() - t0) * 1000:.0f} ms ({type(e).__name__}) : réponse sans souvenirs")
        return "Aucun souvenir pertinent."


async def prechauffer_rappel():
    """Crée le client et ouvre la connexion avant la première phrase (sinon ≈ 300 ms la première fois)."""
    global RAPPEL_DELAI_S
    delai, RAPPEL_DELAI_S = RAPPEL_DELAI_S, 2.0
    try:
        await souvenirs("préchauffage")
    finally:
        RAPPEL_DELAI_S = delai


def sans_reflexion(msgs: list) -> list:
    """D5 : l'appel à `reflechir` et son résultat sortent de la conversation ; seule reste la réponse
    dite. Pipecat garde déjà le texte prononcé (« Attends, je réfléchis. » puis la réponse) comme
    message d'Orchestra : la réponse n'est ajoutée que si elle n'y figure pas."""
    ids = set()
    out = []
    for i, m in enumerate(msgs):
        if not isinstance(m, dict):
            out.append(m)
            continue
        appels = m.get("tool_calls") or []
        noms = [((a.get("function") or {}).get("name") if isinstance(a, dict) else None) for a in appels]
        if appels and all(n == "reflechir" for n in noms):
            ids.update(a.get("id") for a in appels if isinstance(a, dict))
            if m.get("content"):
                out.append({"role": "assistant", "content": m["content"]})
            continue
        if m.get("role") == "tool" and m.get("tool_call_id") in ids:
            try:
                dite = json.loads(m.get("content") or "{}").get("reponse_dite")
            except (ValueError, AttributeError):
                dite = None
            deja_dite = dite and any(
                isinstance(x, dict) and x.get("role") == "assistant" and dite[:40] in str(x.get("content") or "")
                for x in msgs[i + 1 :]
            )
            if dite and not deja_dite:
                out.append({"role": "assistant", "content": dite})
            continue
        out.append(m)
    return out


class DateDuTour(FrameProcessor):
    """Bloc 5, comme en chat : la date, l'heure et les souvenirs utiles en tête du dernier message de
    l'utilisateur, juste avant le cerveau. Le prompt système ne change donc pas (cache).
    Avant chaque tour, les traces de `reflechir` sont retirées de la conversation (D5).
    """

    async def process_frame(self, frame: Frame, direction: FrameDirection):
        await super().process_frame(frame, direction)
        if isinstance(frame, LLMContextFrame) and direction == FrameDirection.DOWNSTREAM:
            msgs = sans_reflexion(frame.context.get_messages())
            for m in reversed(msgs):
                if not isinstance(m, dict) or m.get("role") == "system":
                    continue
                c = m.get("content")
                if m.get("role") == "user" and isinstance(c, str) and not c.startswith("["):
                    s = await souvenirs(c)
                    m["content"] = f"[{maintenant()}]\n[Souvenirs pour cette question :\n{s}]\n\n{c}"
                break
            frame.context.set_messages(msgs)
        await self.push_frame(frame, direction)


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
        # certaines trames sont diffusées dans les deux sens : on ne compte que la descendante
        if data.direction != FrameDirection.DOWNSTREAM:
            return
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


class TexteDit(FrameProcessor):
    """D3 : le texte réellement prononcé. La sortie audio ne laisse passer chaque phrase qu'au moment
    où son audio part ; placé juste après elle, ce processeur l'envoie à l'interface (`bot_dit`).
    À une coupure de parole, l'interface (qui tient la conversation, D1) ne garde que ce texte,
    suivi de « [interrompu] »."""

    def __init__(self, bridge: "UIBridge", **kw):
        super().__init__(**kw)
        self._bridge = bridge

    async def process_frame(self, frame: Frame, direction: FrameDirection):
        await super().process_frame(frame, direction)
        if isinstance(frame, TTSTextFrame) and direction == FrameDirection.DOWNSTREAM and frame.text.strip():
            await self._bridge.send({"type": "bot_dit", "text": frame.text})
        await self.push_frame(frame, direction)


class CompteurMicro(FrameProcessor):
    """Diagnostic : toutes les 10 s, combien de morceaux de micro sont arrivés, et combien avec du son."""

    def __init__(self, **kw):
        super().__init__(**kw)
        self._n = self._voiced = 0
        self._t = time.monotonic()

    async def process_frame(self, frame: Frame, direction: FrameDirection):
        await super().process_frame(frame, direction)
        if isinstance(frame, InputAudioRawFrame):
            self._n += 1
            if frame.audio and np.abs(np.frombuffer(frame.audio, dtype=np.int16)).max() > 500:
                self._voiced += 1
            if time.monotonic() - self._t > 10:
                logger.debug(f"Micro reçu : {self._n} morceaux en 10 s, dont {self._voiced} avec du son")
                self._n = self._voiced = 0
                self._t = time.monotonic()
        await self.push_frame(frame, direction)


class ClientCommands(FrameProcessor):
    """Commandes envoyées par l'interface : contexte du chat, lecture d'un texte, arrêt."""

    def __init__(self, context: LLMContext, settings: C.VoiceSettings, tts=None, **kw):
        super().__init__(**kw)
        self._ctx = context
        self._s = settings
        self._tts = tts
        self._n = self._voiced = 0
        self._t = time.monotonic()

    async def process_frame(self, frame: Frame, direction: FrameDirection):
        await super().process_frame(frame, direction)
        if isinstance(frame, InputAudioRawFrame):
            # diagnostic : l'interface envoie-t-elle du son, ou seulement du silence ?
            self._n += 1
            if frame.audio and np.abs(np.frombuffer(frame.audio, dtype=np.int16)).max() > 500:
                self._voiced += 1
            if time.monotonic() - self._t > 10:
                logger.debug(f"Micro reçu : {self._n} morceaux en 10 s, dont {self._voiced} avec du son")
                self._n = self._voiced = 0
                self._t = time.monotonic()
        if isinstance(frame, InputTransportMessageFrame) and isinstance(frame.message, dict):
            m = frame.message
            kind = m.get("type")
            if kind == "context":
                # lot D (D1) : source unique. Le cœur envoie le profil, le résumé de la conversation et
                # les messages qu'il ne couvre pas ; plus de fenêtre [-40:] ni de coupure des messages.
                global _profil, _resume
                if "profil" in m:
                    _profil = str(m.get("profil") or "")
                _resume = str(m.get("resume") or "").strip()
                hist = [
                    {"role": x["role"], "content": str(x["content"])}
                    for x in m.get("messages", [])
                    if x.get("role") in ("user", "assistant") and x.get("content")
                ]
                self._ctx.set_messages([{"role": "system", "content": system_prompt()}, *hist])
                return
            if kind == "user_text" and m.get("text"):
                # message tapé pendant le mode voix : même cerveau rapide, réponse dite à voix haute
                await self.push_frame(
                    LLMMessagesAppendFrame(messages=[{"role": "user", "content": str(m["text"])}], run_llm=True)
                )
                return
            if kind == "say" and m.get("text"):
                await self.push_frame(TTSSpeakFrame(text=str(m["text"])))
                return
            if kind == "set_llm" and m.get("model"):
                self._s.llm_model = await modele_vocal(self._s, str(m["model"]))
                await self.push_frame(LLMUpdateSettingsFrame(delta=OLLamaLLMService.Settings(model=self._s.llm_model)))
                asyncio.create_task(garder_charge(self._s))
                logger.info(f"Cerveau vocal : {self._s.llm_model}")
                return
            if kind == "set_voice" and m.get("speaker") and self._tts is not None:
                self._tts.set_speaker(str(m["speaker"]))
                logger.info(f"Voix : {m['speaker']}")
                return
            if kind == "stop":
                await self.broadcast_interruption()
                return
            if kind == "contexte_debug":
                # contrôle (lot D) : rôles et début des messages de la conversation, sans le prompt système
                apercu = [
                    {
                        "role": x.get("role"),
                        "outil": ",".join(((c.get("function") or {}).get("name") or "") for c in (x.get("tool_calls") or []) if isinstance(c, dict)),
                        "texte": str(x.get("content") or "")[:160],
                    }
                    for x in self._ctx.get_messages()
                    if isinstance(x, dict) and x.get("role") != "system"
                ]
                await self.push_frame(OutputTransportMessageUrgentFrame(message={"type": "contexte", "messages": apercu}))
                return
        await self.push_frame(frame, direction)


async def garder_charge(s: C.VoiceSettings):
    """Charge le cerveau vocal dans Ollama et l'y garde (sinon ≈ 5 à 10 s de rechargement au premier mot)."""
    import httpx

    base = s.ollama_url.removesuffix("/v1")
    try:
        async with httpx.AsyncClient(timeout=120) as c:
            await c.post(f"{base}/api/generate", json={"model": s.llm_model, "prompt": "", "keep_alive": -1})
        logger.info(f"{s.llm_model} chargé dans Ollama")
    except Exception as e:
        logger.warning(f"Préchargement de {s.llm_model} : {e}")


async def penser(s: C.VoiceSettings, messages: list, question: str, on_thought=None) -> str:
    """Réflexion à la demande : même modèle, mode « thinking » activé, réponse courte à dire.

    La réflexion arrive en flux et part au fil de l'eau dans la Conscience (`on_thought`).
    """
    import json as _json

    import httpx

    base = s.ollama_url.removesuffix("/v1")
    msgs = [m for m in messages if isinstance(m, dict) and m.get("role") in ("system", "user", "assistant") and m.get("content")]
    msgs.append({"role": "user", "content": f"{question}\n\nRéfléchis soigneusement, en français, puis donne une réponse orale claire, en quelques phrases, sans Markdown."})
    content, buf, last = [], [], time.monotonic()
    async with httpx.AsyncClient(timeout=170) as c:
        async with c.stream("POST", f"{base}/api/chat", json={"model": s.llm_model, "messages": msgs, "think": True, "stream": True, "keep_alive": -1}) as r:
            r.raise_for_status()
            async for line in r.aiter_lines():
                if not line.strip():
                    continue
                msg = _json.loads(line).get("message", {})
                if msg.get("thinking"):
                    buf.append(msg["thinking"])
                if msg.get("content"):
                    content.append(msg["content"])
                if on_thought and buf and time.monotonic() - last > 0.25:
                    await on_thought("".join(buf))
                    buf, last = [], time.monotonic()
    if on_thought and buf:
        await on_thought("".join(buf))
    return "".join(content).strip()


def build_tts(s: C.VoiceSettings, lat: LatencyLog):
    if s.tts_model.startswith("crisp:"):
        # voix à usage personnel (Voxtral TTS, Pocket TTS) : licences non commerciales
        from .tts_crisp import CrispTTSService

        voice = s.qwen3_speaker
        if "pocket" in s.tts_model:
            found = list((C.MODELES / "pocket-tts" / "voix").rglob(f"{voice}.wav")) or list((C.MODELES / "pocket-tts" / "voix").rglob("*.wav"))
            voice = str(found[0]) if found else ""
        tts = CrispTTSService(model_path=C.MODELES / s.tts_model.removeprefix("crisp:"), voice=voice, on_latency=lat.hook)
        tts.warmup()
        return tts
    from .tts_qwen3 import Qwen3TTSService

    ref = (C.REFERENCES / s.qwen3_reference) if s.qwen3_reference else None
    tts = Qwen3TTSService(
        model_path=(C.MODELES / "qwen3-tts" / s.tts_model) if s.tts_model else (C.QWEN3_TTS_BASE if ref else C.QWEN3_TTS),
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
        model_path=(C.MODELES / "voxtral" / s.stt_model) if s.stt_model else C.VOXTRAL_GGUF, library=C.transcribe_library(), delay_ms=s.voxtral_delay_ms, on_latency=lat.hook
    )
    if s.mode == "dictee":
        await dictee(s, stt, bridge)
        return

    tts = build_tts(s, lat)
    # cerveau vocal : modèle dérivé à contexte réduit (lot A), créé dans Ollama s'il manque
    s.llm_model = await modele_vocal(s, s.llm_model)
    llm = OLLamaLLMService(
        base_url=s.ollama_url,
        settings=OLLamaLLMService.Settings(
            model=s.llm_model,
            temperature=0.6,
            max_tokens=400,
            # pas de « réflexion » en mode vocal : on veut le premier mot tout de suite.
            # Elle est activée à la demande par l'outil `reflechir`.
            extra={"reasoning_effort": "none", "extra_body": {"think": False}},
        ),
    )

    async def confier(params: FunctionCallParams):
        tache = str(params.arguments.get("tache", "")).strip()
        await bridge.send({"type": "decision", "text": f"C'est un travail concret : je le confie à la boucle centrale d'Orchestra. « {tache} »"})
        await bridge.send({"type": "delegate", "text": tache})
        # accusé de réception immédiat, sans second appel au cerveau (gain d'environ une demi-seconde)
        await params.llm.push_frame(TTSSpeakFrame(text="D'accord, je m'en occupe et je te dis quand c'est prêt."))
        await params.result_callback(
            {"resultat": "Tâche confiée à Orchestra, qui annoncera le résultat."},
            properties=FunctionCallResultProperties(run_llm=False),
        )

    async def reflechir(params: FunctionCallParams):
        question = str(params.arguments.get("question", "")).strip()
        await bridge.send({"type": "thinking", "on": True})
        await bridge.send({"type": "decision", "text": f"Cette question mérite réflexion : je réfléchis avant de répondre. « {question} »"})
        await params.llm.push_frame(TTSSpeakFrame(text="Attends, je réfléchis."))
        t0 = time.perf_counter()
        try:
            async def pensee(t: str):
                await bridge.send({"type": "thought", "text": t})

            reponse = await penser(s, params.context.get_messages(), question, on_thought=pensee)
        except Exception as e:  # le dialogue continue même si la réflexion échoue
            logger.error(f"Réflexion : {e}")
            reponse = "Désolée, je n'ai pas réussi à réfléchir à ça. Tu peux reformuler ?"
        logger.info(f"Réflexion terminée en {time.perf_counter() - t0:.1f} s")
        await bridge.send({"type": "thinking", "on": False})
        await bridge.send({"type": "bot_start"})
        await bridge.send({"type": "bot_text", "text": reponse})
        await bridge.send({"type": "bot_end"})
        await params.llm.push_frame(TTSSpeakFrame(text=reponse))
        await params.result_callback({"reponse_dite": reponse}, properties=FunctionCallResultProperties(run_llm=False))

    llm.register_function("confier_a_orchestra", confier)
    llm.register_function("reflechir", reflechir, timeout_secs=180)
    tools = ToolsSchema(
        standard_tools=[
            FunctionSchema(
                name="confier_a_orchestra",
                description="Confie un travail concret à la boucle centrale d'Orchestra, qui choisit et supervise les agents.",
                properties={"tache": {"type": "string", "description": "La tâche, formulée clairement et complètement."}},
                required=["tache"],
            ),
            FunctionSchema(
                name="reflechir",
                description="Prend le temps de réfléchir à fond avant de répondre. Seulement pour les questions qui le méritent.",
                properties={"question": {"type": "string", "description": "La question complète, avec son contexte."}},
                required=["question"],
            ),
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
            ClientCommands(context, s, tts),
            VADProcessor(vad_analyzer=SileroVADAnalyzer(params=VADParams(stop_secs=s.vad_stop_secs))),
            stt,
            agg.user(),
            DateDuTour(),
            llm,
            tts,
            transport.output(),
            TexteDit(bridge),
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
        enable_rtvi=False,
    )
    bridge.task = task

    @transport.event_handler("on_client_connected")
    async def _connected(_transport, _ws):
        logger.info("Interface connectée")
        nouvelle_session()
        context.set_messages([{"role": "system", "content": system_prompt()}])
        asyncio.create_task(garder_charge(s))
        asyncio.create_task(prechauffer_rappel())
        await bridge.send({"type": "ready", "stt": "voxtral-mini-4b-realtime", "llm": s.llm_model, "tts": s.tts})

    logger.info(f"Serveur voix prêt sur ws://{s.host}:{s.port} (TTS : {s.tts}, cerveau : {s.llm_model})")
    await WorkerRunner(handle_sigint=False).run(task)


async def dictee(s: C.VoiceSettings, stt, bridge: UIBridge):
    """Mode dictée du chat : seulement l'écoute (VAD + Voxtral), le texte part dans la zone de saisie.

    Pas de cerveau ni de voix : la mémoire GPU reste libre pour le cerveau du chat.
    """
    transport = SingleClientWebsocketServerTransport(
        params=SingleClientWebsocketServerParams(
            audio_in_enabled=True, audio_in_sample_rate=16000, audio_out_enabled=False, serializer=OrchestraSerializer(),
        ),
        host=s.host,
        port=s.port,
    )
    pipeline = Pipeline([
        transport.input(),
        CompteurMicro(),
        VADProcessor(vad_analyzer=SileroVADAnalyzer(params=VADParams(stop_secs=0.6))),
        stt,
        transport.output(),
    ])
    task = PipelineWorker(pipeline, params=PipelineParams(audio_in_sample_rate=16000), observers=[bridge],
                          idle_timeout_secs=None, enable_rtvi=False)
    bridge.task = task

    @transport.event_handler("on_client_connected")
    async def _connected(_transport, _ws):
        logger.info("Dictée : interface connectée")
        await bridge.send({"type": "ready", "stt": "voxtral-mini-4b-realtime", "mode": "dictee"})

    logger.info(f"Serveur de dictée prêt sur ws://{s.host}:{s.port}")
    await WorkerRunner(handle_sigint=False).run(task)


def cli():
    p = argparse.ArgumentParser(description="Serveur voix d'Orchestra")
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--llm", default="qwen3.5:9b")
    p.add_argument("--speaker", default="Ryan")
    p.add_argument("--reference", default="", help="fichier de voix/references/ à cloner (avec consentement)")
    p.add_argument("--reference-text", default="")
    p.add_argument("--vad-stop", type=float, default=0.35)
    p.add_argument("--stt-model", default="", help="fichier GGUF de modeles/voxtral")
    p.add_argument("--tts-model", default="", help="dossier de modeles/qwen3-tts")
    p.add_argument("--mode", choices=["voix", "dictee"], default="voix", help="dictee : écoute seule, pour le chat")
    p.add_argument("--num-ctx", type=int, default=8192, help="contexte du cerveau vocal (modèle dérivé dans Ollama)")
    a = p.parse_args()
    s = C.VoiceSettings(
        port=a.port, llm_model=a.llm, qwen3_speaker=a.speaker,
        qwen3_reference=a.reference, qwen3_reference_text=a.reference_text, vad_stop_secs=a.vad_stop,
        stt_model=a.stt_model, tts_model=a.tts_model, mode=a.mode, num_ctx=a.num_ctx,
    )
    asyncio.run(main(s))


if __name__ == "__main__":
    cli()
