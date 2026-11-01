//! Signature des entrées du catalogue de l'Agent Market (lot S, scénario S6 ; choix A1 de PB, 31/10).
//!
//! Le Market signe chaque entrée « Orchestra Certified » ou « livrable seul » avec sa clé privée
//! Ed25519 ; Orchestra vérifie avec la clé publique écrite dans son code (une clé lue dans le
//! catalogue ne prouverait rien). Est signée l'entrée entière, en JSON canonique (clés triées,
//! sans espaces), sans le champ `certification.signature`, précédée de `orchestra-market-v1\n` ;
//! pour un agent local, cela couvre l'empreinte SHA-256 de son paquet. Format du champ :
//! `"signature": "ed25519:<64 octets en base64>"`.
//!
//! Bibliothèque : `ring` (Apache-2.0 + ISC), déjà livrée avec Orchestra pour le HTTPS.

use serde_json::Value;

/// Clé publique du Market (32 octets en base64), créée le 01/11/2026 par `examples/cle_market.rs` ;
/// la clé privée est chez PB (coffre de Windows et copie de sauvegarde hors ligne).
pub const CLE_PUBLIQUE_MARKET: &str = "zAwi+oouNv1f9DPD+nqGVByGeb4jGLnZKLA0KCzczh0=";

const ENTETE: &str = "orchestra-market-v1\n";

/// JSON canonique : clés triées à tous les niveaux, sans espaces.
pub fn canonique(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut cles: Vec<&String> = o.keys().collect();
            cles.sort();
            let champs: Vec<String> = cles.iter().map(|k| format!("{}:{}", Value::String((*k).clone()), canonique(&o[k.as_str()]))).collect();
            format!("{{{}}}", champs.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonique).collect::<Vec<_>>().join(",")),
        autre => autre.to_string(),
    }
}

/// Ce qui est signé pour une entrée du catalogue.
pub fn charge_utile(entree: &Value) -> Vec<u8> {
    let mut e = entree.clone();
    if let Some(c) = e.get_mut("certification").and_then(|c| c.as_object_mut()) {
        c.remove("signature");
    }
    format!("{ENTETE}{}", canonique(&e)).into_bytes()
}

/// Vérifie la signature d'une entrée avec la clé publique donnée (base64).
pub fn verifier(entree: &Value, cle_publique: &str) -> Result<(), String> {
    let cle = depuis_base64(cle_publique).ok().filter(|c| c.len() == 32).ok_or("clé publique du Market inconnue : aucune signature n'est vérifiable")?;
    let texte = entree.pointer("/certification/signature").and_then(|s| s.as_str()).filter(|s| !s.trim().is_empty()).ok_or("signature du Market absente")?;
    let brute = texte.strip_prefix("ed25519:").ok_or("signature du Market au format inconnu (« ed25519:… » attendu)")?;
    let sig = depuis_base64(brute.trim()).map_err(|_| "signature du Market illisible".to_string())?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &cle)
        .verify(&charge_utile(entree), &sig)
        .map_err(|_| "signature du Market invalide : entrée modifiée après signature, ou signée par une autre clé".to_string())
}

/// Signe une entrée (outil du Market) : remplit `certification.signature`.
pub fn signer(entree: &mut Value, pkcs8: &[u8]) -> Result<(), String> {
    let paire = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8).map_err(|e| format!("clé privée illisible ({e})"))?;
    let sig = paire.sign(&charge_utile(entree));
    if !entree["certification"].is_object() {
        return Err("entrée sans bloc « certification »".into());
    }
    entree["certification"]["signature"] = Value::String(format!("ed25519:{}", base64(sig.as_ref())));
    Ok(())
}

/// Nouvelle paire de clés : (clé privée PKCS#8, clé publique en base64).
pub fn generer() -> Result<(Vec<u8>, String), String> {
    let doc = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map_err(|_| "génération de la clé impossible".to_string())?;
    let pkcs8 = doc.as_ref().to_vec();
    let cle = publique(&pkcs8)?;
    Ok((pkcs8, cle))
}

/// Clé publique (base64) d'une clé privée PKCS#8.
pub fn publique(pkcs8: &[u8]) -> Result<String, String> {
    use ring::signature::KeyPair;
    let paire = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8).map_err(|e| format!("clé privée illisible ({e})"))?;
    Ok(base64(paire.public_key().as_ref()))
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(octets: &[u8]) -> String {
    let mut s = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16 | (*bloc.get(1).unwrap_or(&0) as u32) << 8 | *bloc.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= bloc.len() {
                s.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn depuis_base64(texte: &str) -> Result<Vec<u8>, String> {
    let t = texte.trim().trim_end_matches('=');
    let mut out = Vec::with_capacity(t.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in t.bytes() {
        let v = ALPHABET.iter().position(|a| *a == c).ok_or("base64 illisible")? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_canonique() {
        let v = json!({ "b": 1, "a": { "d": [3, { "z": 1, "y": "é" }], "c": "x\"y" } });
        assert_eq!(canonique(&v), r#"{"a":{"c":"x\"y","d":[3,{"y":"é","z":1}]},"b":1}"#);
    }

    #[test]
    fn base64_aller_retour() {
        assert_eq!(base64(b"hello"), "aGVsbG8=");
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(base64(b"abc"), "YWJj");
        for t in ["aGVsbG8=", "aGk=", "YWJj", ""] {
            assert_eq!(base64(&depuis_base64(t).unwrap()), t);
        }
        assert!(depuis_base64("pas du base64 !").is_err());
    }

    fn entree() -> Value {
        json!({
            "id": "agent-mail", "version": "2.1.0",
            "paquet": { "url": "https://exemple.fr/agent-mail-2.1.0.zip", "taille_octets": 1234, "sha256": "ab".repeat(32) },
            "certification": { "statut": "orchestra_certified", "version_certifiee": "2.1.0" }
        })
    }

    #[test]
    fn signature_verifiee_et_toute_retouche_refusee() {
        let (privee, cle) = generer().unwrap();
        assert_eq!(publique(&privee).unwrap(), cle);
        let mut e = entree();
        signer(&mut e, &privee).unwrap();
        assert!(e["certification"]["signature"].as_str().unwrap().starts_with("ed25519:"));
        assert!(verifier(&e, &cle).is_ok());
        // l'ordre des champs ne compte pas
        let reordonnee: Value = serde_json::from_str(&canonique(&e)).unwrap();
        assert!(verifier(&reordonnee, &cle).is_ok());
        // empreinte du paquet changée après signature
        let mut x = e.clone();
        x["paquet"]["sha256"] = json!("cd".repeat(32));
        assert!(verifier(&x, &cle).is_err());
        // statut retouché
        let mut x = e.clone();
        x["version"] = json!("2.1.1");
        assert!(verifier(&x, &cle).is_err());
        // signée par une autre clé
        let (autre, _) = generer().unwrap();
        let mut x = entree();
        signer(&mut x, &autre).unwrap();
        assert!(verifier(&x, &cle).unwrap_err().contains("invalide"));
        // absente ou mal formée
        assert!(verifier(&entree(), &cle).unwrap_err().contains("absente"));
        let mut x = e.clone();
        x["certification"]["signature"] = json!("ed25519:pasdubase64!");
        assert!(verifier(&x, &cle).is_err());
        // pas de clé publique connue : rien n'est vérifiable
        assert!(verifier(&e, "").is_err());
    }
}
