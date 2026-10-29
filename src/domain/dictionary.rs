//! Diccionario personal: lo mismo que hace dictionary.ts en Silb web.
//!
//! Cambia una palabra por otra, solo cuando aparece entera y sin mirar
//! mayúsculas, y respeta cómo esté escrita la palabra de destino.
//!
//! Aquí también están la importación y la exportación del diccionario a JSON
//! (copias de seguridad a mano del usuario; por defecto el fichero vive en
//! `%APPDATA%/whispr-open/dictionary.json` en Windows o
//! `~/.config/whispr-open/dictionary.json` en Linux (XDG_CONFIG_HOME), según
//! lo que devuelve `config::dictionary_path`).

use std::path::Path;

use fancy_regex::Regex;
use serde::{Deserialize, Serialize};

/// Palabra del diccionario: `from` es como la oye whisper y
/// `to` como hay que escribirla (por ejemplo «codi» → «usuario»).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub from: String,
    pub to: String,
}

/// Lo que sale de pasar el diccionario por un texto: el texto final y las
/// palabras que de verdad se cambiaron (para poder avisar en la interfaz).
#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryResult {
    pub text: String,
    pub applied: Vec<String>,
}

/// Guarda el diccionario en un JSON que se puede leer (lista de `{from, to}`).
/// Crea la carpeta si falta. Si algo falla, el error viene explicado en español.
pub fn export_dictionary_to(path: &Path, entries: &[DictionaryEntry]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear {}: {e}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("No se pudo serializar el diccionario: {e}"))?;
    std::fs::write(path, raw).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))
}

/// Carga un diccionario desde un JSON (lista de `{from, to}`).
/// Sustituye la lista entera (así una copia de seguridad se restaura tal cual,
/// sin sorpresas). Falla con un mensaje claro si el fichero no es una lista de
/// entradas válidas, y no acepta ficheros con la lista vacía (un fichero vacío
/// no debe borrar el diccionario sin querer).
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

/// Regla para encontrar la palabra entera, sin mirar mayúsculas y con los
/// límites que marca Unicode: no puede estar pegada a letras, números ni
/// acentos combinados (sí entre espacios, puntuación o extremos del texto).
///
/// `None` si lo que escribió el usuario no forma una regla válida: esa palabra
/// se descarta y se apunta en el log, en vez de tumbar el dictado con un
/// pánico (cero `expect` con texto del usuario; ya va escapado, pero esto
/// cubre cualquier caso raro que se escape).
fn to_word_pattern(value: &str) -> Option<Regex> {
    let pattern = format!(
        r"(?iu)(?<![\p{{L}}\p{{M}}\p{{N}}]){}(?![\p{{L}}\p{{M}}\p{{N}}])",
        escape_regexp(value)
    );
    match Regex::new(&pattern) {
        Ok(regex) => Some(regex),
        Err(e) => {
            crate::log!(
                "dictionary",
                "término «{value}» con patrón inválido ({e}); se descarta"
            );
            None
        }
    }
}

/// Pasa las palabras del diccionario por el texto, en orden.
/// Cada una se cambia solo cuando aparece entera y sin mirar mayúsculas,
/// y se respeta cómo esté escrita la de destino.
/// Devuelve el texto resultante y la lista de palabras cambiadas (sin repetir).
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

        let Some(pattern) = to_word_pattern(from) else {
            continue;
        };
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

    // ── Importar y exportar ────────────────────────────────────────────────

    /// Carpeta temporal distinta para cada test (uuid ya está en dev-deps).
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
        // El fichero es un JSON legible (al leerlo otra vez queda igual).
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
