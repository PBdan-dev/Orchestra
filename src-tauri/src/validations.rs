//! Validation des actions irréversibles (D28, lot T3).
//!
//! Avant chaque fonction `action_sensitive_`, l'agent émet `validation_requise` et attend. Orchestra
//! montre l'action et sa raison avec 4 portées : « cette fois », « ce type d'action pour la session »,
//! « tout pour la session », « tout, de façon permanente » (pour un seul agent, révocable dans le
//! panneau Agents). Les autorisations de session vivent en mémoire et disparaissent à la fermeture ;
//! les permanentes sont dans `agents/autorisations.json`.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::contrat;

pub const PORTEES: [&str; 4] = ["cette_fois", "type_session", "tout_session", "permanent"];

/// Une autorisation en cours (affichée et révocable dans le panneau Agents).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Autorisation {
    pub agent_id: String,
    /// Action concernée (`type_session`), vide pour « tout ».
    pub action: String,
    /// type_session | tout_session | permanent
    pub portee: String,
}

struct Attente {
    mission_id: String,
    agent_id: String,
    action: String,
    vers_agent: UnboundedSender<Value>,
}

pub struct Validations {
    attentes: Mutex<HashMap<String, Attente>>,
    session: Mutex<BTreeSet<Autorisation>>,
    fichier: PathBuf,
}

#[derive(Serialize, Deserialize, Default)]
struct Permanentes {
    #[serde(default)]
    permanent: BTreeSet<String>,
}

impl Validations {
    pub fn new(fichier: PathBuf) -> Self {
        Validations { attentes: Mutex::new(HashMap::new()), session: Mutex::new(BTreeSet::new()), fichier }
    }

    fn permanentes(&self) -> Permanentes {
        std::fs::read_to_string(&self.fichier).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    fn enregistrer(&self, p: &Permanentes) -> anyhow::Result<()> {
        let doc = json!({
            "_commentaire": "Agents autorisés de façon permanente à faire leurs actions irréversibles sans validation (D28). Retirer un agent ici ou dans le panneau Agents.",
            "permanent": p.permanent,
        });
        std::fs::write(&self.fichier, serde_json::to_string_pretty(&doc)?)?;
        Ok(())
    }

    /// Portée qui couvre déjà cette action, s'il y en a une (la question n'est alors pas posée).
    pub fn autorise(&self, agent_id: &str, action: &str) -> Option<&'static str> {
        if self.permanentes().permanent.contains(agent_id) {
            return Some("permanent");
        }
        let s = self.session.lock().unwrap();
        if s.iter().any(|a| a.agent_id == agent_id && a.portee == "tout_session") {
            return Some("tout_session");
        }
        if s.iter().any(|a| a.agent_id == agent_id && a.portee == "type_session" && a.action == action) {
            return Some("type_session");
        }
        None
    }

    /// Met une demande en attente de la réponse de PB.
    pub fn attendre(&self, cle: &str, mission_id: &str, agent_id: &str, action: &str, vers_agent: UnboundedSender<Value>) {
        self.attentes.lock().unwrap().insert(cle.into(), Attente { mission_id: mission_id.into(), agent_id: agent_id.into(), action: action.into(), vers_agent });
    }

    /// Réponse de PB : envoie le message `validation` à l'agent et retient l'autorisation choisie.
    pub fn repondre(&self, cle: &str, accepte: bool, portee: &str) -> Result<(String, String), String> {
        if !PORTEES.contains(&portee) {
            return Err(format!("portée inconnue : {portee}"));
        }
        let a = self.attentes.lock().unwrap().remove(cle).ok_or_else(|| "cette demande de validation n'est plus en attente".to_string())?;
        if accepte {
            match portee {
                "type_session" | "tout_session" => {
                    self.session.lock().unwrap().insert(Autorisation {
                        agent_id: a.agent_id.clone(),
                        action: if portee == "type_session" { a.action.clone() } else { String::new() },
                        portee: portee.into(),
                    });
                }
                "permanent" => {
                    let mut p = self.permanentes();
                    p.permanent.insert(a.agent_id.clone());
                    self.enregistrer(&p).map_err(|e| e.to_string())?;
                }
                _ => {}
            }
        }
        a.vers_agent
            .send(contrat::reponse_validation(&a.mission_id, accepte, portee))
            .map_err(|_| "la mission est déjà terminée".to_string())?;
        Ok((a.mission_id, a.agent_id))
    }

    /// Demandes encore en attente d'une mission terminée : retirées.
    pub fn oublier_mission(&self, mission_id: &str) {
        self.attentes.lock().unwrap().retain(|_, a| a.mission_id != mission_id);
    }

    pub fn lister(&self) -> Vec<Autorisation> {
        let mut l: Vec<Autorisation> = self.session.lock().unwrap().iter().cloned().collect();
        l.extend(self.permanentes().permanent.into_iter().map(|agent_id| Autorisation { agent_id, action: String::new(), portee: "permanent".into() }));
        l
    }

    pub fn retirer(&self, a: &Autorisation) -> anyhow::Result<()> {
        if a.portee == "permanent" {
            let mut p = self.permanentes();
            p.permanent.remove(&a.agent_id);
            self.enregistrer(&p)?;
        } else {
            self.session.lock().unwrap().remove(a);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portees_des_autorisations() {
        let f = std::env::temp_dir().join(format!("orchestra-autorisations-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let v = Validations::new(f.clone());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        assert_eq!(v.autorise("agent-fichiers", "action_sensitive_supprimer"), None);

        // « ce type d'action pour la session »
        v.attendre("m#1", "m", "agent-fichiers", "action_sensitive_supprimer", tx.clone());
        assert_eq!(v.repondre("m#1", true, "type_session").unwrap().0, "m");
        assert_eq!(rx.try_recv().unwrap()["decision"], "accepte");
        assert_eq!(v.autorise("agent-fichiers", "action_sensitive_supprimer"), Some("type_session"));
        assert_eq!(v.autorise("agent-fichiers", "action_sensitive_envoyer"), None);
        assert!(v.repondre("m#1", true, "cette_fois").is_err(), "déjà répondu");

        // refus : rien n'est retenu
        v.attendre("m#2", "m", "agent-mail", "action_sensitive_envoyer", tx.clone());
        v.repondre("m#2", false, "tout_session").unwrap();
        assert_eq!(rx.try_recv().unwrap()["decision"], "refuse");
        assert_eq!(v.autorise("agent-mail", "action_sensitive_envoyer"), None);

        // permanent, puis retiré
        v.attendre("m#3", "m", "agent-mail", "action_sensitive_envoyer", tx);
        v.repondre("m#3", true, "permanent").unwrap();
        assert_eq!(v.autorise("agent-mail", "x"), Some("permanent"));
        assert_eq!(v.lister().len(), 2);
        v.retirer(&Autorisation { agent_id: "agent-mail".into(), action: String::new(), portee: "permanent".into() }).unwrap();
        assert_eq!(v.autorise("agent-mail", "x"), None);
        let _ = std::fs::remove_file(&f);
    }
}
