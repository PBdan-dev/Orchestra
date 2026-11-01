//! Clé de signature de l'Agent Market (lot S, choix A1 de PB) : la créer, signer un catalogue,
//! le vérifier. La clé privée est rangée dans le Gestionnaire d'identification de Windows, sous
//! `orchestra/_market/cle_privee` ; une copie de sauvegarde est écrite à part, à garder hors ligne.
//! La clé publique est écrite dans le code d'Orchestra (`signature::CLE_PUBLIQUE_MARKET`).
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example cle_market -- creer [--sauvegarde <fichier>]
//!   cargo run --release --example cle_market -- publique
//!   cargo run --release --example cle_market -- signer <agents.json> [--sortie <fichier>]
//!   cargo run --release --example cle_market -- verifier <agents.json>

use orchestra_lib::{
    coffre::{Coffre, CoffreWindows},
    signature,
};
use serde_json::Value;

const CIBLE: &str = "orchestra/_market/cle_privee";

fn cle_privee() -> anyhow::Result<Vec<u8>> {
    let b = CoffreWindows.lire(CIBLE).ok_or_else(|| anyhow::anyhow!("pas de clé du Market dans le coffre de Windows ({CIBLE}) : lance d'abord « creer »"))?;
    signature::depuis_base64(&b).map_err(|e| anyhow::anyhow!(e))
}

/// Entrées du catalogue (`{"agents": […]}` ou une liste).
fn entrees(v: &mut Value) -> Vec<&mut Value> {
    match v {
        Value::Array(a) => a.iter_mut().collect(),
        Value::Object(o) => o.get_mut("agents").and_then(|a| a.as_array_mut()).map(|a| a.iter_mut().collect()).unwrap_or_default(),
        _ => vec![],
    }
}

fn certifiee(e: &Value) -> bool {
    matches!(e.pointer("/certification/statut").and_then(|s| s.as_str()), Some("orchestra_certified" | "livrable_seul"))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let val = |nom: &str| args.iter().position(|a| a == nom).and_then(|i| args.get(i + 1)).cloned();
    match args.first().map(String::as_str) {
        Some("creer") => {
            if CoffreWindows.lire(CIBLE).is_some() && !args.iter().any(|a| a == "--remplacer") {
                anyhow::bail!("une clé du Market existe déjà dans le coffre ; « --remplacer » pour en créer une autre (les anciennes signatures ne vaudront plus)");
            }
            let (privee, publique) = signature::generer().map_err(|e| anyhow::anyhow!(e))?;
            let texte = signature::base64(&privee);
            CoffreWindows.ecrire(CIBLE, &texte).map_err(|e| anyhow::anyhow!(e))?;
            anyhow::ensure!(CoffreWindows.lire(CIBLE).as_deref() == Some(texte.as_str()), "clé relue différente dans le coffre");
            let sauvegarde = val("--sauvegarde").map(std::path::PathBuf::from).unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Orchestra-cle-market-SAUVEGARDE.txt"));
            std::fs::write(
                &sauvegarde,
                format!("Clé privée de signature de l'Agent Market (PKCS#8, base64). À garder hors ligne, jamais dans un dépôt.\nPour la remettre dans le coffre : Gestionnaire d'identification de Windows, informations génériques, adresse {CIBLE}.\n\n{texte}\n\nClé publique (dans le code d'Orchestra) : {publique}\n"),
            )?;
            println!("Clé du Market créée et rangée dans le coffre de Windows ({CIBLE}).");
            println!("Copie de sauvegarde : {}", sauvegarde.display());
            println!("Clé publique, à écrire dans signature::CLE_PUBLIQUE_MARKET : {publique}");
        }
        Some("publique") => println!("{}", signature::publique(&cle_privee()?).map_err(|e| anyhow::anyhow!(e))?),
        Some("signer") => {
            let fichier = args.get(1).ok_or_else(|| anyhow::anyhow!("signer <agents.json>"))?;
            let mut v: Value = serde_json::from_str(&std::fs::read_to_string(fichier)?)?;
            let privee = cle_privee()?;
            let mut n = 0;
            for e in entrees(&mut v) {
                if certifiee(e) {
                    signature::signer(e, &privee).map_err(|x| anyhow::anyhow!(x))?;
                    n += 1;
                }
            }
            let sortie = val("--sortie").unwrap_or_else(|| fichier.clone());
            std::fs::write(&sortie, serde_json::to_string_pretty(&v)? + "\n")?;
            println!("{n} entrée(s) certifiée(s) signée(s) → {sortie}");
        }
        Some("verifier") => {
            let fichier = args.get(1).ok_or_else(|| anyhow::anyhow!("verifier <agents.json>"))?;
            let mut v: Value = serde_json::from_str(&std::fs::read_to_string(fichier)?)?;
            for e in entrees(&mut v) {
                let id = e["id"].as_str().unwrap_or("?").to_string();
                if certifiee(e) {
                    match signature::verifier(e, signature::CLE_PUBLIQUE_MARKET) {
                        Ok(()) => println!("OK  {id} : signature valide"),
                        Err(x) => println!("KO  {id} : {x}"),
                    }
                } else {
                    println!("--  {id} : non certifié (pas de signature exigée)");
                }
            }
        }
        _ => println!("Usage : cle_market creer [--sauvegarde <fichier>] | publique | signer <agents.json> [--sortie <fichier>] | verifier <agents.json>"),
    }
    Ok(())
}
