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
- Agent Market : v0 en ligne à `https://pbdan-dev.github.io/Agent_Market/` (dépôt `PBdan-dev/Agent_Market`, suivi dans `2026-10_agent-market`) ; Orchestra le lit par défaut et installe ses agents depuis le panneau Agents (lot T4).
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

## État actuel — Lot T3 (2026-10-30)
- Projet choisi par PB dans la barre du haut (« Projet : divers ▾ », nouveau projet possible) ; missions et livrables dans `projets/<nom>/` ; le chat ne peut pas le changer (D36).
- Dossier de mission complet : mission, échanges, pensée, `actions.jsonl` (actions observées par la porte, déclarées par l'agent), compte rendu ; `memoire/agents/<id>.md` renvoie au dossier ; le journal du jour contient début et fin de chaque mission.
- Conscience : une carte par mission, en trois parties (Pensée, Actions observées ou déclarées, Livrables présents ou non), plus les changements de la mémoire propre de l'agent.
- Validation des actions irréversibles (D28) : l'agent attend la réponse de PB (pas de délai pendant l'attente) ; « Oui, cette fois », « Oui, pour ce type d'action pendant la session », « Non », et, plus discrets avec confirmation, « tout pour la session » et « tout, permanent » ; autorisations listées et révocables dans le panneau Agents (`agents/autorisations.json` pour les permanentes). En mode voix, la voix annonce qu'une validation attend à l'écran (décision de PB).
- Mémoire propre d'un agent : bouton « remettre sa mémoire à zéro » qui archive l'ancienne dans `agents/memoires-archivees/` (décision de PB).
- Vérifié : suppression validée puis refusée avec le vrai agent-fichiers (`examples/essai_validation.rs`) ; interface vérifiée dans le navigateur avec le mode démo ; banc 29/29 ; 44 tests unitaires.

## État actuel — Lot T4 (2026-10-31)
- Panneau Agents, section « Agent Market » : catalogue lu (Market officiel par défaut, ou fichier `agents.json` local pour les essais), une entrée invalide ignorée avec son motif sans gêner les autres ; bouton Installer ou « Mettre à jour (x → y) ».
- Installation (`installation.rs`) : seuls les agents « Orchestra Certified » pour leur version (D25, D35) ; paquet téléchargé, taille et SHA-256 comparées au catalogue avant ouverture ; manifeste v2 contrôlé (format 2.x, champs, même id et version que le catalogue, CHANGELOG, modèles des types de tâche) ; tout refus est affiché dans Conscience avec son motif.
- Mise à jour : programme remplacé, mémoire propre gardée (D32), ancienne version mise de côté dans `agents/anciennes-versions/` ; trace `installation.json` (source, empreinte, certification) dans le dossier de l'agent.
- Agent installé devenu non conforme : gardé, utilisable, avertissement visible (D37).
- Identifiants (D34) : demandés dans le panneau Agents avec leur raison, rangés dans le Gestionnaire d'identification de Windows sous `orchestra/<id>/<nom>` (bibliothèque `keyring`) ; jamais réaffichés ni écrits ailleurs.
- Mode développement (Paramètres, désactivé par défaut) : agents non certifiés acceptés avec le badge « Non certifié », et installation depuis un fichier .zip.
- Nouvelles bibliothèques (accord de PB) : `zip`, `sha2`, `keyring` (MIT/Apache).
- Agent Market : PR #7 (fusionnée le 31/10) ajoute les 3 agents de test en `non_certifie` (visibles en mode développement seulement) et aligne le README sur D26 ; à retirer quand les vrais agents seront publiés (décision de PB).
- Vérifié : `examples/essai_installation.rs` avec les vrais paquets des Releases, 10/10 (certifié installé, non certifié refusé puis accepté en mode développement, empreinte et taille fausses refusées, format 3.x refusé, mise à jour 1.0.2 → 1.0.3 avec mémoire gardée, avertissement D37, mission de bout en bout) ; coffre de Windows écrit, relu puis vidé (test manuel) ; 48 tests unitaires ; interface vérifiée en mode démo.

## État actuel — Lot T5 (2026-10-31)
- Après chaque mission (`verification.rs`), Orchestra compare ce que l'agent déclare à ce qu'il constate : empreintes SHA-256 de chaque fichier du projet avant et après (fichier annoncé absent, fichier créé, modifié ou supprimé sans être annoncé), actions déclarées contre appels vus par la porte, outils hors de la liste de l'agent, action irréversible sans validation ou malgré un refus, mémoire propre modifiée sans le dire, livrable contrôlé avec son modèle (D33), « partiel » sans `manquant`, écarts au contrat. Un échec avoué n'est pas un écart.
- Résultat dans `verification.md` (dossier de mission) et dans une partie « Vérification » de la carte de mission ; la voix prévient en cas d'écart.
- Confiance (`agents/confiance.json`, décisions de PB du 31/10) : au 1er écart, l'agent est retiré des « Agents de confiance » et ne le redevient que par « rétablir la confiance » ; à 3 écarts au total, quarantaine : installé mais jamais choisi, jusqu'à « lever la quarantaine » (compteur remis à zéro, historique gardé). Boutons et badges dans le panneau Agents.
- Un agent non certifié n'est jamais promu « de confiance » (les 3 agents de test ne le sont donc plus ; ils restent utilisables).
- Vérifié : `examples/essai_verification.rs` avec 5 agents piégés et le vrai agent-fichiers, 10/10 ; banc des agents 29/29 ; 54 tests unitaires ; interface vérifiée en mode démo.

## État actuel — Lot T6 (2026-10-31)
- Chaîne d'empreintes SHA-256 (`integrite.rs`) : chaque ligne du journal, de `echanges.jsonl` et de `actions.jsonl` porte `prev` (empreinte de la ligne précédente) ; `<fichier>.chaine` garde le nombre de lignes et l'empreinte de la dernière (fin coupée, dernière ligne modifiée). Les traces antérieures au lot sont « non chaînées », sans alerte. Signature par clé locale : plus tard (D15).
- Vérification au lancement (en arrière-plan, résultat dans Conscience) et à l'ouverture du dossier d'une mission.
- Cassure (décision de PB du 31/10) : copie mise à l'abri dans `integrite/preuves/` ; si elle est dans le dossier d'une mission, écart du lot T5 pour l'agent de la mission (confiance retirée, quarantaine au cumul) ; alerte rouge en haut de Conscience jusqu'au clic « J'ai vu » ; rien n'est réparé ni effacé ; une cassure déjà signalée n'est pas recomptée (`integrite/ruptures-signalees.json`). PB a choisi de voir à l'usage si c'est assez strict.
- Limite : un agent qui modifie à la fois une trace et son fichier `.chaine` en recalculant toute la chaîne n'est pas détecté ; la signature par clé locale (D15) couvrira ce cas.
- Vérifié : `examples/essai_integrite.rs`, 9/9 (vraie porte avec réponse fixe, ligne modifiée, ligne retirée, fin coupée, preuve, écart compté une seule fois) ; banc 29/29, essais T4 et T5 toujours 10/10 ; 58 tests unitaires.

## État actuel — Lot T7 (2026-10-31)
- Pilotage de la mission en cours (`pilote.rs`) depuis sa carte dans Conscience : Pause / Reprendre, Arrêter (arrêt propre), arrêt immédiat après confirmation (programme de l'agent coupé, état `arrete_force`, « fichiers possiblement incomplets »). Pendant une pause ou après un arrêt demandé, la porte ne sert plus le modèle de l'agent (l'appel en cours se termine) ; pas de délai de silence pendant une pause.
- Limites D29 : pause et question « on continue ? » sur la carte (Continuer / Arrêter), y compris au plafond d'1 h (plus d'arrêt automatique) ; sans interface (évaluation), simple avertissement.
- Reprise (décision de PB du 31/10, option A) : mission arrêtée ou en pause → `reprise.json` (mission, point de reprise, actions à effet déjà faites) ; bouton « Reprendre la mission » (carte et vue Missions) : nouvelle mission avec le champ facultatif `reprise` du contrat (point de reprise rendu tel quel), même agent, même projet ; les actions déjà faites sont rappelées et jamais refaites. PR Agent_Market #8 (schéma) et Orchestra_Agents #5 (agent-fichiers 1.0.1 qui sait reprendre, test 11-reprise) à fusionner par PB ; ensuite, paquet 1.0.1 à publier et catalogue à mettre à jour.
- Mode série : une demande tapée pendant un travail part dans une file d'attente visible (retirable), avec un message clair ; elle est envoyée dès la fin. En voix, Orchestra annonce le début et la fin de la mission (le serveur voix garde son propre cerveau : il n'est pas coupé pendant une mission qui utilise le modèle d'Orchestra).
- Vue « Missions » (onglet) : date, projet, agent, mission, état, vérification, ouvrir le dossier, reprendre ; mémoire de la carte graphique (nvidia-smi) et « Mode série ».
- Vérifié : `examples/essai_reprise.rs` 7/7 (pause et reprise, porte suspendue, arrêt propre puis reprise sans refaire la 1re opération, arrêt immédiat en 0,1 s) ; interface vérifiée en mode démo (boutons, file d'attente, vue Missions) ; banc 29/29, essais T5 10/10 et T6 9/9 toujours bons.

## État actuel — Lot T9 (2026-10-31)
- Suivi des projets (`suivi.rs`), en lecture seule : outils du cerveau d'Orchestra `projet_historique`, `projet_lister`, `projet_lire` (texte, Markdown, code ; .docx et .xlsx extraits à la main, .pdf avec `pdf-extract`, MIT, accord de PB). Gros fichier : une page de 6 000 caractères à la fois avec sa structure, ou les passages qui contiennent les mots cherchés. Chemins hors du projet refusés. Tous les projets lisibles (décision de PB).
- Fin de mission : résumé de 5 à 10 lignes par le modèle d'Orchestra (sortie contrainte, sans réflexion, accord de PB pour cet appel en plus), état du projet et prochaines étapes → `projets/<nom>/SUIVI.md` (seul fichier qu'Orchestra écrit dans un projet ; état gardé dans `suivi/<nom>.json`). Aucun outil ne sait écrire un livrable.
- Consigne : 4e façon de répondre, « Suivre les projets » (historique ou SUIVI.md d'abord, puis seulement les livrables utiles).
- Vérifié avec le vrai modèle (qwen3.5:9b), `examples/essai_suivi.rs` : 2 missions d'agent-brouillons sur `salon-2027`, SUIVI.md écrit, puis 5 questions : 5/5 au premier passage (stand, budget, destinataire, nouvelle date, nombre de missions), livrables inchangés. À la 5e, les fichiers étaient cités par leur dossier de mission : l'historique donne maintenant les fichiers produits, et le test l'exige. Second passage impossible : la carte graphique était occupée par ComfyUI (cerveau sans réponse en 60 s) ; à relancer, avec l'évaluation du routage (la consigne a changé).

## État actuel — Lot T9 : tests repris le 31/10 au soir (carte graphique libérée)
- ComfyUI gardait ses modèles en mémoire graphique (21,7 Go occupés sur 24) : fermé par PB, il reste 2,7 Go au repos.
- Corrections après les tests : nouvel essai quand Ollama refuse un appel d'outil mal écrit (« XML syntax error ») ; relance unique quand le modèle a lu des fichiers ou délégué sans conclure ; projet actif indiqué au début de chaque message (« projet actif : … ») et consigne « cherche d'abord dans le projet actif » ; « lire sert à répondre, ranger ou modifier est une production » ; l'historique d'un projet donne les fichiers produits ; l'évaluation crée `projets/divers/notes.txt` (t17).
- Résultats (qwen3.6:27b pour l'évaluation, qwen3.5:9b pour essai_suivi) : aiguillage 14/18 sur 3 passages (13/18 avant ces corrections ; échecs restants t07, t08, t14, t15, où Orchestra répond qu'il ne peut pas ou demande une précision au lieu de passer par la délégation : la notation « hors_capacite » est peut-être à revoir) ; mémoire 19/20 ; essai_suivi 5/5 quand les deux missions ont lieu, sinon Orchestra décrit fidèlement le projet réel (une seule mission) : l'instabilité vient du lancement de la mission par qwen3.5:9b, pas de la lecture.

## État actuel — Lot T8 (2026-10-31)
- Agents hébergés (`heberge.rs`, D2) : mission envoyée en POST HTTPS (http refusé), réponse en lignes JSON (événements puis compte rendu) ou compte rendu seul. Ne part que la mission, sans chemin local (projet désigné par son nom), sans manifeste, mémoire ni fichier ; copie de ce qui est parti dans `envoye.json`.
- Installation depuis le catalogue sans paquet : statut `livrable_seul` (ou non certifié en mode développement) ; « ce qui quittera la machine » (adresse, données envoyées, conservation déclarée) affiché et confirmé avant l'installation, gardé dans `installation.json`.
- Contrôle automatique du livrable : compte rendu valide, aucun fichier annoncé (pas d'accès aux fichiers), texte non vide selon `format_retour`, durée dans la limite ; sinon `echec` avec motif.
- Badges « transparent » (local) et « livrable seul » (hébergé) dans le panneau Agents ; jamais « de confiance » automatiquement (règle du lot T5).
- Écart au cahier, tranché par moi (réversible) : la clé éventuelle d'un agent hébergé n'est pas dans `config.json` (comme le dit T8) mais dans le coffre de Windows comme identifiant déclaré (D34, et lot S-c qui sort les clés de `config.json`).
- Arrêt pendant l'appel : l'appel est abandonné (échec avec motif).
- Vérifié : `examples/essai_heberge.rs` avec un faux agent hébergé local en HTTPS (certificat de test), 11/11 ; banc 29/29, essai T5 10/10, essai T7 7/7 avec agent-fichiers 1.0.1 (6/7 avec la 1.0.0 installée, qui ne sait pas reprendre : à mettre à jour depuis le panneau Agents en mode développement). Bibliothèques de test seulement (dev) : rcgen, rustls, tokio-rustls (MIT/Apache).

## Prochaine action
Plan du lot C (section 11 du cahier), puis D, S, M2, M3, M1. Notation « hors_capacite » revue avec PB le 31/10 : une réponse directe « pas possible, aucun agent » compte comme juste (16/18 sur le dernier passage complet).

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
- [x] Cahier des charges — Lot T3 : dossiers de mission et de projet (2026-10-30)
- [x] Cahier des charges — Lot T4 : installation (2026-10-31)
- [x] Cahier des charges — Lot T5 : vérification et confiance (2026-10-31)
- [x] Cahier des charges — Lot T6 : intégrité (2026-10-31)
- [x] Cahier des charges — Lot T7 : exécution, arrêt, reprise (2026-10-31)
- [x] Cahier des charges — Lot T9 : suivi des projets (2026-10-31)
- [x] Cahier des charges — Lot T8 : agents hébergés (2026-10-31)
- [ ] Cahier des charges — Lot C (prochain)
- [ ] Cahier des charges — Lots A, B, T1 à T9, C, D, S, M2, M3, M1 (ordre de la section 11)
- [x] Délégation sur le vrai Ollama vérifiée par l'évaluation (6 tâches déléguées au bon agent, 3/3)
- [ ] PB teste une vraie délégation sur Ollama dans l'interface
- [x] Brancher l'Agent Market réel (`https://pbdan-dev.github.io/Agent_Market/`, lot T4)
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
- 2026-10-30 (nuit) — Lot T3 : projet choisi dans l'interface, carte de mission en trois parties, validation à 4 portées, mémoire propre des agents.
- 2026-10-31 — Lot T4 : installation depuis l'Agent Market (certification, empreinte, manifeste v2, mise à jour qui garde la mémoire, identifiants dans le coffre de Windows, mode développement) ; PR #7 sur Agent_Market, fusionnée par PB.
- 2026-10-31 — Lot T5 : vérification de chaque mission (déclaré contre constaté), verification.md, confiance retirée au 1er écart, quarantaine au 3e ; essai avec agents piégés 10/10.
- 2026-10-31 — Lot T6 : traces chaînées (journal, échanges, actions), vérification au lancement et à l'ouverture d'une mission, cassure : preuve gardée, écart pour l'agent, alerte.
- 2026-10-31 — Lot T7 : pause, arrêt propre, arrêt immédiat, limites en pause avec question, reprise depuis le point de reprise (champ `reprise` du contrat), file d'attente, vue Missions ; PR #8 Agent_Market et #5 Orchestra_Agents.
- 2026-10-31 — Lot T9 : suivi des projets en lecture seule (historique, liste, lecture ciblée, .docx/.xlsx/.pdf), SUIVI.md de chaque projet tenu par Orchestra ; 5/5 questions justes avec le vrai modèle.
- 2026-10-31 (soir) — Tests du T9 repris après fermeture de ComfyUI ; corrections (projet actif, relances) : aiguillage 14/18, mémoire 19/20. Lot T8 : agents hébergés « livrable seul », 11/11 avec un faux agent HTTPS.
