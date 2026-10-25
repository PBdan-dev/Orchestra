//! Le cerveau : un fournisseur de modèle greffé à Orchestra.
//! Ollama (défaut, local), LM Studio (local, API OpenAI), Claude, Gemini, OpenRouter.

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Config;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

impl Msg {
    pub fn system(s: impl Into<String>) -> Self { Msg { role: "system".into(), content: s.into() } }
    pub fn user(s: impl Into<String>) -> Self { Msg { role: "user".into(), content: s.into() } }
    pub fn assistant(s: impl Into<String>) -> Self { Msg { role: "assistant".into(), content: s.into() } }
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
}

pub struct Brain {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub model: String,
}

impl Brain {
    pub fn new(cfg: Config, http: reqwest::Client) -> Self {
        Brain { model: cfg.model.clone(), cfg, http }
    }

    pub fn label(&self) -> String {
        format!("{} · {}", provider_label(&self.cfg.provider), self.model)
    }

    /// Résout le modèle (détection automatique si vide) et vérifie la connexion.
    pub async fn detect(&mut self) -> BrainInfo {
        let mut info = BrainInfo { provider: self.cfg.provider.clone(), ..Default::default() };
        match self.cfg.provider.as_str() {
            "ollama" => {
                let base = self.cfg.ollama_url.trim_end_matches('/');
                let tags = self.get_json(&format!("{base}/api/tags")).await;
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
                                info.detail = "modèle actif détecté".into();
                            } else if let Some(first) = pick_default_ollama(&info.models) {
                                self.model = first;
                                info.detail = "aucun modèle chargé : premier modèle installé".into();
                            } else {
                                info.detail = "Ollama tourne mais aucun modèle n'est installé (ollama pull …)".into();
                            }
                        } else {
                            info.detail = if running.iter().any(|r| r == &self.cfg.model) { "modèle choisi, déjà chargé".into() } else { "modèle choisi".into() };
                        }
                    }
                    Err(e) => info.detail = format!("Ollama injoignable sur {base} : {e}"),
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
        info
    }

    async fn get_json(&self, url: &str) -> anyhow::Result<Value> {
        let r = self.http.get(url).timeout(Duration::from_secs(4)).send().await?;
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

    /// Appel en streaming. `on` reçoit chaque fragment ; renvoie le contenu complet.
    pub async fn stream<F: FnMut(Chunk)>(&self, msgs: &[Msg], mut on: F) -> anyhow::Result<String> {
        let mut full = String::new();
        let mut splitter = ThinkSplitter::default();
        let mut deliver = |raw: Chunk| {
            // sépare les <think>…</think> éventuellement inclus dans le contenu
            let (th, co) = splitter.feed(&raw.content);
            let c = Chunk { thinking: format!("{}{}", raw.thinking, th), content: co };
            full.push_str(&c.content);
            if !c.thinking.is_empty() || !c.content.is_empty() {
                on(c);
            }
        };
        match self.cfg.provider.as_str() {
            "ollama" => self.stream_ollama(msgs, &mut deliver).await?,
            "lmstudio" => {
                let base = format!("{}/v1", self.cfg.lmstudio_url.trim_end_matches('/'));
                self.stream_openai(&base, "", msgs, &mut deliver).await?
            }
            "openrouter" => {
                self.stream_openai("https://openrouter.ai/api/v1", &self.cfg.openrouter_api_key, msgs, &mut deliver).await?
            }
            "claude" => self.stream_claude(msgs, &mut deliver).await?,
            "gemini" => self.stream_gemini(msgs, &mut deliver).await?,
            other => anyhow::bail!("fournisseur inconnu : {other}"),
        }
        Ok(full.trim().to_string())
    }

    pub async fn complete(&self, msgs: &[Msg]) -> anyhow::Result<String> {
        self.stream(msgs, |_| {}).await
    }

    async fn stream_ollama<F: FnMut(Chunk)>(&self, msgs: &[Msg], on: &mut F) -> anyhow::Result<()> {
        let base = self.cfg.ollama_url.trim_end_matches('/');
        let body = json!({ "model": self.model, "messages": msgs, "stream": true, "options": { "num_ctx": 16384 } });
        let resp = self.http.post(format!("{base}/api/chat")).timeout(Duration::from_secs(600)).json(&body).send().await?;
        check(resp.status(), "Ollama", &resp).await?;
        let mut err: Option<String> = None;
        stream_lines(resp, |line| {
            let Ok(v) = serde_json::from_str::<Value>(line) else { return };
            if let Some(e) = v["error"].as_str() { err = Some(e.to_string()); return; }
            let c = Chunk {
                thinking: v["message"]["thinking"].as_str().unwrap_or("").to_string(),
                content: v["message"]["content"].as_str().unwrap_or("").to_string(),
            };
            on(c);
        })
        .await?;
        if let Some(e) = err { anyhow::bail!("Ollama : {e}"); }
        Ok(())
    }

    async fn stream_openai<F: FnMut(Chunk)>(&self, base: &str, key: &str, msgs: &[Msg], on: &mut F) -> anyhow::Result<()> {
        let body = json!({ "model": self.model, "messages": msgs, "stream": true });
        let mut req = self.http.post(format!("{base}/chat/completions")).timeout(Duration::from_secs(600)).json(&body);
        if !key.is_empty() {
            req = req.bearer_auth(key).header("HTTP-Referer", "https://github.com/PBdan-dev/Orchestra").header("X-Title", "Orchestra");
        }
        let resp = req.send().await?;
        check(resp.status(), if key.is_empty() { "LM Studio" } else { "OpenRouter" }, &resp).await?;
        stream_lines(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return };
            let data = data.trim();
            if data == "[DONE]" { return; }
            let Ok(v) = serde_json::from_str::<Value>(data) else { return };
            let d = &v["choices"][0]["delta"];
            let thinking = d["reasoning"].as_str().or(d["reasoning_content"].as_str()).unwrap_or("").to_string();
            on(Chunk { thinking, content: d["content"].as_str().unwrap_or("").to_string() });
        })
        .await
    }

    async fn stream_claude<F: FnMut(Chunk)>(&self, msgs: &[Msg], on: &mut F) -> anyhow::Result<()> {
        let system: Vec<&str> = msgs.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect();
        let rest: Vec<Value> = msgs.iter().filter(|m| m.role != "system").map(|m| json!({ "role": m.role, "content": m.content })).collect();
        let body = json!({ "model": self.model, "max_tokens": 4096, "stream": true, "system": system.join("\n\n"), "messages": rest });
        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .timeout(Duration::from_secs(600))
            .header("x-api-key", self.cfg.claude_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        check(resp.status(), "Claude", &resp).await?;
        stream_lines(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return };
            let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { return };
            if v["type"] == "content_block_delta" {
                let d = &v["delta"];
                match d["type"].as_str() {
                    Some("text_delta") => on(Chunk { thinking: String::new(), content: d["text"].as_str().unwrap_or("").into() }),
                    Some("thinking_delta") => on(Chunk { thinking: d["thinking"].as_str().unwrap_or("").into(), content: String::new() }),
                    _ => {}
                }
            }
        })
        .await
    }

    async fn stream_gemini<F: FnMut(Chunk)>(&self, msgs: &[Msg], on: &mut F) -> anyhow::Result<()> {
        let system: Vec<&str> = msgs.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect();
        let contents: Vec<Value> = msgs
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| json!({ "role": if m.role == "assistant" { "model" } else { "user" }, "parts": [{ "text": m.content }] }))
            .collect();
        let mut body = json!({ "contents": contents });
        if !system.is_empty() {
            body["systemInstruction"] = json!({ "parts": [{ "text": system.join("\n\n") }] });
        }
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.model,
            self.cfg.gemini_api_key.trim()
        );
        let resp = self.http.post(url).timeout(Duration::from_secs(600)).json(&body).send().await?;
        check(resp.status(), "Gemini", &resp).await?;
        stream_lines(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return };
            let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { return };
            if let Some(parts) = v["candidates"][0]["content"]["parts"].as_array() {
                for p in parts {
                    if let Some(t) = p["text"].as_str() {
                        let is_thought = p["thought"].as_bool().unwrap_or(false);
                        on(if is_thought { Chunk { thinking: t.into(), content: String::new() } } else { Chunk { thinking: String::new(), content: t.into() } });
                    }
                }
            }
        })
        .await
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

async fn check(status: reqwest::StatusCode, who: &str, _resp: &reqwest::Response) -> anyhow::Result<()> {
    if status.is_success() {
        Ok(())
    } else {
        anyhow::bail!("{who} a répondu {status}")
    }
}

/// Lit une réponse HTTP ligne par ligne (NDJSON ou SSE).
async fn stream_lines<F: FnMut(&str)>(resp: reqwest::Response, mut f: F) -> anyhow::Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {status} : {}", body.chars().take(300).collect::<String>());
    }
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
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
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
