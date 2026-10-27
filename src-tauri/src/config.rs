use serde::{Deserialize, Serialize};

use crate::paths::Paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    /// ollama | lmstudio | claude | gemini | openrouter
    pub provider: String,
    /// Modèle à utiliser. Vide = détection automatique (Ollama : modèle actif ou premier installé).
    pub model: String,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub claude_api_key: String,
    pub gemini_api_key: String,
    pub openrouter_api_key: String,
    /// URL de l'Agent Market. Vide = registre local seulement.
    pub market_url: String,
    /// Index vectoriel local via les embeddings d'Ollama (optionnel).
    pub embeddings_enabled: bool,
    pub embedding_model: String,
    pub user_name: String,
    /// Nombre maximal d'itérations outil pour un agent.
    pub max_agent_steps: u32,
    /// Mode vocal (module optionnel, serveur Pipecat local).
    pub voice_enabled: bool,
    /// Démarrer le serveur voix au lancement d'Orchestra.
    pub voice_autostart: bool,
    /// « mains-libres » (le VAD détecte la fin de phrase) ou « appuyer » (appuyer pour parler).
    pub voice_mode: String,
    /// Touche d'appuyer-pour-parler (KeyboardEvent.code).
    pub voice_ptt_key: String,
    /// Lire à voix haute les réponses aux messages tapés.
    pub voice_read_chat: bool,
    /// Le micro reste ouvert pendant qu'Orchestra parle (on peut lui couper la parole).
    pub voice_barge_in: bool,
    /// Cerveau du mode vocal (rapide) ; le mode texte garde `model`.
    pub voice_llm: String,
    /// Voix préréglée de Qwen3-TTS.
    pub voice_speaker: String,
    /// Fichier de voix/references/ à cloner (vide = voix préréglée). Consentement obligatoire.
    pub voice_reference: String,
    pub voice_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            provider: "ollama".into(),
            model: String::new(),
            ollama_url: "http://127.0.0.1:11434".into(),
            lmstudio_url: "http://127.0.0.1:1234".into(),
            claude_api_key: String::new(),
            gemini_api_key: String::new(),
            openrouter_api_key: String::new(),
            market_url: String::new(),
            embeddings_enabled: false,
            embedding_model: "nomic-embed-text".into(),
            user_name: String::new(),
            max_agent_steps: 6,
            voice_enabled: false,
            voice_autostart: true,
            voice_mode: "mains-libres".into(),
            voice_ptt_key: "F2".into(),
            voice_read_chat: false,
            voice_barge_in: false,
            voice_llm: "qwen3.5:9b".into(),
            voice_speaker: "Ryan".into(),
            voice_reference: String::new(),
            voice_port: 8765,
        }
    }
}

impl Config {
    pub fn load(paths: &Paths) -> Config {
        std::fs::read_to_string(&paths.config)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(&paths.config, s)?;
        Ok(())
    }
}
