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
    /// Clés API (lot S-c) : lues dans le coffre de Windows, jamais écrites dans `config.json` ni
    /// envoyées à l'interface. Une ancienne clé encore dans le fichier est lue pour être migrée.
    #[serde(skip_serializing)]
    pub claude_api_key: String,
    #[serde(skip_serializing)]
    pub gemini_api_key: String,
    #[serde(skip_serializing)]
    pub openrouter_api_key: String,
    /// Clés laissées dans `config.json` parce que le coffre les a refusées : gardées à l'enregistrement.
    #[serde(skip)]
    pub cles_en_clair: Vec<String>,
    /// URL de l'Agent Market, ou chemin d'un catalogue local (essais). Vide = Market officiel (D19).
    pub market_url: String,
    /// Mode développement (lot T4) : accepte les agents non certifiés, avec le badge « Non certifié ».
    pub mode_developpement: bool,
    /// « serie » (défaut) : une mission qui prend la carte graphique coupe la conversation, après
    /// confirmation de PB ; « parallele » : la conversation garde son modèle pendant les missions.
    pub mode_execution: String,
    /// Rappel par le sens (lot M2) : vecteurs du modèle d'embedding d'Ollama, sur le processeur (D10).
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
    /// Modèle d'écoute : fichier GGUF de voix/modeles/voxtral.
    pub voice_stt_model: String,
    /// Modèle de synthèse : dossier de voix/modeles/qwen3-tts.
    pub voice_tts_model: String,
    /// Taille du contexte du cerveau du chat (tokens), fixe pour garder le cache (lot A).
    pub num_ctx: u32,
    /// Taille du contexte du cerveau vocal (modèle dérivé `<modèle>-orchestra` dans Ollama).
    pub voice_num_ctx: u32,
    /// Historique du chat : au-delà du seuil haut, on retire les plus anciens échanges jusqu'au seuil bas (tokens estimés).
    pub historique_seuil_haut: u32,
    pub historique_seuil_bas: u32,
    /// Projet où vont les missions et les livrables (`projets/<nom>/`), choisi par PB dans l'interface (D36).
    pub projet_actif: String,
    /// Réserve pour la réflexion et la réponse (tokens générés au plus par appel), §5.2.
    pub reserve_reponse: u32,
    /// Température imposée (lanceur d'évaluation seulement, jamais enregistrée).
    #[serde(skip)]
    pub eval_temperature: Option<f32>,
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
            cles_en_clair: vec![],
            market_url: String::new(),
            mode_developpement: false,
            mode_execution: "serie".into(),
            // lot M2 : rappel par le sens, modèle choisi en D10
            embeddings_enabled: true,
            embedding_model: MODELE_SENS.into(),
            user_name: String::new(),
            max_agent_steps: 6,
            voice_enabled: false,
            voice_autostart: true,
            voice_mode: "mains-libres".into(),
            voice_ptt_key: "F2".into(),
            voice_read_chat: false,
            voice_barge_in: true,
            voice_llm: "qwen3.5:9b".into(),
            voice_speaker: "Ryan".into(),
            voice_reference: String::new(),
            voice_port: 8765,
            voice_stt_model: "Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf".into(),
            voice_tts_model: "Qwen3-TTS-12Hz-1.7B-CustomVoice".into(),
            num_ctx: 16384,
            voice_num_ctx: 8192,
            historique_seuil_haut: 8000,
            historique_seuil_bas: 4000,
            reserve_reponse: 4500,
            projet_actif: "divers".into(),
            eval_temperature: None,
        }
    }
}

/// Modèle d'embedding du rappel par le sens (D10).
pub const MODELE_SENS: &str = "qwen3-embedding:0.6b";

impl Config {
    pub fn load(paths: &Paths) -> Config {
        Self::charger(paths, &crate::coffre::CoffreWindows).0
    }

    /// Lit `config.json` et les clés API du coffre (lot S-c). Une clé encore écrite dans
    /// `config.json` est rangée dans le coffre puis retirée du fichier ; si le coffre la refuse,
    /// elle reste où elle est. Renvoie aussi ce qui a été fait, pour Conscience et le journal.
    pub fn charger(paths: &Paths, coffre: &dyn crate::coffre::Coffre) -> (Config, Vec<String>) {
        let mut cfg: Config = std::fs::read_to_string(&paths.config)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut notes = vec![];
        let mut migrees = 0;
        for (fournisseur, _) in crate::coffre::FOURNISSEURS {
            let cible = crate::coffre::cible_cle_api(paths, fournisseur);
            let en_clair = cfg.cle(fournisseur).trim().to_string();
            if en_clair.is_empty() {
                *cfg.cle_mut(fournisseur) = coffre.lire(&cible).unwrap_or_default();
                continue;
            }
            // clé encore dans le fichier : rangée dans le coffre, relue, puis retirée du fichier
            match coffre.ecrire(&cible, &en_clair).and_then(|_| if coffre.lire(&cible).as_deref() == Some(en_clair.as_str()) { Ok(()) } else { Err("relecture différente".into()) }) {
                Ok(()) => {
                    migrees += 1;
                    notes.push(format!("Clé API {fournisseur} déplacée de config.json vers le coffre de Windows."));
                }
                Err(e) => {
                    cfg.cles_en_clair.push(fournisseur.to_string());
                    notes.push(format!("Clé API {fournisseur} laissée dans config.json : {e}. Elle y reste lisible tant que le coffre de Windows la refuse."));
                }
            }
        }
        // lot M2 : l'ancien réglage par défaut (index vectoriel désactivé, nomic-embed-text, jamais
        // installé) passe au rappel par le sens avec le modèle de D10, une seule fois
        let mut a_ecrire = migrees > 0;
        if cfg.embedding_model.trim().is_empty() || cfg.embedding_model == "nomic-embed-text" {
            cfg.embedding_model = MODELE_SENS.into();
            cfg.embeddings_enabled = true;
            a_ecrire = true;
            notes.push(format!("Rappel par le sens activé avec {MODELE_SENS} (lot M2, D10)."));
        }
        if a_ecrire {
            if let Err(e) = cfg.save(paths) {
                notes.push(format!("config.json non réécrit après la mise à jour des réglages : {e}"));
            }
        }
        (cfg, notes)
    }

    fn cle(&self, fournisseur: &str) -> &String {
        match fournisseur {
            "claude" => &self.claude_api_key,
            "gemini" => &self.gemini_api_key,
            _ => &self.openrouter_api_key,
        }
    }

    pub fn cle_mut(&mut self, fournisseur: &str) -> &mut String {
        match fournisseur {
            "claude" => &mut self.claude_api_key,
            "gemini" => &mut self.gemini_api_key,
            _ => &mut self.openrouter_api_key,
        }
    }

    /// Une clé API est-elle connue d'Orchestra (dans le coffre, ou laissée en clair) ?
    pub fn cle_presente(&self, fournisseur: &str) -> bool {
        !self.cle(fournisseur).trim().is_empty()
    }

    /// Écrit `config.json`, sans les clés API ; seules celles que le coffre a refusées y restent.
    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        let mut v = serde_json::to_value(self)?;
        for (fournisseur, champ) in crate::coffre::FOURNISSEURS {
            if self.cles_en_clair.iter().any(|c| c == fournisseur) {
                v[champ] = serde_json::json!(self.cle(fournisseur));
            }
        }
        std::fs::write(&paths.config, serde_json::to_string_pretty(&v)?)?;
        Ok(())
    }

    /// Réglages envoyés par l'interface, qui ne connaît pas les clés (lot S-c) : on garde celles
    /// d'Orchestra.
    pub fn garder_cles(&mut self, actuelle: &Config) {
        self.claude_api_key = actuelle.claude_api_key.clone();
        self.gemini_api_key = actuelle.gemini_api_key.clone();
        self.openrouter_api_key = actuelle.openrouter_api_key.clone();
        self.cles_en_clair = actuelle.cles_en_clair.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffre::{cible_cle_api, Coffre, CoffreMemoire};

    fn dossier() -> (Paths, std::path::PathBuf) {
        let racine = std::env::temp_dir().join(format!("orchestra-sc-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        (Paths::at(racine.clone()).unwrap(), racine)
    }

    #[test]
    fn aucune_cle_dans_le_fichier_ni_vers_l_interface() {
        let c = Config { claude_api_key: "sk-ant-secret".into(), openrouter_api_key: "sk-or-secret".into(), ..Config::default() };
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("secret"), "{json}");
    }

    #[test]
    fn cles_en_clair_migrees_dans_le_coffre() {
        let (paths, racine) = dossier();
        std::fs::write(&paths.config, r#"{"provider":"claude","claude_api_key":"sk-ant-123","gemini_api_key":""}"#).unwrap();
        let coffre = CoffreMemoire::default();
        let (cfg, notes) = Config::charger(&paths, &coffre);
        assert_eq!(cfg.claude_api_key, "sk-ant-123");
        assert_eq!(cfg.provider, "claude");
        assert_eq!(coffre.lire(&cible_cle_api(&paths, "claude")).as_deref(), Some("sk-ant-123"));
        assert_eq!(coffre.lire(&cible_cle_api(&paths, "gemini")), None, "clé vide : rien à ranger");
        let fichier = std::fs::read_to_string(&paths.config).unwrap();
        assert!(!fichier.contains("sk-ant-123") && fichier.contains("\"provider\": \"claude\""), "{fichier}");
        assert_eq!(notes.len(), 1, "{notes:?}");
        // au lancement suivant, la clé vient du coffre
        let (cfg2, notes2) = Config::charger(&paths, &coffre);
        assert_eq!(cfg2.claude_api_key, "sk-ant-123");
        assert!(notes2.is_empty(), "{notes2:?}");
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn coffre_inaccessible_la_cle_reste_en_place() {
        let (paths, racine) = dossier();
        std::fs::write(&paths.config, r#"{"gemini_api_key":"AIza-456"}"#).unwrap();
        let coffre = CoffreMemoire { refuse: true, ..Default::default() };
        let (mut cfg, notes) = Config::charger(&paths, &coffre);
        assert_eq!(cfg.gemini_api_key, "AIza-456");
        assert!(notes.iter().any(|n| n.contains("gemini")), "{notes:?}");
        // un réglage enregistré plus tard ne la perd pas
        cfg.user_name = "PB".into();
        cfg.save(&paths).unwrap();
        assert!(std::fs::read_to_string(&paths.config).unwrap().contains("AIza-456"));
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn rappel_par_le_sens_active_avec_le_modele_d10() {
        let (paths, racine) = dossier();
        // configuration d'avant le lot M2 : ancien modèle par défaut, index vectoriel désactivé
        std::fs::write(&paths.config, r#"{"embeddings_enabled":false,"embedding_model":"nomic-embed-text","user_name":"PB"}"#).unwrap();
        let (cfg, notes) = Config::charger(&paths, &CoffreMemoire::default());
        assert!(cfg.embeddings_enabled && cfg.embedding_model == "qwen3-embedding:0.6b", "{cfg:?}");
        assert!(notes.iter().any(|n| n.contains("qwen3-embedding")), "{notes:?}");
        // enregistré : la migration n'a lieu qu'une fois, et un choix de PB est respecté ensuite
        let (cfg2, notes2) = Config::charger(&paths, &CoffreMemoire::default());
        assert!(cfg2.embeddings_enabled && notes2.is_empty(), "{notes2:?}");
        std::fs::write(&paths.config, r#"{"embeddings_enabled":false,"embedding_model":"qwen3-embedding:0.6b"}"#).unwrap();
        assert!(!Config::charger(&paths, &CoffreMemoire::default()).0.embeddings_enabled, "désactivé par PB : gardé");
        let defaut = Config::default();
        assert!(defaut.embeddings_enabled && defaut.embedding_model == "qwen3-embedding:0.6b");
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn reglages_de_l_interface_gardent_les_cles() {
        let actuelle = Config { claude_api_key: "sk-ant-1".into(), ..Config::default() };
        // l'interface renvoie ses réglages sans les clés
        let mut recue: Config = serde_json::from_str(&serde_json::to_string(&actuelle).unwrap()).unwrap();
        recue.user_name = "PB".into();
        recue.garder_cles(&actuelle);
        assert_eq!((recue.claude_api_key.as_str(), recue.user_name.as_str()), ("sk-ant-1", "PB"));
    }
}
