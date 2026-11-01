//! Consolidation de la mémoire à l'écriture (lot M3, D21).
//!
//! Avant d'écrire un fait dans `orchestra.md` : ses 5 voisins les plus proches, par les mots et par
//! le sens (lot M2). Si l'un est très proche, le cerveau décide en sortie contrainte : AJOUTER,
//! REMPLACER la ligne N ou IGNORER. Tout ce qui n'est pas net (réponse illisible, numéro hors liste,
//! cerveau injoignable, ligne changée entre-temps) vaut AJOUTER. Une ligne remplacée part dans
//! `memoire/historique.md` ; Conscience affiche le remplacement avec un bouton Annuler.

use std::time::Duration;

use serde_json::{json, Value};

use crate::{
    brain::{Brain, Msg},
    index::Voisin,
    memory,
    paths::Paths,
};

/// Très proche par les mots : au moins la moitié des mots utiles du nouveau fait, et 2 au moins.
pub const SEUIL_MOTS: f32 = 0.5;
/// Très proche par le sens.
pub const SEUIL_SENS: f32 = 0.75;

pub fn tres_proche(v: &Voisin) -> bool {
    (v.communs >= 2 && v.mots >= SEUIL_MOTS) || v.sens.is_some_and(|s| s >= SEUIL_SENS)
}

#[derive(Debug, PartialEq)]
pub enum Decision {
    Ajouter,
    /// Indice (à partir de 0) du voisin remplacé.
    Remplacer(usize),
    Ignorer,
}

/// Schéma de la décision (sortie contrainte, F2).
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "raison": { "type": "string" },
            "decision": { "type": "string", "enum": ["AJOUTER", "REMPLACER", "IGNORER"] },
            "ligne": { "type": "integer", "minimum": 0, "maximum": 5 }
        },
        "required": ["raison", "decision", "ligne"]
    })
}

/// Décision lue dans la sortie du cerveau ; tout ce qui n'est pas net vaut AJOUTER.
pub fn lire_decision(v: &Value, nb_voisins: usize) -> Decision {
    match v["decision"].as_str().unwrap_or("").trim().to_uppercase().as_str() {
        "REMPLACER" => match v["ligne"].as_u64() {
            Some(n) if n >= 1 && (n as usize) <= nb_voisins => Decision::Remplacer(n as usize - 1),
            _ => Decision::Ajouter,
        },
        "IGNORER" => Decision::Ignorer,
        _ => Decision::Ajouter,
    }
}

/// Messages de la décision : le nouveau fait et ses voisins numérotés.
pub fn consigne(section: &str, fait: &str, voisins: &[Voisin]) -> Vec<Msg> {
    let lignes: Vec<String> = voisins.iter().enumerate().map(|(i, v)| format!("{}. [{}] {}", i + 1, v.section, v.texte)).collect();
    vec![
        Msg::system("Tu tiens la mémoire d'Orchestra : un fait durable par ligne, daté. Tu décides si un nouveau fait s'ajoute, remplace une ligne existante ou n'apporte rien."),
        Msg::user(format!(
            "[Tâche : CONSOLIDER]\nNouveau fait (section « {section} ») : {fait}\n\nLignes proches déjà en mémoire :\n{}\n\nDécide :\n- REMPLACER : le nouveau fait porte sur le même sujet qu'une ligne mais avec une autre valeur (un nombre, un lieu, un nom, un jour, un modèle qui a changé, ou une correction) : cette ligne devient fausse. Donne son numéro.\n- IGNORER : une ligne dit déjà la même chose, même formulée autrement. Donne son numéro.\n- AJOUTER : le nouveau fait apporte une information en plus, sans rendre aucune ligne fausse (numéro 0).\nEn cas de doute, AJOUTER.\nDonne d'abord la raison en une phrase, puis la décision, puis le numéro de la ligne (0 si aucune).",
            lignes.join("\n")
        )),
    ]
}

/// Ce qui a été fait du nouveau fait.
#[derive(Debug, PartialEq)]
pub enum Issue {
    Ajoute { ligne: String },
    Remplace { ancien: String, nouveau: String, raison: String },
    Ignore { deja: String },
}

/// Écrit un fait dans `orchestra.md` après consolidation. Sans cerveau, AJOUTER.
pub async fn ecrire(brain: Option<&Brain>, paths: &Paths, section: &str, fait: &str, delai_sens: Duration) -> anyhow::Result<Issue> {
    let ajouter = || -> anyhow::Result<Issue> { Ok(Issue::Ajoute { ligne: memory::append_fact(paths, section, fait)? }) };
    let voisins = crate::index::voisins(paths, fait, 5, delai_sens).await;
    let Some(brain) = brain.filter(|_| voisins.iter().any(tres_proche)) else {
        return ajouter();
    };
    let (decision, raison) = match brain.json_avec("consolidation", &consigne(section, fait, &voisins), &schema(), true, |_| {}).await {
        Ok((v, _)) => (lire_decision(&v, voisins.len()), v["raison"].as_str().unwrap_or("").trim().to_string()),
        Err(_) => (Decision::Ajouter, String::new()),
    };
    match decision {
        Decision::Ajouter => ajouter(),
        Decision::Ignorer => Ok(Issue::Ignore { deja: voisins[0].texte.clone() }),
        Decision::Remplacer(i) => {
            let ancien = format!("- {}", voisins[i].texte);
            match memory::remplacer_fait(paths, &ancien, fait) {
                Ok(nouveau) => Ok(Issue::Remplace { ancien, nouveau, raison }),
                // ligne changée entre-temps (modifiée à la main) : dans le doute, ajouter
                Err(_) => ajouter(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais::{cerveau, faux_chat};
    use crate::index::{self, Sens};

    fn voisin(texte: &str, mots: f32, communs: usize, sens: Option<f32>) -> Voisin {
        Voisin { section: "Utilisateur".into(), texte: texte.into(), mots, communs, sens }
    }

    fn dossier() -> (Paths, std::path::PathBuf) {
        let racine = std::env::temp_dir().join(format!("orchestra-m3c-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        let p = Paths::at(racine.clone()).unwrap();
        std::fs::write(
            p.main_memory(),
            "# Mémoire\n\n## Utilisateur\n- (2026-10-26) PB habite à Annecy, au bord du lac.\n- (2026-10-27) PB est allergique aux arachides.\n\n## Connaissances\n- (2026-10-27) Le serveur voix d'Orchestra écoute sur le port 8765.\n",
        )
        .unwrap();
        index::configurer(&p, Sens::default());
        (p, racine)
    }

    #[test]
    fn tres_proche_par_les_mots_ou_par_le_sens() {
        assert!(tres_proche(&voisin("x", 0.6, 3, None)));
        assert!(!tres_proche(&voisin("x", 1.0, 1, None)), "un seul mot commun ne suffit pas");
        assert!(!tres_proche(&voisin("x", 0.4, 2, None)));
        assert!(tres_proche(&voisin("x", 0.0, 0, Some(0.8))));
        assert!(!tres_proche(&voisin("x", 0.2, 1, Some(0.6))));
    }

    #[test]
    fn decision_lue_et_doute_vaut_ajouter() {
        assert_eq!(lire_decision(&json!({ "decision": "REMPLACER", "ligne": 2, "raison": "nouvelle adresse" }), 3), Decision::Remplacer(1));
        assert_eq!(lire_decision(&json!({ "decision": "IGNORER", "raison": "déjà là" }), 3), Decision::Ignorer);
        assert_eq!(lire_decision(&json!({ "decision": "AJOUTER" }), 3), Decision::Ajouter);
        // en cas de doute : AJOUTER
        assert_eq!(lire_decision(&json!({ "decision": "REMPLACER", "ligne": 4 }), 3), Decision::Ajouter, "numéro hors liste");
        assert_eq!(lire_decision(&json!({ "decision": "REMPLACER" }), 3), Decision::Ajouter, "sans numéro");
        assert_eq!(lire_decision(&json!({ "decision": "peut-être" }), 3), Decision::Ajouter);
        assert_eq!(lire_decision(&Value::Null, 3), Decision::Ajouter);
        // le schéma impose la décision parmi les trois, et toujours un numéro de ligne (0 : aucune),
        // sinon le modèle oublie le numéro et REMPLACER tombe en AJOUTER (vu à l'essai réel)
        assert_eq!(schema()["properties"]["decision"]["enum"], json!(["AJOUTER", "REMPLACER", "IGNORER"]));
        assert!(schema()["required"].as_array().unwrap().contains(&json!("ligne")), "{}", schema());
        assert_eq!(lire_decision(&json!({ "decision": "REMPLACER", "ligne": 0 }), 3), Decision::Ajouter);
    }

    #[test]
    fn consigne_numerote_les_voisins() {
        let m = consigne("Utilisateur", "PB habite maintenant à Lyon.", &[voisin("(2026-10-26) PB habite à Annecy.", 0.3, 1, Some(0.82)), voisin("(2026-10-27) PB court le samedi.", 0.0, 0, Some(0.5))]);
        let tout: String = m.iter().map(|x| x.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(tout.contains("[Tâche : CONSOLIDER]") && tout.contains("PB habite maintenant à Lyon."), "{tout}");
        assert!(tout.contains("1. [Utilisateur] (2026-10-26) PB habite à Annecy.") && tout.contains("2. [Utilisateur] (2026-10-27) PB court le samedi."), "{tout}");
        assert!(tout.contains("En cas de doute, AJOUTER"), "{tout}");
    }

    #[tokio::test]
    async fn fait_sans_voisin_proche_ajoute_sans_appel() {
        let (p, racine) = dossier();
        let (url, vues) = faux_chat(vec![r#"{"decision":"IGNORER","raison":"x"}"#.into()]).await;
        let b = cerveau(&url);
        let i = ecrire(Some(&b), &p, "Préférences", "PB aime le cinéma coréen.", Duration::ZERO).await.unwrap();
        assert!(matches!(i, Issue::Ajoute { .. }), "{i:?}");
        assert!(memory::read_main(&p).contains("PB aime le cinéma coréen."));
        assert!(vues.lock().unwrap().is_empty(), "aucun appel au cerveau");
        let _ = std::fs::remove_dir_all(racine);
    }

    #[tokio::test]
    async fn voisin_tres_proche_remplace_puis_ignore() {
        let (p, racine) = dossier();
        let (url, vues) = faux_chat(vec![
            r#"{"decision":"REMPLACER","ligne":1,"raison":"nouvelle adresse"}"#.into(),
            r#"{"decision":"IGNORER","raison":"déjà en mémoire"}"#.into(),
            "pas du JSON".into(),
        ])
        .await;
        let b = cerveau(&url);
        // remplacer
        let i = ecrire(Some(&b), &p, "Utilisateur", "PB habite à Lyon, au bord du Rhône.", Duration::ZERO).await.unwrap();
        let Issue::Remplace { ancien, nouveau, raison } = &i else { panic!("{i:?}") };
        assert_eq!(ancien, "- (2026-10-26) PB habite à Annecy, au bord du lac.");
        assert!(nouveau.ends_with("PB habite à Lyon, au bord du Rhône.") && raison == "nouvelle adresse", "{i:?}");
        let m = memory::read_main(&p);
        assert!(!m.contains("Annecy") && m.contains("Rhône"), "{m}");
        assert!(std::fs::read_to_string(memory::historique(&p)).unwrap().contains("Annecy"));
        let requete = vues.lock().unwrap()[0].clone();
        assert!(requete["format"]["properties"]["decision"].is_object() && requete["think"] == json!(false), "{requete}");
        // ignorer : rien d'écrit
        let avant = memory::read_main(&p);
        let i = ecrire(Some(&b), &p, "Utilisateur", "PB habite à Lyon, près du Rhône.", Duration::ZERO).await.unwrap();
        assert!(matches!(&i, Issue::Ignore { deja } if deja.contains("Rhône")), "{i:?}");
        assert_eq!(memory::read_main(&p), avant);
        // réponse illisible : AJOUTER
        let i = ecrire(Some(&b), &p, "Utilisateur", "PB habite à Lyon depuis novembre, près du Rhône.", Duration::ZERO).await.unwrap();
        assert!(matches!(i, Issue::Ajoute { .. }), "{i:?}");
        assert!(memory::read_main(&p).contains("depuis novembre"));
        let _ = std::fs::remove_dir_all(racine);
    }
}
