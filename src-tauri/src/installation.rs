//! Installation des agents (lot T4) : depuis le catalogue de l'Agent Market, ou depuis un
//! fichier .zip en mode développement.
//!
//! Étapes : certification vérifiée (D25), paquet téléchargé, taille et SHA-256 contrôlées avant
//! de l'ouvrir, manifeste v2 contrôlé (format 2.x, D35), puis rangement dans
//! `agents/installes/<id>/`. Une mise à jour remplace le programme et garde `memoire/` (D32) ;
//! l'ancienne version est mise de côté dans `agents/anciennes-versions/`, jamais effacée.
//! Les identifiants déclarés (D34) sont rangés dans le Gestionnaire d'identification de Windows
//! sous `orchestra/<id>/<nom>` ; leur valeur n'est écrite nulle part ailleurs.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{agents::{self, Manifest}, paths::Paths};

/// Premier chiffre de `format_manifeste` lu par cette version d'Orchestra (D35).
pub const FORMAT_MAJEUR: u32 = 2;
/// Paquet le plus gros accepté (un agent avec ses dépendances, pas un modèle).
const TAILLE_MAX: u64 = 2_000_000_000;
const FICHIERS_MAX: usize = 20_000;

/// Un identifiant déclaré par l'agent (D34) et s'il est déjà rangé dans le coffre de Windows.
#[derive(Serialize, Clone, Debug)]
pub struct Identifiant {
    pub nom: String,
    pub r#type: String,
    pub raison: String,
    pub present: bool,
}

/// Résultat d'une installation, affiché dans l'interface.
#[derive(Serialize, Clone, Debug)]
pub struct Installation {
    pub manifest: Manifest,
    /// Version remplacée, s'il s'agissait d'une mise à jour.
    pub ancienne_version: Option<String>,
    /// Identifiants à demander à PB (D34).
    pub identifiants: Vec<Identifiant>,
    /// orchestra_certified | non_certifie (mode développement)
    pub certification: String,
}

/// Entrée du catalogue vue par l'interface : installable ou non, et pourquoi.
#[derive(Serialize, Clone, Debug)]
pub struct EntreeCatalogue {
    pub manifest: Manifest,
    pub installable: bool,
    pub motif: String,
    /// Version déjà installée, s'il y en a une.
    pub installe: Option<String>,
    /// Agent hébergé : ce qui quittera la machine, affiché avant l'installation (lot T8).
    pub ce_qui_part: Option<Value>,
}

fn s<'a>(v: &'a Value, p: &str) -> &'a str {
    v.pointer(p).and_then(|x| x.as_str()).unwrap_or("")
}

/// Premier chiffre de `format_manifeste`.
pub fn format_majeur(v: &Value) -> Option<u32> {
    s(v, "/format_manifeste").split('.').next()?.trim().parse().ok()
}

/// Refus explicite d'un format incompatible (D35), avant le contrôle détaillé du schéma.
pub fn controle_format(v: &Value) -> Result<(), String> {
    match format_majeur(v) {
        Some(FORMAT_MAJEUR) => Ok(()),
        Some(_) => Err(format!(
            "format de manifeste {} incompatible : cette version d'Orchestra lit le format {FORMAT_MAJEUR}.x (D35)",
            s(v, "/format_manifeste")
        )),
        None => Err("format_manifeste absent ou illisible".into()),
    }
}

/// L'entrée du catalogue peut-elle être installée ? Renvoie la certification retenue (D25).
pub fn autorisation(entree: &Value, mode_dev: bool) -> Result<String, String> {
    // agent hébergé (lot T8, D2, D25) : badge « livrable seul » au mieux, jamais « Orchestra Certified »
    if s(entree, "/execution/type") == "heberge" {
        if !s(entree, "/execution/endpoint").starts_with("https://") {
            return Err("agent hébergé sans adresse HTTPS : refusé".into());
        }
        return match s(entree, "/certification/statut") {
            "livrable_seul" | "orchestra_certified" => Ok("livrable_seul".into()),
            _ if mode_dev => Ok("non_certifie".into()),
            _ => Err("agent hébergé non vérifié par l'Agent Market (mode développement désactivé)".into()),
        };
    }
    let statut = s(entree, "/certification/statut");
    let version = s(entree, "/version");
    let certifie = statut == "orchestra_certified";
    let version_certifiee = s(entree, "/certification/version_certifiee");
    if certifie && (version_certifiee.is_empty() || version_certifiee == version) {
        return Ok("orchestra_certified".into());
    }
    if mode_dev {
        return Ok("non_certifie".into());
    }
    if certifie {
        Err(format!("certification valable pour la version {version_certifiee}, pas pour la {version} (D35)"))
    } else {
        Err("non certifié : seuls les agents « Orchestra Certified » s'installent (mode développement désactivé)".into())
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let h = Sha256::digest(data);
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Taille et empreinte du paquet, comparées à l'entrée du catalogue.
pub fn verifier_paquet(data: &[u8], taille: u64, sha256: &str) -> Result<(), String> {
    if data.len() as u64 != taille {
        return Err(format!("taille du paquet {} o, le catalogue annonce {taille} o : paquet refusé", data.len()));
    }
    let calcule = sha256_hex(data);
    if !calcule.eq_ignore_ascii_case(sha256.trim()) {
        return Err(format!("empreinte SHA-256 différente de celle du catalogue (calculée {}…) : paquet refusé", &calcule[..12]));
    }
    Ok(())
}

/// Télécharge le paquet annoncé par l'entrée du catalogue et le vérifie avant toute ouverture.
pub async fn telecharger(http: &reqwest::Client, entree: &Value) -> Result<Vec<u8>, String> {
    let url = s(entree, "/paquet/url");
    let taille = entree.pointer("/paquet/taille_octets").and_then(|t| t.as_u64()).ok_or("paquet sans taille dans le catalogue")?;
    let sha = s(entree, "/paquet/sha256");
    if !url.starts_with("https://") {
        return Err("adresse du paquet absente ou non https".into());
    }
    if sha.len() != 64 {
        return Err("empreinte SHA-256 absente du catalogue".into());
    }
    if taille > TAILLE_MAX {
        return Err(format!("paquet de {taille} o, au-delà de la limite de {TAILLE_MAX} o"));
    }
    let mut resp = http
        .get(url)
        .timeout(Duration::from_secs(900))
        .send()
        .await
        .map_err(|e| format!("téléchargement impossible ({url}) : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("téléchargement : le serveur a répondu {} ({url})", resp.status()));
    }
    let mut data = Vec::with_capacity(taille as usize);
    while let Some(bloc) = resp.chunk().await.map_err(|e| format!("téléchargement interrompu : {e}"))? {
        data.extend_from_slice(&bloc);
        if data.len() as u64 > taille {
            return Err(format!("paquet plus gros que les {taille} o annoncés : téléchargement arrêté"));
        }
    }
    verifier_paquet(&data, taille, sha)?;
    Ok(data)
}

/// Décompresse le paquet dans `dest`. Chemins hors du dossier refusés ; un dossier de tête
/// unique (`<id>/…`, forme des paquets du Market) est retiré.
fn decompresser(data: &[u8], dest: &Path) -> Result<(), String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| format!("paquet .zip illisible ({e})"))?;
    if z.len() > FICHIERS_MAX {
        return Err(format!("paquet de {} fichiers, au-delà de la limite de {FICHIERS_MAX}", z.len()));
    }
    let mut noms = Vec::with_capacity(z.len());
    let mut total: u64 = 0;
    for i in 0..z.len() {
        let f = z.by_index(i).map_err(|e| format!("paquet .zip abîmé ({e})"))?;
        let nom = f.enclosed_name().ok_or_else(|| format!("chemin refusé dans le paquet : {}", f.name()))?;
        total += f.size();
        noms.push((nom, f.is_dir()));
    }
    if total > TAILLE_MAX * 2 {
        return Err("paquet trop gros une fois décompressé".into());
    }
    // dossier de tête commun
    let premier = |p: &PathBuf| p.components().next().and_then(|c| match c { Component::Normal(n) => Some(n.to_os_string()), _ => None });
    let tete = noms.first().and_then(|(p, _)| premier(p));
    let racine_commune = tete.as_ref().is_some_and(|t| {
        noms.iter().all(|(p, dir)| premier(p).as_ref() == Some(t) && (p.components().count() > 1 || *dir))
    });
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| format!("paquet .zip abîmé ({e})"))?;
        let (nom, dir) = &noms[i];
        let rel: PathBuf = if racine_commune { nom.components().skip(1).collect() } else { nom.clone() };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let sortie = dest.join(&rel);
        if *dir {
            std::fs::create_dir_all(&sortie).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = sortie.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut contenu = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut contenu).map_err(|e| format!("{} : décompression impossible ({e})", rel.display()))?;
        std::fs::write(&sortie, contenu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Contrôle du paquet décompressé : manifeste v2 complet, cohérent avec le catalogue, CHANGELOG.
fn controler_paquet(dossier: &Path, entree: Option<&Value>) -> Result<Manifest, String> {
    let texte = std::fs::read_to_string(dossier.join("manifeste.json")).map_err(|_| "manifeste.json absent du paquet".to_string())?;
    let v: Value = serde_json::from_str(&texte).map_err(|e| format!("manifeste.json invalide ({e})"))?;
    controle_format(&v)?;
    let mut m = Manifest::depuis_v2(&v).map_err(|e| format!("manifeste incomplet : {e}"))?;
    if let Some(e) = entree {
        for champ in ["id", "version"] {
            if s(&v, &format!("/{champ}")) != s(e, &format!("/{champ}")) {
                return Err(format!(
                    "le paquet annonce {champ} « {} », le catalogue « {} » : paquet refusé",
                    s(&v, &format!("/{champ}")),
                    s(e, &format!("/{champ}"))
                ));
            }
        }
    }
    if !m.est_local() {
        return Err("seuls les agents locaux s'installent depuis un paquet (agents hébergés : lot T8)".into());
    }
    if m.commande().trim().is_empty() {
        return Err("execution.commande manquante".into());
    }
    if s(&v, "/execution/protocole") != "stdio-json" {
        return Err("protocole d'exécution autre que stdio-json".into());
    }
    if !dossier.join("CHANGELOG.md").exists() {
        return Err("CHANGELOG.md absent du paquet (D35)".into());
    }
    for t in m.taches() {
        for chemin in [&t.modele_demande, &t.modele_livrable] {
            if !chemin.is_empty() && (Path::new(chemin).components().any(|c| matches!(c, Component::ParentDir)) || !dossier.join(chemin).exists()) {
                return Err(format!("modèle {chemin} du type de tâche « {} » absent du paquet (D33)", t.r#type));
            }
        }
    }
    m.dossier = Some(dossier.to_path_buf());
    Ok(m)
}

/// Installe (ou met à jour) un agent à partir des octets de son paquet.
/// `entree` : l'entrée du catalogue (absente pour un .zip installé en mode développement).
pub fn installer_paquet(paths: &Paths, data: &[u8], entree: Option<&Value>, certification: &str, source: &str) -> Result<Installation, String> {
    std::fs::create_dir_all(&paths.agents).map_err(|e| e.to_string())?;
    let tmp = paths.agents.join(format!(".installation-{}", &uuid::Uuid::new_v4().to_string()[..8]));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let controle = decompresser(data, &tmp).and_then(|_| controler_paquet(&tmp, entree));
    let m = match controle {
        Ok(m) => m,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    // la mémoire de l'agent n'est jamais livrée par le paquet : celle de PB fait foi
    let _ = std::fs::remove_dir_all(tmp.join("memoire"));
    let _ = std::fs::remove_file(tmp.join("installation.json"));

    let dest = agents::dossier_installe(paths, &m.id);
    let mut ancienne_version = None;
    let mut mise_de_cote: Option<PathBuf> = None;
    if dest.exists() {
        let ancienne: Value = std::fs::read_to_string(dest.join("manifeste.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
        let v = s(&ancienne, "/version").to_string();
        let archive = paths.agents.join("anciennes-versions").join(format!(
            "{}_{}_{}",
            crate::paths::safe_name(&m.id),
            crate::paths::safe_name(if v.is_empty() { "inconnue" } else { &v }),
            chrono::Local::now().format("%Y-%m-%d_%H%M%S")
        ));
        if let Some(p) = archive.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        if let Err(e) = std::fs::rename(&dest, &archive) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!("ancienne version impossible à mettre de côté (agent en cours d'exécution ?) : {e}"));
        }
        // mise à jour : la mémoire propre de l'agent est gardée (D32)
        if archive.join("memoire").exists() {
            std::fs::rename(archive.join("memoire"), tmp.join("memoire")).map_err(|e| format!("mémoire de l'agent non reprise : {e}"))?;
        }
        ancienne_version = Some(v);
        mise_de_cote = Some(archive);
    }
    if m.brut["memoire_propre"].as_bool() == Some(true) {
        std::fs::create_dir_all(tmp.join("memoire")).map_err(|e| e.to_string())?;
    }
    let trace = json!({
        "source": source,
        "version": m.version,
        "sha256": sha256_hex(data),
        "certification": certification,
        "installe_le": chrono::Local::now().to_rfc3339(),
        "ancienne_version": ancienne_version,
    });
    std::fs::write(tmp.join("installation.json"), serde_json::to_string_pretty(&trace).unwrap_or_default()).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        // on remet l'ancienne version en place
        if let Some(a) = &mise_de_cote {
            let _ = std::fs::rename(tmp.join("memoire"), a.join("memoire"));
            let _ = std::fs::rename(a, &dest);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("installation impossible : {e}"));
    }
    let manifest = agents::get_installed(paths, &m.id).ok_or("agent installé mais illisible")?;
    let identifiants = identifiants(&manifest);
    Ok(Installation { manifest, ancienne_version, identifiants, certification: certification.into() })
}

/// Installation depuis le catalogue : certification, téléchargement vérifié, rangement.
pub async fn installer_depuis_catalogue(http: &reqwest::Client, paths: &Paths, entree: &Value, mode_dev: bool, source: &str) -> Result<Installation, String> {
    controle_format(entree)?;
    let certification = autorisation(entree, mode_dev)?;
    if s(entree, "/execution/type") == "heberge" {
        return installer_heberge(paths, entree, &certification, source);
    }
    let data = telecharger(http, entree).await?;
    installer_paquet(paths, &data, Some(entree), &certification, source)
}

/// Agent hébergé (lot T8) : rien à télécharger ; son manifeste est rangé dans
/// `agents/installes/<id>/` avec la trace de ce qui quittera la machine.
pub fn installer_heberge(paths: &Paths, entree: &Value, certification: &str, source: &str) -> Result<Installation, String> {
    let mut v = entree.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("paquet");
    }
    let m = Manifest::depuis_v2(&v).map_err(|e| format!("manifeste incomplet : {e}"))?;
    crate::heberge::endpoint(&m)?;
    let dest = agents::dossier_installe(paths, &m.id);
    let ancienne_version = std::fs::read_to_string(dest.join("manifeste.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|a| a["version"].as_str().map(String::from));
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    std::fs::write(dest.join("manifeste.json"), serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
    let trace = json!({
        "source": source,
        "version": m.version,
        "certification": certification,
        "installe_le": chrono::Local::now().to_rfc3339(),
        "ancienne_version": ancienne_version,
        "ce_qui_part": crate::heberge::ce_qui_part(&m),
    });
    std::fs::write(dest.join("installation.json"), serde_json::to_string_pretty(&trace).unwrap_or_default()).map_err(|e| e.to_string())?;
    let manifest = agents::get_installed(paths, &m.id).ok_or("agent installé mais illisible")?;
    let identifiants = identifiants(&manifest);
    Ok(Installation { manifest, ancienne_version, identifiants, certification: certification.into() })
}

/// Clé d'un agent hébergé (D34), lue dans le coffre de Windows : le premier identifiant déclaré
/// de type clé ou jeton qui y est rangé.
pub fn cle_heberge(m: &Manifest) -> Option<String> {
    m.brut["identifiants"].as_array()?.iter().filter(|i| matches!(i["type"].as_str(), Some("cle_api" | "jeton" | "jeton_oauth"))).find_map(|i| {
        let nom = i["nom"].as_str()?;
        entree_coffre(&m.id, nom).ok()?.get_password().ok()
    })
}

/// Installation d'un .zip local : mode développement seulement, badge « Non certifié » (D25).
pub fn installer_fichier(paths: &Paths, chemin: &Path, mode_dev: bool) -> Result<Installation, String> {
    if !mode_dev {
        return Err("installation depuis un fichier : active d'abord le mode développement (Paramètres)".into());
    }
    let meta = std::fs::metadata(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
    if meta.len() > TAILLE_MAX {
        return Err("fichier trop gros pour un paquet d'agent".into());
    }
    let data = std::fs::read(chemin).map_err(|e| e.to_string())?;
    installer_paquet(paths, &data, None, "non_certifie", &format!("fichier {}", chemin.display()))
}

/// Entrées du catalogue avec, pour chacune, si elle peut être installée et pourquoi.
pub fn lire_catalogue(entrees: Vec<Manifest>, paths: &Paths, mode_dev: bool) -> Vec<EntreeCatalogue> {
    let installes = agents::installed(paths);
    entrees
        .into_iter()
        .filter(|m| mode_dev || m.certification != "non_certifie")
        .map(|m| {
            let (installable, motif) = match autorisation(&m.brut, mode_dev) {
                Ok(c) if c == "non_certifie" => (true, "non certifié (mode développement)".to_string()),
                Ok(_) => (true, String::new()),
                Err(e) => (false, e),
            };
            let installe = installes.iter().find(|i| i.id == m.id).map(|i| i.version.clone());
            let ce_qui_part = (!m.est_local()).then(|| crate::heberge::ce_qui_part(&m));
            EntreeCatalogue { manifest: m, installable, motif, installe, ce_qui_part }
        })
        .collect()
}

// ---------- Identifiants (D34) ----------

/// Nom de l'identifiant dans le Gestionnaire d'identification de Windows.
pub fn cible(agent_id: &str, nom: &str) -> String {
    format!("orchestra/{agent_id}/{nom}")
}

fn entree_coffre(agent_id: &str, nom: &str) -> Result<keyring::Entry, String> {
    let c = cible(agent_id, nom);
    keyring::Entry::new_with_target(&c, "orchestra", &c).map_err(|e| format!("coffre de Windows inaccessible : {e}"))
}

/// Identifiants déclarés par l'agent et leur présence dans le coffre (jamais leur valeur).
pub fn identifiants(m: &Manifest) -> Vec<Identifiant> {
    m.brut["identifiants"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let nom = i["nom"].as_str().unwrap_or("").to_string();
            let present = entree_coffre(&m.id, &nom).and_then(|e| e.get_password().map_err(|e| e.to_string())).is_ok();
            Identifiant { nom, r#type: i["type"].as_str().unwrap_or("").into(), raison: i["raison"].as_str().unwrap_or("").into(), present }
        })
        .collect()
}

/// Range un identifiant déclaré par l'agent dans le coffre de Windows.
pub fn enregistrer_identifiant(m: &Manifest, nom: &str, valeur: &str) -> Result<(), String> {
    if !identifiants(m).iter().any(|i| i.nom == nom) {
        return Err(format!("{} ne déclare pas d'identifiant « {nom} »", m.name));
    }
    if valeur.is_empty() {
        return Err("valeur vide".into());
    }
    entree_coffre(&m.id, nom)?.set_password(valeur).map_err(|e| format!("identifiant non rangé : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn manifeste(version: &str) -> Value {
        json!({
            "format_manifeste": "2.0", "id": "agent-essai", "name": "Essai", "version": version,
            "author": { "nom": "PB", "contact": "https://github.com/PBdan-dev" }, "description": "Agent d'essai.", "categorie": "utilitaire",
            "capabilities": ["essai"],
            "fiche": { "sait_faire": ["Essayer"], "ne_sait_pas_faire": ["Tout", "Le reste"], "fonctions": [], "consignes": "Rien." },
            "taches": [{ "type": "essai", "modele_demande": "modeles/essai.demande.json", "modele_livrable": "modeles/essai.livrable.json" }],
            "execution": { "type": "local", "systemes": ["windows-x64"], "commande": "python agent.py", "protocole": "stdio-json" },
            "modele": { "via": "porte", "demande": "aucun", "minimum": "aucun", "recommande": "aucun" },
            "ressources": { "vram_go": 0, "gpu_exclusif": false }, "reseau_declare": [], "acces_declares": { "fichiers": "aucun" },
            "identifiants": [], "memoire_propre": true, "compatibilite": { "orchestra_min": "1.0.0", "contrat": 1 },
            "certification": { "statut": "non_certifie" }, "licence": "MIT"
        })
    }

    fn paquet(m: &Value, changelog: bool) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            let mut ajouter = |nom: &str, contenu: &str| {
                z.start_file(format!("agent-essai/{nom}"), o).unwrap();
                z.write_all(contenu.as_bytes()).unwrap();
            };
            ajouter("manifeste.json", &m.to_string());
            ajouter("agent.py", "print('ok')");
            ajouter("modeles/essai.demande.json", "{}");
            ajouter("modeles/essai.livrable.json", "{}");
            if changelog {
                ajouter("CHANGELOG.md", "# 1.0.0");
            }
            ajouter("memoire/piege.md", "mémoire livrée par le paquet");
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn paths_essai() -> (Paths, PathBuf) {
        let racine = std::env::temp_dir().join(format!("orchestra-t4-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        (Paths::at(racine.clone()).unwrap(), racine)
    }

    #[test]
    fn format_et_certification() {
        let mut m = manifeste("1.0.0");
        assert!(controle_format(&m).is_ok());
        m["format_manifeste"] = json!("3.0");
        assert!(controle_format(&m).unwrap_err().contains("incompatible"));
        let mut e = manifeste("1.0.0");
        assert!(autorisation(&e, false).unwrap_err().contains("non certifié"));
        assert_eq!(autorisation(&e, true).unwrap(), "non_certifie");
        e["certification"] = json!({ "statut": "orchestra_certified", "version_certifiee": "0.9.0" });
        assert!(autorisation(&e, false).unwrap_err().contains("0.9.0"));
        e["certification"]["version_certifiee"] = json!("1.0.0");
        assert_eq!(autorisation(&e, false).unwrap(), "orchestra_certified");
        e["execution"] = json!({ "type": "heberge", "endpoint": "https://agent.exemple.fr/mission" });
        assert_eq!(autorisation(&e, false).unwrap(), "livrable_seul", "hébergé certifié : au mieux « livrable seul »");
        e["certification"] = json!({ "statut": "non_certifie" });
        assert!(autorisation(&e, false).unwrap_err().contains("hébergé"));
        e["certification"] = json!({ "statut": "livrable_seul" });
        assert_eq!(autorisation(&e, false).unwrap(), "livrable_seul");
        e["execution"]["endpoint"] = json!("http://agent.exemple.fr");
        assert!(autorisation(&e, true).unwrap_err().contains("HTTPS"));
    }

    /// Écrit, relit puis supprime un identifiant d'essai dans le vrai coffre de Windows.
    /// Lancé à la main : `cargo test --lib coffre -- --ignored`.
    #[test]
    #[ignore]
    fn coffre_de_windows() {
        let mut v = manifeste("1.0.0");
        v["identifiants"] = json!([{ "nom": "essai", "type": "cle_api", "raison": "essai du lot T4" }]);
        let m = Manifest::depuis_v2(&v).unwrap();
        assert!(!identifiants(&m)[0].present);
        assert!(enregistrer_identifiant(&m, "autre", "x").is_err());
        enregistrer_identifiant(&m, "essai", "valeur-essai").unwrap();
        assert!(identifiants(&m)[0].present);
        let e = entree_coffre("agent-essai", "essai").unwrap();
        assert_eq!(e.get_password().unwrap(), "valeur-essai");
        e.delete_credential().unwrap();
        assert!(!identifiants(&m)[0].present);
    }

    #[test]
    fn empreinte_controlee() {
        let data = b"paquet";
        let sha = sha256_hex(data);
        assert!(verifier_paquet(data, 6, &sha).is_ok());
        assert!(verifier_paquet(data, 7, &sha).unwrap_err().contains("taille"));
        let faux = format!("{}0", &sha[..63]);
        assert!(verifier_paquet(data, 6, &faux).unwrap_err().contains("SHA-256"));
    }

    #[test]
    fn installation_puis_mise_a_jour_garde_la_memoire() {
        let (paths, racine) = paths_essai();
        let p1 = paquet(&manifeste("1.0.0"), true);
        let i = installer_paquet(&paths, &p1, Some(&manifeste("1.0.0")), "non_certifie", "essai").unwrap();
        assert_eq!(i.manifest.version, "1.0.0");
        assert!(i.ancienne_version.is_none());
        let dossier = agents::dossier_installe(&paths, "agent-essai");
        assert!(dossier.join("agent.py").exists());
        // la mémoire du paquet est ignorée, un dossier vide est créé (memoire_propre)
        assert!(!dossier.join("memoire/piege.md").exists());
        std::fs::write(dossier.join("memoire/ton.md"), "cordial").unwrap();
        assert_eq!(agents::get_installed(&paths, "agent-essai").unwrap().certification, "non_certifie");

        let p2 = paquet(&manifeste("1.1.0"), true);
        let i = installer_paquet(&paths, &p2, Some(&manifeste("1.1.0")), "non_certifie", "essai").unwrap();
        assert_eq!(i.ancienne_version.as_deref(), Some("1.0.0"));
        assert_eq!(i.manifest.version, "1.1.0");
        assert_eq!(std::fs::read_to_string(dossier.join("memoire/ton.md")).unwrap(), "cordial");
        assert!(std::fs::read_dir(paths.agents.join("anciennes-versions")).unwrap().count() == 1);
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn paquets_refuses() {
        let (paths, racine) = paths_essai();
        // CHANGELOG absent
        let e = installer_paquet(&paths, &paquet(&manifeste("1.0.0"), false), None, "non_certifie", "essai").unwrap_err();
        assert!(e.contains("CHANGELOG"), "{e}");
        // version différente du catalogue
        let e = installer_paquet(&paths, &paquet(&manifeste("1.0.0"), true), Some(&manifeste("1.0.1")), "non_certifie", "essai").unwrap_err();
        assert!(e.contains("version"), "{e}");
        // champ manquant
        let mut m = manifeste("1.0.0");
        m.as_object_mut().unwrap().remove("licence");
        let e = installer_paquet(&paths, &paquet(&m, true), None, "non_certifie", "essai").unwrap_err();
        assert!(e.contains("licence"), "{e}");
        // format 3.x
        let mut m = manifeste("1.0.0");
        m["format_manifeste"] = json!("3.0");
        let e = installer_paquet(&paths, &paquet(&m, true), None, "non_certifie", "essai").unwrap_err();
        assert!(e.contains("incompatible"), "{e}");
        // pas un zip
        assert!(installer_paquet(&paths, b"pas un zip", None, "non_certifie", "essai").unwrap_err().contains("illisible"));
        // rien n'a été installé, aucun dossier temporaire laissé
        assert!(agents::installed(&paths).is_empty());
        assert!(std::fs::read_dir(&paths.agents).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(".installation")));
        let _ = std::fs::remove_dir_all(racine);
    }
}
