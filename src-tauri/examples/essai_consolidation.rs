//! Essai de la consolidation (lot M3) avec le vrai modèle : pour chaque nouveau fait, la décision
//! prise (ajouter, remplacer, ignorer) est-elle celle attendue ? Travaille sur une copie de
//! `eval/donnees/orchestra.md` dans un dossier temporaire, jamais dans le vrai.
//!
//! Lancement, depuis `src-tauri/` : `cargo run --release --example essai_consolidation`
//! (`--modele qwen3.6:27b`, `--passages 1`). Rapport : `Orchestra/eval/consolidation-<date>.md`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use orchestra_lib::{
    brain::Brain,
    config::Config,
    consolidation::{self, Issue},
    index,
    paths::Paths,
};

struct Cas {
    section: &'static str,
    fait: &'static str,
    /// AJOUTER, REMPLACER ou IGNORER ; pour REMPLACER, un morceau de la ligne remplacée.
    attendu: &'static str,
    ligne: &'static str,
}

const CAS: &[Cas] = &[
    Cas { section: "Connaissances", fait: "La réunion d'équipe de PB a lieu le mardi à 9 h 30.", attendu: "IGNORER", ligne: "" },
    Cas { section: "Connaissances", fait: "Le disque externe de sauvegarde fait maintenant 8 To.", attendu: "REMPLACER", ligne: "fait 4 To" },
    Cas { section: "Utilisateur", fait: "PB a acheté un vélo électrique.", attendu: "AJOUTER", ligne: "" },
    Cas { section: "Connaissances", fait: "Le serveur voix d'Orchestra écoute désormais sur le port 8800.", attendu: "REMPLACER", ligne: "port 8765" },
    Cas { section: "Connaissances", fait: "La sauvegarde du dossier Claude_Projects se fait aussi chaque mercredi soir.", attendu: "AJOUTER", ligne: "" },
    Cas { section: "Connaissances", fait: "Le modèle de chat principal de PB est maintenant qwen3.5:9b.", attendu: "REMPLACER", ligne: "qwen3.6:27b" },
    Cas { section: "Utilisateur", fait: "PB court le samedi matin, une dizaine de kilomètres autour du lac.", attendu: "IGNORER", ligne: "" },
    Cas { section: "Utilisateur", fait: "PB habite maintenant à Lyon.", attendu: "REMPLACER", ligne: "Annecy" },
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let val = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let modele = val("--modele").unwrap_or_else(|| "qwen3.6:27b".into());
    let passages: usize = val("--passages").and_then(|v| v.parse().ok()).unwrap_or(1);
    let depot = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cfg = Config { provider: "ollama".into(), model: modele.clone(), eval_temperature: Some(0.6), ..Config::default() };
    let http = reqwest::Client::new();
    let brain = Brain::new(cfg.clone(), http.clone());
    // chargement hors mesure, par un vrai appel de chat (même num_ctx que les décisions)
    println!("Chargement de {modele} …");
    brain.complete("prechauffage", &[orchestra_lib::brain::Msg::user("Réponds juste : prêt.")]).await?;
    let tags: serde_json::Value = http.get(format!("{}/api/tags", cfg.ollama_url.trim_end_matches('/'))).send().await?.json().await?;
    let sens_installe = tags["models"].as_array().into_iter().flatten().any(|m| {
        let n = m["name"].as_str().unwrap_or("");
        n == cfg.embedding_model || n == format!("{}:latest", cfg.embedding_model)
    });

    let mut lignes = vec![];
    let mut reussis = 0;
    let mut total = 0;
    for passage in 0..passages {
        for (i, c) in CAS.iter().enumerate() {
            // chaque cas part de la mémoire d'essai intacte
            let racine = std::env::temp_dir().join(format!("orchestra-consolidation-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]));
            let paths = Paths::at(racine.clone())?;
            std::fs::copy(depot.join("eval").join("donnees").join("orchestra.md"), paths.main_memory())?;
            index::configurer_depuis(&paths, &cfg);
            index::vectoriser(&paths).await;
            let voisins = index::voisins(&paths, c.fait, 5, Duration::from_secs(3)).await;
            let proche = voisins.iter().any(consolidation::tres_proche);
            // réglage du seuil du sens : les voisins et leur proximité
            for v in voisins.iter().take(3) {
                println!("      voisin : mots {:.2} ({}), sens {} · {}", v.mots, v.communs, v.sens.map(|s| format!("{s:.3}")).unwrap_or("-".into()), v.texte.chars().take(70).collect::<String>());
            }
            let t = Instant::now();
            let issue = consolidation::ecrire(Some(&brain), &paths, c.section, c.fait, Duration::from_secs(3)).await?;
            let duree = t.elapsed();
            let (obtenu, detail) = match &issue {
                Issue::Ajoute { .. } => ("AJOUTER", String::new()),
                Issue::Remplace { ancien, raison, .. } => ("REMPLACER", format!("{ancien} — {raison}")),
                Issue::Ignore { deja } => ("IGNORER", deja.clone()),
            };
            let ok = obtenu == c.attendu && (c.ligne.is_empty() || detail.contains(c.ligne));
            total += 1;
            if ok {
                reussis += 1;
            }
            println!("[{}/{}] passage {} · {} → {obtenu} ({:.1} s) {}", i + 1, CAS.len(), passage + 1, c.fait, duree.as_secs_f64(), if ok { "✅" } else { "❌" });
            lignes.push(format!(
                "| {} | {} | {} | {} | {} | {:.1} s | {} |",
                i + 1,
                c.fait,
                c.attendu,
                obtenu,
                if proche { "oui" } else { "non" },
                duree.as_secs_f64(),
                if ok { "✅" } else { "❌" }
            ));
            if !detail.is_empty() {
                lignes.push(format!("|  | ↳ {} |  |  |  |  |  |", detail.replace('|', "/")));
            }
            let _ = std::fs::remove_dir_all(racine);
        }
    }
    let date = chrono::Local::now();
    let mut s = format!(
        "# Essai de la consolidation (lot M3) — {}\n\nModèle : `{modele}` · température 0,6 · {passages} passage(s) · modèle d'embedding : `{}` ({})\n\n## Résumé : {reussis}/{total} décisions attendues\n\n« Très proche » : le cerveau n'est consulté que si un voisin l'est ; sinon le fait est ajouté directement.\n\n| # | Nouveau fait | Attendu | Obtenu | Très proche | Durée | |\n|---|---|---|---|---|---|---|\n",
        date.format("%d/%m/%Y %H:%M"),
        cfg.embedding_model,
        if sens_installe { "installé : voisins par les mots et par le sens" } else { "absent : voisins par les mots seuls" }
    );
    s.push_str(&lignes.join("\n"));
    s.push('\n');
    let reel = Paths::init()?;
    let dossier = reel.root.join("eval");
    std::fs::create_dir_all(&dossier)?;
    let rapport = dossier.join(format!("consolidation-{}.md", date.format("%Y-%m-%d_%H%M")));
    std::fs::write(&rapport, &s)?;
    println!("\n{reussis}/{total} décisions attendues · rapport : {}", rapport.display());
    Ok(())
}
