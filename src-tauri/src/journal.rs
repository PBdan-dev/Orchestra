//! Journal : chaque événement est ajouté dans journal/AAAA-MM-JJ.jsonl.

use tauri::{AppHandle, Manager};

use crate::{events::Event, AppState};

pub fn append(app: &AppHandle, ev: &Event) {
    // Les fragments de texte sont trop nombreux pour le journal.
    if matches!(ev, Event::Thought { .. } | Event::Token { .. }) {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else { return };
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let path = state.paths.journal.join(format!("{day}.jsonl"));
    let line = serde_json::json!({
        "at": chrono::Local::now().to_rfc3339(),
        "event": ev,
    });
    // ligne chaînée (lot T6)
    let _ = crate::integrite::ajouter(&path, line);
}

pub fn read_day(state: &AppState, day: &str) -> Vec<serde_json::Value> {
    let path = state.paths.journal.join(format!("{}.jsonl", crate::paths::safe_name(day)));
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn list_days(state: &AppState) -> Vec<String> {
    let mut days: Vec<String> = std::fs::read_dir(&state.paths.journal)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.strip_suffix(".jsonl").map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    days.sort();
    days.reverse();
    days
}
