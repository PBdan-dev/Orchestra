//! Événements envoyés à l'interface. Tout ce qu'Orchestra fait passe par ici :
//! c'est la source de la transparence affichée à l'écran et du journal.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::agents::Manifest;

pub const CHANNEL: &str = "orchestra://event";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Changement d'état global : idle, listening, thinking, recalling,
    /// searching_agent, delegating, working, speaking, memorizing
    Phase { run_id: String, phase: String, label: String },
    /// Fragment de réflexion en direct
    Thought { run_id: String, text: String },
    /// Souvenirs rappelés depuis la mémoire
    Recall { run_id: String, items: Vec<Recalled> },
    /// Un agent se branche sur Orchestra
    AgentAttached { run_id: String, agent: Manifest, source: String, reason: String },
    /// État d'un agent branché
    AgentStatus { run_id: String, agent_id: String, status: String, detail: String },
    /// Un outil a été utilisé par un agent
    ToolCall { run_id: String, agent_id: String, tool: String, input: serde_json::Value, output: String },
    /// Fragment de la réponse finale
    Token { run_id: String, text: String },
    /// Ce qui a été écrit en mémoire
    Memorized { run_id: String, file: String, section: String, line: String },
    /// Rien à retenir de durable
    NothingToMemorize { run_id: String },
    /// Fin de traitement
    Done { run_id: String, answer: String, intent: String, message_historique: String },
    /// Coupe de texte (historique trop long, profil trop long) : jamais silencieuse
    Coupe { run_id: String, quoi: String, detail: String },
    /// Avertissement visible dans Conscience et noté au journal (repli, appel invalide…)
    Avertissement { run_id: String, message: String },
    /// Compaction de la conversation (lot C) : debut | fin | annulee.
    Compaction { etat: String, detail: String },
    /// Vérification des chaînes d'empreintes des traces (lot T6), au lancement ou à l'ouverture d'une mission.
    Integrite { contexte: String, intactes: usize, non_chainees: usize, ruptures: Vec<crate::integrite::Rupture>, deja_signalees: usize },
    /// Agent installé ou mis à jour depuis l'Agent Market ou un fichier (lot T4).
    AgentInstalle { agent_id: String, version: String, ancienne_version: Option<String>, certification: String, identifiants_manquants: Vec<String> },
    /// Mission d'un agent (lot T3) : début, avec ses dossiers.
    MissionDebut { run_id: String, mission_id: String, agent_id: String, projet: String, dossier_mission: String, dossier_projet: String },
    /// Pensée du modèle de l'agent, vue par la porte (D14).
    MissionPensee { run_id: String, mission_id: String, agent_id: String, texte: String },
    /// Action de l'agent : « observee » (vue par la porte) ou « declaree » (compte rendu) (D14).
    MissionAction { run_id: String, mission_id: String, agent_id: String, nature: String, action: String, detail: String, raison: String },
    /// Livrables annoncés, avec leur présence vérifiée sur le disque (D14 ; vérification complète au lot T5).
    MissionLivrables { run_id: String, mission_id: String, agent_id: String, fichiers: Vec<serde_json::Value> },
    /// Changements de la mémoire propre de l'agent (D32).
    MemoireAgent { run_id: String, mission_id: String, agent_id: String, changements: Vec<serde_json::Value> },
    /// Fin de mission (le journal du jour référence ainsi chaque mission).
    MissionFin { run_id: String, mission_id: String, agent_id: String, etat: String, motif: String },
    /// Limite D29 atteinte (lot T7) : la mission est en pause, PB décide de continuer ou d'arrêter.
    LimiteAtteinte { run_id: String, mission_id: String, agent_id: String, texte: String },
    /// Ordre de PB appliqué à la mission en cours (pause, reprendre, arret, arret_immediat).
    MissionOrdre { mission_id: String, ordre: String },
    /// Vérification de la mission (lot T5) : écarts, contrôles, et confiance qui en découle.
    MissionVerification { run_id: String, mission_id: String, agent_id: String, ecarts: Vec<String>, controles: Vec<String>, statut: String, total_ecarts: u32, confiance_retiree: bool },
    /// Action irréversible en attente de la réponse de PB (D28).
    ValidationRequise { run_id: String, cle: String, mission_id: String, agent_id: String, action: String, raison: String },
    /// Réponse donnée (par PB, ou automatiquement par une autorisation en cours).
    ValidationReglee { run_id: String, cle: String, mission_id: String, decision: String, portee: String, automatique: bool },
    Error { run_id: String, message: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Recalled {
    pub file: String,
    pub section: String,
    pub text: String,
    pub score: f32,
    pub reason: String,
}

/// Destination des événements d'un tour : l'interface (et le journal) dans l'application,
/// une simple liste dans le lanceur d'évaluation.
pub trait Sortie: Send + Sync {
    fn envoyer(&self, ev: Event);
}

impl Sortie for AppHandle {
    fn envoyer(&self, ev: Event) {
        crate::journal::append(self, &ev);
        let _ = Emitter::emit(self, CHANNEL, ev);
    }
}

pub fn emit(out: &dyn Sortie, ev: Event) {
    out.envoyer(ev);
}
