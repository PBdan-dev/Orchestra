# Orchestra

**Statut** : Actif
**Dernière session** : 2026-10-28

## Objectif
Agent orchestrateur local pour PB : il discute, répond, et délègue le travail à des agents qu'il trouve, branche, supervise et mémorise. Il doit pouvoir s'ajouter des fonctionnalités de façon autonome, gérer une mémoire très optimisée, et offrir une interface épurée, visuelle et intuitive.

## Critère de fin
Validé le 2026-10-28 : les 11 critères de `docs/CAHIER_DES_CHARGES.md` §13 (bout en bout chat et voix, 3 agents installés depuis le Market, changement de modèle, dossiers de mission, suivi de projet, mémoire ≥ 18/20, routage ≥ 90 %, latence vocale, arrêt et reprise, 5 jours d'usage réel, 8 scénarios de sécurité).

## État actuel
- V0.2 compilée et lancée sur le PC de PB le 2026-10-26 : `Orchestra.exe` et `Orchestra-setup.exe` à la racine de ce dossier. Code poussé sur GitHub (branche `main`, 3 commits, rien en attente).
- Fonctionne : interface « verre glacé » (cœur animé, Conscience, chat, Mémoire, Journal, Paramètres), état Ollama explicite avec bouton Charger, panneau Agents (brancher, débrancher, installer, désinstaller), bouton Simulation.
- Vérifié le 2026-10-26 dans le journal : un vrai échange avec Ollama a abouti (réponse à « qui es-tu »). La délégation à un agent sur le vrai Ollama n'a pas encore été testée par PB.
- Agent Market : v0 en ligne à `https://pbdan-dev.github.io/Agent_Market/` (catalogue vide, dépôt `PBdan-dev/Agent_Market`, suivi dans `2026-10_agent-market`) ; Orchestra utilise encore le registre local (agents de démo).
- Mode vocal (V0.3, en cours depuis le 2026-10-27) : sous-module `voix/` écrit selon le plan de PB (Pipecat, Voxtral Mini 4B Realtime, Qwen 3.5 9B, Qwen3-TTS). Dans l'interface : bouton micro, appuyer-pour-parler, onde du cœur qui suit le micro et la voix, réglages « Voix », délégation vocale vers la boucle centrale. Python, PyTorch CUDA, Pipecat et les modèles (Voxtral Q8, Qwen3-TTS 1.7B) sont installés dans `C:\Users\pbdan\Orchestra\voix\`.
- Boucle vocale testée de bout en bout le 2026-10-27 (phrase jouée comme un micro) : environ 1,8 s entre la fin de la phrase et le premier son de la réponse. Détail : fin de parole détectée ≈ 420 ms, Voxtral ≈ 230 à 330 ms, Qwen 3.5 9B ≈ 270 à 640 ms, Qwen3-TTS (faster-qwen3-tts) ≈ 400 à 450 ms. Une demande de travail est bien confiée à l'orchestrateur (« D'accord, je m'en occupe » en 2,4 s).
- Voxtral tourne via transcribe.cpp précompilé sur le GPU par Vulkan (pas de compilation CUDA). transformers figé à 5.15.1 (5.20 et 5.21 cassent Qwen3-TTS).
- Mémoire GPU presque pleine pendant la voix (≈ 23 Go sur 24, avec Ollama en contexte 64k).
- Interface (2026-10-27 soir) : switch Chat / Voix dans le cœur ; clic sur le cœur = panneau des modèles (cerveau chat ; écoute, cerveau voix, synthèse, voix) avec listes déroulantes. Le switch charge le cerveau du mode et décharge l'autre ; passer en chat arrête le serveur voix (27B + voix > 24 Go), retour en voix ≈ 30 s.
- Voix : réflexion coupée en mode voix, « Attends, je réfléchis » via l'outil `reflechir` quand la question l'exige ; réflexion active en chat. Coupure de parole activée par défaut. Dialogue continu testé sur 4 tours.
- Voix choisie par PB (usage personnel) : Voxtral TTS « fr_female » (CC BY-NC) via CrispASR (MIT, Vulkan), dans un processus séparé (conflit de ggml.dll avec l'écoute). Version légère Q4 par défaut, Q8 disponible dans le panneau ; PB jugera la différence à l'usage. Boucle complète : 1,6 à 2,8 s. Qwen3-TTS reste la seule voix de la version commerciale. Pocket TTS français écarté (trop lent).

## État actuel — conception (2026-10-28)
- Synthèse du projet et cahier des charges pour Claude Code rédigés : `docs/SYNTHESE.md` (dont évaluation complexité / fiabilité et viabilité) et `docs/CAHIER_DES_CHARGES.md` (lots, critères d'acceptation, formats du contrat). Exécutables sans attendre : Phase 0 (mesure), lot A (cache), T1 (contrat), T8 (agents hébergés).

## Prochaine action
PB lance Claude Code sur la Phase 0 et le lot F avec `docs/CAHIER_DES_CHARGES.md`.

## Décisions
- 2026-10-25 — Application de bureau Windows en Tauri 2 (Rust + React/TypeScript), installateur léger.
- 2026-10-25 — Cerveau par défaut : Ollama local (modèle actif détecté) ; LM Studio, Claude, Gemini, OpenRouter en option.
- 2026-10-25 — Mémoire en fichiers Markdown lisibles : un fichier principal pour Orchestra et un par agent.
- 2026-10-25 — Données hors de Documents (`C:\Users\pbdan\Orchestra\`), car l'Accès contrôlé aux dossiers de Windows Defender y bloque les applications inconnues.
- 2026-10-26 — Style « verre liquide glacé » sombre, chat centré et agrandissable, taille du cœur réglable, agents branchables sans casser les autres (retours de PB sur la V1).
- 2026-10-27 — Pile vocale définie par PB : Pipecat (BSD-2), Voxtral Mini Realtime (Apache 2.0), Qwen 3.5 9B en mode vocal, Qwen3-TTS (Apache 2.0). Licences commerciales uniquement (Apache, MIT, BSD). Plan B : Moshi.
- 2026-10-27 — Kokoro écarté par PB (phonétisation française via espeak-ng, sous GPL). Voxtral via transcribe.cpp (MIT), car vLLM ne tourne pas sous Windows natif. Qwen3-TTS via faster-qwen3-tts (MIT), car le paquet officiel est trop lent.
- 2026-10-27 — Voix et chat partagent la mémoire et l'historique ; une demande de travail dite à voix haute passe par l'orchestrateur (outil `confier_a_orchestra`), et le résultat est relu.

- 2026-10-28 — Messages Orchestra ↔ agents en JSON simple (MCP pour les outils) ; arrêt propre par défaut, immédiat sur confirmation ; reprise sur demande ; mode série sans conversation pendant une mission.
- 2026-10-28 — Agents installés = manifestes exécutés par Orchestra ; l'étagère du harnais = modules du runtime activés par le manifeste. (remplacé le même jour par le modèle « prestataire », ci-dessous)
- 2026-10-28 — Agents hébergés de l'Agent Market acceptés dès la V1 avec badge « livrable seul ».
- 2026-10-28 — Analyse et réponse fusionnées en un seul appel (outil `deleguer` avec `raison` obligatoire, cas 2).
- 2026-10-28 — Conception de la transparence validée avec corrections ; lot F de fiabilisation validé ; Conscience sépare explication de l'agent et actions réelles ; signature des traces plus tard.
- 2026-10-28 — Voix actuelle conservée (latence, ElevenLabs : plus tard). Modes série / parallèle confirmés (parallèle plus tard).
- 2026-10-28 — Orchestra ne crée pas d'agents, il appelle ceux de sa liste (téléchargés ou loués) ; la création est un chantier à part. PB fournit un jeu d'agents de base et teste l'URL du Market. Périmètre : Orchestra + Agent Market.
- 2026-10-28 — Agents = programmes autonomes (« prestataires ») avec leurs propres outils ; Orchestra donne la mission, l'agent rend compte ; le modèle de l'agent passe par la porte d'Orchestra qui enregistre pensées et appels d'outils. Orchestra suit les projets en lecture seule et explique ce qui a été fait (salle / cuisine).
- 2026-10-28 — L'agent s'exécute lui-même et demande son modèle via la porte d'Orchestra : modèle d'Orchestra réutilisé s'il convient, sinon déchargé puis rechargé après la mission (qualité avant vitesse). Seuls les agents vérifiés par l'Agent Market sont installables ; mode développement pour les agents de PB.
- 2026-10-28 — Contexte 16 384 tokens en chat et 8 192 en voix, résumé de session en .md modifiable ; embedding qwen3-embedding 0.6B sur CPU ; lots mémoire M2 → M3 → M1 ; garde-fous de la consolidation (historique.md, bouton Annuler, ajouter en cas de doute).
- 2026-10-28 — Critère de fin validé (11 points, dont les tests de sécurité proposés par PB) ; lot S de sécurité ; badge « Orchestra Certified » ; Market v0 sur GitHub, suivi dans `2026-10_agent-market`.
- 2026-10-28 — Brut froid (archives, journal, pensées de mission) compressé en .zst à J+1 ; faits et résumés restent en .md.
## Fichiers et ressources
- Conception : `docs/SYNTHESE.md`, `docs/CAHIER_DES_CHARGES.md`.
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
- [x] Mode vocal : installation et boucle complète testée (≈ 1,8 s)
- [ ] Mode vocal : essai au micro par PB, puis réduction de la latence vers 0,8 s
- [ ] PB teste une vraie délégation sur Ollama
- [ ] Brancher l'Agent Market réel (`https://pbdan-dev.github.io/Agent_Market/`)
- [ ] Brancher Mail Adagio comme agent
- [ ] Ajout autonome de fonctionnalités (Orchestra crée ou installe ses propres agents)
- [ ] Mémoire optimisée (index par embeddings, résumés, oubli)

## Blocages et questions ouvertes
- Installation vocale : le garde-fou de Claude bloque l'installation de code externe ; PB lance lui-même `voix\installer.ps1` quand il faut.
- Voix française : les voix préréglées de Qwen3-TTS ne sont pas natives en français. Une voix clonée (modèle Base, 4,5 Go de plus, avec consentement de la personne) sonnerait plus naturelle. À décider après écoute.
- Proposition non validée : les étapes « Ajout autonome » et « Mémoire optimisée » viennent de l'objectif du projet ; leur ordre et leur périmètre restent à décider avec PB.
- Le dossier suit la convention `2026-10_nom` utilisée par l'INDEX actuel, pas la numérotation `NN_` : laissé tel quel pour ne rien déplacer.

## Journal
- 2026-10-25 — V1 construite, compilée et lancée sur le PC de PB ; données déplacées hors de Documents ; mode démo navigateur.
- 2026-10-26 — V0.2 livrée suite aux retours de PB (état Ollama, chargement du modèle, style, agents modulaires, simulation), poussée sur GitHub.
- 2026-10-27 — Projet enregistré dans le suivi : création de ce SUIVI.md et ajout à INDEX.md.
- 2026-10-27 — Mode vocal : sous-module `voix/` et intégration dans l'interface ; dépendances et modèles installés ; première mesure (cerveau 138 ms ; Qwen3-TTS officiel trop lent, remplacé par faster-qwen3-tts).
- 2026-10-27 — Boucle vocale complète testée : Voxtral (Vulkan), Qwen 3.5 9B, Qwen3-TTS en flux ; environ 1,8 s jusqu'au premier son ; délégation vocale vers l'orchestrateur fonctionnelle. Exe recompilé.
- 2026-10-27 (soir) — Panneau des modèles et switch dans le cœur ; dialogue vocal continu et réflexion à la demande ; Voxtral TTS fr_female choisie par PB pour l'usage personnel (Q4 par défaut, Q8 disponible).
- 2026-10-28 — Synthèse et cahier des charges rédigés à partir des 4 documents de conception et du code (commit 439ece9) ; 4 décisions de PB enregistrées ; évaluation de complexité, fiabilité et viabilité.
