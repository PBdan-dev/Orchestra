//! Module voix : démarre et arrête le serveur voix local (Pipecat, en Python), qui vit dans
//! `<données>/voix/`. L'interface s'y connecte ensuite directement en WebSocket
//! (ws://127.0.0.1:<port>) : micro vers le serveur, voix vers le haut-parleur.
//!
//! Le module est optionnel : s'il n'est pas installé ou désactivé, rien d'autre ne change.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;

use crate::{config::Config, paths::Paths};

#[derive(Default)]
pub struct VoiceProcess {
    child: Mutex<Option<Child>>,
}

#[derive(Serialize)]
pub struct VoiceStatus {
    pub installed: bool,
    pub missing: Vec<String>,
    pub running: bool,
    pub port: u16,
    pub dir: String,
}

fn voix(paths: &Paths) -> PathBuf {
    paths.root.join("voix")
}

fn python(paths: &Paths) -> PathBuf {
    voix(paths).join("env").join("Scripts").join("python.exe")
}

/// Ce qui manque pour que la voix fonctionne (vide = prêt).
fn missing(paths: &Paths) -> Vec<String> {
    let v = voix(paths);
    let mut m = Vec::new();
    if !python(paths).exists() {
        m.push("environnement Python (Pipecat, PyTorch)".into());
    }
    // moteur de Voxtral : version précompilée (Vulkan, installée par pip) ou compilée sur place
    let prebuilt = v.join("env").join("Lib").join("site-packages").join("transcribe_cpp_native").join("_native").join("transcribe.dll");
    let has_dll = prebuilt.exists()
        || v.join("transcribe.cpp").read_dir().ok().map_or(false, |rd| {
            rd.flatten().any(|e| {
                let p = e.path();
                p.file_name().map_or(false, |n| n.to_string_lossy().starts_with("build")) && find_file(&p, "transcribe.dll", 4)
            })
        });
    if !has_dll {
        m.push("transcribe.cpp (moteur de Voxtral)".into());
    }
    if !v.join("modeles").join("voxtral").join("Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf").exists() {
        m.push("modèle Voxtral Mini 4B Realtime".into());
    }
    if !v.join("modeles").join("qwen3-tts").join("Qwen3-TTS-12Hz-1.7B-CustomVoice").join("model.safetensors").exists() {
        m.push("modèle Qwen3-TTS 1.7B".into());
    }
    m
}

fn find_file(dir: &std::path::Path, name: &str, depth: u32) -> bool {
    let Ok(rd) = dir.read_dir() else { return false };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file() && p.file_name().map_or(false, |n| n == name) {
            return true;
        }
        if depth > 0 && p.is_dir() && find_file(&p, name, depth - 1) {
            return true;
        }
    }
    false
}

impl VoiceProcess {
    pub fn status(&self, paths: &Paths, cfg: &Config) -> VoiceStatus {
        let miss = missing(paths);
        VoiceStatus {
            installed: miss.is_empty(),
            missing: miss,
            running: self.is_running(),
            port: cfg.voice_port,
            dir: voix(paths).to_string_lossy().to_string(),
        }
    }

    fn is_running(&self) -> bool {
        let mut g = self.child.lock().unwrap();
        match g.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(None) => true,
                _ => {
                    *g = None;
                    false
                }
            },
            None => false,
        }
    }

    pub fn start(&self, paths: &Paths, cfg: &Config) -> anyhow::Result<()> {
        if self.is_running() {
            return Ok(());
        }
        let miss = missing(paths);
        if !miss.is_empty() {
            anyhow::bail!("module voix incomplet : {}", miss.join(", "));
        }
        let mut cmd = Command::new(python(paths));
        cmd.args(["-m", "orchestra_voix.server", "--port", &cfg.voice_port.to_string(), "--llm", &cfg.voice_llm, "--speaker", &cfg.voice_speaker])
            .env("ORCHESTRA_HOME", &paths.root)
            .env("PYTHONIOENCODING", "utf-8")
            .current_dir(voix(paths))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !cfg.voice_reference.is_empty() {
            cmd.args(["--reference", &cfg.voice_reference]);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let child = cmd.spawn()?;
        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Fin du journal du serveur voix, pour comprendre un échec de démarrage.
    pub fn log_tail(paths: &Paths, lines: usize) -> String {
        let p = voix(paths).join("voix.log");
        let s = std::fs::read_to_string(p).unwrap_or_default();
        let v: Vec<&str> = s.lines().collect();
        v[v.len().saturating_sub(lines)..].join("\n")
    }
}
