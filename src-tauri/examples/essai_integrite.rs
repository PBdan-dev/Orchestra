//! Essai du lot T6 : traces chaînées, puis trafiquées, puis vérifiées comme au lancement.
//! Une mission passe par la vraie porte du modèle (réponse fixe, sans GPU) pour produire un
//! vrai `echanges.jsonl` chaîné ; le journal est écrit comme par l'application.
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example essai_integrite
//! Dossier de test (effacé au début) : `Orchestra/eval/essai-integrite/`.

use orchestra_lib::{agents, config::Config, contrat, integrite, paths::Paths, porte, verification};
use serde_json::{json, Value};

fn ok(cond: bool, quoi: &str, bilan: &mut (u32, u32)) {
    println!("{} {quoi}", if cond { "OK " } else { "KO " });
    if cond { bilan.0 += 1 } else { bilan.1 += 1 }
}

fn lignes(f: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(f).unwrap_or_default().lines().map(String::from).collect()
}

fn reecrire(f: &std::path::Path, l: &[String]) {
    std::fs::write(f, l.iter().map(|x| format!("{x}\n")).collect::<String>()).unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let reel = Paths::init()?;
    let base = reel.root.join("eval").join("essai-integrite");
    let _ = std::fs::remove_dir_all(&base);
    let paths = Paths::at(base.join("Orchestra"))?;
    let mut bilan = (0, 0);

    // 1. Mission d'agent-brouillons par la vraie porte (réponse fixe du modèle)
    let mut m = agents::get_installed(&reel, "agent-brouillons").ok_or_else(|| anyhow::anyhow!("agent-brouillons absent"))?;
    // copie de l'agent dans le dossier d'essai, pour ne pas toucher sa vraie mémoire
    let copie = agents::dossier_installe(&paths, &m.id);
    copier(m.dossier.as_ref().unwrap(), &copie)?;
    m = agents::get_installed(&paths, &m.id).unwrap();
    let projet = base.join("projet");
    std::fs::create_dir_all(&projet)?;
    let mission = contrat::mission(
        &m,
        contrat::Contenu {
            projet: &projet,
            type_tache: Some("brouillon-mail".into()),
            but: "Brouillon de remerciement",
            critere_reussite: "brouillon écrit",
            contexte: "",
            donnees: json!({ "destinataire": "Camille", "objet": "Merci", "points": ["merci pour la réunion"] }),
            demande_originale: "Prépare un brouillon pour remercier Camille",
        },
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let mission_id = mission["id"].as_str().unwrap_or("").to_string();
    let dossier = paths.missions.join(&mission_id);
    std::fs::create_dir_all(&dossier)?;
    std::fs::write(dossier.join("mission.json"), serde_json::to_string_pretty(&mission)?)?;
    let cfg = Config::default();
    let limites = porte::Limites::depuis(&m, Some("brouillon-mail"));
    let config_porte = porte::Config_ { cfg: cfg.clone(), http: reqwest::Client::new(), agent: m.clone(), modele: "qwen3.5:9b".into(), dossier: dossier.clone(), limites, reponse_fixe: Some("Bonjour Camille,\n\nMerci pour la réunion.\n\nPB".into()) };
    let (p, mut notes) = porte::Porte::ouvrir(config_porte).await?;
    let d = contrat::executer(&m, &mission, &dossier, Some(contrat::AvecPorte { porte: &p, notes: &mut notes }), None, |_| vec![], |_| vec![]).await?;
    drop(p);
    let actions: Vec<Value> = d.compte_rendu["actions_declarees"].as_array().cloned().unwrap_or_default();
    integrite::ecrire_tout(&dossier.join("actions.jsonl"), &actions)?;
    println!("  mission {mission_id} : état {}, {} échange(s), {} action(s)", d.compte_rendu["etat"], lignes(&dossier.join("echanges.jsonl")).len(), actions.len());
    ok(integrite::verifier(&dossier.join("echanges.jsonl")) == integrite::Etat::Intacte { lignes: lignes(&dossier.join("echanges.jsonl")).len() } && !lignes(&dossier.join("echanges.jsonl")).is_empty(), "echanges.jsonl écrit par la porte : chaîne intacte", &mut bilan);
    ok(matches!(integrite::verifier(&dossier.join("actions.jsonl")), integrite::Etat::Intacte { .. }), "actions.jsonl : chaîne intacte", &mut bilan);

    // 2. Journal : un ancien jour non chaîné, le jour courant chaîné
    std::fs::write(paths.journal.join("2026-10-29.jsonl"), "{\"at\":\"ancien\"}\n")?;
    let jour = paths.journal.join("2026-10-31.jsonl");
    for i in 0..5 {
        integrite::ajouter(&jour, json!({ "at": format!("t{i}"), "event": { "type": "phase", "label": format!("étape {i}") } }))?;
    }
    let b = integrite::verifier_tout(&paths, None);
    println!("  lancement : {} intacte(s), {} non chaînée(s), {} rupture(s)", b.intactes, b.non_chainees, b.ruptures.len());
    ok(b.ruptures.is_empty() && b.intactes == 3 && b.non_chainees == 1, "au lancement, tout est intact (l'ancien jour est « non chaîné »)", &mut bilan);

    // 3. Un agent trafique la trace de sa mission (une ligne de echanges.jsonl modifiée)
    let f = dossier.join("echanges.jsonl");
    let mut l = lignes(&f);
    l[0] = l[0].replacen("\"n\":1", "\"n\":1,\"trafique\":true", 1);
    reecrire(&f, &l);
    let b = integrite::verifier_tout(&paths, Some(&mission_id));
    println!("  {:?}", b.ruptures);
    let r = b.ruptures.first();
    ok(r.is_some_and(|r| r.agent_id.as_deref() == Some("agent-brouillons")), "cassure dans le dossier de mission détectée, agent de la mission désigné", &mut bilan);
    ok(r.is_some_and(|r| paths.root.join(&r.preuve).exists()), "copie du fichier cassé mise à l'abri dans integrite/preuves/", &mut bilan);
    let e = verification::etat(&paths, "agent-brouillons");
    ok(e.ecarts == 1 && e.statut == verification::Statut::Retire, "compté comme écart : confiance retirée à l'agent", &mut bilan);
    let b2 = integrite::verifier_tout(&paths, None);
    ok(b2.ruptures.is_empty() && b2.deja_signalees == 1 && verification::etat(&paths, "agent-brouillons").ecarts == 1, "relancement : rupture rappelée, pas comptée deux fois", &mut bilan);

    // 4. Journal : ligne retirée, puis fin coupée → alerte sans agent visé
    let mut l = lignes(&jour);
    l.remove(2);
    reecrire(&jour, &l);
    let b = integrite::verifier_tout(&paths, None);
    println!("  {:?}", b.ruptures);
    ok(b.ruptures.len() == 1 && b.ruptures[0].agent_id.is_none() && b.ruptures[0].fichier.starts_with("journal/"), "ligne retirée du journal : rupture signalée, aucun agent visé", &mut bilan);

    let jour2 = paths.journal.join("2026-11-01.jsonl");
    for i in 0..3 {
        integrite::ajouter(&jour2, json!({ "at": format!("t{i}") }))?;
    }
    let l = lignes(&jour2);
    reecrire(&jour2, &l[..2]);
    let b = integrite::verifier_tout(&paths, None);
    ok(b.ruptures.iter().any(|r| r.motif.contains("coupée")), "fin du journal coupée détectée", &mut bilan);

    println!("\nBilan : {} OK, {} KO", bilan.0, bilan.1);
    if bilan.1 > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn copier(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "__pycache__" || n == ".venv") {
            continue;
        }
        if p.is_dir() {
            copier(&p, &dest.join(e.file_name()))?;
        } else {
            std::fs::copy(&p, dest.join(e.file_name()))?;
        }
    }
    Ok(())
}
