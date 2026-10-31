//! Intégrité des traces (lot T6) : chaîne d'empreintes SHA-256 sur `echanges.jsonl`,
//! `actions.jsonl` et le journal, tous écrits par Orchestra.
//!
//! Chaque ligne porte `prev` : l'empreinte de la ligne précédente (la première, une suite de 0).
//! Une ligne modifiée, retirée ou insérée casse la chaîne. À côté de chaque fichier,
//! `<fichier>.chaine` garde le nombre de lignes et l'empreinte de la dernière : une fin coupée
//! ou une dernière ligne modifiée se voit aussi. Les lignes écrites avant ce lot (sans `prev`)
//! sont « non chaînées », sans alerte. La signature par une clé locale viendra plus tard (D15).
//!
//! Cassure (décision de PB du 31/10) : copie du fichier mise à l'abri dans `integrite/preuves/`,
//! écart pour l'agent si la cassure est dans le dossier d'une mission (lot T5), alerte visible
//! jusqu'à ce que PB l'ait vue. Rien n'est réparé ni effacé.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::paths::Paths;

pub const DEBUT: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Un seul écrivain à la fois (le journal reçoit des événements de plusieurs tâches).
static ECRITURE: Mutex<()> = Mutex::new(());

pub fn empreinte(ligne: &str) -> String {
    format!("{:x}", Sha256::digest(ligne.as_bytes()))
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
struct Fin {
    lignes: usize,
    derniere: String,
}

fn chemin_fin(f: &Path) -> PathBuf {
    let mut s = f.as_os_str().to_os_string();
    s.push(".chaine");
    PathBuf::from(s)
}

fn lire_fin(f: &Path) -> Option<Fin> {
    std::fs::read_to_string(chemin_fin(f)).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Ajoute une ligne chaînée à un fichier JSONL (l'objet reçoit `prev`).
pub fn ajouter(f: &Path, mut v: Value) -> std::io::Result<()> {
    let _verrou = ECRITURE.lock().unwrap_or_else(|e| e.into_inner());
    // empreinte et nombre de lignes connus ; sinon (fichier ancien ou fin absente), relus
    let fin = match lire_fin(f) {
        Some(fin) => fin,
        None => {
            let texte = std::fs::read_to_string(f).unwrap_or_default();
            let lignes: Vec<&str> = texte.lines().filter(|l| !l.trim().is_empty()).collect();
            Fin { lignes: lignes.len(), derniere: lignes.last().map(|l| empreinte(l)).unwrap_or_else(|| DEBUT.into()) }
        }
    };
    if let Some(o) = v.as_object_mut() {
        o.insert("prev".into(), json!(fin.derniere));
    }
    let ligne = v.to_string();
    let mut fichier = std::fs::OpenOptions::new().create(true).append(true).open(f)?;
    writeln!(fichier, "{ligne}")?;
    let nouvelle = Fin { lignes: fin.lignes + 1, derniere: empreinte(&ligne) };
    std::fs::write(chemin_fin(f), serde_json::to_string(&nouvelle).unwrap_or_default())
}

/// Écrit d'un coup un fichier JSONL chaîné (remplace l'existant).
pub fn ecrire_tout(f: &Path, lignes: &[Value]) -> std::io::Result<()> {
    let _ = std::fs::remove_file(f);
    let _ = std::fs::remove_file(chemin_fin(f));
    for l in lignes {
        ajouter(f, l.clone())?;
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "etat", rename_all = "snake_case")]
pub enum Etat {
    Intacte { lignes: usize },
    /// Écrit avant le lot T6 : rien à vérifier.
    NonChainee { lignes: usize },
    Cassee { ligne: usize, motif: String },
}

/// Vérifie la chaîne d'un fichier.
pub fn verifier(f: &Path) -> Etat {
    let texte = std::fs::read_to_string(f).unwrap_or_default();
    let lignes: Vec<&str> = texte.lines().filter(|l| !l.trim().is_empty()).collect();
    let fin = lire_fin(f);
    let mut chainee = false;
    for (i, l) in lignes.iter().enumerate() {
        let prev = serde_json::from_str::<Value>(l).ok().and_then(|v| v["prev"].as_str().map(String::from));
        match prev {
            Some(p) => {
                let attendu = if i == 0 { DEBUT.to_string() } else { empreinte(lignes[i - 1]) };
                if p != attendu {
                    return Etat::Cassee { ligne: i + 1, motif: if chainee || i == 0 { "la ligne précédente a été modifiée ou retirée, ou celle-ci insérée".into() } else { "début de chaîne inattendu".into() } };
                }
                chainee = true;
            }
            None if chainee => return Etat::Cassee { ligne: i + 1, motif: "ligne ajoutée hors de la chaîne".into() },
            None => {}
        }
    }
    match fin {
        Some(fin) => {
            if lignes.len() < fin.lignes {
                Etat::Cassee { ligne: lignes.len() + 1, motif: format!("fin du fichier coupée ({} ligne(s) au lieu de {})", lignes.len(), fin.lignes) }
            } else if lignes.len() > fin.lignes {
                Etat::Cassee { ligne: fin.lignes + 1, motif: "ligne(s) ajoutée(s) sans passer par Orchestra".into() }
            } else if lignes.last().map(|l| empreinte(l)).unwrap_or_else(|| DEBUT.into()) != fin.derniere {
                Etat::Cassee { ligne: lignes.len(), motif: "dernière ligne modifiée".into() }
            } else {
                Etat::Intacte { lignes: lignes.len() }
            }
        }
        None if chainee => Etat::Cassee { ligne: lignes.len(), motif: "empreinte de fin absente (fichier .chaine supprimé)".into() },
        None => Etat::NonChainee { lignes: lignes.len() },
    }
}

/// Résultat de la vérification d'un fichier, pour Conscience et le journal.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rupture {
    /// Chemin relatif au dossier de données d'Orchestra.
    pub fichier: String,
    pub ligne: usize,
    pub motif: String,
    /// Agent de la mission, si la cassure est dans le dossier d'une mission.
    pub agent_id: Option<String>,
    pub mission_id: Option<String>,
    /// Copie mise à l'abri.
    pub preuve: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Bilan {
    pub intactes: usize,
    pub non_chainees: usize,
    pub ruptures: Vec<Rupture>,
    /// Ruptures déjà signalées lors d'une vérification précédente (pas de nouvel écart).
    pub deja_signalees: usize,
}

/// Fichiers chaînés : journal du jour et traces de chaque mission.
fn fichiers(paths: &Paths, mission: Option<&str>) -> Vec<PathBuf> {
    let mut out = vec![];
    let missions: Vec<PathBuf> = match mission {
        Some(id) => vec![paths.missions.join(crate::paths::safe_name(id))],
        None => {
            for e in std::fs::read_dir(&paths.journal).into_iter().flatten().flatten() {
                if e.path().extension().is_some_and(|x| x == "jsonl") {
                    out.push(e.path());
                }
            }
            std::fs::read_dir(&paths.missions).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()
        }
    };
    for d in missions {
        for n in ["echanges.jsonl", "actions.jsonl"] {
            if d.join(n).exists() {
                out.push(d.join(n));
            }
        }
    }
    out.sort();
    out
}

fn deja_signalees_chemin(paths: &Paths) -> PathBuf {
    paths.root.join("integrite").join("ruptures-signalees.json")
}

/// Vérifie toutes les chaînes (ou celles d'une mission). Une nouvelle rupture : preuve mise à
/// l'abri, écart pour l'agent de la mission, noté pour ne pas compter deux fois.
pub fn verifier_tout(paths: &Paths, mission: Option<&str>) -> Bilan {
    let mut b = Bilan::default();
    let mut signalees: Vec<String> = std::fs::read_to_string(deja_signalees_chemin(paths)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for f in fichiers(paths, mission) {
        match verifier(&f) {
            Etat::Intacte { .. } => b.intactes += 1,
            Etat::NonChainee { .. } => b.non_chainees += 1,
            Etat::Cassee { ligne, motif } => {
                let rel = f.strip_prefix(&paths.root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                let cle = format!("{rel}#{ligne}#{motif}");
                // dossier de mission : l'agent est dans mission.json
                let dossier = f.parent().map(Path::to_path_buf).unwrap_or_default();
                let (mission_id, agent_id) = if dossier.parent() == Some(paths.missions.as_path()) {
                    let m: Value = std::fs::read_to_string(dossier.join("mission.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
                    (Some(dossier.file_name().unwrap_or_default().to_string_lossy().to_string()), m["agent_id"].as_str().map(String::from))
                } else {
                    (None, None)
                };
                if signalees.contains(&cle) {
                    b.deja_signalees += 1;
                    continue;
                }
                let preuve = paths.root.join("integrite").join("preuves").join(format!("{}_{}", chrono::Local::now().format("%Y-%m-%d_%H%M%S"), rel.replace('/', "_")));
                let _ = std::fs::create_dir_all(preuve.parent().unwrap_or(&paths.root));
                let _ = std::fs::copy(&f, &preuve);
                let _ = std::fs::copy(chemin_fin(&f), { let mut s = preuve.as_os_str().to_os_string(); s.push(".chaine"); PathBuf::from(s) });
                if let (Some(a), Some(m)) = (&agent_id, &mission_id) {
                    let v = crate::verification::Verification { ecarts: vec![format!("trace {rel} cassée à la ligne {ligne} : {motif}")], controles: vec![] };
                    if crate::verification::enregistrer(paths, a, m, &v).is_ok_and(|e| e.statut != crate::verification::Statut::Normal) {
                        let _ = crate::memory::retirer_confiance(paths, a);
                    }
                }
                signalees.push(cle);
                b.ruptures.push(Rupture { fichier: rel, ligne, motif, agent_id, mission_id, preuve: preuve.strip_prefix(&paths.root).unwrap_or(&preuve).to_string_lossy().replace('\\', "/") });
            }
        }
    }
    if !b.ruptures.is_empty() {
        let p = deja_signalees_chemin(paths);
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(&paths.root));
        let _ = std::fs::write(p, serde_json::to_string_pretty(&signalees).unwrap_or_default());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fichier_essai() -> (PathBuf, PathBuf) {
        let d = std::env::temp_dir().join(format!("orchestra-t6-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("echanges.jsonl");
        for i in 0..4 {
            ajouter(&f, json!({ "n": i, "texte": format!("ligne {i}") })).unwrap();
        }
        (d, f)
    }

    fn lignes(f: &Path) -> Vec<String> {
        std::fs::read_to_string(f).unwrap().lines().map(String::from).collect()
    }

    fn reecrire(f: &Path, l: &[String]) {
        std::fs::write(f, l.iter().map(|x| format!("{x}\n")).collect::<String>()).unwrap();
    }

    #[test]
    fn chaine_intacte() {
        let (d, f) = fichier_essai();
        assert_eq!(verifier(&f), Etat::Intacte { lignes: 4 });
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn ligne_modifiee_retiree_inseree() {
        let (d, f) = fichier_essai();
        let orig = lignes(&f);
        let mut l = orig.clone();
        l[1] = l[1].replace("ligne 1", "ligne trafiquée");
        reecrire(&f, &l);
        assert!(matches!(verifier(&f), Etat::Cassee { ligne: 3, .. }), "{:?}", verifier(&f));
        let mut l = orig.clone();
        l.remove(1);
        reecrire(&f, &l);
        assert!(matches!(verifier(&f), Etat::Cassee { ligne: 2, .. }));
        let mut l = orig.clone();
        l.insert(2, json!({ "n": 9, "prev": DEBUT }).to_string());
        reecrire(&f, &l);
        assert!(matches!(verifier(&f), Etat::Cassee { ligne: 3, .. }));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn fin_coupee_ou_derniere_modifiee() {
        let (d, f) = fichier_essai();
        let orig = lignes(&f);
        reecrire(&f, &orig[..3]);
        let e = verifier(&f);
        assert!(matches!(&e, Etat::Cassee { motif, .. } if motif.contains("coupée")), "{e:?}");
        let mut l = orig.clone();
        l[3] = l[3].replace("ligne 3", "autre");
        reecrire(&f, &l);
        let e = verifier(&f);
        assert!(matches!(&e, Etat::Cassee { motif, .. } if motif.contains("dernière")), "{e:?}");
        let mut l = orig.clone();
        l.push(json!({ "n": 4 }).to_string());
        reecrire(&f, &l);
        assert!(matches!(verifier(&f), Etat::Cassee { ligne: 5, .. }));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn trace_ancienne_puis_chainee() {
        let d = std::env::temp_dir().join(format!("orchestra-t6-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("2026-10-30.jsonl");
        std::fs::write(&f, "{\"ancien\":1}\n{\"ancien\":2}\n").unwrap();
        assert_eq!(verifier(&f), Etat::NonChainee { lignes: 2 });
        // le journal du jour continue après le lot T6 : la chaîne part de la dernière ligne ancienne
        ajouter(&f, json!({ "nouveau": 1 })).unwrap();
        ajouter(&f, json!({ "nouveau": 2 })).unwrap();
        assert_eq!(verifier(&f), Etat::Intacte { lignes: 4 });
        let _ = std::fs::remove_dir_all(d);
    }
}
