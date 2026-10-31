//! Contrat 1 entre Orchestra et un agent local, protocole stdio-json (cahier §9, lot T1).
//!
//! Entrée standard de l'agent : une ligne JSON par message ; d'abord la mission, puis les messages
//! d'Orchestra (`arret`, `pause`, `reprendre`, `validation`). Sortie standard : une ligne JSON par
//! message, `{"evenement": {…}}` pendant la mission, puis `{"compte_rendu": {…}}`.
//! Tout ce qui entre et sort est validé avec les schémas du contrat ; un écart est refusé avec motif.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{agents::Manifest, schema};

/// Silence maximal d'un agent avant de le déclarer en échec (les plafonds fins viennent au lot T2, D29).
pub const SILENCE_AGENT: Duration = Duration::from_secs(300);
/// Attente de la fin du programme après son compte rendu.
const FIN_AGENT: Duration = Duration::from_secs(10);

/// Ce que l'agent a émis pendant la mission.
#[derive(Debug, Default, Clone)]
pub struct Deroulement {
    pub evenements: Vec<Value>,
    pub compte_rendu: Value,
    /// Écarts au contrat constatés (lignes illisibles, événements mal formés…).
    pub ecarts: Vec<String>,
}

/// Identifiant de mission : `AAAA-MM-JJ_HHMM_<agent>_<4 caractères>`.
pub fn id_mission(agent_id: &str) -> String {
    let run: String = uuid::Uuid::new_v4().simple().to_string()[..4].to_string();
    format!("{}_{}_{run}", chrono::Local::now().format("%Y-%m-%d_%H%M"), crate::paths::safe_name(agent_id))
}

/// Champs de la mission (section 9), avant validation.
pub struct Contenu<'a> {
    pub projet: &'a Path,
    pub type_tache: Option<String>,
    pub but: &'a str,
    pub critere_reussite: &'a str,
    pub contexte: &'a str,
    pub donnees: Value,
    pub demande_originale: &'a str,
}

/// Construit la mission et la valide avec le schéma du contrat.
pub fn mission(m: &Manifest, c: Contenu) -> Result<Value, String> {
    let mut v = json!({
        "version": 1,
        "id": id_mission(&m.id),
        "agent_id": m.id,
        "projet": c.projet.to_string_lossy(),
        "but": c.but,
        "critere_reussite": c.critere_reussite,
        "contexte": c.contexte,
        "donnees": c.donnees,
        "limites": {},
        "format_retour": { "type": "texte" },
        "validation": m.fonctions_sensibles(),
        "manifeste": m.brut,
        "demande_originale": c.demande_originale,
        "cree_le": chrono::Local::now().to_rfc3339(),
    });
    if let Some(t) = c.type_tache {
        v["type_tache"] = json!(t);
    }
    valider_mission(&v).map(|_| v)
}

pub fn valider_mission(v: &Value) -> Result<(), String> {
    motif(schema::erreurs(v, &schema::schema(schema::MISSION)), "mission mal formée")
}

pub fn valider_compte_rendu(v: &Value) -> Result<(), String> {
    motif(schema::erreurs(v, &schema::schema(schema::REPONSE)), "compte rendu mal formé")
}

pub fn valider_evenement(v: &Value) -> Result<(), String> {
    motif(schema::erreurs(v, &schema::schema(schema::EVENEMENT)), "événement mal formé")
}

pub fn valider_message(v: &Value) -> Result<(), String> {
    motif(schema::erreurs(v, &schema::schema(schema::MESSAGE)), "message d'Orchestra mal formé")
}

fn motif(e: Vec<String>, quoi: &str) -> Result<(), String> {
    if e.is_empty() { Ok(()) } else { Err(format!("{quoi} : {}", e.into_iter().take(4).collect::<Vec<_>>().join(" ; "))) }
}

/// Message d'Orchestra vers l'agent (D28, D30).
pub fn message(mission_id: &str, type_: &str) -> Value {
    json!({ "mission_id": mission_id, "type": type_, "t": chrono::Local::now().to_rfc3339() })
}

pub fn reponse_validation(mission_id: &str, accepte: bool, portee: &str) -> Value {
    let mut m = message(mission_id, "validation");
    m["decision"] = json!(if accepte { "accepte" } else { "refuse" });
    m["portee"] = json!(portee);
    m
}

/// Compte rendu d'échec construit par Orchestra quand l'agent n'en rend pas de valide.
pub fn echec(mission_id: &str, motif: &str) -> Value {
    json!({
        "mission_id": mission_id, "etat": "echec", "motif": motif,
        "resultat": { "texte": "", "fichiers": [] }, "actions_declarees": [],
        "verification": { "controle": [], "non_controle": [] }, "point_de_reprise": null
    })
}

/// Compte rendu d'une mission coupée par un arrêt immédiat (D5, lot T7).
pub fn arret_force(mission_id: &str) -> Value {
    json!({
        "mission_id": mission_id, "etat": "arrete_force",
        "motif": "arrêt immédiat demandé par PB : fichiers possiblement incomplets",
        "resultat": { "texte": "", "fichiers": [] }, "actions_declarees": [],
        "verification": { "controle": [], "non_controle": ["mission coupée : fichiers possiblement incomplets"] }, "point_de_reprise": null
    })
}

/// Message interne (jamais transmis à l'agent) : couper le programme de l'agent.
pub const ARRET_IMMEDIAT: &str = "__arret_immediat";

/// Mission de reprise (lot T7, décision de PB du 31/10) : la mission d'origine, avec un nouvel
/// identifiant et le point de reprise écrit par l'agent, rendu tel quel dans `reprise`.
pub fn mission_reprise(origine: &Value, point_de_reprise: &Value) -> Result<Value, String> {
    let mut v = origine.clone();
    let agent = origine["agent_id"].as_str().unwrap_or("agent");
    v["id"] = json!(id_mission(agent));
    v["cree_le"] = json!(chrono::Local::now().to_rfc3339());
    v["reprise"] = json!({ "mission_origine": origine["id"], "point_de_reprise": point_de_reprise });
    valider_mission(&v)?;
    Ok(v)
}

/// Découpe la commande de lancement (`python agent.py`, guillemets admis).
pub fn decouper_commande(c: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut guillemet = false;
    for ch in c.chars() {
        match ch {
            '"' => guillemet = !guillemet,
            c if c.is_whitespace() && !guillemet => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Porte du modèle de la mission (lot T2) et ses notes, si l'agent utilise un modèle.
pub struct AvecPorte<'a> {
    pub porte: &'a crate::porte::Porte,
    pub notes: &'a mut tokio::sync::mpsc::UnboundedReceiver<crate::porte::Note>,
}

/// Lance l'agent local, lui envoie la mission, relaie ses événements et renvoie son déroulement.
/// `sur_evenement` reçoit chaque événement valide et renvoie les messages à envoyer à l'agent
/// (réponse à une validation, arrêt, pause…) ; `sur_note` reçoit ce que voit la porte du modèle
/// (réflexion, écarts, limites) et peut aussi renvoyer des messages. Tout est noté dans `dossier_mission`.
/// `externes` : messages venus d'ailleurs pendant la mission (réponse de PB à une validation, D28).
#[allow(clippy::too_many_arguments)]
pub async fn executer<F, G>(
    m: &Manifest,
    mission: &Value,
    dossier_mission: &Path,
    mut porte: Option<AvecPorte<'_>>,
    mut externes: Option<&mut tokio::sync::mpsc::UnboundedReceiver<Value>>,
    mut sur_evenement: F,
    mut sur_note: G,
) -> anyhow::Result<Deroulement>
where
    F: FnMut(&Value) -> Vec<Value>,
    G: FnMut(&crate::porte::Note) -> Vec<Value>,
{
    let mission_id = mission["id"].as_str().unwrap_or("").to_string();
    let dossier_agent: PathBuf = m.dossier.clone().ok_or_else(|| anyhow::anyhow!("agent {} non installé", m.id))?;
    if !m.est_local() {
        anyhow::bail!("agent {} : seul le type « local » se lance (agents hébergés : lot T8)", m.id);
    }
    let cmd = decouper_commande(&m.commande());
    let Some((prog, args)) = cmd.split_first() else { anyhow::bail!("agent {} : commande de lancement vide", m.id) };

    std::fs::create_dir_all(dossier_mission)?;
    std::fs::write(dossier_mission.join("mission.json"), serde_json::to_string_pretty(mission)?)?;
    let mut journal = std::fs::OpenOptions::new().create(true).append(true).open(dossier_mission.join("evenements.jsonl"))?;

    // un agent Python qui a son propre environnement (.venv) est lancé avec lui
    let venv = dossier_agent.join(".venv").join("Scripts").join("python.exe");
    let prog: String = if (prog == "python" || prog == "python3") && venv.exists() { venv.to_string_lossy().to_string() } else { prog.clone() };
    let mut commande = tokio::process::Command::new(&prog);
    commande
        .args(args)
        .current_dir(&dossier_agent)
        .env("PYTHONIOENCODING", "utf-8")
        .envs(porte.as_ref().map(|p| p.porte.env()).unwrap_or_default())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(std::fs::File::create(dossier_mission.join("sortie-erreur.log"))?))
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        commande.creation_flags(CREATE_NO_WINDOW);
    }
    let mut enfant = commande.spawn().map_err(|e| anyhow::anyhow!("lancement de « {} » impossible : {e}", m.commande()))?;
    let mut entree = enfant.stdin.take().ok_or_else(|| anyhow::anyhow!("entrée standard indisponible"))?;
    let sortie = enfant.stdout.take().ok_or_else(|| anyhow::anyhow!("sortie standard indisponible"))?;
    let mut lignes = BufReader::new(sortie).lines();

    let mut d = Deroulement::default();
    envoyer(&mut entree, &mut journal, mission).await?;

    let mut horloge = tokio::time::interval(Duration::from_secs(20));
    // pendant qu'une validation attend PB, ou pendant une pause, l'agent attend aussi : pas de délai de silence
    let mut attente_validation = false;
    // message de PB ou d'Orchestra pendant la mission (validation, pause, reprendre, arrêt, arrêt immédiat)
    macro_rules! externe {
        ($msg:expr) => {{
            let msg: Value = $msg;
            if msg["type"] == ARRET_IMMEDIAT {
                let _ = enfant.kill().await;
                d.compte_rendu = arret_force(&mission_id);
                break;
            }
            if msg["type"] == "validation" || msg["type"] == "reprendre" {
                attente_validation = false;
            }
            if valider_message(&msg).is_ok() && envoyer(&mut entree, &mut journal, &msg).await.is_err() {
                d.ecarts.push("message impossible à transmettre à l'agent".into());
            }
            continue;
        }};
    }
    loop {
        let delai = if attente_validation { Duration::from_secs(7 * 24 * 3600) } else { SILENCE_AGENT };
        // messages venus de l'interface (validation), notes de la porte, horloge des limites,
        // en même temps que la sortie de l'agent
        if let Some(msg) = externes.as_mut().and_then(|rx| rx.try_recv().ok()) {
            externe!(msg);
        }
        let lu = if let Some(p) = porte.as_mut() {
            tokio::select! {
                l = tokio::time::timeout(delai, lignes.next_line()) => l,
                Some(msg) = async { match externes.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                    externe!(msg);
                }
                Some(note) = p.notes.recv() => {
                    for msg in sur_note(&note) {
                        if valider_message(&msg).is_ok() && envoyer(&mut entree, &mut journal, &msg).await.is_err() {
                            d.ecarts.push("message impossible à transmettre à l'agent".into());
                        }
                    }
                    continue;
                }
                _ = horloge.tick() => {
                    p.porte.verifier_durees();
                    continue;
                }
            }
        } else {
            tokio::select! {
                l = tokio::time::timeout(delai, lignes.next_line()) => l,
                Some(msg) = async { match externes.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                    externe!(msg);
                }
            }
        };
        let ligne = match lu {
            Err(_) => {
                let _ = enfant.kill().await;
                d.compte_rendu = echec(&mission_id, &format!("l'agent ne répond plus depuis {} s", SILENCE_AGENT.as_secs()));
                break;
            }
            Ok(Err(e)) => {
                d.compte_rendu = echec(&mission_id, &format!("sortie de l'agent illisible : {e}"));
                break;
            }
            Ok(Ok(None)) => {
                d.compte_rendu = echec(&mission_id, "l'agent s'est arrêté sans compte rendu");
                break;
            }
            Ok(Ok(Some(l))) => l,
        };
        if ligne.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&ligne) else {
            d.ecarts.push(format!("ligne non JSON sur la sortie standard : {}", ligne.chars().take(120).collect::<String>()));
            continue;
        };
        let _ = writeln!(journal, "{}", json!({ "de_agent": v }));
        if let Some(ev) = v.get("evenement") {
            if let Err(e) = valider_evenement(ev) {
                d.ecarts.push(e);
                continue;
            }
            d.evenements.push(ev.clone());
            if let Some(p) = porte.as_ref() {
                p.porte.progres();
            }
            if ev["type"] == "validation_requise" || ev["type"] == "en_pause" {
                attente_validation = true;
            }
            for msg in sur_evenement(ev) {
                if msg["type"] == "validation" {
                    attente_validation = false;
                }
                if valider_message(&msg).is_ok() && envoyer(&mut entree, &mut journal, &msg).await.is_err() {
                    d.ecarts.push("message impossible à transmettre à l'agent".into());
                }
            }
        } else if let Some(cr) = v.get("compte_rendu") {
            d.compte_rendu = match valider_compte_rendu(cr) {
                Ok(()) if cr["mission_id"].as_str() == Some(mission_id.as_str()) => cr.clone(),
                Ok(()) => echec(&mission_id, "compte rendu d'une autre mission"),
                Err(e) => echec(&mission_id, &e),
            };
            break;
        } else {
            d.ecarts.push("message ni « evenement » ni « compte_rendu »".into());
        }
    }
    drop(entree);
    // notes de la porte arrivées pendant la fin de la mission
    if let Some(p) = porte.as_mut() {
        while let Ok(note) = p.notes.try_recv() {
            let _ = sur_note(&note);
        }
    }
    if tokio::time::timeout(FIN_AGENT, enfant.wait()).await.is_err() {
        let _ = enfant.kill().await;
    }
    std::fs::write(dossier_mission.join("compte_rendu.json"), serde_json::to_string_pretty(&d.compte_rendu)?)?;
    if !d.ecarts.is_empty() {
        std::fs::write(dossier_mission.join("ecarts.txt"), d.ecarts.join("\n"))?;
    }
    Ok(d)
}

async fn envoyer(entree: &mut tokio::process::ChildStdin, journal: &mut std::fs::File, v: &Value) -> std::io::Result<()> {
    let _ = writeln!(journal, "{}", json!({ "vers_agent": v }));
    entree.write_all(format!("{v}\n").as_bytes()).await?;
    entree.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commande_decoupee() {
        assert_eq!(decouper_commande("python agent.py"), vec!["python", "agent.py"]);
        assert_eq!(decouper_commande("\"C:\\Program Files\\a.exe\" --x"), vec!["C:\\Program Files\\a.exe", "--x"]);
    }

    #[test]
    fn messages_conformes() {
        assert!(valider_message(&message("m", "arret")).is_ok());
        assert!(valider_message(&reponse_validation("m", false, "cette_fois")).is_ok());
        let mut faux = message("m", "validation");
        assert!(valider_message(&faux).is_err());
        faux["type"] = json!("annuler");
        assert!(valider_message(&faux).is_err());
    }

    #[test]
    fn echec_conforme_et_mission_refusee() {
        assert!(valider_compte_rendu(&echec("m", "raison")).is_ok());
        let e = valider_mission(&json!({ "version": 2, "id": "x" })).unwrap_err();
        assert!(e.contains("mission mal formée") && e.contains("agent_id"));
        assert!(id_mission("agent-date").contains("_agent-date_"));
    }
}
