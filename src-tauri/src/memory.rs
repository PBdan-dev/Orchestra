//! Mémoire d'Orchestra : des fichiers Markdown lisibles, rien de caché.
//!
//! - memoire/orchestra.md : faits durables, par section (## Utilisateur, ## Préférences…)
//! - memoire/agents/<id>.md : ce que chaque agent a fait et appris
//! - memoire/archives/AAAA-MM-JJ.md : les conversations brutes
//! - memoire/.index-vectoriel.json : cache d'embeddings (optionnel, recalculable)
//!
//! Le rappel se fait d'abord par mots communs (explicable), et, si activé,
//! par similarité vectorielle via les embeddings d'Ollama.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::{agents::Manifest, brain::Brain, events::Recalled, paths::Paths};

const STOP: &[&str] = &[
    "les", "des", "une", "est", "pas", "que", "qui", "quoi", "pour", "avec", "dans", "sur", "sous",
    "par", "mais", "donc", "car", "aux", "cette", "ces", "ceci", "cela", "comme", "plus", "moins",
    "tout", "tous", "toute", "toutes", "mon", "mes", "ton", "tes", "son", "ses", "nos", "vos",
    "leur", "leurs", "elle", "ils", "elles", "nous", "vous", "moi", "toi", "lui", "the", "and",
    "you", "are", "was", "ont", "fait", "faire", "fais", "peux", "peut", "veux", "veut", "voila",
    "aussi", "bien", "tres", "puis", "etre", "avoir", "sont", "ete", "sera", "quand", "alors",
    "orchestra", "agent", "stp", "svp", "merci", "bonjour", "salut",
];

/// Minuscule, sans accents, lettres/chiffres/espaces seulement.
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'À' | 'Â' | 'Ä' => 'a',
            'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'Î' | 'Ï' => 'i',
            'Ô' | 'Ö' => 'o',
            'Ù' | 'Û' | 'Ü' => 'u',
            'Ç' => 'c',
            c if c.is_alphanumeric() => c.to_ascii_lowercase(),
            _ => ' ',
        })
        .collect()
}

pub fn keywords(s: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for w in fold(s).split_whitespace() {
        if w.len() >= 3 && !STOP.contains(&w) && !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Fact {
    pub file: String,
    pub section: String,
    pub text: String,
}

pub fn seed_main(paths: &Paths, user_name: &str) -> anyhow::Result<()> {
    let p = paths.main_memory();
    if p.exists() {
        return Ok(());
    }
    let who = if user_name.is_empty() { "l'utilisateur" } else { user_name };
    let content = format!(
        "# Mémoire d'Orchestra\n\n\
> Ce fichier est la mémoire principale d'Orchestra. Il est écrit pour être lu par un humain.\n\
> Orchestra n'y ajoute que des faits durables, une ligne par fait, datée. Tu peux le modifier\n\
> librement : ce que tu effaces est oublié, ce que tu ajoutes est retenu.\n\n\
## Utilisateur\n- ({}) Orchestra a été installé pour {}.\n\n\
## Préférences\n\n\
## Projets\n\n\
## Connaissances\n\n\
## Agents de confiance\n\
<!-- besoin → agent qui a su y répondre. Orchestra consulte cette liste avant de chercher ailleurs. -->\n\n",
        today(),
        who
    );
    std::fs::write(p, content)?;
    Ok(())
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn read_main(paths: &Paths) -> String {
    std::fs::read_to_string(paths.main_memory()).unwrap_or_default()
}

/// Profil de l'utilisateur (bloc 2 du prompt) : faits des sections Utilisateur et Préférences.
/// Renvoie le texte et le nombre de faits laissés de côté si le budget de caractères est dépassé.
pub fn profil(paths: &Paths, max_car: usize) -> (String, usize) {
    let faits: Vec<Fact> = parse_facts("orchestra.md", &read_main(paths))
        .into_iter()
        .filter(|f| f.section == "Utilisateur" || f.section == "Préférences")
        .collect();
    let mut out = String::new();
    let mut ecartes = 0;
    for f in &faits {
        let ligne = format!("- {}\n", f.text);
        if out.chars().count() + ligne.chars().count() > max_car {
            ecartes += 1;
        } else {
            out.push_str(&ligne);
        }
    }
    if ecartes > 0 {
        out.push_str(&format!("[… {ecartes} fait(s) du profil non repris ici, faute de place : ils restent dans orchestra.md]\n"));
    }
    (out.trim_end().to_string(), ecartes)
}

pub fn parse_facts(file: &str, text: &str) -> Vec<Fact> {
    let mut section = String::from("Général");
    let mut out = vec![];
    for line in text.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("## ") {
            section = h.trim().to_string();
        } else if let Some(f) = t.strip_prefix("- ") {
            if !f.is_empty() {
                out.push(Fact { file: file.to_string(), section: section.clone(), text: f.to_string() });
            }
        }
    }
    out
}

pub fn all_facts(paths: &Paths) -> Vec<Fact> {
    let mut facts = parse_facts("orchestra.md", &read_main(paths));
    if let Ok(rd) = std::fs::read_dir(&paths.memoire_agents) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") {
                if let Ok(s) = std::fs::read_to_string(e.path()) {
                    facts.extend(parse_facts(&format!("agents/{name}"), &s));
                }
            }
        }
    }
    facts
}

fn keyword_score(fact_words: &[String], query_words: &[String]) -> (f32, Vec<String>) {
    let mut common = vec![];
    for q in query_words {
        if fact_words.iter().any(|f| f == q) {
            common.push(q.clone());
        } else if q.len() >= 5 {
            let stem = &q[..5];
            if fact_words.iter().any(|f| f.starts_with(stem)) {
                common.push(format!("{q}~"));
            }
        }
    }
    if common.is_empty() {
        return (0.0, common);
    }
    let score = common.len() as f32 / (query_words.len().max(1) as f32).sqrt();
    (score, common)
}

/// Rappel : mots communs, et similarité vectorielle si activée.
pub async fn recall(paths: &Paths, brain: Option<&Brain>, query: &str, max: usize) -> Vec<Recalled> {
    let facts = all_facts(paths);
    let qw = keywords(query);
    let mut scored: Vec<(f32, String, &Fact)> = vec![];

    for f in &facts {
        let fw = keywords(&f.text);
        let (s, common) = keyword_score(&fw, &qw);
        if s > 0.0 {
            let reason = format!(
                "mots communs : {}",
                common.iter().map(|c| c.trim_end_matches('~').to_string()).collect::<Vec<_>>().join(", ")
            );
            scored.push((s, reason, f));
        }
    }

    // Index vectoriel optionnel
    if let Some(brain) = brain {
        if brain.cfg.embeddings_enabled {
            if let Ok(Some(qv)) = brain.embed(query).await {
                let mut cache = EmbedCache::load(paths);
                for f in &facts {
                    let key = cache_key(&f.text);
                    let fv = match cache.map.get(&key) {
                        Some(v) => Some(v.clone()),
                        None => match brain.embed(&f.text).await {
                            Ok(Some(v)) => {
                                cache.map.insert(key.clone(), v.clone());
                                cache.dirty = true;
                                Some(v)
                            }
                            _ => None,
                        },
                    };
                    if let Some(fv) = fv {
                        let sim = cosine(&qv, &fv);
                        if sim > 0.55 {
                            let bonus = (sim - 0.5) * 3.0;
                            if let Some(e) = scored.iter_mut().find(|e| std::ptr::eq(e.2, f)) {
                                e.0 += bonus;
                                e.1 = format!("{} · similarité {:.0} %", e.1, sim * 100.0);
                            } else {
                                scored.push((bonus, format!("similarité sémantique {:.0} %", sim * 100.0), f));
                            }
                        }
                    }
                }
                cache.save(paths);
            }
        }
    }

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(max)
        .map(|(s, reason, f)| Recalled {
            file: f.file.clone(),
            section: f.section.clone(),
            text: f.text.clone(),
            score: s,
            reason,
        })
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

fn cache_key(text: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

struct EmbedCache {
    map: HashMap<String, Vec<f32>>,
    dirty: bool,
}

impl EmbedCache {
    fn path(paths: &Paths) -> std::path::PathBuf {
        paths.memoire.join(".index-vectoriel.json")
    }
    fn load(paths: &Paths) -> Self {
        let map = std::fs::read_to_string(Self::path(paths))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        EmbedCache { map, dirty: false }
    }
    fn save(&self, paths: &Paths) {
        if self.dirty {
            if let Ok(s) = serde_json::to_string(&self.map) {
                let _ = std::fs::write(Self::path(paths), s);
            }
        }
    }
}

/// Ajoute une ligne « - (date) fait » sous une section du fichier principal.
/// Renvoie la ligne écrite.
pub fn append_fact(paths: &Paths, section: &str, fact: &str) -> anyhow::Result<String> {
    let line = format!("- ({}) {}", today(), fact.trim());
    let text = read_main(paths);
    let new_text = insert_in_section(&text, section, &line);
    std::fs::write(paths.main_memory(), new_text)?;
    Ok(line)
}

fn insert_in_section(text: &str, section: &str, line: &str) -> String {
    let header = format!("## {section}");
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        let mut t = text.trim_end().to_string();
        t.push_str(&format!("\n\n{header}\n{line}\n"));
        return t;
    };
    // fin de section = prochaine ligne "## " ou fin
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate().skip(start + 1) {
        if l.starts_with("## ") {
            end = i;
            break;
        }
    }
    // on insère après le dernier contenu non vide de la section
    let mut insert_at = end;
    while insert_at > start + 1 && lines[insert_at - 1].trim().is_empty() {
        insert_at -= 1;
    }
    let mut out: Vec<String> = lines[..insert_at].iter().map(|s| s.to_string()).collect();
    out.push(line.to_string());
    if insert_at == end && end < lines.len() {
        out.push(String::new());
    }
    out.extend(lines[insert_at..].iter().map(|s| s.to_string()));
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// « besoin → agent-id (Nom) · réussi N× »
pub struct Trust {
    pub need: String,
    pub agent_id: String,
    pub successes: u32,
}

pub fn trusted_agents(paths: &Paths) -> Vec<Trust> {
    parse_facts("orchestra.md", &read_main(paths))
        .into_iter()
        .filter(|f| f.section == "Agents de confiance")
        .filter_map(|f| {
            let (need, rest) = f.text.split_once('→')?;
            let need = need.trim().trim_start_matches('(').trim();
            // retire une éventuelle date "(2026-10-25) "
            let need = match need.find(')') {
                Some(i) if need.starts_with(|c: char| c.is_ascii_digit()) || need.starts_with('(') => need[i + 1..].trim(),
                _ => need,
            };
            let need = need.trim_matches(|c| c == '«' || c == '»' || c == ' ').to_string();
            let agent_id = rest.trim().split_whitespace().next()?.to_string();
            let successes = rest
                .rsplit("réussi")
                .next()
                .and_then(|s| s.trim().trim_end_matches('×').trim().parse().ok())
                .unwrap_or(1);
            Some(Trust { need, agent_id, successes })
        })
        .collect()
}

/// Mémorise qu'un agent a su répondre à un besoin (ou incrémente le compteur).
pub fn record_trust(paths: &Paths, need: &str, m: &Manifest) -> anyhow::Result<String> {
    let text = read_main(paths);
    let needle = format!("→ {} ", m.id);
    let need_f = fold(need);
    let mut found = None;
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    for l in lines.iter_mut() {
        if l.starts_with("- ") && l.contains(&needle) {
            let before = l.split('→').next().unwrap_or("");
            if fold(before).contains(need_f.trim()) || need_f.trim().is_empty() {
                let n: u32 = l
                    .rsplit("réussi")
                    .next()
                    .and_then(|s| s.trim().trim_end_matches('×').trim().parse().ok())
                    .unwrap_or(1);
                let base = l.rsplit_once('·').map(|(a, _)| a.trim_end().to_string()).unwrap_or(l.clone());
                *l = format!("{base} · réussi {}×", n + 1);
                found = Some(l.clone());
                break;
            }
        }
    }
    if let Some(l) = found {
        let mut s = lines.join("\n");
        s.push('\n');
        std::fs::write(paths.main_memory(), s)?;
        return Ok(l);
    }
    let line = format!("- ({}) « {} » → {} ({}) · réussi 1×", today(), need.trim(), m.id, m.name);
    let new_text = insert_in_section(&text, "Agents de confiance", &line);
    std::fs::write(paths.main_memory(), new_text)?;
    Ok(line)
}

pub fn ensure_agent_memory(paths: &Paths, m: &Manifest) -> anyhow::Result<()> {
    let p = paths.agent_memory(&m.id);
    if p.exists() {
        return Ok(());
    }
    let content = format!(
        "# Mémoire de {}\n\n\
> Identifiant : `{}` · version {} · auteur : {}\n\
> Ce fichier est tenu par Orchestra. Il garde ce que l'agent a fait et appris.\n\n\
## Rôle\n- {}\n\n\
## Outils autorisés\n- {}\n\n\
## Missions\n\n\
## Apprentissages\n\n",
        m.name,
        m.id,
        m.version,
        if m.author.is_empty() { "inconnu" } else { &m.author },
        m.description,
        if m.tools.is_empty() { "aucun".to_string() } else { m.tools.join(", ") }
    );
    std::fs::write(p, content)?;
    Ok(())
}

pub fn agent_memory_text(paths: &Paths, id: &str) -> String {
    std::fs::read_to_string(paths.agent_memory(id)).unwrap_or_default()
}

pub fn append_agent_mission(paths: &Paths, m: &Manifest, mission: &str, outcome: &str, tools_used: &[String]) -> anyhow::Result<String> {
    let p = paths.agent_memory(&m.id);
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let one_line = |s: &str| s.replace('\n', " ").chars().take(220).collect::<String>();
    let line = format!(
        "- ({}) Mission : {} → {}{}",
        today(),
        one_line(mission),
        one_line(outcome),
        if tools_used.is_empty() { String::new() } else { format!(" · outils : {}", tools_used.join(", ")) }
    );
    let new_text = insert_in_section(&text, "Missions", &line);
    std::fs::write(p, new_text)?;
    Ok(line)
}

pub fn append_agent_learning(paths: &Paths, m: &Manifest, fact: &str) -> anyhow::Result<String> {
    let p = paths.agent_memory(&m.id);
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let line = format!("- ({}) {}", today(), fact.trim());
    let new_text = insert_in_section(&text, "Apprentissages", &line);
    std::fs::write(p, new_text)?;
    Ok(line)
}

pub fn archive(paths: &Paths, user: &str, answer: &str, intent: &str, agent: Option<&str>) {
    let p = paths.archives.join(format!("{}.md", today()));
    let mut text = std::fs::read_to_string(&p).unwrap_or_else(|_| format!("# Conversations du {}\n\n", today()));
    let hour = chrono::Local::now().format("%H:%M");
    text.push_str(&format!(
        "## {hour} · {intent}{}\n\n**Utilisateur :** {}\n\n**Orchestra :** {}\n\n---\n\n",
        agent.map(|a| format!(" · via {a}")).unwrap_or_default(),
        user.trim(),
        answer.trim()
    ));
    let _ = std::fs::write(p, text);
}

#[derive(serde::Serialize)]
pub struct MemoryFile {
    pub name: String,
    pub rel: String,
    pub kind: String,
    pub size: u64,
    pub modified: String,
}

pub fn tree(paths: &Paths) -> Vec<MemoryFile> {
    let mut out = vec![];
    let mut push = |p: &std::path::Path, kind: &str| {
        if let Ok(md) = p.metadata() {
            let modified = md
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%d/%m/%Y %H:%M").to_string())
                .unwrap_or_default();
            let rel = p.strip_prefix(&paths.root).unwrap_or(p).to_string_lossy().replace('\\', "/");
            out.push(MemoryFile {
                name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                rel,
                kind: kind.to_string(),
                size: md.len(),
                modified,
            });
        }
    };
    push(&paths.main_memory(), "principal");
    for (dir, kind) in [(&paths.memoire_agents, "agent"), (&paths.archives, "archive")] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "md").unwrap_or(false)).collect();
            entries.sort();
            if kind == "archive" {
                entries.reverse();
            }
            for p in entries {
                push(&p, kind);
            }
        }
    }
    out
}
