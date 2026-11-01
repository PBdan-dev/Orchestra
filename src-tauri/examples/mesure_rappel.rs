//! Mesure du rappel (lot M2) : qualité et temps, sans modèle de chat.
//!
//! - Qualité : pour chaque question de `eval/rappel.jsonl`, le fait attendu figure-t-il dans les
//!   6 souvenirs donnés au modèle ? Trois calculs : l'ancien (mots communs, avant le lot M2), les
//!   mots seuls (BM25) et les mots avec le sens (si le modèle d'embedding est installé dans Ollama).
//! - Temps : rappel par les mots et par le sens, vecteur d'une question (budget de la voix : 35 ms),
//!   calcul des vecteurs ; sur la mémoire d'essai, puis sur une grande mémoire fabriquée.
//!
//! Lancement, depuis `src-tauri/` : `cargo run --release --example mesure_rappel`
//! Options : `--modele qwen3-embedding:0.6b`, `--grand 2000` (faits ajoutés), `--ollama <url>`.
//! Travaille dans un dossier temporaire, jamais dans le vrai ; rapport : `Orchestra/eval/rappel-<date>.md`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use orchestra_lib::{
    index::{self, Sens},
    memory,
    orchestrator::PROFIL_MAX_CAR,
    paths::Paths,
};
use serde_json::{json, Value};

const MAX: usize = 6;
const CONSIGNE_QUESTION: &str = "Instruct: Given a user question, retrieve the stored memory facts that help answer it\nQuery: ";

struct Question {
    id: String,
    question: String,
    fait: String,
    sorte: String,
}

fn lire_questions(f: &Path) -> Vec<Question> {
    std::fs::read_to_string(f)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|v| Question {
            id: v["id"].as_str().unwrap_or("").into(),
            question: v["question"].as_str().unwrap_or("").into(),
            fait: v["fait"].as_str().unwrap_or("").into(),
            sorte: v["type"].as_str().unwrap_or("").into(),
        })
        .collect()
}

/// Ancien rappel (avant le lot M2), recopié de `memory.rs` au commit 5eb5777 : mots communs et
/// radicaux de 5 lettres, sur tous les faits (profil compris), sans index.
fn ancien_rappel(paths: &Paths, requete: &str, max: usize) -> Vec<String> {
    let mut faits = memory::parse_facts("orchestra.md", &memory::read_main(paths));
    for e in std::fs::read_dir(&paths.memoire_agents).into_iter().flatten().flatten() {
        let nom = e.file_name().to_string_lossy().to_string();
        if nom.ends_with(".md") {
            if let Ok(s) = std::fs::read_to_string(e.path()) {
                faits.extend(memory::parse_facts(&format!("agents/{nom}"), &s));
            }
        }
    }
    let qw = memory::keywords(requete);
    let mut notes: Vec<(f32, String)> = faits
        .into_iter()
        .filter_map(|f| {
            let fw = memory::keywords(&f.text);
            let mut communs = 0;
            for q in &qw {
                if fw.iter().any(|x| x == q) {
                    communs += 1;
                } else if q.len() >= 5 {
                    let radical: String = q.chars().take(5).collect();
                    if fw.iter().any(|x| x.starts_with(&radical)) {
                        communs += 1;
                    }
                }
            }
            (communs > 0).then(|| (communs as f32 / (qw.len().max(1) as f32).sqrt(), f.text))
        })
        .collect();
    notes.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    notes.into_iter().take(max).map(|(_, t)| t).collect()
}

fn rang(textes: &[String], fait: &str) -> Option<usize> {
    textes.iter().position(|t| t.contains(fait)).map(|i| i + 1)
}

fn mediane_p95(mut v: Vec<f64>) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v[v.len() / 2];
    let p = v[((v.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(v.len() - 1)];
    (m, p)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Mémoire d'agent fabriquée : `n` comptes rendus de mission variés (mots tirés au sort, graine fixe).
fn grande_memoire(n: usize) -> String {
    let verbes = ["rédiger", "trier", "résumer", "classer", "traduire", "vérifier", "comparer", "préparer", "relire", "archiver", "chercher", "planifier"];
    let objets = ["le devis", "les factures", "le rapport annuel", "la présentation", "les mails du client", "le contrat", "le compte rendu", "la liste des tâches", "le budget", "les photos", "le planning", "la note de synthèse", "le cahier des charges", "les relevés bancaires", "le dossier de subvention"];
    let qui = ["Camille", "Julien", "l'équipe produit", "le comptable", "Marc", "Sophie", "le fournisseur", "la mairie", "Léa", "le service client"];
    let details = ["envoyé en pièce jointe", "trois points à revoir", "validé sans remarque", "version 2 déposée dans le projet", "en attente d'une réponse", "chiffres mis à jour", "format PDF demandé", "relance prévue lundi", "deux doublons retirés", "traduction en anglais jointe"];
    let mut graine: u64 = 0x5eed_2026;
    let mut tirer = |n: usize| {
        graine = graine.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((graine >> 33) as usize) % n
    };
    let mut s = String::from("# Mémoire de Synthétique\n\n## Missions\n");
    for i in 0..n {
        let (mois, jour) = (1 + i % 10, 1 + (i * 7) % 28);
        s.push_str(&format!(
            "- (2026-{mois:02}-{jour:02}) Mission : {} {} pour {} → termine — {} (n° {i})\n",
            verbes[tirer(verbes.len())],
            objets[tirer(objets.len())],
            qui[tirer(qui.len())],
            details[tirer(details.len())]
        ));
    }
    s
}

struct Qualite {
    rangs: Vec<Option<usize>>,
}

impl Qualite {
    fn resume(&self, qs: &[Question], sorte: Option<&str>) -> String {
        let choisis: Vec<&Option<usize>> = qs.iter().zip(&self.rangs).filter(|(q, _)| sorte.is_none_or(|s| q.sorte == s)).map(|(_, r)| r).collect();
        let dans6 = choisis.iter().filter(|r| r.is_some()).count();
        let dans3 = choisis.iter().filter(|r| r.is_some_and(|x| x <= 3)).count();
        format!("{dans6}/{} ({dans3} dans les 3 premiers)", choisis.len())
    }
}

async fn modele_installe(http: &reqwest::Client, ollama: &str, modele: &str) -> bool {
    let Ok(r) = http.get(format!("{ollama}/api/tags")).send().await else { return false };
    let Ok(v) = r.json::<Value>().await else { return false };
    v["models"].as_array().into_iter().flatten().any(|m| {
        let n = m["name"].as_str().unwrap_or("");
        n == modele || n == format!("{modele}:latest")
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let val = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let modele = val("--modele").unwrap_or_else(|| "qwen3-embedding:0.6b".into());
    let ollama = val("--ollama").unwrap_or_else(|| "http://127.0.0.1:11434".into()).trim_end_matches('/').to_string();
    let grand: usize = val("--grand").and_then(|v| v.parse().ok()).unwrap_or(2000);
    // --detail : souvenirs donnés pour chaque question ratée (mots + sens)
    let detail = args.iter().any(|a| a == "--detail");
    let depot = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let qs = lire_questions(&depot.join("eval").join("rappel.jsonl"));
    anyhow::ensure!(!qs.is_empty(), "eval/rappel.jsonl introuvable ou vide");

    let racine = std::env::temp_dir().join(format!("orchestra-rappel-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]));
    let paths = Paths::at(racine.clone())?;
    std::fs::copy(depot.join("eval").join("donnees").join("orchestra.md"), paths.main_memory())?;
    let profil = memory::profil(&paths, PROFIL_MAX_CAR).0;
    let http = reqwest::Client::new();
    let sens_present = modele_installe(&http, &ollama, &modele).await;
    let sens = Sens { actif: true, url: ollama.clone(), modele: modele.clone() };
    println!("{} question(s) · modèle d'embedding {modele} : {}", qs.len(), if sens_present { "installé" } else { "absent (mesures du sens sautées)" });

    // ---------- qualité ----------
    let avant = Qualite { rangs: qs.iter().map(|q| rang(&ancien_rappel(&paths, &q.question, MAX), &q.fait)).collect() };
    index::configurer(&paths, Sens::default());
    let mut rangs_mots = vec![];
    for q in &qs {
        let r = index::rappeler(&paths, &q.question, MAX, &profil, Duration::ZERO).await;
        rangs_mots.push(rang(&r.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), &q.fait));
    }
    let mots = Qualite { rangs: rangs_mots };

    let mut avec_sens: Option<Qualite> = None;
    let mut vectorisation_essai = None;
    let mut temps_question: Vec<f64> = vec![];
    let mut temps_rappel_sens: Vec<f64> = vec![];
    let mut voix_a_temps = (0usize, 0usize);
    let mut taille_index_essai = 0u64;
    if sens_present {
        index::configurer(&paths, sens.clone());
        // chargement du modèle sur le processeur (une fois), hors mesure
        let _ = http.post(format!("{ollama}/api/embed")).json(&json!({ "model": modele, "input": ["échauffement"], "keep_alive": -1, "options": index::options_sens() })).send().await;
        let t = Instant::now();
        let n = index::vectoriser(&paths).await;
        vectorisation_essai = Some((n, t.elapsed()));
        taille_index_essai = std::fs::metadata(paths.memoire.join(".index.bin")).map(|m| m.len()).unwrap_or(0);
        let mut rangs = vec![];
        for q in &qs {
            let t = Instant::now();
            let r = index::rappeler(&paths, &q.question, MAX, &profil, Duration::from_secs(3)).await;
            temps_rappel_sens.push(ms(t.elapsed()));
            rangs.push(rang(&r.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), &q.fait));
            if detail && q.sorte == "sens" {
                let s = index::sens_de(&paths, &q.question, Duration::from_secs(3)).await;
                let pos = s.iter().position(|(t, _)| t.contains(&q.fait));
                println!(
                    "{} sens du fait attendu : {} (rang {}), meilleur {:.3}, 6e {:.3}, 20e {:.3}",
                    q.id,
                    pos.map(|i| format!("{:.3}", s[i].1)).unwrap_or("?".into()),
                    pos.map(|i| (i + 1).to_string()).unwrap_or("?".into()),
                    s.first().map(|x| x.1).unwrap_or(0.0),
                    s.get(5).map(|x| x.1).unwrap_or(0.0),
                    s.get(19).map(|x| x.1).unwrap_or(0.0)
                );
            }
            if detail && rang(&r.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), &q.fait).is_none() {
                println!("\n{} « {} » (attendu : {})", q.id, q.question, q.fait);
                for x in &r {
                    println!("   {:.4} [{}] {}", x.score, x.reason, x.text.chars().take(90).collect::<String>());
                }
            }
            // voix : le sens arrive-t-il dans les 35 ms ? (seulement si le sens a servi avec 3 s)
            if r.iter().any(|x| x.reason.contains("sens")) {
                voix_a_temps.1 += 1;
                let rv = index::rappeler(&paths, &q.question, MAX, &profil, Duration::from_millis(35)).await;
                if rv.iter().any(|x| x.reason.contains("sens")) {
                    voix_a_temps.0 += 1;
                }
            }
        }
        avec_sens = Some(Qualite { rangs });
        // vecteur d'une question seule (client HTTP déjà ouvert), 3 fois chaque question
        for _ in 0..3 {
            for q in &qs {
                let t = Instant::now();
                let _ = http
                    .post(format!("{ollama}/api/embed"))
                    .json(&json!({ "model": modele, "input": [format!("{CONSIGNE_QUESTION}{}", q.question)], "keep_alive": -1, "options": index::options_sens() }))
                    .send()
                    .await
                    .and_then(|r| r.error_for_status());
                temps_question.push(ms(t.elapsed()));
            }
        }
    }

    // ---------- temps : mémoire d'essai, par les mots ----------
    index::configurer(&paths, Sens::default());
    let mut t_mots_essai = vec![];
    let mut t_ancien_essai = vec![];
    for _ in 0..5 {
        for q in &qs {
            let t = Instant::now();
            let _ = index::rappeler(&paths, &q.question, MAX, &profil, Duration::ZERO).await;
            t_mots_essai.push(ms(t.elapsed()));
            let t = Instant::now();
            let _ = ancien_rappel(&paths, &q.question, MAX);
            t_ancien_essai.push(ms(t.elapsed()));
        }
    }
    let faits_essai = index::faits(&paths).len();

    // ---------- grande mémoire ----------
    std::fs::write(paths.memoire_agents.join("agent-synthetique.md"), grande_memoire(grand))?;
    let t = Instant::now();
    let faits_grand = index::faits(&paths).len();
    let construction = t.elapsed();
    let mut t_mots_grand = vec![];
    let mut t_ancien_grand = vec![];
    for _ in 0..5 {
        for q in &qs {
            let t = Instant::now();
            let _ = index::rappeler(&paths, &q.question, MAX, &profil, Duration::ZERO).await;
            t_mots_grand.push(ms(t.elapsed()));
        }
    }
    for q in &qs {
        let t = Instant::now();
        let _ = ancien_rappel(&paths, &q.question, MAX);
        t_ancien_grand.push(ms(t.elapsed()));
    }
    let taille_memoire_grand: u64 = std::fs::read_dir(&paths.memoire_agents).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>() + std::fs::metadata(paths.main_memory()).map(|m| m.len()).unwrap_or(0);
    // qualité gardée quand la mémoire grossit (mots seuls)
    let mut rangs_grand = vec![];
    for q in &qs {
        let r = index::rappeler(&paths, &q.question, MAX, &profil, Duration::ZERO).await;
        rangs_grand.push(rang(&r.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), &q.fait));
    }
    let mots_grand = Qualite { rangs: rangs_grand };

    let mut vectorisation_grand = None;
    let mut t_sens_grand = vec![];
    let mut taille_index_grand = 0u64;
    let mut sens_grand: Option<Qualite> = None;
    if sens_present {
        index::configurer(&paths, sens.clone());
        println!("Calcul des vecteurs de {faits_grand} faits sur le processeur…");
        let t = Instant::now();
        let n = index::vectoriser(&paths).await;
        vectorisation_grand = Some((n, t.elapsed()));
        taille_index_grand = std::fs::metadata(paths.memoire.join(".index.bin")).map(|m| m.len()).unwrap_or(0);
        let mut rangs = vec![];
        for q in &qs {
            let t = Instant::now();
            let r = index::rappeler(&paths, &q.question, MAX, &profil, Duration::from_secs(3)).await;
            t_sens_grand.push(ms(t.elapsed()));
            rangs.push(rang(&r.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), &q.fait));
            if detail && q.sorte == "sens" {
                let s = index::sens_de(&paths, &q.question, Duration::from_secs(3)).await;
                let pos = s.iter().position(|(t, _)| t.contains(&q.fait));
                println!("[grande mémoire] {} sens du fait attendu : {} (rang {})", q.id, pos.map(|i| format!("{:.3}", s[i].1)).unwrap_or("?".into()), pos.map(|i| (i + 1).to_string()).unwrap_or("?".into()));
            }
            if detail && q.sorte == "sens" && rang(&r.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), &q.fait).is_none() {
                println!("\n[grande mémoire] {} « {} » (attendu : {})", q.id, q.question, q.fait);
                for x in &r {
                    println!("   {:.4} [{}] {}", x.score, x.reason, x.text.chars().take(90).collect::<String>());
                }
            }
        }
        sens_grand = Some(Qualite { rangs });
    }

    // ---------- rapport ----------
    let date = chrono::Local::now();
    let mut s = format!("# Mesure du rappel (lot M2) — {}\n\n", date.format("%d/%m/%Y %H:%M"));
    s.push_str(&format!("Jeu : `eval/rappel.jsonl` ({} questions : {} « mots », {} « sens ») sur `eval/donnees/orchestra.md` ({faits_essai} faits indexés hors sections internes). Modèle d'embedding : `{modele}` ({}).\n\n", qs.len(), qs.iter().filter(|q| q.sorte == "mots").count(), qs.iter().filter(|q| q.sorte == "sens").count(), if sens_present { "installé, sur le processeur" } else { "absent : colonnes du sens vides" }));
    s.push_str("## Qualité : le fait attendu est dans les 6 souvenirs\n\n| Calcul | Tout | Questions « mots » | Questions « sens » |\n|---|---|---|---|\n");
    let ligne = |nom: &str, q: &Qualite| format!("| {nom} | {} | {} | {} |\n", q.resume(&qs, None), q.resume(&qs, Some("mots")), q.resume(&qs, Some("sens")));
    s.push_str(&ligne("Avant M2 (mots communs, profil compris)", &avant));
    s.push_str(&ligne("M2, mots seuls (BM25)", &mots));
    if let Some(q) = &avec_sens {
        s.push_str(&ligne("M2, mots + sens", q));
    }
    s.push_str(&ligne(&format!("M2, mots seuls, avec {grand} faits de plus"), &mots_grand));
    if let Some(q) = &sens_grand {
        s.push_str(&ligne(&format!("M2, mots + sens, avec {grand} faits de plus"), q));
    }
    s.push_str("\n### Détail par question (rang du fait attendu, — = absent des 6)\n\n| # | Type | Question | Avant | Mots | Mots + sens |\n|---|---|---|---|---|---|\n");
    let r = |x: &Option<usize>| x.map(|v| v.to_string()).unwrap_or_else(|| "—".into());
    for (i, q) in qs.iter().enumerate() {
        s.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", q.id, q.sorte, q.question, r(&avant.rangs[i]), r(&mots.rangs[i]), avec_sens.as_ref().map(|a| r(&a.rangs[i])).unwrap_or_default()));
    }
    s.push_str("\n## Temps\n\n| Mesure | Médiane | 95 % |\n|---|---|---|\n");
    let t = |nom: String, v: &[f64]| {
        let (m, p) = mediane_p95(v.to_vec());
        format!("| {nom} | {m:.2} ms | {p:.2} ms |\n")
    };
    s.push_str(&t(format!("Rappel avant M2, {faits_essai} faits"), &t_ancien_essai));
    s.push_str(&t(format!("Rappel M2 par les mots, {faits_essai} faits"), &t_mots_essai));
    s.push_str(&t(format!("Rappel avant M2, {faits_grand} faits"), &t_ancien_grand));
    s.push_str(&t(format!("Rappel M2 par les mots, {faits_grand} faits"), &t_mots_grand));
    if sens_present {
        s.push_str(&t("Vecteur d'une question seule (processeur)".into(), &temps_question));
        s.push_str(&t(format!("Rappel M2 mots + sens, {faits_essai} faits (chat)"), &temps_rappel_sens));
        s.push_str(&t(format!("Rappel M2 mots + sens, {faits_grand} faits (chat)"), &t_sens_grand));
    }
    s.push_str(&format!("\n- Lecture et découpage de {faits_grand} faits (premier rappel après lancement) : {:.0} ms.\n", ms(construction)));
    s.push_str(&format!("- Taille des fichiers de mémoire de la grande mémoire : {:.0} Ko.\n", taille_memoire_grand as f64 / 1024.0));
    if let Some((n, d)) = vectorisation_essai {
        s.push_str(&format!("- Vecteurs de la mémoire d'essai : {n} en {:.1} s ; `.index.bin` : {:.1} Ko.\n", d.as_secs_f64(), taille_index_essai as f64 / 1024.0));
    }
    if let Some((n, d)) = vectorisation_grand {
        s.push_str(&format!("- Vecteurs de la grande mémoire : {n} en {:.1} s ({:.1} ms par fait) ; `.index.bin` : {:.0} Ko.\n", d.as_secs_f64(), ms(d) / n.max(1) as f64, taille_index_grand as f64 / 1024.0));
    }
    if sens_present {
        s.push_str(&format!("- Voix (budget 35 ms) : le sens arrive à temps pour {}/{} question(s) où il sert au chat.\n", voix_a_temps.0, voix_a_temps.1));
    }
    let reel = Paths::init()?;
    let dossier = reel.root.join("eval");
    std::fs::create_dir_all(&dossier)?;
    let rapport = dossier.join(format!("rappel-{}.md", date.format("%Y-%m-%d_%H%M")));
    std::fs::write(&rapport, &s)?;
    let _ = std::fs::remove_dir_all(&racine);
    println!("{s}\nRapport : {}", rapport.display());
    Ok(())
}
