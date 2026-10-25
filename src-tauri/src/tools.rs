//! Outils que les agents peuvent se donner. Tous sont cantonnés à
//! l'espace de travail (dossier Orchestra › espace-de-travail), sauf fetch_url.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::paths::Paths;

pub const ALL: &[(&str, &str)] = &[
    ("now", "Date et heure actuelles. Entrée : {}"),
    ("list_dir", "Liste un dossier de l'espace de travail. Entrée : {\"path\": \".\"}"),
    ("read_file", "Lit un fichier texte de l'espace de travail. Entrée : {\"path\": \"notes.md\"}"),
    ("write_file", "Écrit (ou remplace) un fichier texte dans l'espace de travail. Entrée : {\"path\": \"resultat.md\", \"content\": \"...\"}"),
    ("fetch_url", "Télécharge une page web et renvoie son texte (tronqué). Entrée : {\"url\": \"https://...\"}"),
];

pub fn describe(tools: &[String]) -> String {
    ALL.iter()
        .filter(|(n, _)| tools.iter().any(|t| t == n))
        .map(|(n, d)| format!("- {n} : {d}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve(paths: &Paths, rel: &str) -> anyhow::Result<PathBuf> {
    let rel = rel.trim().trim_start_matches(['/', '\\']);
    let rel = if rel.is_empty() { "." } else { rel };
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        anyhow::bail!("chemin refusé (hors de l'espace de travail) : {rel}");
    }
    Ok(paths.workspace.join(p))
}

pub async fn execute(
    name: &str,
    input: &Value,
    paths: &Paths,
    http: &reqwest::Client,
) -> anyhow::Result<String> {
    match name {
        "now" => Ok(chrono::Local::now().format("%A %d %B %Y, %H:%M").to_string()),
        "list_dir" => {
            let p = resolve(paths, input.get("path").and_then(|v| v.as_str()).unwrap_or("."))?;
            let mut out = vec![];
            for e in std::fs::read_dir(&p)?.flatten() {
                let md = e.metadata()?;
                let kind = if md.is_dir() { "dossier" } else { "fichier" };
                out.push(format!("{kind}\t{}\t{} o", e.file_name().to_string_lossy(), md.len()));
            }
            if out.is_empty() {
                out.push("(vide)".into());
            }
            Ok(out.join("\n"))
        }
        "read_file" => {
            let p = resolve(paths, input.get("path").and_then(|v| v.as_str()).unwrap_or(""))?;
            let s = std::fs::read_to_string(&p)?;
            Ok(truncate(&s, 12_000))
        }
        "write_file" => {
            let rel = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let p = resolve(paths, rel)?;
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, content)?;
            Ok(format!("écrit : {} ({} caractères)", p.display(), content.chars().count()))
        }
        "fetch_url" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                anyhow::bail!("URL invalide");
            }
            let r = http.get(url).timeout(std::time::Duration::from_secs(20)).send().await?;
            let body = r.text().await?;
            Ok(truncate(&strip_html(&body), 10_000))
        }
        other => anyhow::bail!("outil inconnu : {other}"),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}\n[... tronqué]")
    }
}

/// Retire grossièrement les balises HTML et les scripts.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut skip_until: Option<&str> = None;
    let lower = html.to_lowercase();
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = skip_until {
            if lower[i..].starts_with(end) {
                i += end.len();
                skip_until = None;
            } else {
                i += 1;
            }
            continue;
        }
        if lower[i..].starts_with("<script") {
            skip_until = Some("</script>");
            i += 7;
            continue;
        }
        if lower[i..].starts_with("<style") {
            skip_until = Some("</style>");
            i += 6;
            continue;
        }
        let c = html[i..].chars().next().unwrap();
        if c == '<' {
            in_tag = true;
            out.push(' ');
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
        i += c.len_utf8();
    }
    // compacte les blancs
    let mut compact = String::with_capacity(out.len());
    let mut last_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !last_space {
                compact.push(if c == '\n' { '\n' } else { ' ' });
            }
            last_space = true;
        } else {
            compact.push(c);
            last_space = false;
        }
    }
    compact
}
