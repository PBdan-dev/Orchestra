"""Chemins et réglages du module voix.

Tout vit dans le dossier de données d'Orchestra (pas dans Documents) :

    %USERPROFILE%\\Orchestra\\voix\\
        env\\                 environnement Python (Pipecat, PyTorch…)
        transcribe.cpp\\      moteur de Voxtral, compilé avec CUDA
        modeles\\             poids des modèles (Voxtral, Qwen3-TTS)
        references\\          voix de référence pour le clonage (avec consentement)
        mesures\\             rapports de latence
        voix.log              journal du serveur

ORCHESTRA_HOME remplace %USERPROFILE%\\Orchestra, comme pour l'application.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path


def orchestra_home() -> Path:
    env = os.environ.get("ORCHESTRA_HOME")
    return Path(env) if env else Path.home() / "Orchestra"


VOIX = orchestra_home() / "voix"
MODELES = VOIX / "modeles"
REFERENCES = VOIX / "references"
MESURES = VOIX / "mesures"
MEMOIRE = orchestra_home() / "memoire" / "orchestra.md"

# Modèles retenus (licences commerciales : Apache 2.0)
VOXTRAL_GGUF = MODELES / "voxtral" / "Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf"
QWEN3_TTS = MODELES / "qwen3-tts" / "Qwen3-TTS-12Hz-1.7B-CustomVoice"
QWEN3_TTS_BASE = MODELES / "qwen3-tts" / "Qwen3-TTS-12Hz-1.7B-Base"


def transcribe_library() -> Path | None:
    """Bibliothèque native de transcribe.cpp (transcribe.dll), si elle est compilée."""
    env = os.environ.get("TRANSCRIBE_LIBRARY")
    if env:
        return Path(env)
    root = VOIX / "transcribe.cpp"
    for p in root.glob("build*/**/transcribe.dll"):
        return p
    return None


@dataclass
class VoiceSettings:
    host: str = "127.0.0.1"
    port: int = 8765
    # écoute
    stt: str = "voxtral"  # voxtral
    voxtral_delay_ms: int = 480
    # cerveau du mode vocal
    ollama_url: str = "http://127.0.0.1:11434/v1"
    llm_model: str = "qwen3.5:9b"
    # voix
    tts: str = "qwen3"
    qwen3_speaker: str = "Ryan"
    qwen3_reference: str = ""  # fichier de references/ pour cloner une voix (avec consentement)
    qwen3_reference_text: str = ""
    # détection de fin de parole
    vad_stop_secs: float = 0.35
    # modèles choisis dans le panneau d'Orchestra (vides = modèles par défaut)
    stt_model: str = ""
    tts_model: str = ""
    # « voix » : conversation complète ; « dictee » : écoute seule (micro du chat)
    mode: str = "voix"
    extra: dict = field(default_factory=dict)
