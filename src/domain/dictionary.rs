//! Diccionario personal — port directo de src/domain/dictionary.ts (Whispr Open web).
//!
//! Reemplazo de términos por palabra completa y case-insensitive,
//! preservando la capitalización de `to`.
//!
//! Además de la lógica de reemplazo, aquí viven la importación y exportación
//! del diccionario a JSON (backup/restore manual del usuario; la ruta por
//! defecto es `%APPDATA%/whispr-open/dictionary.json` vía `config::dictionary_path`).

use std::path::Path;

use fancy_regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryResult {
    pub text: String,
    pub applied: Vec<String>,
}

/// Exporta el diccionario a un JSON legible (lista de `{from, to}`).
/// Crea el directorio padre si falta. Errores con mensaje en español.
pub fn export_dictionary_to(path: &Path, entries: &[DictionaryEntry]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear {}: {e}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("No se pudo serializar el diccionario: {e}"))?;
    std::fs::write(path, raw).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))
}

/// Importa un diccionario desde JSON (lista de `{from, to}`).
/// Reemplaza por completo la lista actual (semántica backup/restore
/// predecible). Falla con mensaje claro si el fichero no es una lista de
/// entradas válidas, y descarta ficheros con la lista vacía (una importación
/// vacía no debe borrar el diccionario por accidente).
pub fn import_dictionary_from(path: &Path) -> Result<Vec<DictionaryEntry>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let entries: Vec<DictionaryEntry> = serde_json::from_str(&raw)
        .map_err(|e| format!("El fichero no es una lista de {{from, to}}: {e}"))?;
    if entries.iter().any(|e| e.from.trim().is_empty()) {
        return Err("El fichero contiene entradas sin «from»".to_string());
    }
    if entries.is_empty() {
        return Err(
            "El fichero importado está vacío: no se toca el diccionario actual".to_string(),
        );
    }
    Ok(entries)
}

fn escape_regexp(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(
            c,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Patrón de palabra completa, case-insensitive y con límites Unicode:
/// el término no puede estar rodeado de letras, números ni marcas de
/// combinación (sí puede estar rodeado de espacios, puntuación o límites del texto).
fn to_word_pattern(value: &str) -> Regex {
    let pattern = format!(
        r"(?iu)(?<![\p{{L}}\p{{M}}\p{{N}}]){}(?![\p{{L}}\p{{M}}\p{{N}}])",
        escape_regexp(value)
    );
    Regex::new(&pattern).expect("patrón de diccionario inválido")
}

/// Aplica las entradas del diccionario sobre el texto, en orden.
/// Cada término se reemplaza con coincidencia de palabra completa y
/// case-insensitive, preservando la capitalización de `to`.
/// Devuelve el texto resultante y la lista (única) de términos aplicados.
pub fn apply_dictionary(text: &str, entries: &[DictionaryEntry]) -> DictionaryResult {
    if text.is_empty() {
        return DictionaryResult {
            text: String::new(),
            applied: vec![],
        };
    }

    let mut result = text.to_string();
    let mut applied: Vec<String> = vec![];

    for entry in entries {
        let from = entry.from.trim();
        if from.is_empty() {
            continue;
        }

        let pattern = to_word_pattern(from);
        let matches = pattern.is_match(&result).unwrap_or(false);
        if !matches {
            continue;
        }

        result = pattern.replace_all(&result, &entry.to).to_string();
        if !applied.contains(&from.to_string()) {
            applied.push(from.to_string());
        }
    }

    DictionaryResult {
        text: result,
        applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(from: &str, to: &str) -> DictionaryEntry {
        DictionaryEntry {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn replaces_term_and_reports_applied() {
        let entries = vec![dict("whatsapp", "WhatsApp")];
        let r = apply_dictionary("mándalo por whatsapp", &entries);
        assert_eq!(r.text, "mándalo por WhatsApp");
        assert_eq!(r.applied, vec!["whatsapp"]);
    }

    #[test]
    fn case_insensitive_match_text_and_term() {
        let entries = vec![dict("openai", "OpenAI")];
        let r = apply_dictionary("HE VISTO OpenAI y openai", &entries);
        assert_eq!(r.text, "HE VISTO OpenAI y OpenAI");
        assert_eq!(r.applied, vec!["openai"]);
    }

    #[test]
    fn no_subword_replacement() {
        let entries = vec![dict("whatsapp", "WhatsApp")];
        let r = apply_dictionary("quiero whatsappear ahora", &entries);
        assert_eq!(r.text, "quiero whatsappear ahora");
        assert_eq!(r.applied, Vec::<String>::new());
    }

    #[test]
    fn no_replacement_inside_other_word() {
        let entries = vec![dict("año", "anio")];
        let r = apply_dictionary("una añoranza antigua", &entries);
        assert_eq!(r.text, "una añoranza antigua");
        assert_eq!(r.applied, Vec::<String>::new());
    }

    #[test]
    fn respects_adjacent_punctuation() {
        let entries = vec![dict("whatsapp", "WhatsApp")];
        let r = apply_dictionary("dime, whatsapp, por favor", &entries);
        assert_eq!(r.text, "dime, WhatsApp, por favor");
        assert_eq!(r.applied, vec!["whatsapp"]);
    }

    #[test]
    fn replaces_multi_word_terms() {
        let entries = vec![dict("open ai", "OpenAI")];
        let r = apply_dictionary("usa open ai para esto", &entries);
        assert_eq!(r.text, "usa OpenAI para esto");
        assert_eq!(r.applied, vec!["open ai"]);
    }

    #[test]
    fn applies_multiple_entries_in_order() {
        let entries = vec![dict("whatsapp", "WhatsApp"), dict("lidl", "LIDR")];
        let r = apply_dictionary("whatsapp y lidl", &entries);
        assert_eq!(r.text, "WhatsApp y LIDR");
        assert_eq!(r.applied, vec!["whatsapp", "lidl"]);
    }

    // ── Import / export ─────────────────────────────────────────────────────

    /// Directorio temporal único por test (uuid ya está en dev-deps).
    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("whispr-dict-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn export_import_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("dictionary.json");
        let entries = vec![dict("whatsapp", "WhatsApp"), dict("open ai", "OpenAI")];

        export_dictionary_to(&path, &entries).unwrap();
        let back = import_dictionary_from(&path).unwrap();

        assert_eq!(back, entries);
        // El fichero es JSON legible (roundtrip serde idéntico).
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("whatsapp"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_crea_el_directorio_padre() {
        let dir = temp_dir();
        let path = dir.join("anidado").join("dictionary.json");
        export_dictionary_to(&path, &[dict("a", "b")]).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_fichero_inexistente_da_error_claro() {
        let err = import_dictionary_from(std::path::Path::new("no-existe-xyz.json")).unwrap_err();
        assert!(err.contains("No se pudo leer"), "{err}");
    }

    #[test]
    fn import_json_invalido_da_error_claro() {
        let dir = temp_dir();
        let path = dir.join("dictionary.json");
        std::fs::write(&path, "esto no es json").unwrap();
        let err = import_dictionary_from(&path).unwrap_err();
        assert!(err.contains("lista de {from, to}"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_lista_vacia_se_descarta() {
        let dir = temp_dir();
        let path = dir.join("dictionary.json");
        std::fs::write(&path, "[]").unwrap();
        let err = import_dictionary_from(&path).unwrap_err();
        assert!(err.contains("vacío"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_rechaza_entrada_sin_from() {
        let dir = temp_dir();
        let path = dir.join("dictionary.json");
        std::fs::write(&path, r#"[{"from":"","to":"x"}]"#).unwrap();
        let err = import_dictionary_from(&path).unwrap_err();
        assert!(err.contains("sin «from»"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
