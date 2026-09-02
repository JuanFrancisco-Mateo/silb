//! Diccionario personal — port directo de src/domain/dictionary.ts (Whispr Open web).
//!
//! Reemplazo de términos por palabra completa y case-insensitive,
//! preservando la capitalización de `to`.

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

fn escape_regexp(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\') {
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
        return DictionaryResult { text: String::new(), applied: vec![] };
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

    DictionaryResult { text: result, applied }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(from: &str, to: &str) -> DictionaryEntry {
        DictionaryEntry { from: from.into(), to: to.into() }
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
}