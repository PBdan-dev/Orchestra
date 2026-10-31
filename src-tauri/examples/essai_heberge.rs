//! Essai du lot T8 avec un faux agent hébergé local, en HTTPS (certificat auto-signé de test) :
//! installation (ce qui quitte la machine), mission envoyée sans mémoire, fichier ni chemin local,
//! événements et compte rendu, clé envoyée seulement si déclarée, contrôle automatique du
//! livrable, HTTPS obligatoire, arrêt pendant l'appel, jamais « de confiance ».
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example essai_heberge
//! Dossier de test (effacé au début) : `Orchestra/eval/essai-heberge/`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orchestra_lib::{agents, contrat, heberge, installation, paths::Paths, verification};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn ok(cond: bool, quoi: &str, bilan: &mut (u32, u32)) {
    println!("{} {quoi}", if cond { "OK " } else { "KO " });
    if cond { bilan.0 += 1 } else { bilan.1 += 1 }
}

/// Faux agent hébergé : répond selon le chemin ; garde les corps reçus.
async fn serveur(recus: Arc<Mutex<Vec<(String, String, String)>>>) -> anyhow::Result<(u16, reqwest::Certificate)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let pem = cert.cert.pem();
    let cle = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());
    let config = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(vec![cert.cert.der().clone()], cle)?;
    let accepteur = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = ecoute.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((flux, _)) = ecoute.accept().await {
            let (a, recus) = (accepteur.clone(), recus.clone());
            tokio::spawn(async move {
                let Ok(mut tls) = a.accept(flux).await else { return };
                let mut buf = vec![0u8; 1 << 20];
                let mut lu = 0;
                // en-têtes puis corps (Content-Length)
                loop {
                    let Ok(n) = tls.read(&mut buf[lu..]).await else { return };
                    if n == 0 { return; }
                    lu += n;
                    let t = String::from_utf8_lossy(&buf[..lu]).to_string();
                    if let Some(i) = t.find("\r\n\r\n") {
                        let longueur: usize = t[..i].lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                        if lu >= i + 4 + longueur { break; }
                    }
                }
                let t = String::from_utf8_lossy(&buf[..lu]).to_string();
                let (tete, corps) = t.split_once("\r\n\r\n").unwrap_or((&t, ""));
                let chemin = tete.split_whitespace().nth(1).unwrap_or("").to_string();
                let auth = tete.lines().find_map(|l| l.to_ascii_lowercase().starts_with("authorization:").then(|| l[14..].trim().to_string())).unwrap_or_default();
                recus.lock().unwrap().push((chemin.clone(), auth.clone(), corps.to_string()));
                let mission: Value = serde_json::from_str(corps).unwrap_or_default();
                let id = mission["id"].as_str().unwrap_or("");
                let cr = |texte: &str, fichiers: Value| json!({ "mission_id": id, "etat": "termine", "resultat": { "texte": texte, "fichiers": fichiers }, "actions_declarees": [], "verification": { "controle": ["texte relu"], "non_controle": [] }, "point_de_reprise": null });
                let (statut, reponse) = match chemin.as_str() {
                    "/bon" => (200, format!("{}\n{}\n", json!({ "evenement": { "mission_id": id, "agent_id": "agent-traduction", "type": "etape", "message": "traduction", "t": "2026-10-31T18:00:00+01:00" } }), json!({ "compte_rendu": cr("Hello Camille, thank you for Tuesday's meeting.", json!([])) }))),
                    "/fichier" => (200, json!({ "compte_rendu": cr("voir le fichier", json!(["traduction.md"])) }).to_string()),
                    "/vide" => (200, cr("", json!([])).to_string()),
                    "/cle" if auth == "Bearer cle-de-test" => (200, cr("clé reçue", json!([])).to_string()),
                    "/cle" => (401, "{}".into()),
                    "/lent" => {
                        tokio::time::sleep(Duration::from_secs(20)).await;
                        (200, cr("trop tard", json!([])).to_string())
                    }
                    _ => (404, "{}".into()),
                };
                let entete = format!("HTTP/1.1 {statut} X\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reponse.len());
                let _ = tls.write_all(entete.as_bytes()).await;
                let _ = tls.write_all(reponse.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    Ok((port, reqwest::Certificate::from_pem(pem.as_bytes())?))
}

fn manifeste(port: u16, chemin: &str, identifiants: Value) -> Value {
    json!({
        "format_manifeste": "2.0", "id": "agent-traduction", "name": "Traduction (hébergé)", "version": "1.0.0",
        "author": { "nom": "Essai", "contact": "https://exemple.fr" },
        "description": "Traduit un texte court.", "categorie": "redaction", "capabilities": ["traduction"],
        "fiche": { "sait_faire": ["Traduire un texte court"], "ne_sait_pas_faire": ["Lire des fichiers", "Écrire des fichiers"], "fonctions": [], "consignes": "Traduis." },
        "execution": { "type": "heberge", "endpoint": format!("https://localhost:{port}{chemin}") },
        "donnees_envoyees": "le texte à traduire", "conservation_declaree": "aucune (traitement en mémoire)",
        "reseau_declare": [], "acces_declares": { "fichiers": "aucun" }, "identifiants": identifiants, "memoire_propre": false,
        "compatibilite": { "orchestra_min": "1.0.0", "contrat": 1 }, "certification": { "statut": "livrable_seul" }, "licence": "MIT"
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let reel = Paths::init()?;
    let base = reel.root.join("eval").join("essai-heberge");
    let _ = std::fs::remove_dir_all(&base);
    let paths = Paths::at(base.join("Orchestra"))?;
    let projet = paths.projets.join("divers");
    std::fs::create_dir_all(&projet)?;
    std::fs::write(projet.join("secret-local.md"), "ne doit jamais partir")?;
    let recus = Arc::new(Mutex::new(vec![]));
    let (port, cert) = serveur(recus.clone()).await?;
    let http = reqwest::Client::builder().add_root_certificate(cert).build()?;
    let mut bilan = (0, 0);

    // 1. Installation : catalogue livrable_seul, ce qui quitte la machine
    let entree = manifeste(port, "/bon", json!([]));
    let i = installation::installer_depuis_catalogue(&http, &paths, &entree, false, "catalogue d'essai").await.map_err(|e| anyhow::anyhow!(e))?;
    let trace: Value = serde_json::from_str(&std::fs::read_to_string(agents::dossier_installe(&paths, "agent-traduction").join("installation.json"))?)?;
    println!("  ce qui part : {}", trace["ce_qui_part"]);
    ok(i.certification == "livrable_seul" && trace["ce_qui_part"]["donnees_envoyees"] == "le texte à traduire", "agent hébergé installé (livrable seul), ce qui quitte la machine noté", &mut bilan);
    let m = agents::get_installed(&paths, "agent-traduction").unwrap();
    ok(!m.est_local() && !verification::peut_etre_de_confiance(&paths, &m), "jamais promu « de confiance » automatiquement", &mut bilan);

    // 2. Mission : seule la mission part (ni chemin local, ni fichier, ni mémoire)
    let mission = contrat::mission(&m, contrat::Contenu { projet: &projet, type_tache: None, but: "Traduire en anglais", critere_reussite: "traduction fidèle", contexte: "Mail de remerciement", donnees: json!({ "texte": "Bonjour Camille, merci pour la réunion de mardi." }), demande_originale: "traduis ce mail" }).map_err(|e| anyhow::anyhow!(e))?;
    let dossier = paths.missions.join(mission["id"].as_str().unwrap());
    let mut evenements = vec![];
    let d = heberge::executer(&http, &m, &mission, &dossier, None, None, |ev| { evenements.push(ev["type"].as_str().unwrap_or("").to_string()); vec![] }).await?;
    let (_, auth, corps) = recus.lock().unwrap().last().cloned().unwrap();
    println!("  reçu par l'agent : {} octets · compte rendu {} · {:?}", corps.len(), d.compte_rendu["etat"], d.compte_rendu["resultat"]["texte"]);
    ok(d.compte_rendu["etat"] == "termine" && evenements == ["etape"], "compte rendu et événement reçus", &mut bilan);
    let racine = paths.root.to_string_lossy().replace('\\', "\\\\");
    ok(!corps.contains(&racine) && !corps.contains("Users") && !corps.contains("secret-local") && !corps.contains("manifeste") && corps.contains("\"projet\":\"divers\"") && auth.is_empty(), "rien d'autre que la mission n'est parti (ni chemin, ni fichier, ni manifeste, ni clé)", &mut bilan);
    ok(dossier.join("envoye.json").exists(), "ce qui est parti est gardé dans envoye.json", &mut bilan);

    // 3. Contrôle automatique du livrable
    let mut m2 = m.clone();
    m2.brut["execution"]["endpoint"] = json!(format!("https://localhost:{port}/fichier"));
    let d = heberge::executer(&http, &m2, &mission, &dossier, None, None, |_| vec![]).await?;
    ok(d.compte_rendu["etat"] == "echec" && d.compte_rendu["motif"].as_str().unwrap_or("").contains("fichier"), "livrable avec un fichier : refusé (aucun accès aux fichiers)", &mut bilan);
    m2.brut["execution"]["endpoint"] = json!(format!("https://localhost:{port}/vide"));
    let d = heberge::executer(&http, &m2, &mission, &dossier, None, None, |_| vec![]).await?;
    ok(d.compte_rendu["etat"] == "echec" && d.compte_rendu["motif"].as_str().unwrap_or("").contains("vide"), "livrable vide : refusé", &mut bilan);

    // 4. HTTPS obligatoire
    m2.brut["execution"]["endpoint"] = json!(format!("http://localhost:{port}/bon"));
    let d = heberge::executer(&http, &m2, &mission, &dossier, None, None, |_| vec![]).await?;
    ok(d.compte_rendu["motif"].as_str().unwrap_or("").contains("HTTPS"), "adresse http refusée (HTTPS obligatoire)", &mut bilan);
    let mut sans = entree.clone();
    sans["execution"]["endpoint"] = json!("http://localhost/bon");
    ok(installation::autorisation(&sans, true).is_err(), "installation refusée sans HTTPS", &mut bilan);

    // 5. Clé (D34) : envoyée en Authorization seulement à l'agent qui la déclare
    m2.brut["execution"]["endpoint"] = json!(format!("https://localhost:{port}/cle"));
    let d = heberge::executer(&http, &m2, &mission, &dossier, Some("cle-de-test".into()), None, |_| vec![]).await?;
    ok(d.compte_rendu["etat"] == "termine", "clé reçue par l'agent qui la déclare", &mut bilan);

    // 6. Arrêt pendant l'appel
    m2.brut["execution"]["endpoint"] = json!(format!("https://localhost:{port}/lent"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let t0 = Instant::now();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = tx.send(json!({ "type": contrat::ARRET_IMMEDIAT }));
    });
    let d = heberge::executer(&http, &m2, &mission, &dossier, None, Some(&mut rx), |_| vec![]).await?;
    ok(d.compte_rendu["etat"] == "echec" && t0.elapsed() < Duration::from_secs(5), "arrêt pendant l'appel : abandonné tout de suite", &mut bilan);

    println!("\nBilan : {} OK, {} KO", bilan.0, bilan.1);
    if bilan.1 > 0 {
        std::process::exit(1);
    }
    Ok(())
}
