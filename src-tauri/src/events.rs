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
    Done { run_id: String, answer: String, intent: String },
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

pub fn emit(app: &AppHandle, ev: Event) {
    crate::journal::append(app, &ev);
    let _ = app.emit(CHANNEL, ev);
}
