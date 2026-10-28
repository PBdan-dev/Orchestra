//! Lanceur d'évaluation (Phase 0) : rejoue les jeux `eval/memoire.jsonl` et `eval/taches.jsonl`
//! sans interface, dans un dossier de données séparé (`Orchestra/eval/donnees-test/`, remis à neuf
//! avant chaque question), et écrit un rapport lisible dans `Orchestra/eval/`.
//! La vraie mémoire de l'utilisateur n'est ni lue ni modifiée.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::{
    agents,
    brain::Brain,
    config::Config,
    events::{Event, Sortie},
    memory,
    mesures::{Mesure, Releve},
    orchestrator::{self, Contexte, Tour},
    paths::Paths,
};

// ---------- Jeux ----------

/// Une question à réponse connue sur la mémoire de test.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct QuestionMemoire {
    pub id: String,
    pub question: String,
    /// Réponse de référence (pour le lecteur du rapport).
    pub attendu: String,
    /// Tous doivent figurer dans la réponse ; « a|b » = a ou b. Sans accents ni majuscules.
    pub mots_cles: Vec<String>,
    /// Aucun ne doit figurer dans la réponse (piège : l'information voisine, ou l'ancienne).
    #[serde(default)]
    pub interdits: Vec<String>,
    /// La réponse n'est pas en mémoire : Orchestra doit le dire au lieu d'inventer.
    #[serde(default)]
    pub hors_memoire: bool,
    /// direct | reformulee | piege | modifie | absent
    #[serde(default)]
    pub r#type: String,
}

impl QuestionMemoire {
    pub fn reussie(&self, reponse: &str) -> bool {
        note_memoire(reponse, &self.mots_cles) && !self.interdits.iter().any(|i| note_memoire(reponse, std::slice::from_ref(i)))
    }
}

/// Une demande type et la décision attendue.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Tache {
    pub id: String,
    pub demande: String,
    /// repondre | preciser | deleguer | hors_capacite
    pub attendu: String,
    /// Agent attendu quand `attendu` = deleguer (vide = n'importe lequel).
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub note: String,
}

pub fn lire_jsonl<T: for<'de> Deserialize<'de>>(p: &Path) -> anyhow::Result<Vec<T>> {
    let txt = std::fs::read_to_string(p).map_err(|e| anyhow::anyhow!("{} : {e}", p.display()))?;
    let mut out = vec![];
    for (i, l) in txt.lines().enumerate() {
        let l = l.trim();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        out.push(serde_json::from_str(l).map_err(|e| anyhow::anyhow!("{} ligne {} : {e}", p.display(), i + 1))?);
    }
    Ok(out)
}

// ---------- Notation ----------

/// Vrai si chaque mot-clé (ou l'une de ses variantes « a|b ») figure dans la réponse,
/// en mots entiers (« mit » ne compte pas dans « limite »), sans accents ni majuscules.
pub fn note_memoire(reponse: &str, mots_cles: &[String]) -> bool {
    let plie = |s: &str| memory::fold(s).split_whitespace().collect::<Vec<_>>().join(" ");
    let r = format!(" {} ", plie(reponse));
    mots_cles.iter().all(|k| {
        k.split('|').any(|v| {
            let v = plie(v);
            !v.is_empty() && r.contains(&format!(" {v} "))
        })
    })
}

/// Décision prise pendant un tour : deleguer:<agent> | hors_capacite | preciser | repondre.
pub fn decision(t: &Tour) -> String {
    if let Some(a) = &t.agent {
        return format!("deleguer:{a}");
    }
    if t.aucun_agent {
        return "hors_capacite".into();
    }
    if pose_une_question(&t.reponse) {
        return "preciser".into();
    }
    "repondre".into()
}

/// La réponse se termine-t-elle par une question à l'utilisateur ? (dernières phrases)
pub fn pose_une_question(reponse: &str) -> bool {
    let fin: String = reponse.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
    fin.contains('?')
}

pub fn note_tache(t: &Tache, decision: &str) -> bool {
    match t.attendu.as_str() {
        "deleguer" => match decision.strip_prefix("deleguer:") {
            Some(a) => t.agent.is_empty() || t.agent == a,
            None => false,
        },
        // Une réponse directe peut finir par une question de politesse (« et toi ? ») : seul
        // compte qu'Orchestra ait répondu lui-même, sans déléguer.
        "repondre" => decision == "repondre" || decision == "preciser",
        autre => decision == autre,
    }
}

// ---------- Exécution ----------

/// Sortie du lanceur : garde les événements en mémoire au lieu de les afficher.
#[derive(Default)]
pub struct Collecte(pub Mutex<Vec<Event>>);

impl Sortie for Collecte {
    fn envoyer(&self, ev: Event) {
        if !matches!(ev, Event::Token { .. } | Event::Thought { .. }) {
            self.0.lock().unwrap().push(ev);
        }
    }
}

pub struct Options {
    pub modele: String,
    pub passages: usize,
    pub temperature: f32,
    pub memoire: bool,
    pub taches: bool,
    /// Nombre maximal de questions par jeu (essai rapide).
    pub limite: Option<usize>,
    /// Dossier de données réel (`C:\Users\…\Orchestra`) : on n'y écrit que dans `eval/`.
    pub racine: PathBuf,
    /// Dossier `eval/` du dépôt (jeux et mémoire de test).
    pub jeux: PathBuf,
}

#[derive(Serialize, Clone, Debug)]
pub struct Essai {
    pub jeu: String,
    pub id: String,
    pub passage: usize,
    pub entree: String,
    pub reponse: String,
    pub decision: String,
    pub reussi: bool,
    pub souvenirs: Vec<String>,
    pub erreur: Option<String>,
    pub duree_ms: f64,
    pub mesures: Vec<Mesure>,
}

/// Remet à neuf le dossier de test : mémoire de test, registre de démonstration, rien d'autre.
fn preparer(dossier: &Path, memoire_test: &str) -> anyhow::Result<Paths> {
    if dossier.exists() {
        std::fs::remove_dir_all(dossier)?;
    }
    let paths = Paths::at(dossier.to_path_buf())?;
    std::fs::write(paths.main_memory(), memoire_test)?;
    agents::seed_registry(&paths)?;
    Ok(paths)
}

async fn un_essai(opts: &Options, cfg: &Config, http: &reqwest::Client, memoire_test: &str, dossier_mesures: &Path, entree: &str) -> (Tour, Vec<String>, Option<String>, f64, Vec<Mesure>) {
    let dossier = opts.racine.join("eval").join("donnees-test");
    let paths = match preparer(&dossier, memoire_test) {
        Ok(p) => p,
        Err(e) => return (Tour::default(), vec![], Some(format!("préparation : {e}")), 0.0, vec![]),
    };
    let releve = Arc::new(Releve::vers(dossier_mesures.to_path_buf()));
    let ctx = Contexte { cfg: cfg.clone(), paths, http: http.clone(), releve: releve.clone() };
    let sortie = Collecte::default();
    let debut = Instant::now();
    let res = orchestrator::tour(&sortie, &ctx, "eval", entree, vec![]).await;
    let duree = debut.elapsed().as_secs_f64() * 1000.0;
    let evs = sortie.0.lock().unwrap();
    let souvenirs = evs
        .iter()
        .filter_map(|e| if let Event::Recall { items, .. } = e { Some(items.iter().map(|r| r.text.clone()).collect::<Vec<_>>()) } else { None })
        .flatten()
        .collect();
    let erreurs: Vec<String> = evs.iter().filter_map(|e| if let Event::Error { message, .. } = e { Some(message.clone()) } else { None }).collect();
    match res {
        Ok(t) => (t, souvenirs, if erreurs.is_empty() { None } else { Some(erreurs.join(" ; ")) }, duree, releve.lignes()),
        Err(e) => (Tour::default(), souvenirs, Some(e.to_string()), duree, releve.lignes()),
    }
}

/// Lance l'évaluation et renvoie le chemin du rapport `.md`.
pub async fn lancer(opts: Options, journal: impl Fn(&str)) -> anyhow::Result<PathBuf> {
    let reel = Config::load(&Paths::at(opts.racine.clone())?);
    let mut cfg = Config { provider: "ollama".into(), model: opts.modele.clone(), ollama_url: reel.ollama_url.clone(), ..Config::default() };
    cfg.max_agent_steps = reel.max_agent_steps;
    cfg.eval_temperature = Some(opts.temperature);
    let http = reqwest::Client::builder().user_agent("Orchestra-eval/0.1").build()?;

    let memoire_test = std::fs::read_to_string(opts.jeux.join("donnees").join("orchestra.md"))?;
    let mut questions: Vec<QuestionMemoire> = if opts.memoire { lire_jsonl(&opts.jeux.join("memoire.jsonl"))? } else { vec![] };
    let mut taches: Vec<Tache> = if opts.taches { lire_jsonl(&opts.jeux.join("taches.jsonl"))? } else { vec![] };
    if let Some(n) = opts.limite {
        questions.truncate(n);
        taches.truncate(n);
    }

    let horodatage = chrono::Local::now().format("%Y-%m-%d_%H%M").to_string();
    let dossier_eval = opts.racine.join("eval");
    std::fs::create_dir_all(&dossier_eval)?;
    let dossier_mesures = dossier_eval.join(format!("mesures-{horodatage}"));

    // Chargement du modèle hors mesure, pour ne pas fausser la première latence. Par un vrai
    // appel de chat (même num_ctx que les tours) : `load_ollama` charge sans num_ctx, et Ollama
    // rechargerait le modèle au premier tour (constaté : 18 s).
    journal(&format!("Chargement de {} …", cfg.model));
    let b = Brain::new(cfg.clone(), http.clone());
    b.complete("prechauffage", &[crate::brain::Msg::user("Réponds juste : prêt.")]).await?;

    let debut = Instant::now();
    let mut essais: Vec<Essai> = vec![];
    let total = (questions.len() + taches.len()) * opts.passages;
    for p in 1..=opts.passages {
        for q in &questions {
            journal(&format!("[{}/{}] mémoire {} · passage {p}", essais.len() + 1, total, q.id));
            let (t, souv, err, duree, mesures) = un_essai(&opts, &cfg, &http, &memoire_test, &dossier_mesures, &q.question).await;
            let reussi = err.is_none() && q.reussie(&t.reponse);
            essais.push(Essai { jeu: "memoire".into(), id: q.id.clone(), passage: p, entree: q.question.clone(), decision: decision(&t), reponse: t.reponse, reussi, souvenirs: souv, erreur: err, duree_ms: duree, mesures });
        }
        for tache in &taches {
            journal(&format!("[{}/{}] tâche {} · passage {p}", essais.len() + 1, total, tache.id));
            let (t, souv, err, duree, mesures) = un_essai(&opts, &cfg, &http, &memoire_test, &dossier_mesures, &tache.demande).await;
            let d = decision(&t);
            let reussi = err.is_none() && note_tache(tache, &d);
            essais.push(Essai { jeu: "taches".into(), id: tache.id.clone(), passage: p, entree: tache.demande.clone(), decision: d, reponse: t.reponse, reussi, souvenirs: souv, erreur: err, duree_ms: duree, mesures });
        }
    }
    let duree_totale = debut.elapsed().as_secs_f64();
    let _ = std::fs::remove_dir_all(dossier_eval.join("donnees-test"));

    // Détail complet (réponses entières, mesures) en JSONL, rapport lisible en Markdown.
    let detail = dossier_eval.join(format!("rapport-{horodatage}.jsonl"));
    let mut lignes = String::new();
    for e in &essais {
        lignes.push_str(&serde_json::to_string(e)?);
        lignes.push('\n');
    }
    std::fs::write(&detail, lignes)?;

    let md = rapport(&opts, &cfg, &questions, &taches, &essais, duree_totale, &detail);
    let chemin = dossier_eval.join(format!("rapport-{horodatage}.md"));
    std::fs::write(&chemin, md)?;
    Ok(chemin)
}

// ---------- Rapport ----------

pub fn mediane(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

pub fn centile(mut v: Vec<f64>, p: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let i = ((p / 100.0) * (v.len() - 1) as f64).round() as usize;
    Some(v[i.min(v.len() - 1)])
}

fn moyenne(v: &[f64]) -> Option<f64> {
    if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) }
}

fn f0(x: Option<f64>) -> String {
    x.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into())
}

/// Taille d'un dossier, sous-dossiers compris (octets).
pub fn taille(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => taille(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

fn mo(o: u64) -> String {
    if o >= 1 << 30 {
        format!("{:.2} Go", o as f64 / (1u64 << 30) as f64)
    } else if o >= 1 << 20 {
        format!("{:.1} Mo", o as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.1} ko", o as f64 / 1024.0)
    }
}

/// Texte sur une ligne pour un tableau ; une coupe est toujours marquée.
fn cellule(s: &str, n: usize) -> String {
    let one = s.replace(['\n', '\r'], " ").replace('|', "/");
    if one.chars().count() <= n {
        one
    } else {
        format!("{} […coupé, texte entier dans le .jsonl]", one.chars().take(n).collect::<String>())
    }
}

fn rapport(opts: &Options, cfg: &Config, questions: &[QuestionMemoire], taches: &[Tache], essais: &[Essai], duree_s: f64, detail: &Path) -> String {
    let mut s = String::new();
    let quand = chrono::Local::now().format("%d/%m/%Y %H:%M");
    s.push_str(&format!("# Évaluation d'Orchestra — {quand}\n\n"));
    s.push_str(&format!(
        "Cerveau : Ollama · `{}` · température {} · {} passage(s) par question · durée totale {:.0} min\n\n",
        cfg.model, opts.temperature, opts.passages, duree_s / 60.0
    ));
    s.push_str(&format!("Détail complet (réponses entières, mesures de chaque appel) : `{}`\n\n", detail.display()));

    // Réussite majoritaire : une question compte si elle réussit à plus de la moitié des passages.
    let majorite = |jeu: &str, id: &str| {
        let e: Vec<&Essai> = essais.iter().filter(|e| e.jeu == jeu && e.id == id).collect();
        let ok = e.iter().filter(|e| e.reussi).count();
        (ok, e.len(), ok * 2 > e.len())
    };

    s.push_str("## Résumé\n\n| Mesure | Valeur |\n|---|---|\n");
    if !questions.is_empty() {
        let ok = questions.iter().filter(|q| majorite("memoire", &q.id).2).count();
        let essais_ok = essais.iter().filter(|e| e.jeu == "memoire" && e.reussi).count();
        let n = essais.iter().filter(|e| e.jeu == "memoire").count();
        s.push_str(&format!("| Mémoire (réussite majoritaire) | **{ok}/{}** |\n| Mémoire (tous passages) | {essais_ok}/{n} |\n", questions.len()));
        // par type de question
        let mut types: Vec<&str> = questions.iter().map(|q| q.r#type.as_str()).filter(|t| !t.is_empty()).collect();
        types.sort();
        types.dedup();
        for ty in types {
            let qs: Vec<&QuestionMemoire> = questions.iter().filter(|q| q.r#type == ty).collect();
            let ok = qs.iter().filter(|q| majorite("memoire", &q.id).2).count();
            s.push_str(&format!("| · questions « {ty} » | {ok}/{} |\n", qs.len()));
        }
        let mots: Vec<f64> = essais.iter().filter(|e| e.jeu == "memoire" && e.erreur.is_none()).map(|e| e.reponse.split_whitespace().count() as f64).collect();
        s.push_str(&format!("| Longueur des réponses mémoire, médiane / moyenne (mots) | {} / {} |\n", f0(mediane(mots.clone())), f0(moyenne(&mots))));
    }
    if !taches.is_empty() {
        let ok = taches.iter().filter(|t| majorite("taches", &t.id).2).count();
        let pct = 100.0 * ok as f64 / taches.len() as f64;
        s.push_str(&format!("| Routage (réussite majoritaire) | **{ok}/{} ({pct:.0} %)** |\n", taches.len()));
    }
    let durees: Vec<f64> = essais.iter().filter(|e| e.erreur.is_none()).map(|e| e.duree_ms).collect();
    s.push_str(&format!("| Durée d'un tour, médiane / 90e centile | {} / {} ms |\n", f0(mediane(durees.clone())), f0(centile(durees, 90.0))));
    let appels: Vec<f64> = essais.iter().map(|e| e.mesures.len() as f64).collect();
    s.push_str(&format!("| Appels au cerveau par tour (moyenne) | {} |\n", moyenne(&appels).map(|v| format!("{v:.2}")).unwrap_or_else(|| "—".into())));
    let n_err = essais.iter().filter(|e| e.erreur.is_some()).count();
    s.push_str(&format!("| Tours en erreur | {n_err} |\n"));
    let racine = &opts.racine;
    let tot = taille(racine);
    let voix = taille(&racine.join("voix"));
    let eval = taille(&racine.join("eval"));
    s.push_str(&format!(
        "| Taille du dossier `Orchestra/` | {} (dont voix {}, eval {}) ; hors voix et eval : {} |\n",
        mo(tot), mo(voix), mo(eval), mo(tot.saturating_sub(voix + eval))
    ));
    s.push_str(&format!("| Taille de `Orchestra/memoire/` | {} |\n\n", mo(taille(&racine.join("memoire")))));

    // Mesures par rôle
    s.push_str("## Appels au cerveau, par rôle\n\n| Rôle | Appels | prompt_eval_count moyen | Calcul du prompt, moyenne (ms) | Premier token, médiane (ms) | Tokens générés, moyenne | Durée, médiane (ms) |\n|---|---|---|---|---|---|---|\n");
    let toutes: Vec<&Mesure> = essais.iter().flat_map(|e| e.mesures.iter()).collect();
    for role in ["analyse", "reponse", "agent", "memorisation"] {
        let m: Vec<&&Mesure> = toutes.iter().filter(|m| m.role == role).collect();
        if m.is_empty() {
            continue;
        }
        let pe: Vec<f64> = m.iter().filter_map(|m| m.prompt_eval_count.map(|v| v as f64)).collect();
        let pm: Vec<f64> = m.iter().filter_map(|m| m.prompt_eval_ms).collect();
        let pt: Vec<f64> = m.iter().filter_map(|m| m.premier_token_ms).collect();
        let ev: Vec<f64> = m.iter().filter_map(|m| m.eval_count.map(|v| v as f64)).collect();
        let to: Vec<f64> = m.iter().map(|m| m.total_ms).collect();
        s.push_str(&format!("| {role} | {} | {} | {} | {} | {} | {} |\n", m.len(), f0(moyenne(&pe)), f0(moyenne(&pm)), f0(mediane(pt)), f0(moyenne(&ev)), f0(mediane(to))));
    }
    s.push('\n');

    if !questions.is_empty() {
        s.push_str("## Mémoire\n\n| # | Question | Attendu | Réussite | Réponse (passage 1) |\n|---|---|---|---|---|\n");
        for q in questions {
            let (ok, n, maj) = majorite("memoire", &q.id);
            let r = essais.iter().find(|e| e.jeu == "memoire" && e.id == q.id).map(|e| e.erreur.clone().map(|x| format!("ERREUR : {x}")).unwrap_or_else(|| e.reponse.clone())).unwrap_or_default();
            s.push_str(&format!("| {} | {} | {} | {}{ok}/{n} | {} |\n", q.id, cellule(&q.question, 120), cellule(&q.attendu, 80), if maj { "✅ " } else { "❌ " }, cellule(&r, 220)));
        }
        s.push('\n');
    }
    if !taches.is_empty() {
        s.push_str("## Tâches (routage)\n\n| # | Demande | Attendu | Décisions | Réussite |\n|---|---|---|---|---|\n");
        for t in taches {
            let (ok, n, maj) = majorite("taches", &t.id);
            let ds: Vec<String> = essais.iter().filter(|e| e.jeu == "taches" && e.id == t.id).map(|e| if e.erreur.is_some() { "erreur".into() } else { e.decision.clone() }).collect();
            let att = if t.attendu == "deleguer" && !t.agent.is_empty() { format!("deleguer:{}", t.agent) } else { t.attendu.clone() };
            s.push_str(&format!("| {} | {} | {att} | {} | {}{ok}/{n} |\n", t.id, cellule(&t.demande, 120), ds.join(", "), if maj { "✅ " } else { "❌ " }));
        }
        s.push('\n');
    }
    s.push_str("Notation : mémoire = tous les mots-clés attendus figurent dans la réponse (sans accents ni majuscules). \
Routage : la décision du tour (déléguer à tel agent, aucun agent disponible, question posée à l'utilisateur, réponse directe) est comparée à l'attendu ; \
« préciser » est reconnu à un point d'interrogation dans les 300 derniers caractères de la réponse ; \
une tâche « répondre » est réussie dès qu'Orchestra répond lui-même (une question de politesse finale est admise).\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoire_mots_cles_et_variantes() {
        let r = "Ta carte graphique est une RTX 3090 avec 24 Go de mémoire.";
        assert!(note_memoire(r, &["3090".into(), "24 go|24go".into()]));
        assert!(!note_memoire(r, &["4090".into()]));
        // mots entiers seulement
        assert!(!note_memoire("Dans la limite du possible.", &["mit".into()]));
        assert!(note_memoire("Licences Apache, MIT ou BSD.", &["mit".into()]));
        // sans accents ni majuscules
        assert!(note_memoire("Tu préfères le thé vert.", &["the vert".into()]));
    }

    #[test]
    fn decisions() {
        let mut t = Tour { reponse: "Voici.".into(), ..Default::default() };
        assert_eq!(decision(&t), "repondre");
        t.reponse = "Tu veux le mail en français ou en anglais ?".into();
        assert_eq!(decision(&t), "preciser");
        t.aucun_agent = true;
        assert_eq!(decision(&t), "hors_capacite");
        t.agent = Some("agent-redacteur".into());
        assert_eq!(decision(&t), "deleguer:agent-redacteur");
    }

    #[test]
    fn notation_des_taches() {
        let mut t = Tache { id: "t".into(), demande: "x".into(), attendu: "deleguer".into(), agent: "agent-redacteur".into(), note: String::new() };
        assert!(note_tache(&t, "deleguer:agent-redacteur"));
        assert!(!note_tache(&t, "deleguer:agent-fichiers"));
        t.agent.clear();
        assert!(note_tache(&t, "deleguer:agent-fichiers"));
        t.attendu = "preciser".into();
        assert!(note_tache(&t, "preciser"));
        assert!(!note_tache(&t, "repondre"));
        t.attendu = "repondre".into();
        assert!(note_tache(&t, "repondre"));
        assert!(note_tache(&t, "preciser"));
        assert!(!note_tache(&t, "deleguer:agent-fichiers"));
        assert!(!note_tache(&t, "hors_capacite"));
    }

    #[test]
    fn statistiques() {
        assert_eq!(mediane(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(mediane(vec![1.0, 2.0, 3.0, 4.0]), Some(2.5));
        assert_eq!(centile((1..=10).map(|x| x as f64).collect(), 90.0), Some(9.0));
        assert_eq!(mediane(vec![]), None);
    }

    #[test]
    fn coupe_marquee() {
        assert_eq!(cellule("court", 10), "court");
        assert!(cellule(&"x".repeat(50), 10).contains("coupé"));
    }

    #[test]
    fn jeux_du_depot_lisibles() {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("eval");
        let q: Vec<QuestionMemoire> = lire_jsonl(&d.join("memoire.jsonl")).unwrap();
        let t: Vec<Tache> = lire_jsonl(&d.join("taches.jsonl")).unwrap();
        assert_eq!(q.len(), 20);
        assert!((10..=20).contains(&t.len()));
        for t in &t {
            assert!(["repondre", "preciser", "deleguer", "hors_capacite"].contains(&t.attendu.as_str()), "{}", t.id);
        }
        // chaque réponse attendue se retrouve dans la mémoire de test
        let mem = std::fs::read_to_string(d.join("donnees").join("orchestra.md")).unwrap();
        for q in q.iter().filter(|q| !q.hors_memoire) {
            assert!(note_memoire(&mem, &q.mots_cles), "{} : mots-clés absents de la mémoire de test", q.id);
        }
    }

    #[test]
    fn interdits_et_hors_memoire() {
        let q = QuestionMemoire {
            id: "x".into(),
            question: "avocate ?".into(),
            attendu: "21 novembre".into(),
            mots_cles: vec!["21 novembre".into()],
            interdits: vec!["14 novembre".into()],
            hors_memoire: false,
            r#type: "piege".into(),
        };
        assert!(q.reussie("Le 21 novembre, avec maître Duval."));
        assert!(!q.reussie("Le 14 novembre."));
        assert!(!q.reussie("Le 21 novembre, ou peut-être le 14 novembre."));
        let absent = vec!["sais pas|n ai pas|aucune information".to_string()];
        assert!(note_memoire("Je n'ai pas cette information en mémoire.", &absent));
        assert!(!note_memoire("Ton frère s'appelle Marc.", &absent));
    }
}
