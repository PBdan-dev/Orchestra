//! Agents hébergés (lot T8, D2) : badge « livrable seul ».
//!
//! `execution: {"type": "heberge", "endpoint": "https://…"}`. Orchestra envoie la mission en JSON
//! (POST, HTTPS obligatoire), reçoit les événements si l'agent en fournit (une ligne JSON par
//! message, comme le contrat stdio-json) puis le compte rendu. Rien d'autre ne quitte la machine :
//! ni mémoire, ni fichiers, ni chemin local (le projet est désigné par son seul nom). Ce qui part
//! est gardé dans `envoye.json` du dossier de mission. Le livrable est contrôlé automatiquement
//! (état, format, limites) ; sinon `echec` avec motif. Un agent hébergé ne peut ni écrire de
//! fichier ni être promu « de confiance » automatiquement.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{
    agents::Manifest,
    contrat::{self, Deroulement},
};

/// Délai maximal d'une mission hébergée sans limite mesurée (l'appel est alors abandonné).
pub const DELAI_DEFAUT: Duration = Duration::from_secs(15 * 60);

pub fn endpoint(m: &Manifest) -> Result<String, String> {
    let e = m.brut.pointer("/execution/endpoint").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if !e.starts_with("https://") {
        return Err(format!("agent hébergé {} : adresse « {e} » refusée (HTTPS obligatoire)", m.id));
    }
    Ok(e)
}

/// Ce qui quitte la machine (affiché avant l'installation) : adresse, données déclarées, conservation.
pub fn ce_qui_part(m: &Manifest) -> Value {
    json!({
        "adresse": m.brut.pointer("/execution/endpoint"),
        "donnees_envoyees": m.brut.get("donnees_envoyees").and_then(|v| v.as_str()).unwrap_or("non précisé par l'agent"),
        "conservation": m.brut.get("conservation_declaree").or(m.brut.get("conservation")).and_then(|v| v.as_str()).unwrap_or("non précisée par l'agent"),
        "envoye_par_orchestra": "la mission seulement (but, critère de réussite, contexte, données de la demande, nom du projet) ; jamais ta mémoire, tes fichiers ni de chemin local",
    })
}

/// Mission telle qu'envoyée : sans chemin local ni manifeste ; le projet par son nom.
pub fn mission_envoyee(mission: &Value) -> Value {
    let mut v = mission.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("manifeste");
        let nom = Path::new(o.get("projet").and_then(|p| p.as_str()).unwrap_or("")).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        o.insert("projet".into(), json!(nom));
    }
    v
}

/// Contrôle automatique du livrable d'un agent hébergé (en plus du schéma du compte rendu).
pub fn controler_livrable(mission: &Value, cr: &Value, duree: Duration) -> Result<(), String> {
    let etat = cr["etat"].as_str().unwrap_or("");
    if !cr.pointer("/resultat/fichiers").and_then(|f| f.as_array()).is_none_or(|a| a.is_empty()) {
        return Err("un agent hébergé n'a pas accès aux fichiers : le compte rendu ne peut pas annoncer de fichier".into());
    }
    if etat == "termine" || etat == "partiel" {
        let texte = cr.pointer("/resultat/texte").and_then(|t| t.as_str()).unwrap_or("").trim();
        match mission.pointer("/format_retour/type").and_then(|t| t.as_str()).unwrap_or("texte") {
            "texte" | "fichier" if texte.is_empty() => return Err("livrable vide : le format attendu est un texte".into()),
            "tableau" if !(texte.contains('|') || texte.contains('\t') || texte.contains(';')) => return Err("livrable non conforme : un tableau était attendu".into()),
            _ => {}
        }
    }
    if let Some(max) = mission.pointer("/limites/duree_max_s").and_then(|d| d.as_f64()) {
        if duree.as_secs_f64() > max {
            return Err(format!("limite de durée dépassée ({:.0} s pour {max:.0} s au plus)", duree.as_secs_f64()));
        }
    }
    Ok(())
}

/// Envoie la mission à l'agent hébergé et lit sa réponse. `cle` : identifiant (D34) envoyé en
/// `Authorization: Bearer`. `externes` : un arrêt (propre ou immédiat) abandonne l'appel.
pub async fn executer<F>(
    http: &reqwest::Client,
    m: &Manifest,
    mission: &Value,
    dossier_mission: &Path,
    cle: Option<String>,
    mut externes: Option<&mut tokio::sync::mpsc::UnboundedReceiver<Value>>,
    mut sur_evenement: F,
) -> anyhow::Result<Deroulement>
where
    F: FnMut(&Value) -> Vec<Value>,
{
    let mission_id = mission["id"].as_str().unwrap_or("").to_string();
    std::fs::create_dir_all(dossier_mission)?;
    std::fs::write(dossier_mission.join("mission.json"), serde_json::to_string_pretty(mission)?)?;
    let mut d = Deroulement::default();
    let url = match endpoint(m) {
        Ok(u) => u,
        Err(e) => {
            d.compte_rendu = contrat::echec(&mission_id, &e);
            std::fs::write(dossier_mission.join("compte_rendu.json"), serde_json::to_string_pretty(&d.compte_rendu)?)?;
            return Ok(d);
        }
    };
    let envoyee = mission_envoyee(mission);
    std::fs::write(dossier_mission.join("envoye.json"), serde_json::to_string_pretty(&json!({ "adresse": url, "mission": envoyee }))?)?;
    let delai = mission.pointer("/limites/duree_max_s").and_then(|d| d.as_f64()).map(|s| Duration::from_secs_f64(s * 2.0).max(Duration::from_secs(30))).unwrap_or(DELAI_DEFAUT);
    let debut = Instant::now();
    let mut req = http.post(&url).timeout(delai).header("Accept", "application/x-ndjson, application/json").json(&envoyee);
    if let Some(c) = &cle {
        req = req.bearer_auth(c);
    }
    let appel = async {
        let r = req.send().await.map_err(|e| format!("agent hébergé injoignable ({url}) : {e}"))?;
        if !r.status().is_success() {
            return Err(format!("l'agent hébergé a répondu {}", r.status()));
        }
        r.text().await.map_err(|e| format!("réponse illisible : {e}"))
    };
    let corps = tokio::select! {
        c = appel => c,
        Some(msg) = async { match externes.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
            let t = msg["type"].as_str().unwrap_or("");
            Err(if t == contrat::ARRET_IMMEDIAT || t == "arret" { "mission arrêtée par PB : appel à l'agent hébergé abandonné".to_string() } else { format!("message « {t} » sans effet sur un agent hébergé") })
        }
    };
    let duree = debut.elapsed();
    let mut journal = String::new();
    match corps {
        Err(e) => d.compte_rendu = contrat::echec(&mission_id, &e),
        Ok(texte) => {
            journal.push_str(&texte);
            // une ligne JSON par message (événements puis compte rendu), ou un seul compte rendu
            let entier: Option<Value> = serde_json::from_str(texte.trim()).ok();
            let messages: Vec<Value> = match entier {
                Some(v) if v.get("etat").is_some() => vec![json!({ "compte_rendu": v })],
                Some(v) => vec![v],
                None => texte.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).map_err(|_| d.ecarts.push(format!("ligne non JSON : {}", l.chars().take(120).collect::<String>()))).ok()).collect(),
            };
            for v in messages {
                if let Some(ev) = v.get("evenement") {
                    match contrat::valider_evenement(ev) {
                        Ok(()) => {
                            d.evenements.push(ev.clone());
                            let _ = sur_evenement(ev);
                        }
                        Err(e) => d.ecarts.push(e),
                    }
                } else if let Some(cr) = v.get("compte_rendu") {
                    d.compte_rendu = match contrat::valider_compte_rendu(cr) {
                        Ok(()) if cr["mission_id"].as_str() == Some(mission_id.as_str()) => cr.clone(),
                        Ok(()) => contrat::echec(&mission_id, "compte rendu d'une autre mission"),
                        Err(e) => contrat::echec(&mission_id, &e),
                    };
                } else {
                    d.ecarts.push("message ni « evenement » ni « compte_rendu »".into());
                }
            }
            if d.compte_rendu.is_null() {
                d.compte_rendu = contrat::echec(&mission_id, "l'agent hébergé n'a pas rendu de compte rendu");
            } else if let Err(motif) = controler_livrable(mission, &d.compte_rendu, duree) {
                d.ecarts.push(format!("livrable refusé : {motif}"));
                d.compte_rendu = contrat::echec(&mission_id, &format!("livrable refusé par le contrôle automatique : {motif}"));
            }
        }
    }
    std::fs::write(dossier_mission.join("reponse-agent-heberge.txt"), journal)?;
    std::fs::write(dossier_mission.join("compte_rendu.json"), serde_json::to_string_pretty(&d.compte_rendu)?)?;
    if !d.ecarts.is_empty() {
        std::fs::write(dossier_mission.join("ecarts.txt"), d.ecarts.join("\n"))?;
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_sans_chemin_ni_manifeste() {
        let m = json!({ "id": "x", "projet": "C:\\Users\\pb\\Orchestra\\projets\\salon", "manifeste": { "a": 1 }, "contexte": "c" });
        let e = mission_envoyee(&m);
        assert_eq!(e["projet"], "salon");
        assert!(e.get("manifeste").is_none() && e["contexte"] == "c");
        assert!(!e.to_string().contains("Users"));
    }

    #[test]
    fn livrable_controle() {
        let mission = json!({ "format_retour": { "type": "texte" }, "limites": { "duree_max_s": 10 } });
        let cr = |etat: &str, texte: &str, fichiers: Value| json!({ "etat": etat, "resultat": { "texte": texte, "fichiers": fichiers } });
        assert!(controler_livrable(&mission, &cr("termine", "Bonjour", json!([])), Duration::from_secs(1)).is_ok());
        assert!(controler_livrable(&mission, &cr("termine", " ", json!([])), Duration::from_secs(1)).unwrap_err().contains("vide"));
        assert!(controler_livrable(&mission, &cr("termine", "x", json!(["a.md"])), Duration::from_secs(1)).unwrap_err().contains("fichier"));
        assert!(controler_livrable(&mission, &cr("termine", "x", json!([])), Duration::from_secs(11)).unwrap_err().contains("durée"));
        assert!(controler_livrable(&mission, &cr("echec", "", json!([])), Duration::from_secs(1)).is_ok());
    }
}
