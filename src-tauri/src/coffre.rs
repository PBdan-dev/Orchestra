//! Coffre de Windows (Gestionnaire d'identification) : clés API du cerveau d'Orchestra (lot S-c).
//! Les identifiants des agents (D34) y sont aussi, sous `orchestra/<id>/<nom>` (installation.rs).
//!
//! Les clés API sont rangées sous `orchestra/_cles_api/<fournisseur>` : le `_` ne peut pas
//! commencer un identifiant d'agent, donc aucune collision avec D34. Un dossier de données autre
//! que celui d'Orchestra (essais, évaluation) a ses propres entrées, suffixées par l'empreinte de
//! son chemin : un essai ne touche jamais aux vraies clés de PB.

use crate::paths::Paths;

/// Accès au coffre ; remplacé par un coffre en mémoire dans les tests.
pub trait Coffre: Send + Sync {
    fn lire(&self, cible: &str) -> Option<String>;
    fn ecrire(&self, cible: &str, valeur: &str) -> Result<(), String>;
    fn supprimer(&self, cible: &str) -> Result<(), String>;
}

/// Le Gestionnaire d'identification de Windows (bibliothèque `keyring`).
pub struct CoffreWindows;

impl CoffreWindows {
    fn entree(cible: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new_with_target(cible, "orchestra", cible).map_err(|e| format!("coffre de Windows inaccessible : {e}"))
    }
}

impl Coffre for CoffreWindows {
    fn lire(&self, cible: &str) -> Option<String> {
        Self::entree(cible).ok()?.get_password().ok()
    }

    fn ecrire(&self, cible: &str, valeur: &str) -> Result<(), String> {
        Self::entree(cible)?.set_password(valeur).map_err(|e| format!("coffre de Windows : écriture refusée ({e})"))
    }

    fn supprimer(&self, cible: &str) -> Result<(), String> {
        match Self::entree(cible)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("coffre de Windows : suppression refusée ({e})")),
        }
    }
}

/// Fournisseurs à clé API, et le champ de `config.json` où elles étaient avant le lot S.
pub const FOURNISSEURS: [(&str, &str); 3] = [("claude", "claude_api_key"), ("gemini", "gemini_api_key"), ("openrouter", "openrouter_api_key")];

/// Nom de la clé API d'un fournisseur dans le coffre.
pub fn cible_cle_api(paths: &Paths, fournisseur: &str) -> String {
    let base = format!("orchestra/_cles_api/{fournisseur}");
    let normalise = |p: &std::path::Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    match Paths::racine_par_defaut() {
        Ok(r) if normalise(&r) == normalise(&paths.root) => base,
        _ => {
            use sha2::{Digest, Sha256};
            let h = Sha256::digest(normalise(&paths.root).as_bytes());
            format!("{base}@{:02x}{:02x}{:02x}{:02x}", h[0], h[1], h[2], h[3])
        }
    }
}

/// Coffre en mémoire, pour les tests.
#[cfg(test)]
#[derive(Default)]
pub struct CoffreMemoire {
    pub entrees: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
    /// Coffre qui refuse toute écriture (coffre de Windows inaccessible).
    pub refuse: bool,
}

#[cfg(test)]
impl Coffre for CoffreMemoire {
    fn lire(&self, cible: &str) -> Option<String> {
        self.entrees.lock().unwrap().get(cible).cloned()
    }

    fn ecrire(&self, cible: &str, valeur: &str) -> Result<(), String> {
        if self.refuse {
            return Err("coffre inaccessible".into());
        }
        self.entrees.lock().unwrap().insert(cible.into(), valeur.into());
        Ok(())
    }

    fn supprimer(&self, cible: &str) -> Result<(), String> {
        self.entrees.lock().unwrap().remove(cible);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cle_d_un_dossier_d_essai_a_part() {
        let racine = std::env::temp_dir().join("orchestra-sc-essai");
        let p = Paths { root: racine.clone(), ..Paths::at(racine).unwrap() };
        let c = cible_cle_api(&p, "claude");
        assert!(c.starts_with("orchestra/_cles_api/claude@"), "{c}");
        assert_eq!(c.len(), "orchestra/_cles_api/claude@".len() + 8);
        // même dossier, même entrée ; autre fournisseur, autre entrée
        assert_eq!(c, cible_cle_api(&p, "claude"));
        assert_ne!(c, cible_cle_api(&p, "gemini"));
    }
}
