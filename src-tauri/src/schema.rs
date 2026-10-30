//! Validateur JSON Schema minimal pour les schémas du contrat (`contrat/schemas/`, copiés de
//! l'Agent Market, `agent-certification/schemas/`). Mêmes mots-clés que `schema_mini.py` du kit :
//! type, required, properties, additionalProperties, items, enum, const, pattern, minLength,
//! minItems, if/then/else, allOf.

use serde_json::Value;

pub const MISSION: &str = include_str!("../../contrat/schemas/mission.schema.json");
pub const REPONSE: &str = include_str!("../../contrat/schemas/reponse.schema.json");
pub const EVENEMENT: &str = include_str!("../../contrat/schemas/evenement.schema.json");
pub const MESSAGE: &str = include_str!("../../contrat/schemas/message-orchestra.schema.json");
pub const MANIFESTE: &str = include_str!("../../contrat/schemas/manifeste.schema.json");

/// Schéma intégré, lu une fois.
pub fn schema(texte: &'static str) -> Value {
    serde_json::from_str(texte).expect("schéma du contrat illisible")
}

fn type_ok(v: &Value, t: &Value) -> bool {
    match t {
        Value::Array(ts) => ts.iter().any(|x| type_ok(v, x)),
        Value::String(t) => match t.as_str() {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "boolean" => v.is_boolean(),
            "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
            "number" => v.is_number(),
            "null" => v.is_null(),
            _ => true,
        },
        _ => true,
    }
}

fn court(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 60 { format!("{}…", s.chars().take(60).collect::<String>()) } else { s }
}

/// Liste des erreurs ; vide si `v` respecte le schéma `s`.
pub fn erreurs(v: &Value, s: &Value) -> Vec<String> {
    erreurs_a(v, s, "$")
}

pub fn valide(v: &Value, s: &Value) -> bool {
    erreurs(v, s).is_empty()
}

fn erreurs_a(v: &Value, s: &Value, chemin: &str) -> Vec<String> {
    let mut e = vec![];
    if let Some(t) = s.get("type") {
        if !type_ok(v, t) {
            return vec![format!("{chemin} : type attendu {t}")];
        }
    }
    if let Some(c) = s.get("const") {
        if v != c {
            e.push(format!("{chemin} : valeur attendue {c}, trouvée {}", court(v)));
        }
    }
    if let Some(Value::Array(en)) = s.get("enum") {
        if !en.contains(v) {
            e.push(format!("{chemin} : {} n'est pas une valeur permise", court(v)));
        }
    }
    if let Some(t) = v.as_str() {
        if let Some(n) = s.get("minLength").and_then(|n| n.as_u64()) {
            if (t.chars().count() as u64) < n {
                e.push(format!("{chemin} : texte vide ou trop court"));
            }
        }
        if let Some(p) = s.get("pattern").and_then(|p| p.as_str()) {
            match regex::Regex::new(p) {
                Ok(r) if !r.is_match(t) => e.push(format!("{chemin} : « {t} » ne respecte pas le motif {p}")),
                Err(_) => e.push(format!("{chemin} : motif du schéma illisible {p}")),
                _ => {}
            }
        }
    }
    if let Some(a) = v.as_array() {
        if let Some(n) = s.get("minItems").and_then(|n| n.as_u64()) {
            if (a.len() as u64) < n {
                e.push(format!("{chemin} : au moins {n} élément(s) attendu(s)"));
            }
        }
        if let Some(items) = s.get("items") {
            for (i, x) in a.iter().enumerate() {
                e.extend(erreurs_a(x, items, &format!("{chemin}[{i}]")));
            }
        }
    }
    if let Some(o) = v.as_object() {
        if let Some(Value::Array(req)) = s.get("required") {
            for k in req.iter().filter_map(|k| k.as_str()) {
                if !o.contains_key(k) {
                    e.push(format!("{chemin} : champ obligatoire « {k} » absent"));
                }
            }
        }
        let props = s.get("properties").and_then(|p| p.as_object());
        for (k, x) in o {
            if let Some(ps) = props.and_then(|p| p.get(k)) {
                e.extend(erreurs_a(x, ps, &format!("{chemin}.{k}")));
            } else {
                match s.get("additionalProperties") {
                    Some(Value::Bool(false)) => e.push(format!("{chemin} : champ inconnu « {k} »")),
                    Some(ap @ Value::Object(_)) => e.extend(erreurs_a(x, ap, &format!("{chemin}.{k}"))),
                    _ => {}
                }
            }
        }
    }
    if let Some(Value::Array(tous)) = s.get("allOf") {
        for sous in tous {
            e.extend(erreurs_a(v, sous, chemin));
        }
    }
    if let Some(si) = s.get("if") {
        if valide(v, si) {
            if let Some(alors) = s.get("then") {
                e.extend(erreurs_a(v, alors, chemin));
            }
        } else if let Some(sinon) = s.get("else") {
            e.extend(erreurs_a(v, sinon, chemin));
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schemas_integres_lisibles() {
        for t in [MISSION, REPONSE, EVENEMENT, MESSAGE, MANIFESTE] {
            assert!(schema(t).is_object());
        }
    }

    #[test]
    fn compte_rendu_partiel_exige_motif_et_manquant() {
        let s = schema(REPONSE);
        let mut cr = json!({
            "mission_id": "m", "etat": "termine", "resultat": { "texte": "ok", "fichiers": [] },
            "actions_declarees": [], "verification": { "controle": [], "non_controle": [] }, "point_de_reprise": null
        });
        assert!(valide(&cr, &s), "{:?}", erreurs(&cr, &s));
        cr["etat"] = json!("partiel");
        let e = erreurs(&cr, &s);
        assert!(e.iter().any(|x| x.contains("motif")) && e.iter().any(|x| x.contains("manquant")));
        cr["etat"] = json!("fini");
        assert!(!valide(&cr, &s));
    }

    #[test]
    fn evenement_validation_requise_exige_action_et_raison() {
        let s = schema(EVENEMENT);
        let ev = json!({ "mission_id": "m", "agent_id": "a", "type": "validation_requise", "message": "x", "t": "2026" });
        assert_eq!(erreurs(&ev, &s).len(), 2);
    }

    #[test]
    fn motifs_et_types() {
        let s = json!({ "type": "object", "properties": { "v": { "type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$" }, "n": { "type": "integer" } } });
        assert!(valide(&json!({ "v": "1.0.2", "n": 3 }), &s));
        assert!(!valide(&json!({ "v": "1.0", "n": 3 }), &s));
        assert!(!valide(&json!({ "n": 1.5 }), &s));
        assert!(!valide(&json!({ "n": true }), &s));
    }
}
