# Orchestra

Agent orchestrateur local, transparent et extensible. Orchestra ne fait pas les tâches lui-même :
il discute, répond aux questions, et **délègue le travail à des agents** qu'il va chercher,
branche, supervise et mémorise.

- **Application desktop** Windows (Tauri 2 : Rust + React/TypeScript), installateur léger.
- **Cerveau greffable** : Ollama (défaut, local, modèle actif détecté), LM Studio, Claude, Gemini, OpenRouter.
- **Transparence totale** : réflexion en direct, souvenirs rappelés, agents branchés, outils utilisés, faits enregistrés.
- **Mémoire lisible** : des fichiers Markdown, un principal pour Orchestra et un par agent.
- **Agent Market** : interface abstraite (URL configurable, manifestes JSON) avec un registre local de secours.

## Lancer

Installateur : `src-tauri/target/release/bundle/nsis/Orchestra_0.1.0_x64-setup.exe`
Exécutable direct : `src-tauri/target/release/orchestra.exe`

Prérequis : [Ollama](https://ollama.com) lancé avec au moins un modèle (`ollama pull qwen3.5:9b`), ou un autre cerveau configuré dans Paramètres.

## Données (en clair, dans `C:\Users\<toi>\Orchestra`)

Le dossier peut être changé avec la variable d'environnement `ORCHESTRA_HOME`. Il n'est pas dans `Documents`
car l'« Accès contrôlé aux dossiers » de Windows Defender y bloque les applications inconnues.

```
Orchestra/
  config.json                 cerveau, clés, URL de l'Agent Market
  memoire/
    orchestra.md              mémoire principale (faits datés, par section)
    agents/<id>.md            une mémoire par agent (missions, apprentissages)
    archives/AAAA-MM-JJ.md    conversations brutes (.md.zst dès le lendemain, lot M1)
    archives/agents/          missions condensées des agents (lignes complètes, lot M3)
    sessions/                 résumés de conversation (lot C), modifiables
    resumes/AAAA-MM-JJ.md     résumé de chaque journée (lot M3), modifiable
    historique.md             faits remplacés par la consolidation, avec leur date (lot M3)
    .index.bin                index du rappel : vecteurs et usage (supprimable, il se reconstruit)
  agents/
    registre-local.json       catalogue local (même format que l'Agent Market)
    installes/<id>.json       manifestes des agents installés
  espace-de-travail/          seul dossier où les agents écrivent
  journal/AAAA-MM-JJ.jsonl    chaque événement
```

## Une demande, étape par étape

1. **Rappel** : les 6 faits les plus utiles, trouvés par leurs mots et par leur sens (voir « Mémoire » ci-dessous) ; le profil (Utilisateur, Préférences) est déjà dans le prompt.
2. **Analyse** : le cerveau classe la demande (discussion · recherche · travail) et formule le besoin et la mission.
3. **Recherche d'agent** (si travail) : mémoire « Agents de confiance » → agents installés → Agent Market → registre local.
4. **Délégation** : l'agent reçoit sa mission et agit par outils (`now`, `list_dir`, `read_file`, `write_file`, `fetch_url`), un à la fois.
5. **Restitution** : Orchestra supervise et restitue le résultat.
6. **Mémorisation** : 0 à 3 faits durables dans `orchestra.md`, la mission dans la mémoire de l'agent, le lien besoin → agent dans « Agents de confiance ».

## Format d'un agent (Agent Market)

```json
{
  "id": "agent-mail",
  "name": "Agent Mail",
  "version": "1.0.0",
  "author": "…",
  "description": "Ce qu'il sait faire",
  "capabilities": ["mails", "email"],
  "model": "inherit",
  "system_prompt": "…",
  "tools": ["now", "fetch_url", "write_file"],
  "color": "#b794f4",
  "icon": "pen"
}
```

L'Agent Market doit exposer `GET <url>/agents.json` (ou l'URL directe d'un `.json`) renvoyant `[manifeste, …]` ou `{ "agents": [...] }`.

## Mémoire : le rappel (lot M2)

- **Où il cherche** (`index.rs`) : `orchestra.md` hors profil, les fiches des agents (`memoire/agents/`), les résumés de conversation (`memoire/sessions/`, `memoire/resumes/`). L'index vit en mémoire vive ; à chaque message, seuls les fichiers dont la date ou la taille a changé sont relus, modifications à la main comprises.
- **Par les mots** : BM25 sur les mots utiles et leurs radicaux de 5 lettres, sans bibliothèque. Un fait n'est retenu par les mots que s'il couvre une part de la question ou partage avec elle un mot rare : un mot banal seul (« jour », « projet ») ne suffit pas.
- **Par le sens** : modèle d'embedding `qwen3-embedding:0.6b` sur le processeur via Ollama (`ollama pull qwen3-embedding:0.6b`, une fois), contexte de 2 048 tokens (0,8 Go de mémoire vive, gardé chargé). Le vecteur d'un fait est calculé quand il est écrit (ou en tâche de fond pour une ligne modifiée à la main), jamais au rappel ; seul celui de la question l'est (75 à 100 ms). Vecteurs réduits à 256 dimensions et rangés sur un octet par valeur dans `memoire/.index.bin`. Sans le modèle, le rappel se fait par les mots seuls et Conscience le dit une fois ; installé plus tard, il est pris en compte sans relancer.
- **Classement** : fusion par rang (Σ 1/(60 + rang)), fraîcheur (demi-vie de 90 jours pour Projets et Connaissances, jamais sous 25 %), petit bonus aux faits déjà rappelés puis repris dans une réponse. La raison s'affiche dans Conscience (« mots : port, studio · sens 82 % »).
- **Voix** : même rappel ; le sens n'est ajouté que si le vecteur de la phrase arrive en 35 ms, sinon les mots seuls. Sur le processeur, il en faut 75 à 100 : en pratique, la voix se sert des mots seuls.
- **Outil `retrouver`** : résumés de conversation d'abord, puis les archives des seuls jours visés.
- **Mesure** : `cargo run --release --example mesure_rappel` (jeu `eval/rappel.jsonl`, rapport dans `Orchestra/eval/rappel-<date>.md`).

## Mémoire : la propreté (lot M3)

- **Consolidation à l'écriture** (`consolidation.rs`) : avant d'écrire un fait, ses 5 voisins dans `orchestra.md` (par les mots et par le sens). Si l'un est très proche (la moitié de ses mots et 2 au moins, ou un sens à 75 %), le cerveau décide en sortie contrainte : ajouter, remplacer la ligne N ou ignorer. Au moindre doute, ajouter. Une ligne remplacée part dans `memoire/historique.md` ; Conscience affiche « Fait mis à jour » avec un bouton Annuler.
- **Entretien à l'inactivité** (`entretien.rs`), dans le créneau de la compaction (2 min sans rien dire), abandonné dès qu'on reparle, et seulement si le cerveau est déjà chargé : résumé de chaque journée passée dans `memoire/resumes/AAAA-MM-JJ.md` (7 jours en arrière, 2 par créneau) ; au-delà de 50 missions dans la fiche d'un agent, les 25 plus anciennes condensées en 1 à 3 apprentissages, les lignes complètes rangées dans `memoire/archives/agents/`.
- **Essai** : `cargo run --release --example essai_consolidation` (8 cas avec le vrai modèle, rapport dans `Orchestra/eval/consolidation-<date>.md`).

## Mémoire : le disque (lot M1)

- **Compression à J+1** (`compression.rs`, zstd niveau 19, bibliothèque `zstd` sous MIT et BSD) : au lancement et pendant les pauses, les fichiers d'hier et d'avant sont compressés en `.zst` : archives de conversation, missions condensées des agents, journal et sa chaîne, traces des missions (échanges, actions, pensée, journaux d'erreurs). La pensée brute d'un agent l'est dès la fin de sa mission. Restent en clair : le fichier du jour, `orchestra.md`, les fiches des agents, les résumés, et les petits fichiers JSON des missions. Chaque fichier est relu et comparé avant que l'original soit retiré.
- **Lecture transparente** : vues Mémoire et Journal, outil `retrouver`, résumés du jour, vérification des chaînes (lot T6) lisent indifféremment le clair et le `.zst`. Corriger une archive compressée depuis la vue Mémoire la réécrit en clair (recompressée à la passe suivante).
- **Journal** : la réponse d'un tour n'y est plus recopiée (aperçu et référence vers l'archive).
- **Ouvrir un `.zst` hors d'Orchestra** : PeaZip, 7-Zip dans sa version zstd (« 7-Zip ZS »), ou en ligne de commande `zstd -d fichier.md.zst`.
- **Mesure** : `cargo run --release --example mesure_compression` (sur une copie des données, rapport dans `Orchestra/eval/compression-<date>.md`).

## Sécurité (lot S)

Principe : un modèle peut toujours se faire piéger ; le code d'Orchestra doit tenir quand même.

- **Ce qui vient d'ailleurs reste de la donnée** (`securite.rs`) : comptes rendus d'agents, pages, fichiers et extraits des fiches d'agents arrivent entre `<donnees>` et `</donnees>`, une fausse balise ou une fausse consigne interne étant désamorcée. Un texte qui ressemble à une consigne adressée à Orchestra est signalé dans Conscience (repérage par mots, sans rien bloquer). La mémorisation ne retient que ce que dit l'utilisateur. Après un contenu extérieur, la lecture d'une adresse qu'il n'a pas écrite attend sa confirmation (carte Oui / Non).
- **Dossier d'Orchestra surveillé pendant chaque mission** (`surveillance.rs`) : empreinte de chaque fichier avant le lancement de l'agent et après sa fin ; une modification hors de ses zones (projet de la mission, mémoire propre) est un écart, une alerte et une preuve gardée dans `integrite/preuves/`. Non surveillés : programmes et modèles de la voix, `eval/`.
- **Clés API dans le Gestionnaire d'identification de Windows** (`coffre.rs`) : `orchestra/_cles_api/<fournisseur>`, migrées depuis `config.json` au lancement, jamais réaffichées. Les variables d'environnement qui ressemblent à des secrets ne sont pas transmises aux agents.
- **Agents certifiés signés** (`signature.rs`) : le Market signe chaque entrée « Orchestra Certified » ou « livrable seul » (Ed25519, champ `certification.signature`) ; Orchestra vérifie avec la clé publique écrite dans son code. Outil : `cargo run --release --example cle_market -- creer | signer <agents.json> | verifier <agents.json>`.
- **Banc de sécurité** : `cargo run --release --example securite` (10 scénarios piégés de `eval/securite/`, 5 passages chacun), rapport dans `Orchestra/eval/securite-<date>.md`.

Limites (pas de bac à sable en V1) : un agent tourne sous le compte Windows de l'utilisateur. Il peut lire le coffre de Windows par son interface de programmation, écrire ailleurs sur le disque ou laisser tourner un programme après la mission ; la certification du Market couvre ce risque. Le repérage des consignes par mots peut rater une formulation habile ou signaler un texte innocent.

## Développer

```
npm install
npm run tauri dev      # fenêtre de dev avec rechargement à chaud
npm run tauri build    # exécutable + installateur NSIS
```

Prérequis : Node.js, Rust (rustup), Visual Studio Build Tools (C++), WebView2 (inclus dans Windows 11).

## Arborescence du code

```
src/                 interface React
  components/        Stage (cercle + agents), Conscience, Conversation, Inspector, Mémoire, Journal, Paramètres
  store.ts           état + réception des événements
src-tauri/src/       cœur Rust
  orchestrator.rs    les étapes d'une demande
  brain.rs           fournisseurs de modèles (streaming)
  memory.rs          fichiers .md, profil, mémorisation
  index.rs           index du rappel (mots, sens, fraîcheur, usage)
  consolidation.rs   ajouter, remplacer ou ignorer un nouveau fait
  entretien.rs       résumés du jour et missions condensées, à l'inactivité
  compression.rs     niveau froid compressé à J+1, lecture transparente
  agents.rs          manifestes, registre, installation
  market.rs          Agent Market (abstrait) + repli local
  tools.rs           outils des agents (cantonnés à l'espace de travail)
  events.rs          événements → interface + journal
```
