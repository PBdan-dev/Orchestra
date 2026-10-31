# Orchestra — Synthèse du projet

28 octobre 2026 · état du code au commit `439ece9` (branche `main`) · sources : `SUIVI.md` et les documents de conception du 27–28/10, dont ce document reprend l'essentiel.
Le travail à faire est détaillé dans `docs/CAHIER_DES_CHARGES.md`.

## 1. Objet

Orchestra est l'orchestrateur local de PB et la vitrine d'un projet en trois briques :

| Brique | Rôle | État |
|---|---|---|
| **Orchestra** | Agent orchestrateur open source, application Windows | V1 + voix livrées |
| **Agent Market** | Bibliothèque d'agents : à télécharger (installés, transparents) ou à louer (hébergés, livrable seul) | v0 en ligne : `https://pbdan-dev.github.io/Agent_Market/` (catalogue vide) ; agents de base fournis par PB |
| **Cloud Power** (provisoire) | Puissance de calcul partagée entre utilisateurs contre des crédits | Idée pour le futur ; projet actuel = Orchestra + Agent Market |

Promesse d'Orchestra :
1. **Tout voir** : pensées, souvenirs, agents, outils, traces.
2. **Mémoire lisible** : fichiers .md que PB peut lire et corriger.
3. **Chat et voix** : deux entrées vers le même cerveau et la même mémoire.
4. **S'étendre** : trouver et installer depuis l'Agent Market les agents dont il a besoin (téléchargés ou loués). La création d'agents se fait à côté d'Orchestra.

## 2. Architecture

### Deux étages
- **Orchestra** = sélectionneur léger. Discute, cherche sur le web, répond lui-même aux questions simples, demande une précision si la demande est ambiguë, annonce puis délègue tout travail concret. Outils : web, mémoire, recherche d'agents.
- **Agents spécialisés** = font le travail.

### Salle et cuisine
- **Orchestra = la salle** (harnais fixe, petit) : conversation, mémoire, choix de l'agent, mission, porte du modèle (qui gère aussi le chargement des modèles sur la carte graphique), contrôle des livrables, suivi des projets, restitution. Il n'installe que des agents vérifiés par l'Agent Market. Il ne produit rien et n'a pas à connaître les outils des agents.
- **Agents = la cuisine** : programmes autonomes avec leurs propres outils, construits avec un kit commun (boucle bornée, contrat, compte rendu, arrêt propre, point de reprise, contenu externe = donnée). Le kit et le contrôle avant publication relèvent du chantier de création d'agents.

### Types d'agents

| Type | Où il tourne | Ce qu'Orchestra voit | Badge |
|---|---|---|---|
| Local (téléchargé) | Chez PB, programme lancé par Orchestra | Pensées et appels d'outils via la porte ; actions déclarées ; livrables vérifiés | « transparent » |
| Hébergé (loué) | Chez le développeur | Le livrable seul | « livrable seul » |

### Contrat de délégation (JSON simple)
- **Mission** : identifiant, but + critère de réussite, contexte, limites, format de retour, actions soumises à validation.
- **Réponse** : état, résultat, pensées, traces, vérification, point de reprise.
- **Événements** : démarrage, étape, changement d'agent, validation requise, blocage, terminé.
- **Arrêt** en deux temps (propre par défaut, immédiat sur confirmation). **Reprise** sur demande depuis le point de reprise.

### Exécution
- **Mode série** (actuel, un GPU de 24 Go) : demande → Orchestra → agent → travail → retour → Orchestra restitue, tout sur le même GPU, l'un après l'autre. Plus lent ; pas de conversation pendant le travail ; événements affichés.
- **Mode parallèle** (plus tard) : Orchestra délègue et continue de converser pendant que l'agent travaille. Exige assez de mémoire pour plusieurs modèles (~128 Go, mémoire unifiée ou plusieurs cartes) ou une clé API (Claude ou autre). Choix selon la mémoire détectée.

### Modèles et voix

| Usage | Modèle |
|---|---|
| Chat | Qwen 27B local (qwen3.6:27b) via Ollama ; LM Studio, Claude, Gemini, OpenRouter en option |
| Voix — écoute | Voxtral Mini Realtime (transcribe.cpp, Vulkan) |
| Voix — cerveau | Qwen 3.5 9B, réflexion coupée par défaut, outils `confier_a_orchestra` et `reflechir` |
| Voix — synthèse | Voxtral TTS Q8 « fr_female » (usage perso, CC BY-NC) ; Qwen3-TTS (version commerciale) |
| À terme (non arrêté) | API Claude : Haiku par défaut, Sonnet/Opus en escalade, clé fournie par l'utilisateur |

### Technique
- Tauri 2 : cœur Rust (`orchestrator`, `brain`, `memory`, `agents`, `market`, `tools`, `journal`, `voice`), interface React/TypeScript.
- Serveur voix Python (Pipecat) sur `ws://127.0.0.1:8765`, sous-dossier `voix/`.
- Données : `C:\Users\pbdan\Orchestra\` (hors Documents, à cause de Windows Defender).
- Dépôt : github.com/PBdan-dev/Orchestra. Machine : Zeus, GPU 24 Go.

## 3. État au 28 octobre

### Fait
- V1 sur GitHub : cœur, Conscience, mémoire .md, délégation à des agents (registre de démo), plusieurs cerveaux.
- Voix opérationnelle : boucle complète ~1,8 s jusqu'au premier son, coupure de parole, réflexion à la demande, 40 messages d'historique, date transmise.
- Interface : cœur cliquable, panneau des modèles, switch Chat/Voix (change aussi le cerveau), cœur agrandissable, micro coupé et Stop, consignes de la voix éditables, paramètres dédoublonnés.

### Limites constatées dans le code
- **Contexte** : cache presque jamais réutilisé (souvenirs dans le prompt système, 3 appels par tour aux débuts différents, fenêtre glissante). En voix, la date à la minute est en tête du prompt. Oubli au-delà de 5 échanges en chat ; troncature silencieuse à 2 000 caractères.
- **Agents** : appels d'outils lus dans le texte (blocs ```tool```) au lieu de l'appel d'outils natif ; pensée non enregistrée ; attribution incomplète ; écriture à plat avec écrasement ; aucune vérification ; réseau non borné.
- **Mémoire** : doublons et faits périmés jamais nettoyés ; rappel qui relit tout le disque ; embeddings en JSON calculés au rappel.
- **Qualité logicielle** : aucun test automatisé, aucune intégration continue.
- **Voix** : VRAM ~23/24 Go ; passage chat → voix ~30 s.

## 4. Décisions

### Actées
- 25/10 : Tauri 2 ; Ollama par défaut ; mémoire en .md ; données hors Documents.
- 27/10 : pile vocale (Pipecat, Voxtral, Qwen 3.5 9B, Qwen3-TTS) ; licences Apache/MIT/BSD seulement ; chat et voix partagent mémoire et historique ; Voxtral TTS pour l'usage perso.
- 28/10 : messages Orchestra ↔ agents en JSON simple, MCP pour les outils ; arrêt propre par défaut, immédiat sur confirmation ; reprise sur demande ; mode série sans conversation pendant une mission.
- 28/10 : agents = programmes autonomes (« prestataires ») avec leurs propres outils ; leur modèle passe par la porte d'Orchestra ; Orchestra lit et suit les projets sans jamais modifier les livrables.
- 28/10 : agents hébergés acceptés dès la V1 avec badge « livrable seul ».
- 28/10 : brut froid compressé en .zst à J+1 ; faits et résumés restent en .md.
- 28/10 : analyse et réponse fusionnées en un seul appel (outil `deleguer` avec `raison` obligatoire).
- 28/10 : conception de la transparence validée avec corrections ; lot F de fiabilisation (appel d'outils natif, sorties contraintes, tests) validé.
- 28/10 : Conscience sépare « explication de l'agent » et « actions réelles » ; signature des traces par clé locale plus tard.
- 28/10 : voix actuelle conservée ; latence et ElevenLabs plus tard, non prioritaires.
- 28/10 : Orchestra ne crée pas d'agents, il appelle ceux de sa liste ; la création est un chantier à part.
- 28/10 : l'agent s'exécute lui-même ; il réutilise le modèle d'Orchestra s'il convient, sinon la porte décharge celui d'Orchestra, charge le sien, puis recharge celui d'Orchestra (la qualité prime sur la vitesse).
- 28/10 : les règles de transparence font partie de la conception des agents et sont vérifiées avant publication ; Orchestra n'installe que des agents vérifiés, avec un mode développement pour les agents de PB.
- 28/10 : PB fournit un jeu d'agents de base ; pas d'import de formats externes ; Market v0 en ligne à `https://pbdan-dev.github.io/Agent_Market/`.
- 28/10 : périmètre = Orchestra + Agent Market ; Cloud Power et publicité pour le futur.
- 28/10 : badge « Orchestra Certified » pour les agents vérifiés ; lot S de sécurité (agents et contenus piégés) ; critère de fin de la V1 en 11 points (cahier des charges §13) ; Market v0 sur GitHub (projet `2026-10_agent-market`).
- 28/10 : contexte de 16 384 tokens en chat et 8 192 en voix, résumé de session en .md modifiable ; recherche par le sens avec qwen3-embedding 0.6B sur processeur ; mémoire dans l'ordre rappel → propreté → disque ; garde-fous de la consolidation (historique, bouton Annuler, ajouter en cas de doute).

### En attente de PB
- Aucune décision en attente.

## 5. Feuille de route

| Priorité | Chantier | Lots (cahier des charges) |
|---|---|---|
| 0 | Mesure et fiabilisation de base | Phase 0, Lot F |
| 1 | Contexte (plus gros gain de vitesse, surtout en voix) | A, B, C, D |
| 2 | Contrat, porte du modèle, transparence et suivi des projets | T1 à T9 |
| 3 | Mémoire à 3 niveaux | M1, M2, M3 |
| 4 | Extension par le Market : recherche et installation d'agents | Après les lots T |
| 5 | Latence vocale fine | Plus tard |

## 6. Évaluation : complexité et fiabilité

Complexité = effort et risque technique de réalisation. Fiabilité = le résultat attendu sera-t-il obtenu de façon stable, compte tenu de la conception.

| Élément | Complexité | Fiabilité | Limite ou erreur de conception |
|---|---|---|---|
| Phase 0 — Mesure | Faible | Haute | 20 questions = échantillon court ; réponses non déterministes : température 0 et 3 passages par question. |
| A — Cache | Faible | Moyenne | Gain conditionné à la réutilisation du cache par Ollama sur Qwen 3.5/3.6 (attention hybride possible). Non prouvé avant mesure. |
| B — Un seul appel | Faible | Moyenne | Risque de mauvais routage (sur- ou sous-délégation) avec un modèle local. Déjà en place en voix, donc praticable. |
| C — Compaction | Moyenne | Moyenne | **Erreur de conception** : « en arrière-plan » est faux sur un seul GPU. La compaction occupe le même modèle ; si PB parle juste après, sa requête attend. Correction : lancer à l'inactivité, annulable dès que PB parle. Un résumé par modèle local peut perdre des décisions : garder le brut et le rappel ciblé. |
| D — Voix | Moyenne | Moyenne | État partagé entre deux processus (Rust et Python) : source de désynchronisation. Exige une source unique côté Rust. |
| T1 — Contrat et lancement | Moyenne | Haute | Code déterministe ; s'aligne sur le mode JSON de Mail Adagio. |
| T2 — Porte du modèle | Moyenne | Haute | Pensées et appels d'outils observés par Orchestra, pas racontés. Limites : une pensée reste un texte produit par le modèle, pas son raisonnement réel ; ce que le code de l'agent fait hors du modèle n'est que déclaré. Gère aussi le chargement des modèles des agents : bascule de quelques secondes à quelques dizaines de secondes quand le modèle change. |
| T3 — Dossiers de mission et de projet | Faible | Haute | — |
| T4 — Installation | Faible | Haute | Ne contrôle que la déclaration, pas le comportement. |
| T5 — Vérification | Moyenne | Moyenne | Comparaison sur des données structurées (liste des fichiers, empreintes avant/après, appels vus par la porte), pas sur un rapport en texte libre. Faux positifs possibles : levée de quarantaine par PB. |
| T6 — Chaîne de hachage | Faible | Moyenne | Détecte une modification accidentelle, pas une falsification : la chaîne peut être recalculée. Suffisant en usage local ; signature par clé plus tard. |
| T7 — Arrêt et reprise | Élevée | Moyenne | Les effets externes (requête envoyée, fichier modifié) ne s'annulent pas. Reprise fiable seulement pour des étapes sans effet externe. |
| T8 — Agents hébergés | Moyenne | Faible (confiance) | Contrôle de la forme du livrable seulement ; le contexte de la mission quitte la machine. Badge et affichage de ce qui est envoyé. |
| T9 — Suivi des projets | Moyenne | Moyenne à haute | Fiable sur texte, code et .md ; .docx, .pdf, .xlsx par extraction de texte. Gros dossiers : lecture ciblée, appuyée d'abord sur le SUIVI et les résumés de mission. |
| M1 — Disque | Faible | Haute | Gain négligeable au volume actuel. |
| M2 — Rappel | Moyenne à élevée | Haute | Technique éprouvée (BM25 + vecteurs + fusion par rang). Racinisation française approximative (préfixe de 5 lettres). |
| M3 — Propreté | Moyenne | Moyenne | Un modèle local peut décider à tort de REMPLACER un fait vrai. Rattrapable grâce à `historique.md`. |
| Agents locaux autonomes | — | Moyenne | Rien n'empêche techniquement un agent d'écrire hors de son dossier (pas de bac à sable). Parades : contrôle avant publication, empreintes du dossier avant/après, bac à sable plus tard. |
| Latence vocale 0,5–0,8 s | Élevée | Faible | Fin de parole détectée ~420 ms + écoute 230–330 ms + cerveau 270–640 ms + synthèse ~400 ms : le plancher est ~1,2 s en enchaînant les étapes. Objectif réaliste : ~1,0–1,2 s, avec détection sémantique de fin de tour et étapes qui se chevauchent. Non prioritaire : la voix actuelle convient. |

## 7. Viabilité

**Verdict : viable comme outil personnel et comme démonstrateur. Pas encore comme produit.**

Ce qui tient :
- Architecture saine : orchestrateur léger, agents déclaratifs exécutés par lui, contrat unique. La transparence par construction (Orchestra exécute tout) est le bon choix et un vrai différenciateur.
- Pile 100 % locale, licences commerciales, mémoire lisible : cohérent avec la cible.
- Conception déjà documentée et chiffrée ; le code existe et tourne.

Ce qui fragilise :
1. **Matériel** : un seul GPU de 24 Go. En mode série, chaque mission suspend la conversation. Le mode parallèle exige une clé API ou ~128 Go de mémoire pour plusieurs modèles (mémoire unifiée ou plusieurs cartes : aucune carte grand public n'a 128 Go de VRAM).
2. **Modèles locaux** : 9B et 27B tiennent des chaînes d'outils courtes, décrochent sur les tâches longues.
3. **Outils lus dans le texte** et **aucun test automatisé** : corrigés par le lot F (validé).
4. **Agents autonomes** : leurs actions internes sont déclarées, pas observées, et rien n'empêche techniquement un agent d'écrire hors de son dossier. La porte du modèle, la vérification des livrables et le contrôle avant publication réduisent ce risque.

## 8. Ce qui manque (par ordre d'impact)

1. **Appel d'outils natif + sorties contraintes** : lot F, validé.
2. **Tests automatisés et intégration continue** : lot F, validé.
3. **Kit commun des agents** (chantier de création) : il rend les comptes rendus fiables et uniformes. **Suivi des projets** par Orchestra (T9) : prévu.
4. **API de l'Agent Market** : format, recherche, versions, signature des manifestes ; v0 en ligne à `https://pbdan-dev.github.io/Agent_Market/`.
5. **Jeu d'agents de base** fourni par PB, pour tester Orchestra sur de vraies tâches.
6. **Mode parallèle par clé API** : voie la plus accessible vers le mode parallèle.
7. **Critère de fin du projet** validé.

## 9. À vérifier
- Réutilisation du cache Ollama avec Qwen 3.5/3.6 (mesure de la phase 0).
- `num_ctx` non réglable via le point d'accès compatible OpenAI d'Ollama utilisé par Pipecat : passer par un modèle dérivé (Modelfile).
- Prix de l'API Claude et d'ElevenLabs : relevés sur des sources tierces, à confirmer sur les pages officielles.
- Publicité intégrée aux réponses (niveau 3) face aux conditions d'utilisation des fournisseurs de modèles.
- Volumes mémoire : estimations, à mesurer sur le dossier réel.
