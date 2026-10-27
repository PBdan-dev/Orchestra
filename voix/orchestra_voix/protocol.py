"""Protocole entre l'interface d'Orchestra et le serveur voix (WebSocket local).

Interface → serveur
- binaire : audio du micro, PCM 16 bits mono 16 kHz (morceaux de 20 ms)
- texte JSON :
    {"type": "context", "messages": [{"role","content"}…]}   historique du chat
    {"type": "say", "text": "…"}                              lire un texte à voix haute
    {"type": "stop"}                                          couper la parole

Serveur → interface
- binaire : audio de la voix, PCM 16 bits mono 24 kHz
- texte JSON :
    {"type": "ready", …}                       serveur prêt, modèles chargés
    {"type": "user_interim", "text"}           transcription provisoire
    {"type": "user_final", "text"}             transcription finale
    {"type": "bot_start"} / {"type": "bot_text", "text"} / {"type": "bot_end"}
    {"type": "delegate", "text"}               tâche confiée à la boucle centrale d'Orchestra
    {"type": "interrupt"}                      la personne a coupé la parole : vider la lecture
    {"type": "latency", …}                     mesures du dernier tour, en millisecondes
"""

from __future__ import annotations

import json

from pipecat.frames.frames import (
    Frame,
    InputAudioRawFrame,
    InputTransportMessageFrame,
    InterruptionFrame,
    OutputAudioRawFrame,
    OutputTransportMessageFrame,
    OutputTransportMessageUrgentFrame,
)
from pipecat.serializers.base_serializer import FrameSerializer

IN_RATE = 16000


class OrchestraSerializer(FrameSerializer):
    async def serialize(self, frame: Frame) -> str | bytes | None:
        if isinstance(frame, OutputAudioRawFrame):
            return frame.audio
        if isinstance(frame, InterruptionFrame):
            return json.dumps({"type": "interrupt"})
        if isinstance(frame, (OutputTransportMessageFrame, OutputTransportMessageUrgentFrame)):
            if self.should_ignore_frame(frame):
                return None
            return json.dumps(frame.message, ensure_ascii=False)
        return None

    async def deserialize(self, data: str | bytes) -> Frame | None:
        if isinstance(data, (bytes, bytearray)):
            return InputAudioRawFrame(audio=bytes(data), sample_rate=IN_RATE, num_channels=1)
        try:
            msg = json.loads(data)
        except ValueError:
            return None
        if not isinstance(msg, dict):
            return None
        return InputTransportMessageFrame(message=msg)
