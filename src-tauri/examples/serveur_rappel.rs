//! Ouvre seul le point d'accès des souvenirs du mode voix (lot D), sur les vraies données, pour
//! tester le serveur voix sans l'application. Affiche l'adresse et le jeton à donner au serveur voix
//! (`ORCHESTRA_RAPPEL_URL`, `ORCHESTRA_RAPPEL_JETON`) et les écrit dans le fichier donné en argument.
//!
//!   cargo run --release --example serveur_rappel -- rappel.env

use orchestra_lib::{paths::Paths, rappel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = Paths::init()?;
    let (url, jeton) = rappel::ouvrir(paths).await?;
    println!("ORCHESTRA_RAPPEL_URL={url}\nORCHESTRA_RAPPEL_JETON={jeton}");
    if let Some(f) = std::env::args().nth(1) {
        std::fs::write(f, format!("{url}\n{jeton}\n"))?;
    }
    // reste ouvert jusqu'à l'arrêt du programme
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}
