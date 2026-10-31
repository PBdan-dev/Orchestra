//! Lanceur d'évaluation d'Orchestra, sans interface.
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example evaluation -- --modele qwen3.6:27b
//! Options : --passages 3 · --temperature 0.6 · --jeu tout|memoire|taches|conversation · --limite N · --ids t07,t08
//!
//! Le rapport est écrit dans `Orchestra/eval/rapport-<date>.md` (détail complet dans le `.jsonl` voisin).

use std::path::Path;

use orchestra_lib::{evaluation, paths::Paths};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let val = |nom: &str| args.iter().position(|a| a == nom).and_then(|i| args.get(i + 1)).cloned();

    let modele = val("--modele").unwrap_or_else(|| "qwen3.6:27b".into());
    let passages = val("--passages").and_then(|v| v.parse().ok()).unwrap_or(3);
    // 0,6 (recommandée par Qwen) avec graine fixe : reproductible, sans les réflexions en boucle de la température 0 (décision de PB, 29/10)
    let temperature = val("--temperature").and_then(|v| v.parse().ok()).unwrap_or(0.6);
    let jeu = val("--jeu").unwrap_or_else(|| "tout".into());
    let limite = val("--limite").and_then(|v| v.parse().ok());
    let ids: Vec<String> = val("--ids").map(|v| v.split(',').map(|s| s.trim().to_string()).collect()).unwrap_or_default();

    let racine = match Paths::racine_par_defaut() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Dossier de données introuvable : {e}");
            std::process::exit(1);
        }
    };
    let opts = evaluation::Options {
        modele,
        passages,
        temperature,
        memoire: jeu == "tout" || jeu == "memoire",
        taches: jeu == "tout" || jeu == "taches",
        conversation: jeu == "tout" || jeu == "conversation",
        limite,
        ids,
        racine,
        jeux: Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("eval"),
    };
    match evaluation::lancer(opts, |l| println!("{l}")).await {
        Ok(p) => println!("\nRapport : {}", p.display()),
        Err(e) => {
            eprintln!("Évaluation interrompue : {e}");
            std::process::exit(1);
        }
    }
}
