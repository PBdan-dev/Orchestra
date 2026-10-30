//! Modèle d'une mission et carte graphique (D24, lot T2).
//!
//! - `inherit` : Orchestra prête son modèle s'il fait partie des modèles admis par l'agent
//!   (`modele.minimum`, `modele.recommande`) ; sinon il charge le modèle recommandé.
//! - Autre modèle : Orchestra décharge le sien (et arrête la voix si on le lui demande), charge
//!   celui de l'agent, le sert pendant la mission, puis le décharge et recharge le sien.
//! - Jamais en dessous du minimum, jamais de bascule silencieuse sur le processeur : un modèle qui ne
//!   tient pas entièrement sur la carte graphique est refusé avec motif.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{agents::Manifest, brain, config::Config};

/// Modèle à servir à l'agent ; `None` si l'agent n'utilise pas de modèle.
pub fn choisir(m: &Manifest, actif: &str) -> Option<String> {
    let s = |p: &str| m.brut.pointer(p).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (demande, minimum, recommande) = (s("/modele/demande"), s("/modele/minimum"), s("/modele/recommande"));
    let absent = |x: &str| x.is_empty() || x == "aucun";
    match demande.as_str() {
        d if absent(d) => None,
        "inherit" => {
            let admis = [minimum.as_str(), recommande.as_str()];
            if !actif.is_empty() && (admis.contains(&actif) || (absent(&minimum) && absent(&recommande))) {
                Some(actif.to_string())
            } else if !absent(&recommande) {
                Some(recommande)
            } else if !absent(&minimum) {
                Some(minimum)
            } else {
                Some(actif.to_string())
            }
        }
        autre => Some(autre.to_string()),
    }
}

/// Un changement de modèle fait pour la mission, à défaire ensuite.
#[derive(Debug, Clone)]
pub struct Changement {
    pub modele_agent: String,
    pub modele_orchestra: String,
    pub duree: Duration,
}

async fn decharger(cfg: &Config, http: &reqwest::Client, modele: &str) {
    if modele.is_empty() {
        return;
    }
    let base = cfg.ollama_url.trim_end_matches('/');
    let _ = http.post(format!("{base}/api/generate")).timeout(Duration::from_secs(30)).json(&json!({ "model": modele, "keep_alive": 0 })).send().await;
}

async fn charger(cfg: &Config, http: &reqwest::Client, modele: &str) -> Result<(), String> {
    let base = cfg.ollama_url.trim_end_matches('/');
    let r = http
        .post(format!("{base}/api/generate"))
        .timeout(Duration::from_secs(900))
        .json(&json!({ "model": modele, "prompt": "", "stream": false, "keep_alive": brain::KEEP_ALIVE, "options": { "num_ctx": cfg.num_ctx } }))
        .send()
        .await
        .map_err(|e| format!("chargement de {modele} impossible : {e}"))?;
    if !r.status().is_success() {
        let corps = r.text().await.unwrap_or_default();
        return Err(format!("chargement de {modele} refusé par Ollama : {}", corps.chars().take(200).collect::<String>()));
    }
    Ok(())
}

/// Part du modèle chargée sur la carte graphique (1,0 = entièrement), d'après `/api/ps`.
pub async fn part_sur_gpu(cfg: &Config, http: &reqwest::Client, modele: &str) -> Option<f64> {
    let base = cfg.ollama_url.trim_end_matches('/');
    let v: Value = http.get(format!("{base}/api/ps")).timeout(Duration::from_secs(8)).send().await.ok()?.json().await.ok()?;
    let m = v["models"].as_array()?.iter().find(|m| m["name"].as_str() == Some(modele) || m["model"].as_str() == Some(modele))?;
    let (taille, vram) = (m["size"].as_f64()?, m["size_vram"].as_f64()?);
    Some(if taille > 0.0 { vram / taille } else { 1.0 })
}

/// Prépare le modèle de la mission. `Ok(None)` : rien à changer (modèle d'Orchestra prêté).
pub async fn preparer(cfg: &Config, http: &reqwest::Client, modele_agent: &str, modele_orchestra: &str, arreter_voix: Option<&(dyn Fn() + Send + Sync)>) -> Result<Option<Changement>, String> {
    if cfg.provider != "ollama" || modele_agent == modele_orchestra {
        return Ok(None);
    }
    let debut = Instant::now();
    decharger(cfg, http, modele_orchestra).await;
    if let Some(stop) = arreter_voix {
        stop();
    }
    charger(cfg, http, modele_agent).await?;
    if let Some(p) = part_sur_gpu(cfg, http, modele_agent).await {
        if p < 0.98 {
            decharger(cfg, http, modele_agent).await;
            let _ = charger(cfg, http, modele_orchestra).await;
            return Err(format!(
                "{modele_agent} ne tient pas sur la carte graphique ({:.0} % seulement) : refusé, pour ne pas basculer sur le processeur",
                p * 100.0
            ));
        }
    }
    Ok(Some(Changement { modele_agent: modele_agent.into(), modele_orchestra: modele_orchestra.into(), duree: debut.elapsed() }))
}

/// Après la mission : décharge le modèle de l'agent et recharge celui d'Orchestra.
pub async fn restaurer(cfg: &Config, http: &reqwest::Client, c: &Changement) -> Result<Duration, String> {
    let debut = Instant::now();
    decharger(cfg, http, &c.modele_agent).await;
    charger(cfg, http, &c.modele_orchestra).await?;
    Ok(debut.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(demande: &str, minimum: &str, recommande: &str) -> Manifest {
        Manifest { brut: json!({ "modele": { "demande": demande, "minimum": minimum, "recommande": recommande } }), ..Default::default() }
    }

    #[test]
    fn choix_du_modele() {
        assert_eq!(choisir(&agent("aucun", "aucun", "aucun"), "qwen3.6:27b"), None);
        // modèle d'Orchestra admis : prêté
        assert_eq!(choisir(&agent("inherit", "qwen3.5:9b", "qwen3.6:27b"), "qwen3.6:27b").as_deref(), Some("qwen3.6:27b"));
        assert_eq!(choisir(&agent("inherit", "qwen3.5:9b", "qwen3.6:27b"), "qwen3.5:9b").as_deref(), Some("qwen3.5:9b"));
        // modèle d'Orchestra non admis : le recommandé
        assert_eq!(choisir(&agent("inherit", "qwen3.5:9b", "qwen3.6:27b"), "llama3:8b").as_deref(), Some("qwen3.6:27b"));
        assert_eq!(choisir(&agent("mistral:7b", "", ""), "qwen3.6:27b").as_deref(), Some("mistral:7b"));
    }
}
