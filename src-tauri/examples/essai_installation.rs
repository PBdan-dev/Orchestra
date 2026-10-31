//! Essai de bout en bout du lot T4 avec les vrais paquets des agents de test publiés dans les
//! Releases de l'Agent Market : catalogue local construit à partir des paquets, téléchargement
//! depuis GitHub, contrôle de la taille et du SHA-256, certification, mise à jour qui garde la
//! mémoire, puis une mission lancée avec l'agent installé.
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example essai_installation
//! Dossier de test (effacé au début) : `Orchestra/eval/essai-installation/`.

use std::io::Read;

use orchestra_lib::{agents, config::Config, contrat, installation, market, paths::Paths};
use serde_json::{json, Value};

const PAQUETS: &[(&str, &str)] = &[
    ("agent-date", "1.0.0"),
    ("agent-fichiers", "1.0.0"),
    ("agent-brouillons", "1.0.2"),
    ("agent-brouillons", "1.0.3"),
];

fn url(id: &str, v: &str) -> String {
    format!("https://github.com/PBdan-dev/Agent_Market/releases/download/{id}-{v}/{id}-{v}.zip")
}

/// Entrée du catalogue : manifeste du paquet + bloc `paquet` + certification.
fn entree(data: &[u8], id: &str, v: &str, statut: &str) -> anyhow::Result<Value> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data))?;
    let mut texte = String::new();
    z.by_name(&format!("{id}/manifeste.json"))?.read_to_string(&mut texte)?;
    let mut m: Value = serde_json::from_str(&texte)?;
    m["paquet"] = json!({ "url": url(id, v), "taille_octets": data.len(), "sha256": installation::sha256_hex(data) });
    m["certification"] = json!({ "statut": statut, "version_certifiee": v, "date": "2026-10-31", "signature": null });
    Ok(m)
}

fn ok(cond: bool, quoi: &str, bilan: &mut (u32, u32)) {
    println!("{} {quoi}", if cond { "OK " } else { "KO " });
    if cond { bilan.0 += 1 } else { bilan.1 += 1 }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let reel = Paths::init()?;
    let base = reel.root.join("eval").join("essai-installation");
    let _ = std::fs::remove_dir_all(&base);
    let paths = Paths::at(base.join("Orchestra"))?;
    let http = reqwest::Client::builder().user_agent("Orchestra/0.1").build()?;
    let mut bilan = (0, 0);

    // 1. Catalogue local construit à partir des vrais paquets (téléchargés une première fois)
    let mut entrees = vec![];
    for (id, v) in PAQUETS {
        let data = http.get(url(id, v)).send().await?.error_for_status()?.bytes().await?;
        println!("paquet {id}-{v} : {} o, sha256 {}…", data.len(), &installation::sha256_hex(&data)[..12]);
        let statut = if *id == "agent-fichiers" { "non_certifie" } else { "orchestra_certified" };
        entrees.push(entree(&data, id, v, statut)?);
    }
    let ecrire_catalogue = |entrees: &[Value]| -> anyhow::Result<std::path::PathBuf> {
        let p = base.join("agents.json");
        std::fs::write(&p, serde_json::to_string_pretty(&json!({ "format": "agent-market/catalogue", "version": 1, "agents": entrees }))?)?;
        Ok(p)
    };
    // une entrée invalide ne doit pas rendre le catalogue illisible
    let mut avec_invalide = entrees.clone();
    avec_invalide.push(json!({ "id": "agent-casse", "format_manifeste": "2.0", "version": "x" }));
    let mut cfg = Config { market_url: ecrire_catalogue(&avec_invalide)?.display().to_string(), ..Config::default() };
    let (lecture, source) = market::fetch_catalogue(&http, &cfg).await.map_err(|e| anyhow::anyhow!(e))?;
    println!("catalogue {source} : {} entrée(s), {} ignorée(s) : {:?}", lecture.agents.len(), lecture.ignores.len(), lecture.ignores);
    ok(lecture.agents.len() == 4 && lecture.ignores.len() == 1, "entrée invalide ignorée, les autres lisibles", &mut bilan);
    let trouver = |id: &str, v: &str| lecture.agents.iter().find(|m| m.id == id && m.version == v).map(|m| m.brut.clone()).unwrap();

    // 2. Agent certifié : installation depuis GitHub
    let r = installation::installer_depuis_catalogue(&http, &paths, &trouver("agent-date", "1.0.0"), false, &source).await;
    println!("  {:?}", r.as_ref().map(|i| (&i.manifest.id, &i.certification)).map_err(|e| e.clone()));
    ok(r.is_ok() && agents::get_installed(&paths, "agent-date").is_some_and(|m| m.certification == "orchestra_certified"), "agent-date certifié installé", &mut bilan);

    // 3. Non certifié refusé hors mode développement, accepté avec le badge en mode développement
    let e = installation::installer_depuis_catalogue(&http, &paths, &trouver("agent-fichiers", "1.0.0"), false, &source).await;
    println!("  {:?}", e.as_ref().err());
    ok(e.is_err() && agents::get_installed(&paths, "agent-fichiers").is_none(), "agent-fichiers non certifié refusé (mode développement désactivé)", &mut bilan);
    let r = installation::installer_depuis_catalogue(&http, &paths, &trouver("agent-fichiers", "1.0.0"), true, &source).await;
    ok(r.is_ok() && agents::get_installed(&paths, "agent-fichiers").is_some_and(|m| m.certification == "non_certifie"), "agent-fichiers installé en mode développement, badge « Non certifié »", &mut bilan);

    // 4. Empreinte ou taille fausses : refus avant toute ouverture
    let mut faux = trouver("agent-brouillons", "1.0.2");
    faux["paquet"]["sha256"] = json!("0".repeat(64));
    let e = installation::installer_depuis_catalogue(&http, &paths, &faux, false, &source).await;
    println!("  {:?}", e.as_ref().err());
    ok(e.as_ref().is_err_and(|e| e.contains("SHA-256")), "empreinte fausse refusée", &mut bilan);
    let mut faux = trouver("agent-brouillons", "1.0.2");
    faux["paquet"]["taille_octets"] = json!(100);
    let e = installation::installer_depuis_catalogue(&http, &paths, &faux, false, &source).await;
    println!("  {:?}", e.as_ref().err());
    ok(e.is_err() && agents::get_installed(&paths, "agent-brouillons").is_none(), "taille fausse refusée", &mut bilan);

    // 5. Format 3.x refusé avec motif
    let mut f3 = trouver("agent-brouillons", "1.0.2");
    f3["format_manifeste"] = json!("3.0");
    let e = installation::installer_depuis_catalogue(&http, &paths, &f3, false, &source).await;
    println!("  {:?}", e.as_ref().err());
    ok(e.as_ref().is_err_and(|e| e.contains("incompatible")), "format 3.x refusé", &mut bilan);

    // 6. Mise à jour 1.0.2 → 1.0.3 : programme remplacé, mémoire gardée
    installation::installer_depuis_catalogue(&http, &paths, &trouver("agent-brouillons", "1.0.2"), false, &source).await.map_err(|e| anyhow::anyhow!(e))?;
    let memoire = agents::dossier_installe(&paths, "agent-brouillons").join("memoire");
    std::fs::create_dir_all(&memoire)?;
    std::fs::write(memoire.join("ton.md"), "PB préfère un ton cordial.")?;
    let i = installation::installer_depuis_catalogue(&http, &paths, &trouver("agent-brouillons", "1.0.3"), false, &source).await.map_err(|e| anyhow::anyhow!(e))?;
    println!("  mise à jour : {:?} → {}", i.ancienne_version, i.manifest.version);
    ok(
        i.ancienne_version.as_deref() == Some("1.0.2") && i.manifest.version == "1.0.3" && std::fs::read_to_string(memoire.join("ton.md")).unwrap_or_default() == "PB préfère un ton cordial.",
        "mise à jour 1.0.2 → 1.0.3, mémoire gardée",
        &mut bilan,
    );

    // 7. D37 : agent installé dont le manifeste n'est plus conforme : utilisable, avec avertissement
    let d = agents::dossier_installe(&paths, "agent-date");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(d.join("manifeste.json"))?)?;
    let sauvegarde = v.clone();
    v["format_manifeste"] = json!("3.0");
    std::fs::write(d.join("manifeste.json"), v.to_string())?;
    let m = agents::get_installed(&paths, "agent-date");
    println!("  avertissement : {:?}", m.as_ref().map(|m| &m.avertissement));
    ok(m.is_some_and(|m| !m.avertissement.is_empty()), "agent devenu incompatible gardé, avec avertissement", &mut bilan);
    std::fs::write(d.join("manifeste.json"), sauvegarde.to_string())?;

    // 8. Mission avec l'agent installé depuis le Market
    let m = agents::get_installed(&paths, "agent-date").unwrap();
    let projet = base.join("projet");
    std::fs::create_dir_all(&projet)?;
    let mission = contrat::mission(
        &m,
        contrat::Contenu {
            projet: &projet,
            type_tache: Some("date-du-jour".into()),
            but: "Donner la date du jour",
            critere_reussite: "date au format long",
            contexte: "",
            donnees: json!({ "format": "long" }),
            demande_originale: "Quelle date sommes-nous ?",
        },
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let r = contrat::executer(&m, &mission, &base.join("mission"), None, None, |_| vec![], |_| vec![]).await?;
    println!("  compte rendu : {} · {}", r.compte_rendu["etat"], r.compte_rendu["resume"]);
    ok(r.compte_rendu["etat"] == "termine", "mission de bout en bout avec l'agent installé", &mut bilan);

    cfg.mode_developpement = false;
    let _ = cfg;
    println!("\nBilan : {} OK, {} KO", bilan.0, bilan.1);
    if bilan.1 > 0 {
        std::process::exit(1);
    }
    Ok(())
}
