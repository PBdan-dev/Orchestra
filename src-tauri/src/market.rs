//! Agent Market : interface abstraite. L'URL est configurable ; le format attendu
//! est un JSON `[manifeste, ...]` ou `{ "agents": [...] }` (voir agents.rs).
//! Sans URL ou en cas d'échec, Orchestra pioche dans le registre local.

use std::time::Duration;

use crate::{agents::{self, Manifest}, config::Config, paths::Paths};

pub struct Found {
    pub manifest: Manifest,
    pub source: String,
    pub score: f32,
}

fn market_endpoint(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    if u.ends_with(".json") || u.contains("/api/") {
        u.to_string()
    } else {
        format!("{u}/agents.json")
    }
}

pub async fn fetch_remote(http: &reqwest::Client, cfg: &Config) -> Result<Vec<Manifest>, String> {
    if cfg.market_url.trim().is_empty() {
        return Err("aucune URL d'Agent Market configurée".into());
    }
    let url = market_endpoint(&cfg.market_url);
    let resp = http
        .get(&url)
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .map_err(|e| format!("Market injoignable ({url}) : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Market a répondu {} ({url})", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let list = agents::parse_registry(&text);
    if list.is_empty() {
        return Err("Market : aucun agent lisible dans la réponse".into());
    }
    Ok(list)
}

/// Cherche le meilleur agent pour un besoin. Renvoie aussi un petit journal
/// de la recherche (pour l'afficher dans l'interface).
pub async fn find_agent(
    http: &reqwest::Client,
    cfg: &Config,
    paths: &Paths,
    need: &str,
) -> (Option<Found>, Vec<String>) {
    let mut log = vec![];
    let mut candidates: Vec<Found> = vec![];

    // 1. Agents déjà installés
    for m in agents::installed(paths) {
        let s = agents::match_score(&m, need);
        candidates.push(Found { manifest: m, source: "installé".into(), score: s });
    }
    log.push(format!("{} agent(s) déjà installé(s) examiné(s)", candidates.len()));

    // 2. Agent Market distant
    match fetch_remote(http, cfg).await {
        Ok(list) => {
            log.push(format!("Agent Market : {} agent(s) disponibles", list.len()));
            for m in list {
                let s = agents::match_score(&m, need);
                candidates.push(Found { manifest: m, source: "Agent Market".into(), score: s });
            }
        }
        Err(e) => log.push(format!("{e} → repli sur le registre local")),
    }

    // 3. Registre local
    let local = agents::local_registry(paths);
    log.push(format!("Registre local : {} agent(s)", local.len()));
    for m in local {
        let s = agents::match_score(&m, need);
        candidates.push(Found { manifest: m, source: "registre local".into(), score: s });
    }

    // Déduplication par id (on garde le meilleur score / la source la plus proche)
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.manifest.id.clone()));

    for c in candidates.iter().take(4) {
        log.push(format!("  {} ({}) : score {:.1}", c.manifest.name, c.source, c.score));
    }

    let best = candidates.into_iter().next().filter(|c| c.score >= 1.0);
    (best, log)
}
