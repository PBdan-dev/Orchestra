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

## État actuel — Phase 0 terminée (2026-10-28)
- Branche Git : `main` reste à la V0.2. Le travail voix (jusqu'à `439ece9`) et la Phase 0 sont sur `claude/project-thread-9jt23q` (poussée sur GitHub, partie de `439ece9`). Fusion dans `main` : sur feu vert de PB.
- Mesure : chaque appel au cerveau (analyse, réponse, agent, mémorisation) écrit une ligne dans `Orchestra/mesures/cerveau-AAAA-MM-JJ.jsonl` (tokens du prompt calculés, durées, premier token), pour les 5 fournisseurs.
- Évaluation : `eval/` du dépôt (mémoire de test de 26 faits, 20 questions, 15 tâches, validés par PB). Lanceur : depuis `src-tauri/`, `cargo run --release --example evaluation -- --modele qwen3.6:27b` ; il travaille dans `Orchestra/eval/donnees-test/` (vraie mémoire jamais touchée) et écrit `Orchestra/eval/rapport-<date>.md` + `.jsonl`.
- **Référence** (qwen3.6:27b, température 0, 3 passages, 75 min, `Orchestra/eval/rapport-2026-10-28_1823.md`) : mémoire **20/20** ; routage **14/15 (93 %)**, seul échec « Écris-le. » délégué au lieu de demander une précision ; tour médian 35 s (90e centile 93 s) ; 3,49 appels au cerveau par tour ; prompt calculé en entier à chaque appel (≈ 280 à 1 000 tokens) ; premier token ≈ 0,4 à 0,6 s ; dossier de données hors voix : 19 ko.
- **Référence mémoire v2** (jeu durci à la demande de PB : 56 faits, questions reformulées, pièges, fait modifié, réponses absentes ; 1 passage, 13 min, `Orchestra/eval/rapport-2026-10-28_2021.md`) : **15/20**. Pièges 8/8, absents 2/2 (pas d'invention), direct 3/3, modifié 1/1, **reformulées 1/6** : sans mot commun avec le fait, le rappel par mots ne trouve rien (à corriger par M2, recherche par le sens). Cas grave : « puis-je manger des cacahuètes ? » → « oui » alors que PB est allergique. Réponses médianes de 32 mots.
## État actuel — Lot F terminé (2026-10-28)
- Appel d'outils natif pour les 5 fournisseurs (Ollama `tools`, Claude `tool_use`, LM Studio/OpenRouter `tool_calls`, Gemini `functionCall`) ; repli sur la lecture du texte seulement si le modèle refuse, signalé dans Conscience et dans la mesure (`repli_texte`).
- Sorties contraintes par schéma pour l'analyse et la mémorisation (`format` Ollama, outil imposé Claude, `response_format`, `responseSchema`) ; `extract_json` retiré du chemin normal.
- Délais : connexion 10 s, première réponse 120 s, silence 60 s, appel 600 s ; un seul nouvel essai, seulement si rien n'a été reçu ; erreurs lisibles dans Conscience.
- Intégration continue GitHub Actions (Windows) : interface, `cargo test`, `cargo clippy` — verte. 22 tests unitaires.
- Mesure après (qwen3.6:27b, 1 passage) : 125 appels, 0 repli texte, 0 erreur, 12 appels d'outils natifs par les agents ; mémoire v2 15/20 (inchangée) ; routage **12/15** (réf. 14/15) : les 3 demandes vagues sont désormais déléguées au lieu de demander une précision.
- Règle ajoutée au prompt d'analyse, validée par PB le 28/10 : « demande trop vague → demander une précision » (PB : toujours préférable de demander avant une tâche lourde). Routage **14/15** (`rapport-2026-10-28_2209.md`) ; « crée notes.md avec mes trois projets » demande maintenant le nom des projets.
- Demande de PB (28/10) : réponses plus synthétiques, aller à l'essentiel (« Ton ordinateur s'appelle Zeus. », sans proposition en plus). À traiter avec la réécriture du prompt au lot A, suivi par la longueur des réponses dans le rapport.
- Constaté : `load_ollama` charge le modèle sans `num_ctx`, donc Ollama le recharge au premier vrai appel (18 s mesurées). À corriger avec le lot A (A5).
- Tests : 11 tests unitaires Rust (`cargo test --lib`), pas encore d'intégration continue (lot F).

## État actuel — Lot A terminé (2026-10-29)
- Chat : un seul message système stable (règles, tâches, profil figé pour la session) partagé par l'analyse, la réponse et la mémorisation ; date et souvenirs dans le message de PB, gardé tel quel dans l'historique (`store.ts`) ; consigne de chaque appel ajoutée à la fin ; historique complet jusqu'à 8 000 tokens, coupe marquée au-delà ; règle « va à l'essentiel ».
- `config.json` : `num_ctx` 16 384, `voice_num_ctx` 8 192, seuils 8 000 / 4 000, `reserve_reponse` 4 500. Chargement avec le même `num_ctx` (plus de rechargement de 18 s), modèle gardé tant qu'Orchestra est ouvert, déchargé à la fermeture.
- Voix : prompt sans date ni mémoire variable, profil figé, date en tête du message (étape `DateDuTour`) ; modèle dérivé `qwen3.5:9b-orchestra` (num_ctx 8 192) créé automatiquement dans Ollama. Vérifié : le point d'accès /v1 ignore `num_ctx`.
- Mesures (qwen3.6:27b, température 0, `Orchestra/eval/rapport-2026-10-29_0140.md`) : mémoire **19/20** (avant 15/20 : le profil dans le prompt répond aux questions reformulées), routage **13/15** (« Écris-le. » : réflexion en boucle), tour médian **20 s** (avant 35 s), réponses de **10 mots** en médiane (avant 34).
- Cache sur Qwen 3.6 : partiel. Sur 10 tours, le calcul du prompt passe de 290 à 670 ms pendant que le prompt grandit de 1 550 à 3 000 tokens (≈ 2,5 s sans cache, soit ≈ 70 % d'économie), mais il ne reste pas constant comme le voulait le critère : l'architecture hybride de Qwen 3.6 ne réutilise qu'une partie du calcul. Mémorisation à ~230 ms stable.
- Voix (test_boucle + test_dialogue) : VRAM **18,8 Go** (avant 21,1) ; cerveau vocal 5,5 Go (avant 7,6, contexte 64k) ; premier son 1,6 à 2,7 s (avant 1,7 à 2,7 s), inchangé.
- Constaté : à température 0 (évaluation), Qwen 3.6 tombe parfois dans une réflexion sans fin ; la réserve de 4 500 tokens l'arrête et un message clair remplace la réponse vide. À température 0,6 (recommandée par Qwen), plus aucune boucle sur les mêmes demandes.
- Limite à signaler : en voix, seule la partie profil (Utilisateur, Préférences) d'`orchestra.md` est dans le prompt ; Projets et Connaissances n'y sont plus jusqu'au lot D (rappel en voix).

## État actuel — Lot B terminé (2026-10-30)
- Un seul appel au cerveau pour décider et répondre : réponse directe, lecture d'une page web par Orchestra lui-même (`lire_page`, ajouté à la demande de PB), ou délégation par l'outil `deleguer(besoin, mission, raison)` ; Orchestra choisit l'agent (décision de PB : le prompt ne contient pas la liste des agents). Annonce dans Conscience avant le lancement (B2) ; appel invalide : un nouvel essai, puis réponse directe et avertissement (B3).
- Mesures fiables (qwen3.6:27b, 0,6, carte graphique refroidie), avant → après : **tour médian 23,2 s → 7,4 s** ; appels par tour 3,3 → 2,5 ; premier mot de la réponse ≈ 0,8 s ; mémoire 19/20 → 19/20 ; routage 14/15 → 13/15 (avant / après recalculés avec la notation corrigée). Rapports : `Orchestra/eval/rapport-2026-10-30_1244.md` (avant), `rapport-2026-10-30_1309.md` (après).
- Seule différence de routage : « Lis la page … et fais-m'en une synthèse sourcée » est maintenant traitée par Orchestra lui-même (lecture web) au lieu d'être déléguée à l'agent de veille. Décision attendue de PB.
- Lenteurs du 29/10 : la RTX 3090 surchauffait (92 °C, bridée à 240 MHz, ventilateur à 43 %) ; corrigé par PB le 30/10 (≈ 70 °C sous charge). Réglages Ollama A7 posés par PB : effet quasi nul sur ce modèle (≈ 7 % plus lent, pas de gain de mémoire visible).
- Cahier mis à jour par PB (PR #2, décisions D26 à D37) : à relire avant le lot T1.

## État actuel — Lot T1 (2026-10-30)
- Contrat des agents en place : mission, événements, compte rendu et messages validés avec les 5 schémas de l'Agent Market (copiés dans `contrat/schemas/`) ; protocole stdio-json ; dossier de mission `Orchestra/missions/<id>/` (mission.json, evenements.jsonl, compte_rendu.json, sortie-erreur.log).
- Manifeste 2.0 lu ; une entrée invalide du catalogue est ignorée avec son motif. Les 4 agents de démonstration (fiches exécutées par Orchestra) sont retirés, avec l'accord de PB ; l'ancien registre est gardé dans `agents/registre-local.v1.json`.
- Agents de test installés à la main dans `agents/installes/` (agent-date, agent-fichiers, agent-brouillons, dépôt Orchestra_Agents 352db62), « Non certifié », en attendant le lot T4.
- La mission est préparée par un appel contraint au modèle de demande de l'agent (D33) ; si l'agent ne sait pas faire la mission, rien n'est lancé et Orchestra le dit.
- En attendant les lots suivants (décisions de PB du 30/10) : validation d'une action irréversible refusée automatiquement (boutons au lot T3) ; livrables dans `projets/divers/` (choix du projet au lot T3) ; agent-brouillons attend la porte du modèle (lot T2).
- Banc des agents (`cargo run --release --example banc_agents`) : **17/17** tests d'agent-date et d'agent-fichiers réussis à travers Orchestra (dont validation, refus, pause, arrêt, contenu piégé, mission mal formée).
- Mail Adagio parle déjà le contrat 1 (`agent/mission.py`) : aucun écart vu à la lecture ; essai réel au lot T2.
- Mesure (qwen3.6:27b, 0,6, `Orchestra/eval/rapport-2026-10-30_1617.md`) : mémoire 19/20, routage **15/18** (jeu de tâches mis à jour pour les agents de test : 3 échecs de comportement, dont un « Salut » délégué une fois, à surveiller), tour médian 8,5 s, 2,4 appels par tour. agent-brouillons répond « modèle indisponible » tant que la porte (T2) n'existe pas.

## État actuel — Lot T2 (2026-10-30)
- Porte du modèle (`porte.rs`) : point d'accès local propre à chaque mission (jeton, fermé à la fin), compatible Ollama et OpenAI ; traces `echanges.jsonl` et `pensee.md` dans le dossier de mission ; réflexion de l'agent affichée en direct dans Conscience ; outils non déclarés et actions sans raison signalés ; `num_ctx` d'Orchestra imposé (pas de rechargement).
- Limites D29 comptées (pas, tokens, durée, boucle, 10 min sans progrès) : avertissement ; arrêt propre au plafond d'1 h (décision de PB du 30/10, en attendant les boutons du lot T7).
- Carte graphique (D24, `modele.rs`) : modèle d'Orchestra prêté si l'agent l'admet, sinon modèle de l'agent chargé (voix arrêtée si besoin) puis modèle d'Orchestra rechargé ; refus si le modèle ne tient pas sur la carte.
- Banc : **29/29** (dont les 12 tests d'agent-brouillons par le vrai modèle via la porte, mémoire propre et mail piégé compris).
- Mail Adagio lancé sans modification par la porte (`examples/mission_directe.rs`) : aperçu de 10 mails de sa boîte de démonstration, 10 appels au modèle tracés, rien de changé. La vraie boîte Gmail attend les identifiants (D34, lot T4).
- Brouillon de mail demandé dans le chat : de bout en bout en 26 s (préparation de mission sans réflexion du modèle : 86 s → 3 s).
- Mesure (`Orchestra/eval/rapport-2026-10-30_1858.md`) : mémoire 19/20, routage 16/18, tour médian 13 s (carte graphique à 46 tokens/s pendant la mesure, contre 63 au lot T1).

## Prochaine action
PB lit le résumé du lot T2 ; puis plan du lot T3 (dossiers de mission et de projet, validation à 4 boutons, Conscience en trois parties).

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
- [x] Cahier des charges — Phase 0 : mesure, jeux d'évaluation, lanceur, référence sur qwen3.6:27b (2026-10-28)
- [x] Cahier des charges — Lot F : fiabilisation (2026-10-28)
- [x] Cahier des charges — Lot A : cache (2026-10-29)
- [x] Cahier des charges — Lot B : un seul appel (2026-10-30)
- [x] Cahier des charges — Lot T1 : contrat (2026-10-30)
- [x] Cahier des charges — Lot T2 : porte du modèle (2026-10-30)
- [ ] Cahier des charges — Lot T3 : dossiers de mission et de projet (prochain)
- [ ] Cahier des charges — Lots A, B, T1 à T9, C, D, S, M2, M3, M1 (ordre de la section 11)
- [x] Délégation sur le vrai Ollama vérifiée par l'évaluation (6 tâches déléguées au bon agent, 3/3)
- [ ] PB teste une vraie délégation sur Ollama dans l'interface
- [ ] Brancher l'Agent Market réel (`https://pbdan-dev.github.io/Agent_Market/`)
- [ ] Brancher Mail Adagio comme agent
- [ ] Ajout autonome de fonctionnalités (Orchestra crée ou installe ses propres agents)
- [ ] Mémoire optimisée (index par embeddings, résumés, oubli)

## Blocages et questions ouvertes
- Piste pour plus tard (proposée par PB le 29/10) : garder sur disque le calcul du début fixe du prompt entre deux chargements. Impossible avec Ollama ; possible avec llama-server (llama.cpp, MIT), qui sauvegarde et restaure ce calcul. Gain estimé faible (1 à 2 s de recalcul, contre 18 s de chargement du modèle) : non prioritaire.
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
- 2026-10-28 (soir) — Phase 0 du cahier : écarts code / section 3 signalés à PB (12 constats confirmés, branche main restée en V0.2, chat sur 9B, mémoire presque vide) ; mesure de chaque appel au cerveau ; jeux d'évaluation validés par PB ; lanceur sans interface ; référence qwen3.6:27b : mémoire 20/20, routage 14/15, tour médian 35 s.
- 2026-10-28 (soir) — Jeu mémoire durci (v2) : 15/20, les 5 échecs sont des questions reformulées (rappel par mots) ; faux positif de notation corrigé (h09) ; demande de PB de réponses plus synthétiques notée pour le lot A.
- 2026-10-28 (nuit) — Lot F : outils natifs, sorties contraintes, délais et nouvel essai, intégration continue verte ; routage 12/15 sans la règle « demande vague », 14/15 avec (en attente de PB).
- 2026-10-28 (nuit) — PB garde la règle « demande vague » : routage 14/15.
- 2026-10-29 (nuit) — Lot A : pile stable, profil, concision, voix à 8 192 tokens ; mémoire 19/20, routage 13/15, tour médian 20 s, VRAM voix −2,3 Go ; cache partiel sur Qwen 3.6.
- 2026-10-30 — Lot B : un seul appel, lire_page, deleguer(besoin, mission, raison) ; tour médian 23 s → 7 s. Surchauffe de la carte graphique trouvée et corrigée par PB.
- 2026-10-30 — Lot T1 : contrat stdio-json, manifeste 2.0, agents de démo retirés, agents de test installés ; banc 17/17.
- 2026-10-30 (soir) — Lot T2 : porte du modèle, traces, contrôle des outils, limites, carte graphique ; banc 29/29 ; Mail Adagio par la porte (boîte de démonstration).
