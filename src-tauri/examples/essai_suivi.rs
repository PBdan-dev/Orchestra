//! Essai d'acceptation du lot T9, avec le vrai modèle (Ollama) : deux missions d'agent-brouillons
//! sur un même projet, puis 5 questions sur le contenu des livrables. Orchestra doit répondre
//! juste en lisant le suivi et les livrables, sans les modifier.
//!
//! Depuis `src-tauri/` (Ollama lancé, carte graphique libre) :
//!   cargo run --release --example essai_suivi
//! Dossier de test (effacé au début) : `Orchestra/eval/essai-suivi/`.

use std::sync::{Arc, Mutex};

use orchestra_lib::{
    agents,
    config::Config,
    events::{Event, Sortie},
    memory,
    mesures::Releve,
    orchestrator::{self, Contexte},
    paths::Paths,
    verification,
};

#[derive(Default)]
struct Collecte(Mutex<Vec<Event>>);
impl Sortie for Collecte {
    fn envoyer(&self, ev: Event) {
        self.0.lock().unwrap().push(ev);
    }
}

const PROJET: &str = "salon-2027";

const QUESTIONS: &[(&str, &[&[&str]])] = &[
    ("Sur le projet salon-2027, quel stand a-t-on retenu ?", &[&["b12"]]),
    ("Dans le projet salon-2027, quel budget est indiqué pour le stand dans le mail à Camille ?", &[&["4 500", "4500", "4.500"]]),
    ("Dans le projet salon-2027, à qui a-t-on écrit au sujet du changement de date ?", &[&["marc"]]),
    ("D'après les livrables du projet salon-2027, quelle est la nouvelle date du salon ?", &[&["14 mars", "14/03"]]),
    ("Combien de missions ont été faites sur le projet salon-2027, et quels fichiers ont été produits ?", &[&["2 ", "deux"], &["merci"], &["decalage"]]),
];

fn copier(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "__pycache__" || n == "memoire") {
            continue;
        }
        if p.is_dir() { copier(&p, &dest.join(e.file_name()))? } else { std::fs::copy(&p, dest.join(e.file_name())).map(|_| ())? }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let reel = Paths::init()?;
    let base = reel.root.join("eval").join("essai-suivi");
    let _ = std::fs::remove_dir_all(&base);
    let paths = Paths::at(base.join("Orchestra"))?;
    memory::seed_main(&paths, "PB")?;
    copier(&agents::dossier_installe(&reel, "agent-brouillons"), &agents::dossier_installe(&paths, "agent-brouillons"))?;
    std::fs::create_dir_all(paths.projets.join(PROJET))?;
    let mut cfg = Config::load(&reel);
    cfg.projet_actif = PROJET.into();
    cfg.market_url = base.join("pas-de-market.json").display().to_string();
    let http = reqwest::Client::builder().user_agent("Orchestra/0.1").build()?;
    let ctx = Contexte {
        cfg: cfg.clone(),
        paths: paths.clone(),
        http,
        releve: Arc::new(Releve::vers(base.join("mesures"))),
        session: Default::default(),
        arreter_voix: None,
        validations: None,
        pilote: None,
    };
    println!("modèle : {}", cfg.model);

    // 1. Deux missions sur le projet
    for demande in [
        "Prépare un brouillon de mail pour Camille Martin pour la remercier de la réunion de mardi : on a retenu le stand B12, avec un budget de 4 500 euros.",
        "Prépare un brouillon de mail pour Marc Dubois : le salon est décalé au 14 mars 2027 et il faut réserver l'hôtel avant le 1er février.",
    ] {
        let sortie = Collecte::default();
        let t = orchestrator::tour(&sortie, &ctx, "essai", demande, vec![]).await?;
        println!("mission : agent {:?} · {}", t.agent, t.reponse.chars().take(160).collect::<String>().replace('\n', " "));
    }
    let suivi = std::fs::read_to_string(paths.projets.join(PROJET).join("SUIVI.md")).unwrap_or_default();
    println!("\n--- SUIVI.md ---\n{suivi}\n----------------");
    let livrables_avant = verification::empreintes(&paths.projets.join(PROJET));

    // 2. Cinq questions, chacune dans une conversation neuve
    let mut justes = 0;
    for (q, attendus) in QUESTIONS {
        let sortie = Collecte::default();
        let t = orchestrator::tour(&sortie, &ctx, "essai", q, vec![]).await?;
        let outils: Vec<String> = sortie.0.lock().unwrap().iter().filter_map(|e| if let Event::ToolCall { tool, input, .. } = e { Some(format!("{tool}({})", input["chemin"].as_str().unwrap_or(""))) } else { None }).collect();
        let r = memory::fold(&t.reponse);
        let ok = attendus.iter().all(|groupe| groupe.iter().any(|a| r.contains(&memory::fold(a))));
        justes += usize::from(ok);
        println!("\n{} {q}\n   outils : {}\n   réponse : {}", if ok { "OK " } else { "KO " }, outils.join(", "), t.reponse.replace('\n', " "));
    }
    let intacts = verification::empreintes(&paths.projets.join(PROJET)) == livrables_avant;
    println!("\nlivrables et SUIVI.md inchangés par les questions : {intacts}");
    println!("Bilan : {justes}/5 réponses justes · SUIVI.md {}", if suivi.contains("### ") { "écrit" } else { "ABSENT" });
    if justes < 5 || !intacts || suivi.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
