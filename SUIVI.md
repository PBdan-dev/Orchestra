# Orchestra

**Statut** : Actif
**Dernière session** : 2026-10-27

## Objectif
Agent orchestrateur local pour PB : il discute, répond, et délègue le travail à des agents qu'il trouve, branche, supervise et mémorise. Il doit pouvoir s'ajouter des fonctionnalités de façon autonome, gérer une mémoire très optimisée, et offrir une interface épurée, visuelle et intuitive.

## Critère de fin
Proposé, à valider par PB (voir « Blocages ») : sur le PC de PB, avec Ollama, Orchestra traite une vraie demande de bout en bout (analyse, choix d'agent, délégation, résultat, mémorisation), installe un agent depuis l'Agent Market réel, et retrouve un fait mémorisé dans une conversation ultérieure.

## État actuel
- V0.2 compilée et lancée sur le PC de PB le 2026-10-26 : `Orchestra.exe` et `Orchestra-setup.exe` à la racine de ce dossier. Code poussé sur GitHub (branche `main`, 3 commits, rien en attente).
- Fonctionne : interface « verre glacé » (cœur animé, Conscience, chat, Mémoire, Journal, Paramètres), état Ollama explicite avec bouton Charger, panneau Agents (brancher, débrancher, installer, désinstaller), bouton Simulation.
- Vérifié le 2026-10-26 dans le journal : un vrai échange avec Ollama a abouti (réponse à « qui es-tu »). La délégation à un agent sur le vrai Ollama n'a pas encore été testée par PB.
- Agent Market : URL toujours absente, Orchestra utilise le registre local (agents de démo).
- Mode vocal (V0.3, en cours depuis le 2026-10-27) : sous-module `voix/` écrit selon le plan de PB (Pipecat, Voxtral Mini 4B Realtime, Qwen 3.5 9B, Qwen3-TTS). Dans l'interface : bouton micro, appuyer-pour-parler, onde du cœur qui suit le micro et la voix, réglages « Voix », délégation vocale vers la boucle centrale. Python, PyTorch CUDA, Pipecat et les modèles (Voxtral Q8, Qwen3-TTS 1.7B) sont installés dans `C:\Users\pbdan\Orchestra\voix\`.
- Mesures du 2026-10-27 : Qwen 3.5 9B sort son premier mot en 138 ms. Qwen3-TTS officiel met 6,8 s avant le premier son : trop lent, remplacé par faster-qwen3-tts (MIT, en flux, graphes CUDA), pas encore mesuré.
- On s'est arrêté en attendant que PB lance deux étapes d'installation bloquées par le garde-fou de Claude (voir « Prochaine action »).

## Prochaine action
PB lance, depuis ce dossier : `powershell -ExecutionPolicy Bypass -File voix\installer.ps1 -Etape python`, puis la même commande avec `-Etape transcribe` (faster-qwen3-tts et compilation CUDA de transcribe.cpp). Ensuite : mesure complète avec `python -m orchestra_voix.bench`, puis essai dans Orchestra (Paramètres, Voix, Activer, puis bouton micro).

## Décisions
- 2026-10-25 — Application de bureau Windows en Tauri 2 (Rust + React/TypeScript), installateur léger.
- 2026-10-25 — Cerveau par défaut : Ollama local (modèle actif détecté) ; LM Studio, Claude, Gemini, OpenRouter en option.
- 2026-10-25 — Mémoire en fichiers Markdown lisibles : un fichier principal pour Orchestra et un par agent.
- 2026-10-25 — Données hors de Documents (`C:\Users\pbdan\Orchestra\`), car l'Accès contrôlé aux dossiers de Windows Defender y bloque les applications inconnues.
- 2026-10-26 — Style « verre liquide glacé » sombre, chat centré et agrandissable, taille du cœur réglable, agents branchables sans casser les autres (retours de PB sur la V1).
- 2026-10-27 — Pile vocale définie par PB : Pipecat (BSD-2), Voxtral Mini Realtime (Apache 2.0), Qwen 3.5 9B en mode vocal, Qwen3-TTS (Apache 2.0). Licences commerciales uniquement (Apache, MIT, BSD). Plan B : Moshi.
- 2026-10-27 — Kokoro écarté par PB (phonétisation française via espeak-ng, sous GPL). Voxtral via transcribe.cpp (MIT), car vLLM ne tourne pas sous Windows natif. Qwen3-TTS via faster-qwen3-tts (MIT), car le paquet officiel est trop lent.
- 2026-10-27 — Voix et chat partagent la mémoire et l'historique ; une demande de travail dite à voix haute passe par l'orchestrateur (outil `confier_a_orchestra`), et le résultat est relu.

## Fichiers et ressources
- Code : ce dossier (`src/` interface React, `src-tauri/src/` cœur Rust). Dépôt : github.com/PBdan-dev/Orchestra. Détails de l'architecture dans `README.md`.
- Données de l'application : `C:\Users\pbdan\Orchestra\` (config.json, memoire/, agents/registre-local.json, espace-de-travail/, journal/).
- Outils de compilation installés dans le profil de PB : Rust (`%USERPROFILE%\.cargo\bin`), Node.js portable (`%USERPROFILE%\.orchestra-build-tools\node`). Visual Studio 2026 (C++) et WebView2 déjà présents. Ajouter les deux chemins au PATH avant `npm` ou `cargo`.
- Modèles Ollama de PB : qwen3.6:27b, qwen3.5:9b, Qwen3.8-27B-Q4-logic, jimscard/new-adult-writer.
- Lancer : `Orchestra.exe` à la racine. Développer : `npm install`, puis `npm run tauri dev` ; compiler : `npm run tauri build` (exécutable dans `src-tauri/target/release/`).
- Projet lié : `2026-10_mail_adagio` est conçu pour être appelé en JSON par Orchestra.
- Voix : code dans `voix/` (voir `voix/README.md`) ; environnement, modèles, transcribe.cpp, mesures et `voix.log` dans `C:\Users\pbdan\Orchestra\voix\`. Serveur local ws://127.0.0.1:8765.

## Étapes
- [x] V1 : orchestrateur transparent (rappel, analyse, recherche d'agent, délégation, mémorisation), interface, registre local
- [x] V0.2 : état Ollama explicite et chargement du modèle, verre glacé, agents modulaires, simulation
- [ ] Mode vocal : finir l'installation (faster-qwen3-tts, transcribe.cpp), mesurer la latence complète, essai par PB
- [ ] PB teste une vraie délégation sur Ollama
- [ ] Brancher l'Agent Market réel (URL et format à fournir par PB)
- [ ] Brancher Mail Adagio comme agent
- [ ] Ajout autonome de fonctionnalités (Orchestra crée ou installe ses propres agents)
- [ ] Mémoire optimisée (index par embeddings, résumés, oubli)

## Blocages et questions ouvertes
- Installation vocale : le garde-fou de Claude bloque l'installation de code externe (transcribe.cpp, faster-qwen3-tts). PB lance les deux commandes ou accorde la permission.
- Voix française : les voix préréglées de Qwen3-TTS ne sont pas natives en français. Une voix clonée (modèle Base, 4,5 Go de plus, avec consentement de la personne) sonnerait plus naturelle. À décider après écoute.
- URL et format de l'Agent Market : à fournir par PB.
- Critère de fin proposé ci-dessus : à valider ou corriger par PB.
- Proposition non validée : les étapes « Ajout autonome » et « Mémoire optimisée » viennent de l'objectif du projet ; leur ordre et leur périmètre restent à décider avec PB.
- Le dossier suit la convention `2026-10_nom` utilisée par l'INDEX actuel, pas la numérotation `NN_` : laissé tel quel pour ne rien déplacer.

## Journal
- 2026-10-25 — V1 construite, compilée et lancée sur le PC de PB ; données déplacées hors de Documents ; mode démo navigateur.
- 2026-10-26 — V0.2 livrée suite aux retours de PB (état Ollama, chargement du modèle, style, agents modulaires, simulation), poussée sur GitHub.
- 2026-10-27 — Projet enregistré dans le suivi : création de ce SUIVI.md et ajout à INDEX.md.
- 2026-10-27 — Mode vocal : sous-module `voix/` et intégration dans l'interface ; dépendances et modèles installés ; première mesure (cerveau 138 ms ; Qwen3-TTS officiel trop lent, remplacé par faster-qwen3-tts).
