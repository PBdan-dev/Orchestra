//! Mesures des appels au cerveau (Phase 0) : une ligne par appel dans
//! `mesures/cerveau-AAAA-MM-JJ.jsonl`, pour prouver chaque gain par un chiffre avant/après.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Un appel au cerveau. Les durées sont en millisecondes ; un champ vide veut dire
/// que le fournisseur ne donne pas l'information.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Mesure {
    pub t: String,
    /// analyse | reponse | memorisation | agent | compaction
    pub role: String,
    pub fournisseur: String,
    pub modele: String,
    /// Tokens du prompt réellement calculés (hors cache).
    pub prompt_eval_count: Option<u64>,
    pub prompt_eval_ms: Option<f64>,
    /// Tokens du prompt relus depuis le cache (Claude).
    pub cache_lu: Option<u64>,
    pub eval_count: Option<u64>,
    pub eval_ms: Option<f64>,
    pub chargement_ms: Option<f64>,
    /// Temps entre l'envoi de la requête et le premier fragment (réflexion ou réponse).
    pub premier_token_ms: Option<f64>,
    pub total_ms: f64,
    /// Numéro de l'essai (2 = nouvel essai après une erreur passagère).
    #[serde(default)]
    pub essai: u32,
    /// Appels d'outils natifs reçus dans la réponse.
    #[serde(default)]
    pub appels_outils: u32,
    /// La sortie a dû être lue dans le texte (modèle sans outils ni sortie contrainte).
    #[serde(default)]
    pub repli_texte: bool,
    pub ok: bool,
    pub erreur: Option<String>,
}

impl Mesure {
    pub fn nouvelle(role: &str, fournisseur: &str, modele: &str) -> Self {
        Mesure {
            t: chrono::Local::now().to_rfc3339(),
            role: role.into(),
            fournisseur: fournisseur.into(),
            modele: modele.into(),
            ..Default::default()
        }
    }

    /// Statistiques du dernier fragment d'Ollama (`"done": true`) ; durées en nanosecondes.
    pub fn lire_ollama(&mut self, v: &Value) {
        if v["done_reason"] == "length" {
            self.erreur = Some("réserve de tokens atteinte (réponse coupée)".into());
        }
        let ms = |k: &str| v[k].as_f64().map(|ns| ns / 1_000_000.0);
        self.prompt_eval_count = v["prompt_eval_count"].as_u64().or(self.prompt_eval_count);
        self.prompt_eval_ms = ms("prompt_eval_duration").or(self.prompt_eval_ms);
        self.eval_count = v["eval_count"].as_u64().or(self.eval_count);
        self.eval_ms = ms("eval_duration").or(self.eval_ms);
        self.chargement_ms = ms("load_duration").or(self.chargement_ms);
    }

    /// Usage de l'API Claude : `message_start` (entrée) puis `message_delta` (sortie).
    pub fn lire_claude(&mut self, usage: &Value) {
        if let Some(i) = usage["input_tokens"].as_u64() {
            let creation = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
            self.prompt_eval_count = Some(i + creation);
        }
        if let Some(c) = usage["cache_read_input_tokens"].as_u64() {
            self.cache_lu = Some(c);
        }
        if let Some(o) = usage["output_tokens"].as_u64() {
            self.eval_count = Some(o);
        }
    }

    /// Usage d'un point d'accès compatible OpenAI (dernier fragment, si demandé).
    pub fn lire_openai(&mut self, usage: &Value) {
        if let Some(p) = usage["prompt_tokens"].as_u64() {
            let cache = usage["prompt_tokens_details"]["cached_tokens"].as_u64();
            self.prompt_eval_count = Some(p - cache.unwrap_or(0).min(p));
            self.cache_lu = cache;
        }
        if let Some(c) = usage["completion_tokens"].as_u64() {
            self.eval_count = Some(c);
        }
    }

    /// `usageMetadata` de Gemini.
    pub fn lire_gemini(&mut self, usage: &Value) {
        if let Some(p) = usage["promptTokenCount"].as_u64() {
            let cache = usage["cachedContentTokenCount"].as_u64();
            self.prompt_eval_count = Some(p - cache.unwrap_or(0).min(p));
            self.cache_lu = cache;
        }
        if let Some(c) = usage["candidatesTokenCount"].as_u64() {
            self.eval_count = Some(c);
        }
    }
}

/// Relevé partagé par les cerveaux d'un même tour : garde les mesures en mémoire
/// (pour le lanceur d'évaluation) et les écrit sur disque si un dossier est donné.
#[derive(Default)]
pub struct Releve {
    pub dossier: Option<PathBuf>,
    lignes: Mutex<Vec<Mesure>>,
}

impl Releve {
    pub fn vers(dossier: PathBuf) -> Self {
        Releve { dossier: Some(dossier), lignes: Mutex::new(vec![]) }
    }

    pub fn noter(&self, m: Mesure) {
        if let Some(d) = &self.dossier {
            let _ = std::fs::create_dir_all(d);
            let jour = chrono::Local::now().format("%Y-%m-%d");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join(format!("cerveau-{jour}.jsonl"))) {
                if let Ok(s) = serde_json::to_string(&m) {
                    let _ = writeln!(f, "{s}");
                }
            }
        }
        self.lignes.lock().unwrap().push(m);
    }

    pub fn lignes(&self) -> Vec<Mesure> {
        self.lignes.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ollama_dernier_fragment() {
        let mut m = Mesure::nouvelle("reponse", "ollama", "qwen");
        m.lire_ollama(&json!({
            "done": true, "prompt_eval_count": 1234, "prompt_eval_duration": 250_000_000u64,
            "eval_count": 56, "eval_duration": 1_500_000_000u64, "load_duration": 3_000_000u64
        }));
        assert_eq!(m.prompt_eval_count, Some(1234));
        assert_eq!(m.prompt_eval_ms, Some(250.0));
        assert_eq!(m.eval_count, Some(56));
        assert_eq!(m.eval_ms, Some(1500.0));
        assert_eq!(m.chargement_ms, Some(3.0));
    }

    #[test]
    fn ollama_fragment_sans_stats_ne_remet_pas_a_zero() {
        let mut m = Mesure::nouvelle("reponse", "ollama", "qwen");
        m.lire_ollama(&json!({ "prompt_eval_count": 10 }));
        m.lire_ollama(&json!({ "message": { "content": "x" } }));
        assert_eq!(m.prompt_eval_count, Some(10));
    }

    #[test]
    fn claude_usage_avec_cache() {
        let mut m = Mesure::default();
        m.lire_claude(&json!({ "input_tokens": 20, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 900 }));
        m.lire_claude(&json!({ "output_tokens": 42 }));
        assert_eq!(m.prompt_eval_count, Some(120));
        assert_eq!(m.cache_lu, Some(900));
        assert_eq!(m.eval_count, Some(42));
    }

    #[test]
    fn openai_usage_retire_le_cache() {
        let mut m = Mesure::default();
        m.lire_openai(&json!({ "prompt_tokens": 1000, "completion_tokens": 30, "prompt_tokens_details": { "cached_tokens": 800 } }));
        assert_eq!(m.prompt_eval_count, Some(200));
        assert_eq!(m.cache_lu, Some(800));
        assert_eq!(m.eval_count, Some(30));
    }

    #[test]
    fn releve_ecrit_une_ligne_par_appel() {
        let dir = std::env::temp_dir().join(format!("orchestra-test-releve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = Releve::vers(dir.clone());
        r.noter(Mesure::nouvelle("analyse", "ollama", "m"));
        r.noter(Mesure::nouvelle("reponse", "ollama", "m"));
        assert_eq!(r.lignes().len(), 2);
        let f = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let txt = std::fs::read_to_string(f).unwrap();
        assert_eq!(txt.lines().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
