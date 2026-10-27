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

/// Modèles installés pour la voix, pour les listes du panneau latéral.
#[derive(Serialize)]
pub struct VoiceModels {
    /// fichiers GGUF de Voxtral (écoute)
    pub stt: Vec<String>,
    /// dossiers Qwen3-TTS (synthèse) ; « Base » = voix clonées, « CustomVoice » = voix préréglées
    pub tts: Vec<String>,
    /// enregistrements de voix/references (voix clonées, avec consentement)
    pub references: Vec<String>,
    /// voix de référence de Pocket TTS (modeles/pocket-tts/voix, licences libres)
    pub pocket_voices: Vec<String>,
    /// le moteur CrispASR (voix à usage personnel) est-il installé ?
    pub crisp: bool,
}

pub fn models(paths: &Paths) -> VoiceModels {
    let m = voix(paths).join("modeles");
    let list = |dir: PathBuf, keep: &dyn Fn(&std::path::Path) -> bool| -> Vec<String> {
        let mut v: Vec<String> = dir
            .read_dir()
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| keep(p)).filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect())
            .unwrap_or_default();
        v.sort();
        v
    };
    let crisp = voix(paths).join("env").join("Lib").join("site-packages").join("crispasr").exists();
    let mut tts = list(m.join("qwen3-tts"), &|p| p.join("model.safetensors").exists());
    if crisp {
        // voix à usage personnel (licences non commerciales), servies par CrispASR
        for (dir, prefix) in [("voxtral-tts", "voxtral-tts"), ("pocket-tts", "pocket-tts")] {
            for f in list(m.join(dir), &|p| p.extension().map_or(false, |e| e == "gguf")) {
                if prefix != "pocket-tts" || f.contains("french") {
                    tts.push(format!("crisp:{dir}/{f}"));
                }
            }
        }
    }
    let mut pocket_voices = Vec::new();
    collect_wavs(&m.join("pocket-tts").join("voix"), &mut pocket_voices, 3);
    pocket_voices.sort();
    VoiceModels {
        stt: list(m.join("voxtral"), &|p| p.extension().map_or(false, |e| e == "gguf")),
        tts,
        pocket_voices,
        crisp,
        references: list(voix(paths).join("references"), &|p| {
            p.extension().map_or(false, |e| ["wav", "mp3", "flac", "m4a"].contains(&e.to_string_lossy().to_lowercase().as_str()))
        }),
    }
}

fn collect_wavs(dir: &std::path::Path, out: &mut Vec<String>, depth: u32) {
    let Ok(rd) = dir.read_dir() else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            collect_wavs(&p, out, depth - 1);
        } else if p.extension().map_or(false, |x| x == "wav") {
            if let Some(n) = p.file_stem() {
                out.push(n.to_string_lossy().to_string());
            }
        }
    }
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
    let mo = models(paths);
    if mo.stt.is_empty() {
        m.push("modèle Voxtral Mini 4B Realtime".into());
    }
    if mo.tts.is_empty() {
        m.push("modèle Qwen3-TTS".into());
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
        // modèles choisis dans le panneau (sinon les premiers installés)
        let mo = models(paths);
        let stt = if mo.stt.contains(&cfg.voice_stt_model) { cfg.voice_stt_model.clone() } else { mo.stt[0].clone() };
        let tts = if mo.tts.contains(&cfg.voice_tts_model) { cfg.voice_tts_model.clone() } else { mo.tts[0].clone() };
        cmd.args(["--stt-model", &stt, "--tts-model", &tts]);
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
