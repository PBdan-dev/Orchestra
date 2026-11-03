//! Ce que le cerveau d'Orchestra sait de ses agents : ceux qui sont installés et ceux du catalogue
//! de l'Agent Market (retour de PB du 03/11 : pour « trie mes 100 derniers mails », Orchestra n'avait
//! évoqué aucun agent, faute de connaître la liste).
//!
//! Le bloc « Agents disponibles » va dans le message système (stable d'un tour à l'autre tant que la
//! liste ne change pas, pour garder le cache). Les fiches viennent d'auteurs tiers : elles sont
//! encadrées comme de la donnée (lot S-a).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{agents::{self, Manifest}, config::Config, market, paths::Paths};

/// Catalogue gardé en mémoire 10 minutes : le Market ne change pas d'un tour à l'autre.
static CATALOGUE: Mutex<Option<(Instant, Vec<Manifest>)>> = Mutex::new(None);
const DUREE_CATALOGUE: Duration = Duration::from_secs(600);
const DELAI_MARKET: Duration = Duration::from_secs(6);

/// Catalogue de l'Agent Market (agents lisibles), depuis le cache s'il est récent.
pub async fn catalogue(http: &reqwest::Client, cfg: &Config) -> Result<Vec<Manifest>, String> {
    if let Some((t, l)) = CATALOGUE.lock().unwrap().as_ref() {
        if t.elapsed() < DUREE_CATALOGUE {
            return Ok(l.clone());
        }
    }
    let l = tokio::time::timeout(DELAI_MARKET, market::fetch_remote(http, cfg))
        .await
        .map_err(|_| format!("l'Agent Market n'a pas répondu en {} s", DELAI_MARKET.as_secs()))??;
    *CATALOGUE.lock().unwrap() = Some((Instant::now(), l.clone()));
    Ok(l)
}

/// Le catalogue vient d'être relu par l'interface : on garde cette lecture.
pub fn retenir_catalogue(l: &[Manifest]) {
    *CATALOGUE.lock().unwrap() = Some((Instant::now(), l.to_vec()));
}

#[derive(Default, Clone, Debug)]
pub struct Connaissance {
    /// agents installés et utilisables (hors quarantaine)
    pub installes: Vec<Manifest>,
    /// agents du Market qu'Orchestra peut proposer d'installer (certifiés, ou tous en mode développement)
    pub market: Vec<Manifest>,
    /// le Market n'a pas pu être lu
    pub market_erreur: Option<String>,
}

pub async fn connaitre(http: &reqwest::Client, cfg: &Config, paths: &Paths) -> Connaissance {
    let installes: Vec<Manifest> = agents::installes(paths).agents.into_iter().filter(|m| crate::verification::utilisable(paths, &m.id)).collect();
    let (market, market_erreur) = match catalogue(http, cfg).await {
        Ok(l) => (
            l.into_iter()
                .filter(|m| !installes.iter().any(|i| i.id == m.id))
                .filter(|m| cfg.mode_developpement || m.certification == "orchestra_certified")
                .collect(),
            None,
        ),
        Err(e) => (vec![], Some(e)),
    };
    Connaissance { installes, market, market_erreur }
}

/// Une ligne par agent : identifiant, nom, description et jusqu'à 3 choses qu'il sait faire.
fn ligne(m: &Manifest) -> String {
    let court = |t: &str, n: usize| if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t.to_string() };
    let mut l = format!("- {} · {} : {}", m.id, m.name, court(&m.description, 220));
    if !m.sait_faire.is_empty() {
        let sf: Vec<String> = m.sait_faire.iter().take(3).map(|s| court(s, 140)).collect();
        l.push_str(&format!(" Sait faire : {}.", sf.join(" ; ")));
    }
    l
}

impl Connaissance {
    /// Change quand un agent est installé, retiré ou mis à jour, ou quand le catalogue change.
    pub fn cle(&self) -> String {
        let ids = |l: &[Manifest]| l.iter().map(|m| format!("{}@{}", m.id, m.version)).collect::<Vec<_>>().join(",");
        format!("{}|{}|{}", ids(&self.installes), ids(&self.market), self.market_erreur.is_some())
    }

    /// Bloc du message système.
    pub fn bloc(&self) -> String {
        let liste = |l: &[Manifest]| crate::securite::encadrer(&l.iter().map(ligne).collect::<Vec<_>>().join("\n"));
        let mut s = String::from("## Agents disponibles\n");
        if self.installes.is_empty() {
            s.push_str("Installés : aucun pour l'instant.\n");
        } else {
            s.push_str("Installés (tu peux leur confier une mission avec deleguer, en donnant leur identifiant dans « agent ») :\n");
            s.push_str(&liste(&self.installes));
            s.push('\n');
        }
        match &self.market_erreur {
            Some(e) => s.push_str(&format!("Agent Market : catalogue illisible pour l'instant ({e}).\n")),
            None if self.market.is_empty() => s.push_str("Agent Market : aucun autre agent proposé pour l'instant.\n"),
            None => {
                s.push_str("Sur l'Agent Market, pas encore installés (l'utilisateur doit d'abord les installer ; appelle quand même deleguer avec leur identifiant, Orchestra lui proposera de l'installer d'un clic) :\n");
                s.push_str(&liste(&self.market));
                s.push('\n');
            }
        }
        s.trim_end().to_string()
    }

    pub fn installe(&self, id: &str) -> Option<&Manifest> {
        self.installes.iter().find(|m| m.id == id)
    }

    pub fn au_market(&self, id: &str) -> Option<&Manifest> {
        self.market.iter().find(|m| m.id == id)
    }

    /// Meilleur agent du Market (non installé) pour un besoin, s'il correspond assez.
    pub fn meilleur_au_market(&self, besoin: &str) -> Option<&Manifest> {
        self.market
            .iter()
            .map(|m| (m, agents::match_score(m, besoin)))
            .filter(|(_, s)| *s >= 1.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(m, _)| m)
    }
}

/// Ce qu'Orchestra a déjà fait avec un agent, pour l'onglet Agents.
#[derive(Serialize, Default, Clone, Debug)]
pub struct Usage {
    pub agent_id: String,
    /// missions confiées (dossier missions/)
    pub missions: u32,
    pub reussies: u32,
    /// date de la dernière mission (RFC 3339)
    pub derniere: Option<String>,
    /// date d'installation et provenance (installation.json)
    pub installe_le: Option<String>,
    /// « market » (paquet téléchargé depuis l'Agent Market) ou « local » (paquet ou dossier local)
    pub provenance: Option<String>,
}

/// Usage de tous les agents connus : missions passées et trace d'installation.
pub fn usages(paths: &Paths) -> Vec<Usage> {
    use std::collections::BTreeMap;
    let mut u: BTreeMap<String, Usage> = BTreeMap::new();
    for e in std::fs::read_dir(&paths.missions).into_iter().flatten().flatten() {
        let d = e.path();
        let Some(m) = std::fs::read_to_string(d.join("mission.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
        let Some(id) = m["agent_id"].as_str() else { continue };
        let x = u.entry(id.to_string()).or_insert_with(|| Usage { agent_id: id.to_string(), ..Default::default() });
        x.missions += 1;
        let etat = std::fs::read_to_string(d.join("compte_rendu.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|c| c["etat"].as_str().map(String::from));
        if etat.as_deref() == Some("termine") {
            x.reussies += 1;
        }
        if let Some(c) = m["cree_le"].as_str() {
            if x.derniere.as_deref().is_none_or(|d| d < c) {
                x.derniere = Some(c.to_string());
            }
        }
    }
    for e in std::fs::read_dir(&paths.installes).into_iter().flatten().flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        let trace = std::fs::read_to_string(e.path().join("installation.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let x = u.entry(id.clone()).or_insert_with(|| Usage { agent_id: id.clone(), ..Default::default() });
        match trace {
            Some(t) => {
                x.installe_le = t["installe_le"].as_str().map(String::from);
                let src = t["source"].as_str().or(t["paquet"].as_str()).unwrap_or("");
                x.provenance = Some(if src.starts_with("http") { "market" } else { "local" }.into());
            }
            // copié à la main dans installes/ (agents de test des lots T)
            None => x.provenance = Some("local".into()),
        }
    }
    u.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, desc: &str, cert: &str) -> Manifest {
        Manifest { id: id.into(), name: id.into(), version: "1.0.0".into(), description: desc.into(), certification: cert.into(), capabilities: vec!["mail".into()], ..Default::default() }
    }

    #[test]
    fn bloc_liste_installes_et_market() {
        let c = Connaissance { installes: vec![agent("agent-a", "Rédige des mails", "orchestra_certified")], market: vec![agent("agent-b", "Trie Gmail", "orchestra_certified")], market_erreur: None };
        let b = c.bloc();
        assert!(b.contains("agent-a · agent-a : Rédige des mails"));
        assert!(b.contains("pas encore installés") && b.contains("agent-b"));
        assert!(b.contains("<donnees>"));
        assert_eq!(c.au_market("agent-b").map(|m| m.id.as_str()), Some("agent-b"));
        assert!(c.installe("agent-b").is_none());
        assert_ne!(c.cle(), Connaissance::default().cle());
    }

    #[test]
    fn market_injoignable_dit_pourquoi() {
        let c = Connaissance { market_erreur: Some("pas de réponse".into()), ..Default::default() };
        assert!(c.bloc().contains("aucun pour l'instant") && c.bloc().contains("pas de réponse"));
    }
}
