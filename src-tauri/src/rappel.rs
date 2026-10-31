//! Souvenirs pour le mode voix (lot D, D2) : point d'accès local que le serveur voix interroge à
//! chaque phrase de PB, pour recevoir les mêmes souvenirs qu'en chat. Réponse en quelques
//! millisecondes (rappel par mots, sans appel au modèle) ; le serveur voix n'attend pas plus de
//! 50 ms et répond sans souvenirs au-delà.
//!
//! `POST /rappel` sur 127.0.0.1 (port libre), jeton dans `Authorization: Bearer`, corps
//! `{"texte": "…"}` → `{"souvenirs": "- [fichier › section] fait\n…"}`. L'adresse et le jeton sont
//! donnés au serveur voix au lancement (`ORCHESTRA_RAPPEL_URL`, `ORCHESTRA_RAPPEL_JETON`).

use std::sync::OnceLock;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::{memory, paths::Paths};

static ADRESSE: OnceLock<(String, String)> = OnceLock::new();

/// Adresse et jeton du point d'accès, s'il est ouvert.
pub fn adresse() -> Option<(String, String)> {
    ADRESSE.get().cloned()
}

/// Souvenirs d'une phrase, au format du bloc 5 du chat.
pub async fn souvenirs(paths: &Paths, texte: &str) -> String {
    let r = memory::recall(paths, None, texte, 6).await;
    if r.is_empty() {
        "Aucun souvenir pertinent.".into()
    } else {
        r.iter().map(|x| format!("- [{} › {}] {}", x.file, x.section, x.text)).collect::<Vec<_>>().join("\n")
    }
}

/// Ouvre le point d'accès (une seule fois par lancement d'Orchestra).
pub async fn ouvrir(paths: Paths) -> anyhow::Result<(String, String)> {
    if let Some(a) = adresse() {
        return Ok(a);
    }
    let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", ecoute.local_addr()?);
    let jeton = uuid::Uuid::new_v4().simple().to_string();
    let _ = ADRESSE.set((url.clone(), jeton.clone()));
    tokio::spawn(async move {
        while let Ok((flux, _)) = ecoute.accept().await {
            let (p, j) = (paths.clone(), jeton.clone());
            tokio::spawn(async move {
                let _ = servir(flux, &p, &j).await;
            });
        }
    });
    Ok(adresse().unwrap_or_default())
}

async fn servir(flux: tokio::net::TcpStream, paths: &Paths, jeton: &str) -> anyhow::Result<()> {
    let mut lecteur = BufReader::new(flux);
    let mut ligne = String::new();
    lecteur.read_line(&mut ligne).await?;
    let mut longueur = 0usize;
    let mut auth = String::new();
    loop {
        let mut l = String::new();
        if lecteur.read_line(&mut l).await? == 0 || l.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => longueur = v.trim().parse().unwrap_or(0),
                "authorization" => auth = v.trim().trim_start_matches("Bearer ").trim().to_string(),
                _ => {}
            }
        }
    }
    let mut corps = vec![0u8; longueur.min(64 * 1024)];
    lecteur.read_exact(&mut corps).await?;
    let mut flux = lecteur.into_inner();
    let (statut, reponse) = if auth != jeton {
        (401, json!({ "erreur": "jeton invalide" }))
    } else if !ligne.starts_with("POST /rappel") {
        (404, json!({ "erreur": "chemin inconnu" }))
    } else {
        let v: Value = serde_json::from_slice(&corps).unwrap_or_default();
        (200, json!({ "souvenirs": souvenirs(paths, v["texte"].as_str().unwrap_or("")).await }))
    };
    let corps = reponse.to_string();
    let tete = format!("HTTP/1.1 {statut} OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", corps.len());
    flux.write_all(tete.as_bytes()).await?;
    flux.write_all(corps.as_bytes()).await?;
    flux.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rappel_par_http_en_quelques_millisecondes() {
        let r = std::env::temp_dir().join(format!("orchestra-d-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let paths = Paths::at(r.clone()).unwrap();
        memory::seed_main(&paths, "PB").unwrap();
        let mut m = std::fs::read_to_string(paths.main_memory()).unwrap();
        m = m.replace("## Préférences\n", "## Préférences\n- (2026-10-30) PB écoute du jazz en travaillant, surtout Bill Evans.\n");
        std::fs::write(paths.main_memory(), m).unwrap();
        let (url, jeton) = ouvrir(paths).await.unwrap();
        let http = reqwest::Client::new();
        let t0 = std::time::Instant::now();
        let v: Value = http.post(format!("{url}/rappel")).bearer_auth(&jeton).json(&json!({ "texte": "quel jazz j'écoute ?" })).send().await.unwrap().json().await.unwrap();
        let duree = t0.elapsed();
        assert!(v["souvenirs"].as_str().unwrap().contains("Bill Evans"), "{v}");
        assert!(duree.as_millis() < 50, "{duree:?}");
        let refus = http.post(format!("{url}/rappel")).bearer_auth("faux").json(&json!({ "texte": "x" })).send().await.unwrap();
        assert_eq!(refus.status(), 401);
        let _ = std::fs::remove_dir_all(r);
    }
}
