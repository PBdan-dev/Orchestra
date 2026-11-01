//! Mesure de la compression à J+1 (lot M1) sur une copie des vraies données : taille avant/après,
//! durée, relecture identique de chaque fichier, vérification des chaînes (lot T6) inchangée.
//!
//! Copie `memoire/`, `journal/` et `missions/` du dossier de données (et les missions des essais de
//! `eval/`, pour avoir plus de traces) dans un dossier temporaire ; jamais de compression du vrai.
//! Lancement, depuis `src-tauri/` : `cargo run --release --example mesure_compression`.
//! Rapport : `Orchestra/eval/compression-<date>.md`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use orchestra_lib::{compression, integrite, paths::Paths};

fn copier(de: &Path, vers: &Path) -> std::io::Result<()> {
    if !de.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(vers)?;
    for e in std::fs::read_dir(de)?.flatten() {
        let (a, b) = (e.path(), vers.join(e.file_name()));
        if a.is_dir() {
            copier(&a, &b)?;
        } else {
            std::fs::copy(&a, &b)?;
        }
    }
    Ok(())
}

fn taille(d: &Path) -> u64 {
    std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| if e.path().is_dir() { taille(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum()
}

fn main() -> anyhow::Result<()> {
    let reel = Paths::init()?;
    let racine = std::env::temp_dir().join(format!("orchestra-compression-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]));
    let p = Paths::at(racine.join("Orchestra"))?;
    copier(&reel.memoire, &p.memoire)?;
    copier(&reel.journal, &p.journal)?;
    copier(&reel.missions, &p.missions)?;
    // missions des essais (eval/<essai>/Orchestra/missions), pour plus de traces
    for e in std::fs::read_dir(reel.root.join("eval")).into_iter().flatten().flatten() {
        copier(&e.path().join("Orchestra").join("missions"), &p.missions)?;
    }
    let categories: [(&str, PathBuf); 3] = [("memoire (archives comprises)", p.memoire.clone()), ("journal", p.journal.clone()), ("missions", p.missions.clone())];
    let avant: Vec<u64> = categories.iter().map(|(_, d)| taille(d)).collect();
    let dus = compression::dus(&p, chrono::Local::now().date_naive(), None);
    let textes: Vec<(PathBuf, Vec<u8>)> = dus.iter().map(|f| (f.clone(), std::fs::read(f).unwrap_or_default())).collect();
    let chaines_avant = integrite::verifier_tout(&p, None);
    let t = Instant::now();
    let b = compression::compresser_dus(&p, chrono::Local::now().date_naive(), None);
    let duree = t.elapsed();
    let identiques = textes.iter().filter(|(f, o)| compression::lire_texte(f).map(|s| s.as_bytes() == o.as_slice()).unwrap_or(false)).count();
    let chaines_apres = integrite::verifier_tout(&p, None);
    let apres: Vec<u64> = categories.iter().map(|(_, d)| taille(d)).collect();

    let date = chrono::Local::now();
    let mut s = format!(
        "# Mesure de la compression à J+1 (lot M1) — {}\n\nCopie des données réelles (et des missions des essais de `eval/`), zstd niveau {}.\n\n| Dossier | Avant | Après |\n|---|---|---|\n",
        date.format("%d/%m/%Y %H:%M"),
        compression::NIVEAU
    );
    for (i, (nom, _)) in categories.iter().enumerate() {
        s.push_str(&format!("| {nom} | {:.1} Ko | {:.1} Ko |\n", avant[i] as f64 / 1024.0, apres[i] as f64 / 1024.0));
    }
    s.push_str(&format!(
        "\n- Fichiers compressés : {} ({:.1} Ko → {:.1} Ko, divisé par {:.1}) en {:.1} s ; erreurs : {}.\n- Relecture transparente identique : {identiques}/{}.\n- Chaînes vérifiées (T6) avant : {} intacte(s), {} non chaînée(s), {} cassée(s) ; après : {} intacte(s), {} non chaînée(s), {} cassée(s) (cassures volontaires des essais T6).\n",
        b.fichiers,
        b.avant as f64 / 1024.0,
        b.apres as f64 / 1024.0,
        b.avant as f64 / b.apres.max(1) as f64,
        duree.as_secs_f64(),
        if b.erreurs.is_empty() { "aucune".to_string() } else { b.erreurs.join(" · ") },
        textes.len(),
        chaines_avant.intactes,
        chaines_avant.non_chainees,
        chaines_avant.ruptures.len() + chaines_avant.deja_signalees,
        chaines_apres.intactes,
        chaines_apres.non_chainees,
        chaines_apres.ruptures.len() + chaines_apres.deja_signalees,
    ));
    let dossier = reel.root.join("eval");
    std::fs::create_dir_all(&dossier)?;
    let rapport = dossier.join(format!("compression-{}.md", date.format("%Y-%m-%d_%H%M")));
    std::fs::write(&rapport, &s)?;
    let _ = std::fs::remove_dir_all(&racine);
    println!("{s}\nRapport : {}", rapport.display());
    Ok(())
}
