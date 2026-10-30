//! Banc d'essai des agents installés : rejoue les tests livrés avec chaque agent, à travers le contrat.
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example banc_agents -- agent-date agent-fichiers
//! Sans argument : tous les agents installés. Rapport dans `Orchestra/eval/banc/rapport-<date>.md`.

use orchestra_lib::{banc, paths::Paths};

#[tokio::main]
async fn main() {
    let ids: Vec<String> = std::env::args().skip(1).collect();
    let paths = match Paths::init() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Dossier de données introuvable : {e}");
            std::process::exit(1);
        }
    };
    match banc::rejouer(&paths, &ids, |l| println!("{l}")).await {
        Ok((rapport, r)) => {
            let ok = r.iter().filter(|x| x.reussi()).count();
            println!("\n{ok}/{} réussis — rapport : {}", r.len(), rapport.display());
            if ok < r.len() {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("Banc interrompu : {e}");
            std::process::exit(1);
        }
    }
}
