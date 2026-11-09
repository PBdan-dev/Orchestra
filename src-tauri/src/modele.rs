//! Modèle d'une mission et carte graphique (D24, lot T2).
//!
//! - `inherit` : avec un cerveau en ligne (Claude, Gemini, OpenRouter), l'agent hérite de ce cerveau
//!   (retour de PB du 09/11 : avec Haiku actif, l'agent chargeait Qwen). Avec un cerveau local,
//!   Orchestra prête son modèle s'il fait partie des modèles admis par l'agent (`modele.minimum`,
//!   `modele.recommande`) ; sinon il charge le modèle recommandé.
//! - Autre modèle : Orchestra décharge le sien (et arrête la voix si on le lui demande), charge
//!   celui de l'agent, le sert pendant la mission, puis le décharge et recharge le sien.
//! - Jamais en dessous du minimum, jamais de bascule silencieuse sur le processeur : un modèle qui ne
//!   tient pas entièrement sur la carte graphique est refusé avec motif.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{agents::Manifest, brain, config::Config};

/// Le cerveau d'Orchestra est-il un service en ligne (pas de modèle sur la carte graphique) ?
pub fn cerveau_en_ligne(cfg: &Config) -> bool {
    matches!(cfg.provider.as_str(), "claude" | "gemini" | "openrouter")
}

/// Modèle à servir à l'agent selon le cerveau actif : `inherit` hérite d'un cerveau en ligne tel quel.
pub fn choisir_selon(m: &Manifest, cfg: &Config, actif: &str) -> Option<String> {
    let demande = m.brut.pointer("/modele/demande").and_then(|v| v.as_str()).unwrap_or("");
    if demande == "inherit" && cerveau_en_ligne(cfg) && !actif.is_empty() {
        return Some(actif.to_string());
    }
    choisir(m, actif)
}

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
    /// mode parallèle : le modèle d'Orchestra est resté chargé, seul celui de l'agent est à décharger
    pub parallele: bool,
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
    if modele_agent == modele_orchestra || (cfg.provider != "ollama" && !cerveau_en_ligne(cfg)) {
        return Ok(None);
    }
    let debut = Instant::now();
    // cerveau en ligne : rien à décharger, le modèle local de l'agent est chargé dans Ollama
    if !cerveau_en_ligne(cfg) {
        decharger(cfg, http, modele_orchestra).await;
    }
    if let Some(stop) = arreter_voix {
        stop();
    }
    if let Err(e) = charger(cfg, http, modele_agent).await {
        // modèle absent ou refusé (lot S, scénario S7) : celui d'Orchestra, déchargé juste avant, est rechargé
        let _ = charger(cfg, http, modele_orchestra).await;
        return Err(e);
    }
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
    Ok(Some(Changement { modele_agent: modele_agent.into(), modele_orchestra: modele_orchestra.into(), duree: debut.elapsed(), parallele: false }))
}

/// Mode parallèle : le modèle de l'agent est chargé à côté de celui d'Orchestra (et de la voix),
/// qui restent en place. Refusé s'il ne tient pas entièrement sur la carte graphique.
pub async fn preparer_parallele(cfg: &Config, http: &reqwest::Client, modele_agent: &str, modele_orchestra: &str) -> Result<Option<Changement>, String> {
    if modele_agent == modele_orchestra || (cfg.provider != "ollama" && !cerveau_en_ligne(cfg)) {
        return Ok(None);
    }
    let debut = Instant::now();
    charger(cfg, http, modele_agent).await?;
    if let Some(p) = part_sur_gpu(cfg, http, modele_agent).await {
        if p < 0.98 {
            decharger(cfg, http, modele_agent).await;
            return Err(format!("en parallèle, {modele_agent} ne tient pas à côté du modèle de la conversation ({:.0} % sur la carte graphique) : passe en mode série pour cette mission", p * 100.0));
        }
    }
    Ok(Some(Changement { modele_agent: modele_agent.into(), modele_orchestra: modele_orchestra.into(), duree: debut.elapsed(), parallele: true }))
}

/// Taille des modèles installés dans Ollama (octets), d'après `/api/tags`.
pub async fn tailles(cfg: &Config, http: &reqwest::Client) -> std::collections::HashMap<String, u64> {
    let base = cfg.ollama_url.trim_end_matches('/');
    let v: Option<Value> = async { http.get(format!("{base}/api/tags")).timeout(Duration::from_secs(8)).send().await.ok()?.json().await.ok() }.await;
    v.as_ref().and_then(|v| v["models"].as_array()).into_iter().flatten()
        .filter_map(|m| Some((m["name"].as_str()?.trim_end_matches(":latest").to_string(), m["size"].as_u64()?)))
        .collect()
}

/// Après la mission : décharge le modèle de l'agent et recharge celui d'Orchestra.
pub async fn restaurer(cfg: &Config, http: &reqwest::Client, c: &Changement) -> Result<Duration, String> {
    let debut = Instant::now();
    decharger(cfg, http, &c.modele_agent).await;
    if c.parallele || cerveau_en_ligne(cfg) {
        return Ok(debut.elapsed());
    }
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
    fn inherit_herite_d_un_cerveau_en_ligne() {
        // retour de PB du 09/11 : Haiku actif, l'agent « inherit » (recommandé qwen3.6:27b) doit garder Haiku
        let a = agent("inherit", "qwen3.5:9b", "qwen3.6:27b");
        let claude = Config { provider: "claude".into(), model: "claude-haiku-4-5".into(), ..Default::default() };
        assert_eq!(choisir_selon(&a, &claude, "claude-haiku-4-5").as_deref(), Some("claude-haiku-4-5"));
        // cerveau local : les modèles admis par l'agent restent la règle
        let ollama = Config { provider: "ollama".into(), ..Default::default() };
        assert_eq!(choisir_selon(&a, &ollama, "llama3:8b").as_deref(), Some("qwen3.6:27b"));
        // un modèle précis demandé par l'agent reste le sien
        assert_eq!(choisir_selon(&agent("qwen3.5:9b", "", ""), &claude, "claude-haiku-4-5").as_deref(), Some("qwen3.5:9b"));
    }

    /// Faux Ollama : note chaque requête `/api/generate` et refuse de charger `geant:405b`.
    async fn faux_ollama() -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", ecoute.local_addr().unwrap());
        let vues = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let v2 = vues.clone();
        tokio::spawn(async move {
            while let Ok((mut flux, _)) = ecoute.accept().await {
                let mut buf = vec![0u8; 65536];
                let mut lu = 0;
                // en-têtes, puis le corps annoncé
                let corps = loop {
                    let n = flux.read(&mut buf[lu..]).await.unwrap_or(0);
                    if n == 0 {
                        break String::new();
                    }
                    lu += n;
                    let t = String::from_utf8_lossy(&buf[..lu]).to_string();
                    if let Some((tete, reste)) = t.split_once("\r\n\r\n") {
                        let taille: usize = tete.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|x| x.trim().parse().unwrap_or(0))).unwrap_or(0);
                        if reste.len() >= taille {
                            break reste.to_string();
                        }
                    }
                };
                let v: Value = serde_json::from_str(&corps).unwrap_or(Value::Null);
                let refuse = v["model"] == "geant:405b" && v.get("prompt").is_some();
                v2.lock().unwrap().push(v);
                let (statut, texte) = if refuse { ("404 Not Found", "{\"error\":\"model 'geant:405b' not found\"}") } else { ("200 OK", "{}") };
                let _ = flux.write_all(format!("HTTP/1.1 {statut}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{texte}", texte.len()).as_bytes()).await;
            }
        });
        (url, vues)
    }

    #[tokio::test]
    async fn modele_refuse_au_chargement_celui_d_orchestra_est_recharge() {
        let (url, vues) = faux_ollama().await;
        let cfg = Config { ollama_url: url, ..Config::default() };
        let e = preparer(&cfg, &reqwest::Client::new(), "geant:405b", "qwen3.6:27b", None).await.unwrap_err();
        assert!(e.contains("geant:405b"), "{e}");
        let vues = vues.lock().unwrap().clone();
        let resume: Vec<(String, bool)> = vues.iter().map(|v| (v["model"].as_str().unwrap_or("").to_string(), v.get("prompt").is_some())).collect();
        assert_eq!(
            resume,
            vec![("qwen3.6:27b".to_string(), false), ("geant:405b".to_string(), true), ("qwen3.6:27b".to_string(), true)],
            "déchargé, refusé, puis le modèle d'Orchestra rechargé"
        );
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
