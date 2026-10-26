//! Le chef d'orchestre. Une demande traverse ces étapes, chacune visible dans l'interface :
//! rappel de mémoire → analyse → (recherche d'agent → délégation → travail) → réponse → mémorisation.

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::{
    agents::{self, Manifest},
    brain::{Brain, Chunk, Msg},
    events::{emit, Event},
    market, memory, tools, AppState,
};

const ORCHESTRA_IDENTITY: &str = "Tu es Orchestra, un orchestrateur d'agents qui tourne en local sur l'ordinateur de l'utilisateur. \
Tu es sobre, précis et chaleureux. Tu réponds en français. \
Tu ne fais pas les tâches toi-même : tu discutes, tu réponds aux questions avec tes connaissances, et quand une demande exige du travail concret \
(créer, rédiger, modifier, organiser, envoyer, traiter des fichiers, agir sur un service…), tu la délègues à un agent spécialisé que tu supervises. \
Tu es transparent : tu dis ce que tu fais et pourquoi.";

struct Analysis {
    intent: String,
    needs_agent: bool,
    need: String,
    mission: String,
}

pub async fn run(app: AppHandle, run_id: String, text: String, history: Vec<Msg>) {
    if let Err(e) = run_inner(&app, &run_id, &text, history).await {
        emit(&app, Event::Error { run_id: run_id.clone(), message: e.to_string() });
        phase(&app, &run_id, "idle", "");
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.busy.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn phase(app: &AppHandle, run_id: &str, p: &str, label: &str) {
    emit(app, Event::Phase { run_id: run_id.into(), phase: p.into(), label: label.into() });
}

fn thought(app: &AppHandle, run_id: &str, s: impl Into<String>) {
    emit(app, Event::Thought { run_id: run_id.into(), text: s.into() });
}

async fn run_inner(app: &AppHandle, run_id: &str, text: &str, history: Vec<Msg>) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let paths = state.paths.clone();
    let http = state.http.clone();
    let mut brain = Brain::new(cfg.clone(), http.clone());

    // 0. Cerveau
    phase(app, run_id, "thinking", "Connexion au cerveau");
    let info = brain.detect().await;
    if !info.online || brain.model.is_empty() {
        anyhow::bail!("Cerveau indisponible : {}", info.detail);
    }
    if cfg.provider == "ollama" && !info.loaded {
        phase(app, run_id, "loading", &format!("Je charge {}", brain.model));
        thought(app, run_id, format!("⏻ {} n'est pas en mémoire : je le charge dans Ollama…\n", brain.model));
        brain.load_ollama().await?;
    }
    thought(app, run_id, format!("⚡ Cerveau : {}\n", brain.label()));

    // 1. Rappel de mémoire
    phase(app, run_id, "recalling", "Je consulte ma mémoire");
    let recalled = memory::recall(&paths, Some(&brain), text, 6).await;
    emit(app, Event::Recall { run_id: run_id.into(), items: recalled.clone() });
    let memory_block = if recalled.is_empty() {
        "Aucun souvenir pertinent.".to_string()
    } else {
        recalled.iter().map(|r| format!("- [{} › {}] {}", r.file, r.section, r.text)).collect::<Vec<_>>().join("\n")
    };

    // 2. Analyse
    phase(app, run_id, "thinking", "J'analyse la demande");
    let analysis = analyse(app, run_id, &brain, text, &history, &memory_block).await?;
    thought(
        app,
        run_id,
        format!(
            "\n→ Intention : {} · agent nécessaire : {}{}\n",
            analysis.intent,
            if analysis.needs_agent { "oui" } else { "non" },
            if analysis.needs_agent { format!(" · besoin : {}", analysis.need) } else { String::new() }
        ),
    );

    let answer: String;
    let mut used_agent: Option<Manifest> = None;

    if analysis.needs_agent && analysis.intent == "travail" {
        // 3. Recherche d'agent
        phase(app, run_id, "searching_agent", "Je cherche l'agent qu'il faut");
        let need = if analysis.need.is_empty() { text.to_string() } else { analysis.need.clone() };
        let mut chosen: Option<(Manifest, String, String)> = None;

        // 3a. mémoire : un agent de confiance pour ce besoin ?
        let need_words = memory::keywords(&need);
        for t in memory::trusted_agents(&paths) {
            let tw = memory::keywords(&t.need);
            let common = tw.iter().filter(|w| need_words.contains(w)).count();
            if common > 0 {
                if let Some(m) = agents::get_installed(&paths, &t.agent_id) {
                    thought(app, run_id, format!("🧠 Ma mémoire dit : « {} » → {} (réussi {}×). Je le réutilise.\n", t.need, m.name, t.successes));
                    chosen = Some((m, "mémoire".into(), format!("agent de confiance pour « {} »", t.need)));
                    break;
                }
            }
        }

        // 3b. sinon : installés → Agent Market → registre local
        if chosen.is_none() {
            let (found, log) = market::find_agent(&http, &cfg, &paths, &need).await;
            for l in &log {
                thought(app, run_id, format!("🔎 {l}\n"));
            }
            if let Some(f) = found {
                let reason = format!("meilleur score ({:.1}) pour « {} »", f.score, need);
                chosen = Some((f.manifest, f.source, reason));
            }
        }

        match chosen {
            None => {
                thought(app, run_id, "✗ Aucun agent ne correspond. Je ne fais pas le travail moi-même : je l'explique.\n");
                phase(app, run_id, "speaking", "Je réponds");
                answer = speak(
                    app,
                    run_id,
                    &brain,
                    &format!(
                        "{ORCHESTRA_IDENTITY}\n\nSituation : l'utilisateur demande un travail (« {need} ») mais aucun agent disponible ne sait le faire \
(ni installé, ni sur l'Agent Market, ni dans le registre local). Tu ne dois PAS faire le travail toi-même. \
Explique-le en 3 à 5 phrases, sans t'excuser à répétition : dis quel type d'agent il faudrait, et qu'il pourra être ajouté à l'Agent Market ou au registre local (dossier Orchestra › agents/registre-local.json). \
Si une partie de la demande relève de la simple discussion, tu peux y répondre."
                    ),
                    &history,
                    text,
                    &memory_block,
                )
                .await?;
            }
            Some((m, source, reason)) => {
                if agents::get_installed(&paths, &m.id).is_none() {
                    agents::install(&paths, &m)?;
                    thought(app, run_id, format!("⬇ {} installé depuis {} (manifeste enregistré dans agents/installes).\n", m.name, source));
                }
                memory::ensure_agent_memory(&paths, &m)?;
                emit(app, Event::AgentAttached { run_id: run_id.into(), agent: m.clone(), source: source.clone(), reason });

                // 4. Délégation
                phase(app, run_id, "delegating", &format!("Je confie la mission à {}", m.name));
                let mission = if analysis.mission.trim().is_empty() { text.to_string() } else { analysis.mission.clone() };
                emit(app, Event::AgentStatus { run_id: run_id.into(), agent_id: m.id.clone(), status: "running".into(), detail: mission.clone() });
                phase(app, run_id, "working", &format!("{} travaille", m.name));

                let result = run_agent(app, run_id, &brain, &cfg, &paths, &http, &m, &mission, text).await;
                match result {
                    Ok((outcome, tools_used)) => {
                        emit(app, Event::AgentStatus { run_id: run_id.into(), agent_id: m.id.clone(), status: "done".into(), detail: short(&outcome, 160) });
                        let line = memory::append_agent_mission(&paths, &m, &mission, &outcome, &tools_used)?;
                        emit(app, Event::Memorized { run_id: run_id.into(), file: format!("agents/{}.md", m.id), section: "Missions".into(), line });
                        let trust_line = memory::record_trust(&paths, &need, &m)?;
                        emit(app, Event::Memorized { run_id: run_id.into(), file: "orchestra.md".into(), section: "Agents de confiance".into(), line: trust_line });

                        // 5. Restitution supervisée
                        phase(app, run_id, "speaking", "Je restitue le résultat");
                        answer = speak(
                            app,
                            run_id,
                            &brain,
                            &format!(
                                "{ORCHESTRA_IDENTITY}\n\nTu as délégué la mission suivante à l'agent « {} » : {}\n\nRésultat rendu par l'agent :\n\"\"\"\n{}\n\"\"\"\n\n\
Restitue ce résultat à l'utilisateur de façon claire : commence par une phrase qui dit ce que l'agent a fait, puis donne le résultat utile (texte produit, fichiers créés dans l'espace de travail d'Orchestra, etc.). \
N'invente rien qui ne soit pas dans le résultat. Si l'agent a échoué ou n'a pas tout fait, dis-le simplement.",
                                m.name, mission, outcome
                            ),
                            &history,
                            text,
                            &memory_block,
                        )
                        .await?;
                        used_agent = Some(m);
                    }
                    Err(e) => {
                        emit(app, Event::AgentStatus { run_id: run_id.into(), agent_id: m.id.clone(), status: "error".into(), detail: e.to_string() });
                        let line = memory::append_agent_mission(&paths, &m, &mission, &format!("ÉCHEC : {e}"), &[])?;
                        emit(app, Event::Memorized { run_id: run_id.into(), file: format!("agents/{}.md", m.id), section: "Missions".into(), line });
                        phase(app, run_id, "speaking", "Je réponds");
                        answer = speak(
                            app,
                            run_id,
                            &brain,
                            &format!("{ORCHESTRA_IDENTITY}\n\nL'agent « {} » a échoué sur la mission « {} » avec l'erreur : {e}. Explique-le brièvement à l'utilisateur et propose une piste.", m.name, mission),
                            &history,
                            text,
                            &memory_block,
                        )
                        .await?;
                        used_agent = Some(m);
                    }
                }
            }
        }
    } else {
        // Discussion ou recherche : Orchestra répond lui-même
        phase(app, run_id, "speaking", "Je réponds");
        answer = speak(
            app,
            run_id,
            &brain,
            &format!("{ORCHESTRA_IDENTITY}\n\nRéponds directement à l'utilisateur. Sois clair et concis ; développe seulement si la question le demande. Utilise les souvenirs ci-dessous quand ils sont pertinents, sans les réciter."),
            &history,
            text,
            &memory_block,
        )
        .await?;
    }

    // 6. Mémorisation
    phase(app, run_id, "memorizing", "Je retiens l'essentiel");
    memorize(app, run_id, &brain, &paths, text, &answer, used_agent.as_ref()).await;
    memory::archive(&paths, text, &answer, &analysis.intent, used_agent.as_ref().map(|m| m.name.as_str()));

    emit(app, Event::Done { run_id: run_id.into(), answer, intent: analysis.intent });
    phase(app, run_id, "idle", "");
    Ok(())
}

fn short(s: &str, n: usize) -> String {
    let one: String = s.replace('\n', " ");
    if one.chars().count() <= n { one } else { format!("{}…", one.chars().take(n).collect::<String>()) }
}

fn recent_history(history: &[Msg], n: usize) -> Vec<Msg> {
    let start = history.len().saturating_sub(n);
    history[start..].iter().map(|m| Msg { role: m.role.clone(), content: m.content.chars().take(2000).collect() }).collect()
}

async fn analyse(app: &AppHandle, run_id: &str, brain: &Brain, text: &str, history: &[Msg], memory_block: &str) -> anyhow::Result<Analysis> {
    let mut msgs = vec![Msg::system(format!(
        "{ORCHESTRA_IDENTITY}\n\nTa tâche ici : analyser la dernière demande de l'utilisateur. \
Réfléchis à voix haute en 2 à 4 phrases courtes (en français), puis termine OBLIGATOIREMENT par une dernière ligne qui commence par `JSON:` suivie d'un objet JSON sur une seule ligne :\n\
{{\"intent\": \"discussion\" | \"recherche\" | \"travail\", \"needs_agent\": true|false, \"need\": \"besoin en 2 à 5 mots (ex: rédiger un mail)\", \"mission\": \"consigne précise et complète pour l'agent, vide si pas d'agent\"}}\n\n\
Règles : « discussion » = échange, salutations, avis, questions sur toi. « recherche » = question de connaissance à laquelle tu réponds toi-même. \
« travail » = action concrète à produire (rédiger, créer un fichier, organiser, lister des fichiers, résumer une page web, envoyer, etc.) → needs_agent = true. \
Si la demande mélange les deux, privilégie « travail » si une action concrète est attendue.\n\nSouvenirs disponibles :\n{memory_block}"
    ))];
    msgs.extend(recent_history(history, 6));
    msgs.push(Msg::user(text));

    let full = brain
        .stream(&msgs, |c: Chunk| {
            if !c.thinking.is_empty() { thought(app, run_id, c.thinking); }
            if !c.content.is_empty() { thought(app, run_id, c.content); }
        })
        .await?;

    let json_line = full.rsplit("JSON:").next().filter(|_| full.contains("JSON:")).map(|s| s.trim().to_string());
    let parsed: Option<Value> = json_line.as_deref().and_then(extract_json);
    let v = parsed.unwrap_or_else(|| serde_json::json!({}));
    let intent = v["intent"].as_str().unwrap_or("discussion").to_lowercase();
    let intent = if ["discussion", "recherche", "travail"].contains(&intent.as_str()) { intent } else { "discussion".into() };
    let needs_agent = v["needs_agent"].as_bool().unwrap_or(intent == "travail");
    Ok(Analysis {
        intent,
        needs_agent,
        need: v["need"].as_str().unwrap_or("").trim().to_string(),
        mission: v["mission"].as_str().unwrap_or("").trim().to_string(),
    })
}

/// Extrait le premier objet JSON équilibré d'un texte.
fn extract_json(s: &str) -> Option<Value> {
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in s[start..].char_indices() {
        if in_str {
            if esc { esc = false; } else if c == '\\' { esc = true; } else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&s[start..start + i + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

async fn speak(app: &AppHandle, run_id: &str, brain: &Brain, system: &str, history: &[Msg], text: &str, memory_block: &str) -> anyhow::Result<String> {
    let mut msgs = vec![Msg::system(format!("{system}\n\nSouvenirs pertinents :\n{memory_block}"))];
    msgs.extend(recent_history(history, 10));
    msgs.push(Msg::user(text));
    let full = brain
        .stream(&msgs, |c: Chunk| {
            if !c.thinking.is_empty() { thought(app, run_id, c.thinking); }
            if !c.content.is_empty() { emit(app, Event::Token { run_id: run_id.into(), text: c.content }); }
        })
        .await?;
    Ok(full)
}

/// Boucle d'exécution d'un agent : le modèle agit par outils, un à la fois.
async fn run_agent(
    app: &AppHandle,
    run_id: &str,
    brain: &Brain,
    cfg: &crate::config::Config,
    paths: &crate::paths::Paths,
    http: &reqwest::Client,
    m: &Manifest,
    mission: &str,
    original: &str,
) -> anyhow::Result<(String, Vec<String>)> {
    let agent_brain;
    let b: &Brain = if m.model.is_empty() || m.model == "inherit" {
        brain
    } else {
        // "provider:modèle" → cerveau dédié
        let (prov, model) = m.model.split_once(':').unwrap_or(("ollama", &m.model));
        let mut c = cfg.clone();
        c.provider = prov.to_string();
        c.model = model.to_string();
        agent_brain = Brain::new(c, http.clone());
        &agent_brain
    };

    let tool_doc = tools::describe(&m.tools);
    let mem = memory::agent_memory_text(paths, &m.id);
    let mem_tail: String = mem.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    let system = format!(
        "{}\n\nTu es activé par Orchestra, qui supervise ton travail. Tu réponds en français.\n\n\
Outils disponibles :\n{}\n\n\
Pour utiliser un outil, réponds UNIQUEMENT avec ce bloc (un seul outil par réponse) :\n```tool\n{{\"tool\": \"nom_outil\", \"input\": {{...}}}}\n```\n\
Tu recevras le résultat, puis tu continueras. Quand la mission est accomplie, réponds SANS bloc tool avec ton rapport final complet (le texte produit, les fichiers créés, ce qui n'a pas pu être fait).\n\n\
Extrait de ta mémoire :\n{}",
        m.system_prompt,
        if tool_doc.is_empty() { "(aucun)".to_string() } else { tool_doc },
        if mem_tail.trim().is_empty() { "(vide)".to_string() } else { mem_tail }
    );
    let mut msgs = vec![Msg::system(system), Msg::user(format!("Mission confiée par Orchestra : {mission}\n\nDemande originale de l'utilisateur : « {original} »"))];
    let mut tools_used: Vec<String> = vec![];
    let prefix = format!("[{}] ", m.name);

    for step in 0..cfg.max_agent_steps.max(1) {
        let content = b
            .stream(&msgs, |c: Chunk| {
                if !c.thinking.is_empty() { thought(app, run_id, format!("{prefix}{}", c.thinking)); }
                if !c.content.is_empty() { thought(app, run_id, c.content); }
            })
            .await?;
        thought(app, run_id, "\n");

        match extract_tool_call(&content) {
            None => return Ok((content, tools_used)),
            Some((tool, input)) => {
                if !m.tools.iter().any(|t| t == &tool) {
                    let out = format!("outil « {tool} » non autorisé pour cet agent");
                    emit(app, Event::ToolCall { run_id: run_id.into(), agent_id: m.id.clone(), tool: tool.clone(), input: input.clone(), output: out.clone() });
                    msgs.push(Msg::assistant(content));
                    msgs.push(Msg::user(format!("Résultat de l'outil {tool} : {out}")));
                    continue;
                }
                let out = match tools::execute(&tool, &input, paths, http).await {
                    Ok(o) => o,
                    Err(e) => format!("ERREUR : {e}"),
                };
                emit(app, Event::ToolCall { run_id: run_id.into(), agent_id: m.id.clone(), tool: tool.clone(), input: input.clone(), output: out.clone() });
                if !tools_used.contains(&tool) {
                    tools_used.push(tool.clone());
                }
                msgs.push(Msg::assistant(content));
                let remaining = cfg.max_agent_steps.saturating_sub(step + 1);
                msgs.push(Msg::user(format!(
                    "Résultat de l'outil {tool} :\n{out}\n\n(Il te reste {remaining} action(s) outil. Si la mission est terminée, donne ton rapport final sans bloc tool.)"
                )));
            }
        }
    }
    // dernier tour : rapport forcé
    msgs.push(Msg::user("Limite d'actions atteinte. Donne maintenant ton rapport final, sans bloc tool."));
    let content = b.stream(&msgs, |c: Chunk| {
        if !c.content.is_empty() { thought(app, run_id, c.content); }
    }).await?;
    Ok((content, tools_used))
}

fn extract_tool_call(content: &str) -> Option<(String, Value)> {
    // bloc ```tool … ``` ou ```json … ``` contenant "tool"
    let mut candidates: Vec<&str> = vec![];
    let mut rest = content;
    while let Some(i) = rest.find("```") {
        let after = &rest[i + 3..];
        let Some(j) = after.find("```") else { break };
        let block = &after[..j];
        let body = block.splitn(2, '\n').nth(1).unwrap_or(block);
        candidates.push(body);
        rest = &after[j + 3..];
    }
    if candidates.is_empty() {
        candidates.push(content);
    }
    for c in candidates {
        if let Some(v) = extract_json(c) {
            if let Some(tool) = v["tool"].as_str() {
                let input = v.get("input").cloned().unwrap_or(Value::Object(Default::default()));
                return Some((tool.to_string(), input));
            }
        }
    }
    None
}

async fn memorize(app: &AppHandle, run_id: &str, brain: &Brain, paths: &crate::paths::Paths, text: &str, answer: &str, agent: Option<&Manifest>) {
    let prompt = format!(
        "Tu es la mémoire d'Orchestra. Voici un échange. Extrais 0 à 3 faits DURABLES et utiles pour les prochaines conversations : \
qui est l'utilisateur, ce qu'il aime ou n'aime pas, ses projets, des informations stables. \
Ignore ce qui est éphémère, les simples questions, les salutations, et ne reformule pas la réponse d'Orchestra. \
Réponds UNIQUEMENT avec un tableau JSON : [{{\"section\": \"Utilisateur\" | \"Préférences\" | \"Projets\" | \"Connaissances\", \"fact\": \"une phrase courte\"}}] ou [] s'il n'y a rien à retenir.\n\n\
Utilisateur : {}\n\nOrchestra : {}",
        text.chars().take(4000).collect::<String>(),
        answer.chars().take(2000).collect::<String>()
    );
    let res = brain.complete(&[Msg::user(prompt)]).await.unwrap_or_default();
    let arr = res.find('[').and_then(|i| {
        let tail = &res[i..];
        let end = tail.rfind(']')?;
        serde_json::from_str::<Value>(&tail[..=end]).ok()
    });
    let mut count = 0;
    if let Some(Value::Array(items)) = arr {
        for it in items.iter().take(3) {
            let section = it["section"].as_str().unwrap_or("Connaissances").trim();
            let section = match section { "Utilisateur" | "Préférences" | "Projets" | "Connaissances" => section, _ => "Connaissances" };
            let fact = it["fact"].as_str().unwrap_or("").trim();
            if fact.len() < 4 { continue; }
            // évite les doublons exacts
            if memory::read_main(paths).contains(fact) { continue; }
            if let Ok(line) = memory::append_fact(paths, section, fact) {
                count += 1;
                emit(app, Event::Memorized { run_id: run_id.into(), file: "orchestra.md".into(), section: section.into(), line });
            }
        }
    }
    if let Some(m) = agent {
        // un apprentissage côté agent si le modèle en a extrait un
        if let Some(Value::Array(items)) = res.find('[').and_then(|i| serde_json::from_str::<Value>(&res[i..]).ok()) {
            if let Some(first) = items.first().and_then(|f| f["fact"].as_str()) {
                if first.len() > 4 {
                    if let Ok(line) = memory::append_agent_learning(paths, m, &format!("Contexte utilisateur : {first}")) {
                        emit(app, Event::Memorized { run_id: run_id.into(), file: format!("agents/{}.md", m.id), section: "Apprentissages".into(), line });
                    }
                }
            }
        }
    }
    if count == 0 {
        emit(app, Event::NothingToMemorize { run_id: run_id.into() });
    }
}
