# Module voix d'Orchestra

On parle à Orchestra et elle répond à l'oral. Le chat et la voix sont deux entrées vers la même
boucle : un travail concret demandé à voix haute est confié à l'orchestrateur, exactement comme
un message tapé, et le résultat est relu.

Tout tourne en local. Le module est optionnel : désactivé dans les Paramètres, il ne change rien.

## Pile

| Rôle | Brique | Licence |
|---|---|---|
| Orchestration en flux | Pipecat 1.14 | BSD-2 |
| Détection de fin de parole | Silero VAD (inclus dans Pipecat) | MIT |
| Écoute | Voxtral Mini 4B Realtime 2602, Q8_0, via transcribe.cpp (CUDA) | Apache 2.0 / MIT |
| Cerveau du mode vocal | Qwen 3.5 9B via Ollama, sans réflexion | Apache 2.0 |
| Voix | Qwen3-TTS 12Hz 1.7B CustomVoice, via faster-qwen3-tts (graphes CUDA, en flux) | Apache 2.0 / MIT |

Écartés : Kokoro (sa phonétisation française passe par espeak-ng, sous GPL), le paquet officiel
`qwen-tts` (pas de flux, 6,8 s avant le premier son sur RTX 3090), vLLM (pas de Windows natif).

Licences à faire valider par un juriste avant toute commercialisation. Le clonage de voix
(`voice_reference`) exige le consentement explicite de la personne enregistrée.

## Fichiers

- `orchestra_voix/server.py` : la boucle Pipecat (WebSocket local, port 8765).
- `orchestra_voix/stt_voxtral.py` : connecteur Voxtral Realtime (écrit pour Orchestra).
- `orchestra_voix/tts_qwen3.py` : connecteur Qwen3-TTS en flux (écrit pour Orchestra).
- `orchestra_voix/protocol.py` : format des messages entre l'interface et le serveur.
- `orchestra_voix/bench.py` : mesure de la latence de chaque brique.
- `installer.ps1` : installe tout dans `%USERPROFILE%\Orchestra\voix\`.

Côté application : `src-tauri/src/voice.rs` démarre et arrête le serveur, `src/voice.ts` gère le
micro, la lecture et les niveaux qui font onduler le cœur.

## Installer et mesurer

```powershell
powershell -ExecutionPolicy Bypass -File voix\installer.ps1          # tout (≈12 Go)
& "$env:USERPROFILE\Orchestra\voix\env\Scripts\python.exe" -m orchestra_voix.bench
```

Le rapport de latence est écrit dans `%USERPROFILE%\Orchestra\voix\mesures\`. Chaque tour de
parole réel y est aussi consigné (`latence-AAAA-MM-JJ.jsonl`).

## Mémoire GPU (RTX 3090, 24 Go)

Qwen 3.5 9B (≈6,6 Go) + Voxtral Q8 (≈5 Go) + Qwen3-TTS 1.7B restent chargés ensemble. Le
modèle 27B du mode texte ne tient pas en même temps : Ollama le décharge quand la voix sert.
