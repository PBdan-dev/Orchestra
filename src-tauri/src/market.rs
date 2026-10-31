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

/// Agent Market officiel (D19), utilisé quand aucune adresse n'est configurée.
pub const MARKET_OFFICIEL: &str = "https://pbdan-dev.github.io/Agent_Market/";

/// Lit le catalogue : adresse https du Market, ou chemin d'un fichier `agents.json` local
/// (essais). Une entrée invalide est ignorée avec son motif (lot T4). Renvoie aussi la source.
pub async fn fetch_catalogue(http: &reqwest::Client, cfg: &Config) -> Result<(agents::Lecture, String), String> {
    let brut = if cfg.market_url.trim().is_empty() { MARKET_OFFICIEL } else { cfg.market_url.trim() };
    if !brut.starts_with("http://") && !brut.starts_with("https://") {
        let chemin = std::path::Path::new(brut.trim_start_matches("file:///"));
        let text = std::fs::read_to_string(chemin).map_err(|e| format!("catalogue local illisible ({}) : {e}", chemin.display()))?;
        return Ok((agents::parse_registry(&text), chemin.display().to_string()));
    }
    let url = market_endpoint(brut);
    let resp = http
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Market injoignable ({url}) : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Market a répondu {} ({url})", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok((agents::parse_registry(&text), url))
}

pub async fn fetch_remote(http: &reqwest::Client, cfg: &Config) -> Result<Vec<Manifest>, String> {
    let (l, _) = fetch_catalogue(http, cfg).await?;
    if l.agents.is_empty() {
        let motifs = if l.ignores.is_empty() { String::new() } else { format!(" ({})", l.ignores.join(" ; ")) };
        return Err(format!("Market : aucun agent lisible dans la réponse{motifs}"));
    }
    Ok(l.agents)
}

/// Cherche le meilleur agent pour un besoin, parmi les agents installés (les seuls qu'Orchestra
/// sait lancer). Le Market et le registre local sont consultés pour information : l'installation
/// depuis le Market arrive au lot T4. Renvoie aussi le journal de la recherche, affiché dans Conscience.
pub async fn find_agent(
    http: &reqwest::Client,
    cfg: &Config,
    paths: &Paths,
    need: &str,
) -> (Option<Found>, Vec<String>) {
    let mut log = vec![];
    let lecture = agents::installes(paths);
    for i in &lecture.ignores {
        log.push(format!("⚠ agent installé inutilisable : {i}"));
    }
    // T5 : un agent en quarantaine n'est jamais choisi
    for m in lecture.agents.iter().filter(|m| !crate::verification::utilisable(paths, &m.id)) {
        log.push(format!("{} est en quarantaine : écarté (à lever dans le panneau Agents)", m.name));
    }
    let mut candidates: Vec<Found> = lecture
        .agents
        .into_iter()
        .filter(|m| crate::verification::utilisable(paths, &m.id))
        .map(|m| {
            let score = agents::match_score(&m, need);
            Found { manifest: m, source: "installé".into(), score }
        })
        .collect();
    log.push(format!("{} agent(s) installé(s) examiné(s)", candidates.len()));

    // Agent Market : Orchestra n'installe jamais seul ; il signale les agents à installer (panneau Agents)
    match fetch_remote(http, cfg).await {
        Ok(list) => {
            let mieux: Vec<String> = list
                .iter()
                .filter(|m| !candidates.iter().any(|c| c.manifest.id == m.id) && agents::match_score(m, need) >= 1.0)
                .filter(|m| cfg.mode_developpement || m.certification == "orchestra_certified")
                .map(|m| m.name.clone())
                .collect();
            log.push(format!("Agent Market : {} agent(s) au catalogue", list.len()));
            if !mieux.is_empty() {
                log.push(format!("  à installer depuis le panneau Agents pour ce besoin : {}", mieux.join(", ")));
            }
        }
        Err(e) => log.push(e),
    }
    let local = agents::local_registry(paths);
    for i in &local.ignores {
        log.push(format!("registre local, entrée ignorée : {i}"));
    }

    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    for c in candidates.iter().take(4) {
        log.push(format!("  {} ({}) : score {:.1}", c.manifest.name, c.source, c.score));
    }
    let best = candidates.into_iter().next().filter(|c| c.score >= 1.0);
    (best, log)
}
