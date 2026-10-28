pub mod agents;
pub mod brain;
pub mod config;
pub mod evaluation;
pub mod events;
mod journal;
pub mod market;
pub mod memory;
pub mod mesures;
pub mod orchestrator;
pub mod paths;
pub mod tools;
mod voice;

use std::sync::{atomic::AtomicBool, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::{agents::Manifest, brain::{Brain, BrainInfo, Msg}, config::Config, paths::Paths};

pub struct AppState {
    pub config: Mutex<Config>,
    pub paths: Paths,
    pub http: reqwest::Client,
    pub busy: AtomicBool,
    pub voice: voice::VoiceProcess,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(state: State<AppState>, config: Config) -> CmdResult<()> {
    config.save(&state.paths).map_err(err)?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
async fn detect_brain(state: State<'_, AppState>) -> CmdResult<BrainInfo> {
    let cfg = state.config.lock().unwrap().clone();
    let mut b = Brain::new(cfg, state.http.clone());
    Ok(b.detect().await)
}

#[tauri::command]
async fn load_model(state: State<'_, AppState>) -> CmdResult<BrainInfo> {
    let cfg = state.config.lock().unwrap().clone();
    let mut b = Brain::new(cfg, state.http.clone());
    let info = b.detect().await;
    if b.cfg.provider != "ollama" || info.loaded {
        return Ok(info);
    }
    if !info.online {
        return Err(info.detail);
    }
    b.load_ollama().await.map_err(err)?;
    Ok(b.detect().await)
}

#[tauri::command]
fn install_agent(state: State<AppState>, id: String) -> CmdResult<Manifest> {
    let m = agents::local_registry(&state.paths)
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("agent {id} absent du registre"))?;
    agents::install(&state.paths, &m).map_err(err)?;
    memory::ensure_agent_memory(&state.paths, &m).map_err(err)?;
    Ok(m)
}

/// Désinstalle un agent : son manifeste est retiré, sa mémoire est conservée.
#[tauri::command]
fn uninstall_agent(state: State<AppState>, id: String) -> CmdResult<()> {
    let p = state.paths.installed_manifest(&id);
    if p.exists() {
        std::fs::remove_file(p).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
async fn send_message(app: AppHandle, state: State<'_, AppState>, text: String, history: Vec<Msg>) -> CmdResult<String> {
    if state.busy.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Orchestra est déjà en train de traiter une demande.".into());
    }
    let run_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let rid = run_id.clone();
    tauri::async_runtime::spawn(async move {
        orchestrator::run(app, rid, text, history).await;
    });
    Ok(run_id)
}

#[derive(Serialize)]
struct AgentsList {
    installed: Vec<Manifest>,
    registry: Vec<Manifest>,
}

#[tauri::command]
fn list_agents(state: State<AppState>) -> AgentsList {
    AgentsList { installed: agents::installed(&state.paths), registry: agents::local_registry(&state.paths) }
}

#[tauri::command]
async fn market_preview(state: State<'_, AppState>) -> CmdResult<Vec<Manifest>> {
    let cfg = state.config.lock().unwrap().clone();
    market::fetch_remote(&state.http, &cfg).await
}

#[tauri::command]
fn memory_tree(state: State<AppState>) -> Vec<memory::MemoryFile> {
    memory::tree(&state.paths)
}

fn safe_path(state: &AppState, rel: &str) -> CmdResult<std::path::PathBuf> {
    let p = std::path::Path::new(rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("chemin refusé".into());
    }
    Ok(state.paths.root.join(p))
}

#[tauri::command]
fn read_text_file(state: State<AppState>, rel: String) -> CmdResult<String> {
    let p = safe_path(&state, &rel)?;
    std::fs::read_to_string(p).map_err(err)
}

#[tauri::command]
fn write_text_file(state: State<AppState>, rel: String, content: String) -> CmdResult<()> {
    let p = safe_path(&state, &rel)?;
    std::fs::write(p, content).map_err(err)
}

#[tauri::command]
fn list_journal_days(state: State<AppState>) -> Vec<String> {
    journal::list_days(&state)
}

#[tauri::command]
fn read_journal(state: State<AppState>, day: String) -> Vec<serde_json::Value> {
    journal::read_day(&state, &day)
}

#[derive(Serialize)]
struct PathsInfo {
    root: String,
    memoire: String,
    agents: String,
    workspace: String,
    journal: String,
}

#[tauri::command]
fn get_paths(state: State<AppState>) -> PathsInfo {
    let s = |p: &std::path::Path| p.to_string_lossy().to_string();
    PathsInfo {
        root: s(&state.paths.root),
        memoire: s(&state.paths.memoire),
        agents: s(&state.paths.agents),
        workspace: s(&state.paths.workspace),
        journal: s(&state.paths.journal),
    }
}

#[tauri::command]
fn open_folder(app: AppHandle, state: State<AppState>, which: String) -> CmdResult<()> {
    let p = match which.as_str() {
        "memoire" => state.paths.memoire.clone(),
        "agents" => state.paths.agents.clone(),
        "workspace" => state.paths.workspace.clone(),
        "journal" => state.paths.journal.clone(),
        _ => state.paths.root.clone(),
    };
    app.opener().open_path(p.to_string_lossy().to_string(), None::<&str>).map_err(err)
}

#[tauri::command]
fn is_busy(state: State<AppState>) -> bool {
    state.busy.load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
fn voice_status(state: State<AppState>) -> voice::VoiceStatus {
    let cfg = state.config.lock().unwrap().clone();
    state.voice.status(&state.paths, &cfg)
}

#[tauri::command]
fn voice_start(state: State<AppState>) -> CmdResult<voice::VoiceStatus> {
    let cfg = state.config.lock().unwrap().clone();
    state.voice.start(&state.paths, &cfg).map_err(err)?;
    Ok(state.voice.status(&state.paths, &cfg))
}

/// Démarre le serveur voix dans un mode : « voix » (conversation) ou « dictee » (écoute seule).
#[tauri::command]
fn voice_start_mode(state: State<AppState>, mode: String) -> CmdResult<voice::VoiceStatus> {
    let cfg = state.config.lock().unwrap().clone();
    state.voice.start_mode(&state.paths, &cfg, &mode).map_err(err)?;
    Ok(state.voice.status(&state.paths, &cfg))
}

#[tauri::command]
fn voice_stop(state: State<AppState>) -> voice::VoiceStatus {
    state.voice.stop();
    let cfg = state.config.lock().unwrap().clone();
    state.voice.status(&state.paths, &cfg)
}

#[tauri::command]
fn voice_models(state: State<AppState>) -> voice::VoiceModels {
    voice::models(&state.paths)
}

/// Redémarre le serveur voix (changement de modèle d'écoute ou de synthèse).
#[tauri::command]
fn voice_restart(state: State<AppState>) -> CmdResult<voice::VoiceStatus> {
    state.voice.stop();
    let cfg = state.config.lock().unwrap().clone();
    state.voice.start(&state.paths, &cfg).map_err(err)?;
    Ok(state.voice.status(&state.paths, &cfg))
}

/// Ollama : charge `load` (gardé 30 min) et décharge `unload` pour libérer la mémoire GPU.
#[tauri::command]
async fn ollama_switch(state: State<'_, AppState>, load: Option<String>, unload: Option<String>) -> CmdResult<()> {
    let base = state.config.lock().unwrap().ollama_url.trim_end_matches('/').to_string();
    if let Some(m) = unload.filter(|m| !m.is_empty()) {
        let _ = state.http.post(format!("{base}/api/generate")).json(&serde_json::json!({ "model": m, "keep_alive": 0 })).send().await;
    }
    if let Some(m) = load.filter(|m| !m.is_empty()) {
        let r = state
            .http
            .post(format!("{base}/api/generate"))
            .timeout(std::time::Duration::from_secs(900))
            .json(&serde_json::json!({ "model": m, "prompt": "", "keep_alive": "30m", "stream": false }))
            .send()
            .await
            .map_err(err)?;
        if !r.status().is_success() {
            return Err(format!("Ollama a répondu {}", r.status()));
        }
    }
    Ok(())
}

#[tauri::command]
fn voice_log(state: State<AppState>) -> String {
    voice::VoiceProcess::log_tail(&state.paths, 40)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = Paths::init().map_err(|e| anyhow::anyhow!("dossiers : {e}"))?;
            let config = Config::load(&paths);
            if !paths.config.exists() {
                let _ = config.save(&paths);
            }
            agents::seed_registry(&paths).map_err(|e| anyhow::anyhow!("registre : {e}"))?;
            memory::seed_main(&paths, &config.user_name).map_err(|e| anyhow::anyhow!("mémoire : {e}"))?;
            let http = reqwest::Client::builder().user_agent("Orchestra/0.1").build().map_err(|e| anyhow::anyhow!("http : {e}"))?;
            let state = AppState { config: Mutex::new(config), paths, http, busy: AtomicBool::new(false), voice: voice::VoiceProcess::default() };
            {
                let cfg = state.config.lock().unwrap().clone();
                if cfg.voice_enabled && cfg.voice_autostart {
                    let _ = state.voice.start(&state.paths, &cfg);
                }
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            detect_brain,
            load_model,
            install_agent,
            uninstall_agent,
            send_message,
            list_agents,
            market_preview,
            memory_tree,
            read_text_file,
            write_text_file,
            list_journal_days,
            read_journal,
            get_paths,
            open_folder,
            is_busy,
            voice_status,
            voice_start,
            voice_stop,
            voice_log,
            voice_models,
            voice_start_mode,
            voice_restart,
            ollama_switch,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement d'Orchestra")
        .run(|app, event| {
            // le serveur voix s'arrête avec Orchestra
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().voice.stop();
            }
        });
}
