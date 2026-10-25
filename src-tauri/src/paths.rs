//! Arborescence des données d'Orchestra (dans Documents\Orchestra).
//!
//! Documents/Orchestra/
//!   config.json              paramètres (cerveau, clés, URL du market)
//!   memoire/
//!     orchestra.md           mémoire principale, lisible et modifiable
//!     agents/<id>.md         une mémoire par agent utilisé
//!     archives/AAAA-MM-JJ.md conversations brutes, par jour
//!   agents/
//!     registre-local.json    catalogue local d'agents (secours du Market)
//!     installes/<id>.json    manifestes des agents installés
//!   espace-de-travail/       seul dossier où les agents peuvent écrire
//!   journal/AAAA-MM-JJ.jsonl tout ce qui s'est passé, événement par événement

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub memoire: PathBuf,
    pub memoire_agents: PathBuf,
    pub archives: PathBuf,
    pub agents: PathBuf,
    pub registre_local: PathBuf,
    pub installes: PathBuf,
    pub workspace: PathBuf,
    pub journal: PathBuf,
}

impl Paths {
    pub fn init() -> anyhow::Result<Self> {
        let docs = dirs::document_dir()
            .or_else(dirs::home_dir)
            .ok_or_else(|| anyhow::anyhow!("dossier Documents introuvable"))?;
        let root = docs.join("Orchestra");
        let p = Paths {
            config: root.join("config.json"),
            memoire: root.join("memoire"),
            memoire_agents: root.join("memoire").join("agents"),
            archives: root.join("memoire").join("archives"),
            agents: root.join("agents"),
            registre_local: root.join("agents").join("registre-local.json"),
            installes: root.join("agents").join("installes"),
            workspace: root.join("espace-de-travail"),
            journal: root.join("journal"),
            root,
        };
        for d in [
            &p.root,
            &p.memoire,
            &p.memoire_agents,
            &p.archives,
            &p.agents,
            &p.installes,
            &p.workspace,
            &p.journal,
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(p)
    }

    pub fn main_memory(&self) -> PathBuf {
        self.memoire.join("orchestra.md")
    }

    pub fn agent_memory(&self, id: &str) -> PathBuf {
        self.memoire_agents.join(format!("{}.md", safe_name(id)))
    }

    pub fn installed_manifest(&self, id: &str) -> PathBuf {
        self.installes.join(format!("{}.json", safe_name(id)))
    }
}

pub fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}
