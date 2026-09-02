//! Limpieza de muletillas — port directo de src/domain/cleanup.ts (Whispr Open web).
//!
//! Detecta y elimina muletillas del habla transcrita ("eh", "um", "o sea", ...),
//! colapsa repeticiones y normaliza puntuación, manteniendo el resto del texto.

use fancy_regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct CleanupResult {
    pub text: String,
    pub removed: Vec<String>,
}

struct CleanupRule {
    pattern: Regex,
}

/// Cada patrón captura dos grupos:
///   grupo 1 -> texto que se CONSERVA (la frontera anterior a la muletilla,
///              o la primera ocurrencia en una repetición)
///   grupo 2 -> texto que se ELIMINA y se reporta en `removed`
fn rules() -> Vec<CleanupRule> {
    let entries: &[(&str, &str)] = &[
        // Muletillas inequívocas (ES/EN): cualquier posición, palabra completa
        ("eh", r#"(^|[\s(«"'¡¿…])(eh)([,;:…]*)(?=$|[\s,;:.!?…])"#),
        ("um", r#"(^|[\s(«"'¡¿…])(um)([,;:…]*)(?=$|[\s,;:.!?…])"#),
        ("uh", r#"(^|[\s(«"'¡¿…])(uh)([,;:…]*)(?=$|[\s,;:.!?…])"#),
        ("o sea", r#"(^|[\s(«"'¡¿…])(o sea)([,;:…]*)(?=$|[\s,;:.!?…])"#),
        ("you know", r#"(^|[\s(«"'¡¿…])(you know)([,;:…]*)(?=$|[\s,;:.!?…])"#),
        // Muletillas ambiguas: solo seguidas de pausa (no romper usos legítimos)
        ("este", r#"(^|[\s(«"'¡¿…])(este)([,;:…]+)"#),
        ("esto", r#"(^|[\s(«"'¡¿…])(esto)([,;:…]+)"#),
        ("tipo", r#"(^|[\s(«"'¡¿…])(tipo)([,;:…]+)"#),
        ("like", r#"(^|[\s(«"'¡¿…])(like)([,;:…]+)"#),
        // Muletillas solo al inicio del turno
        ("bueno", r"(^)(bueno)([,;:.!?…]*)"),
        ("pues (inicio)", r"(^)(pues)([,;:.!?…]*)"),
        ("pues (pausa)", r"([\s])(pues)([,;:…]+)"),
        // Repeticiones de palabra y de bigrama -> una sola ocurrencia
        ("repetición de palabra", r"\b(\p{L}+)(\s+\1)\b"),
        ("repetición de bigrama", r"\b((?:\p{L}+\s+)\p{L}+)(\s+\1)\b"),
    ];
    entries
        .iter()
        .map(|(_, pattern)| CleanupRule {
            pattern: Regex::new(&format!("(?iu){pattern}")).expect("patrón de cleanup inválido"),
        })
        .collect()
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_punctuation(value: &str) -> String {
    let space_before_punct = Regex::new(r"\s+([,;:.!?…])").expect("regex válida");
    let comma_before_terminal = Regex::new(r"[,;:]([.!?…])").expect("regex válida");
    let space_after_open = Regex::new(r#"([«"'¿¡(…])\s+"#).expect("regex válida");

    let step1 = space_before_punct.replace_all(value, "$1");
    let step2 = comma_before_terminal.replace_all(&step1, "$1");
    space_after_open.replace_all(&step2, "$1").to_string()
}

/// Un turno que solo contiene pausas (o puntuación sin texto) no es texto que
/// limpiar: se devuelve intacto para no destruir respuestas como "Bueno.".
fn is_filler_only(value: &str) -> bool {
    value.is_empty()
        || Regex::new(r#"^[\s,;:.!?…()«"»'¡¿\-]+$"#)
            .expect("regex válida")
            .is_match(value)
            .unwrap_or(false)
}

pub fn clean_transcript(input: &str) -> CleanupResult {
    let original = normalize_spaces(input);
    if original.is_empty() {
        return CleanupResult { text: String::new(), removed: vec![] };
    }

    let mut text = original.clone();
    let mut removed: Vec<String> = vec![];
    let mut removed_at_start = false;

    for rule in rules() {
        let mut iterations = 0;
        while iterations < 10 {
            iterations += 1;
            let before = text.clone();

            // Reconstruye el texto aplicando la regla: se conserva el grupo 1
            // y se descarta el grupo 2 (la muletilla o la repetición).
            let mut rebuilt = String::with_capacity(text.len());
            let mut last_end = 0;
            for caps in rule.pattern.captures_iter(&text) {
                let caps = match caps {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let Some(m) = caps.get(0) else { continue };
                rebuilt.push_str(&text[last_end..m.start()]);
                let keep = caps.get(1).map(|g| g.as_str()).unwrap_or(m.as_str());
                let piece = caps.get(2).map(|g| g.as_str()).unwrap_or("");
                if m.start() == 0 {
                    removed_at_start = true;
                }
                if !piece.is_empty() {
                    removed.push(piece.trim().to_string());
                }
                rebuilt.push_str(keep);
                last_end = m.end();
            }
            rebuilt.push_str(&text[last_end..]);
            text = rebuilt;

            if text == before {
                break;
            }
            text = normalize_spaces(&text);
        }
    }

    text = normalize_punctuation(&text);
    text = normalize_spaces(&text);

    // Al eliminar una muletilla que abría el turno, la primera palabra pasa a
    // encabezar la frase: se capitaliza sin tocar el resto.
    if removed_at_start {
        let capitalize_first = Regex::new(r"^([^\p{L}]*)(\p{Ll})").expect("regex válida");
        if let Ok(Some(c)) = capitalize_first.captures(&text) {
            let prefix = c.get(1).map(|g| g.as_str()).unwrap_or("");
            let first = c.get(2).map(|g| g.as_str()).unwrap_or("");
            let upper = first.to_uppercase();
            text = format!("{prefix}{upper}{}", &text[prefix.len() + first.len()..]);
        }
    }

    if is_filler_only(&text) {
        return CleanupResult { text: original, removed: vec![] };
    }

    // Dedupe conservando el orden.
    let mut seen = std::collections::HashSet::new();
    removed.retain(|r| seen.insert(r.clone()));

    CleanupResult { text, removed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleaned(input: &str) -> (String, Vec<String>) {
        let r = clean_transcript(input);
        (r.text, r.removed)
    }

    #[test]
    fn removes_eh_mid_sentence() {
        assert_eq!(cleaned("Vamos a empezar, eh, con la reunión"), ("Vamos a empezar, con la reunión".into(), vec!["eh".into()]));
    }

    #[test]
    fn removes_eh_at_end_without_comma() {
        assert_eq!(cleaned("Avísame luego eh"), ("Avísame luego".into(), vec!["eh".into()]));
    }

    #[test]
    fn removes_eh_keeping_final_period() {
        assert_eq!(cleaned("Vamos, eh."), ("Vamos.".into(), vec!["eh".into()]));
    }

    #[test]
    fn removes_um_mid_sentence() {
        assert_eq!(cleaned("No sé, um, qué hacer"), ("No sé, qué hacer".into(), vec!["um".into()]));
    }

    #[test]
    fn removes_este_before_pause() {
        assert_eq!(cleaned("Este, el informe del jueves"), ("El informe del jueves".into(), vec!["Este".into()]));
    }

    #[test]
    fn keeps_este_as_legit_demonstrative() {
        assert_eq!(cleaned("Este es el plan"), ("Este es el plan".into(), vec![]));
    }

    #[test]
    fn removes_esto_before_pause() {
        assert_eq!(cleaned("Dijo, esto, que vendría"), ("Dijo, que vendría".into(), vec!["esto".into()]));
    }

    #[test]
    fn keeps_esto_as_legit_demonstrative() {
        assert_eq!(cleaned("Esto es importante"), ("Esto es importante".into(), vec![]));
    }

    #[test]
    fn removes_o_sea() {
        assert_eq!(cleaned("Vamos, o sea, al parque"), ("Vamos, al parque".into(), vec!["o sea".into()]));
    }

    #[test]
    fn removes_tipo_before_pause() {
        assert_eq!(cleaned("Fue, tipo, muy raro"), ("Fue, muy raro".into(), vec!["tipo".into()]));
    }

    #[test]
    fn keeps_tipo_in_tipo_de_documento() {
        assert_eq!(cleaned("Necesito un tipo de documento"), ("Necesito un tipo de documento".into(), vec![]));
    }

    #[test]
    fn removes_bueno_at_start_and_capitalizes() {
        assert_eq!(cleaned("Bueno, vamos a empezar"), ("Vamos a empezar".into(), vec!["Bueno".into()]));
    }

    #[test]
    fn keeps_bueno_as_adjective() {
        assert_eq!(cleaned("El vino es bueno"), ("El vino es bueno".into(), vec![]));
    }

    #[test]
    fn keeps_bueno_as_one_word_answer() {
        assert_eq!(cleaned("Bueno."), ("Bueno.".into(), vec![]));
    }

    #[test]
    fn collapses_word_repetition() {
        // La repetición abre el turno -> la primera palabra se capitaliza.
        assert_eq!(cleaned("el el proyecto"), ("El proyecto".into(), vec!["el".into()]));
    }

    #[test]
    fn collapses_bigram_repetition() {
        assert_eq!(cleaned("hay que hay que hacerlo"), ("Hay que hacerlo".into(), vec!["hay que".into()]));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(cleaned(""), ("".into(), vec![]));
    }

    #[test]
    fn removes_english_fillers() {
        // "you know" es muletilla inequívoca: se elimina aunque no haya pausa.
        assert_eq!(cleaned("I mean, like, you know"), ("I mean,".into(), vec!["you know".into(), "like".into()]));
    }
}