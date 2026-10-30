//! Essai de la validation des actions irréversibles (D28, lot T3), avec le vrai agent-fichiers :
//! la demande de validation part dans le hub `Validations`, une « réponse de PB » simulée arrive
//! une seconde plus tard, et l'agent supprime (ou non) le fichier dans un projet de test.
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example essai_validation -- accepte|refuse
//! Dossiers de test dans `Orchestra/eval/essai-validation/`.

use std::sync::Arc;
use std::time::Duration;

use orchestra_lib::{agents, contrat, paths::Paths, validations::Validations};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let accepte = std::env::args().nth(1).as_deref() != Some("refuse");
    let paths = Paths::init()?;
    let m = agents::get_installed(&paths, "agent-fichiers").ok_or_else(|| anyhow::anyhow!("agent-fichiers non installé"))?;
    let base = paths.root.join("eval").join("essai-validation");
    let _ = std::fs::remove_dir_all(&base);
    let projet = base.join("projet");
    std::fs::create_dir_all(&projet)?;
    std::fs::write(projet.join("brouillon.md"), "Brouillon à supprimer.")?;
    let mission = contrat::mission(
        &m,
        contrat::Contenu {
            projet: &projet,
            type_tache: Some("operations-fichiers".into()),
            but: "Supprimer brouillon.md",
            critere_reussite: "brouillon.md supprimé après validation",
            contexte: "",
            donnees: json!({ "operations": [{ "type": "supprimer_definitivement", "source": "brouillon.md", "raison": "fichier obsolète" }] }),
            demande_originale: "Supprime brouillon.md",
        },
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let mission_id = mission["id"].as_str().unwrap_or("").to_string();

    let hub = Arc::new(Validations::new(base.join("autorisations.json")));
    let (vers_agent, mut depuis_interface) = tokio::sync::mpsc::unbounded_channel();
    let d = contrat::executer(
        &m,
        &mission,
        &base.join("mission"),
        None,
        Some(&mut depuis_interface),
        |ev| {
            println!("[événement] {} : {}", ev["type"].as_str().unwrap_or(""), ev["message"].as_str().unwrap_or(""));
            if ev["type"] == "validation_requise" {
                let cle = format!("{mission_id}#1");
                hub.attendre(&cle, &mission_id, &m.id, ev["action"].as_str().unwrap_or(""), vers_agent.clone());
                let h = hub.clone();
                // « PB » répond une seconde plus tard, comme depuis l'interface
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    println!("[PB] {}", if accepte { "Oui, cette fois" } else { "Non" });
                    let _ = h.repondre(&cle, accepte, "cette_fois");
                });
            }
            vec![]
        },
        |_| vec![],
    )
    .await?;
    println!("\nétat : {} · brouillon.md présent : {}", d.compte_rendu["etat"], projet.join("brouillon.md").exists());
    Ok(())
}
