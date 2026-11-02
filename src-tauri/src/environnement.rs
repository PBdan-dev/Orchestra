//! Environnement Python d'un agent (agents de base, 02/11) : un agent qui livre un `requirements.txt`
//! reçoit son propre `.venv` dans `agents/installes/<id>/`, que `contrat.rs` utilise s'il existe.
//!
//! Préparé en tâche de fond après l'installation (et au lancement d'Orchestra pour les agents déjà
//! installés) : `python -m venv`, puis `pip install -r requirements.txt` (dépendances téléchargées
//! depuis PyPI, sous licences vérifiées à la certification). Construit dans `.venv-preparation`,
//! renommé en `.venv` seulement quand tout a réussi : une mission ne voit jamais un environnement à
//! moitié prêt. L'empreinte du `requirements.txt` est notée dans `.venv/orchestra-requirements.sha256` ;
//! une mise à jour qui ne change pas les dépendances ne refait rien.
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const DELAI: Duration = Duration::from_secs(600);
const EMPREINTE: &str = "orchestra-requirements.sha256";

/// Ce que demande l'agent : `None` s'il n'a pas de `requirements.txt` (rien à préparer).
pub fn besoin(dossier: &Path) -> Option<String> {
    let texte = std::fs::read(dossier.join("requirements.txt")).ok()?;
    Some(format!("{:x}", Sha256::digest(&texte)))
}

/// L'environnement est-il prêt et à jour ?
pub fn pret(dossier: &Path) -> bool {
    match besoin(dossier) {
        None => true,
        Some(h) => std::fs::read_to_string(dossier.join(".venv").join(EMPREINTE)).map(|t| t.trim() == h).unwrap_or(false),
    }
}

fn lancer(programme: &Path, args: &[&str], dossier: &Path) -> Result<(), String> {
    let mut c = Command::new(programme);
    c.args(args).current_dir(dossier).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // pas de fenêtre de console
    }
    let mut enfant = c.spawn().map_err(|e| format!("{} introuvable ({e}) : Python 3.10 ou plus doit être installé", programme.display()))?;
    // la sortie d'erreur est lue en continu : un tampon plein bloquerait pip
    let lecteur = enfant.stderr.take().map(|mut e| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut t = String::new();
            let _ = e.read_to_string(&mut t);
            t
        })
    });
    let debut = Instant::now();
    loop {
        if let Some(statut) = enfant.try_wait().map_err(|e| e.to_string())? {
            if statut.success() {
                return Ok(());
            }
            let erreur = lecteur.and_then(|l| l.join().ok()).unwrap_or_default();
            let fin: String = erreur.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ");
            return Err(format!("{} {} a échoué : {}", programme.file_name().and_then(|n| n.to_str()).unwrap_or("python"), args.join(" "), fin.trim()));
        }
        if debut.elapsed() > DELAI {
            let _ = enfant.kill();
            return Err("préparation arrêtée après 10 minutes (réseau lent ?)".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Prépare (ou met à jour) l'environnement de l'agent installé dans `dossier`.
/// `Ok(None)` : rien à faire ; `Ok(Some(détail))` : environnement prêt ; `Err` : motif en français.
pub fn preparer(dossier: &Path) -> Result<Option<String>, String> {
    let Some(h) = besoin(dossier) else { return Ok(None) };
    if pret(dossier) {
        return Ok(None);
    }
    let temp = dossier.join(".venv-preparation");
    let _ = std::fs::remove_dir_all(&temp);
    lancer(Path::new("python"), &["-m", "venv", ".venv-preparation"], dossier)?;
    let py = temp.join("Scripts").join("python.exe");
    let py = if py.exists() { py } else { temp.join("bin").join("python") };
    if let Err(e) = lancer(&py, &["-m", "pip", "install", "--disable-pip-version-check", "--no-input", "--quiet", "-r", "requirements.txt"], dossier) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(e);
    }
    std::fs::write(temp.join(EMPREINTE), &h).map_err(|e| e.to_string())?;
    let venv = dossier.join(".venv");
    if venv.exists() {
        std::fs::remove_dir_all(&venv).map_err(|e| format!("ancien environnement impossible à remplacer (agent en cours ?) : {e}"))?;
    }
    std::fs::rename(&temp, &venv).map_err(|e| e.to_string())?;
    let paquets: Vec<String> = std::fs::read_to_string(dossier.join("requirements.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    Ok(Some(paquets.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_requirements_rien_a_faire() {
        let d = std::env::temp_dir().join(format!("orch-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(besoin(&d).is_none());
        assert!(pret(&d));
        assert_eq!(preparer(&d).unwrap(), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn empreinte_suit_le_fichier() {
        let d = std::env::temp_dir().join(format!("orch-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join(".venv")).unwrap();
        std::fs::write(d.join("requirements.txt"), "pypdf>=6,<7\n").unwrap();
        assert!(!pret(&d));
        std::fs::write(d.join(".venv").join(EMPREINTE), besoin(&d).unwrap()).unwrap();
        assert!(pret(&d));
        std::fs::write(d.join("requirements.txt"), "pypdf>=7\n").unwrap();
        assert!(!pret(&d), "un requirements.txt modifié demande une nouvelle préparation");
        let _ = std::fs::remove_dir_all(&d);
    }
}
