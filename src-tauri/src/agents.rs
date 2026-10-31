//! Agents : manifeste 2.0 (contrat 1, section 9 du cahier), catalogue et agents installés.
//!
//! Un agent est un programme autonome (« prestataire », D1) : Orchestra lui donne une mission
//! et reçoit son compte rendu (`contrat.rs`). Le manifeste est validé avec le schéma de l'Agent
//! Market (`contrat/schemas/manifeste.schema.json`). Les anciens agents « fiches de consignes »
//! exécutés par Orchestra lui-même ont été retirés au lot T1.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{paths::Paths, schema};

/// Vue d'un agent pour l'interface et le choix de l'agent ; le manifeste complet est dans `brut`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub capabilities: Vec<String>,
    /// Modèle demandé à la porte (`modele.demande`, ou le recommandé), « aucun » si l'agent n'en utilise pas.
    pub model: String,
    /// Consignes de la fiche (affichées dans l'inspecteur).
    pub system_prompt: String,
    /// Fonctions de l'agent (`fiche.fonctions`, D26).
    pub tools: Vec<String>,
    pub color: String,
    pub icon: String,
    /// orchestra_certified | livrable_seul | non_certifie
    pub certification: String,
    /// Agent installé lu avec un format devenu incompatible : utilisable, avec cet avertissement (D37).
    pub avertissement: String,
    /// local (transparent : pensée et actions vues par la porte) | heberge (livrable seul, lot T8)
    pub execution: String,
    #[serde(skip)]
    pub brut: Value,
    /// Dossier de l'agent installé (`agents/installes/<id>/`).
    #[serde(skip)]
    pub dossier: Option<PathBuf>,
}

/// Un type de tâche de l'agent et ses modèles de demande et de livrable (D33).
#[derive(Clone, Debug)]
pub struct Tache {
    pub r#type: String,
    pub modele_demande: String,
    pub modele_livrable: String,
}

impl Manifest {
    /// Lit et valide un manifeste 2.0. L'erreur donne les premiers motifs de refus.
    pub fn depuis_v2(v: &Value) -> Result<Manifest, String> {
        let e = schema::erreurs(v, &schema::schema(schema::MANIFESTE));
        if !e.is_empty() {
            let n = e.len();
            let mut motif = e.into_iter().take(3).collect::<Vec<_>>().join(" ; ");
            if n > 3 {
                motif.push_str(&format!(" ; et {} autre(s)", n - 3));
            }
            return Err(motif);
        }
        let s = |p: &str| v.pointer(p).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let modele = match s("/modele/demande").as_str() {
            "" | "aucun" => "aucun".to_string(),
            "inherit" => format!("inherit (recommandé : {})", s("/modele/recommande")),
            autre => autre.to_string(),
        };
        Ok(Manifest {
            id: s("/id"),
            name: s("/name"),
            version: s("/version"),
            author: v["author"]["nom"].as_str().or(v["author"].as_str()).unwrap_or("").to_string(),
            description: s("/description"),
            capabilities: v["capabilities"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
            model: modele,
            system_prompt: s("/fiche/consignes"),
            tools: v.pointer("/fiche/fonctions").and_then(|f| f.as_array()).map(|a| a.iter().filter_map(|f| f["nom"].as_str().map(String::from)).collect()).unwrap_or_default(),
            color: s("/color"),
            icon: s("/icon"),
            certification: s("/certification/statut"),
            avertissement: String::new(),
            execution: s("/execution/type"),
            brut: v.clone(),
            dossier: None,
        })
    }

    /// Lecture sans contrôle du schéma (agent installé au format devenu incompatible, D37).
    pub fn souple(v: &Value) -> Manifest {
        let s = |p: &str| v.pointer(p).and_then(|x| x.as_str()).unwrap_or("").to_string();
        Manifest {
            id: s("/id"),
            name: if s("/name").is_empty() { s("/id") } else { s("/name") },
            version: s("/version"),
            author: v["author"]["nom"].as_str().or(v["author"].as_str()).unwrap_or("").to_string(),
            description: s("/description"),
            capabilities: v["capabilities"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
            model: s("/modele/demande"),
            system_prompt: s("/fiche/consignes"),
            tools: v.pointer("/fiche/fonctions").and_then(|f| f.as_array()).map(|a| a.iter().filter_map(|f| f["nom"].as_str().map(String::from)).collect()).unwrap_or_default(),
            color: s("/color"),
            icon: s("/icon"),
            certification: s("/certification/statut"),
            avertissement: String::new(),
            execution: s("/execution/type"),
            brut: v.clone(),
            dossier: None,
        }
    }

    pub fn taches(&self) -> Vec<Tache> {
        self.brut["taches"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|t| Tache {
                        r#type: t["type"].as_str().unwrap_or("").into(),
                        modele_demande: t["modele_demande"].as_str().unwrap_or("").into(),
                        modele_livrable: t["modele_livrable"].as_str().unwrap_or("").into(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Fonctions irréversibles (préfixe `action_sensitive_`, D26) : elles passent par une validation.
    pub fn fonctions_sensibles(&self) -> Vec<String> {
        self.tools.iter().filter(|f| f.starts_with("action_sensitive_")).cloned().collect()
    }

    pub fn commande(&self) -> String {
        self.brut.pointer("/execution/commande").and_then(|c| c.as_str()).unwrap_or("").to_string()
    }

    pub fn est_local(&self) -> bool {
        self.brut.pointer("/execution/type").and_then(|c| c.as_str()) == Some("local")
    }

    /// Lit un fichier JSON du paquet de l'agent (modèle de demande ou de livrable).
    pub fn lire_modele(&self, chemin: &str) -> Option<Value> {
        let d = self.dossier.as_ref()?;
        let p = d.join(chemin);
        // jamais hors du dossier de l'agent
        if Path::new(chemin).components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return None;
        }
        serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
    }
}

/// Résultat de la lecture d'un catalogue : agents valides et entrées ignorées (avec motif).
#[derive(Default, Debug)]
pub struct Lecture {
    pub agents: Vec<Manifest>,
    pub ignores: Vec<String>,
}

/// Une entrée invalide est ignorée avec son motif ; les autres restent lisibles (lot T4).
pub fn parse_registry(text: &str) -> Lecture {
    let mut l = Lecture::default();
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        l.ignores.push("catalogue illisible (JSON invalide)".into());
        return l;
    };
    let entrees = if v.is_array() { v } else { v.get("agents").cloned().unwrap_or(json!([])) };
    for e in entrees.as_array().into_iter().flatten() {
        let id = e["id"].as_str().unwrap_or("(sans id)");
        if e.get("format_manifeste").is_none() {
            l.ignores.push(format!("{id} : ancien format (agent interne retiré au lot T1)"));
            continue;
        }
        match Manifest::depuis_v2(e) {
            Ok(m) => l.agents.push(m),
            Err(motif) => l.ignores.push(format!("{id} : {motif}")),
        }
    }
    l
}

/// Registre local (développement) ; vidé des anciens agents de démonstration au lot T1.
pub fn seed_registry(paths: &Paths) -> anyhow::Result<()> {
    let vide = json!({
        "_commentaire": "Registre local d'agents (développement) : même format que le catalogue de l'Agent Market (manifestes 2.0).",
        "_seed": SEED_VERSION,
        "agents": [],
    });
    if !paths.registre_local.exists() {
        std::fs::write(&paths.registre_local, serde_json::to_string_pretty(&vide)?)?;
        return Ok(());
    }
    let text = std::fs::read_to_string(&paths.registre_local)?;
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({}));
    if v.get("_seed").and_then(|s| s.as_u64()).unwrap_or(1) < SEED_VERSION {
        // anciens agents de démonstration : le fichier est gardé à côté, puis le registre repart vide
        std::fs::write(paths.agents.join("registre-local.v1.json"), &text)?;
        std::fs::write(&paths.registre_local, serde_json::to_string_pretty(&vide)?)?;
    }
    Ok(())
}

const SEED_VERSION: u64 = 3;

pub fn local_registry(paths: &Paths) -> Lecture {
    std::fs::read_to_string(&paths.registre_local).map(|s| parse_registry(&s)).unwrap_or_default()
}

/// Dossier d'un agent installé.
pub fn dossier_installe(paths: &Paths, id: &str) -> PathBuf {
    paths.installes.join(crate::paths::safe_name(id))
}

fn lire_installe(dossier: &Path) -> Result<Manifest, String> {
    let texte = std::fs::read_to_string(dossier.join("manifeste.json")).map_err(|e| format!("manifeste.json illisible ({e})"))?;
    let v: Value = serde_json::from_str(&texte).map_err(|e| format!("manifeste.json invalide ({e})"))?;
    let mut m = match Manifest::depuis_v2(&v) {
        Ok(m) => m,
        // D37 : un agent déjà installé dont le format n'est plus celui attendu reste utilisable,
        // avec un avertissement, s'il a au moins un id et une commande de lancement
        Err(motif) if v["id"].is_string() && v.pointer("/execution/commande").is_some_and(|c| c.is_string()) => {
            let mut m = Manifest::souple(&v);
            m.avertissement = format!("manifeste (format {}) non conforme à cette version d'Orchestra : {motif}", v["format_manifeste"].as_str().unwrap_or("absent"));
            m
        }
        Err(motif) => return Err(motif),
    };
    // certification retenue à l'installation (catalogue de l'Agent Market, lot T4)
    if let Some(c) = std::fs::read_to_string(dossier.join("installation.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|i| i["certification"].as_str().map(String::from)) {
        m.certification = c;
    }
    m.dossier = Some(dossier.to_path_buf());
    Ok(m)
}

/// Agents installés (`agents/installes/<id>/manifeste.json`) et ceux qui ne sont pas utilisables, avec motif.
pub fn installes(paths: &Paths) -> Lecture {
    let mut l = Lecture::default();
    if let Ok(rd) = std::fs::read_dir(&paths.installes) {
        for e in rd.flatten() {
            let p = e.path();
            let nom = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                match lire_installe(&p) {
                    Ok(m) => l.agents.push(m),
                    // D37 : signalé, jamais supprimé
                    Err(motif) => l.ignores.push(format!("{nom} : {motif}")),
                }
            } else if nom.ends_with(".json") {
                l.ignores.push(format!("{nom} : ancien format (agent interne retiré au lot T1)"));
            }
        }
    }
    l.agents.sort_by(|a, b| a.name.cmp(&b.name));
    l
}

pub fn installed(paths: &Paths) -> Vec<Manifest> {
    installes(paths).agents
}

pub fn get_installed(paths: &Paths, id: &str) -> Option<Manifest> {
    lire_installe(&dossier_installe(paths, id)).ok()
}

/// Contenu texte de la mémoire propre d'un agent (`<dossier>/memoire/`, D32), fichier par fichier.
pub fn instantane_memoire(dossier_agent: &Path) -> std::collections::BTreeMap<String, String> {
    let racine = dossier_agent.join("memoire");
    let mut out = std::collections::BTreeMap::new();
    fn parcourir(racine: &Path, d: &Path, out: &mut std::collections::BTreeMap<String, String>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                parcourir(racine, &p, out);
            } else {
                let rel = p.strip_prefix(racine).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                let contenu = match std::fs::metadata(&p) {
                    Ok(m) if m.len() > 1_000_000 => format!("[fichier de {} o, non comparé]", m.len()),
                    _ => std::fs::read(&p).map(|b| String::from_utf8(b).unwrap_or_else(|e| format!("[contenu binaire de {} o]", e.as_bytes().len()))).unwrap_or_default(),
                };
                out.insert(rel, contenu);
            }
        }
    }
    parcourir(&racine, &racine, &mut out);
    out
}

/// Ce qui a changé dans la mémoire de l'agent : fichier, état, lignes ajoutées et retirées.
pub fn differences_memoire(avant: &std::collections::BTreeMap<String, String>, apres: &std::collections::BTreeMap<String, String>) -> Vec<Value> {
    let mut out = vec![];
    let fichiers: std::collections::BTreeSet<&String> = avant.keys().chain(apres.keys()).collect();
    for f in fichiers {
        let (a, b) = (avant.get(f), apres.get(f));
        if a == b {
            continue;
        }
        let la: Vec<&str> = a.map(|x| x.lines().collect()).unwrap_or_default();
        let lb: Vec<&str> = b.map(|x| x.lines().collect()).unwrap_or_default();
        let ajoutees: Vec<&str> = lb.iter().filter(|l| !la.contains(l)).copied().take(50).collect();
        let retirees: Vec<&str> = la.iter().filter(|l| !lb.contains(l)).copied().take(50).collect();
        let etat = match (a, b) { (None, Some(_)) => "ajouté", (Some(_), None) => "supprimé", _ => "modifié" };
        out.push(json!({ "fichier": format!("memoire/{f}"), "etat": etat, "ajoutees": ajoutees, "retirees": retirees }));
    }
    out
}

/// Remise à zéro de la mémoire propre d'un agent (D32) : l'ancienne est archivée, jamais effacée
/// (décision de PB du 30/10), dans `agents/memoires-archivees/<id>_<date>/`.
pub fn archiver_memoire(paths: &Paths, id: &str) -> anyhow::Result<PathBuf> {
    let src = dossier_installe(paths, id).join("memoire");
    let dest = paths.agents.join("memoires-archivees").join(format!("{}_{}", crate::paths::safe_name(id), chrono::Local::now().format("%Y-%m-%d_%H%M%S")));
    std::fs::create_dir_all(dest.parent().unwrap_or(&paths.agents))?;
    if src.exists() {
        std::fs::rename(&src, &dest)?;
    } else {
        std::fs::create_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&src)?;
    Ok(dest)
}

/// Score de correspondance entre un besoin exprimé et un agent (mots communs).
pub fn match_score(m: &Manifest, need: &str) -> f32 {
    let need_words = crate::memory::keywords(need);
    if need_words.is_empty() {
        return 0.0;
    }
    let mut hay = m.capabilities.join(" ");
    hay.push(' ');
    hay.push_str(&m.name);
    hay.push(' ');
    hay.push_str(&m.description);
    if let Some(sf) = m.brut.pointer("/fiche/sait_faire").and_then(|s| s.as_array()) {
        for s in sf.iter().filter_map(|s| s.as_str()) {
            hay.push(' ');
            hay.push_str(s);
        }
    }
    let hay_words = crate::memory::keywords(&hay);
    let mut score = 0.0;
    for w in &need_words {
        if m.capabilities.iter().any(|c| crate::memory::fold(c).trim() == w) {
            score += 2.0;
        } else if hay_words.contains(w) {
            score += 1.0;
        } else if hay_words.iter().any(|h| h.starts_with(&w[..w.len().min(5)]) || w.starts_with(&h[..h.len().min(5)])) {
            score += 0.5;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifeste_date() -> Value {
        json!({
            "format_manifeste": "2.0", "id": "agent-date", "name": "Date du jour", "version": "1.0.0",
            "author": { "nom": "PB", "contact": "https://github.com/PBdan-dev" },
            "description": "Renvoie la date et l'heure du jour.", "categorie": "utilitaire",
            "capabilities": ["date", "heure"],
            "fiche": { "sait_faire": ["Donner la date"], "ne_sait_pas_faire": ["Calculer une date", "Gérer un agenda"], "fonctions": [], "consignes": "Rien." },
            "taches": [{ "type": "date-du-jour", "modele_demande": "modeles/date-du-jour.demande.json", "modele_livrable": "modeles/date-du-jour.livrable.json" }],
            "execution": { "type": "local", "systemes": ["windows-x64"], "commande": "python agent.py", "protocole": "stdio-json" },
            "modele": { "via": "porte", "demande": "aucun", "minimum": "aucun", "recommande": "aucun" },
            "ressources": { "vram_go": 0, "gpu_exclusif": false }, "reseau_declare": [], "acces_declares": { "fichiers": "aucun" },
            "identifiants": [], "memoire_propre": false, "compatibilite": { "orchestra_min": "1.0.0", "contrat": 1 },
            "certification": { "statut": "non_certifie" }, "licence": "MIT", "color": "#f6e05e", "icon": "calendar"
        })
    }

    #[test]
    fn manifeste_v2_lu() {
        let m = Manifest::depuis_v2(&manifeste_date()).unwrap();
        assert_eq!(m.author, "PB");
        assert_eq!(m.model, "aucun");
        assert_eq!(m.taches()[0].r#type, "date-du-jour");
        assert_eq!(m.commande(), "python agent.py");
        assert!(m.est_local());
    }

    #[test]
    fn catalogue_tolerant() {
        let mut faux = manifeste_date();
        faux["id"] = json!("Agent Faux");
        faux["version"] = json!("1.0");
        let texte = json!({ "agents": [manifeste_date(), faux, { "id": "agent-redacteur", "name": "Ancien" }] }).to_string();
        let l = parse_registry(&texte);
        assert_eq!(l.agents.len(), 1);
        assert_eq!(l.ignores.len(), 2);
        assert!(l.ignores.iter().any(|i| i.starts_with("Agent Faux") && i.contains("version")));
        assert!(l.ignores.iter().any(|i| i.contains("ancien format")));
    }

    #[test]
    fn memoire_comparee() {
        let mut a = std::collections::BTreeMap::new();
        a.insert("ton.md".to_string(), "cordial
signe PB".to_string());
        let mut b = a.clone();
        b.insert("ton.md".into(), "cordial
signe Pierre".into());
        b.insert("regles.md".into(), "x".into());
        let d = differences_memoire(&a, &b);
        assert_eq!(d.len(), 2);
        let ton = d.iter().find(|x| x["fichier"] == "memoire/ton.md").unwrap();
        assert_eq!(ton["ajoutees"], json!(["signe Pierre"]));
        assert_eq!(ton["retirees"], json!(["signe PB"]));
    }

    #[test]
    fn fonctions_sensibles() {
        let mut v = manifeste_date();
        v["fiche"]["fonctions"] = json!([
            { "nom": "action_deplacer", "description": "x", "expose_au_modele": false },
            { "nom": "action_sensitive_supprimer", "description": "y", "expose_au_modele": false }
        ]);
        let m = Manifest::depuis_v2(&v).unwrap();
        assert_eq!(m.fonctions_sensibles(), vec!["action_sensitive_supprimer".to_string()]);
    }
}
