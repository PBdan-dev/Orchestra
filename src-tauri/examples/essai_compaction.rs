//! Essai d'acceptation du lot C, avec le vrai modèle : une conversation de 40 échanges en chat,
//! compactée à chaque pause (seuils abaissés pour l'essai), puis des questions de contrôle sur les
//! premiers échanges. Mesure aussi le calcul du prompt après une compaction et la latence d'une
//! réponse quand PB parle pendant une compaction (elle est annulée).
//!
//! Depuis `src-tauri/` (Ollama lancé, carte graphique libre) :
//!   cargo run --release --example essai_compaction
//! Dossier de test (effacé au début) : `Orchestra/eval/essai-compaction/`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orchestra_lib::{
    brain::Msg,
    config::Config,
    events::{Event, Sortie},
    memory,
    mesures::Releve,
    orchestrator::{self, Contexte, Session},
    paths::Paths,
};

#[derive(Default)]
struct Collecte(Mutex<Vec<Event>>);
impl Sortie for Collecte {
    fn envoyer(&self, ev: Event) {
        self.0.lock().unwrap().push(ev);
    }
}

const DEBUT: &[&str] = &[
    "On démarre un nouveau projet ensemble : on l'appelle « Aurore ».",
    "Pour Aurore, le budget maximum est de 3 000 euros, pas plus.",
    "Consigne importante : ne m'envoie jamais de mail avant 9 h du matin.",
    "La réunion de lancement d'Aurore est fixée au 12 novembre.",
    "Pour le séminaire d'Aurore, on hésite encore entre Lyon et Nantes, on tranchera plus tard.",
    "Autre décision : c'est Camille qui pilote la partie communication d'Aurore.",
];

const BAVARDAGE: &[&str] = &[
    "Au fait, pourquoi le ciel est bleu ?", "C'est quoi la différence entre un virus et une bactérie ?", "Donne-moi une idée de dessert rapide.",
    "Combien de temps faut-il pour cuire un œuf mollet ?", "Qui a peint la Joconde ?", "C'est quoi un nombre premier ?",
    "Pourquoi les feuilles tombent en automne ?", "Une astuce pour mieux dormir ?", "Quelle est la capitale de l'Australie ?",
    "Comment on dit merci en japonais ?", "C'est quoi la photosynthèse, en deux phrases ?", "Pourquoi la mer est salée ?",
    "Un conseil pour apprendre une langue ?", "Qui a écrit Les Misérables ?", "C'est quoi un trou noir ?",
    "Combien de pattes a une araignée ?", "Pourquoi on baille ?", "Une idée de lecture pour l'hiver ?",
    "C'est quoi l'inflation ?", "Pourquoi le pain rassit ?", "Comment fonctionne un arc-en-ciel ?",
    "Qui a inventé l'imprimerie ?", "C'est quoi un haïku ?", "Pourquoi les chats ronronnent ?",
    "Comment garder une plante verte en vie ?", "C'est quoi la différence entre météo et climat ?", "Pourquoi la Lune change de forme ?",
    "Une idée de sortie pour dimanche ?", "C'est quoi un algorithme ?", "Pourquoi les oignons font pleurer ?",
    "Combien d'os dans le corps humain ?", "Qui a découvert la pénicilline ?", "C'est quoi le jazz modal ?", "Pourquoi l'eau bout à 100 °C ?",
];

const CONTROLE: &[(&str, &[&[&str]])] = &[
    ("Comment s'appelle le projet qu'on a démarré au début de notre conversation ?", &[&["aurore"]]),
    ("Quel est le budget maximum du projet Aurore ?", &[&["3 000", "3000", "3.000"]]),
    ("Quelle consigne t'ai-je donnée pour les mails ?", &[&["9 h", "9h", "9 heures", "neuf heures"]]),
    ("Quand a lieu la réunion de lancement d'Aurore ?", &[&["12 novembre"]]),
    ("Pour le séminaire d'Aurore, quelle question reste ouverte ?", &[&["lyon"], &["nantes"]]),
    ("Qui pilote la communication d'Aurore ?", &[&["camille"]]),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let reel = Paths::init()?;
    let base = reel.root.join("eval").join("essai-compaction");
    let _ = std::fs::remove_dir_all(&base);
    let paths = Paths::at(base.join("Orchestra"))?;
    memory::seed_main(&paths, "PB")?;
    let mut cfg = Config::load(&reel);
    cfg.historique_seuil_haut = 1500;
    cfg.historique_seuil_bas = 700;
    cfg.market_url = base.join("pas-de-market.json").display().to_string();
    let http = reqwest::Client::builder().user_agent("Orchestra/0.1").build()?;
    let releve = Arc::new(Releve::vers(base.join("mesures")));
    let session: Arc<Mutex<Session>> = Default::default();
    let ctx = Contexte { cfg: cfg.clone(), paths: paths.clone(), http: http.clone(), releve: releve.clone(), session: session.clone(), arreter_voix: None, validations: None, pilote: None };
    println!("modèle : {} · seuils {} / {} tokens", cfg.model, cfg.historique_seuil_haut, cfg.historique_seuil_bas);

    let mut history: Vec<Msg> = vec![];
    let mut compactions = 0;
    let mut durees = vec![];
    let messages: Vec<&str> = DEBUT.iter().chain(BAVARDAGE.iter()).copied().collect();
    for (i, m) in messages.iter().enumerate() {
        let sortie = Collecte::default();
        let t0 = Instant::now();
        let t = orchestrator::tour(&sortie, &ctx, "essai", m, history.clone()).await?;
        durees.push(t0.elapsed().as_secs_f64());
        history.push(Msg::user(if t.message_historique.is_empty() { m.to_string() } else { t.message_historique.clone() }));
        history.push(Msg::new("assistant", t.reponse.clone()));
        // pause de PB : compaction s'il le faut
        let sortie = Collecte::default();
        if let Some(n) = orchestrator::compacter_session(&sortie, &cfg, &http, &paths, &session, &history).await? {
            compactions += 1;
            println!("[{:>2}/{}] compaction n° {compactions} : {n} messages fondus", i + 1, messages.len());
        }
    }
    let fichier = session.lock().unwrap().fichier.clone();
    let resume = fichier.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
    println!("\n{} échanges, {compactions} compaction(s), résumé {} :\n{resume}", messages.len(), fichier.map(|f| f.display().to_string()).unwrap_or_default());

    // Un seul calcul complet par compaction : prompt du 1er tour après la dernière compaction, puis du suivant (en cache)
    let n_avant = releve.lignes().len();

    // Questions de contrôle sur les premiers échanges
    let mut justes = 0;
    for (q, attendus) in CONTROLE {
        let sortie = Collecte::default();
        let t = orchestrator::tour(&sortie, &ctx, "essai", q, history.clone()).await?;
        history.push(Msg::user(t.message_historique.clone()));
        history.push(Msg::new("assistant", t.reponse.clone()));
        let r = memory::fold(&t.reponse);
        let ok = attendus.iter().all(|g| g.iter().any(|a| r.contains(&memory::fold(a))));
        justes += usize::from(ok);
        println!("{} {q}\n   {}", if ok { "OK " } else { "KO " }, t.reponse.replace('\n', " ").chars().take(220).collect::<String>());
    }
    let calculs: Vec<u64> = releve.lignes()[n_avant..].iter().filter(|m| m.role == "reponse").filter_map(|m| m.prompt_eval_count).collect();
    println!("\nprompt calculé par appel de réponse pendant les questions (tokens, hors cache) : {calculs:?}");

    // Latence quand PB parle pendant une compaction : la compaction est annulée
    {
        let mut s = session.lock().unwrap();
        s.couvre = 0; // force une compaction longue (tout l'historique)
    }
    let mut cfg2 = cfg.clone();
    cfg2.historique_seuil_haut = 100;
    cfg2.historique_seuil_bas = 50;
    let (h2, p2, s2, http2) = (history.clone(), paths.clone(), session.clone(), http.clone());
    let tache = tokio::spawn(async move {
        let sortie = Collecte::default();
        let _ = orchestrator::compacter_session(&sortie, &cfg2, &http2, &p2, &s2, &h2).await;
    });
    tokio::time::sleep(Duration::from_secs(2)).await;
    tache.abort();
    let t0 = Instant::now();
    let sortie = Collecte::default();
    let _ = orchestrator::tour(&sortie, &ctx, "essai", "Merci, c'est tout pour ce soir.", history.clone()).await?;
    let pendant = t0.elapsed().as_secs_f64();
    durees.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mediane = durees[durees.len() / 2];
    println!("réponse pendant une compaction annulée : {pendant:.1} s (médiane de la conversation : {mediane:.1} s)");

    println!("\nBilan : {justes}/{} questions de contrôle justes · {compactions} compaction(s)", CONTROLE.len());
    if justes < CONTROLE.len() || compactions == 0 {
        std::process::exit(1);
    }
    Ok(())
}
