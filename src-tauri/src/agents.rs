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
    if paths.registre_local.exists() {
        return Ok(());
    }
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
    ];
    let doc = serde_json::json!({
        "_commentaire": "Registre local d'agents. Même format que l'Agent Market : un tableau de manifestes. Tu peux ajouter tes propres agents ici.",
        "agents": demo,
    });
    std::fs::write(&paths.registre_local, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}

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
