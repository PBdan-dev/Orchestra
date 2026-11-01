//! Mémoire d'Orchestra : des fichiers Markdown lisibles, rien de caché.
//!
//! - memoire/orchestra.md : faits durables, par section (## Utilisateur, ## Préférences…)
//! - memoire/agents/<id>.md : ce que chaque agent a fait et appris
//! - memoire/archives/AAAA-MM-JJ.md : les conversations brutes
//! - memoire/.index.bin : index du rappel (vecteurs et usage), supprimable (lot M2, `index.rs`)
//!
//! Le rappel cherche par les mots (BM25, explicable) et par le sens (vecteurs du modèle
//! d'embedding, D10), fusionnés par rang.

use crate::{agents::Manifest, events::Recalled, paths::Paths};

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

/// Rappel (lot M2) : index des mots (BM25) et du sens (vecteurs), fusionnés par rang, hors des
/// faits déjà dans le profil du prompt. `delai_sens` borne le calcul du vecteur de la question
/// (chat : quelques secondes ; voix : quelques dizaines de millisecondes, choix A1).
pub async fn recall(paths: &Paths, query: &str, max: usize, delai_sens: std::time::Duration) -> Vec<Recalled> {
    let (profil, _) = profil(paths, crate::orchestrator::PROFIL_MAX_CAR);
    crate::index::rappeler(paths, query, max, &profil, delai_sens).await
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
    // `p` : nom en clair ; le fichier peut être compressé à J+1 (lot M1), la lecture est transparente
    let mut push = |p: &std::path::Path, kind: &str| {
        if let Ok(md) = crate::compression::chemin_reel(p).metadata() {
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
    // lot M3 : faits remplacés, puis résumés (du jour, de conversation), les plus récents d'abord
    push(&historique(paths), "historique");
    let (resumes, sessions) = (paths.memoire.join("resumes"), paths.memoire.join("sessions"));
    for (dir, kind) in [(&paths.memoire_agents, "agent"), (&resumes, "resume"), (&sessions, "resume"), (&paths.archives, "archive")] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut entries: Vec<_> = rd
                .flatten()
                .filter_map(|e| {
                    let nom = e.file_name().to_string_lossy().to_string();
                    let clair = nom.strip_suffix(".zst").unwrap_or(&nom).to_string();
                    clair.ends_with(".md").then(|| dir.join(clair))
                })
                .collect();
            entries.sort();
            entries.dedup();
            if kind == "archive" || kind == "resume" {
                entries.reverse();
            }
            for p in entries {
                push(&p, kind);
            }
        }
    }
    out
}

/// Retire un agent des « Agents de confiance » (lot T5) ; renvoie les lignes retirées.
pub fn retirer_confiance(paths: &Paths, agent_id: &str) -> anyhow::Result<Vec<String>> {
    let text = read_main(paths);
    let needle = format!("→ {agent_id} ");
    let mut section = String::new();
    let mut retirees = vec![];
    let mut garde = vec![];
    for l in text.lines() {
        if let Some(t) = l.strip_prefix("## ") {
            section = t.trim().to_string();
        }
        if section == "Agents de confiance" && l.starts_with("- ") && l.contains(&needle) {
            retirees.push(l.to_string());
        } else {
            garde.push(l);
        }
    }
    if !retirees.is_empty() {
        let mut s = garde.join("\n");
        s.push('\n');
        std::fs::write(paths.main_memory(), s)?;
    }
    Ok(retirees)
}

// ---------- lot M3 : propreté ----------

/// `memoire/historique.md` : faits remplacés par la consolidation, avec leur date.
pub fn historique(paths: &Paths) -> std::path::PathBuf {
    paths.memoire.join("historique.md")
}

fn noter_historique(paths: &Paths, ligne: &str) -> anyhow::Result<()> {
    let p = historique(paths);
    let mut t = std::fs::read_to_string(&p).unwrap_or_else(|_| {
        "# Historique de la mémoire\n\n> Faits remplacés par la consolidation (lot M3), avec la date du remplacement. Rien n'est\n> effacé : un remplacement se défait depuis Conscience (Annuler) ou à la main dans orchestra.md.\n\n".to_string()
    });
    if !t.ends_with('\n') {
        t.push('\n');
    }
    t.push_str(ligne.trim_end());
    t.push('\n');
    std::fs::write(p, t)?;
    Ok(())
}

/// Remplace la première ligne égale à `ancienne` (espaces de bord ignorés) ; `None` si absente.
fn remplacer_ligne(texte: &str, ancienne: &str, nouvelle: &str) -> Option<String> {
    let mut trouve = false;
    let lignes: Vec<String> = texte
        .lines()
        .map(|l| {
            if !trouve && l.trim() == ancienne.trim() {
                trouve = true;
                nouvelle.to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    trouve.then(|| {
        let mut s = lignes.join("\n");
        s.push('\n');
        s
    })
}

/// Remplace une ligne de `orchestra.md` par un fait plus récent (consolidation, lot M3) ; l'ancienne
/// ligne part dans `historique.md`. Renvoie la nouvelle ligne.
pub fn remplacer_fait(paths: &Paths, ancienne: &str, fait: &str) -> anyhow::Result<String> {
    let nouvelle = format!("- ({}) {}", today(), fait.trim());
    let texte = remplacer_ligne(&read_main(paths), ancienne, &nouvelle).ok_or_else(|| anyhow::anyhow!("ligne introuvable dans orchestra.md : {}", ancienne.trim()))?;
    std::fs::write(paths.main_memory(), texte)?;
    noter_historique(paths, &format!("- ({}) Remplacé : « {} » par « {} »", today(), ancienne.trim().trim_start_matches("- "), fait.trim()))?;
    Ok(nouvelle)
}

/// Bouton Annuler de Conscience : l'ancienne ligne reprend la place de la nouvelle, et l'annulation
/// est notée dans `historique.md`.
pub fn annuler_remplacement(paths: &Paths, ancienne: &str, nouvelle: &str) -> anyhow::Result<()> {
    let texte = remplacer_ligne(&read_main(paths), nouvelle, ancienne)
        .ok_or_else(|| anyhow::anyhow!("la ligne « {} » a changé depuis : rétablis l'ancienne à la main (elle est dans historique.md)", nouvelle.trim().trim_start_matches("- ")))?;
    std::fs::write(paths.main_memory(), texte)?;
    noter_historique(paths, &format!("- ({}) Annulé par PB : « {} » rétabli à la place de « {} »", today(), ancienne.trim().trim_start_matches("- "), nouvelle.trim().trim_start_matches("- ")))?;
    Ok(())
}

/// Plafond des missions gardées dans la fiche d'un agent, et nombre condensé d'un coup (lot M3).
pub const MISSIONS_MAX: usize = 50;
const MISSIONS_A_CONDENSER: usize = 25;

fn lignes_de_section(texte: &str, section: &str) -> Vec<String> {
    let mut dedans = false;
    let mut out = vec![];
    for l in texte.lines() {
        if let Some(h) = l.trim().strip_prefix("## ") {
            dedans = h.trim() == section;
        } else if dedans && l.trim().starts_with("- ") {
            out.push(l.trim().to_string());
        }
    }
    out
}

/// Missions d'un agent au-delà de 50 lignes : les 25 plus anciennes, à condenser (lot M3).
pub fn missions_a_condenser(paths: &Paths, agent_id: &str) -> Option<Vec<String>> {
    let missions = lignes_de_section(&agent_memory_text(paths, agent_id), "Missions");
    (missions.len() > MISSIONS_MAX).then(|| missions.into_iter().take(MISSIONS_A_CONDENSER).collect())
}

/// Retire les missions condensées de la fiche de l'agent, les range au niveau froid
/// (`memoire/archives/agents/<id>_<date>.md`) et ajoute les apprentissages tirés d'elles.
/// Renvoie le fichier froid.
pub fn condenser_missions(paths: &Paths, agent_id: &str, retirees: &[String], apprentissages: &[String]) -> anyhow::Result<std::path::PathBuf> {
    let dossier = paths.archives.join("agents");
    std::fs::create_dir_all(&dossier)?;
    let froid = dossier.join(format!("{}_{}.md", crate::paths::safe_name(agent_id), today()));
    let mut f = std::fs::read_to_string(&froid).unwrap_or_else(|_| format!("# Missions condensées de {agent_id}\n\n> Lignes complètes retirées de memoire/agents/{agent_id}.md (plus de {MISSIONS_MAX} missions), lot M3.\n\n"));
    for l in retirees {
        f.push_str(l.trim());
        f.push('\n');
    }
    std::fs::write(&froid, f)?;
    // fiche : lignes retirées (une fois chacune), puis les apprentissages
    let fiche = paths.agent_memory(agent_id);
    let contenu = std::fs::read_to_string(&fiche)?;
    let mut restantes: Vec<&str> = retirees.iter().map(|r| r.trim()).collect();
    let garde: Vec<&str> = contenu
        .lines()
        .filter(|l| match restantes.iter().position(|r| *r == l.trim()) {
            Some(i) => {
                restantes.remove(i);
                false
            }
            None => true,
        })
        .collect();
    let mut texte = garde.join("\n");
    texte.push('\n');
    let date = |l: &String| l.trim_start_matches("- (").get(..10).unwrap_or("").to_string();
    let periode = match (retirees.first(), retirees.last()) {
        (Some(a), Some(b)) => format!("du {} au {}", date(a), date(b)),
        _ => String::new(),
    };
    for a in apprentissages.iter().filter(|a| !a.trim().is_empty()) {
        texte = insert_in_section(&texte, "Apprentissages", &format!("- ({}) {} [tiré de {} missions condensées, {periode}]", today(), a.trim(), retirees.len()));
    }
    std::fs::write(&fiche, texte)?;
    Ok(froid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier() -> (Paths, std::path::PathBuf) {
        let racine = std::env::temp_dir().join(format!("orchestra-m3-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let p = Paths::at(racine.clone()).unwrap();
        std::fs::write(
            p.main_memory(),
            "# Mémoire\n\n## Utilisateur\n- (2026-10-26) PB habite à Annecy.\n- (2026-10-27) PB est allergique aux arachides.\n\n## Connaissances\n- (2026-10-25) Ollama tourne sur le port 11434.\n",
        )
        .unwrap();
        (p, racine)
    }

    #[test]
    fn fait_remplace_garde_sa_place_et_part_dans_l_historique() {
        let (p, racine) = dossier();
        let nouvelle = remplacer_fait(&p, "- (2026-10-26) PB habite à Annecy.", "PB habite maintenant à Lyon.").unwrap();
        assert_eq!(nouvelle, format!("- ({}) PB habite maintenant à Lyon.", today()));
        let m = read_main(&p);
        assert!(!m.contains("Annecy"), "{m}");
        // même place : avant l'allergie, dans la section Utilisateur
        assert!(m.find("Lyon").unwrap() < m.find("arachides").unwrap(), "{m}");
        let h = std::fs::read_to_string(historique(&p)).unwrap();
        assert!(h.contains("(2026-10-26) PB habite à Annecy.") && h.contains("PB habite maintenant à Lyon.") && h.contains(&today()), "{h}");
        // ligne absente : rien n'est touché
        assert!(remplacer_fait(&p, "- (2026-01-01) Inconnue.", "x").is_err());
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn annuler_un_remplacement() {
        let (p, racine) = dossier();
        let nouvelle = remplacer_fait(&p, "- (2026-10-26) PB habite à Annecy.", "PB habite maintenant à Lyon.").unwrap();
        annuler_remplacement(&p, "- (2026-10-26) PB habite à Annecy.", &nouvelle).unwrap();
        let m = read_main(&p);
        assert!(m.contains("- (2026-10-26) PB habite à Annecy.") && !m.contains("Lyon"), "{m}");
        let h = std::fs::read_to_string(historique(&p)).unwrap();
        assert!(h.lines().any(|l| l.contains("Annulé") && l.contains("Annecy")), "{h}");
        // la nouvelle ligne a été modifiée à la main depuis : on ne devine pas
        assert!(annuler_remplacement(&p, "- (2026-10-26) PB habite à Annecy.", &nouvelle).is_err());
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn archive_compressee_listee_sous_son_nom() {
        let (p, racine) = dossier();
        std::fs::write(p.archives.join("2026-10-29.md"), "# Conversations du 2026-10-29\n").unwrap();
        std::fs::write(p.archives.join("2026-10-30.md"), "# Conversations du 2026-10-30\n").unwrap();
        crate::compression::compresser(&p, &p.archives.join("2026-10-29.md")).unwrap();
        let archives: Vec<(String, String)> = tree(&p).into_iter().filter(|f| f.kind == "archive").map(|f| (f.name, f.rel)).collect();
        // l'interface lit et écrit par le nom en clair ; la lecture est transparente (lot M1)
        assert_eq!(
            archives,
            vec![("2026-10-30.md".to_string(), "memoire/archives/2026-10-30.md".to_string()), ("2026-10-29.md".to_string(), "memoire/archives/2026-10-29.md".to_string())]
        );
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn missions_au_dela_de_50_condensees() {
        let (p, racine) = dossier();
        let m = Manifest { id: "agent-x".into(), name: "X".into(), ..Default::default() };
        ensure_agent_memory(&p, &m).unwrap();
        for i in 0..50 {
            append_agent_mission(&p, &m, &format!("mission {i}"), "termine", &[]).unwrap();
        }
        assert!(missions_a_condenser(&p, "agent-x").is_none(), "50 lignes : sous le plafond");
        append_agent_mission(&p, &m, "mission 50", "termine", &[]).unwrap();
        let vieilles = missions_a_condenser(&p, "agent-x").expect("51 lignes : au-delà du plafond");
        assert_eq!(vieilles.len(), 25);
        assert!(vieilles[0].contains("mission 0 ") && vieilles[24].contains("mission 24 "), "{vieilles:?}");
        let froid = condenser_missions(&p, "agent-x", &vieilles, &["Les missions de tri réussissent quand la boîte est indiquée.".into()]).unwrap();
        let fiche = agent_memory_text(&p, "agent-x");
        assert!(!fiche.contains("mission 0 ") && fiche.contains("mission 25 ") && fiche.contains("mission 50 "), "{fiche}");
        assert!(fiche.contains("Les missions de tri réussissent") && fiche.contains("25 missions"), "{fiche}");
        let f = std::fs::read_to_string(&froid).unwrap();
        assert!(froid.starts_with(p.archives.join("agents")) && f.contains("mission 0 ") && f.contains("mission 24 "), "{f}");
        assert!(missions_a_condenser(&p, "agent-x").is_none());
        let _ = std::fs::remove_dir_all(racine);
    }
}
