//! Essai de l'environnement Python d'un agent (agents de base, 02/11), sans l'interface ni le GPU :
//! installe un paquet .zip dans un dossier de données d'essai, prépare son `.venv` (requirements.txt),
//! vérifie que la dépendance s'importe, puis qu'une mise à jour sans changement de dépendances reprend
//! l'environnement au lieu de le refaire.
//!
//! Depuis `src-tauri/` : `cargo run --release --example essai_environnement -- <paquet.zip>`
//! (par exemple le paquet agent-documents-1.0.0.zip de l'Agent Market). Demande le réseau (PyPI).
use orchestra_lib::{environnement, installation, paths::Paths};
use std::process::Command;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let zip = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage : essai_environnement <paquet.zip>"))?;
    let racine = std::env::temp_dir().join(format!("orchestra-essai-env-{}", std::process::id()));
    std::env::set_var("ORCHESTRA_HOME", &racine);
    let paths = Paths::init()?;
    let mut ok = 0;
    let mut total = 0;
    let mut controle = |nom: &str, vrai: bool, detail: String| {
        total += 1;
        if vrai {
            ok += 1;
        }
        println!("{} {nom} {detail}", if vrai { "OK" } else { "KO" });
    };

    let i = installation::installer_fichier(&paths, std::path::Path::new(&zip), true).map_err(|e| anyhow::anyhow!(e))?;
    let dossier = orchestra_lib::agents::dossier_installe(&paths, &i.manifest.id);
    controle("agent installé", dossier.exists(), format!("{} {}", i.manifest.id, i.manifest.version));
    controle("environnement à préparer", !environnement::pret(&dossier), String::new());

    let debut = Instant::now();
    let r = environnement::preparer(&dossier);
    controle("préparation", matches!(r, Ok(Some(_))), format!("{:?} en {:.1} s", r, debut.elapsed().as_secs_f32()));
    controle("environnement prêt", environnement::pret(&dossier), String::new());
    controle("pas de dossier de préparation restant", !dossier.join(".venv-preparation").exists(), String::new());
    let py = dossier.join(".venv").join("Scripts").join("python.exe");
    let import = Command::new(&py).args(["-c", "import pypdf; print(pypdf.__version__)"]).output();
    let version = import.as_ref().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    controle("pypdf importable dans l'environnement de l'agent", import.map(|o| o.status.success()).unwrap_or(false), version);
    controle("seconde préparation : rien à faire", matches!(environnement::preparer(&dossier), Ok(None)), String::new());

    // mise à jour (même paquet réinstallé) : l'environnement est repris tel quel
    let i2 = installation::installer_fichier(&paths, std::path::Path::new(&zip), true).map_err(|e| anyhow::anyhow!(e))?;
    controle("mise à jour : environnement repris", environnement::pret(&dossier) && py.exists(), format!("ancienne version {:?}", i2.ancienne_version));

    let _ = std::fs::remove_dir_all(&racine);
    println!("\n{ok}/{total} contrôles réussis");
    if ok != total {
        std::process::exit(1);
    }
    Ok(())
}
