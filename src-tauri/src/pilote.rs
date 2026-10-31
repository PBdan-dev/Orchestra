//! Pilotage de la mission en cours (lot T7) : pause, reprise, arrêt propre, arrêt immédiat.
//!
//! Mode série (D7) : une seule mission à la fois. Pendant une pause ou après un arrêt demandé,
//! la porte ne sert plus le modèle de l'agent (l'appel en cours se termine). L'arrêt immédiat
//! coupe le programme de l'agent ; il n'est envoyé qu'après confirmation de PB (D5).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::contrat;

struct EnCours {
    mission_id: String,
    vers_agent: UnboundedSender<Value>,
    porte: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
pub struct Pilote {
    en_cours: Mutex<Option<EnCours>>,
}

pub const ORDRES: [&str; 4] = ["pause", "reprendre", "arret", "arret_immediat"];

impl Pilote {
    pub fn commencer(&self, mission_id: &str, vers_agent: UnboundedSender<Value>, porte: Option<Arc<AtomicBool>>) {
        *self.en_cours.lock().unwrap() = Some(EnCours { mission_id: mission_id.into(), vers_agent, porte });
    }

    pub fn finir(&self, mission_id: &str) {
        let mut e = self.en_cours.lock().unwrap();
        if e.as_ref().is_some_and(|x| x.mission_id == mission_id) {
            *e = None;
        }
    }

    pub fn mission_en_cours(&self) -> Option<String> {
        self.en_cours.lock().unwrap().as_ref().map(|e| e.mission_id.clone())
    }

    /// Ordre de PB (ou d'Orchestra aux limites D29) pour la mission en cours.
    pub fn commander(&self, mission_id: &str, ordre: &str) -> Result<(), String> {
        let e = self.en_cours.lock().unwrap();
        let e = e.as_ref().filter(|e| e.mission_id == mission_id).ok_or_else(|| "cette mission n'est plus en cours".to_string())?;
        let suspendre = |v: bool| {
            if let Some(p) = &e.porte {
                p.store(v, Ordering::SeqCst);
            }
        };
        let msg = match ordre {
            "pause" => {
                suspendre(true);
                contrat::message(mission_id, "pause")
            }
            "reprendre" => {
                suspendre(false);
                contrat::message(mission_id, "reprendre")
            }
            // arrêt propre : l'agent finit son étape ; s'il l'ignore, la porte ne sert plus son modèle
            "arret" => {
                suspendre(true);
                contrat::message(mission_id, "arret")
            }
            "arret_immediat" => {
                suspendre(true);
                json!({ "type": contrat::ARRET_IMMEDIAT })
            }
            autre => return Err(format!("ordre inconnu : {autre}")),
        };
        e.vers_agent.send(msg).map_err(|_| "la mission est déjà terminée".to_string())
    }
}
