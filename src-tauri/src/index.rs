//! Index du rappel (lot M2) : retrouver un souvenir par ses mots et par son sens.
//!
//! - Les faits viennent des fichiers lisibles de `memoire/` : `orchestra.md`, les fiches des agents,
//!   les résumés de conversation (`sessions/`, `resumes/`). L'index vit en mémoire vive ; à chaque
//!   rappel, seuls les fichiers dont la date ou la taille a changé sont relus (modifications de PB
//!   comprises).
//! - Mots : BM25 (sans bibliothèque), même découpage qu'avant (mots utiles et radicaux de 5 lettres).
//! - Sens : vecteurs du modèle d'embedding (D10, `qwen3-embedding:0.6b` sur le processeur via
//!   Ollama), réduits à 256 dimensions puis renormalisés (modèle Matryoshka), rangés sur un octet
//!   par valeur. Ils sont calculés à l'écriture d'un fait, jamais au rappel, sauf celui de la
//!   question ; une ligne modifiée à la main reçoit le sien en tâche de fond (choix B1).
//! - Fusion par rang : score = Σ 1/(60 + rang), pondéré par la fraîcheur (demi-vie de 90 jours pour
//!   Projets et Connaissances) et un petit bonus aux faits déjà rappelés et utilisés.
//! - `memoire/.index.bin` garde les vecteurs et l'usage ; supprimable, il se reconstruit.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::{events::Recalled, paths::Paths};

/// Dimensions gardées (Matryoshka).
pub const DIMENSIONS: usize = 256;
/// Similarité minimale pour qu'un fait soit retenu par le sens seul.
pub const SEUIL_SENS: f32 = 0.5;
/// Constante de la fusion par rang.
const K_RANG: f32 = 60.0;
/// Demi-vie de la fraîcheur (jours) et plancher du poids.
const DEMI_VIE_JOURS: f32 = 90.0;
const POIDS_MIN: f32 = 0.25;
/// Sections jamais cherchées : liste interne, description fixe des agents.
const SECTIONS_EXCLUES: [&str; 3] = ["Agents de confiance", "Rôle", "Outils autorisés"];
/// Consigne de la question pour le modèle d'embedding (les faits n'en ont pas).
const CONSIGNE_QUESTION: &str = "Instruct: Given a user question, retrieve the stored memory facts that help answer it\nQuery: ";

/// Un fait de la mémoire.
#[derive(Clone, Debug)]
pub struct Fait {
    /// Chemin relatif à `memoire/` (`orchestra.md`, `agents/x.md`, `sessions/…`).
    pub fichier: String,
    pub section: String,
    pub texte: String,
    pub date: Option<chrono::NaiveDate>,
    cle: String,
    mots: Vec<String>,
}

/// Vecteur rangé sur un octet par valeur : valeur ≈ q × échelle.
#[derive(Clone, Debug, PartialEq)]
pub struct Vecteur {
    pub q: Vec<i8>,
    pub echelle: f32,
}

/// Où calculer les vecteurs (Ollama, D10).
#[derive(Clone, Debug, Default)]
pub struct Sens {
    pub actif: bool,
    pub url: String,
    pub modele: String,
}

#[derive(Default)]
struct Etat {
    fichiers: BTreeMap<PathBuf, (SystemTime, u64)>,
    faits: Vec<Fait>,
    vecteurs: HashMap<String, Vecteur>,
    /// clé → (fois rappelé, fois utilisé dans la réponse)
    usage: HashMap<String, (u32, u32)>,
    sens: Sens,
    charge: bool,
    vectorisation: bool,
    /// Le modèle d'embedding a refusé ou n'est pas installé : signalé une fois.
    sens_indisponible: Option<String>,
    signale: bool,
}

static INDEX: Mutex<BTreeMap<PathBuf, Etat>> = Mutex::new(BTreeMap::new());

/// Réglages du rappel par le sens pour ce dossier de données.
pub fn configurer(paths: &Paths, sens: Sens) {
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.entry(paths.root.clone()).or_default();
    if e.sens.modele != sens.modele {
        // autre modèle : les anciens vecteurs ne sont plus comparables
        e.vecteurs.clear();
        e.sens_indisponible = None;
    }
    e.sens = sens;
}

/// Réglages tirés de la configuration : le sens passe toujours par Ollama (D10), quel que soit le cerveau du chat.
pub fn configurer_depuis(paths: &Paths, cfg: &crate::config::Config) {
    configurer(paths, Sens { actif: cfg.embeddings_enabled && !cfg.embedding_model.trim().is_empty(), url: cfg.ollama_url.clone(), modele: cfg.embedding_model.clone() });
}

/// Vecteur réduit à `DIMENSIONS`, renormalisé, rangé sur un octet par valeur.
pub fn quantifier(v: &[f32]) -> Option<Vecteur> {
    if v.len() < DIMENSIONS {
        return None;
    }
    let v = &v[..DIMENSIONS];
    let norme = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norme <= f32::EPSILON || !norme.is_finite() {
        return None;
    }
    let unitaire: Vec<f32> = v.iter().map(|x| x / norme).collect();
    let max = unitaire.iter().fold(0f32, |m, x| m.max(x.abs()));
    let echelle = max / 127.0;
    Some(Vecteur { q: unitaire.iter().map(|x| (x / echelle).round().clamp(-127.0, 127.0) as i8).collect(), echelle })
}

/// Similarité cosinus de deux vecteurs rangés (normés avant le rangement).
pub fn similarite(a: &Vecteur, b: &Vecteur) -> f32 {
    let produit: i64 = a.q.iter().zip(&b.q).map(|(x, y)| *x as i64 * *y as i64).sum();
    produit as f32 * a.echelle * b.echelle
}

/// Mots d'un texte pour BM25 : mots utiles et leurs radicaux de 5 lettres.
pub fn mots(texte: &str) -> Vec<String> {
    let mut out = vec![];
    for m in crate::memory::keywords(texte) {
        if m.chars().count() > 5 {
            out.push(format!("{}~", m.chars().take(5).collect::<String>()));
        }
        out.push(m);
    }
    out
}

/// Scores BM25 de chaque document pour la requête (k1 = 1,2 ; b = 0,75).
pub fn bm25<D: AsRef<[String]>>(documents: &[D], requete: &[String]) -> Vec<f32> {
    if documents.is_empty() {
        return vec![];
    }
    let documents: Vec<&[String]> = documents.iter().map(|d| d.as_ref()).collect();
    let n = documents.len() as f32;
    let longueur_moyenne = documents.iter().map(|d| d.len() as f32).sum::<f32>() / n;
    let mut termes: Vec<&String> = requete.iter().collect();
    termes.sort();
    termes.dedup();
    let idf: Vec<f32> = termes
        .iter()
        .map(|t| {
            let df = documents.iter().filter(|d| d.contains(t)).count() as f32;
            ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
        })
        .collect();
    let (k1, b) = (1.2f32, 0.75f32);
    documents
        .iter()
        .map(|d| {
            termes
                .iter()
                .zip(&idf)
                .map(|(t, idf)| {
                    let tf = d.iter().filter(|m| m == t).count() as f32;
                    if tf == 0.0 {
                        0.0
                    } else {
                        idf * tf * (k1 + 1.0) / (tf + k1 * (1.0 - b + b * d.len() as f32 / longueur_moyenne.max(1.0)))
                    }
                })
                .sum()
        })
        .collect()
}

/// Part minimale de la question qu'un fait doit couvrir (pondérée par la rareté des mots) pour
/// compter parmi les souvenirs trouvés par les mots.
pub const COUVERTURE_MIN: f32 = 0.25;

/// Part de la question couverte par chaque document, pondérée par la rareté des mots : un mot
/// présent partout (« jour ») pèse peu, un mot rare ou absent de la mémoire pèse beaucoup. Rareté
/// lissée (ln(1 + (N + 1) / (df + 0,5))) pour rester juste sur une toute petite mémoire.
pub fn couverture<D: AsRef<[String]>>(documents: &[D], requete: &[String]) -> Vec<f32> {
    let documents: Vec<&[String]> = documents.iter().map(|d| d.as_ref()).collect();
    let n = documents.len() as f32;
    let mut termes: Vec<&String> = requete.iter().collect();
    termes.sort();
    termes.dedup();
    let idf: Vec<f32> = termes
        .iter()
        .map(|t| {
            let df = documents.iter().filter(|d| d.contains(t)).count() as f32;
            (1.0 + (n + 1.0) / (df + 0.5)).ln()
        })
        .collect();
    let total: f32 = idf.iter().sum();
    if total <= 0.0 {
        return vec![0.0; documents.len()];
    }
    documents.iter().map(|d| termes.iter().zip(&idf).filter(|(t, _)| d.contains(t)).map(|(_, i)| *i).sum::<f32>() / total).collect()
}

/// Faits retenus par les mots : ceux qui couvrent une part suffisante de la question, ou qui
/// partagent avec elle un mot rare dans la mémoire (2 % des faits au plus, 2 faits au moins). Un
/// mot banal seul (« jour », « projet ») ne suffit pas ; un mot rare et juste (« licence »), si.
pub fn retenus_par_les_mots<D: AsRef<[String]>>(documents: &[D], requete: &[String]) -> Vec<bool> {
    let documents: Vec<&[String]> = documents.iter().map(|d| d.as_ref()).collect();
    let seuil_rare = (documents.len() as f32 * 0.02).max(2.0);
    let rares: Vec<&String> = requete.iter().filter(|t| (documents.iter().filter(|d| d.contains(t)).count() as f32) <= seuil_rare).collect();
    let couverts = couverture(&documents, requete);
    documents.iter().zip(couverts).map(|(d, c)| c >= COUVERTURE_MIN || rares.iter().any(|t| d.contains(t))).collect()
}

/// Poids de fraîcheur : demi-vie de 90 jours pour Projets et Connaissances, plancher à 0,25.
pub fn fraicheur(section: &str, date: Option<chrono::NaiveDate>, aujourd_hui: chrono::NaiveDate) -> f32 {
    match (section, date) {
        ("Projets" | "Connaissances", Some(d)) => {
            let age = (aujourd_hui - d).num_days().max(0) as f32;
            0.5f32.powf(age / DEMI_VIE_JOURS).max(POIDS_MIN)
        }
        _ => 1.0,
    }
}

fn cle(texte: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(texte.trim().as_bytes());
    h[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn date_du_fait(texte: &str) -> Option<chrono::NaiveDate> {
    let t = texte.trim_start().strip_prefix('(')?;
    chrono::NaiveDate::parse_from_str(t.get(..10)?, "%Y-%m-%d").ok()
}

/// Fichiers lus par l'index : mémoire principale, fiches des agents, résumés.
fn sources(paths: &Paths) -> Vec<PathBuf> {
    let mut out = vec![paths.main_memory()];
    for d in [paths.memoire_agents.clone(), paths.memoire.join("sessions"), paths.memoire.join("resumes")] {
        let mut f: Vec<PathBuf> = std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md")).collect();
        f.sort();
        out.extend(f);
    }
    out
}

/// Relit les fichiers dont la date ou la taille a changé ; charge `.index.bin` la première fois.
fn rafraichir(paths: &Paths, e: &mut Etat) {
    if !e.charge {
        e.charge = true;
        if let Some((modele, vecteurs, usage)) = lire_index(&chemin_index(paths)) {
            if modele == e.sens.modele {
                e.vecteurs = vecteurs;
            }
            e.usage = usage;
        }
        // ancien cache d'embeddings (avant le lot M2), remplacé par .index.bin
        let ancien = paths.memoire.join(".index-vectoriel.json");
        if ancien.exists() {
            crate::surveillance::noter(paths, &ancien);
            let _ = std::fs::remove_file(ancien);
        }
    }
    let presents = sources(paths);
    let mut change = false;
    for f in &presents {
        let Ok(meta) = std::fs::metadata(f) else { continue };
        let signe = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
        if e.fichiers.get(f) == Some(&signe) {
            continue;
        }
        change = true;
        let rel = relatif(paths, f);
        e.faits.retain(|x| x.fichier != rel);
        let texte = std::fs::read_to_string(f).unwrap_or_default();
        for x in crate::memory::parse_facts(&rel, &texte) {
            if SECTIONS_EXCLUES.contains(&x.section.as_str()) || x.text.trim().len() < 3 {
                continue;
            }
            e.faits.push(Fait { cle: cle(&x.text), mots: mots(&x.text), date: date_du_fait(&x.text), fichier: rel.clone(), section: x.section, texte: x.text });
        }
        e.fichiers.insert(f.clone(), signe);
    }
    // fichiers disparus
    let disparus: Vec<PathBuf> = e.fichiers.keys().filter(|f| !presents.contains(f)).cloned().collect();
    for f in disparus {
        let rel = relatif(paths, &f);
        e.faits.retain(|x| x.fichier != rel);
        e.fichiers.remove(&f);
        change = true;
    }
    if change {
        // vecteurs des faits qui n'existent plus : oubliés
        let cles: std::collections::HashSet<&String> = e.faits.iter().map(|x| &x.cle).collect();
        e.vecteurs.retain(|k, _| cles.contains(k));
    }
}

/// Relit les fichiers changés ; renvoie les faits de ce dossier (pour les tests et l'inspection).
pub fn faits(paths: &Paths) -> Vec<Fait> {
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.entry(paths.root.clone()).or_default();
    rafraichir(paths, e);
    e.faits.clone()
}

/// Options d'Ollama pour le modèle d'embedding : sur le processeur (D10), contexte de 2 048 tokens
/// (les faits sont courts ; le contexte par défaut gardait 2,7 Gio de mémoire vive, contre 0,8).
/// Toujours les mêmes, sinon Ollama recharge le modèle (1 à 1,3 s).
pub fn options_sens() -> serde_json::Value {
    serde_json::json!({ "num_gpu": 0, "num_ctx": 2048 })
}

/// Vecteurs de textes par Ollama (sur le processeur, D10), réduits et rangés.
async fn plonger(sens: &Sens, textes: &[String], delai: Duration) -> Result<Vec<Option<Vecteur>>, String> {
    // client gardé ouvert : la voix n'a que 35 ms pour le vecteur de sa phrase
    static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let http = HTTP.get_or_init(reqwest::Client::new);
    let corps = serde_json::json!({ "model": sens.modele, "input": textes, "keep_alive": -1, "options": options_sens() });
    let r = http.post(format!("{}/api/embed", sens.url.trim_end_matches('/'))).timeout(delai).json(&corps).send().await.map_err(|e| format!("modèle d'embedding injoignable : {e}"))?;
    let statut = r.status();
    let v: serde_json::Value = r.json().await.map_err(|e| format!("réponse illisible : {e}"))?;
    if !statut.is_success() {
        return Err(format!("{} ({statut})", v["error"].as_str().unwrap_or("refus d'Ollama")));
    }
    let lignes = v["embeddings"].as_array().ok_or("réponse sans « embeddings »")?;
    Ok(lignes.iter().map(|l| l.as_array().and_then(|a| quantifier(&a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect::<Vec<_>>()))).collect())
}

/// Rappel : les `max` faits les plus utiles pour la requête, hors ceux déjà dans `exclure`
/// (le profil du prompt). `delai_sens` borne le calcul du vecteur de la question.
pub async fn rappeler(paths: &Paths, requete: &str, max: usize, exclure: &str, delai_sens: Duration) -> Vec<Recalled> {
    // 1. fichiers changés relus ; lignes sans vecteur confiées à une tâche de fond (choix B1)
    let (sens, a_vectoriser) = {
        let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
        let e = g.entry(paths.root.clone()).or_default();
        rafraichir(paths, e);
        let manque = e.sens.actif && !e.vectorisation && e.sens_indisponible.is_none() && e.faits.iter().any(|x| !e.vecteurs.contains_key(&x.cle));
        (e.sens.clone(), manque)
    };
    if a_vectoriser {
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            let p = paths.clone();
            h.spawn(async move {
                vectoriser(&p).await;
            });
        }
    }
    // 2. vecteur de la question, dans le temps donné ; sinon, les mots seuls
    let question = if sens.actif && !requete.trim().is_empty() {
        tokio::time::timeout(delai_sens, plonger(&sens, &[format!("{CONSIGNE_QUESTION}{requete}")], delai_sens + Duration::from_millis(50)))
            .await
            .ok()
            .and_then(|r| r.ok())
            .and_then(|mut v| v.pop().flatten())
    } else {
        None
    };
    // 3. classement : BM25 et sens, fusion par rang, fraîcheur et usage
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.entry(paths.root.clone()).or_default();
    if question.is_some() && e.sens_indisponible.is_some() && !e.vectorisation {
        // le modèle répond de nouveau (installé après le lancement, Ollama relancé) : vecteurs des faits en tâche de fond
        e.sens_indisponible = None;
        e.signale = false;
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            let p = paths.clone();
            h.spawn(async move {
                vectoriser(&p).await;
            });
        }
    }
    let candidats: Vec<&Fait> = e.faits.iter().filter(|x| !exclure.contains(x.texte.as_str())).collect();
    let q = mots(requete);
    let documents: Vec<&[String]> = candidats.iter().map(|x| x.mots.as_slice()).collect();
    let scores_mots = bm25(&documents, &q);
    // un mot banal seul (« jour », « projet ») ne suffit pas : part de la question couverte, ou mot rare
    let retenus = retenus_par_les_mots(&documents, &q);
    let mut rang_mots: Vec<(usize, f32)> = scores_mots.iter().copied().enumerate().filter(|(i, s)| *s > 0.0 && retenus[*i]).collect();
    rang_mots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut rang_sens: Vec<(usize, f32)> = match &question {
        Some(vq) => candidats.iter().enumerate().filter_map(|(i, x)| e.vecteurs.get(&x.cle).map(|v| (i, similarite(vq, v)))).filter(|(_, s)| *s >= SEUIL_SENS).collect(),
        None => vec![],
    };
    rang_sens.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut total: BTreeMap<usize, f32> = BTreeMap::new();
    for liste in [&rang_mots, &rang_sens] {
        for (rang, (i, _)) in liste.iter().enumerate() {
            *total.entry(*i).or_default() += 1.0 / (K_RANG + rang as f32 + 1.0);
        }
    }
    let aujourd_hui = chrono::Local::now().date_naive();
    let mut classes: Vec<(usize, f32)> = total
        .into_iter()
        .map(|(i, s)| {
            let f = candidats[i];
            let utilises = e.usage.get(&f.cle).map(|u| u.1).unwrap_or(0).min(5) as f32;
            (i, s * fraicheur(&f.section, f.date, aujourd_hui) * (1.0 + 0.1 * utilises))
        })
        .collect();
    classes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mots_question: std::collections::HashSet<&String> = q.iter().filter(|m| !m.ends_with('~')).collect();
    let resultats: Vec<Recalled> = classes
        .into_iter()
        .take(max)
        .map(|(i, score)| {
            let f = candidats[i];
            let communs: Vec<&str> = f.mots.iter().filter(|m| mots_question.contains(m)).map(|m| m.as_str()).collect();
            let mut raison = vec![];
            if !communs.is_empty() {
                raison.push(format!("mots : {}", communs.join(", ")));
            } else if rang_mots.iter().any(|(j, _)| *j == i) {
                raison.push("mots proches".to_string());
            }
            if let Some((_, s)) = rang_sens.iter().find(|(j, _)| *j == i) {
                raison.push(format!("sens {:.0} %", s * 100.0));
            }
            Recalled { file: f.fichier.clone(), section: f.section.clone(), text: f.texte.clone(), score, reason: raison.join(" · ") }
        })
        .collect();
    // 4. usage : fois rappelé (gardé sur disque avec le reste de l'index)
    for r in &resultats {
        e.usage.entry(cle(&r.text)).or_default().0 += 1;
    }
    resultats
}

/// Similarité de sens de chaque fait avec une question, de la plus forte à la plus faible
/// (mesure et réglage du rappel ; vide si le sens est indisponible).
pub async fn sens_de(paths: &Paths, requete: &str, delai: Duration) -> Vec<(String, f32)> {
    let sens = {
        let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
        let e = g.entry(paths.root.clone()).or_default();
        rafraichir(paths, e);
        e.sens.clone()
    };
    if !sens.actif {
        return vec![];
    }
    let Ok(Ok(mut v)) = tokio::time::timeout(delai, plonger(&sens, &[format!("{CONSIGNE_QUESTION}{requete}")], delai)).await else { return vec![] };
    let Some(vq) = v.pop().flatten() else { return vec![] };
    let g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let Some(e) = g.get(&paths.root) else { return vec![] };
    let mut out: Vec<(String, f32)> = e.faits.iter().filter_map(|f| e.vecteurs.get(&f.cle).map(|vf| (f.texte.clone(), similarite(&vq, vf)))).collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Fait voisin d'un nouveau fait (consolidation, lot M3).
#[derive(Clone, Debug)]
pub struct Voisin {
    pub section: String,
    /// Texte de la ligne sans le tiret (« (date) fait »).
    pub texte: String,
    /// Part des mots utiles du nouveau fait retrouvés dans celui-ci (radicaux de 5 lettres compris).
    pub mots: f32,
    pub communs: usize,
    /// Similarité de sens, si les deux vecteurs existent.
    pub sens: Option<f32>,
}

/// Les `n` faits de `orchestra.md` les plus proches d'un texte, par les mots et par le sens.
pub async fn voisins(paths: &Paths, texte: &str, n: usize, delai_sens: Duration) -> Vec<Voisin> {
    let sens = {
        let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
        let e = g.entry(paths.root.clone()).or_default();
        rafraichir(paths, e);
        e.sens.clone()
    };
    // vecteur du nouveau fait, calculé comme ceux des faits (sans consigne de question)
    let vecteur = if sens.actif && !texte.trim().is_empty() && !delai_sens.is_zero() {
        tokio::time::timeout(delai_sens, plonger(&sens, &[texte.to_string()], delai_sens + Duration::from_millis(50)))
            .await
            .ok()
            .and_then(|r| r.ok())
            .and_then(|mut v| v.pop().flatten())
    } else {
        None
    };
    let mots_du_fait = crate::memory::keywords(texte);
    let radical = |m: &str| m.chars().take(5).collect::<String>();
    let g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let Some(e) = g.get(&paths.root) else { return vec![] };
    let mut out: Vec<Voisin> = e
        .faits
        .iter()
        .filter(|f| f.fichier == "orchestra.md")
        .map(|f| {
            let siens = crate::memory::keywords(&f.texte);
            let communs = mots_du_fait.iter().filter(|m| siens.iter().any(|s| s == *m || (m.chars().count() >= 5 && s.starts_with(&radical(m))))).count();
            let mots = if mots_du_fait.is_empty() { 0.0 } else { communs as f32 / mots_du_fait.len() as f32 };
            let sens = vecteur.as_ref().and_then(|vq| e.vecteurs.get(&f.cle).map(|vf| similarite(vq, vf)));
            Voisin { section: f.section.clone(), texte: f.texte.clone(), mots, communs, sens }
        })
        .filter(|v| v.communs > 0 || v.sens.is_some_and(|s| s >= SEUIL_SENS))
        .collect();
    let note = |v: &Voisin| v.mots.max(v.sens.unwrap_or(0.0));
    out.sort_by(|a, b| note(b).partial_cmp(&note(a)).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(n);
    out
}

/// Après la réponse : un fait rappelé dont la réponse reprend au moins deux mots est « utilisé ».
pub fn noter_utilisation(paths: &Paths, rappeles: &[Recalled], reponse: &str) {
    let r: std::collections::HashSet<String> = crate::memory::keywords(reponse).into_iter().collect();
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.entry(paths.root.clone()).or_default();
    for f in rappeles {
        let communs = crate::memory::keywords(&f.text).iter().filter(|m| r.contains(*m)).count();
        if communs >= 2 {
            e.usage.entry(cle(&f.text)).or_default().1 += 1;
        }
    }
    if !rappeles.is_empty() {
        ecrire_index(paths, e);
    }
}

/// Calcule les vecteurs manquants (à l'écriture de faits, ou au démarrage). Renvoie le nombre calculé.
pub async fn vectoriser(paths: &Paths) -> usize {
    let (sens, manquants) = {
        let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
        let e = g.entry(paths.root.clone()).or_default();
        rafraichir(paths, e);
        if !e.sens.actif || e.vectorisation {
            return 0;
        }
        let mut vus = std::collections::HashSet::new();
        let manquants: Vec<(String, String)> = e.faits.iter().filter(|x| !e.vecteurs.contains_key(&x.cle) && vus.insert(x.cle.clone())).map(|x| (x.cle.clone(), x.texte.clone())).collect();
        if manquants.is_empty() {
            return 0;
        }
        e.vectorisation = true;
        (e.sens.clone(), manquants)
    };
    let mut calcules = 0;
    let mut erreur = None;
    for lot in manquants.chunks(32) {
        let textes: Vec<String> = lot.iter().map(|(_, t)| t.clone()).collect();
        match plonger(&sens, &textes, Duration::from_secs(120)).await {
            Ok(vs) => {
                let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
                let e = g.entry(paths.root.clone()).or_default();
                for ((k, _), v) in lot.iter().zip(vs) {
                    if let Some(v) = v {
                        e.vecteurs.insert(k.clone(), v);
                        calcules += 1;
                    }
                }
            }
            Err(x) => {
                erreur = Some(x);
                break;
            }
        }
    }
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.entry(paths.root.clone()).or_default();
    e.vectorisation = false;
    e.sens_indisponible = erreur;
    if calcules > 0 {
        ecrire_index(paths, e);
    }
    calcules
}

/// Lance `vectoriser` en tâche de fond (après l'écriture de faits), si un moteur tokio tourne.
pub fn vectoriser_en_fond(paths: &Paths) {
    if let Ok(h) = tokio::runtime::Handle::try_current() {
        let p = paths.clone();
        h.spawn(async move {
            vectoriser(&p).await;
        });
    }
}

/// Oublie l'état d'un dossier (recréé par l'évaluation) ; les vecteurs déjà calculés sont gardés,
/// puisqu'ils ne dépendent que du texte des faits.
pub fn oublier(paths: &Paths) {
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = g.get_mut(&paths.root) {
        let (vecteurs, sens) = (std::mem::take(&mut e.vecteurs), e.sens.clone());
        *e = Etat { vecteurs, sens, ..Etat::default() };
    }
}

/// Raison de l'indisponibilité du sens, une seule fois par lancement (pour Conscience).
pub fn a_signaler(paths: &Paths) -> Option<String> {
    let mut g = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.get_mut(&paths.root)?;
    if e.sens.actif && !e.signale {
        if let Some(m) = e.sens_indisponible.clone() {
            e.signale = true;
            return Some(m);
        }
    }
    None
}

/// Pourquoi le sens est indisponible (modèle absent, Ollama arrêté…), s'il l'est.
pub fn sens_indisponible(paths: &Paths) -> Option<String> {
    INDEX.lock().unwrap_or_else(|e| e.into_inner()).get(&paths.root).and_then(|e| e.sens_indisponible.clone())
}

// ---------- .index.bin : vecteurs et usage, supprimable ----------

const MAGIQUE: &[u8; 8] = b"ORIDX01\n";

fn ecrire_index(paths: &Paths, e: &Etat) {
    let mut b: Vec<u8> = MAGIQUE.to_vec();
    let modele = e.sens.modele.as_bytes();
    b.extend((modele.len() as u16).to_le_bytes());
    b.extend(modele);
    b.extend((DIMENSIONS as u16).to_le_bytes());
    b.extend((e.vecteurs.len() as u32).to_le_bytes());
    for (k, v) in &e.vecteurs {
        b.extend(k.as_bytes().iter().copied().chain(std::iter::repeat(b' ')).take(32));
        b.extend(v.echelle.to_le_bytes());
        b.extend(v.q.iter().map(|x| *x as u8));
    }
    b.extend((e.usage.len() as u32).to_le_bytes());
    for (k, (r, u)) in &e.usage {
        b.extend(k.as_bytes().iter().copied().chain(std::iter::repeat(b' ')).take(32));
        b.extend(r.to_le_bytes());
        b.extend(u.to_le_bytes());
    }
    let f = chemin_index(paths);
    // écrit par Orchestra : jamais compté comme une modification par un agent (lot S-b)
    crate::surveillance::noter(paths, &f);
    let _ = std::fs::write(f, b);
}

type Lu = (String, HashMap<String, Vecteur>, HashMap<String, (u32, u32)>);

fn lire_index(f: &Path) -> Option<Lu> {
    let b = std::fs::read(f).ok()?;
    let mut i = 0usize;
    let mut prendre = |n: usize| -> Option<Vec<u8>> {
        let s = b.get(i..i + n)?.to_vec();
        i += n;
        Some(s)
    };
    if prendre(8)? != MAGIQUE {
        return None;
    }
    let l = u16::from_le_bytes(prendre(2)?.try_into().ok()?) as usize;
    let modele = String::from_utf8(prendre(l)?).ok()?;
    let dims = u16::from_le_bytes(prendre(2)?.try_into().ok()?) as usize;
    if dims != DIMENSIONS {
        return None;
    }
    let n = u32::from_le_bytes(prendre(4)?.try_into().ok()?) as usize;
    let mut vecteurs = HashMap::with_capacity(n.min(1_000_000));
    for _ in 0..n {
        let k = String::from_utf8(prendre(32)?).ok()?.trim().to_string();
        let echelle = f32::from_le_bytes(prendre(4)?.try_into().ok()?);
        let q = prendre(dims)?.into_iter().map(|x| x as i8).collect();
        vecteurs.insert(k, Vecteur { q, echelle });
    }
    let n = u32::from_le_bytes(prendre(4)?.try_into().ok()?) as usize;
    let mut usage = HashMap::with_capacity(n.min(1_000_000));
    for _ in 0..n {
        let k = String::from_utf8(prendre(32)?).ok()?.trim().to_string();
        let r = u32::from_le_bytes(prendre(4)?.try_into().ok()?);
        let u = u32::from_le_bytes(prendre(4)?.try_into().ok()?);
        usage.insert(k, (r, u));
    }
    Some((modele, vecteurs, usage))
}

fn chemin_index(paths: &Paths) -> PathBuf {
    paths.memoire.join(".index.bin")
}

fn relatif(paths: &Paths, p: &Path) -> String {
    p.strip_prefix(&paths.memoire).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dossier() -> (Paths, PathBuf) {
        let racine = std::env::temp_dir().join(format!("orchestra-m2-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        (Paths::at(racine.clone()).unwrap(), racine)
    }

    fn ecrire_memoire(p: &Paths) {
        std::fs::write(
            p.main_memory(),
            "# Mémoire\n\n## Utilisateur\n- (2026-10-27) PB est allergique aux arachides.\n- (2026-10-25) L'ordinateur de PB s'appelle Zeus.\n\n## Préférences\n- (2026-10-28) PB boit du thé vert, jamais de café.\n\n## Projets\n- (2026-10-25) Orchestra est un orchestrateur d'agents local.\n- (2025-01-10) Le site de la chorale a été refait en janvier.\n\n## Connaissances\n- (2026-10-27) Le serveur voix d'Orchestra écoute sur le port 8765.\n\n## Agents de confiance\n- (2026-10-30) « rédiger un mail » → agent-brouillons (Brouillons) · réussi 2×\n",
        )
        .unwrap();
        std::fs::write(p.memoire_agents.join("agent-brouillons.md"), "# Mémoire de Brouillons\n\n## Rôle\n- Rédige des brouillons de mails.\n\n## Missions\n- (2026-10-30) Mission : mail à Camille → termine — stand B12 retenu\n").unwrap();
    }

    #[test]
    fn mots_et_radicaux() {
        let m = mots("PB est allergique aux arachides");
        assert!(m.contains(&"allergique".to_string()) && m.contains(&"arachides".to_string()), "{m:?}");
        // radical commun : « allergie » retrouve « allergique »
        let q = mots("une allergie ?");
        assert!(q.iter().any(|x| m.contains(x)), "{q:?} / {m:?}");
        assert!(mots("les des une").is_empty());
    }

    #[test]
    fn bm25_classe_le_fait_le_plus_pertinent() {
        let docs: Vec<Vec<String>> = ["PB est allergique aux arachides", "PB court le samedi matin autour du lac", "La réunion d'équipe a lieu le mardi matin"].iter().map(|t| mots(t)).collect();
        let s = bm25(&docs, &mots("allergie aux arachides"));
        assert_eq!(s.len(), 3);
        assert!(s[0] > 0.0 && s[1] == 0.0 && s[2] == 0.0, "{s:?}");
        // un mot rare pèse plus qu'un mot fréquent
        let s = bm25(&docs, &mots("samedi matin"));
        assert!(s[1] > s[2] && s[2] > 0.0, "{s:?}");
    }

    #[test]
    fn vecteur_reduit_normalise_et_range() {
        let mut v: Vec<f32> = (0..1024).map(|i| ((i * 7 % 13) as f32 - 6.0) / 10.0).collect();
        v[0] = 3.0;
        let q = quantifier(&v).unwrap();
        assert_eq!(q.q.len(), DIMENSIONS);
        assert!((similarite(&q, &q) - 1.0).abs() < 0.02, "{}", similarite(&q, &q));
        let mut w = v.clone();
        for x in w.iter_mut().take(DIMENSIONS).skip(128) {
            *x = -*x;
        }
        let qw = quantifier(&w).unwrap();
        // similarité calculée sur les 256 premières dimensions renormalisées
        let a: Vec<f32> = v[..DIMENSIONS].to_vec();
        let b: Vec<f32> = w[..DIMENSIONS].to_vec();
        let n = |x: &[f32]| x.iter().map(|y| y * y).sum::<f32>().sqrt();
        let attendu = a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>() / (n(&a) * n(&b));
        assert!((similarite(&q, &qw) - attendu).abs() < 0.02, "{} vs {attendu}", similarite(&q, &qw));
        assert!(quantifier(&[0.0; 300]).is_none(), "vecteur nul");
        assert!(quantifier(&[1.0; 10]).is_none(), "trop court");
    }

    #[test]
    fn fraicheur_des_projets_et_connaissances() {
        let aujourd_hui = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let recent = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let il_y_a_90_jours = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let tres_vieux = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        assert!((fraicheur("Projets", Some(recent), aujourd_hui) - 1.0).abs() < 0.03);
        assert!((fraicheur("Connaissances", Some(il_y_a_90_jours), aujourd_hui) - 0.5).abs() < 0.01);
        assert_eq!(fraicheur("Projets", Some(tres_vieux), aujourd_hui), POIDS_MIN);
        // le reste ne vieillit pas
        assert_eq!(fraicheur("Utilisateur", Some(tres_vieux), aujourd_hui), 1.0);
        assert_eq!(fraicheur("Projets", None, aujourd_hui), 1.0);
    }

    #[test]
    fn seuls_les_fichiers_changes_sont_relus() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        let f = faits(&p);
        assert!(f.iter().any(|x| x.texte.contains("arachides") && x.section == "Utilisateur" && x.date == NaiveDate::from_ymd_opt(2026, 10, 27)));
        assert!(f.iter().any(|x| x.fichier == "agents/agent-brouillons.md" && x.section == "Missions"));
        assert!(!f.iter().any(|x| x.section == "Agents de confiance" || x.section == "Rôle"), "sections internes exclues");
        // une ligne ajoutée à la main est vue au rappel suivant ; un fichier supprimé disparaît
        let mut t = std::fs::read_to_string(p.main_memory()).unwrap();
        t.push_str("\n## Connaissances\n- (2026-11-01) Moka est le chat de PB.\n");
        std::fs::write(p.main_memory(), t).unwrap();
        std::fs::remove_file(p.memoire_agents.join("agent-brouillons.md")).unwrap();
        let f = faits(&p);
        assert!(f.iter().any(|x| x.texte.contains("Moka")));
        assert!(!f.iter().any(|x| x.fichier.starts_with("agents/")));
        let _ = std::fs::remove_dir_all(racine);
    }

    /// Faux Ollama : le vecteur d'un texte dépend de ses « thèmes » (boisson, allergie, voix…),
    /// pour vérifier le rappel par le sens sans le vrai modèle.
    async fn faux_ollama(appels: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> String {
        faux_ollama_avec(appels, None).await
    }

    /// Même faux Ollama ; tant que `absent` est vrai, il répond comme si le modèle n'était pas installé.
    async fn faux_ollama_avec(appels: std::sync::Arc<std::sync::atomic::AtomicUsize>, absent: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", ecoute.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut flux, _)) = ecoute.accept().await {
                let appels = appels.clone();
                let absent = absent.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 1 << 20];
                    let mut lu = 0;
                    let corps = loop {
                        let n = flux.read(&mut buf[lu..]).await.unwrap_or(0);
                        if n == 0 {
                            break String::new();
                        }
                        lu += n;
                        let t = String::from_utf8_lossy(&buf[..lu]).to_string();
                        if let Some((tete, reste)) = t.split_once("\r\n\r\n") {
                            let taille: usize = tete.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|x| x.trim().parse().unwrap_or(0))).unwrap_or(0);
                            if reste.len() >= taille {
                                break reste.to_string();
                            }
                        }
                    };
                    appels.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if absent.as_ref().is_some_and(|a| a.load(std::sync::atomic::Ordering::SeqCst)) {
                        let rep = r#"{"error":"model \"faux\" not found, try pulling it first"}"#;
                        let _ = flux.write_all(format!("HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{rep}", rep.len()).as_bytes()).await;
                        return;
                    }
                    let v: serde_json::Value = serde_json::from_str(&corps).unwrap_or_default();
                    let themes = [["the", "boisson", "cafe", "chaude"], ["allerg", "arachide", "cacahuete", "manger"], ["voix", "8765", "port", "ecoute"], ["chorale", "site", "janvier", "refait"]];
                    let vecteur = |t: &str| -> Vec<f32> {
                        let f = crate::memory::fold(t);
                        let mut out = vec![0.01f32; 1024];
                        for (i, th) in themes.iter().enumerate() {
                            if th.iter().any(|m| f.contains(m)) {
                                out[i * 10] = 1.0;
                            }
                        }
                        out
                    };
                    let entrees: Vec<String> = match &v["input"] {
                        serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                        serde_json::Value::String(s) => vec![s.clone()],
                        _ => vec![],
                    };
                    let rep = serde_json::json!({ "embeddings": entrees.iter().map(|t| vecteur(t)).collect::<Vec<_>>() }).to_string();
                    let _ = flux.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{rep}", rep.len()).as_bytes()).await;
                });
            }
        });
        url
    }

    #[tokio::test]
    async fn rappel_par_le_sens_et_par_les_mots() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        let appels = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = faux_ollama(appels.clone()).await;
        configurer(&p, Sens { actif: true, url, modele: "faux".into() });
        assert!(vectoriser(&p).await >= 6, "vecteurs calculés à l'écriture des faits");
        // aucun mot commun avec le fait : trouvé par le sens, avec sa raison
        let r = rappeler(&p, "Qu'est-ce que je bois le matin, chaud ?", 6, "", Duration::from_secs(5)).await;
        let the = r.iter().find(|x| x.text.contains("thé vert")).unwrap_or_else(|| panic!("{r:?}"));
        assert!(the.reason.contains("sens"), "{}", the.reason);
        // par les mots
        let r = rappeler(&p, "Sur quel port écoute le serveur voix ?", 6, "", Duration::from_secs(5)).await;
        assert!(r[0].text.contains("8765") && r[0].reason.contains("mots"), "{r:?}");
        // profil exclu : déjà dans le prompt
        let r = rappeler(&p, "allergie arachides", 6, "- (2026-10-27) PB est allergique aux arachides.", Duration::from_secs(5)).await;
        assert!(!r.iter().any(|x| x.text.contains("arachides")), "{r:?}");
        // l'index sur disque se reconstruit : supprimé, il est refait sans tout recalculer en double
        assert!(chemin_index(&p).exists());
        let _ = std::fs::remove_dir_all(racine);
    }

    #[tokio::test]
    async fn sans_modele_d_embedding_le_rappel_reste_par_les_mots() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        configurer(&p, Sens { actif: true, url: "http://127.0.0.1:9".into(), modele: "absent".into() });
        let r = rappeler(&p, "Sur quel port écoute le serveur voix ?", 6, "", Duration::from_millis(500)).await;
        assert!(r.first().is_some_and(|x| x.text.contains("8765")), "{r:?}");
        assert!(r.iter().all(|x| !x.reason.contains("sens")));
        let _ = std::fs::remove_dir_all(racine);
    }

    #[tokio::test]
    async fn modele_d_embedding_sur_le_processeur_avec_un_petit_contexte() {
        // mesuré le 01/11 : contexte par défaut = 2,7 Gio de mémoire vive gardés ; 2 048 tokens = 0,8 Gio, même vecteur
        let (url, vues) = crate::essais::faux_chat(vec!["x".into()]).await;
        let _ = plonger(&Sens { actif: true, url, modele: "m".into() }, &["un fait".into()], Duration::from_secs(2)).await;
        let corps = vues.lock().unwrap()[0].clone();
        assert_eq!(corps["options"], serde_json::json!({ "num_gpu": 0, "num_ctx": 2048 }), "{corps}");
        assert_eq!(corps["keep_alive"], serde_json::json!(-1));
    }

    #[tokio::test]
    async fn un_mot_banal_seul_ne_fait_pas_un_souvenir() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        let noms = ["Albane", "Bastien", "Camille", "Damien", "Elodie", "Fabien", "Gaelle", "Hugues", "Ines", "Jules", "Karima", "Lucien", "Maelle", "Nadir", "Oriane", "Pascal", "Quentin", "Rosalie", "Simon", "Thibault"];
        let lignes: String = noms.iter().enumerate().map(|(i, n)| format!("- (2026-10-{:02}) Mission : devis de {n} → termine — chiffres mis à jour\n", 1 + i)).collect();
        std::fs::write(p.memoire_agents.join("agent-devis.md"), format!("# Mémoire de Devis\n\n## Missions\n{lignes}")).unwrap();
        configurer(&p, Sens::default());
        // « jour » est partout, « copie », « sécurité », « hebdomadaire » nulle part : aucun de ces faits ne répond
        let r = rappeler(&p, "Quel jour a lieu la copie de sécurité hebdomadaire ?", 6, "", Duration::ZERO).await;
        assert!(r.iter().all(|x| !x.text.contains("mis à jour")), "{r:?}");
        // une question qui couvre bien un fait le retrouve toujours
        let r = rappeler(&p, "Le devis de Camille a-t-il été mis à jour ?", 6, "", Duration::ZERO).await;
        assert!(r.first().is_some_and(|x| x.text.contains("devis de Camille")), "{r:?}");
        // question longue, la plupart de ses mots absents de la mémoire, mais un mot rare et juste :
        // le fait est retrouvé (vu sur le jeu de rappel : « Quelle licence empêche de vendre… »)
        let mut m = std::fs::read_to_string(p.main_memory()).unwrap();
        m.push_str("\n## Connaissances\n- (2026-10-27) Voxtral TTS est sous licence CC BY-NC.\n");
        std::fs::write(p.main_memory(), m).unwrap();
        let r = rappeler(&p, "Quelle licence empêche de vendre un produit avec la voix de Mistral ?", 6, "", Duration::ZERO).await;
        assert!(r.first().is_some_and(|x| x.text.contains("CC BY-NC")), "{r:?}");
        let _ = std::fs::remove_dir_all(racine);
    }

    #[tokio::test]
    async fn voisins_d_un_nouveau_fait() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        // par les mots seuls
        configurer(&p, Sens::default());
        let v = voisins(&p, "Le serveur voix écoute maintenant sur le port 8800.", 5, Duration::ZERO).await;
        let premier = v.first().unwrap_or_else(|| panic!("{v:?}"));
        assert!(premier.texte.contains("8765") && premier.communs >= 3 && premier.mots >= 0.5 && premier.sens.is_none(), "{v:?}");
        assert!(v.iter().all(|x| !x.texte.contains("Rédige") && !x.texte.contains("agent-brouillons")), "seulement orchestra.md, hors sections internes : {v:?}");
        assert!(v.len() <= 5);
        // par le sens : aucun mot commun avec le fait de l'allergie
        let appels = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = faux_ollama(appels).await;
        configurer(&p, Sens { actif: true, url, modele: "faux".into() });
        vectoriser(&p).await;
        let v = voisins(&p, "Interdit de manger des cacahuètes.", 5, Duration::from_secs(5)).await;
        let a = v.iter().find(|x| x.texte.contains("arachides")).unwrap_or_else(|| panic!("{v:?}"));
        assert!(a.sens.is_some_and(|s| s > 0.9), "{a:?}");
        let _ = std::fs::remove_dir_all(racine);
    }

    #[tokio::test]
    async fn modele_installe_apres_le_lancement_les_vecteurs_suivent() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        let appels = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let absent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let url = faux_ollama_avec(appels, Some(absent.clone())).await;
        configurer(&p, Sens { actif: true, url, modele: "faux".into() });
        // au lancement, le modèle n'est pas installé : rien de calculé, la raison est gardée
        assert_eq!(vectoriser(&p).await, 0);
        assert!(sens_indisponible(&p).is_some_and(|m| m.contains("not found")), "{:?}", sens_indisponible(&p));
        // PB lance « ollama pull » sans relancer Orchestra : le rappel suivant s'en aperçoit
        absent.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = rappeler(&p, "Qu'est-ce que je bois le matin, chaud ?", 6, "", Duration::from_secs(5)).await;
        let mut trouve = false;
        for _ in 0..100 {
            let r = rappeler(&p, "Qu'est-ce que je bois le matin, chaud ?", 6, "", Duration::from_secs(5)).await;
            if r.iter().any(|x| x.text.contains("thé vert") && x.reason.contains("sens")) {
                trouve = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(trouve, "vecteurs des faits calculés après l'installation du modèle");
        assert!(sens_indisponible(&p).is_none());
        let _ = std::fs::remove_dir_all(racine);
    }

    #[tokio::test]
    async fn faits_utilises_favorises_et_vieux_projets_moins() {
        let (p, racine) = dossier();
        ecrire_memoire(&p);
        configurer(&p, Sens::default());
        let r = rappeler(&p, "orchestrateur chorale", 6, "", Duration::from_millis(10)).await;
        let rang = |r: &[Recalled], m: &str| r.iter().position(|x| x.text.contains(m));
        // le projet vieux de près de deux ans passe après le récent, à mots égaux
        assert!(rang(&r, "orchestrateur").unwrap() < rang(&r, "chorale").unwrap(), "{r:?}");
        let score = |r: &[Recalled]| r.iter().find(|x| x.text.contains("chorale")).map(|x| x.score).unwrap_or(0.0);
        let avant = score(&r);
        // utilisé plusieurs fois dans les réponses : il remonte ; rappelé sans être utilisé : rien ne change
        for _ in 0..5 {
            let r = rappeler(&p, "orchestrateur chorale", 6, "", Duration::from_millis(10)).await;
            noter_utilisation(&p, &r, "Le site de la chorale a été refait en janvier.");
        }
        let r = rappeler(&p, "orchestrateur chorale", 6, "", Duration::from_millis(10)).await;
        assert!(score(&r) > avant * 1.3, "{} → {}", avant, score(&r));
        let orchestra = |r: &[Recalled]| r.iter().find(|x| x.text.contains("orchestrateur")).map(|x| x.score).unwrap_or(0.0);
        let r2 = rappeler(&p, "orchestrateur chorale", 6, "", Duration::from_millis(10)).await;
        assert!((orchestra(&r2) - orchestra(&r)).abs() < 1e-6, "rappelé sans être utilisé : même score");
        let _ = std::fs::remove_dir_all(racine);
    }
}
