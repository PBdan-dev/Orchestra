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

## Données (en clair, dans `Documents\Orchestra`)

```
Documents/Orchestra/
  config.json                 cerveau, clés, URL de l'Agent Market
  memoire/
    orchestra.md              mémoire principale (faits datés, par section)
    agents/<id>.md            une mémoire par agent (missions, apprentissages)
    archives/AAAA-MM-JJ.md    conversations brutes
    .index-vectoriel.json     cache d'embeddings (optionnel)
  agents/
    registre-local.json       catalogue local (même format que l'Agent Market)
    installes/<id>.json       manifestes des agents installés
  espace-de-travail/          seul dossier où les agents écrivent
  journal/AAAA-MM-JJ.jsonl    chaque événement
```

## Une demande, étape par étape

1. **Rappel** : les faits de la mémoire qui partagent des mots avec la demande (et, si activé, les plus proches par embeddings).
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
  memory.rs          fichiers .md, rappel, mémorisation
  agents.rs          manifestes, registre, installation
  market.rs          Agent Market (abstrait) + repli local
  tools.rs           outils des agents (cantonnés à l'espace de travail)
  events.rs          événements → interface + journal
```
