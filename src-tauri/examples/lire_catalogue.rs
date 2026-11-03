//! Lit un catalogue de l'Agent Market (fichier local ou adresse) et affiche les entrées lisibles
//! et celles qu'Orchestra ignore, avec leur motif.
//!
//! Depuis `src-tauri/` :
//!   cargo run --release --example lire_catalogue -- chemin\vers\agents.json

use orchestra_lib::agents;

fn main() {
    let Some(chemin) = std::env::args().nth(1) else {
        eprintln!("usage : lire_catalogue <agents.json>");
        std::process::exit(2);
    };
    let texte = std::fs::read_to_string(&chemin).expect("catalogue illisible");
    let l = agents::parse_registry(&texte);
    println!("{} agent(s) lisible(s) :", l.agents.len());
    for m in &l.agents {
        println!("  {} {} ({}) : {}", m.id, m.version, m.certification, m.name);
    }
    println!("{} entrée(s) ignorée(s) :", l.ignores.len());
    for i in &l.ignores {
        println!("  {i}");
    }
}
