//! Suivi des projets par Orchestra (lot T9, D23).
//!
//! Lecture seule des dossiers de projet et de mission : lister, lire un fichier (texte, code,
//! .md ; .docx, .xlsx et .pdf par extraction du texte), historique des missions d'un projet.
//! Gros fichier : lecture ciblée (structure, passages recherchés, page suivante), jamais tout
//! d'un coup. Orchestra ne modifie jamais un livrable : rien ici n'écrit dans un projet, sauf
//! `SUIVI.md` qu'Orchestra seul tient (résumé de chaque mission, état, livrables, prochaines étapes).

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::paths::Paths;

/// Texte rendu au modèle par lecture (≈ 2 000 tokens).
pub const PAGE: usize = 6000;
const LISTE_MAX: usize = 200;
const FICHIER_MAX: u64 = 100_000_000;

/// Projets existants (dossiers de `projets/`).
pub fn projets(paths: &Paths) -> Vec<String> {
    let mut l: Vec<String> = std::fs::read_dir(&paths.projets).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    l.sort();
    l
}

/// Chemin sûr dans un projet : jamais en dehors (`..`, chemin absolu refusés).
fn dans_projet(paths: &Paths, projet: &str, chemin: &str) -> Result<PathBuf, String> {
    let nom = crate::paths::safe_name(projet.trim());
    let racine = paths.projets.join(&nom);
    if nom.is_empty() || !racine.is_dir() {
        let dispo = projets(paths);
        return Err(format!("projet « {projet} » introuvable. Projets existants : {}", if dispo.is_empty() { "aucun".into() } else { dispo.join(", ") }));
    }
    let rel = Path::new(chemin.trim().trim_start_matches(['/', '\\']));
    if rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        return Err("chemin refusé : il doit rester dans le dossier du projet".into());
    }
    Ok(racine.join(rel))
}

/// Contenu d'un dossier de projet (2 niveaux), avec la taille des fichiers.
pub fn lister(paths: &Paths, projet: &str, chemin: &str) -> Result<String, String> {
    let d = dans_projet(paths, projet, chemin)?;
    if !d.is_dir() {
        return Err(format!("« {chemin} » n'est pas un dossier du projet"));
    }
    let mut lignes = vec![];
    fn parcourir(racine: &Path, d: &Path, niveau: usize, lignes: &mut Vec<String>) {
        let mut entrees: Vec<_> = std::fs::read_dir(d).into_iter().flatten().flatten().collect();
        entrees.sort_by_key(|e| e.file_name());
        for e in entrees {
            if lignes.len() >= LISTE_MAX {
                return;
            }
            let p = e.path();
            let rel = p.strip_prefix(racine).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            if p.is_dir() {
                lignes.push(format!("{}{rel}/", "  ".repeat(niveau)));
                if niveau < 1 {
                    parcourir(racine, &p, niveau + 1, lignes);
                }
            } else {
                let taille = e.metadata().map(|m| m.len()).unwrap_or(0);
                lignes.push(format!("{}{rel} ({})", "  ".repeat(niveau), taille_lisible(taille)));
            }
        }
    }
    let racine = paths.projets.join(crate::paths::safe_name(projet.trim()));
    parcourir(&racine, &d, 0, &mut lignes);
    if lignes.is_empty() {
        return Ok("(dossier vide)".into());
    }
    if lignes.len() >= LISTE_MAX {
        lignes.push(format!("… (liste limitée à {LISTE_MAX} entrées)"));
    }
    Ok(lignes.join("\n"))
}

fn taille_lisible(n: u64) -> String {
    match n {
        n if n < 1024 => format!("{n} o"),
        n if n < 1024 * 1024 => format!("{:.0} Ko", n as f64 / 1024.0),
        n => format!("{:.1} Mo", n as f64 / 1024.0 / 1024.0),
    }
}

/// Texte d'un fichier : brut, ou extrait des .docx, .xlsx et .pdf.
pub fn texte_de(p: &Path) -> Result<String, String> {
    let taille = std::fs::metadata(p).map_err(|e| e.to_string())?.len();
    if taille > FICHIER_MAX {
        return Err(format!("fichier de {} : trop gros pour être lu", taille_lisible(taille)));
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "docx" => texte_docx(p),
        "xlsx" => texte_xlsx(p),
        "pdf" => pdf_extract::extract_text(p).map_err(|e| format!("PDF illisible : {e}")),
        _ => {
            let octets = std::fs::read(p).map_err(|e| e.to_string())?;
            if octets.iter().take(8000).any(|b| *b == 0) {
                return Err(format!("fichier binaire ({}) : pas de texte à lire", if ext.is_empty() { "sans extension".into() } else { format!(".{ext}") }));
            }
            Ok(String::from_utf8_lossy(&octets).to_string())
        }
    }
}

fn entree_zip(p: &Path, nom: &str) -> Result<Option<String>, String> {
    let f = std::fs::File::open(p).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("document illisible ({e})"))?;
    let r = match z.by_name(nom) {
        Ok(mut e) => {
            let mut s = String::new();
            e.read_to_string(&mut s).map_err(|e| e.to_string())?;
            Some(s)
        }
        Err(_) => None,
    };
    Ok(r)
}

/// Texte entre balises XML, avec les entités courantes décodées.
fn decoder(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// .docx : paragraphes de `word/document.xml` (texte des `<w:t>`), un par ligne.
fn texte_docx(p: &Path) -> Result<String, String> {
    let xml = entree_zip(p, "word/document.xml")?.ok_or("document Word sans word/document.xml")?;
    let mut out = String::new();
    for para in xml.split("</w:p>") {
        let mut ligne = String::new();
        let mut reste = para;
        while let Some(i) = reste.find("<w:t") {
            let apres = &reste[i..];
            // <w:t> ou <w:t xml:space="preserve"> (pas <w:tab/> ni <w:tbl>)
            let Some(fin_balise) = apres.find('>') else { break };
            let balise = &apres[..fin_balise];
            if !(balise == "<w:t" || balise.starts_with("<w:t ")) || balise.ends_with('/') {
                reste = &apres[fin_balise + 1..];
                continue;
            }
            let contenu = &apres[fin_balise + 1..];
            let Some(j) = contenu.find("</w:t>") else { break };
            ligne.push_str(&decoder(&contenu[..j]));
            reste = &contenu[j..];
        }
        if para.contains("<w:tab/>") && ligne.is_empty() {
            continue;
        }
        if !ligne.trim().is_empty() {
            out.push_str(ligne.trim_end());
            out.push('\n');
        }
    }
    Ok(out)
}

/// .xlsx : chaque feuille, ligne par ligne, cellules séparées par « | ».
fn texte_xlsx(p: &Path) -> Result<String, String> {
    let partagees: Vec<String> = entree_zip(p, "xl/sharedStrings.xml")?
        .map(|x| x.split("</si>").filter(|si| si.contains("<t")).map(|si| textes_t(si).concat()).collect())
        .unwrap_or_default();
    let mut out = String::new();
    for n in 1..=50 {
        let Some(xml) = entree_zip(p, &format!("xl/worksheets/sheet{n}.xml"))? else { break };
        out.push_str(&format!("## Feuille {n}\n"));
        for row in xml.split("</row>") {
            let mut cellules = vec![];
            for c in row.split("<c ").skip(1) {
                let partagee = c.split('>').next().unwrap_or("").contains("t=\"s\"");
                let enligne = c.split('>').next().unwrap_or("").contains("t=\"inlineStr\"");
                let valeur = if enligne {
                    textes_t(c).concat()
                } else {
                    c.split("<v>").nth(1).and_then(|v| v.split("</v>").next()).map(decoder).unwrap_or_default()
                };
                let v = if partagee { valeur.parse::<usize>().ok().and_then(|i| partagees.get(i).cloned()).unwrap_or(valeur) } else { valeur };
                cellules.push(v);
            }
            if cellules.iter().any(|c| !c.trim().is_empty()) {
                out.push_str(&cellules.join(" | "));
                out.push('\n');
            }
        }
    }
    Ok(out)
}

fn textes_t(xml: &str) -> Vec<String> {
    let mut v = vec![];
    let mut reste = xml;
    while let Some(i) = reste.find("<t") {
        let apres = &reste[i..];
        let Some(f) = apres.find('>') else { break };
        let balise = &apres[..f];
        if !(balise == "<t" || balise.starts_with("<t ")) {
            reste = &apres[f + 1..];
            continue;
        }
        let contenu = &apres[f + 1..];
        let Some(j) = contenu.find("</t>") else { break };
        v.push(decoder(&contenu[..j]));
        reste = &contenu[j..];
    }
    v
}

/// Lecture ciblée d'un fichier du projet :
/// - `chercher` : passages qui contiennent ces mots (avec un peu de contexte) ;
/// - sinon, la page `page` (1 par défaut) ; si le fichier a plusieurs pages, sa structure
///   (titres, nombre de pages) est donnée en tête pour choisir la suite.
pub fn lire(paths: &Paths, projet: &str, chemin: &str, chercher: Option<&str>, page: usize) -> Result<String, String> {
    let p = dans_projet(paths, projet, chemin)?;
    if !p.is_file() {
        return Err(format!("« {chemin} » n'est pas un fichier du projet. Utilise projet_lister pour voir les fichiers."));
    }
    let texte = texte_de(&p)?;
    let car: Vec<char> = texte.chars().collect();
    let pages = car.len().div_ceil(PAGE).max(1);
    if let Some(mots) = chercher.map(str::trim).filter(|m| !m.is_empty()) {
        return Ok(passages(&texte, mots, chemin));
    }
    let page = page.clamp(1, pages);
    let morceau: String = car.iter().skip((page - 1) * PAGE).take(PAGE).collect();
    let mut s = String::new();
    if pages > 1 {
        s.push_str(&format!("[{chemin} : {} caractères, page {page} sur {pages}. Pour la suite, rappelle projet_lire avec page={}, ou cherche des mots avec chercher.]\n", car.len(), (page + 1).min(pages)));
        if page == 1 {
            let titres: Vec<&str> = texte.lines().filter(|l| l.starts_with('#') || l.starts_with("## Feuille")).take(30).collect();
            if !titres.is_empty() {
                s.push_str(&format!("Structure : {}\n", titres.join(" · ")));
            }
        }
    }
    s.push_str(&morceau);
    Ok(s)
}

/// Passages qui contiennent au moins un des mots cherchés (sans accents ni majuscules).
fn passages(texte: &str, mots: &str, chemin: &str) -> String {
    let cles: Vec<String> = crate::memory::keywords(mots).into_iter().collect();
    let lignes: Vec<&str> = texte.lines().collect();
    // les lignes qui contiennent le plus de mots cherchés d'abord (tous les mots, si possible)
    let score = |l: &str| {
        let f = crate::memory::fold(l);
        if cles.is_empty() { usize::from(f.contains(&crate::memory::fold(mots))) } else { cles.iter().filter(|c| f.contains(c.as_str())).count() }
    };
    let meilleur = lignes.iter().map(|l| score(l)).max().unwrap_or(0);
    let mut gardees = std::collections::BTreeSet::new();
    for (i, l) in lignes.iter().enumerate() {
        if meilleur > 0 && score(l) == meilleur {
            for k in i.saturating_sub(2)..=(i + 2).min(lignes.len().saturating_sub(1)) {
                gardees.insert(k);
            }
        }
    }
    if gardees.is_empty() {
        return format!("Aucun passage de {chemin} ne contient « {mots} ».");
    }
    let mut s = format!("Passages de {chemin} qui contiennent « {mots} » :\n");
    let mut precedente = None;
    for k in gardees {
        if precedente.is_some_and(|p: usize| k > p + 1) {
            s.push_str("…\n");
        }
        s.push_str(lignes[k]);
        s.push('\n');
        precedente = Some(k);
        if s.len() > PAGE {
            s.push_str("… (passages tronqués : précise la recherche)\n");
            break;
        }
    }
    s
}

/// Historique des missions d'un projet (de la plus ancienne à la plus récente) avec leur résumé.
pub fn historique(paths: &Paths, projet: &str) -> Result<String, String> {
    let nom = crate::paths::safe_name(projet.trim());
    dans_projet(paths, projet, "")?;
    let etat = lire_etat(paths, &nom);
    let mut s = String::new();
    if !etat.missions.is_empty() {
        for m in &etat.missions {
            s.push_str(&format!(
                "- {} · {} · {} · état {} · dossier missions/{}\n  fichiers produits : {}\n  {}\n",
                m.date,
                m.agent,
                m.but,
                m.etat,
                m.mission_id,
                if m.livrables.is_empty() { "aucun".to_string() } else { m.livrables.join(", ") },
                m.resume.replace('\n', "\n  ")
            ));
        }
    }
    // missions sans résumé (avant le lot T9) : lues dans leur dossier
    let connues: Vec<&str> = etat.missions.iter().map(|m| m.mission_id.as_str()).collect();
    let mut autres = vec![];
    for e in std::fs::read_dir(&paths.missions).into_iter().flatten().flatten() {
        let d = e.path();
        let id = e.file_name().to_string_lossy().to_string();
        if connues.contains(&id.as_str()) {
            continue;
        }
        let m: Value = std::fs::read_to_string(d.join("mission.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if Path::new(m["projet"].as_str().unwrap_or("")).file_name().map(|n| n.to_string_lossy().to_string()) != Some(nom.clone()) {
            continue;
        }
        let cr: Value = std::fs::read_to_string(d.join("compte_rendu.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        autres.push(format!(
            "- {} · {} · {} · état {} · fichiers : {}",
            m["cree_le"].as_str().unwrap_or("").get(..16).unwrap_or(""),
            m["agent_id"].as_str().unwrap_or(""),
            m["but"].as_str().unwrap_or(""),
            cr["etat"].as_str().unwrap_or("inconnu"),
            cr.pointer("/resultat/fichiers").and_then(|f| f.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
        ));
    }
    autres.sort();
    for a in autres {
        s.push_str(&a);
        s.push('\n');
    }
    if s.is_empty() {
        return Ok(format!("Aucune mission sur le projet « {nom} »."));
    }
    Ok(format!("Missions du projet « {nom} » :\n{s}"))
}

// ---------- SUIVI.md du projet ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MissionSuivie {
    pub mission_id: String,
    pub date: String,
    pub agent: String,
    pub but: String,
    pub etat: String,
    pub resume: String,
    pub livrables: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EtatProjet {
    pub etat: String,
    pub prochaines_etapes: Vec<String>,
    pub missions: Vec<MissionSuivie>,
}

/// L'état du suivi est gardé dans les données d'Orchestra ; `SUIVI.md` du projet en est la vue.
fn chemin_etat(paths: &Paths, projet: &str) -> PathBuf {
    paths.root.join("suivi").join(format!("{}.json", crate::paths::safe_name(projet)))
}

pub fn lire_etat(paths: &Paths, projet: &str) -> EtatProjet {
    std::fs::read_to_string(chemin_etat(paths, projet)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Schéma de la sortie contrainte du résumé de fin de mission.
pub fn schema_resume() -> Value {
    json!({
        "type": "object",
        "properties": {
            "resume": { "type": "string", "description": "5 à 10 lignes : ce qui a été fait, avec les faits utiles du livrable (noms, chiffres, décisions)" },
            "etat": { "type": "string", "description": "une phrase : où en est le projet" },
            "prochaines_etapes": { "type": "array", "items": { "type": "string" }, "description": "0 à 3 prochaines étapes, seulement si elles découlent de la mission" }
        },
        "required": ["resume", "etat", "prochaines_etapes"]
    })
}

/// Données données au modèle pour écrire le résumé : mission, compte rendu, début des livrables.
pub fn matiere_resume(projet_dossier: &Path, mission: &Value, cr: &Value) -> String {
    let mut s = format!(
        "Mission : {}\nAgent : {}\nÉtat : {}{}\nRésultat de l'agent :\n{}\n",
        mission["but"].as_str().unwrap_or(""),
        mission["agent_id"].as_str().unwrap_or(""),
        cr["etat"].as_str().unwrap_or(""),
        cr["motif"].as_str().map(|m| format!(" ({m})")).unwrap_or_default(),
        cr.pointer("/resultat/texte").and_then(|t| t.as_str()).unwrap_or("")
    );
    for f in cr.pointer("/resultat/fichiers").and_then(|f| f.as_array()).into_iter().flatten().filter_map(|f| f.as_str()).take(5) {
        let p = if Path::new(f).is_absolute() { PathBuf::from(f) } else { projet_dossier.join(f) };
        match texte_de(&p) {
            Ok(t) => s.push_str(&format!("\nLivrable {f} (début) :\n{}\n", t.chars().take(1500).collect::<String>())),
            Err(_) => s.push_str(&format!("\nLivrable {f} : {}\n", if p.exists() { "non lisible comme texte" } else { "absent (supprimé ou déplacé)" })),
        }
    }
    s
}

/// Ajoute la mission au suivi et réécrit `projets/<nom>/SUIVI.md` (seul fichier qu'Orchestra
/// écrit dans un projet ; il ne touche jamais aux livrables).
pub fn enregistrer(paths: &Paths, projet: &str, m: MissionSuivie, etat: &str, prochaines: Vec<String>) -> anyhow::Result<PathBuf> {
    let mut e = lire_etat(paths, projet);
    e.missions.retain(|x| x.mission_id != m.mission_id);
    e.missions.push(m);
    if !etat.trim().is_empty() {
        e.etat = etat.trim().to_string();
    }
    e.prochaines_etapes = prochaines;
    let f = chemin_etat(paths, projet);
    std::fs::create_dir_all(f.parent().unwrap_or(&paths.root))?;
    std::fs::write(&f, serde_json::to_string_pretty(&e)?)?;
    let md = rendre(projet, &e);
    let dest = paths.projets.join(crate::paths::safe_name(projet)).join("SUIVI.md");
    std::fs::write(&dest, md)?;
    Ok(dest)
}

fn rendre(projet: &str, e: &EtatProjet) -> String {
    let mut s = format!("# Suivi du projet {projet}\n\n> Tenu par Orchestra après chaque mission. Les agents n'écrivent pas ce fichier.\n\n## État\n\n{}\n\n## Prochaines étapes\n\n", if e.etat.is_empty() { "—" } else { &e.etat });
    if e.prochaines_etapes.is_empty() {
        s.push_str("—\n");
    }
    for p in &e.prochaines_etapes {
        s.push_str(&format!("- {p}\n"));
    }
    s.push_str("\n## Livrables\n\n");
    let mut livrables: Vec<(&str, &str)> = vec![];
    for m in &e.missions {
        for l in &m.livrables {
            livrables.retain(|(x, _)| x != l);
            livrables.push((l, &m.date));
        }
    }
    if livrables.is_empty() {
        s.push_str("—\n");
    }
    for (l, d) in livrables {
        s.push_str(&format!("- `{l}` ({d})\n"));
    }
    s.push_str("\n## Missions\n");
    for m in e.missions.iter().rev() {
        s.push_str(&format!("\n### {} · {} · {}\n\n{} · état {} · dossier `missions/{}`\n\n{}\n", m.date, m.agent, m.but, m.agent, m.etat, m.mission_id, m.resume.trim()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn paths_essai() -> (Paths, PathBuf) {
        let r = std::env::temp_dir().join(format!("orchestra-t9-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let p = Paths::at(r.clone()).unwrap();
        std::fs::create_dir_all(p.projets.join("salon")).unwrap();
        (p, r)
    }

    fn zip(f: &Path, entrees: &[(&str, &str)]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(f).unwrap());
        for (n, c) in entrees {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(c.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn lecture_seule_et_chemins_surs() {
        let (p, r) = paths_essai();
        let d = p.projets.join("salon");
        std::fs::write(d.join("plan.md"), "# Plan\n## Budget\nBudget total : 12 000 €\n## Lieu\nParis, porte de Versailles").unwrap();
        assert!(lister(&p, "salon", "").unwrap().contains("plan.md"));
        assert!(lire(&p, "salon", "../../config.json", None, 1).unwrap_err().contains("refusé"));
        assert!(lire(&p, "inconnu", "plan.md", None, 1).unwrap_err().contains("salon"));
        assert!(lire(&p, "salon", "plan.md", None, 1).unwrap().contains("12 000"));
        let r2 = lire(&p, "salon", "plan.md", Some("lieu"), 1).unwrap();
        assert!(r2.contains("Versailles"), "{r2}");
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn gros_fichier_par_pages() {
        let (p, r) = paths_essai();
        let texte: String = (0..2000).map(|i| format!("# Partie {i}\nligne {i} du rapport\n")).collect();
        std::fs::write(p.projets.join("salon").join("rapport.md"), &texte).unwrap();
        let p1 = lire(&p, "salon", "rapport.md", None, 1).unwrap();
        assert!(p1.contains("page 1 sur") && p1.contains("Structure") && p1.len() < PAGE * 3);
        let p2 = lire(&p, "salon", "rapport.md", None, 2).unwrap();
        assert!(p2.contains("page 2 sur"));
        let c = lire(&p, "salon", "rapport.md", Some("Partie 1999"), 1).unwrap();
        assert!(c.contains("ligne 1999"), "{c}");
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn docx_et_xlsx() {
        let (p, r) = paths_essai();
        let d = p.projets.join("salon");
        zip(&d.join("compte-rendu.docx"), &[("word/document.xml", r#"<w:document><w:body><w:p><w:r><w:t>Réunion du 12 &amp; 13 mars</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">Décision : </w:t></w:r><w:r><w:t>stand B12</w:t></w:r></w:p></w:body></w:document>"#)]);
        let t = texte_de(&d.join("compte-rendu.docx")).unwrap();
        assert_eq!(t, "Réunion du 12 & 13 mars\nDécision : stand B12\n");
        zip(
            &d.join("budget.xlsx"),
            &[
                ("xl/sharedStrings.xml", "<sst><si><t>Poste</t></si><si><t>Montant</t></si><si><t>Stand</t></si></sst>"),
                ("xl/worksheets/sheet1.xml", r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>4500</v></c></row></sheetData></worksheet>"#),
            ],
        );
        let t = texte_de(&d.join("budget.xlsx")).unwrap();
        assert!(t.contains("Poste | Montant") && t.contains("Stand | 4500"), "{t}");
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn suivi_md_tenu_par_orchestra() {
        let (p, r) = paths_essai();
        let m = |id: &str, livrable: &str| MissionSuivie { mission_id: id.into(), date: "2026-10-31 17:00".into(), agent: "agent-brouillons".into(), but: "Brouillon".into(), etat: "termine".into(), resume: "Brouillon écrit.".into(), livrables: vec![livrable.into()] };
        enregistrer(&p, "salon", m("m1", "brouillons/a.md"), "Premier brouillon prêt.", vec!["Relire".into()]).unwrap();
        let f = enregistrer(&p, "salon", m("m2", "brouillons/b.md"), "Deux brouillons prêts.", vec![]).unwrap();
        let s = std::fs::read_to_string(f).unwrap();
        assert!(s.contains("Deux brouillons prêts.") && s.contains("brouillons/a.md") && s.contains("brouillons/b.md") && s.matches("### ").count() == 2);
        let h = historique(&p, "salon").unwrap();
        assert!(h.contains("missions/m1") && h.contains("missions/m2"));
        let _ = std::fs::remove_dir_all(r);
    }
}
