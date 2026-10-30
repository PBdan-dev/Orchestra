//! Lance une mission à la main, à travers la porte du modèle (outil de développement, lot T2).
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example mission_directe -- <dossier_de_l_agent> <mission.json>
//! Le dossier de mission (traces, pensée, compte rendu) est créé dans `Orchestra/missions/`.

use std::path::PathBuf;

use orchestra_lib::{agents::Manifest, config::Config, contrat, modele, paths::Paths, porte};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(dossier), Some(fichier)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage : mission_directe <dossier_de_l_agent> <mission.json>");
    };
    let paths = Paths::init()?;
    let cfg = Config::load(&paths);
    let http = reqwest::Client::new();
    let brut: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(PathBuf::from(dossier).join("manifeste.json"))?)?;
    let mut m = Manifest::depuis_v2(&brut).map_err(|e| anyhow::anyhow!("manifeste refusé : {e}"))?;
    m.dossier = Some(PathBuf::from(dossier));
    let mut mission: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(fichier)?)?;
    mission["id"] = serde_json::json!(contrat::id_mission(&m.id));
    mission["manifeste"] = brut.clone();
    contrat::valider_mission(&mission).map_err(|e| anyhow::anyhow!(e))?;
    let dossier_mission = paths.missions.join(mission["id"].as_str().unwrap_or("mission"));
    std::fs::create_dir_all(&dossier_mission)?;
    let mut ouverte = None;
    if let Some(modele_agent) = modele::choisir(&m, &cfg.model) {
        println!("modèle servi par la porte : {modele_agent}");
        let limites = porte::Limites::depuis(&m, mission["type_tache"].as_str());
        ouverte = Some(porte::Porte::ouvrir(porte::Config_ { cfg: cfg.clone(), http: http.clone(), agent: m.clone(), modele: modele_agent, dossier: dossier_mission.clone(), limites, reponse_fixe: None }).await?);
    }
    let avec = ouverte.as_mut().map(|(p, n)| contrat::AvecPorte { porte: &*p, notes: n });
    let d = contrat::executer(
        &m,
        &mission,
        &dossier_mission,
        avec,
        |ev| {
            println!("[événement] {} : {}", ev["type"].as_str().unwrap_or(""), ev["message"].as_str().unwrap_or(""));
            if ev["type"] == "validation_requise" {
                vec![contrat::reponse_validation(mission["id"].as_str().unwrap_or(""), false, "cette_fois")]
            } else {
                vec![]
            }
        },
        |note| {
            println!("[porte] {note:?}");
            vec![]
        },
    )
    .await?;
    if let Some((p, _)) = &ouverte {
        println!("porte : {:?} (appels, tokens)", p.compteurs());
    }
    for e in &d.ecarts {
        println!("[écart] {e}");
    }
    println!("\ncompte rendu :\n{}", serde_json::to_string_pretty(&d.compte_rendu)?);
    println!("\ndossier de mission : {}", dossier_mission.display());
    let _ = http.post(format!("{}/api/generate", cfg.ollama_url.trim_end_matches('/'))).json(&serde_json::json!({ "model": cfg.model, "keep_alive": 0 })).send().await;
    Ok(())
}
