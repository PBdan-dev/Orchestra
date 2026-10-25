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
