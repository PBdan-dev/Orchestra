//! Banc d'essai des agents (lot T1) : rejoue, à travers Orchestra (`contrat::executer`), les tests
//! livrés avec chaque agent (`tests/*.json`, format du kit de certification de l'Agent Market).
//! Chaque test a son dossier de projet neuf dans `Orchestra/eval/banc/` ; rapport `.md` à côté.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;

use crate::{agents, config::Config, contrat, modele, paths::Paths, porte};

#[derive(Debug, Clone)]
pub struct ResultatTest {
    pub agent: String,
    pub test: String,
    pub description: String,
    pub etat: String,
    pub controles: Vec<(bool, String)>,
    pub duree_ms: f64,
}

impl ResultatTest {
    pub fn reussi(&self) -> bool {
        self.controles.iter().all(|c| c.0)
    }
}

/// Contenu de tous les fichiers d'un dossier (petits dossiers de test).
fn instantane(d: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn parcourir(racine: &Path, d: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    parcourir(racine, &p, out);
                } else if let Ok(c) = std::fs::read(&p) {
                    let rel = p.strip_prefix(racine).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                    out.insert(rel, c);
                }
            }
        }
    }
    parcourir(d, d, &mut out);
    out
}

/// Fichiers créés, modifiés ou supprimés entre deux instantanés.
pub fn changements(avant: &BTreeMap<String, Vec<u8>>, apres: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut c: Vec<String> = apres.iter().filter(|(k, v)| avant.get(*k) != Some(*v)).map(|(k, _)| k.clone()).collect();
    c.extend(avant.keys().filter(|k| !apres.contains_key(*k)).cloned());
    c.sort();
    c.dedup();
    c
}

fn remplacer_projet(v: &mut Value, projet: &str) {
    match v {
        Value::String(s) if s.contains("PROJET_DE_TEST") => *s = s.replace("PROJET_DE_TEST", projet),
        Value::Array(a) => a.iter_mut().for_each(|x| remplacer_projet(x, projet)),
        Value::Object(o) => o.values_mut().for_each(|x| remplacer_projet(x, projet)),
        _ => {}
    }
}

async fn un_test(cfg: &Config, http: &reqwest::Client, original: &agents::Manifest, fichier: &Path, dossier_banc: &Path) -> ResultatTest {
    let m = &mut original.clone();
    let nom = fichier.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let mut r = ResultatTest { agent: m.id.clone(), test: nom.clone(), description: String::new(), etat: String::new(), controles: vec![], duree_ms: 0.0 };
    let t: Value = match std::fs::read_to_string(fichier).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        Some(t) => t,
        None => {
            r.controles.push((false, "fichier de test illisible".into()));
            return r;
        }
    };
    r.description = t["description"].as_str().unwrap_or("").to_string();

    // dossier de projet neuf, avec les fichiers du test
    let base = dossier_banc.join(&m.id).join(&nom);
    let _ = std::fs::remove_dir_all(&base);
    let projet = base.join("projet");
    let _ = std::fs::create_dir_all(&projet);
    for (rel, contenu) in t["fichiers_projet"].as_object().into_iter().flatten() {
        let p = projet.join(rel);
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(p, contenu.as_str().unwrap_or(""));
    }
    // copie de l'agent (sans ses tests) : sa mémoire de test ne touche pas l'agent installé
    let copie = base.join("agent");
    if let Some(src) = &original.dossier {
        if let Err(e) = copier_agent(src, &copie) {
            r.controles.push((false, format!("copie de l'agent impossible : {e}")));
            return r;
        }
    }
    for (rel, contenu) in t["memoire_agent"].as_object().into_iter().flatten() {
        let p = copie.join("memoire").join(rel);
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(p, contenu.as_str().unwrap_or(""));
    }
    m.dossier = Some(copie.clone());
    let avant = instantane(&projet);

    let mut mission = t["mission"].clone();
    remplacer_projet(&mut mission, &projet.to_string_lossy());
    let acceptes: Vec<&str> = t["etats_acceptes"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    if let Err(motif) = contrat::valider_mission(&mission) {
        // mission refusée par Orchestra avant tout lancement
        r.etat = "refusée par Orchestra".into();
        r.controles.push((acceptes.contains(&"echec"), format!("mission refusée avant lancement : {motif}")));
        return r;
    }

    let mission_id = mission["id"].as_str().unwrap_or("").to_string();
    let mut a_envoyer: Vec<Value> = t["messages"].as_array().cloned().unwrap_or_default();
    let debut = Instant::now();
    let dossier_mission = base.join("mission");
    let _ = std::fs::create_dir_all(&dossier_mission);
    // porte du modèle : le modèle d'Orchestra (prêté), ou la réponse fixe du test (`reponse_porte_test`)
    let mut porte_ouverte = None;
    if let Some(modele_agent) = modele::choisir(m, &cfg.model) {
        let limites = porte::Limites::depuis(m, mission["type_tache"].as_str());
        let reponse_fixe = t["reponse_porte_test"].as_str().map(String::from);
        match porte::Porte::ouvrir(porte::Config_ { cfg: cfg.clone(), http: http.clone(), agent: m.clone(), modele: modele_agent, dossier: dossier_mission.clone(), limites, reponse_fixe }).await {
            Ok(p) => porte_ouverte = Some(p),
            Err(e) => {
                r.controles.push((false, format!("porte du modèle impossible : {e}")));
                return r;
            }
        }
    }
    let avec_porte = porte_ouverte.as_mut().map(|(p, n)| contrat::AvecPorte { porte: &*p, notes: n });
    let mut ecarts_porte: Vec<String> = vec![];
    let d = contrat::executer(m, &mission, &dossier_mission, avec_porte, |ev: &Value| {
        let type_ = ev["type"].as_str().unwrap_or("").to_string();
        let mut sortie = vec![];
        a_envoyer.retain(|msg| {
            if msg["apres_evenement"].as_str() == Some(type_.as_str()) {
                let mut x = contrat::message(&mission_id, msg["type"].as_str().unwrap_or(""));
                for k in ["decision", "portee"] {
                    if let Some(v) = msg.get(k) {
                        x[k] = v.clone();
                    }
                }
                sortie.push(x);
                false
            } else {
                true
            }
        });
        sortie
    }, |note: &porte::Note| {
        if let porte::Note::Ecart { texte } | porte::Note::Refus { texte } = note {
            ecarts_porte.push(texte.clone());
        }
        vec![]
    })
    .await;
    let pas = porte_ouverte.as_ref().map(|(p, _)| p.compteurs());
    drop(porte_ouverte);
    r.duree_ms = debut.elapsed().as_secs_f64() * 1000.0;
    let d = match d {
        Ok(d) => d,
        Err(e) => {
            r.controles.push((false, format!("lancement impossible : {e}")));
            return r;
        }
    };
    let cr = &d.compte_rendu;
    r.etat = cr["etat"].as_str().unwrap_or("").to_string();
    if !acceptes.is_empty() {
        r.controles.push((acceptes.contains(&r.etat.as_str()), format!("état « {} » parmi {:?}", r.etat, acceptes)));
    }
    let vus: Vec<&str> = d.evenements.iter().filter_map(|e| e["type"].as_str()).collect();
    for attendu in t["evenements_attendus"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
        r.controles.push((vus.contains(&attendu), format!("événement « {attendu} » reçu")));
    }
    r.controles.push((d.ecarts.is_empty(), if d.ecarts.is_empty() { "aucun écart au contrat".into() } else { format!("écarts : {}", d.ecarts.join(" ; ")) }));
    if let Some((n, tokens)) = pas {
        r.controles.push((ecarts_porte.is_empty(), if ecarts_porte.is_empty() { format!("porte : {n} appel(s) au modèle, {tokens} tokens, aucun écart") } else { format!("porte : {}", ecarts_porte.join(" ; ")) }));
    }
    if contrat::valider_compte_rendu(cr).is_ok() && cr["motif"].as_str().is_none_or(|m| !m.starts_with("compte rendu mal formé")) {
        r.controles.push((true, "compte rendu conforme au schéma".into()));
    } else {
        r.controles.push((false, format!("compte rendu non conforme : {}", cr["motif"].as_str().unwrap_or(""))));
    }
    // fichiers réellement changés dans le projet = fichiers annoncés
    let changes = changements(&avant, &instantane(&projet));
    let annonces: Vec<String> = cr.pointer("/resultat/fichiers").and_then(|f| f.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.replace('\\', "/"))).collect()).unwrap_or_default();
    let caches: Vec<&String> = changes.iter().filter(|c| !annonces.iter().any(|a| a.ends_with(c.as_str()) || c.ends_with(a.as_str()))).collect();
    r.controles.push((caches.is_empty(), if caches.is_empty() { format!("{} fichier(s) changé(s), tous annoncés", changes.len()) } else { format!("fichiers changés non annoncés : {caches:?}") }));
    r
}

fn copier_agent(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let nom = e.file_name();
        if nom == "tests" || nom == "__pycache__" {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            copier_agent(&p, &dest.join(&nom))?;
        } else {
            std::fs::copy(&p, dest.join(&nom))?;
        }
    }
    Ok(())
}

/// Rejoue les tests des agents installés donnés (tous si la liste est vide) ; renvoie le rapport `.md`.
pub async fn rejouer(paths: &Paths, ids: &[String], journal: impl Fn(&str)) -> anyhow::Result<(PathBuf, Vec<ResultatTest>)> {
    let dossier_banc = paths.root.join("eval").join("banc");
    let mut cfg = Config::load(paths);
    if cfg.model.trim().is_empty() {
        cfg.model = "qwen3.6:27b".into();
    }
    let http = reqwest::Client::builder().connect_timeout(crate::brain::DELAI_CONNEXION).build()?;
    std::fs::create_dir_all(&dossier_banc)?;
    let mut resultats = vec![];
    for m in agents::installed(paths).into_iter().filter(|m| ids.is_empty() || ids.contains(&m.id)) {
        let Some(dossier) = m.dossier.clone() else { continue };
        let mut tests: Vec<PathBuf> = std::fs::read_dir(dossier.join("tests")).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect()).unwrap_or_default();
        tests.sort();
        for f in tests {
            journal(&format!("{} · {}", m.id, f.file_name().unwrap_or_default().to_string_lossy()));
            let r = un_test(&cfg, &http, &m, &f, &dossier_banc).await;
            journal(&format!("   {} ({})", if r.reussi() { "réussi" } else { "ÉCHEC" }, r.etat));
            resultats.push(r);
        }
    }
    // le modèle prêté par la porte reste chargé (keep_alive) : le banc le libère en partant
    let _ = http
        .post(format!("{}/api/generate", cfg.ollama_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "model": cfg.model, "keep_alive": 0 }))
        .send()
        .await;
    let ok = resultats.iter().filter(|r| r.reussi()).count();
    let mut md = format!(
        "# Banc d'essai des agents — {}\n\nTests livrés avec chaque agent, rejoués à travers le contrat d'Orchestra (stdio-json). **{ok}/{} réussis.**\n\n| Agent | Test | État | Résultat | Contrôles | Durée (ms) |\n|---|---|---|---|---|---|\n",
        chrono::Local::now().format("%d/%m/%Y %H:%M"),
        resultats.len()
    );
    for r in &resultats {
        let ctrl = r.controles.iter().map(|(ok, c)| format!("{} {}", if *ok { "✅" } else { "❌" }, c.replace('|', "/"))).collect::<Vec<_>>().join("<br>");
        md.push_str(&format!("| {} | {} — {} | {} | {} | {} | {:.0} |\n", r.agent, r.test, r.description, r.etat, if r.reussi() { "✅" } else { "❌" }, ctrl, r.duree_ms));
    }
    let chemin = dossier_banc.join(format!("rapport-{}.md", chrono::Local::now().format("%Y-%m-%d_%H%M")));
    std::fs::write(&chemin, md)?;
    Ok((chemin, resultats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn changements_detectes() {
        let mut a = BTreeMap::new();
        a.insert("x.md".to_string(), b"1".to_vec());
        a.insert("y.md".to_string(), b"2".to_vec());
        let mut b = a.clone();
        b.insert("x.md".into(), b"9".to_vec());
        b.remove("y.md");
        b.insert("z.md".into(), b"3".to_vec());
        assert_eq!(changements(&a, &b), vec!["x.md", "y.md", "z.md"]);
        let mut v = json!({ "projet": "PROJET_DE_TEST", "l": ["PROJET_DE_TEST/a"] });
        remplacer_projet(&mut v, "C:/p");
        assert_eq!(v, json!({ "projet": "C:/p", "l": ["C:/p/a"] }));
    }
}
