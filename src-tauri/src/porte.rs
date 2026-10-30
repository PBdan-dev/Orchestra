//! Porte du modèle (lot T2) : point d'accès local, propre à une mission, par lequel le modèle d'un
//! agent passe. Compatible Ollama (`/api/chat`, `/api/embed`) et OpenAI (`/v1/chat/completions`).
//!
//! - Jeton par mission (`ORCHESTRA_JETON`) ; la porte ferme à la fin de la mission : un jeton réutilisé
//!   ne mène plus nulle part.
//! - Chaque échange est écrit dans `echanges.jsonl` (requête, réflexion, réponse, appels d'outils,
//!   tokens, durées) et la réflexion dans `pensee.md`.
//! - Contrôle des outils (D26, D27) : outil non déclaré ou non exposé au modèle, action sans `raison`.
//! - Limites (D29) : pas, tokens, durée, boucle ; décision de PB du 30/10 : avertissement, et arrêt
//!   propre au plafond d'1 h seulement (les boutons Pause et Reprendre viennent au lot T7).
//! - Carte graphique (D24) : `modele.rs` choisit, charge et décharge le modèle de la mission.
//!
//! Serveur HTTP minimal sur tokio (une requête par connexion, `Connection: close`), sans dépendance.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::{
    agents::Manifest,
    brain::{self, Brain, Demande, Msg, Outil},
    config::Config,
};

/// Ce que la porte signale à la mission en cours (affiché dans Conscience, compté pour T5).
#[derive(Debug, Clone)]
pub enum Note {
    /// Réflexion ou réponse du modèle de l'agent.
    Pensee { texte: String },
    /// Appel d'outil décidé par le modèle de l'agent, vu par la porte (action « observée », D14).
    Appel { nom: String, arguments: Value },
    /// Écart au contrat vu par la porte (outil non déclaré, action sans raison…).
    Ecart { texte: String },
    /// Seuil de limite atteint (D29) : `arreter` = plafond d'1 h, arrêt propre demandé.
    Limite { texte: String, arreter: bool },
    /// Requête refusée (jeton, modèle non déclaré…).
    Refus { texte: String },
}

/// Seuils de la mission (D29) : 3 fois les mesures de certification, 10 min sans progrès, 1 h.
#[derive(Debug, Clone)]
pub struct Limites {
    pub pas_max: Option<u64>,
    pub tokens_max: Option<u64>,
    pub duree_max_s: Option<u64>,
    pub sans_progres: Duration,
    pub plafond: Duration,
}

impl Limites {
    pub fn depuis(m: &Manifest, type_tache: Option<&str>) -> Self {
        let mesures = type_tache.and_then(|t| m.brut.pointer(&format!("/certification/mesures/{t}")).cloned()).unwrap_or(Value::Null);
        let fois3 = |k: &str| mesures[k].as_u64().filter(|v| *v > 0).map(|v| v * 3);
        Limites {
            pas_max: fois3("pas_max"),
            tokens_max: fois3("tokens_max"),
            duree_max_s: fois3("duree_max_s"),
            sans_progres: Duration::from_secs(600),
            plafond: Duration::from_secs(3600),
        }
    }
}

/// Comptage d'une mission et alertes déjà émises (une seule fois chacune).
#[derive(Debug, Default)]
pub struct Compteurs {
    pub pas: u64,
    pub tokens: u64,
    pub dernieres: Vec<u64>,
    pub alertes: Vec<String>,
}

/// Configuration de la porte pour une mission.
pub struct Config_ {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub agent: Manifest,
    pub modele: String,
    pub dossier: PathBuf,
    pub limites: Limites,
    /// Banc d'essai seulement : réponse fixe au lieu du vrai modèle (`reponse_porte_test` du kit).
    pub reponse_fixe: Option<String>,
}

struct Etat {
    c: Config_,
    jeton: String,
    debut: Instant,
    dernier_progres: Mutex<Instant>,
    compteurs: Mutex<Compteurs>,
    notes: mpsc::UnboundedSender<Note>,
    /// Un appel au modèle à la fois (mode série, D7).
    file: tokio::sync::Mutex<()>,
    n: Mutex<u64>,
}

/// Porte ouverte pour une mission ; elle se ferme quand on la lâche.
pub struct Porte {
    pub url: String,
    pub jeton: String,
    etat: Arc<Etat>,
    tache: tokio::task::JoinHandle<()>,
}

impl Drop for Porte {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

impl Porte {
    /// Ouvre la porte sur 127.0.0.1 (port libre) et renvoie les notes de la mission.
    pub async fn ouvrir(c: Config_) -> anyhow::Result<(Porte, mpsc::UnboundedReceiver<Note>)> {
        let ecoute = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", ecoute.local_addr()?);
        let jeton = uuid::Uuid::new_v4().simple().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let etat = Arc::new(Etat {
            c,
            jeton: jeton.clone(),
            debut: Instant::now(),
            dernier_progres: Mutex::new(Instant::now()),
            compteurs: Mutex::new(Compteurs::default()),
            notes: tx,
            file: tokio::sync::Mutex::new(()),
            n: Mutex::new(0),
        });
        let e2 = etat.clone();
        let tache = tokio::spawn(async move {
            while let Ok((flux, _)) = ecoute.accept().await {
                let e = e2.clone();
                tokio::spawn(async move {
                    let _ = servir(flux, e).await;
                });
            }
        });
        Ok((Porte { url, jeton, etat, tache }, rx))
    }

    /// Variables d'environnement données à l'agent.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![("ORCHESTRA_MODELE_URL".into(), self.url.clone()), ("ORCHESTRA_JETON".into(), self.jeton.clone())]
    }

    /// L'agent a émis un événement : c'est un progrès (pour le seuil « sans progrès »).
    pub fn progres(&self) {
        *self.etat.dernier_progres.lock().unwrap() = Instant::now();
    }

    /// Vérifie les seuils de durée (appelée régulièrement par la mission).
    pub fn verifier_durees(&self) {
        verifier_limites(&self.etat);
    }

    pub fn compteurs(&self) -> (u64, u64) {
        let c = self.etat.compteurs.lock().unwrap();
        (c.pas, c.tokens)
    }
}

// ---------- HTTP minimal ----------

struct Requete {
    methode: String,
    chemin: String,
    entetes: HashMap<String, String>,
    corps: Vec<u8>,
}

async fn lire_requete(flux: &mut BufReader<TcpStream>) -> anyhow::Result<Requete> {
    let mut ligne = String::new();
    flux.read_line(&mut ligne).await?;
    let mut parts = ligne.split_whitespace();
    let methode = parts.next().unwrap_or("").to_string();
    let chemin = parts.next().unwrap_or("").to_string();
    let mut entetes = HashMap::new();
    loop {
        let mut l = String::new();
        if flux.read_line(&mut l).await? == 0 {
            break;
        }
        let l = l.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            entetes.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let n: usize = entetes.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if n > 32 * 1024 * 1024 {
        anyhow::bail!("requête trop grosse");
    }
    let mut corps = vec![0u8; n];
    flux.read_exact(&mut corps).await?;
    Ok(Requete { methode, chemin, entetes, corps })
}

async fn repondre(flux: &mut TcpStream, statut: u16, type_: &str, corps: &[u8]) -> std::io::Result<()> {
    let raison = match statut {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Service Unavailable",
    };
    let tete = format!("HTTP/1.1 {statut} {raison}\r\nContent-Type: {type_}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", corps.len());
    flux.write_all(tete.as_bytes()).await?;
    flux.write_all(corps).await?;
    flux.flush().await
}

async fn erreur(flux: &mut TcpStream, statut: u16, message: &str) -> std::io::Result<()> {
    repondre(flux, statut, "application/json", json!({ "error": message }).to_string().as_bytes()).await
}

async fn servir(flux: TcpStream, e: Arc<Etat>) -> anyhow::Result<()> {
    let mut lecteur = BufReader::new(flux);
    let r = lire_requete(&mut lecteur).await?;
    let mut flux = lecteur.into_inner();
    let jeton_ok = r.entetes.get("authorization").map(|a| a.trim_start_matches("Bearer ").trim()) == Some(e.jeton.as_str());
    if !jeton_ok {
        let _ = e.notes.send(Note::Refus { texte: format!("requête sans le jeton de la mission ({} {})", r.methode, r.chemin) });
        return Ok(erreur(&mut flux, 401, "jeton de mission invalide").await?);
    }
    if r.methode != "POST" {
        return Ok(erreur(&mut flux, 404, "seules les requêtes POST sont servies").await?);
    }
    let Ok(corps) = serde_json::from_slice::<Value>(&r.corps) else {
        return Ok(erreur(&mut flux, 400, "corps JSON illisible").await?);
    };
    match r.chemin.as_str() {
        "/api/chat" => {
            let flux_demande = corps["stream"].as_bool().unwrap_or(true);
            match chat(&e, corps, "/api/chat").await {
                Ok(rep) => {
                    if flux_demande {
                        // un seul fragment final, au format NDJSON d'Ollama
                        let mut l = rep.to_string();
                        l.push('\n');
                        repondre(&mut flux, 200, "application/x-ndjson", l.as_bytes()).await?;
                    } else {
                        repondre(&mut flux, 200, "application/json", rep.to_string().as_bytes()).await?;
                    }
                }
                Err((s, m)) => erreur(&mut flux, s, &m).await?,
            }
        }
        "/v1/chat/completions" => {
            let flux_demande = corps["stream"].as_bool().unwrap_or(false);
            let ollama = openai_vers_ollama(&corps);
            match chat(&e, ollama, "/v1/chat/completions").await {
                Ok(rep) => {
                    let o = ollama_vers_openai(&rep);
                    if flux_demande {
                        let mut morceau = o.clone();
                        morceau["object"] = json!("chat.completion.chunk");
                        morceau["choices"][0]["delta"] = morceau["choices"][0]["message"].take();
                        let corps = format!("data: {morceau}\n\ndata: [DONE]\n\n");
                        repondre(&mut flux, 200, "text/event-stream", corps.as_bytes()).await?;
                    } else {
                        repondre(&mut flux, 200, "application/json", o.to_string().as_bytes()).await?;
                    }
                }
                Err((s, m)) => erreur(&mut flux, s, &m).await?,
            }
        }
        "/api/embed" => match embed(&e, corps).await {
            Ok(v) => repondre(&mut flux, 200, "application/json", v.to_string().as_bytes()).await?,
            Err((s, m)) => erreur(&mut flux, s, &m).await?,
        },
        autre => erreur(&mut flux, 404, &format!("chemin inconnu : {autre}")).await?,
    }
    Ok(())
}

// ---------- Contrôles ----------

/// Fonctions que le modèle de l'agent a le droit d'appeler (D26) : déclarées et exposées au modèle.
fn fonctions_exposees(m: &Manifest) -> Vec<String> {
    m.brut
        .pointer("/fiche/fonctions")
        .and_then(|f| f.as_array())
        .map(|a| a.iter().filter(|f| f["expose_au_modele"].as_bool() == Some(true)).filter_map(|f| f["nom"].as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// Écarts d'une requête (outils proposés) et d'une réponse (outils appelés).
pub fn controler_outils(m: &Manifest, requete: &Value, reponse: &Value) -> Vec<String> {
    let permis = fonctions_exposees(m);
    let mut e = vec![];
    for t in requete["tools"].as_array().into_iter().flatten() {
        let nom = t.pointer("/function/name").or(t.get("name")).and_then(|n| n.as_str()).unwrap_or("");
        if !permis.iter().any(|p| p == nom) {
            e.push(format!("outil proposé au modèle sans être déclaré et exposé dans la fiche : « {nom} »"));
        }
    }
    for c in reponse.pointer("/message/tool_calls").and_then(|t| t.as_array()).into_iter().flatten() {
        let nom = c.pointer("/function/name").and_then(|n| n.as_str()).unwrap_or("");
        if !permis.iter().any(|p| p == nom) {
            e.push(format!("outil appelé par le modèle sans être déclaré et exposé : « {nom} »"));
        }
        if nom.starts_with("action_") {
            let args = match c.pointer("/function/arguments") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(v) => v.clone(),
                None => Value::Null,
            };
            if args["raison"].as_str().is_none_or(|r| r.trim().is_empty()) {
                e.push(format!("action « {nom} » appelée sans raison (D27)"));
            }
        }
    }
    e
}

fn empreinte_requete(v: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v["messages"].to_string().hash(&mut h);
    h.finish()
}

/// Seuils D29 : chaque alerte une seule fois ; plafond d'1 h = arrêt propre demandé.
fn verifier_limites(e: &Etat) {
    let l = &e.c.limites;
    let mut c = e.compteurs.lock().unwrap();
    let ecoule = e.debut.elapsed();
    let mut alertes: Vec<(String, String, bool)> = vec![];
    if let Some(p) = l.pas_max.filter(|p| c.pas > *p) {
        alertes.push(("pas".into(), format!("{} appels au modèle (plus de 3 fois le maximum mesuré : {})", c.pas, p / 3), false));
    }
    if let Some(t) = l.tokens_max.filter(|t| c.tokens > *t) {
        alertes.push(("tokens".into(), format!("{} tokens (plus de 3 fois le maximum mesuré : {})", c.tokens, t / 3), false));
    }
    if let Some(d) = l.duree_max_s.filter(|d| ecoule.as_secs() > *d) {
        alertes.push(("duree".into(), format!("{} s de mission (plus de 3 fois le maximum mesuré : {} s)", ecoule.as_secs(), d / 3), false));
    }
    if e.dernier_progres.lock().unwrap().elapsed() > l.sans_progres {
        alertes.push(("progres".into(), format!("aucun progrès depuis {} min", l.sans_progres.as_secs() / 60), false));
    }
    let n = c.dernieres.len();
    if n >= 3 && c.dernieres[n - 3..].iter().all(|x| *x == c.dernieres[n - 1]) {
        alertes.push(("boucle".into(), "boucle détectée : la même requête 3 fois de suite".into(), false));
    }
    if ecoule > l.plafond {
        alertes.push(("plafond".into(), format!("plafond d'{} h atteint : arrêt propre demandé", l.plafond.as_secs() / 3600), true));
    }
    for (cle, texte, arreter) in alertes {
        if !c.alertes.contains(&cle) {
            c.alertes.push(cle);
            let _ = e.notes.send(Note::Limite { texte, arreter });
        }
    }
}

// ---------- Appels au modèle ----------

fn ecrire_trace(e: &Etat, ligne: Value, pensee: &str, reponse: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(e.c.dossier.join("echanges.jsonl")) {
        let _ = writeln!(f, "{ligne}");
    }
    if !pensee.trim().is_empty() || !reponse.trim().is_empty() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(e.c.dossier.join("pensee.md")) {
            let n = ligne["n"].as_u64().unwrap_or(0);
            let _ = write!(f, "## Appel {n} — {}\n\n", chrono::Local::now().format("%H:%M:%S"));
            if !pensee.trim().is_empty() {
                let _ = write!(f, "**Réflexion**\n\n{}\n\n", pensee.trim());
            }
            if !reponse.trim().is_empty() {
                let _ = write!(f, "**Réponse**\n\n{}\n\n", reponse.trim());
            }
        }
    }
}

/// Un appel `/api/chat` (format Ollama) : modèle de la mission, `num_ctx` d'Orchestra, trace, contrôles.
async fn chat(e: &Etat, mut corps: Value, chemin: &str) -> Result<Value, (u16, String)> {
    // modèle : « inherit » ou un modèle déclaré dans le manifeste ; jamais un autre
    let demande = corps["model"].as_str().unwrap_or("inherit").to_string();
    let declares: Vec<String> = ["/modele/demande", "/modele/minimum", "/modele/recommande"]
        .iter()
        .filter_map(|p| e.c.agent.brut.pointer(p).and_then(|v| v.as_str()).map(String::from))
        .collect();
    if demande != "inherit" && demande != e.c.modele && !declares.contains(&demande) {
        let m = format!("modèle « {demande} » non déclaré dans le manifeste : refusé");
        let _ = e.notes.send(Note::Refus { texte: m.clone() });
        return Err((403, m));
    }
    let _tour = e.file.lock().await;
    let n = {
        let mut n = e.n.lock().unwrap();
        *n += 1;
        *n
    };
    let debut = Instant::now();
    corps["model"] = json!(e.c.modele);
    let num_ctx_agent = corps.pointer("/options/num_ctx").cloned();
    // même contexte que celui chargé par Orchestra : sinon Ollama rechargerait le modèle (D24)
    if !corps["options"].is_object() {
        corps["options"] = json!({});
    }
    corps["options"]["num_ctx"] = json!(e.c.cfg.num_ctx);
    corps["keep_alive"] = json!(brain::KEEP_ALIVE);
    let flux_demande = corps["stream"].as_bool().unwrap_or(true);
    corps["stream"] = json!(false);

    let rep: Value = if let Some(fixe) = &e.c.reponse_fixe {
        json!({ "model": e.c.modele, "message": { "role": "assistant", "content": fixe }, "done": true, "prompt_eval_count": 0, "eval_count": 0 })
    } else if e.c.cfg.provider == "ollama" {
        let base = e.c.cfg.ollama_url.trim_end_matches('/');
        let r = e
            .c
            .http
            .post(format!("{base}/api/chat"))
            .timeout(brain::DELAI_TOTAL)
            .json(&corps)
            .send()
            .await
            .map_err(|x| (503, format!("modèle injoignable : {x}")))?;
        let statut = r.status();
        let v: Value = r.json().await.map_err(|x| (503, format!("réponse du modèle illisible : {x}")))?;
        if !statut.is_success() {
            return Err((503, format!("le modèle a répondu {statut} : {}", v["error"].as_str().unwrap_or(""))));
        }
        v
    } else {
        par_brain(e, &corps).await?
    };

    let pensee = rep.pointer("/message/thinking").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let texte = rep.pointer("/message/content").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let tokens = rep["prompt_eval_count"].as_u64().unwrap_or(0) + rep["eval_count"].as_u64().unwrap_or(0);
    let ecarts = controler_outils(&e.c.agent, &corps, &rep);
    {
        let mut c = e.compteurs.lock().unwrap();
        c.pas += 1;
        c.tokens += tokens;
        c.dernieres.push(empreinte_requete(&corps));
    }
    let mut ligne = json!({
        "n": n,
        "t": chrono::Local::now().to_rfc3339(),
        "chemin": chemin,
        "modele": e.c.modele,
        "requete": { "messages": corps["messages"], "tools": corps["tools"], "format": corps["format"], "think": corps["think"] },
        "reponse": { "reflexion": pensee, "texte": texte, "appels_outils": rep.pointer("/message/tool_calls").cloned().unwrap_or(Value::Null) },
        "tokens": { "prompt": rep["prompt_eval_count"], "generes": rep["eval_count"] },
        "duree_ms": debut.elapsed().as_millis() as u64,
        "ecarts": ecarts,
    });
    if let Some(nc) = num_ctx_agent.filter(|nc| *nc != json!(e.c.cfg.num_ctx)) {
        ligne["num_ctx_demande_par_agent"] = nc;
    }
    ecrire_trace(e, ligne, &pensee, &texte);
    if !pensee.trim().is_empty() {
        let _ = e.notes.send(Note::Pensee { texte: pensee.clone() });
    }
    if !texte.trim().is_empty() {
        let _ = e.notes.send(Note::Pensee { texte: texte.clone() });
    }
    for c in rep.pointer("/message/tool_calls").and_then(|t| t.as_array()).into_iter().flatten() {
        let nom = c.pointer("/function/name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        let arguments = c.pointer("/function/arguments").cloned().unwrap_or(Value::Null);
        let _ = e.notes.send(Note::Appel { nom, arguments });
    }
    for x in ecarts {
        let _ = e.notes.send(Note::Ecart { texte: x });
    }
    verifier_limites(e);
    let _ = flux_demande;
    Ok(rep)
}

/// Fournisseur autre qu'Ollama (Claude, OpenRouter…) : conversion vers le cerveau d'Orchestra.
async fn par_brain(e: &Etat, corps: &Value) -> Result<Value, (u16, String)> {
    let mut cfg = e.c.cfg.clone();
    cfg.model = e.c.modele.clone();
    let b = Brain::new(cfg, e.c.http.clone());
    let msgs: Vec<Msg> = corps["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| Msg::new(m["role"].as_str().unwrap_or("user"), m["content"].as_str().unwrap_or("")))
        .collect();
    let outils: Vec<Outil> = corps["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| Outil {
            nom: t.pointer("/function/name").and_then(|n| n.as_str()).unwrap_or("").into(),
            description: t.pointer("/function/description").and_then(|n| n.as_str()).unwrap_or("").into(),
            parametres: t.pointer("/function/parameters").cloned().unwrap_or(json!({ "type": "object", "properties": {} })),
        })
        .collect();
    let schema = corps.get("format").filter(|f| f.is_object()).cloned();
    let demande = Demande { outils: &outils, schema: schema.as_ref(), ..Default::default() };
    let r = b.appeler("agent", &msgs, demande, |_| {}).await.map_err(|x| (503, x.to_string()))?;
    let texte = if schema.is_some() { r.appels.first().map(|a| a.arguments.to_string()).unwrap_or(r.texte.clone()) } else { r.texte.clone() };
    let appels: Vec<Value> = if schema.is_some() { vec![] } else { r.appels.iter().map(|a| json!({ "function": { "name": a.nom, "arguments": a.arguments } })).collect() };
    let mut m = json!({ "role": "assistant", "content": texte });
    if !appels.is_empty() {
        m["tool_calls"] = json!(appels);
    }
    Ok(json!({ "model": e.c.modele, "message": m, "done": true }))
}

async fn embed(e: &Etat, mut corps: Value) -> Result<Value, (u16, String)> {
    if corps["model"].as_str().is_none_or(|m| m == "inherit") {
        corps["model"] = json!(e.c.cfg.embedding_model);
    }
    let base = e.c.cfg.ollama_url.trim_end_matches('/');
    let r = e.c.http.post(format!("{base}/api/embed")).timeout(Duration::from_secs(60)).json(&corps).send().await.map_err(|x| (503, x.to_string()))?;
    let v: Value = r.json().await.map_err(|x| (503, x.to_string()))?;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(e.c.dossier.join("echanges.jsonl")) {
        let _ = writeln!(f, "{}", json!({ "t": chrono::Local::now().to_rfc3339(), "chemin": "/api/embed", "modele": corps["model"], "entrees": corps["input"].as_array().map(|a| a.len()).unwrap_or(1) }));
    }
    Ok(v)
}

// ---------- Conversions OpenAI ↔ Ollama ----------

pub fn openai_vers_ollama(o: &Value) -> Value {
    let messages: Vec<Value> = o["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            let mut x = json!({ "role": m["role"], "content": m["content"].as_str().unwrap_or("") });
            if let Some(tc) = m["tool_calls"].as_array() {
                x["tool_calls"] = json!(tc
                    .iter()
                    .map(|c| {
                        let args = c.pointer("/function/arguments").and_then(|a| a.as_str()).and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(json!({}));
                        json!({ "function": { "name": c.pointer("/function/name"), "arguments": args } })
                    })
                    .collect::<Vec<_>>());
            }
            x
        })
        .collect();
    let mut v = json!({ "model": o["model"].as_str().unwrap_or("inherit"), "messages": messages, "stream": false, "options": {} });
    if o["tools"].is_array() {
        v["tools"] = o["tools"].clone();
    }
    if let Some(s) = o.pointer("/response_format/json_schema/schema") {
        v["format"] = s.clone();
    } else if o.pointer("/response_format/type").and_then(|t| t.as_str()) == Some("json_object") {
        v["format"] = json!("json");
    }
    if let Some(t) = o.get("temperature") {
        v["options"]["temperature"] = t.clone();
    }
    if let Some(n) = o.get("max_tokens") {
        v["options"]["num_predict"] = n.clone();
    }
    v
}

pub fn ollama_vers_openai(r: &Value) -> Value {
    let mut message = json!({ "role": "assistant", "content": r.pointer("/message/content").cloned().unwrap_or(json!("")) });
    if let Some(tc) = r.pointer("/message/tool_calls").and_then(|t| t.as_array()) {
        message["tool_calls"] = json!(tc
            .iter()
            .enumerate()
            .map(|(i, c)| json!({ "id": format!("appel-{}", i + 1), "type": "function", "function": { "name": c.pointer("/function/name"), "arguments": c.pointer("/function/arguments").map(|a| a.to_string()).unwrap_or_default() } }))
            .collect::<Vec<_>>());
    }
    let prompt = r["prompt_eval_count"].as_u64().unwrap_or(0);
    let gen = r["eval_count"].as_u64().unwrap_or(0);
    json!({
        "id": "porte", "object": "chat.completion", "model": r["model"],
        "choices": [{ "index": 0, "message": message, "finish_reason": if message.get("tool_calls").is_some() { "tool_calls" } else { "stop" } }],
        "usage": { "prompt_tokens": prompt, "completion_tokens": gen, "total_tokens": prompt + gen }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> Manifest {
        let mut m = Manifest::default();
        m.brut = json!({ "fiche": { "fonctions": [
            { "nom": "read_mail_recu", "expose_au_modele": true },
            { "nom": "action_ecrire_brouillon", "expose_au_modele": true },
            { "nom": "memory_ecrire", "expose_au_modele": false }
        ] } });
        m
    }

    #[test]
    fn outils_controles() {
        let req = json!({ "tools": [{ "type": "function", "function": { "name": "read_mail_recu" } }, { "type": "function", "function": { "name": "memory_ecrire" } }] });
        let rep = json!({ "message": { "tool_calls": [
            { "function": { "name": "action_ecrire_brouillon", "arguments": { "nom": "x" } } },
            { "function": { "name": "supprimer_tout", "arguments": {} } }
        ] } });
        let e = controler_outils(&agent(), &req, &rep);
        assert_eq!(e.len(), 3, "{e:?}");
        assert!(e.iter().any(|x| x.contains("memory_ecrire")));
        assert!(e.iter().any(|x| x.contains("sans raison")));
        assert!(e.iter().any(|x| x.contains("supprimer_tout")));
        let ok = json!({ "message": { "tool_calls": [{ "function": { "name": "action_ecrire_brouillon", "arguments": { "raison": "demandé" } } }] } });
        assert!(controler_outils(&agent(), &json!({}), &ok).is_empty());
    }

    #[test]
    fn conversions_openai() {
        let o = json!({ "model": "inherit", "messages": [{ "role": "user", "content": "x" }], "max_tokens": 50,
            "response_format": { "type": "json_schema", "json_schema": { "name": "s", "schema": { "type": "object" } } } });
        let v = openai_vers_ollama(&o);
        assert_eq!(v["format"], json!({ "type": "object" }));
        assert_eq!(v["options"]["num_predict"], 50);
        let r = json!({ "model": "m", "message": { "content": "ok", "tool_calls": [{ "function": { "name": "f", "arguments": { "a": 1 } } }] }, "prompt_eval_count": 3, "eval_count": 2 });
        let x = ollama_vers_openai(&r);
        assert_eq!(x["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(x["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"], "{\"a\":1}");
        assert_eq!(x["usage"]["total_tokens"], 5);
    }

    #[test]
    fn limites_depuis_certification() {
        let mut m = Manifest::default();
        m.brut = json!({ "certification": { "mesures": { "brouillon-mail": { "duree_max_s": 20, "tokens_max": 1000, "pas_max": 2 } } } });
        let l = Limites::depuis(&m, Some("brouillon-mail"));
        assert_eq!((l.pas_max, l.tokens_max, l.duree_max_s), (Some(6), Some(3000), Some(60)));
        assert_eq!(Limites::depuis(&m, None).pas_max, None);
    }

    #[tokio::test]
    async fn jeton_exige_et_reponse_fixe() {
        let dir = std::env::temp_dir().join(format!("orchestra-test-porte-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut ag = agent();
        ag.brut["modele"] = json!({ "demande": "inherit", "minimum": "petit", "recommande": "grand" });
        let (porte, mut notes) = Porte::ouvrir(Config_ {
            cfg: Config::default(),
            http: reqwest::Client::new(),
            agent: ag,
            modele: "grand".into(),
            dossier: dir.clone(),
            limites: Limites { pas_max: None, tokens_max: None, duree_max_s: None, sans_progres: Duration::from_secs(600), plafond: Duration::from_secs(3600) },
            reponse_fixe: Some("Bonjour".into()),
        })
        .await
        .unwrap();
        let c = reqwest::Client::new();
        let sans = c.post(format!("{}/api/chat", porte.url)).json(&json!({ "messages": [] })).send().await.unwrap();
        assert_eq!(sans.status().as_u16(), 401);
        let r: Value = c.post(format!("{}/api/chat", porte.url)).bearer_auth(&porte.jeton).json(&json!({ "model": "inherit", "messages": [{ "role": "user", "content": "x" }], "stream": false })).send().await.unwrap().json().await.unwrap();
        assert_eq!(r["message"]["content"], "Bonjour");
        let refuse = c.post(format!("{}/api/chat", porte.url)).bearer_auth(&porte.jeton).json(&json!({ "model": "autre-modele", "messages": [] })).send().await.unwrap();
        assert_eq!(refuse.status().as_u16(), 403);
        let o: Value = c.post(format!("{}/v1/chat/completions", porte.url)).bearer_auth(&porte.jeton).json(&json!({ "model": "inherit", "messages": [{ "role": "user", "content": "x" }] })).send().await.unwrap().json().await.unwrap();
        assert_eq!(o["choices"][0]["message"]["content"], "Bonjour");
        assert_eq!(porte.compteurs().0, 2);
        assert!(std::fs::read_to_string(dir.join("echanges.jsonl")).unwrap().lines().count() >= 2);
        let mut vu_refus = false;
        while let Ok(n) = notes.try_recv() {
            vu_refus |= matches!(n, Note::Refus { .. });
        }
        assert!(vu_refus);
        let url = porte.url.clone();
        drop(porte);
        tokio::time::sleep(Duration::from_millis(50)).await;
        // porte fermée : plus rien ne répond
        assert!(c.post(format!("{url}/api/chat")).timeout(Duration::from_secs(2)).send().await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
