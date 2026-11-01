//! Outils des tests : un faux Ollama qui répond aux appels de chat par des réponses fixées d'avance.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

/// Faux `/api/chat` : chaque appel reçoit la réponse suivante de la liste (la dernière se répète).
/// Renvoie l'adresse et les corps des requêtes reçues.
pub async fn faux_chat(reponses: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", ecoute.local_addr().unwrap());
    let vues = Arc::new(Mutex::new(Vec::new()));
    let v2 = vues.clone();
    tokio::spawn(async move {
        let mut i = 0usize;
        while let Ok((mut flux, _)) = ecoute.accept().await {
            let mut buf = vec![0u8; 1 << 20];
            let mut lu = 0;
            let corps = loop {
                let n = flux.read(&mut buf[lu..]).await.unwrap_or(0);
                if n == 0 {
                    break String::new();
                }
                lu += n;
                let t = String::from_utf8_lossy(&buf[..lu]).to_string();
                if let Some((tete, reste)) = t.split_once("\r\n\r\n") {
                    let taille: usize = tete.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|x| x.trim().parse().unwrap_or(0))).unwrap_or(0);
                    if reste.len() >= taille {
                        break reste.to_string();
                    }
                }
            };
            v2.lock().unwrap().push(serde_json::from_str::<Value>(&corps).unwrap_or(Value::Null));
            let texte = reponses.get(i).or(reponses.last()).cloned().unwrap_or_default();
            i += 1;
            let lignes = format!(
                "{}\n{}\n",
                json!({ "model": "faux", "message": { "role": "assistant", "content": texte }, "done": false }),
                json!({ "model": "faux", "message": { "role": "assistant", "content": "" }, "done": true, "prompt_eval_count": 10, "eval_count": 5 })
            );
            let _ = flux.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{lignes}", lignes.len()).as_bytes()).await;
        }
    });
    (url, vues)
}

/// Cerveau Ollama branché sur une adresse de test.
pub fn cerveau(url: &str) -> crate::brain::Brain {
    let cfg = crate::config::Config { provider: "ollama".into(), ollama_url: url.into(), model: "faux".into(), ..crate::config::Config::default() };
    crate::brain::Brain::new(cfg, reqwest::Client::new())
}
