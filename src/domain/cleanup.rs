//! Limpieza de muletillas: lo mismo que hace cleanup.ts en Silb web.
//!
//! Busca y quita muletillas del habla transcrita («eh», «um», «o sea»…),
//! junta las repeticiones y arregla la puntuación, dejando el resto igual.
//!
//! Las muletillas se cambian desde Ajustes (`FillerWords`): tres listas
//! según la regla (seguras / solo antes de una pausa / solo al empezar el
//! turno). Se guardan en config.json con serde y, si falta la configuración,
//! se usan las listas de siempre.

use fancy_regex::Regex;
use serde::{Deserialize, Serialize};

/// Lo que sale de limpiar un turno: el texto final y las muletillas quitadas
/// (la interfaz las enseña: qué se quitó y de dónde).
#[derive(Debug, Clone, PartialEq)]
pub struct CleanupResult {
    pub text: String,
    pub removed: Vec<String>,
}

/// Muletillas que se pueden cambiar, agrupadas por tipo de regla.
///
/// Las listas por defecto (`Default`) son las de siempre:
/// - `unambiguous`: «eh», «um», «uh», «o sea», «you know», en cualquier sitio.
/// - `ambiguous`: «este», «esto», «tipo», «like», «pues», solo antes de una pausa.
/// - `start_of_turn`: «bueno», «pues», solo al empezar el turno.
///
/// Con serde, cada campo se queda en su valor por defecto si falta en el JSON
/// (un config viejo sin `fillers` limpia igual que siempre).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillerWords {
    #[serde(default = "default_unambiguous")]
    pub unambiguous: Vec<String>,
    #[serde(default = "default_ambiguous")]
    pub ambiguous: Vec<String>,
    #[serde(default = "default_start_of_turn")]
    pub start_of_turn: Vec<String>,
}

fn default_unambiguous() -> Vec<String> {
    ["eh", "um", "uh", "o sea", "you know"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_ambiguous() -> Vec<String> {
    ["este", "esto", "tipo", "like", "pues"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_start_of_turn() -> Vec<String> {
    ["bueno", "pues"].iter().map(|s| s.to_string()).collect()
}

impl Default for FillerWords {
    fn default() -> Self {
        Self {
            unambiguous: default_unambiguous(),
            ambiguous: default_ambiguous(),
            start_of_turn: default_start_of_turn(),
        }
    }
}

impl FillerWords {
    /// Cambia las tres listas enteras (lo usa el botón «Por defecto» de
    /// Ajustes: sin nada en el config se usan las de siempre).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Escapa los caracteres especiales de una palabra del usuario antes de
/// meterla en una regla (una muletilla con «.» o «+» no debe convertirse
/// en una regla que se coma cualquier cosa).
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

struct CleanupRule {
    pattern: Regex,
}

/// Cada regla atrapa dos grupos:
///   grupo 1 -> texto que SE QUEDA (lo de antes de la muletilla,
///              o la primera vez en una repetición)
///   grupo 2 -> texto que SE QUITA y se apunta en `removed`
fn rules(fillers: &FillerWords) -> Vec<CleanupRule> {
    let entries: Vec<(String, String)> = {
        let mut out: Vec<(String, String)> = vec![];
        // Muletillas que no admiten duda (ES/EN): en cualquier sitio y palabra entera.
        for w in &fillers.unambiguous {
            let e = escape_regexp(w);
            out.push((
                w.clone(),
                format!(r#"(^|[\s(«"'¡¿…])({e})([,;:…]*)(?=$|[\s,;:.!?…])"#),
            ));
        }
        // Muletillas de doble filo: solo si detrás hay pausa (para no romper usos normales).
        for w in &fillers.ambiguous {
            let e = escape_regexp(w);
            out.push((w.clone(), format!(r#"(^|[\s(«"'¡¿…])({e})([,;:…]+)"#)));
        }
        // Muletillas que solo valen al empezar el turno.
        for w in &fillers.start_of_turn {
            let e = escape_regexp(w);
            out.push((w.clone(), format!(r"(^)({e})([,;:.!?…]*)")));
        }
        // Repeticiones de palabra y de dos palabras -> una sola
        // (reglas fijas, no se pueden cambiar: no son muletillas).
        out.push((
            "repetición de palabra".to_string(),
            r"\b(\p{L}+)(\s+\1)\b".to_string(),
        ));
        out.push((
            "repetición de bigrama".to_string(),
            r"\b((?:\p{L}+\s+)\p{L}+)(\s+\1)\b".to_string(),
        ));
        out
    };

    entries
        .iter()
        // Cero `expect` con texto del usuario: la regla lleva dentro la
        // muletilla que escribió (ya escapada por `escape_regexp`, pero por
        // si algún caso raro se escapa), así que una regla que no compila se
        // descarta y se apunta en el log, en vez de tumbar el turno con un
        // pánico. Las reglas fijas están cubiertas por tests.
        .filter_map(|(name, pattern)| {
            match Regex::new(&format!("(?iu){pattern}")) {
                Ok(pattern) => Some(CleanupRule { pattern }),
                Err(e) => {
                    crate::log!(
                        "cleanup",
                        "regla «{name}» con patrón inválido ({e}); se descarta"
                    );
                    None
                }
            }
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

/// Un turno que solo tiene pausas (o puntuación sin texto) no es texto que
/// limpiar: se devuelve tal cual para no cargarse respuestas como «Bueno.».
fn is_filler_only(value: &str) -> bool {
    value.is_empty()
        || Regex::new(r#"^[\s,;:.!?…()«"'»¡¿\-]+$"#)
            .expect("regex válida")
            .is_match(value)
            .unwrap_or(false)
}

/// Limpieza con las muletillas que se pasan (las de Ajustes; `Config::fillers()`
/// pone las de siempre si el usuario no ha tocado nada).
pub fn clean_transcript_with(input: &str, fillers: &FillerWords) -> CleanupResult {
    let original = normalize_spaces(input);
    if original.is_empty() {
        return CleanupResult {
            text: String::new(),
            removed: vec![],
        };
    }

    let mut text = original.clone();
    let mut removed: Vec<String> = vec![];
    let mut removed_at_start = false;

    for rule in rules(fillers) {
        let mut iterations = 0;
        while iterations < 10 {
            iterations += 1;
            let before = text.clone();

            // Rehace el texto con la regla: se queda el grupo 1
            // y se tira el grupo 2 (la muletilla o la repetición).
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

    // Al quitar una muletilla que abría el turno, la primera palabra pasa a
    // empezar la frase: se pone en mayúscula sin tocar el resto.
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
        return CleanupResult {
            text: original,
            removed: vec![],
        };
    }

    // Quita repetidos sin cambiar el orden.
    let mut seen = std::collections::HashSet::new();
    removed.retain(|r| seen.insert(r.clone()));

    CleanupResult { text, removed }
}

/// Limpieza con las muletillas de siempre (la función original, sin configuración).
pub fn clean_transcript(input: &str) -> CleanupResult {
    clean_transcript_with(input, &FillerWords::default())
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
        assert_eq!(
            cleaned("Vamos a empezar, eh, con la reunión"),
            ("Vamos a empezar, con la reunión".into(), vec!["eh".into()])
        );
    }

    /// Texto con acentos, emoji, mayúsculas y saltos de línea no rompe la
    /// limpieza. Los saltos se convierten en un espacio (el texto dictado es
    /// habla seguida, no un documento) y los emoji (que no son `\p{L}`) pasan
    /// intactos sin activar las reglas de repetición.
    #[test]
    fn acentos_emoji_y_saltos_de_linea_sobreviven_a_la_limpieza() {
        let (text, removed) =
            cleaned("Hola José, qué tal\n\nLa reunión es a las 4 en punto ✓");
        assert!(text.contains("José"), "acentos intactos: {text}");
        assert!(text.contains("qué tal"));
        assert!(text.contains("✓"), "el emoji sobrevive: {text}");
        assert!(
            !text.contains('\n'),
            "los saltos se normalizan a espacio (flujo hablado): {text:?}"
        );
        assert!(removed.is_empty(), "nada que quitar en texto limpio");
        // Repetición con un emoji en medio NO se junta (un emoji no es palabra):
        // el texto llega raro, pero la regla no puede mentir sobre él.
        let (raro, _) = cleaned("hola 😊 hola");
        assert_eq!(raro, "hola 😊 hola");
    }

    #[test]
    fn removes_eh_at_end_without_comma() {
        assert_eq!(
            cleaned("Avísame luego eh"),
            ("Avísame luego".into(), vec!["eh".into()])
        );
    }

    #[test]
    fn removes_eh_keeping_final_period() {
        assert_eq!(cleaned("Vamos, eh."), ("Vamos.".into(), vec!["eh".into()]));
    }

    #[test]
    fn removes_um_mid_sentence() {
        assert_eq!(
            cleaned("No sé, um, qué hacer"),
            ("No sé, qué hacer".into(), vec!["um".into()])
        );
    }

    #[test]
    fn removes_este_before_pause() {
        assert_eq!(
            cleaned("Este, el informe del jueves"),
            ("El informe del jueves".into(), vec!["Este".into()])
        );
    }

    #[test]
    fn keeps_este_as_legit_demonstrative() {
        assert_eq!(
            cleaned("Este es el plan"),
            ("Este es el plan".into(), vec![])
        );
    }

    #[test]
    fn removes_esto_before_pause() {
        assert_eq!(
            cleaned("Dijo, esto, que vendría"),
            ("Dijo, que vendría".into(), vec!["esto".into()])
        );
    }

    #[test]
    fn keeps_esto_as_legit_demonstrative() {
        assert_eq!(
            cleaned("Esto es importante"),
            ("Esto es importante".into(), vec![])
        );
    }

    #[test]
    fn removes_o_sea() {
        assert_eq!(
            cleaned("Vamos, o sea, al parque"),
            ("Vamos, al parque".into(), vec!["o sea".into()])
        );
    }

    #[test]
    fn removes_tipo_before_pause() {
        assert_eq!(
            cleaned("Fue, tipo, muy raro"),
            ("Fue, muy raro".into(), vec!["tipo".into()])
        );
    }

    #[test]
    fn keeps_tipo_in_tipo_de_documento() {
        assert_eq!(
            cleaned("Necesito un tipo de documento"),
            ("Necesito un tipo de documento".into(), vec![])
        );
    }

    #[test]
    fn removes_bueno_at_start_and_capitalizes() {
        assert_eq!(
            cleaned("Bueno, vamos a empezar"),
            ("Vamos a empezar".into(), vec!["Bueno".into()])
        );
    }

    #[test]
    fn keeps_bueno_as_adjective() {
        assert_eq!(
            cleaned("El vino es bueno"),
            ("El vino es bueno".into(), vec![])
        );
    }

    #[test]
    fn keeps_bueno_as_one_word_answer() {
        assert_eq!(cleaned("Bueno."), ("Bueno.".into(), vec![]));
    }

    #[test]
    fn removes_pues_at_start_and_before_pause() {
        // «pues» está en dos categorías por defecto: al empezar y antes de pausa.
        assert_eq!(
            cleaned("Pues, vamos"),
            ("Vamos".into(), vec!["Pues".into()])
        );
        assert_eq!(
            cleaned("Vamos, pues, ya"),
            ("Vamos, ya".into(), vec!["pues".into()])
        );
    }

    #[test]
    fn collapses_word_repetition() {
        // La repetición abre el turno -> la primera palabra se pone en mayúscula.
        assert_eq!(
            cleaned("el el proyecto"),
            ("El proyecto".into(), vec!["el".into()])
        );
    }

    #[test]
    fn collapses_bigram_repetition() {
        assert_eq!(
            cleaned("hay que hay que hacerlo"),
            ("Hay que hacerlo".into(), vec!["hay que".into()])
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(cleaned(""), ("".into(), vec![]));
    }

    #[test]
    fn removes_english_fillers() {
        // «you know» es muletilla segura: se quita aunque no haya pausa.
        assert_eq!(
            cleaned("I mean, like, you know"),
            ("I mean,".into(), vec!["you know".into(), "like".into()])
        );
    }

    // ── Muletillas configuradas ─────────────────────────────────────────────

    #[test]
    fn defaults_replican_las_listas_originales() {
        let d = FillerWords::default();
        assert_eq!(d.unambiguous, vec!["eh", "um", "uh", "o sea", "you know"]);
        assert_eq!(d.ambiguous, vec!["este", "esto", "tipo", "like", "pues"]);
        assert_eq!(d.start_of_turn, vec!["bueno", "pues"]);
        assert!(d.is_default());
    }

    #[test]
    fn quitar_muletilla_desactiva_su_regla() {
        let mut f = FillerWords::default();
        f.ambiguous.retain(|w| w != "este");
        let r = clean_transcript_with("Este, el informe del jueves", &f);
        // Sin «este» en la lista ambigua, la regla ya no existe: se queda.
        assert_eq!(r.text, "Este, el informe del jueves");
        assert!(r.removed.is_empty());
    }

    #[test]
    fn anadir_muletilla_activa_su_regla() {
        let mut f = FillerWords::default();
        f.ambiguous.push("vale".to_string());
        let r = clean_transcript_with("Vamos, vale, ya", &f);
        assert_eq!(r.text, "Vamos, ya");
        assert_eq!(r.removed, vec!["vale"]);
    }

    #[test]
    fn palabras_con_metacaracteres_se_escapan() {
        let mut f = FillerWords::default();
        f.unambiguous.push("a+b".to_string());
        // El «+» no debe repetir la «a»: solo encaja «a+b» tal cual.
        let r = clean_transcript_with("dijo, a+b, algo", &f);
        assert_eq!(r.text, "dijo, algo");
        let r2 = clean_transcript_with("dijo, aaab, algo", &f);
        assert_eq!(r2.text, "dijo, aaab, algo");
    }

    #[test]
    fn config_serde_sin_fillers_cae_a_defaults() {
        let parsed: FillerWords = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, FillerWords::default());
        // Una lista a medias solo cambia su campo (los configs viejos siguen valiendo).
        let parcial: FillerWords = serde_json::from_str(r#"{"unambiguous":["vale"]}"#).unwrap();
        assert_eq!(parcial.unambiguous, vec!["vale"]);
        assert_eq!(parcial.ambiguous, FillerWords::default().ambiguous);
    }
}
