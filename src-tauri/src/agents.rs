//! Agents : manifeste (format attendu de l'Agent Market), registre local et installation.
//!
//! Un agent est un tout-en-un : un modèle (ou "inherit" pour celui d'Orchestra),
//! un prompt système et une liste d'outils. Orchestra l'active, lui donne sa
//! mission, supervise et reçoit le résultat.

use serde::{Deserialize, Serialize};

use crate::paths::Paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// Mots-clés de ce que l'agent sait faire ("mails", "fichiers", "rédaction"...)
    pub capabilities: Vec<String>,
    /// "inherit" = le cerveau d'Orchestra, sinon "provider:modèle" (ex: "ollama:qwen3.5:9b")
    pub model: String,
    pub system_prompt: String,
    /// Outils autorisés : now, list_dir, read_file, write_file, fetch_url
    pub tools: Vec<String>,
    /// Couleur d'accent pour l'interface (optionnelle)
    pub color: String,
    pub icon: String,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            id: String::new(),
            name: String::new(),
            version: "1.0.0".into(),
            author: String::new(),
            description: String::new(),
            capabilities: vec![],
            model: "inherit".into(),
            system_prompt: String::new(),
            tools: vec![],
            color: String::new(),
            icon: String::new(),
        }
    }
}

/// Registre local d'exemple, écrit au premier lancement si absent.
pub fn seed_registry(paths: &Paths) -> anyhow::Result<()> {
    let demo = vec![
        Manifest {
            id: "agent-fichiers".into(),
            name: "Agent Fichiers".into(),
            version: "1.0.0".into(),
            author: "Orchestra (démo)".into(),
            description: "Organise, lit, crée et range des fichiers dans l'espace de travail d'Orchestra. Fait l'inventaire d'un dossier, renomme, classe, résume des documents texte.".into(),
            capabilities: vec![
                "fichiers".into(), "dossier".into(), "documents".into(), "ranger".into(),
                "organiser".into(), "lire".into(), "inventaire".into(), "classer".into(),
            ],
            model: "inherit".into(),
            system_prompt: "Tu es l'Agent Fichiers. Tu travailles uniquement dans l'espace de travail qui t'est donné. Tu procèdes par étapes : tu inspectes (list_dir, read_file) avant d'agir (write_file). Tu expliques brièvement ce que tu as fait et tu listes les fichiers touchés. Réponds en français.".into(),
            tools: vec!["now".into(), "list_dir".into(), "read_file".into(), "write_file".into()],
            color: "#4fd1c5".into(),
            icon: "folder".into(),
        },
        Manifest {
            id: "agent-redacteur".into(),
            name: "Agent Rédacteur".into(),
            version: "1.0.0".into(),
            author: "Orchestra (démo)".into(),
            description: "Rédige des textes structurés : mails, comptes rendus, notes, articles, plans. Peut s'appuyer sur une page web donnée et sauvegarder le résultat en Markdown.".into(),
            capabilities: vec![
                "rédaction".into(), "rediger".into(), "écrire".into(), "ecrire".into(), "texte".into(),
                "mail".into(), "mails".into(), "email".into(), "compte-rendu".into(), "article".into(),
                "note".into(), "résumé".into(), "resume".into(), "plan".into(),
            ],
            model: "inherit".into(),
            system_prompt: "Tu es l'Agent Rédacteur. Tu produis des textes clairs, structurés et prêts à l'emploi, en français sauf demande contraire. Si une URL est fournie, tu la lis avec fetch_url avant d'écrire. Quand la mission demande un fichier, tu sauvegardes avec write_file (format .md) et tu indiques son nom. Termine toujours par le texte final complet.".into(),
            tools: vec!["now".into(), "fetch_url".into(), "write_file".into(), "read_file".into()],
            color: "#b794f4".into(),
            icon: "pen".into(),
        },
        Manifest {
            id: "agent-veille".into(),
            name: "Agent Veille Web".into(),
            version: "1.0.0".into(),
            author: "Orchestra (démo)".into(),
            description: "Lit des pages web et en tire une synthèse sourcée : veille, résumé d'article, comparaison de pages. Sauvegarde la synthèse en Markdown.".into(),
            capabilities: vec![
                "web".into(), "veille".into(), "page".into(), "site".into(), "article".into(),
                "url".into(), "internet".into(), "synthese".into(), "synthèse".into(), "lien".into(),
            ],
            model: "inherit".into(),
            system_prompt: "Tu es l'Agent Veille Web. Pour chaque URL de la mission, tu la lis avec fetch_url, puis tu rédiges une synthèse claire avec les points clés et la source. Si on te le demande, tu enregistres la synthèse avec write_file. Réponds en français.".into(),
            tools: vec!["fetch_url".into(), "write_file".into(), "now".into()],
            color: "#60a5fa".into(),
            icon: "globe".into(),
        },
        Manifest {
            id: "agent-planning".into(),
            name: "Agent Planning".into(),
            version: "1.0.0".into(),
            author: "Orchestra (démo)".into(),
            description: "Organise le temps : transforme une liste de tâches en planning daté, découpe un projet en étapes, prépare un ordre du jour. Écrit le planning dans un fichier.".into(),
            capabilities: vec![
                "planning".into(), "planifier".into(), "agenda".into(), "calendrier".into(), "organiser".into(),
                "semaine".into(), "reunion".into(), "réunion".into(), "taches".into(), "tâches".into(), "projet".into(),
            ],
            model: "inherit".into(),
            system_prompt: "Tu es l'Agent Planning. Tu récupères la date du jour avec now, puis tu construis un planning réaliste (tableau Markdown : date, créneau, tâche, durée). Tu l'enregistres avec write_file si la mission le demande. Réponds en français.".into(),
            tools: vec!["now".into(), "write_file".into(), "read_file".into()],
            color: "#fbbf24".into(),
            icon: "calendar".into(),
        },
        Manifest {
            id: "agent-mail".into(),
            name: "Agent Mail (exemple)".into(),
            version: "0.1.0".into(),
            author: "Orchestra (démo)".into(),
            description: "Exemple d'agent tel qu'il pourrait venir de l'Agent Market : trier, résumer et rédiger des mails. Il n'a pas encore d'accès à une vraie boîte mail ; il rédige des brouillons dans l'espace de travail.".into(),
            capabilities: vec![
                "mail".into(), "mails".into(), "email".into(), "courriel".into(), "boite".into(),
                "brouillon".into(), "repondre".into(), "répondre".into(), "trier".into(),
            ],
            model: "inherit".into(),
            system_prompt: "Tu es l'Agent Mail (version d'exemple). Tu n'as pas accès à une boîte mail réelle : dis-le clairement si la mission l'exige. Tu peux en revanche rédiger des réponses ou des brouillons de mails et les enregistrer avec write_file (dossier mails/). Réponds en français.".into(),
            tools: vec!["write_file".into(), "now".into()],
            color: "#f472b6".into(),
            icon: "mail".into(),
        },
    ];

    // Premier lancement : on écrit le registre complet.
    if !paths.registre_local.exists() {
        let doc = serde_json::json!({
            "_commentaire": "Registre local d'agents. Même format que l'Agent Market : un tableau de manifestes. Tu peux ajouter, modifier ou supprimer des agents ici.",
            "_seed": SEED_VERSION,
            "agents": demo,
        });
        std::fs::write(&paths.registre_local, serde_json::to_string_pretty(&doc)?)?;
        return Ok(());
    }

    // Registre existant : on ajoute une seule fois les nouveaux agents d'exemple, sans toucher au reste.
    let text = std::fs::read_to_string(&paths.registre_local)?;
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "agents": [] }));
    if v.is_array() {
        v = serde_json::json!({ "agents": v });
    }
    if v.get("_seed").and_then(|s| s.as_u64()).unwrap_or(1) >= SEED_VERSION {
        return Ok(());
    }
    let existing: Vec<String> = parse_registry(&text).into_iter().map(|m| m.id).collect();
    if let Some(arr) = v.get_mut("agents").and_then(|a| a.as_array_mut()) {
        for m in demo.into_iter().filter(|m| !existing.contains(&m.id)) {
            arr.push(serde_json::to_value(m)?);
        }
    }
    v["_seed"] = serde_json::json!(SEED_VERSION);
    std::fs::write(&paths.registre_local, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

const SEED_VERSION: u64 = 2;

pub fn parse_registry(text: &str) -> Vec<Manifest> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        let arr = if v.is_array() {
            v.clone()
        } else {
            v.get("agents").cloned().unwrap_or(serde_json::Value::Array(vec![]))
        };
        if let Ok(list) = serde_json::from_value::<Vec<Manifest>>(arr) {
            return list.into_iter().filter(|m| !m.id.is_empty()).collect();
        }
    }
    vec![]
}

pub fn local_registry(paths: &Paths) -> Vec<Manifest> {
    std::fs::read_to_string(&paths.registre_local)
        .map(|s| parse_registry(&s))
        .unwrap_or_default()
}

pub fn installed(paths: &Paths) -> Vec<Manifest> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&paths.installes) {
        for e in rd.flatten() {
            if let Ok(s) = std::fs::read_to_string(e.path()) {
                if let Ok(m) = serde_json::from_str::<Manifest>(&s) {
                    out.push(m);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn get_installed(paths: &Paths, id: &str) -> Option<Manifest> {
    std::fs::read_to_string(paths.installed_manifest(id))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn install(paths: &Paths, m: &Manifest) -> anyhow::Result<()> {
    std::fs::write(paths.installed_manifest(&m.id), serde_json::to_string_pretty(m)?)?;
    Ok(())
}

/// Score de correspondance entre un besoin exprimé et un agent (mots communs).
pub fn match_score(m: &Manifest, need: &str) -> f32 {
    let need_words = crate::memory::keywords(need);
    if need_words.is_empty() {
        return 0.0;
    }
    let mut hay = m.capabilities.join(" ");
    hay.push(' ');
    hay.push_str(&m.name);
    hay.push(' ');
    hay.push_str(&m.description);
    let hay_words = crate::memory::keywords(&hay);
    let mut score = 0.0;
    for w in &need_words {
        if m.capabilities.iter().any(|c| crate::memory::fold(c) == *w) {
            score += 2.0;
        } else if hay_words.contains(w) {
            score += 1.0;
        } else if hay_words.iter().any(|h| h.starts_with(&w[..w.len().min(5)]) || w.starts_with(&h[..h.len().min(5)])) {
            score += 0.5;
        }
    }
    score
}
