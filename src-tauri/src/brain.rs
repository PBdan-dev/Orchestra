//! Le cerveau : un fournisseur de modèle greffé à Orchestra.
//! Ollama (défaut, local), LM Studio (local, API OpenAI), Claude, Gemini, OpenRouter.
//!
//! Deux façons d'obtenir une sortie structurée, sans lire du JSON dans du texte libre :
//! - l'appel d'outils natif de chaque fournisseur (`appeler` avec des `outils`) ;
//! - la sortie contrainte par un schéma JSON (`json`).
//! Si le modèle ne les gère pas, on retombe sur la lecture du texte, et l'appelant le signale.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    config::Config,
    mesures::{Mesure, Releve},
};

/// Attente maximale de la première réponse (chargement du modèle et calcul du prompt compris).
pub const DELAI_PREMIER: Duration = Duration::from_secs(120);
/// Silence maximal au milieu d'une réponse.
pub const DELAI_SILENCE: Duration = Duration::from_secs(60);
/// Durée maximale d'un appel complet.
pub const DELAI_TOTAL: Duration = Duration::from_secs(600);
/// Délai de connexion (à poser sur le client HTTP).
pub const DELAI_CONNEXION: Duration = Duration::from_secs(10);
/// Le modèle du mode actif reste chargé tant qu'Orchestra est ouvert (lot A) ; il est déchargé
/// au changement de mode et à la fermeture.
pub const KEEP_ALIVE: i64 = -1;

/// Nom du modèle vocal dérivé (même poids, `num_ctx` réduit) : `qwen3.5:9b` → `qwen3.5:9b-orchestra`.
/// Même règle dans `voix/orchestra_voix/server.py`.
pub fn modele_vocal_derive(base: &str) -> String {
    if base.ends_with("-orchestra") || base.is_empty() {
        base.to_string()
    } else if base.contains(':') {
        format!("{base}-orchestra")
    } else {
        format!("{base}:latest-orchestra")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Msg {
    pub role: String,
    pub content: String,
    /// Appels d'outils décidés par le modèle (message `assistant`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appels: Vec<AppelOutil>,
    /// Résultat d'outil (rôle `tool`) : identifiant et nom de l'appel auquel il répond.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outil_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outil_nom: Option<String>,
}

impl Msg {
    pub fn new(role: impl Into<String>, s: impl Into<String>) -> Self { Msg { role: role.into(), content: s.into(), ..Default::default() } }
    pub fn system(s: impl Into<String>) -> Self { Msg::new("system", s) }
    pub fn user(s: impl Into<String>) -> Self { Msg::new("user", s) }
    pub fn assistant(s: impl Into<String>) -> Self { Msg::new("assistant", s) }
    /// Réponse du modèle qui appelle des outils.
    pub fn appels_outils(texte: impl Into<String>, appels: Vec<AppelOutil>) -> Self {
        Msg { appels, ..Msg::assistant(texte) }
    }
    /// Résultat d'un outil, renvoyé au modèle.
    pub fn resultat_outil(appel: &AppelOutil, sortie: impl Into<String>) -> Self {
        Msg { outil_id: Some(appel.id.clone()), outil_nom: Some(appel.nom.clone()), ..Msg::new("tool", sortie) }
    }
}

/// Un outil proposé au modèle : nom, description, paramètres (schéma JSON).
#[derive(Serialize, Clone, Debug)]
pub struct Outil {
    pub nom: String,
    pub description: String,
    pub parametres: Value,
}

/// Un appel d'outil décidé par le modèle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppelOutil {
    pub id: String,
    pub nom: String,
    pub arguments: Value,
}

/// Ce qu'on demande en plus du texte : des outils, ou une sortie au format d'un schéma.
#[derive(Default, Clone, Copy)]
pub struct Demande<'a> {
    pub outils: &'a [Outil],
    pub schema: Option<&'a Value>,
    /// Appel en texte seul parce que le modèle a refusé outils ou schéma (noté dans la mesure).
    pub texte_seul_en_repli: bool,
}

impl Demande<'_> {
    pub fn repli() -> Self {
        Demande { texte_seul_en_repli: true, ..Default::default() }
    }

    fn contrainte(&self) -> bool {
        !self.outils.is_empty() || self.schema.is_some()
    }
}

/// Réponse complète d'un appel.
#[derive(Debug, Default, Clone)]
pub struct Reponse {
    pub texte: String,
    pub appels: Vec<AppelOutil>,
}

/// Erreurs du cerveau, avec un message lisible et la règle de nouvel essai.
#[derive(Debug)]
pub enum ErreurCerveau {
    Connexion(String),
    DelaiPremier,
    Silence,
    Http { statut: u16, corps: String },
    /// Le modèle refuse les outils ou la sortie contrainte.
    NonSupporte(String),
    Fournisseur(String),
}

impl std::fmt::Display for ErreurCerveau {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurCerveau::Connexion(e) => write!(f, "connexion au cerveau impossible ({e})"),
            ErreurCerveau::DelaiPremier => write!(f, "le cerveau n'a pas commencé à répondre en {} s", DELAI_PREMIER.as_secs()),
            ErreurCerveau::Silence => write!(f, "le cerveau ne répond plus depuis {} s", DELAI_SILENCE.as_secs()),
            ErreurCerveau::Http { statut, corps } => write!(f, "le cerveau a répondu {statut} : {corps}"),
            ErreurCerveau::NonSupporte(e) => write!(f, "ce modèle ne gère pas les outils ou les sorties contraintes ({e})"),
            ErreurCerveau::Fournisseur(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ErreurCerveau {}

impl ErreurCerveau {
    /// Un nouvel essai n'a de sens que si rien n'a été reçu et que l'erreur peut être passagère.
    pub fn rejouable(&self) -> bool {
        match self {
            ErreurCerveau::Connexion(_) | ErreurCerveau::DelaiPremier => true,
            ErreurCerveau::Http { statut, .. } => *statut >= 500 || *statut == 429,
            _ => false,
        }
    }
}

pub fn non_supporte(e: &anyhow::Error) -> bool {
    matches!(e.downcast_ref::<ErreurCerveau>(), Some(ErreurCerveau::NonSupporte(_)))
}

/// Un fragment reçu en streaming : réflexion et/ou contenu.
#[derive(Default, Debug, Clone)]
pub struct Chunk {
    pub thinking: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BrainInfo {
    pub provider: String,
    pub model: String,
    pub online: bool,
    pub models: Vec<String>,
    pub detail: String,
    /// Le modèle est-il chargé en mémoire (Ollama) ? Toujours vrai pour les autres fournisseurs.
    pub loaded: bool,
    /// offline | not_loaded | ready | no_model | no_key
    pub state: String,
}

pub struct Brain {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub model: String,
    /// Mesures de chaque appel (partagées entre les cerveaux d'un même tour).
    pub releve: Arc<Releve>,
}

/// Ce qu'un fournisseur remplit pendant un appel.
struct Collecte<'a> {
    mesure: &'a mut Mesure,
    appels: &'a mut Vec<AppelOutil>,
}

impl Brain {
    pub fn new(cfg: Config, http: reqwest::Client) -> Self {
        Brain { model: cfg.model.clone(), cfg, http, releve: Arc::new(Releve::default()) }
    }

    /// Même cerveau, avec un relevé de mesures donné.
    pub fn avec_releve(mut self, releve: Arc<Releve>) -> Self {
        self.releve = releve;
        self
    }

    pub fn label(&self) -> String {
        format!("{} · {}", provider_label(&self.cfg.provider), self.model)
    }

    /// Résout le modèle (détection automatique si vide) et vérifie la connexion.
    pub async fn detect(&mut self) -> BrainInfo {
        let mut info = BrainInfo { provider: self.cfg.provider.clone(), ..Default::default() };
        match self.cfg.provider.as_str() {
            "ollama" => {
                let base = self.cfg.ollama_url.trim_end_matches('/').to_string();
                // deux essais : Ollama peut mettre un instant à répondre au démarrage
                let mut tags = self.get_json(&format!("{base}/api/tags")).await;
                if tags.is_err() {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    tags = self.get_json(&format!("{base}/api/tags")).await;
                }
                match tags {
                    Ok(v) => {
                        info.online = true;
                        info.models = v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                        let running: Vec<String> = self
                            .get_json(&format!("{base}/api/ps"))
                            .await
                            .ok()
                            .and_then(|v| v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(|s| s.to_string())).collect()))
                            .unwrap_or_default();
                        if self.cfg.model.trim().is_empty() {
                            if let Some(r) = running.first() {
                                self.model = r.clone();
                            } else if let Some(first) = pick_default_ollama(&info.models) {
                                self.model = first;
                            }
                        }
                        let same = |a: &str, b: &str| a == b || a.trim_end_matches(":latest") == b.trim_end_matches(":latest");
                        info.loaded = !self.model.is_empty() && running.iter().any(|r| same(r, &self.model));
                        if self.model.is_empty() {
                            info.state = "no_model".into();
                            info.detail = "Ollama tourne mais aucun modèle n'est installé (ollama pull …)".into();
                        } else if info.loaded {
                            info.state = "ready".into();
                            info.detail = "modèle chargé, prêt".into();
                        } else {
                            info.state = "not_loaded".into();
                            info.detail = "modèle non chargé : il le sera à la première demande, ou avec le bouton Charger".into();
                        }
                    }
                    Err(e) => {
                        info.state = "offline".into();
                        info.detail = format!("Ollama injoignable sur {base} ({})", short_err(&e));
                    }
                }
            }
            "lmstudio" => {
                let base = self.cfg.lmstudio_url.trim_end_matches('/');
                match self.get_json(&format!("{base}/v1/models")).await {
                    Ok(v) => {
                        info.online = true;
                        info.models = v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                        if self.cfg.model.trim().is_empty() {
                            if let Some(f) = info.models.first() {
                                self.model = f.clone();
                                info.detail = "premier modèle chargé dans LM Studio".into();
                            } else {
                                info.detail = "LM Studio tourne mais aucun modèle n'est chargé".into();
                            }
                        }
                    }
                    Err(e) => info.detail = format!("LM Studio injoignable sur {base} : {e}"),
                }
            }
            "claude" => {
                if self.cfg.model.trim().is_empty() { self.model = "claude-sonnet-4-5".into(); }
                info.online = !self.cfg.claude_api_key.trim().is_empty();
                info.detail = if info.online { "clé API renseignée".into() } else { "clé API Claude manquante".into() };
                info.models = vec!["claude-sonnet-4-5".into(), "claude-opus-4-1".into(), "claude-haiku-4-5".into()];
            }
            "gemini" => {
                if self.cfg.model.trim().is_empty() { self.model = "gemini-2.5-flash".into(); }
                info.online = !self.cfg.gemini_api_key.trim().is_empty();
                info.detail = if info.online { "clé API renseignée".into() } else { "clé API Gemini manquante".into() };
                info.models = vec!["gemini-2.5-flash".into(), "gemini-2.5-pro".into()];
            }
            "openrouter" => {
                if self.cfg.model.trim().is_empty() { self.model = "openai/gpt-4o-mini".into(); }
                info.online = !self.cfg.openrouter_api_key.trim().is_empty();
                info.detail = if info.online { "clé API renseignée".into() } else { "clé API OpenRouter manquante".into() };
                info.models = vec!["openai/gpt-4o-mini".into(), "anthropic/claude-sonnet-4.5".into(), "google/gemini-2.5-flash".into(), "meta-llama/llama-3.3-70b-instruct".into()];
            }
            other => info.detail = format!("fournisseur inconnu : {other}"),
        }
        info.model = self.model.clone();
        if info.state.is_empty() {
            info.loaded = info.online;
            info.state = if info.online {
                if self.model.is_empty() { "no_model".into() } else { "ready".into() }
            } else if ["claude", "gemini", "openrouter"].contains(&self.cfg.provider.as_str()) {
                "no_key".into()
            } else {
                "offline".into()
            };
        }
        info
    }

    /// Charge le modèle dans Ollama (requête vide), avec le même `num_ctx` que les appels : sinon
    /// Ollama le recharge au premier vrai appel. Il reste chargé tant qu'Orchestra est ouvert.
    pub async fn load_ollama(&self) -> anyhow::Result<()> {
        if self.model.is_empty() {
            anyhow::bail!("aucun modèle à charger");
        }
        let base = self.cfg.ollama_url.trim_end_matches('/');
        let r = self
            .http
            .post(format!("{base}/api/generate"))
            .timeout(Duration::from_secs(900))
            .json(&json!({ "model": self.model, "prompt": "", "keep_alive": KEEP_ALIVE, "stream": false, "options": { "num_ctx": self.cfg.num_ctx } }))
            .send()
            .await?;
        if !r.status().is_success() {
            let s = r.status();
            let body = r.text().await.unwrap_or_default();
            anyhow::bail!("Ollama a répondu {s} : {}", body.chars().take(200).collect::<String>());
        }
        Ok(())
    }

    async fn get_json(&self, url: &str) -> anyhow::Result<Value> {
        let r = self.http.get(url).timeout(Duration::from_secs(8)).send().await?;
        Ok(r.json::<Value>().await?)
    }

    /// Embedding via Ollama (seul fournisseur local d'embeddings supporté en V1).
    pub async fn embed(&self, text: &str) -> anyhow::Result<Option<Vec<f32>>> {
        let base = self.cfg.ollama_url.trim_end_matches('/');
        let r = self
            .http
            .post(format!("{base}/api/embed"))
            .timeout(Duration::from_secs(30))
            .json(&json!({ "model": self.cfg.embedding_model, "input": text }))
            .send()
            .await?;
        if !r.status().is_success() {
            return Ok(None);
        }
        let v: Value = r.json().await?;
        Ok(v["embeddings"][0].as_array().map(|a| a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect()))
    }

    /// Appel en streaming, texte seul. `role` (analyse, reponse, memorisation, agent…) sert aux
    /// mesures ; `on` reçoit chaque fragment ; renvoie le contenu complet.
    pub async fn stream<F: FnMut(Chunk)>(&self, role: &str, msgs: &[Msg], on: F) -> anyhow::Result<String> {
        Ok(self.appeler(role, msgs, Demande::default(), on).await?.texte)
    }

    pub async fn complete(&self, role: &str, msgs: &[Msg]) -> anyhow::Result<String> {
        self.stream(role, msgs, |_| {}).await
    }

    /// Sortie contrainte par un schéma JSON. Renvoie l'objet et `true` si la contrainte a été
    /// appliquée par le fournisseur, `false` si l'on a dû lire le JSON dans le texte (repli).
    pub async fn json<F: FnMut(Chunk)>(&self, role: &str, msgs: &[Msg], schema: &Value, mut on: F) -> anyhow::Result<(Value, bool)> {
        match self.appeler(role, msgs, Demande { schema: Some(schema), ..Default::default() }, &mut on).await {
            Ok(r) => {
                // Claude rend la sortie contrainte sous forme d'appel d'outil imposé
                let brut = r.appels.first().map(|a| a.arguments.to_string()).unwrap_or(r.texte);
                match serde_json::from_str::<Value>(brut.trim()) {
                    Ok(v) => Ok((v, true)),
                    Err(e) => Err(ErreurCerveau::Fournisseur(format!("sortie non conforme au schéma ({e})")).into()),
                }
            }
            Err(e) if non_supporte(&e) => {
                let t = self.appeler(role, msgs, Demande::repli(), &mut on).await?.texte;
                let v = lire_json_dans_texte(&t).ok_or_else(|| ErreurCerveau::Fournisseur("aucun objet JSON dans la réponse".into()))?;
                Ok((v, false))
            }
            Err(e) => Err(e),
        }
    }

    /// Appel complet : texte, et selon la demande, appels d'outils ou sortie contrainte.
    /// Un seul nouvel essai, et seulement si rien n'a encore été reçu (connexion, délai, 5xx).
    pub async fn appeler<F: FnMut(Chunk)>(&self, role: &str, msgs: &[Msg], demande: Demande<'_>, mut on: F) -> anyhow::Result<Reponse> {
        let mut essai = 0;
        loop {
            essai += 1;
            let debut = Instant::now();
            let mut mesure = Mesure::nouvelle(role, &self.cfg.provider, &self.model);
            let mut appels: Vec<AppelOutil> = vec![];
            let mut premier: Option<f64> = None;
            let mut full = String::new();
            let mut splitter = ThinkSplitter::default();
            let res = {
                let mut emettre = |thinking: String, content: String| {
                    if thinking.is_empty() && content.is_empty() {
                        return;
                    }
                    if premier.is_none() {
                        premier = Some(debut.elapsed().as_secs_f64() * 1000.0);
                    }
                    full.push_str(&content);
                    on(Chunk { thinking, content });
                };
                let r = {
                    // sépare les <think>…</think> éventuellement inclus dans le contenu
                    let mut deliver = |raw: Chunk| {
                        let (th, co) = splitter.feed(&raw.content);
                        emettre(format!("{}{}", raw.thinking, th), co);
                    };
                    let mut col = Collecte { mesure: &mut mesure, appels: &mut appels };
                    match self.cfg.provider.as_str() {
                        "ollama" => self.stream_ollama(msgs, demande, &mut deliver, &mut col).await,
                        "lmstudio" => {
                            let base = format!("{}/v1", self.cfg.lmstudio_url.trim_end_matches('/'));
                            self.stream_openai(&base, "", msgs, demande, &mut deliver, &mut col).await
                        }
                        "openrouter" => self.stream_openai("https://openrouter.ai/api/v1", &self.cfg.openrouter_api_key, msgs, demande, &mut deliver, &mut col).await,
                        "claude" => self.stream_claude(msgs, demande, &mut deliver, &mut col).await,
                        "gemini" => self.stream_gemini(msgs, demande, &mut deliver, &mut col).await,
                        other => Err(anyhow::anyhow!("fournisseur inconnu : {other}")),
                    }
                };
                let (th, co) = splitter.finir();
                emettre(th, co);
                r
            };
            mesure.premier_token_ms = premier;
            mesure.total_ms = debut.elapsed().as_secs_f64() * 1000.0;
            mesure.ok = res.is_ok();
            mesure.essai = essai;
            mesure.repli_texte = demande.texte_seul_en_repli;
            mesure.appels_outils = appels.len() as u32;
            if let Err(e) = &res { mesure.erreur = Some(e.to_string()); }
            self.releve.noter(mesure);
            match res {
                Ok(()) => return Ok(Reponse { texte: full.trim().to_string(), appels }),
                Err(e) => {
                    let rejouable = e.downcast_ref::<ErreurCerveau>().map(|x| x.rejouable()).unwrap_or(false);
                    if essai < 2 && rejouable && premier.is_none() && appels.is_empty() {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Envoie la requête avec les délais d'Orchestra et transforme les erreurs en `ErreurCerveau`.
    async fn envoyer(&self, req: reqwest::RequestBuilder, contrainte: bool) -> anyhow::Result<reqwest::Response> {
        let resp = match tokio::time::timeout(DELAI_PREMIER, req.timeout(DELAI_TOTAL).send()).await {
            Err(_) => return Err(ErreurCerveau::DelaiPremier.into()),
            Ok(Err(e)) if e.is_connect() => return Err(ErreurCerveau::Connexion(e.to_string()).into()),
            Ok(Err(e)) if e.is_timeout() => return Err(ErreurCerveau::DelaiPremier.into()),
            Ok(Err(e)) => return Err(ErreurCerveau::Fournisseur(e.to_string()).into()),
            Ok(Ok(r)) => r,
        };
        let statut = resp.status();
        if statut.is_success() {
            return Ok(resp);
        }
        let corps: String = resp.text().await.unwrap_or_default().chars().take(300).collect();
        if contrainte && statut.as_u16() == 400 && refus_de_contrainte(&corps) {
            return Err(ErreurCerveau::NonSupporte(corps).into());
        }
        Err(ErreurCerveau::Http { statut: statut.as_u16(), corps }.into())
    }

    fn options_ollama(&self) -> Value {
        // num_predict : une réflexion qui tourne en rond ne peut pas dépasser la réserve (§5.2)
        let mut options = json!({ "num_ctx": self.cfg.num_ctx, "num_predict": self.cfg.reserve_reponse });
        // évaluation : réponses reproductibles (température fixée, graine fixe)
        if let Some(t) = self.cfg.eval_temperature {
            options["temperature"] = json!(t);
            options["seed"] = json!(42);
        }
        options
    }

    async fn stream_ollama<F: FnMut(Chunk)>(&self, msgs: &[Msg], demande: Demande<'_>, on: &mut F, col: &mut Collecte<'_>) -> anyhow::Result<()> {
        let base = self.cfg.ollama_url.trim_end_matches('/');
        let mut body = json!({ "model": self.model, "messages": msgs_ollama(msgs), "stream": true, "keep_alive": KEEP_ALIVE, "options": self.options_ollama() });
        if !demande.outils.is_empty() {
            body["tools"] = outils_openai(demande.outils);
        }
        if let Some(s) = demande.schema {
            body["format"] = s.clone();
        }
        let resp = self.envoyer(self.http.post(format!("{base}/api/chat")).json(&body), demande.contrainte()).await?;
        let mut err: Option<String> = None;
        stream_lines(resp, |line| {
            let Ok(v) = serde_json::from_str::<Value>(line) else { return };
            if let Some(e) = v["error"].as_str() { err = Some(e.to_string()); return; }
            if v["done"].as_bool() == Some(true) { col.mesure.lire_ollama(&v); }
            lire_appels_ollama(&v["message"], col.appels);
            on(Chunk {
                thinking: v["message"]["thinking"].as_str().unwrap_or("").to_string(),
                content: v["message"]["content"].as_str().unwrap_or("").to_string(),
            });
        })
        .await?;
        if let Some(e) = err {
            if demande.contrainte() && refus_de_contrainte(&e) {
                return Err(ErreurCerveau::NonSupporte(e).into());
            }
            return Err(ErreurCerveau::Fournisseur(format!("Ollama : {e}")).into());
        }
        Ok(())
    }

    async fn stream_openai<F: FnMut(Chunk)>(&self, base: &str, key: &str, msgs: &[Msg], demande: Demande<'_>, on: &mut F, col: &mut Collecte<'_>) -> anyhow::Result<()> {
        let mut body = json!({ "model": self.model, "messages": msgs_openai(msgs), "stream": true });
        if !key.is_empty() {
            // OpenRouter : usage (tokens) dans le dernier fragment
            body["stream_options"] = json!({ "include_usage": true });
        }
        if let Some(t) = self.cfg.eval_temperature {
            body["temperature"] = json!(t);
            body["seed"] = json!(42);
        }
        if !demande.outils.is_empty() {
            body["tools"] = outils_openai(demande.outils);
        }
        if let Some(s) = demande.schema {
            body["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "sortie", "schema": s } });
        }
        let mut req = self.http.post(format!("{base}/chat/completions")).json(&body);
        if !key.is_empty() {
            req = req.bearer_auth(key).header("HTTP-Referer", "https://github.com/PBdan-dev/Orchestra").header("X-Title", "Orchestra");
        }
        let resp = self.envoyer(req, demande.contrainte()).await?;
        let mut partiels: Vec<(String, String, String)> = vec![];
        stream_lines(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return };
            let data = data.trim();
            if data == "[DONE]" { return; }
            let Ok(v) = serde_json::from_str::<Value>(data) else { return };
            if v["usage"].is_object() { col.mesure.lire_openai(&v["usage"]); }
            let d = &v["choices"][0]["delta"];
            cumuler_appels_openai(d, &mut partiels);
            let thinking = d["reasoning"].as_str().or(d["reasoning_content"].as_str()).unwrap_or("").to_string();
            on(Chunk { thinking, content: d["content"].as_str().unwrap_or("").to_string() });
        })
        .await?;
        col.appels.extend(finir_appels(partiels));
        Ok(())
    }

    async fn stream_claude<F: FnMut(Chunk)>(&self, msgs: &[Msg], demande: Demande<'_>, on: &mut F, col: &mut Collecte<'_>) -> anyhow::Result<()> {
        let (system, mut rest) = msgs_claude(msgs);
        marquer_cache_claude(&mut rest);
        // marqueur de cache en fin de prompt système (blocs 1 à 3, lot A)
        let system = json!([{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }]);
        let mut body = json!({ "model": self.model, "max_tokens": 4096, "stream": true, "system": system, "messages": rest });
        if let Some(t) = self.cfg.eval_temperature {
            body["temperature"] = json!(t);
        }
        if !demande.outils.is_empty() {
            body["tools"] = outils_claude(demande.outils);
        }
        if let Some(s) = demande.schema {
            // sortie contrainte : un outil unique, imposé, dont l'entrée est l'objet voulu
            body["tools"] = json!([{ "name": "sortie", "description": "Donne ta réponse sous la forme de cet objet.", "input_schema": s }]);
            body["tool_choice"] = json!({ "type": "tool", "name": "sortie" });
        }
        let req = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", self.cfg.claude_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&body);
        let resp = self.envoyer(req, demande.contrainte()).await?;
        let mut partiels: Vec<(String, String, String)> = vec![];
        stream_lines(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return };
            let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { return };
            match v["type"].as_str() {
                Some("message_start") => col.mesure.lire_claude(&v["message"]["usage"]),
                Some("message_delta") => col.mesure.lire_claude(&v["usage"]),
                Some("content_block_start") if v["content_block"]["type"] == "tool_use" => {
                    let b = &v["content_block"];
                    partiels.push((b["id"].as_str().unwrap_or("").into(), b["name"].as_str().unwrap_or("").into(), String::new()));
                }
                Some("content_block_delta") => {
                    let d = &v["delta"];
                    match d["type"].as_str() {
                        Some("text_delta") => on(Chunk { thinking: String::new(), content: d["text"].as_str().unwrap_or("").into() }),
                        Some("thinking_delta") => on(Chunk { thinking: d["thinking"].as_str().unwrap_or("").into(), content: String::new() }),
                        Some("input_json_delta") => {
                            if let Some(p) = partiels.last_mut() {
                                p.2.push_str(d["partial_json"].as_str().unwrap_or(""));
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        })
        .await?;
        col.appels.extend(finir_appels(partiels));
        Ok(())
    }

    async fn stream_gemini<F: FnMut(Chunk)>(&self, msgs: &[Msg], demande: Demande<'_>, on: &mut F, col: &mut Collecte<'_>) -> anyhow::Result<()> {
        let (system, contents) = msgs_gemini(msgs);
        let mut body = json!({ "contents": contents });
        let mut generation = json!({});
        if let Some(t) = self.cfg.eval_temperature {
            generation["temperature"] = json!(t);
        }
        if let Some(s) = demande.schema {
            generation["responseMimeType"] = json!("application/json");
            generation["responseSchema"] = s.clone();
        }
        if generation.as_object().is_some_and(|o| !o.is_empty()) {
            body["generationConfig"] = generation;
        }
        if !demande.outils.is_empty() {
            body["tools"] = outils_gemini(demande.outils);
        }
        if !system.is_empty() {
            body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
        }
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.model,
            self.cfg.gemini_api_key.trim()
        );
        let resp = self.envoyer(self.http.post(url).json(&body), demande.contrainte()).await?;
        stream_lines(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return };
            let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { return };
            if v["usageMetadata"].is_object() { col.mesure.lire_gemini(&v["usageMetadata"]); }
            if let Some(parts) = v["candidates"][0]["content"]["parts"].as_array() {
                for p in parts {
                    if let Some(t) = p["text"].as_str() {
                        let is_thought = p["thought"].as_bool().unwrap_or(false);
                        on(if is_thought { Chunk { thinking: t.into(), content: String::new() } } else { Chunk { thinking: String::new(), content: t.into() } });
                    }
                    if let Some(fc) = p.get("functionCall") {
                        let n = col.appels.len() + 1;
                        col.appels.push(AppelOutil {
                            id: format!("appel-{n}"),
                            nom: fc["name"].as_str().unwrap_or("").into(),
                            arguments: fc.get("args").cloned().unwrap_or(json!({})),
                        });
                    }
                }
            }
        })
        .await
    }
}

// ---------- Conversion des messages et des outils, par fournisseur ----------

/// Le fournisseur refuse-t-il les outils ou la sortie contrainte ? (message d'erreur 400)
pub fn refus_de_contrainte(corps: &str) -> bool {
    let c = corps.to_lowercase();
    (c.contains("support") || c.contains("unsupported") || c.contains("not available"))
        && (c.contains("tool") || c.contains("format") || c.contains("schema") || c.contains("function"))
}

pub fn msgs_ollama(msgs: &[Msg]) -> Vec<Value> {
    msgs.iter()
        .map(|m| match m.role.as_str() {
            "tool" => json!({ "role": "tool", "content": m.content, "tool_name": m.outil_nom.clone().unwrap_or_default() }),
            "assistant" if !m.appels.is_empty() => json!({
                "role": "assistant",
                "content": m.content,
                "tool_calls": m.appels.iter().map(|a| json!({ "function": { "name": a.nom, "arguments": a.arguments } })).collect::<Vec<_>>()
            }),
            _ => json!({ "role": m.role, "content": m.content }),
        })
        .collect()
}

pub fn msgs_openai(msgs: &[Msg]) -> Vec<Value> {
    msgs.iter()
        .map(|m| match m.role.as_str() {
            "tool" => json!({ "role": "tool", "tool_call_id": m.outil_id.clone().unwrap_or_default(), "content": m.content }),
            "assistant" if !m.appels.is_empty() => json!({
                "role": "assistant",
                "content": if m.content.is_empty() { Value::Null } else { json!(m.content) },
                "tool_calls": m.appels.iter().map(|a| json!({
                    "id": a.id, "type": "function", "function": { "name": a.nom, "arguments": a.arguments.to_string() }
                })).collect::<Vec<_>>()
            }),
            _ => json!({ "role": m.role, "content": m.content }),
        })
        .collect()
}

/// Claude : prompt système à part, résultats d'outils regroupés dans un message `user`.
pub fn msgs_claude(msgs: &[Msg]) -> (String, Vec<Value>) {
    let system = msgs.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n\n");
    let mut out: Vec<Value> = vec![];
    for m in msgs.iter().filter(|m| m.role != "system") {
        match m.role.as_str() {
            "tool" => {
                let bloc = json!({ "type": "tool_result", "tool_use_id": m.outil_id.clone().unwrap_or_default(), "content": m.content });
                let regroupe = out.last_mut().filter(|d| d["role"] == "user" && d["content"].as_array().is_some_and(|a| a.iter().all(|b| b["type"] == "tool_result")));
                match regroupe {
                    Some(d) => d["content"].as_array_mut().unwrap().push(bloc),
                    None => out.push(json!({ "role": "user", "content": [bloc] })),
                }
            }
            "assistant" if !m.appels.is_empty() => {
                let mut blocs = vec![];
                if !m.content.is_empty() {
                    blocs.push(json!({ "type": "text", "text": m.content }));
                }
                for a in &m.appels {
                    blocs.push(json!({ "type": "tool_use", "id": a.id, "name": a.nom, "input": a.arguments }));
                }
                out.push(json!({ "role": "assistant", "content": blocs }));
            }
            _ => out.push(json!({ "role": m.role, "content": m.content })),
        }
    }
    (system, out)
}

/// Gemini : rôles user/model, appels en `functionCall`, résultats en `functionResponse`.
pub fn msgs_gemini(msgs: &[Msg]) -> (String, Vec<Value>) {
    let system = msgs.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n\n");
    let mut out: Vec<Value> = vec![];
    for m in msgs.iter().filter(|m| m.role != "system") {
        match m.role.as_str() {
            "tool" => {
                let part = json!({ "functionResponse": { "name": m.outil_nom.clone().unwrap_or_default(), "response": { "resultat": m.content } } });
                let regroupe = out.last_mut().filter(|d| d["role"] == "user" && d["parts"].as_array().is_some_and(|a| a.iter().all(|p| p.get("functionResponse").is_some())));
                match regroupe {
                    Some(d) => d["parts"].as_array_mut().unwrap().push(part),
                    None => out.push(json!({ "role": "user", "parts": [part] })),
                }
            }
            "assistant" => {
                let mut parts = vec![];
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                for a in &m.appels {
                    parts.push(json!({ "functionCall": { "name": a.nom, "args": a.arguments } }));
                }
                out.push(json!({ "role": "model", "parts": parts }));
            }
            _ => out.push(json!({ "role": "user", "parts": [{ "text": m.content }] })),
        }
    }
    (system, out)
}

/// Marqueur de cache Claude sur le dernier message de l'historique (avant le message du tour).
pub fn marquer_cache_claude(messages: &mut [Value]) {
    let n = messages.len();
    if n < 2 {
        return;
    }
    let m = &mut messages[n - 2];
    if let Some(t) = m["content"].as_str().map(|t| t.to_string()) {
        m["content"] = json!([{ "type": "text", "text": t }]);
    }
    if let Some(blocs) = m["content"].as_array_mut() {
        if let Some(dernier) = blocs.last_mut() {
            dernier["cache_control"] = json!({ "type": "ephemeral" });
        }
    }
}

pub fn outils_openai(outils: &[Outil]) -> Value {
    json!(outils.iter().map(|o| json!({ "type": "function", "function": { "name": o.nom, "description": o.description, "parameters": o.parametres } })).collect::<Vec<_>>())
}

pub fn outils_claude(outils: &[Outil]) -> Value {
    json!(outils.iter().map(|o| json!({ "name": o.nom, "description": o.description, "input_schema": o.parametres })).collect::<Vec<_>>())
}

pub fn outils_gemini(outils: &[Outil]) -> Value {
    let decl: Vec<Value> = outils
        .iter()
        .map(|o| {
            // Gemini refuse un objet sans propriétés : on omet alors les paramètres
            let vide = o.parametres["properties"].as_object().map(|p| p.is_empty()).unwrap_or(true);
            if vide { json!({ "name": o.nom, "description": o.description }) } else { json!({ "name": o.nom, "description": o.description, "parameters": o.parametres }) }
        })
        .collect();
    json!([{ "functionDeclarations": decl }])
}

/// Ollama : `message.tool_calls` (arguments déjà en objet).
pub fn lire_appels_ollama(message: &Value, appels: &mut Vec<AppelOutil>) {
    if let Some(tc) = message["tool_calls"].as_array() {
        for c in tc {
            let n = appels.len() + 1;
            let f = &c["function"];
            let arguments = match &f["arguments"] {
                Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
                Value::Null => json!({}),
                v => v.clone(),
            };
            appels.push(AppelOutil {
                id: c["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("appel-{n}")),
                nom: f["name"].as_str().unwrap_or("").to_string(),
                arguments,
            });
        }
    }
}

/// API OpenAI en flux : chaque appel arrive en morceaux, repérés par `index`.
pub fn cumuler_appels_openai(delta: &Value, partiels: &mut Vec<(String, String, String)>) {
    let Some(tc) = delta["tool_calls"].as_array() else { return };
    for c in tc {
        let i = c["index"].as_u64().unwrap_or(partiels.len().saturating_sub(1) as u64) as usize;
        while partiels.len() <= i {
            partiels.push((String::new(), String::new(), String::new()));
        }
        let p = &mut partiels[i];
        if let Some(id) = c["id"].as_str() { p.0 = id.into(); }
        if let Some(n) = c["function"]["name"].as_str() { p.1.push_str(n); }
        if let Some(a) = c["function"]["arguments"].as_str() { p.2.push_str(a); }
    }
}

/// Assemble les appels reçus en morceaux (id, nom, arguments en texte JSON).
pub fn finir_appels(partiels: Vec<(String, String, String)>) -> Vec<AppelOutil> {
    partiels
        .into_iter()
        .enumerate()
        .filter(|(_, p)| !p.1.is_empty())
        .map(|(i, (id, nom, args))| AppelOutil {
            id: if id.is_empty() { format!("appel-{}", i + 1) } else { id },
            nom,
            arguments: if args.trim().is_empty() { json!({}) } else { serde_json::from_str(&args).unwrap_or(json!({ "_brut": args })) },
        })
        .collect()
}

/// Repli seulement (modèle sans sortie contrainte) : premier objet JSON équilibré d'un texte.
pub fn lire_json_dans_texte(s: &str) -> Option<Value> {
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in s[start..].char_indices() {
        if in_str {
            if esc { esc = false; } else if c == '\\' { esc = true; } else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&s[start..start + i + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

fn short_err(e: &anyhow::Error) -> String {
    let s = e.to_string();
    if s.contains("timed out") || s.contains("timeout") {
        "délai dépassé".into()
    } else if s.contains("connect") {
        "connexion refusée".into()
    } else {
        s.chars().take(120).collect()
    }
}

fn pick_default_ollama(models: &[String]) -> Option<String> {
    // évite les modèles d'embeddings, préfère le plus petit "instruct"
    let usable: Vec<&String> = models.iter().filter(|m| !m.contains("embed") && !m.contains("bge") && !m.contains("minilm")).collect();
    usable.first().map(|s| s.to_string())
}

pub fn provider_label(p: &str) -> &'static str {
    match p {
        "ollama" => "Ollama",
        "lmstudio" => "LM Studio",
        "claude" => "Claude",
        "gemini" => "Gemini",
        "openrouter" => "OpenRouter",
        _ => "?",
    }
}

/// Lit une réponse HTTP ligne par ligne (NDJSON ou SSE), avec un délai pour le premier
/// morceau et un délai de silence ensuite : jamais d'attente sans fin.
async fn stream_lines<F: FnMut(&str)>(resp: reqwest::Response, mut f: F) -> anyhow::Result<()> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut attente = DELAI_PREMIER;
    let mut recu = false;
    loop {
        let chunk = match tokio::time::timeout(attente, stream.next()).await {
            Err(_) => return Err(if recu { ErreurCerveau::Silence } else { ErreurCerveau::DelaiPremier }.into()),
            Ok(None) => break,
            Ok(Some(Err(e))) if e.is_timeout() => {
                return Err(ErreurCerveau::Fournisseur(format!("réponse trop longue : délai total de {} s dépassé", DELAI_TOTAL.as_secs())).into())
            }
            Ok(Some(Err(e))) => return Err(ErreurCerveau::Fournisseur(format!("flux interrompu : {e}")).into()),
            Ok(Some(Ok(c))) => c,
        };
        recu = true;
        attente = DELAI_SILENCE;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let s = String::from_utf8_lossy(&line);
            let s = s.trim_end();
            if !s.is_empty() {
                f(s);
            }
        }
    }
    if !buf.is_empty() {
        let s = String::from_utf8_lossy(&buf);
        let s = s.trim();
        if !s.is_empty() {
            f(s);
        }
    }
    Ok(())
}

/// Sépare les balises <think>…</think> présentes dans le flux de contenu.
#[derive(Default)]
struct ThinkSplitter {
    in_think: bool,
    pending: String,
}

impl ThinkSplitter {
    fn feed(&mut self, s: &str) -> (String, String) {
        let mut thinking = String::new();
        let mut content = String::new();
        self.pending.push_str(s);
        loop {
            if self.in_think {
                if let Some(i) = self.pending.find("</think>") {
                    thinking.push_str(&self.pending[..i]);
                    self.pending = self.pending[i + 8..].to_string();
                    self.in_think = false;
                } else {
                    // garde une queue au cas où la balise de fermeture soit coupée
                    let keep = self.pending.len().min(8);
                    let cut = self.pending.len() - keep;
                    let cut = floor_char(&self.pending, cut);
                    thinking.push_str(&self.pending[..cut]);
                    self.pending = self.pending[cut..].to_string();
                    break;
                }
            } else if let Some(i) = self.pending.find("<think>") {
                content.push_str(&self.pending[..i]);
                self.pending = self.pending[i + 7..].to_string();
                self.in_think = true;
            } else {
                let keep = if self.pending.ends_with('<') || self.pending.contains("<t") || self.pending.contains("<th") || self.pending.ends_with("<thi") || self.pending.ends_with("<thin") || self.pending.ends_with("<think") {
                    self.pending.rfind('<').map(|i| self.pending.len() - i).unwrap_or(0)
                } else {
                    0
                };
                let cut = floor_char(&self.pending, self.pending.len() - keep);
                content.push_str(&self.pending[..cut]);
                self.pending = self.pending[cut..].to_string();
                break;
            }
        }
        (thinking, content)
    }

    /// Fin du flux : rend ce qui restait en attente.
    fn finir(&mut self) -> (String, String) {
        let reste = std::mem::take(&mut self.pending);
        if self.in_think { (reste, String::new()) } else { (String::new(), reste) }
    }
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appel() -> AppelOutil {
        AppelOutil { id: "c1".into(), nom: "write_file".into(), arguments: json!({ "path": "a.md", "content": "x" }) }
    }

    fn conversation() -> Vec<Msg> {
        vec![
            Msg::system("sys"),
            Msg::user("fais-le"),
            Msg::appels_outils("", vec![appel(), AppelOutil { id: "c2".into(), nom: "now".into(), arguments: json!({}) }]),
            Msg::resultat_outil(&appel(), "écrit"),
            Msg::resultat_outil(&AppelOutil { id: "c2".into(), nom: "now".into(), arguments: json!({}) }, "lundi"),
        ]
    }

    #[test]
    fn ollama_messages_et_appels() {
        let m = msgs_ollama(&conversation());
        assert_eq!(m[2]["tool_calls"][0]["function"]["name"], "write_file");
        assert_eq!(m[2]["tool_calls"][0]["function"]["arguments"]["path"], "a.md");
        assert_eq!(m[3]["role"], "tool");
        assert_eq!(m[3]["tool_name"], "write_file");
        let mut appels = vec![];
        lire_appels_ollama(&json!({ "tool_calls": [{ "function": { "name": "now", "arguments": {} } }] }), &mut appels);
        assert_eq!(appels, vec![AppelOutil { id: "appel-1".into(), nom: "now".into(), arguments: json!({}) }]);
    }

    #[test]
    fn openai_messages_et_appels_en_morceaux() {
        let m = msgs_openai(&conversation());
        assert_eq!(m[2]["content"], Value::Null);
        assert_eq!(m[2]["tool_calls"][0]["function"]["arguments"], json!({ "path": "a.md", "content": "x" }).to_string());
        assert_eq!(m[3]["tool_call_id"], "c1");
        let mut p = vec![];
        cumuler_appels_openai(&json!({ "tool_calls": [{ "index": 0, "id": "x1", "function": { "name": "read_file", "arguments": "{\"pa" } }] }), &mut p);
        cumuler_appels_openai(&json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "th\": \"n.md\"}" } }] }), &mut p);
        let a = finir_appels(p);
        assert_eq!(a, vec![AppelOutil { id: "x1".into(), nom: "read_file".into(), arguments: json!({ "path": "n.md" }) }]);
    }

    #[test]
    fn claude_resultats_regroupes() {
        let (system, m) = msgs_claude(&conversation());
        assert_eq!(system, "sys");
        assert_eq!(m.len(), 3);
        assert_eq!(m[1]["content"][0]["type"], "tool_use");
        assert_eq!(m[1]["content"][1]["id"], "c2");
        assert_eq!(m[2]["role"], "user");
        assert_eq!(m[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(m[2]["content"][1]["tool_use_id"], "c2");
    }

    #[test]
    fn gemini_appels_et_reponses() {
        let (_, m) = msgs_gemini(&conversation());
        assert_eq!(m[1]["role"], "model");
        assert_eq!(m[1]["parts"][0]["functionCall"]["name"], "write_file");
        assert_eq!(m[2]["parts"].as_array().unwrap().len(), 2);
        assert_eq!(m[2]["parts"][1]["functionResponse"]["response"]["resultat"], "lundi");
        let o = outils_gemini(&[Outil { nom: "now".into(), description: "heure".into(), parametres: json!({ "type": "object", "properties": {} }) }]);
        assert!(o[0]["functionDeclarations"][0].get("parameters").is_none());
    }

    #[test]
    fn refus_et_nouvel_essai() {
        assert!(refus_de_contrainte("registry.ollama.ai/library/gemma:2b does not support tools"));
        assert!(!refus_de_contrainte("model not found"));
        assert!(ErreurCerveau::Connexion("x".into()).rejouable());
        assert!(ErreurCerveau::DelaiPremier.rejouable());
        assert!(ErreurCerveau::Http { statut: 503, corps: String::new() }.rejouable());
        assert!(!ErreurCerveau::Http { statut: 400, corps: String::new() }.rejouable());
        assert!(!ErreurCerveau::Silence.rejouable());
        let e: anyhow::Error = ErreurCerveau::NonSupporte("x".into()).into();
        assert!(non_supporte(&e));
    }

    #[test]
    fn cache_claude_sur_le_dernier_message_d_historique() {
        let (_, mut m) = msgs_claude(&[Msg::system("s"), Msg::user("a"), Msg::assistant("b"), Msg::user("c")]);
        marquer_cache_claude(&mut m);
        assert_eq!(m[1]["content"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(m[1]["content"][0]["text"], "b");
        assert_eq!(m[2]["content"], "c");
        assert_eq!(modele_vocal_derive("qwen3.5:9b"), "qwen3.5:9b-orchestra");
        assert_eq!(modele_vocal_derive("qwen3.5:9b-orchestra"), "qwen3.5:9b-orchestra");
        assert_eq!(modele_vocal_derive("mistral"), "mistral:latest-orchestra");
    }

    #[test]
    fn json_dans_texte_repli() {
        assert_eq!(lire_json_dans_texte("Voici : {\"a\": {\"b\": \"}\"}} fin"), Some(json!({ "a": { "b": "}" } })));
        assert_eq!(lire_json_dans_texte("rien"), None);
    }

    #[test]
    fn balises_think_et_fin_de_flux() {
        let mut s = ThinkSplitter::default();
        let (t, c) = s.feed("<think>je pense</think>Bonjour <");
        assert_eq!(t, "je pense");
        assert_eq!(c, "Bonjour ");
        let (t, c) = s.finir();
        assert_eq!((t.as_str(), c.as_str()), ("", "<"));
    }
}
