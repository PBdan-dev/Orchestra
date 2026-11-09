//! Ce qu'il manque à un agent pour travailler (retour de PB du 03/11 : Mail Adagio s'est arrêté
//! faute d'adresse Gmail et de mot de passe d'application, et Orchestra les a demandés dans le chat).
//!
//! Quand une mission finit « bloquée », Orchestra relève les champs qui manquent :
//! - les identifiants déclarés par le manifeste et absents du coffre de Windows (secrets) ;
//! - les champs de données que l'agent nomme dans son motif (`donnees.<champ>`), décrits par
//!   ses modèles de demande (réglages simples, comme une adresse).
//!
//! L'interface ouvre alors un formulaire. Les secrets vont directement dans le coffre (jamais dans
//! un fichier, le chat ou le journal) ; les réglages dans `agents/reglages/<id>.json`, puis ils
//! complètent les données de chaque mission de cet agent.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{agents::Manifest, installation, paths::Paths};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Champ {
    pub nom: String,
    /// « identifiant » (secret, rangé dans le coffre) ou « reglage » (donnée simple, gardée en clair)
    pub genre: String,
    /// type d'identifiant (mot_de_passe_application, cle_api…) ou « texte »
    pub r#type: String,
    /// pourquoi l'agent en a besoin (manifeste ou modèle de demande)
    pub raison: String,
    /// sans lui, l'agent reste bloqué
    pub requis: bool,
    /// valeur actuelle d'un réglage (jamais celle d'un identifiant)
    pub valeur: String,
    /// déjà renseigné (réglage gardé, ou identifiant présent dans le coffre)
    pub present: bool,
}

/// Un nom de champ qui ressemble à un secret n'est jamais gardé en clair comme réglage.
fn ressemble_a_un_secret(nom: &str) -> bool {
    let n = nom.to_lowercase();
    ["mot_de_passe", "password", "passwd", "secret", "token", "jeton", "cle", "key", "code"].iter().any(|s| n.contains(s))
}

fn fichier_reglages(dossier_agents: &Path, agent_id: &str) -> PathBuf {
    dossier_agents.join("reglages").join(format!("{}.json", crate::paths::safe_name(agent_id)))
}

/// `agents/` à partir du dossier d'un agent installé (`agents/installes/<id>/`).
fn dossier_agents(m: &Manifest) -> Option<PathBuf> {
    m.dossier.as_ref()?.parent()?.parent().map(Path::to_path_buf)
}

pub fn lire_reglages(dossier_agents: &Path, agent_id: &str) -> Map<String, Value> {
    std::fs::read_to_string(fichier_reglages(dossier_agents, agent_id))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

pub fn enregistrer_reglages(paths: &Paths, agent_id: &str, valeurs: &Map<String, Value>) -> Result<(), String> {
    if let Some(k) = valeurs.keys().find(|k| ressemble_a_un_secret(k)) {
        return Err(format!("« {k} » ressemble à un secret : il doit être déclaré comme identifiant de l'agent, pas gardé en clair"));
    }
    let mut r = lire_reglages(&paths.agents, agent_id);
    for (k, v) in valeurs {
        match v.as_str().map(str::trim) {
            Some("") => {
                r.remove(k);
            }
            Some(s) => {
                r.insert(k.clone(), Value::String(s.to_string()));
            }
            None => {
                r.insert(k.clone(), v.clone());
            }
        }
    }
    let f = fichier_reglages(&paths.agents, agent_id);
    std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&f, serde_json::to_string_pretty(&Value::Object(r)).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Complète les données d'une mission avec les réglages gardés pour l'agent : seulement les champs
/// que son modèle de demande connaît et que la mission laisse vides.
pub fn completer_donnees(m: &Manifest, schema_demande: &Value, donnees: &mut Value) -> Vec<String> {
    let Some(dossier) = dossier_agents(m) else { return vec![] };
    let reglages = lire_reglages(&dossier, &m.id);
    let Some(props) = schema_demande["properties"].as_object() else { return vec![] };
    if !donnees.is_object() {
        *donnees = Value::Object(Map::new());
    }
    let mut ajoutes = vec![];
    for (k, v) in reglages {
        let vide = donnees.get(&k).is_none_or(|x| x.is_null() || x.as_str().is_some_and(|s| s.trim().is_empty()));
        if props.contains_key(&k) && vide {
            donnees[&k] = v;
            ajoutes.push(k);
        }
    }
    ajoutes
}

/// Description d'un champ de données dans les modèles de demande de l'agent.
fn description_donnee(m: &Manifest, nom: &str) -> Option<String> {
    m.taches().iter().find_map(|t| {
        let s = m.lire_modele(&t.modele_demande)?;
        s.pointer(&format!("/properties/{nom}")).map(|p| p["description"].as_str().unwrap_or("").to_string())
    })
}

/// Champs à demander à PB après une mission bloquée (vide : rien que PB puisse renseigner).
pub fn champs_manquants(m: &Manifest, motif: &str) -> Vec<Champ> {
    let mut champs = vec![];
    // réglages nommés par l'agent : « indiquez-la dans donnees.compte »
    let re = Regex::new(r"donnees\.([A-Za-z0-9_]+)").unwrap();
    let reglages = dossier_agents(m).map(|d| lire_reglages(&d, &m.id)).unwrap_or_default();
    for c in re.captures_iter(motif) {
        let nom = c[1].to_string();
        if ressemble_a_un_secret(&nom) || champs.iter().any(|x: &Champ| x.nom == nom) {
            continue;
        }
        if let Some(raison) = description_donnee(m, &nom) {
            let valeur = reglages.get(&nom).and_then(|v| v.as_str()).unwrap_or("").to_string();
            champs.push(Champ { nom, genre: "reglage".into(), r#type: "texte".into(), raison, requis: true, present: !valeur.is_empty(), valeur });
        }
    }
    // identifiants absents du coffre : requis si l'agent les nomme, ou si ce sont des mots de passe ou des jetons
    let f = crate::memory::fold(motif);
    let bloque_par_le_coffre = f.contains("coffre") || f.contains("identifiant");
    for i in installation::identifiants(m).into_iter().filter(|i| !i.present) {
        let nomme = motif.contains(&i.nom);
        let requis = nomme || (i.r#type != "cle_api" && (bloque_par_le_coffre || !champs.is_empty()));
        champs.push(Champ { nom: i.nom, genre: "identifiant".into(), r#type: i.r#type, raison: i.raison, requis, valeur: String::new(), present: false });
    }
    // rien de requis : le blocage vient d'ailleurs, pas d'un champ que PB peut remplir
    if !champs.iter().any(|c| c.requis) {
        return vec![];
    }
    champs
}

/// Programme de configuration livré avec l'agent (`configurer.bat`…), lancé par PB depuis Orchestra :
/// il règle ce que le manifeste ne peut pas décrire (adresses des appareils, appairage). Retour de PB du
/// 09/11 : Maison connectée installée, mais jamais configurée (pont Hue introuvable).
pub fn configurateur(m: &Manifest) -> Option<String> {
    let d = m.dossier.as_ref()?;
    ["configurer.bat", "configurer.cmd", "configurer.ps1"].iter().find(|f| d.join(f).is_file()).map(|f| f.to_string())
}

/// Le compte rendu dit-il que l'agent n'est pas configuré ?
pub fn reclame_configuration(texte: &str) -> bool {
    let f = crate::memory::fold(texte);
    ["configurer.bat", "non configure", "pas configure", "lancer configurer", "lancez configurer", "a configurer"].iter().any(|m| f.contains(m))
}

/// Noms de données qui sont des réglages durables (un compte, une adresse), pas le détail d'une mission.
const NOMS_DE_REGLAGE: [&str; 9] = ["compte", "email", "mail", "adresse", "utilisateur", "login", "serveur", "url", "calendrier"];

/// Tout ce dont l'agent a besoin pour fonctionner, avec son état, pour les fiches de l'interface :
/// ses identifiants (renseignés ou non dans le coffre) et ses réglages (déjà gardés, ou des champs
/// de ses modèles de demande qui ressemblent à un compte ou une adresse).
pub fn champs_agent(m: &Manifest) -> Vec<Champ> {
    let mut champs: Vec<Champ> = vec![];
    let reglages = dossier_agents(m).map(|d| lire_reglages(&d, &m.id)).unwrap_or_default();
    for t in m.taches() {
        let Some(s) = m.lire_modele(&t.modele_demande) else { continue };
        for (nom, p) in s["properties"].as_object().into_iter().flatten() {
            let n = nom.to_lowercase();
            let reglage = reglages.contains_key(nom) || NOMS_DE_REGLAGE.iter().any(|r| n == *r || n.starts_with(&format!("{r}_")) || n.ends_with(&format!("_{r}")));
            if !reglage || ressemble_a_un_secret(nom) || p["type"].as_str() != Some("string") || champs.iter().any(|c| &c.nom == nom) {
                continue;
            }
            let valeur = reglages.get(nom).and_then(|v| v.as_str()).unwrap_or("").to_string();
            champs.push(Champ { nom: nom.clone(), genre: "reglage".into(), r#type: "texte".into(), raison: p["description"].as_str().unwrap_or("").to_string(), requis: false, present: !valeur.is_empty(), valeur });
        }
    }
    for i in installation::identifiants(m) {
        let requis = i.r#type != "cle_api";
        champs.push(Champ { nom: i.nom, genre: "identifiant".into(), r#type: i.r#type, raison: i.raison, requis, valeur: String::new(), present: i.present });
    }
    champs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent(dossier: &Path) -> Manifest {
        let installe = dossier.join("installes").join("agent-x");
        std::fs::create_dir_all(installe.join("modeles")).unwrap();
        std::fs::write(installe.join("modeles/t.demande.json"), json!({ "type": "object", "properties": { "compte": { "type": "string", "description": "Adresse Gmail." }, "nombre": { "type": "integer" } } }).to_string()).unwrap();
        Manifest {
            id: "agent-x".into(),
            name: "X".into(),
            brut: json!({ "taches": [{ "type": "t", "modele_demande": "modeles/t.demande.json", "modele_livrable": "" }], "identifiants": [] }),
            dossier: Some(installe),
            ..Default::default()
        }
    }

    #[test]
    fn champ_de_donnees_nomme_par_l_agent() {
        let d = std::env::temp_dir().join(format!("orchestra-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let m = agent(&d);
        let c = champs_manquants(&m, "adresse Gmail inconnue : indiquez-la dans donnees.compte");
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].nom.as_str(), c[0].genre.as_str(), c[0].raison.as_str(), c[0].requis), ("compte", "reglage", "Adresse Gmail.", true));
        // un motif sans champ connu : pas de formulaire
        assert!(champs_manquants(&m, "boîte pleine").is_empty());
        assert!(champs_manquants(&m, "indiquez donnees.inconnu").is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn reglages_completent_la_mission_sans_ecraser() {
        let d = std::env::temp_dir().join(format!("orchestra-reglages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let m = agent(&d);
        let f = fichier_reglages(&d, "agent-x");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, json!({ "compte": "pb@gmail.com", "ailleurs": "x" }).to_string()).unwrap();
        let schema = m.lire_modele("modeles/t.demande.json").unwrap();
        let mut donnees = json!({ "nombre": 100 });
        assert_eq!(completer_donnees(&m, &schema, &mut donnees), vec!["compte".to_string()]);
        assert_eq!(donnees, json!({ "nombre": 100, "compte": "pb@gmail.com" }));
        let mut deja = json!({ "compte": "autre@gmail.com" });
        assert!(completer_donnees(&m, &schema, &mut deja).is_empty());
        assert_eq!(deja["compte"], "autre@gmail.com");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_secret_n_est_jamais_un_reglage() {
        assert!(ressemble_a_un_secret("gmail_mot_de_passe"));
        assert!(ressemble_a_un_secret("api_key"));
        assert!(!ressemble_a_un_secret("compte"));
    }
}
