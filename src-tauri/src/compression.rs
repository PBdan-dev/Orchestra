//! Compression du niveau froid (lot M1) : zstd niveau 19, à J+1, des archives de conversation, des
//! missions condensées des agents, du journal et des traces de mission ; la pensée brute d'une
//! mission dès sa fin. Le fichier du jour reste en clair ; faits, résumés et fiches restent en `.md`.
//!
//! Lecture transparente : `lire_texte(p)` lit `p`, ou à défaut `p.zst`. Hors d'Orchestra, un `.zst`
//! s'ouvre avec PeaZip, 7-Zip (version zstd) ou `zstd -d fichier.zst`.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::paths::Paths;

/// Niveau de zstd (cahier, M1).
pub const NIVEAU: i32 = 19;

/// Chemin compressé d'un fichier (`x.md` → `x.md.zst`).
pub fn chemin_zst(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".zst");
    PathBuf::from(s)
}

/// Le fichier tel qu'il est sur le disque : en clair s'il existe, sinon compressé.
pub fn chemin_reel(p: &Path) -> PathBuf {
    if p.exists() {
        p.to_path_buf()
    } else {
        let z = chemin_zst(p);
        if z.exists() {
            z
        } else {
            p.to_path_buf()
        }
    }
}

/// Le fichier existe, en clair ou compressé.
pub fn existe(p: &Path) -> bool {
    p.exists() || chemin_zst(p).exists()
}

/// Lit un fichier texte en clair ou compressé (`p` d'abord, sinon `p.zst`).
pub fn lire_texte(p: &Path) -> std::io::Result<String> {
    if p.exists() {
        return std::fs::read_to_string(p);
    }
    let octets = zstd::decode_all(std::fs::File::open(chemin_zst(p))?)?;
    String::from_utf8(octets).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Compresse un fichier : écrit `p.zst` (fichier temporaire, puis renommage), le relit et le compare,
/// puis retire `p`. Renvoie les tailles avant et après.
pub fn compresser(paths: &Paths, p: &Path) -> std::io::Result<(u64, u64)> {
    let clair = std::fs::read(p)?;
    let compresse = zstd::encode_all(clair.as_slice(), NIVEAU)?;
    if zstd::decode_all(compresse.as_slice())? != clair {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "relecture différente : fichier laissé en clair"));
    }
    let z = chemin_zst(p);
    let mut tmp = z.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // écrit par Orchestra : jamais compté comme une modification par un agent (lot S-b)
    for f in [p, z.as_path(), tmp.as_path()] {
        crate::surveillance::noter(paths, f);
    }
    std::fs::write(&tmp, &compresse)?;
    std::fs::rename(&tmp, &z)?;
    std::fs::remove_file(p)?;
    Ok((clair.len() as u64, compresse.len() as u64))
}

/// Écrit un fichier en clair (modification par PB depuis l'interface) : une version compressée
/// éventuelle est retirée ; un fichier daté du passé sera recompressé à la prochaine passe.
pub fn ecrire_texte(paths: &Paths, p: &Path, texte: &str) -> std::io::Result<()> {
    let z = chemin_zst(p);
    crate::surveillance::noter(paths, p);
    crate::surveillance::noter(paths, &z);
    std::fs::write(p, texte)?;
    if z.exists() {
        std::fs::remove_file(z)?;
    }
    Ok(())
}

/// Fin de mission : la pensée brute de l'agent est compressée tout de suite (cahier, M1).
pub fn fin_de_mission(paths: &Paths, dossier_mission: &Path) -> Option<(u64, u64)> {
    let pensee = dossier_mission.join("pensee.md");
    pensee.exists().then(|| compresser(paths, &pensee).ok()).flatten()
}

/// Date en tête d'un nom de fichier ou de dossier (`AAAA-MM-JJ…`), ou après `_` (`agent_AAAA-MM-JJ.md`).
fn date_du_nom(nom: &str) -> Option<NaiveDate> {
    let essai = |s: &str| s.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    essai(nom).or_else(|| nom.rsplit_once('_').and_then(|(_, fin)| essai(fin)))
}

fn fichiers(d: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()
}

/// Trace de mission à compresser : `.jsonl`, `.chaine`, `.md` (sauf `verification.md`), `.log`, `.txt`.
fn trace_de_mission(p: &Path) -> bool {
    let nom = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    nom != "verification.md" && [".jsonl", ".chaine", ".md", ".log", ".txt"].iter().any(|x| nom.ends_with(x))
}

/// Fichiers dus à J+1 : archives de conversation et missions condensées des agents, journal (et sa
/// chaîne), traces des missions (`.jsonl`, `.chaine`, `.md` sauf `verification.md`, `.log`, `.txt`),
/// sauf la mission en cours. Les petits fichiers JSON des missions restent en clair (liste des missions).
pub fn dus(paths: &Paths, aujourd_hui: NaiveDate, mission_en_cours: Option<&str>) -> Vec<PathBuf> {
    let passe = |p: &Path| p.file_name().and_then(|n| date_du_nom(&n.to_string_lossy())).is_some_and(|d| d < aujourd_hui);
    let pas_compresse = |p: &PathBuf| p.extension().is_none_or(|x| x != "zst" && x != "tmp");
    let mut out: Vec<PathBuf> = vec![];
    for d in [paths.archives.clone(), paths.archives.join("agents")] {
        out.extend(fichiers(&d).into_iter().filter(|p| pas_compresse(p) && p.extension().is_some_and(|x| x == "md") && passe(p)));
    }
    out.extend(fichiers(&paths.journal).into_iter().filter(|p| pas_compresse(p) && passe(p) && {
        let n = p.to_string_lossy();
        n.ends_with(".jsonl") || n.ends_with(".jsonl.chaine")
    }));
    for m in std::fs::read_dir(&paths.missions).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        let nom = m.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if mission_en_cours == Some(nom.as_str()) || !passe(&m) {
            continue;
        }
        out.extend(fichiers(&m).into_iter().filter(|p| pas_compresse(p) && trace_de_mission(p)));
    }
    out.sort();
    out
}

/// Bilan d'une passe de compression.
#[derive(Debug, Default, PartialEq)]
pub struct Bilan {
    pub fichiers: usize,
    pub avant: u64,
    pub apres: u64,
    pub erreurs: Vec<String>,
}

/// Compresse tout ce qui est dû.
pub fn compresser_dus(paths: &Paths, aujourd_hui: NaiveDate, mission_en_cours: Option<&str>) -> Bilan {
    let mut b = Bilan::default();
    for f in dus(paths, aujourd_hui, mission_en_cours) {
        match compresser(paths, &f) {
            Ok((avant, apres)) => {
                b.fichiers += 1;
                b.avant += avant;
                b.apres += apres;
            }
            Err(e) => b.erreurs.push(format!("{} : {e}", f.strip_prefix(&paths.root).unwrap_or(&f).display())),
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier() -> (Paths, PathBuf) {
        let racine = std::env::temp_dir().join(format!("orchestra-m1-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        (Paths::at(racine.clone()).unwrap(), racine)
    }

    fn jour(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn ecrire(p: &Path, texte: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, texte).unwrap();
    }

    #[test]
    fn compresse_relit_et_retire_le_clair() {
        let (p, racine) = dossier();
        let f = p.archives.join("2026-10-30.md");
        let texte = "# Conversations du 2026-10-30\n\n**Utilisateur :** bonjour, prépare le salon\n\n".repeat(200);
        ecrire(&f, &texte);
        let (avant, apres) = compresser(&p, &f).unwrap();
        assert!(!f.exists() && chemin_zst(&f).exists());
        assert_eq!(avant, texte.len() as u64);
        assert!(apres * 10 < avant, "{avant} → {apres}");
        assert_eq!(lire_texte(&f).unwrap(), texte, "lecture transparente");
        assert!(existe(&f) && chemin_reel(&f) == chemin_zst(&f));
        // en clair : lu tel quel
        let g = p.archives.join("2026-11-01.md");
        ecrire(&g, "du jour");
        assert_eq!(lire_texte(&g).unwrap(), "du jour");
        assert_eq!(chemin_reel(&g), g);
        assert!(lire_texte(&p.archives.join("absent.md")).is_err() && !existe(&p.archives.join("absent.md")));
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn modifie_depuis_l_interface_et_pensee_de_fin_de_mission() {
        let (p, racine) = dossier();
        let f = p.archives.join("2026-10-30.md");
        ecrire(&f, "ancien texte");
        compresser(&p, &f).unwrap();
        // PB corrige une archive compressée dans la vue Mémoire : écrite en clair, l'ancienne version retirée
        ecrire_texte(&p, &f, "texte corrigé").unwrap();
        assert!(f.exists() && !chemin_zst(&f).exists());
        assert_eq!(lire_texte(&f).unwrap(), "texte corrigé");
        // fin de mission : la pensée brute est compressée tout de suite, le reste attend J+1
        let m = p.missions.join("2026-11-01_0900_agent-x_abc");
        ecrire(&m.join("pensee.md"), &"je réfléchis\n".repeat(50));
        ecrire(&m.join("echanges.jsonl"), "{}\n");
        assert!(fin_de_mission(&p, &m).is_some());
        assert!(chemin_zst(&m.join("pensee.md")).exists() && m.join("echanges.jsonl").exists());
        assert!(fin_de_mission(&p, &m).is_none(), "déjà fait");
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn fichiers_dus_a_j_plus_1() {
        let (p, racine) = dossier();
        for f in [
            p.archives.join("2026-10-30.md"),
            p.archives.join("2026-11-01.md"),
            p.archives.join("agents").join("agent-x_2026-10-30.md"),
            p.archives.join("agents").join("agent-x_2026-11-01.md"),
            p.journal.join("2026-10-30.jsonl"),
            p.journal.join("2026-10-30.jsonl.chaine"),
            p.journal.join("2026-11-01.jsonl"),
        ] {
            ecrire(&f, "x\n");
        }
        let m = p.missions.join("2026-10-30_1200_agent-x_abc");
        for n in ["echanges.jsonl", "echanges.jsonl.chaine", "actions.jsonl", "pensee.md", "sortie-erreur.log", "mission.json", "compte_rendu.json", "verification.md", "reprise.json"] {
            ecrire(&m.join(n), "x\n");
        }
        let en_cours = p.missions.join("2026-10-30_2350_agent-y_def");
        ecrire(&en_cours.join("echanges.jsonl"), "x\n");
        ecrire(&p.missions.join("2026-11-01_0900_agent-x_ghi").join("echanges.jsonl"), "x\n");
        let mut d: Vec<String> = dus(&p, jour("2026-11-01"), Some("2026-10-30_2350_agent-y_def")).iter().map(|f| f.strip_prefix(&p.root).unwrap().to_string_lossy().replace('\\', "/")).collect();
        d.sort();
        assert_eq!(
            d,
            vec![
                "journal/2026-10-30.jsonl",
                "journal/2026-10-30.jsonl.chaine",
                "memoire/archives/2026-10-30.md",
                "memoire/archives/agents/agent-x_2026-10-30.md",
                "missions/2026-10-30_1200_agent-x_abc/actions.jsonl",
                "missions/2026-10-30_1200_agent-x_abc/echanges.jsonl",
                "missions/2026-10-30_1200_agent-x_abc/echanges.jsonl.chaine",
                "missions/2026-10-30_1200_agent-x_abc/pensee.md",
                "missions/2026-10-30_1200_agent-x_abc/sortie-erreur.log",
            ]
        );
        // une passe compresse tout ce qui est dû ; la suivante ne trouve plus rien
        let b = compresser_dus(&p, jour("2026-11-01"), Some("2026-10-30_2350_agent-y_def"));
        assert_eq!((b.fichiers, b.erreurs.len()), (9, 0), "{b:?}");
        assert!(dus(&p, jour("2026-11-01"), None).iter().all(|f| f.starts_with(&en_cours)), "seule la mission alors en cours reste due");
        assert!(m.join("mission.json").exists() && m.join("verification.md").exists(), "petits fichiers lus par la liste des missions : en clair");
        let _ = std::fs::remove_dir_all(racine);
    }
}
