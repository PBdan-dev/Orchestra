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
        "event": pour_le_journal(ev, &day),
    });
    // ligne chaînée (lot T6)
    let _ = crate::integrite::ajouter(&path, line);
}

/// Forme d'un événement dans le journal (lot M1) : la réponse d'un tour n'y est pas recopiée,
/// seulement un aperçu et une référence vers l'archive de la conversation, qui la garde en entier.
pub(crate) fn pour_le_journal(ev: &Event, jour: &str) -> serde_json::Value {
    const APERCU: usize = 160;
    let mut v = serde_json::to_value(ev).unwrap_or_default();
    if let Event::Done { answer, .. } = ev {
        if let Some(o) = v.as_object_mut() {
            o.remove("answer");
            o.remove("message_historique");
            let mut apercu: String = answer.chars().take(APERCU).collect();
            if answer.chars().count() > APERCU {
                apercu.push('…');
            }
            o.insert("apercu".into(), serde_json::json!(apercu));
            o.insert("archive".into(), serde_json::json!(format!("memoire/archives/{jour}.md")));
        }
    }
    v
}

pub fn read_day(state: &AppState, day: &str) -> Vec<serde_json::Value> {
    lire_jour(&state.paths.journal, day)
}

/// Journal d'un jour, en clair ou compressé à J+1 (lot M1).
fn lire_jour(dossier: &std::path::Path, day: &str) -> Vec<serde_json::Value> {
    crate::compression::lire_texte(&dossier.join(format!("{}.jsonl", crate::paths::safe_name(day))))
        .map(|s| s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
        .unwrap_or_default()
}

pub fn list_days(state: &AppState) -> Vec<String> {
    jours(&state.paths.journal)
}

/// Jours du journal, du plus récent au plus ancien (fichiers en clair ou compressés).
fn jours(dossier: &std::path::Path) -> Vec<String> {
    let mut days: Vec<String> = std::fs::read_dir(dossier)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.strip_suffix(".jsonl").or_else(|| n.strip_suffix(".jsonl.zst")).map(String::from)
        })
        .collect();
    days.sort();
    days.dedup();
    days.reverse();
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_non_recopiee_dans_le_journal() {
        let longue = "réponse ".repeat(60);
        let v = pour_le_journal(&Event::Done { run_id: "r".into(), answer: longue, intent: "discussion".into(), message_historique: "message entier envoyé au cerveau".into() }, "2026-11-01");
        assert!(v.get("answer").is_none() && v.get("message_historique").is_none(), "{v}");
        assert_eq!(v["archive"], "memoire/archives/2026-11-01.md");
        let apercu = v["apercu"].as_str().unwrap();
        assert!(apercu.starts_with("réponse réponse") && apercu.chars().count() <= 161, "{apercu}");
        assert_eq!((v["type"].as_str(), v["intent"].as_str(), v["run_id"].as_str()), (Some("done"), Some("discussion"), Some("r")));
        // réponse courte : entière dans l'aperçu ; les autres événements ne changent pas
        let c = pour_le_journal(&Event::Done { run_id: "r".into(), answer: "Bonjour PB.".into(), intent: "discussion".into(), message_historique: String::new() }, "2026-11-01");
        assert_eq!(c["apercu"], "Bonjour PB.");
        let n = Event::NothingToMemorize { run_id: "r".into() };
        assert_eq!(pour_le_journal(&n, "2026-11-01"), serde_json::to_value(&n).unwrap());
    }

    #[test]
    fn journal_compresse_lu_comme_avant() {
        let racine = std::env::temp_dir().join(format!("orchestra-m1j-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let p = crate::paths::Paths::at(racine.clone()).unwrap();
        std::fs::write(p.journal.join("2026-10-30.jsonl"), "{\"at\":\"a\",\"event\":{\"type\":\"phase\"}}\n{\"at\":\"b\",\"event\":{\"type\":\"done\"}}\n").unwrap();
        std::fs::write(p.journal.join("2026-10-31.jsonl"), "{\"at\":\"c\",\"event\":{\"type\":\"phase\"}}\n").unwrap();
        crate::compression::compresser(&p, &p.journal.join("2026-10-30.jsonl")).unwrap();
        assert_eq!(lire_jour(&p.journal, "2026-10-30").len(), 2);
        assert_eq!(jours(&p.journal), vec!["2026-10-31".to_string(), "2026-10-30".to_string()]);
        let _ = std::fs::remove_dir_all(racine);
    }
}
