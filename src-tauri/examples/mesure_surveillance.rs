//! Mesure du lot S-b : temps d'un relevé du dossier de données d'Orchestra (lecture seule),
//! sur le vrai dossier. Chaque mission en fait deux (avant et après l'agent).
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example mesure_surveillance

use std::time::Instant;

use orchestra_lib::{paths::Paths, surveillance};

fn main() -> anyhow::Result<()> {
    let paths = Paths::init()?;
    let zones = surveillance::Zones::de(&paths, &paths.projets.join("divers"), None, "mesure");
    let mut durees = vec![];
    let mut releve = surveillance::Releve::default();
    for _ in 0..5 {
        let t = Instant::now();
        releve = surveillance::relever(&paths, &zones);
        durees.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let octets: u64 = releve.fichiers.keys().filter_map(|f| std::fs::metadata(paths.root.join(f)).ok()).map(|m| m.len()).sum();
    durees.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "{} : {} fichiers relevés ({:.1} Mo), {} gardés pour les différences ; relevé médian {:.0} ms (min {:.0}, max {:.0})",
        paths.root.display(),
        releve.fichiers.len(),
        octets as f64 / 1e6,
        releve.textes.len(),
        durees[2],
        durees[0],
        durees[4]
    );
    Ok(())
}
