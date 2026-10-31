//! Essai du lot T7 : pause puis reprise, arrêt propre puis reprise depuis le point de reprise,
//! arrêt immédiat. Les ordres passent par le pilote, comme depuis les boutons de l'interface.
//!
//! agent-fichiers 1.0.1 (reprise) est pris dans le dossier donné en argument (sinon l'agent
//! installé) ; un agent « lent » est fabriqué à partir d'agent-date. Depuis `src-tauri/` :
//!   cargo run --release --example essai_reprise -- [dossier d'agent-fichiers 1.0.1]
//! Dossier de test (effacé au début) : `Orchestra/eval/essai-reprise/`.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use orchestra_lib::{agents, contrat, paths::Paths, pilote::Pilote};
use serde_json::{json, Value};

const LENT_PY: &str = r#""""Agent lent pour les essais du lot T7 : 6 étapes d'une demi-seconde, ou une attente de 60 s."""
import time
from contrat import Arret, entre_deux_etapes, evenement, lancer, reponse


def executer(mission):
    if mission["donnees"].get("format") == "court":   # « bloqué » : ignore tout pendant 60 s
        evenement(mission, "demarrage", "je bloque")
        time.sleep(60)
        return reponse(mission, "termine", texte="fini")
    for i in range(1, 7):
        try:
            entre_deux_etapes(mission, f"étape {i}")
        except Arret:
            return reponse(mission, "arrete_proprement", texte=f"arrêté avant l'étape {i}", reprise={"etape": i})
        evenement(mission, "etape", f"étape {i}")
        time.sleep(0.5)
    return reponse(mission, "termine", texte="6 étapes")


if __name__ == "__main__":
    lancer(executer)
"#;

fn copier(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "__pycache__" || n == ".venv" || n == "memoire") {
            continue;
        }
        if p.is_dir() { copier(&p, &dest.join(e.file_name()))? } else { std::fs::copy(&p, dest.join(e.file_name())).map(|_| ())? }
    }
    Ok(())
}

fn ok(cond: bool, quoi: &str, bilan: &mut (u32, u32)) {
    println!("{} {quoi}", if cond { "OK " } else { "KO " });
    if cond { bilan.0 += 1 } else { bilan.1 += 1 }
}

/// Lance une mission avec un pilote ; `ordres(evenement)` renvoie les ordres à donner (comme PB).
async fn lancer(m: &agents::Manifest, mission: &Value, dossier: &std::path::Path, pilote: &Arc<Pilote>, inter: &Arc<AtomicBool>, mut ordres: impl FnMut(&Value) -> Vec<&'static str>) -> anyhow::Result<(Value, Vec<String>)> {
    let id = mission["id"].as_str().unwrap_or("").to_string();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    pilote.commencer(&id, tx, Some(inter.clone()));
    let mut types = vec![];
    let p2 = pilote.clone();
    let d = contrat::executer(m, mission, dossier, None, Some(&mut rx), |ev| {
        types.push(ev["type"].as_str().unwrap_or("").to_string());
        for o in ordres(ev) {
            if o == "valider" {
                return vec![contrat::reponse_validation(&id, true, "cette_fois")];
            }
            let _ = p2.commander(&id, o);
        }
        vec![]
    }, |_| vec![]).await?;
    pilote.finir(&id);
    Ok((d.compte_rendu, types))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let reel = Paths::init()?;
    let base = reel.root.join("eval").join("essai-reprise");
    let _ = std::fs::remove_dir_all(&base);
    let paths = Paths::at(base.join("Orchestra"))?;
    let mut bilan = (0, 0);
    let pilote = Arc::new(Pilote::default());
    let inter = Arc::new(AtomicBool::new(false));

    // agents : agent-fichiers 1.0.1 et l'agent lent
    let src_fichiers = std::env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| agents::dossier_installe(&reel, "agent-fichiers"));
    copier(&src_fichiers, &agents::dossier_installe(&paths, "agent-fichiers"))?;
    let fichiers = agents::get_installed(&paths, "agent-fichiers").ok_or_else(|| anyhow::anyhow!("agent-fichiers illisible"))?;
    println!("agent-fichiers {}", fichiers.version);
    let dl = agents::dossier_installe(&paths, "agent-lent");
    copier(&agents::dossier_installe(&reel, "agent-date"), &dl)?;
    let mut man: Value = serde_json::from_str(&std::fs::read_to_string(dl.join("manifeste.json"))?)?;
    man["id"] = json!("agent-lent");
    man["name"] = json!("Agent lent");
    std::fs::write(dl.join("manifeste.json"), man.to_string())?;
    std::fs::write(dl.join("agent.py"), LENT_PY)?;
    let lent = agents::get_installed(&paths, "agent-lent").unwrap();
    let projet = base.join("projet");
    std::fs::create_dir_all(&projet)?;
    let mission_lente = |format: &str| contrat::mission(&lent, contrat::Contenu { projet: &projet, type_tache: Some("date-du-jour".into()), but: "attendre", critere_reussite: "attente terminée", contexte: "", donnees: json!({ "format": format }), demande_originale: "attends" });

    // 1. Pause au 1er événement, reprise 2 s plus tard (aucun délai de silence pendant la pause)
    let mission = mission_lente("long").map_err(|e| anyhow::anyhow!(e))?;
    let id = mission["id"].as_str().unwrap().to_string();
    let (p3, i3, id3) = (pilote.clone(), inter.clone(), id.clone());
    let mut pause_vue = false;
    let mut suspendue_pendant_pause = false;
    let t0 = Instant::now();
    let (cr, types) = lancer(&lent, &mission, &paths.missions.join(&id), &pilote, &inter, |ev| {
        if ev["type"] == "demarrage" || (ev["type"] == "etape" && !pause_vue && ev["message"] == "étape 1") {
            if !pause_vue {
                pause_vue = true;
                return vec!["pause"];
            }
        }
        if ev["type"] == "en_pause" {
            suspendue_pendant_pause = i3.load(std::sync::atomic::Ordering::SeqCst);
            let (p, id) = (p3.clone(), id3.clone());
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = p.commander(&id, "reprendre");
            });
        }
        vec![]
    })
    .await?;
    println!("  pause : {} en {:.1} s, événements {:?}", cr["etat"], t0.elapsed().as_secs_f64(), types);
    ok(types.contains(&"en_pause".to_string()) && cr["etat"] == "termine" && t0.elapsed() >= Duration::from_secs(2), "pause puis reprise : l'agent attend, puis termine", &mut bilan);
    ok(suspendue_pendant_pause && !inter.load(std::sync::atomic::Ordering::SeqCst), "porte suspendue pendant la pause, rouverte à la reprise", &mut bilan);

    // 2. Arrêt propre pendant la validation, puis reprise depuis le point de reprise
    for (f, c) in [("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")] {
        std::fs::write(projet.join(f), c)?;
    }
    let ops = json!({ "operations": [
        { "type": "renommer", "source": "a.txt", "destination": "a2.txt" },
        { "type": "supprimer_definitivement", "source": "b.txt", "raison": "obsolète" },
        { "type": "renommer", "source": "c.txt", "destination": "c2.txt" }
    ] });
    let mission = contrat::mission(&fichiers, contrat::Contenu { projet: &projet, type_tache: Some("operations-fichiers".into()), but: "Ranger", critere_reussite: "trois opérations", contexte: "", donnees: ops, demande_originale: "range" }).map_err(|e| anyhow::anyhow!(e))?;
    let id = mission["id"].as_str().unwrap().to_string();
    let (cr, _) = lancer(&fichiers, &mission, &paths.missions.join(&id), &pilote, &inter, |ev| if ev["type"] == "validation_requise" { vec!["arret"] } else { vec![] }).await?;
    println!("  arrêt : {} · point de reprise {}", cr["etat"], cr["point_de_reprise"]);
    ok(cr["etat"] == "arrete_proprement" && cr["point_de_reprise"]["prochaine_operation"] == 2 && projet.join("a2.txt").exists() && projet.join("b.txt").exists(), "arrêt propre : 1re opération faite, point de reprise à l'opération 2", &mut bilan);
    inter.store(false, std::sync::atomic::Ordering::SeqCst);
    let reprise = contrat::mission_reprise(&mission, &cr["point_de_reprise"]).map_err(|e| anyhow::anyhow!(e))?;
    let id2 = reprise["id"].as_str().unwrap().to_string();
    let (cr2, _) = lancer(&fichiers, &reprise, &paths.missions.join(&id2), &pilote, &inter, |ev| if ev["type"] == "validation_requise" { vec!["valider"] } else { vec![] }).await?;
    println!("  reprise : {} · {:?}", cr2["etat"], cr2["resultat"]["texte"]);
    ok(cr2["etat"] == "termine" && !projet.join("b.txt").exists() && projet.join("c2.txt").exists() && projet.join("a2.txt").exists(), "reprise : opérations 2 et 3 faites, la 1re n'est pas refaite", &mut bilan);
    ok(id2 != id && reprise["reprise"]["mission_origine"] == id.as_str(), "mission de reprise : nouvel identifiant, rattachée à la mission d'origine", &mut bilan);

    // 3. Arrêt immédiat d'un agent qui ignore tout
    let mission = mission_lente("court").map_err(|e| anyhow::anyhow!(e))?;
    let id = mission["id"].as_str().unwrap().to_string();
    let t0 = Instant::now();
    let (cr, _) = lancer(&lent, &mission, &paths.missions.join(&id), &pilote, &inter, |ev| if ev["type"] == "demarrage" { vec!["arret_immediat"] } else { vec![] }).await?;
    println!("  arrêt immédiat : {} en {:.1} s · {}", cr["etat"], t0.elapsed().as_secs_f64(), cr["motif"]);
    ok(cr["etat"] == "arrete_force" && t0.elapsed() < Duration::from_secs(15) && cr["motif"].as_str().unwrap_or("").contains("incomplets"), "arrêt immédiat : agent coupé, « fichiers possiblement incomplets »", &mut bilan);
    ok(pilote.mission_en_cours().is_none() && pilote.commander(&id, "pause").is_err(), "mission terminée : plus aucun ordre accepté", &mut bilan);

    println!("\nBilan : {} OK, {} KO", bilan.0, bilan.1);
    if bilan.1 > 0 {
        std::process::exit(1);
    }
    Ok(())
}
