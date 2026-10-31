//! Vérification d'une mission et confiance dans les agents (lot T5).
//!
//! Après chaque mission, Orchestra compare ce que l'agent déclare (compte rendu) à ce qu'il
//! constate lui-même : fichiers du projet (empreintes avant et après), appels d'outils vus par
//! la porte, validations accordées par PB, mémoire propre de l'agent, livrable contrôlé avec son
//! modèle (D33). Chaque différence est un écart. Un échec avoué n'est pas un écart.
//!
//! Confiance (`agents/confiance.json`, décisions de PB du 31/10) : 1 écart → l'agent perd sa
//! place parmi les « Agents de confiance » et ne la regagne que sur un clic « rétablir » de PB ;
//! 3 écarts au total → quarantaine (installé, jamais choisi) jusqu'à ce que PB la lève, ce qui
//! remet le compteur à zéro (l'historique est gardé). Un agent non certifié n'est jamais « de confiance ».

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{agents::Manifest, paths::Paths, schema};

/// Au-delà, un fichier est comparé par taille et date de modification, sans être lu.
const LECTURE_MAX: u64 = 200_000_000;
const FICHIERS_MAX: usize = 50_000;
/// Écarts au total avant la quarantaine.
pub const SEUIL_QUARANTAINE: u32 = 3;

/// Empreinte de chaque fichier d'un dossier : chemin relatif (avec `/`) → empreinte.
pub type Empreintes = BTreeMap<String, String>;

pub fn empreintes(dossier: &Path) -> Empreintes {
    let mut out = Empreintes::new();
    fn parcourir(racine: &Path, d: &Path, out: &mut Empreintes) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            if out.len() >= FICHIERS_MAX {
                return;
            }
            let p = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                parcourir(racine, &p, out);
                continue;
            }
            let rel = p.strip_prefix(racine).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            let empreinte = if meta.len() > LECTURE_MAX {
                let date = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                format!("taille {} · modifié {date}", meta.len())
            } else {
                let mut h = Sha256::new();
                let mut buf = vec![0u8; 1 << 16];
                match std::fs::File::open(&p) {
                    Ok(mut f) => loop {
                        match f.read(&mut buf) {
                            Ok(0) => break format!("{:x}", h.finalize()),
                            Ok(n) => h.update(&buf[..n]),
                            Err(_) => break "illisible".into(),
                        }
                    },
                    Err(_) => "illisible".into(),
                }
            };
            out.insert(rel, empreinte);
        }
    }
    parcourir(dossier, dossier, &mut out);
    out
}

/// Ce qu'Orchestra a constaté pendant la mission, à comparer au compte rendu.
pub struct Constats<'a> {
    pub agent: &'a Manifest,
    pub mission: &'a Value,
    pub compte_rendu: &'a Value,
    pub projet: &'a Path,
    pub avant: &'a Empreintes,
    pub apres: &'a Empreintes,
    /// Appels d'outils du modèle de l'agent, vus par la porte (noms des fonctions).
    pub appels_observes: &'a [String],
    /// Écarts signalés par la porte (outil hors liste, action sans raison…).
    pub ecarts_porte: &'a [String],
    /// Écarts au contrat relevés pendant l'échange (messages mal formés…).
    pub ecarts_contrat: &'a [String],
    /// Validations réglées : (action, accordée).
    pub validations: &'a [(String, bool)],
    /// Fichiers de la mémoire propre de l'agent qui ont changé (`memoire/…`).
    pub memoire_changee: &'a [String],
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Verification {
    pub ecarts: Vec<String>,
    /// Ce qui a été contrôlé (affiché avec le résultat).
    pub controles: Vec<String>,
}

/// Chemin d'un fichier annoncé, relatif au dossier du projet, avec `/`.
fn relatif(projet: &Path, f: &str) -> String {
    let p = Path::new(f);
    let rel: PathBuf = if p.is_absolute() { p.strip_prefix(projet).map(Path::to_path_buf).unwrap_or_else(|_| p.to_path_buf()) } else { p.to_path_buf() };
    rel.to_string_lossy().replace('\\', "/").trim_start_matches("./").to_string()
}

fn chaines(v: &Value, pointeur: &str) -> Vec<String> {
    v.pointer(pointeur).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

pub fn verifier(c: &Constats) -> Verification {
    let mut v = Verification::default();
    let cr = c.compte_rendu;
    let etat = cr["etat"].as_str().unwrap_or("echec");

    // 1. Fichiers du projet : annoncés contre constatés
    let annonces: BTreeSet<String> = chaines(cr, "/resultat/fichiers").iter().map(|f| relatif(c.projet, f)).collect();
    for f in &annonces {
        let dans = |e: &Empreintes| e.contains_key(f) || e.keys().any(|k| k.starts_with(&format!("{}/", f.trim_end_matches('/'))));
        let present = dans(c.apres) || c.projet.join(f).exists();
        let supprime = dans(c.avant) && !present;
        if !present && !supprime {
            v.ecarts.push(format!("fichier annoncé mais absent : {f}"));
        }
    }
    let changes: BTreeSet<&String> = c.avant.keys().chain(c.apres.keys()).filter(|f| c.avant.get(*f) != c.apres.get(*f)).collect();
    for f in changes {
        let ancetre_annonce = annonces.iter().any(|a| f.starts_with(&format!("{}/", a.trim_end_matches('/'))));
        if !annonces.contains(f) && !ancetre_annonce {
            let quoi = match (c.avant.contains_key(f), c.apres.contains_key(f)) {
                (false, true) => "créé",
                (true, false) => "supprimé",
                _ => "modifié",
            };
            v.ecarts.push(format!("fichier {quoi} sans être annoncé : {f}"));
        }
    }
    v.controles.push(format!("fichiers du projet : {} avant, {} après, {} annoncé(s)", c.avant.len(), c.apres.len(), annonces.len()));

    // 2. Actions déclarées contre appels vus par la porte, et liste des fonctions de l'agent
    let fonctions: BTreeMap<String, bool> = c
        .agent
        .brut
        .pointer("/fiche/fonctions")
        .and_then(|f| f.as_array())
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f["nom"].as_str()?.to_string(), f["expose_au_modele"].as_bool().unwrap_or(false))))
        .collect();
    let mut declarees: BTreeMap<String, usize> = BTreeMap::new();
    for a in cr["actions_declarees"].as_array().into_iter().flatten() {
        let nom = a["action"].as_str().unwrap_or("").to_string();
        if (nom.starts_with("action_") || nom.starts_with("secret_") || nom.starts_with("memory_") || nom.starts_with("read_")) && !fonctions.contains_key(&nom) {
            v.ecarts.push(format!("action déclarée hors de la liste des fonctions de l'agent : {nom}"));
        }
        *declarees.entry(nom).or_default() += 1;
    }
    let mut observees: BTreeMap<String, usize> = BTreeMap::new();
    for a in c.appels_observes {
        *observees.entry(a.clone()).or_default() += 1;
    }
    for (nom, n) in &declarees {
        if fonctions.get(nom) == Some(&true) && observees.get(nom).copied().unwrap_or(0) < *n {
            v.ecarts.push(format!("action déclarée {n} fois mais vue {} fois par la porte : {nom}", observees.get(nom).copied().unwrap_or(0)));
        }
    }
    for (nom, n) in &observees {
        if nom.starts_with("action_") && declarees.get(nom).copied().unwrap_or(0) < *n {
            v.ecarts.push(format!("action vue {n} fois par la porte mais déclarée {} fois : {nom}", declarees.get(nom).copied().unwrap_or(0)));
        }
    }
    v.ecarts.extend(c.ecarts_porte.iter().map(|e| format!("porte du modèle : {e}")));
    v.controles.push(format!("actions : {} déclarée(s), {} appel(s) vu(s) par la porte", declarees.values().sum::<usize>(), c.appels_observes.len()));

    // 3. Actions irréversibles : chacune doit avoir été accordée par PB (D28)
    for (nom, n) in declarees.iter().filter(|(nom, _)| nom.starts_with("action_sensitive_")) {
        let accordees = c.validations.iter().filter(|(a, ok)| a == nom && *ok).count();
        let refusees = c.validations.iter().filter(|(a, ok)| a == nom && !*ok).count();
        if accordees < *n {
            if refusees > 0 {
                v.ecarts.push(format!("action irréversible faite malgré un refus : {nom}"));
            } else {
                v.ecarts.push(format!("action irréversible faite sans validation : {nom}"));
            }
        }
    }
    v.controles.push(format!("validations : {} accordée(s), {} refusée(s)", c.validations.iter().filter(|x| x.1).count(), c.validations.iter().filter(|x| !x.1).count()));

    // 4. Mémoire propre de l'agent : changements déclarés dans memoire_modifiee (D32)
    let declares: BTreeSet<String> = chaines(cr, "/memoire_modifiee").iter().map(|f| f.replace('\\', "/").trim_start_matches("memoire/").to_string()).collect();
    for f in c.memoire_changee {
        let court = f.trim_start_matches("memoire/");
        if c.agent.brut["memoire_propre"].as_bool() != Some(true) {
            v.ecarts.push(format!("mémoire modifiée alors que l'agent n'en déclare pas : {f}"));
        } else if !declares.contains(court) {
            v.ecarts.push(format!("mémoire de l'agent modifiée sans le dire : {f}"));
        }
    }
    v.controles.push(format!("mémoire de l'agent : {} fichier(s) changé(s), {} déclaré(s)", c.memoire_changee.len(), declares.len()));

    // 5. Livrable : contrôlé avec le modèle de livrable du type de tâche (D33)
    if etat == "partiel" && chaines(cr, "/manquant").is_empty() {
        v.ecarts.push("état « partiel » sans la liste de ce qui manque".into());
    }
    if etat == "termine" || etat == "partiel" {
        let type_tache = c.mission["type_tache"].as_str().unwrap_or("");
        match c.agent.taches().into_iter().find(|t| t.r#type == type_tache).and_then(|t| c.agent.lire_modele(&t.modele_livrable)) {
            Some(modele) => {
                let e = schema::erreurs(&cr["resultat"], &modele);
                for x in e.iter().take(3) {
                    v.ecarts.push(format!("livrable non conforme au modèle « {type_tache} » : {x}"));
                }
                v.controles.push(format!("livrable contrôlé avec le modèle « {type_tache} »"));
            }
            None => v.controles.push("livrable : pas de modèle de livrable lisible pour ce type de tâche (non contrôlé)".into()),
        }
    }

    // 6. Écarts au contrat pendant l'échange
    v.ecarts.extend(c.ecarts_contrat.iter().map(|e| format!("contrat : {e}")));
    v
}

/// `verification.md` du dossier de mission.
pub fn rapport(agent: &Manifest, mission_id: &str, etat: &str, v: &Verification, confiance: &EtatConfiance) -> String {
    let mut s = format!("# Vérification de la mission {mission_id}\n\nAgent : {} ({}) · état annoncé : {etat}\n\n", agent.name, agent.id);
    if v.ecarts.is_empty() {
        s.push_str("## Résultat : aucun écart\n\n");
    } else {
        s.push_str(&format!("## Résultat : {} écart(s)\n\n", v.ecarts.len()));
        for e in &v.ecarts {
            s.push_str(&format!("- {e}\n"));
        }
        s.push('\n');
    }
    s.push_str("## Contrôlé\n\n");
    for c in &v.controles {
        s.push_str(&format!("- {c}\n"));
    }
    s.push_str(&format!(
        "\n## Confiance\n\n{} · {} écart(s) au total depuis la dernière levée de quarantaine (quarantaine à {SEUIL_QUARANTAINE}).\n",
        confiance.libelle(),
        confiance.ecarts
    ));
    s
}

// ---------- Confiance (agents/confiance.json) ----------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Statut {
    /// Peut être promu « de confiance » après une mission réussie sans écart.
    Normal,
    /// A perdu la confiance : utilisable, mais plus jamais promu sans un « rétablir » de PB.
    Retire,
    /// Installé mais jamais choisi par Orchestra, jusqu'à la levée par PB.
    Quarantaine,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EtatConfiance {
    pub statut: Statut,
    /// Écarts depuis la dernière levée de quarantaine.
    pub ecarts: u32,
    #[serde(default)]
    pub historique: Vec<Value>,
}

impl Default for EtatConfiance {
    fn default() -> Self {
        EtatConfiance { statut: Statut::Normal, ecarts: 0, historique: vec![] }
    }
}

impl EtatConfiance {
    pub fn libelle(&self) -> &'static str {
        match self.statut {
            Statut::Normal => "peut être de confiance",
            Statut::Retire => "confiance retirée (à rétablir par PB)",
            Statut::Quarantaine => "en quarantaine (à lever par PB)",
        }
    }
}

fn fichier(paths: &Paths) -> PathBuf {
    paths.agents.join("confiance.json")
}

pub fn lire(paths: &Paths) -> BTreeMap<String, EtatConfiance> {
    std::fs::read_to_string(fichier(paths)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| serde_json::from_value(v["agents"].clone()).ok()).unwrap_or_default()
}

fn ecrire(paths: &Paths, tout: &BTreeMap<String, EtatConfiance>) -> anyhow::Result<()> {
    let v = json!({
        "_commentaire": "Confiance d'Orchestra dans ses agents (lot T5) : écarts relevés après chaque mission, quarantaine à 3, levée et rétablissement par PB dans le panneau Agents.",
        "agents": tout,
    });
    std::fs::write(fichier(paths), serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

pub fn etat(paths: &Paths, id: &str) -> EtatConfiance {
    lire(paths).remove(id).unwrap_or_default()
}

/// Peut-on choisir cet agent pour une mission ?
pub fn utilisable(paths: &Paths, id: &str) -> bool {
    etat(paths, id).statut != Statut::Quarantaine
}

/// Peut-il être (ou rester) parmi les « Agents de confiance » ? Jamais s'il n'est pas certifié.
pub fn peut_etre_de_confiance(paths: &Paths, m: &Manifest) -> bool {
    m.certification == "orchestra_certified" && etat(paths, &m.id).statut == Statut::Normal
}

/// Enregistre le résultat de la vérification d'une mission ; renvoie le nouvel état.
pub fn enregistrer(paths: &Paths, agent_id: &str, mission_id: &str, v: &Verification) -> anyhow::Result<EtatConfiance> {
    let mut tout = lire(paths);
    let e = tout.entry(agent_id.to_string()).or_default();
    if !v.ecarts.is_empty() {
        e.ecarts += v.ecarts.len() as u32;
        if e.statut == Statut::Normal {
            e.statut = Statut::Retire;
        }
        if e.ecarts >= SEUIL_QUARANTAINE {
            e.statut = Statut::Quarantaine;
        }
        e.historique.push(json!({ "date": chrono::Local::now().to_rfc3339(), "mission_id": mission_id, "ecarts": v.ecarts, "statut": e.statut }));
    }
    let r = e.clone();
    ecrire(paths, &tout)?;
    Ok(r)
}

/// Levée de la quarantaine par PB : compteur remis à zéro, la confiance reste à rétablir.
pub fn lever_quarantaine(paths: &Paths, agent_id: &str) -> anyhow::Result<()> {
    let mut tout = lire(paths);
    let e = tout.entry(agent_id.to_string()).or_default();
    e.ecarts = 0;
    e.statut = Statut::Retire;
    e.historique.push(json!({ "date": chrono::Local::now().to_rfc3339(), "levee_quarantaine": true }));
    ecrire(paths, &tout)
}

/// Rétablissement de la confiance par PB (hors quarantaine).
pub fn retablir(paths: &Paths, agent_id: &str) -> anyhow::Result<()> {
    let mut tout = lire(paths);
    let e = tout.entry(agent_id.to_string()).or_default();
    if e.statut == Statut::Quarantaine {
        anyhow::bail!("lève d'abord la quarantaine");
    }
    e.statut = Statut::Normal;
    e.historique.push(json!({ "date": chrono::Local::now().to_rfc3339(), "confiance_retablie": true }));
    ecrire(paths, &tout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(fonctions: Value, memoire: bool) -> Manifest {
        Manifest::souple(&json!({
            "id": "agent-essai", "name": "Essai", "version": "1.0.0",
            "fiche": { "fonctions": fonctions }, "memoire_propre": memoire,
            "execution": { "commande": "python agent.py" }
        }))
    }

    fn emp(paires: &[(&str, &str)]) -> Empreintes {
        paires.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn verif(m: &Manifest, cr: Value, avant: &Empreintes, apres: &Empreintes, obs: &[&str], val: &[(&str, bool)], mem: &[&str]) -> Verification {
        let projet = std::env::temp_dir().join("orchestra-t5-inexistant");
        let obs: Vec<String> = obs.iter().map(|s| s.to_string()).collect();
        let val: Vec<(String, bool)> = val.iter().map(|(a, b)| (a.to_string(), *b)).collect();
        let mem: Vec<String> = mem.iter().map(|s| s.to_string()).collect();
        verifier(&Constats {
            agent: m,
            mission: &json!({}),
            compte_rendu: &cr,
            projet: &projet,
            avant,
            apres,
            appels_observes: &obs,
            ecarts_porte: &[],
            ecarts_contrat: &[],
            validations: &val,
            memoire_changee: &mem,
        })
    }

    fn cr(etat: &str, fichiers: Value, actions: Value) -> Value {
        json!({ "etat": etat, "resultat": { "texte": "", "fichiers": fichiers }, "actions_declarees": actions })
    }

    #[test]
    fn mission_honnete_sans_ecart() {
        let m = agent(json!([]), false);
        let v = verif(&m, cr("termine", json!(["note.md"]), json!([])), &emp(&[]), &emp(&[("note.md", "a")]), &[], &[], &[]);
        assert!(v.ecarts.is_empty(), "{:?}", v.ecarts);
    }

    #[test]
    fn fichier_annonce_absent_et_fichier_cache() {
        let m = agent(json!([]), false);
        let v = verif(&m, cr("termine", json!(["rapport.md"]), json!([])), &emp(&[("ancien.md", "a")]), &emp(&[("ancien.md", "b"), ("cache.txt", "c")]), &[], &[], &[]);
        assert_eq!(v.ecarts.len(), 3, "{:?}", v.ecarts);
        assert!(v.ecarts.iter().any(|e| e.contains("annoncé mais absent : rapport.md")));
        assert!(v.ecarts.iter().any(|e| e.contains("créé sans être annoncé : cache.txt")));
        assert!(v.ecarts.iter().any(|e| e.contains("modifié sans être annoncé : ancien.md")));
    }

    #[test]
    fn suppression_et_renommage_annonces() {
        let m = agent(json!([]), false);
        let v = verif(&m, cr("termine", json!(["vieux.md", "a.md", "b.md", "dossier"]), json!([])), &emp(&[("vieux.md", "x"), ("a.md", "y"), ("dossier/c.md", "z")]), &emp(&[("b.md", "y"), ("dossier/c.md", "w")]), &[], &[], &[]);
        assert!(v.ecarts.is_empty(), "{:?}", v.ecarts);
    }

    #[test]
    fn actions_et_validations() {
        let fonctions = json!([
            { "nom": "action_ecrire", "description": "", "expose_au_modele": true },
            { "nom": "action_sensitive_supprimer", "description": "", "expose_au_modele": false }
        ]);
        let m = agent(fonctions, false);
        let a = |n: &str| json!({ "t": "", "action": n, "detail": "", "raison": "r" });
        // déclarée par le modèle mais jamais vue par la porte ; vue mais pas déclarée
        let v = verif(&m, cr("echec", json!([]), json!([a("action_ecrire")])), &emp(&[]), &emp(&[]), &[], &[], &[]);
        assert!(v.ecarts.iter().any(|e| e.contains("vue 0 fois")), "{:?}", v.ecarts);
        let v = verif(&m, cr("echec", json!([]), json!([])), &emp(&[]), &emp(&[]), &["action_ecrire"], &[], &[]);
        assert!(v.ecarts.iter().any(|e| e.contains("déclarée 0 fois")), "{:?}", v.ecarts);
        // irréversible : accordée, sans validation, malgré un refus
        let v = verif(&m, cr("echec", json!([]), json!([a("action_sensitive_supprimer")])), &emp(&[]), &emp(&[]), &[], &[("action_sensitive_supprimer", true)], &[]);
        assert!(v.ecarts.is_empty(), "{:?}", v.ecarts);
        let v = verif(&m, cr("echec", json!([]), json!([a("action_sensitive_supprimer")])), &emp(&[]), &emp(&[]), &[], &[], &[]);
        assert!(v.ecarts.iter().any(|e| e.contains("sans validation")));
        let v = verif(&m, cr("echec", json!([]), json!([a("action_sensitive_supprimer")])), &emp(&[]), &emp(&[]), &[], &[("action_sensitive_supprimer", false)], &[]);
        assert!(v.ecarts.iter().any(|e| e.contains("malgré un refus")));
        // hors liste
        let v = verif(&m, cr("echec", json!([]), json!([a("action_sensitive_payer")])), &emp(&[]), &emp(&[]), &[], &[("action_sensitive_payer", true)], &[]);
        assert!(v.ecarts.iter().any(|e| e.contains("hors de la liste")));
    }

    #[test]
    fn memoire_et_partiel() {
        let m = agent(json!([]), true);
        let mut r = cr("partiel", json!([]), json!([]));
        r["memoire_modifiee"] = json!(["memoire/ton.md"]);
        let v = verif(&m, r, &emp(&[]), &emp(&[]), &[], &[], &["memoire/ton.md", "memoire/secret.md"]);
        assert_eq!(v.ecarts.len(), 2, "{:?}", v.ecarts);
        assert!(v.ecarts.iter().any(|e| e.contains("sans le dire : memoire/secret.md")));
        assert!(v.ecarts.iter().any(|e| e.contains("partiel")));
    }

    #[test]
    fn confiance_retrait_puis_quarantaine() {
        let racine = std::env::temp_dir().join(format!("orchestra-t5-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let paths = Paths::at(racine.clone()).unwrap();
        let un = Verification { ecarts: vec!["x".into()], controles: vec![] };
        let e = enregistrer(&paths, "a", "m1", &Verification::default()).unwrap();
        assert_eq!(e.statut, Statut::Normal);
        assert_eq!(enregistrer(&paths, "a", "m2", &un).unwrap().statut, Statut::Retire);
        assert!(utilisable(&paths, "a"));
        assert_eq!(enregistrer(&paths, "a", "m3", &un).unwrap().statut, Statut::Retire);
        assert_eq!(enregistrer(&paths, "a", "m4", &un).unwrap().statut, Statut::Quarantaine);
        assert!(!utilisable(&paths, "a"));
        assert!(retablir(&paths, "a").is_err());
        lever_quarantaine(&paths, "a").unwrap();
        let e = etat(&paths, "a");
        assert_eq!((e.statut, e.ecarts), (Statut::Retire, 0));
        assert_eq!(e.historique.len(), 4);
        retablir(&paths, "a").unwrap();
        assert_eq!(etat(&paths, "a").statut, Statut::Normal);
        let _ = std::fs::remove_dir_all(racine);
    }
}

/// Bilan d'une mission vérifiée, pour Conscience et le dossier de mission.
pub struct Bilan {
    pub verification: Verification,
    pub confiance: EtatConfiance,
    /// Lignes retirées des « Agents de confiance » de la mémoire d'Orchestra.
    pub confiance_retiree: Vec<String>,
}

/// Vérifie la mission, écrit `verification.md`, met la confiance à jour et, en cas d'écart,
/// retire l'agent des « Agents de confiance ».
pub fn apres_mission(paths: &Paths, dossier_mission: &Path, mission_id: &str, c: &Constats) -> anyhow::Result<Bilan> {
    let verification = verifier(c);
    let confiance = enregistrer(paths, &c.agent.id, mission_id, &verification)?;
    let confiance_retiree = if confiance.statut != Statut::Normal { crate::memory::retirer_confiance(paths, &c.agent.id)? } else { vec![] };
    let mut texte = rapport(c.agent, mission_id, c.compte_rendu["etat"].as_str().unwrap_or("echec"), &verification, &confiance);
    if !confiance_retiree.is_empty() {
        texte.push_str("\nRetiré des « Agents de confiance » de la mémoire d'Orchestra :\n");
        for l in &confiance_retiree {
            texte.push_str(&format!("{l}\n"));
        }
    }
    std::fs::write(dossier_mission.join("verification.md"), texte)?;
    Ok(Bilan { verification, confiance, confiance_retiree })
}
