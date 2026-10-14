//! Rectificaciones habladas — port directo de src/domain/correction.ts (Silb web).
//!
//! Detección de frases del tipo "no, quería decir X" / "mejor dicho X" para
//! reemplazar el turno dictado por la corrección expresada.

use chrono::Local;
use fancy_regex::Regex;
use uuid::Uuid;

/// Rectificación detectada: qué se dijo (`original`), qué debía decirse
/// (`replacement`), la regla que casó (`reason`) y cuándo.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub id: String,
    pub original: String,
    pub replacement: String,
    pub reason: String,
    pub created_at: String,
}

/// Texto resultante de aplicar las reglas de rectificación a un turno y, si
/// casó alguna, la corrección detectada.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionResult {
    pub text: String,
    pub correction: Option<Correction>,
}

/// Turno ya procesado por la sesión multi-turno.
///
/// - `text`: lo que debe mostrarse e inyectarse (vacío si el turno es solo el
///   arranque de una rectificación todavía sin completar).
/// - `correction`: la rectificación aplicada, si la hubo.
/// - `pending`: `true` cuando el turno quedó guardado esperando el resto de la
///   frase correctiva; el siguiente turno lo completa.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionTurn {
    pub text: String,
    pub correction: Option<Correction>,
    pub pending: bool,
}

struct CorrectionRule {
    pattern: Regex,
    reason: &'static str,
}

/// Compila un patrón JS (flags `i`) a fancy-regex con case-insensitive.
fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("patrón de corrección inválido")
}

const CORRECTION_PATTERNS: &[(&str, &str)] = &[
    (
        r"^(?:uy[,;:.!? ]*)?(?:así|asi) no (?:era|es)[,;:.!? ]*(?:era|quise decir)[,;:.!? ]+(.+)$",
        "Rectificación detectada en tu frase",
    ),
    (
        r"^(?:no[,;:.!? ]*)?(?:así|asi) no[,;:.!? ]*(?:dije|era)[,;:.!? ]*(?:lo que quería decir era|quise decir)[,;:.!? ]+(.+)$",
        "Rectificación detectada en tu frase",
    ),
    (
        r"^no era .+[,;:.!?]+[ ]*(?:era|es)[,;:.!? ]+(.+)$",
        "Rectificación detectada",
    ),
    (
        r"^no[,;:.!? ]+(?:era|es)[,;:.!? ]+(.+)$",
        "Rectificación detectada",
    ),
    (
        r"^donde dije .+[,;:.!? ]+(?:quer[ií]a|quise) decir[,;:.!? ]+(.+)$",
        "Reformulación detectada",
    ),
    (
        r"^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir[,;:.!? ]+(?:era|es)[,;:.!? ]+(.+)$",
        "Rectificación detectada",
    ),
    (
        r"^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir .+[,;:.!? ]+(?:quer[ií]a|quise) decir[,;:.!? ]+(.+)$",
        "Rectificación detectada",
    ),
    (
        r"^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir(?!\s+que\b)[,;:.!? ]+(.+)$",
        "Rectificación detectada",
    ),
    (
        r"^perd[oó]n(?:a|eme)?[,;:.!? ]+(?:(?:quer[ií]a|quise) decir|era|es)[,;:.!? ]+(.+)$",
        "Rectificación después de una disculpa",
    ),
    (
        r"^(?:bueno[,;:.!? ]+)?mejor dicho[,;:.!? ]+(.+)$",
        "Reformulación detectada",
    ),
    (
        r"^(?:bueno[,;:.!? ]+)?m[aá]s bien[,;:.!? ]+(.+)$",
        "Reformulación detectada",
    ),
    (
        r"^me refer[ií]a ((?:a la|al)\s+.+)$",
        "Aclaración detectada",
    ),
    (r"^me refer[ií]a a[,;:.!? ]+(.+)$", "Aclaración detectada"),
    (
        r"^(?:la palabra correcta|lo correcto)[,;:.!? ]+(?:es|ser[ií]a)[,;:.!? ]+(.+)$",
        "Corrección explícita detectada",
    ),
    (
        r"^lo que quería decir era[,;:.!? ]+(.+)$",
        "Rectificación detectada",
    ),
    (
        r"^me equivoqu[eé][,;:.!? ]+(?:(?:quer[ií]a|quise) decir|era|es)[,;:.!? ]+(.+)$",
        "Rectificación después de reconocer un error",
    ),
    (
        r"^(?:rectifico|corrijo)[,;:.!? ]+(.+)$",
        "Corrección explícita detectada",
    ),
    (
        r"^(?:digo|o mejor)[,;:.!?]+[ ]*(.+)$",
        "Reformulación detectada",
    ),
    (r"^.+[,;] *no[,;:.!? ]+(.+)$", "Autocorrección detectada"),
];

const CORRECTION_STARTS: &[&str] = &[
    r"^(?:uy[,;:.!? ]*)?(?:así|asi) no (?:era|es)\.?$",
    r"^(?:no[,;:.!? ]*)?(?:así|asi) no[,;:.!? ]*(?:dije|era)\.?$",
    r"^(?:no[,;:.!? ]*)?(?:así|asi) no[,;:.!? ]*(?:dije|era)[,;:.!? ]*(?:lo que quería decir era|quise decir)\.?$",
    r"^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir[,;:.!? ]*$",
    r"^perd[oó]n(?:a|eme)?[,;:.!? ]+(?:(?:quer[ií]a|quise) decir|era|es)[,;:.!? ]*$",
    r"^(?:bueno[,;:.!? ]+)?mejor dicho[,;:.!? ]*$",
    r"^(?:bueno[,;:.!? ]+)?m[aá]s bien[,;:.!? ]*$",
    r"^me refer[ií]a (?:a|al|a la)[,;:.!? ]*$",
    r"^(?:la palabra correcta|lo correcto)[,;:.!? ]+(?:es|ser[ií]a)[,;:.!? ]*$",
    r"^lo que quería decir era[,;:.!? ]*$",
    r"^donde dije .+[,;:.!? ]*$",
    r"^no era .+[,;:.!? ]*$",
    r"^digo$",
    r"^o mejor$",
    r"^.+[,;] *no[,;:.!? ]*$",
];

fn rules() -> Vec<CorrectionRule> {
    CORRECTION_PATTERNS
        .iter()
        .map(|(pattern, reason)| CorrectionRule {
            pattern: rx(&format!("(?i){pattern}")),
            reason,
        })
        .collect()
}

fn start_rules() -> Vec<Regex> {
    CORRECTION_STARTS
        .iter()
        .map(|p| rx(&format!("(?i){p}")))
        .collect()
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn capitalize_sentence(value: &str) -> String {
    let clean = normalize_spaces(value);
    let mut chars = clean.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => clean,
    }
}

fn is_correction_start(value: &str) -> bool {
    start_rules()
        .iter()
        .any(|r| r.is_match(value).unwrap_or(false))
}

/// Aplica la primera regla de rectificación que casa; si ninguna casa,
/// devuelve el texto normalizado sin corrección.
pub fn apply_correction(input: &str) -> CorrectionResult {
    let value = normalize_spaces(input);

    for rule in rules() {
        let captures = match rule.pattern.captures(&value) {
            Ok(Some(c)) => c,
            _ => continue,
        };
        let Some(matched) = captures.get(1) else {
            continue;
        };
        let replacement = capitalize_sentence(matched.as_str());
        return CorrectionResult {
            correction: Some(Correction {
                id: Uuid::new_v4().to_string(),
                original: value.clone(),
                replacement: replacement.clone(),
                reason: rule.reason.to_string(),
                created_at: Local::now().format("%H:%M").to_string(),
            }),
            text: replacement,
        };
    }

    CorrectionResult {
        text: value,
        correction: None,
    }
}

/// Añade `incoming` al final de `current` con un solo espacio y espacios
/// normalizados. Un `incoming` vacío o en blanco deja el texto actual intacto.
pub fn append_transcript(current: &str, incoming: &str) -> String {
    let next = normalize_spaces(incoming);
    if next.is_empty() {
        return current.to_string();
    }
    normalize_spaces(&format!("{current} {next}"))
}

/// Sustituye la ÚLTIMA frase de `current` (lo que sigue a su último `.`, `!`
/// o `?`) por `replacement`, capitalizado. Sin puntuación previa devuelve solo
/// `replacement`: no había frase que reemplazar.
pub fn replace_last_sentence(current: &str, replacement: &str) -> String {
    let next = capitalize_sentence(replacement);
    if next.is_empty() {
        return current.to_string();
    }

    let last_boundary = current
        .rfind('.')
        .max(current.rfind('!'))
        .max(current.rfind('?'));
    match last_boundary {
        None => next,
        Some(idx) => format!("{} {}", &current[..=idx], next).trim().to_string(),
    }
}

/// Sesión multi-turno: guarda prefijos de rectificación ("Uy, así no era")
/// hasta que el siguiente turno completa la frase corregida.
#[derive(Debug, Default)]
pub struct CorrectionSession {
    pending_turn: String,
}

impl CorrectionSession {
    /// Sesión vacía, sin rectificación a medias.
    pub fn new() -> Self {
        Self::default()
    }

    /// Procesa un turno dictado y dice qué hacer con él.
    ///
    /// Si había un arranque de rectificación guardado («no, era …») se le pega
    /// este turno para completar la frase. Mientras siga sin completarse, el
    /// arranque se mantiene pendiente y `text` vuelve vacío: el llamador debe
    /// conservar el turno anterior, porque la rectificación lo sustituirá.
    ///
    /// `pending` de la respuesta indica si la sesión sigue esperando más texto;
    /// `correction` solo viene rellena cuando la rectificación se aplicó.
    pub fn accept(&mut self, turn: &str) -> CorrectionTurn {
        let value = normalize_spaces(turn);
        if value.is_empty() {
            return CorrectionTurn {
                text: String::new(),
                correction: None,
                pending: !self.pending_turn.is_empty(),
            };
        }

        if !self.pending_turn.is_empty() {
            let combined = append_transcript(&self.pending_turn, &value);
            let candidate = format!(
                "{}, {}",
                self.pending_turn
                    .trim_end_matches(|c| matches!(c, ',' | ';' | ':' | '.' | '!' | '?')),
                value
            );
            let mut correction = apply_correction(&candidate);
            if correction.correction.is_some() {
                self.pending_turn.clear();
                if let Some(c) = correction.correction.as_mut() {
                    c.original = candidate;
                }
                return CorrectionTurn {
                    text: correction.text,
                    correction: correction.correction,
                    pending: false,
                };
            }

            if is_correction_start(&candidate) {
                self.pending_turn = candidate;
                return CorrectionTurn {
                    text: String::new(),
                    correction: None,
                    pending: true,
                };
            }

            let flushed = combined;
            self.pending_turn.clear();
            return CorrectionTurn {
                text: flushed,
                correction: None,
                pending: false,
            };
        }

        let correction = apply_correction(&value);
        if correction.correction.is_some() {
            return CorrectionTurn {
                text: correction.text,
                correction: correction.correction,
                pending: false,
            };
        }

        if is_correction_start(&value) {
            self.pending_turn = value;
            return CorrectionTurn {
                text: String::new(),
                correction: None,
                pending: true,
            };
        }

        CorrectionTurn {
            text: value,
            correction: None,
            pending: false,
        }
    }

    /// Descarta el arranque de rectificación pendiente (al borrar la nota o
    /// al abandonar un turno a medias).
    pub fn reset(&mut self) {
        self.pending_turn.clear();
    }

    /// True si la sesión guarda un arranque de rectificación a medias.
    pub fn has_pending_correction(&self) -> bool {
        !self.pending_turn.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat {
        text: String,
        has_correction: bool,
        original: Option<String>,
        replacement: Option<String>,
        reason: Option<String>,
    }

    fn flat(result: CorrectionResult) -> Flat {
        Flat {
            text: result.text,
            has_correction: result.correction.is_some(),
            original: result.correction.as_ref().map(|c| c.original.clone()),
            replacement: result.correction.as_ref().map(|c| c.replacement.clone()),
            reason: result.correction.as_ref().map(|c| c.reason.clone()),
        }
    }

    fn assert_correction(input: &str, replacement: &str, reason: &str) {
        let f = flat(apply_correction(input));
        assert_eq!(f.text, replacement, "texto para {input:?}");
        assert!(f.has_correction, "debe haber corrección para {input:?}");
        assert_eq!(f.original.as_deref(), Some(input));
        assert_eq!(f.replacement.as_deref(), Some(replacement));
        assert_eq!(f.reason.as_deref(), Some(reason));
    }

    #[test]
    fn one_turn_complete_correction() {
        let f = flat(apply_correction(
            "Uy, así no era, era el informe del jueves",
        ));
        assert_eq!(f.text, "El informe del jueves");
        assert!(f.has_correction);
    }

    #[test]
    fn detects_all_common_markers() {
        assert_correction(
            "No, quería decir el jueves",
            "El jueves",
            "Rectificación detectada",
        );
        assert_correction("No, era el jueves", "El jueves", "Rectificación detectada");
        assert_correction(
            "Perdón, era la reunión del viernes",
            "La reunión del viernes",
            "Rectificación después de una disculpa",
        );
        assert_correction(
            "Perdona, quería decir la reunión del viernes",
            "La reunión del viernes",
            "Rectificación después de una disculpa",
        );
        assert_correction(
            "Mejor dicho, el informe final",
            "El informe final",
            "Reformulación detectada",
        );
        assert_correction(
            "Más bien, el informe final",
            "El informe final",
            "Reformulación detectada",
        );
        assert_correction(
            "Me refería al informe final",
            "Al informe final",
            "Aclaración detectada",
        );
        assert_correction(
            "La palabra correcta es presupuesto",
            "Presupuesto",
            "Corrección explícita detectada",
        );
        assert_correction(
            "Lo correcto sería el jueves",
            "El jueves",
            "Corrección explícita detectada",
        );
        assert_correction(
            "Lo que quería decir era el jueves",
            "El jueves",
            "Rectificación detectada",
        );
        assert_correction(
            "Me equivoqué, quería decir el jueves",
            "El jueves",
            "Rectificación después de reconocer un error",
        );
        assert_correction(
            "Rectifico: el jueves",
            "El jueves",
            "Corrección explícita detectada",
        );
        assert_correction(
            "Corrijo, la reunión del viernes",
            "La reunión del viernes",
            "Corrección explícita detectada",
        );
        assert_correction("Digo, el jueves", "El jueves", "Reformulación detectada");
        assert_correction("O mejor, el jueves", "El jueves", "Reformulación detectada");
        assert_correction(
            "El martes, no, el jueves",
            "El jueves",
            "Autocorrección detectada",
        );
        assert_correction(
            "Donde dije martes, quería decir jueves",
            "Jueves",
            "Reformulación detectada",
        );
        assert_correction(
            "No era martes, era jueves",
            "Jueves",
            "Rectificación detectada",
        );
    }

    #[test]
    fn accepts_punctuation_between_marker_and_replacement() {
        let f = flat(apply_correction("Así no era. Era la reunión del viernes"));
        assert_eq!(f.text, "La reunión del viernes");
        assert!(f.has_correction);
    }

    #[test]
    fn leaves_ordinary_speech_unchanged() {
        for input in [
            "Preparar el informe para el jueves",
            "La reunión es el jueves",
            "Digo que la reunión empieza a las diez",
            "Quise decir que el informe ya está listo",
        ] {
            let f = flat(apply_correction(input));
            assert_eq!(f.text, input, "debe quedar igual: {input:?}");
            assert!(!f.has_correction);
        }
    }

    #[test]
    fn session_holds_prefix_until_completed() {
        let mut session = CorrectionSession::new();
        assert_eq!(
            session.accept("Uy, así no era"),
            CorrectionTurn {
                text: String::new(),
                correction: None,
                pending: true
            }
        );
        assert!(session.has_pending_correction());

        let completed = session.accept("era el informe del jueves");
        assert_eq!(completed.text, "El informe del jueves");
        assert_eq!(
            completed.correction.as_ref().map(|c| c.original.as_str()),
            Some("Uy, así no era, era el informe del jueves")
        );
        assert_eq!(
            completed
                .correction
                .as_ref()
                .map(|c| c.replacement.as_str()),
            Some("El informe del jueves")
        );
        assert!(!completed.pending);
        assert!(!session.has_pending_correction());
    }

    #[test]
    fn session_completes_split_prefixes() {
        for (prefix, replacement, expected) in [
            ("No, quería decir", "el jueves", "El jueves"),
            (
                "Perdón, quería decir",
                "la reunión del viernes",
                "La reunión del viernes",
            ),
            ("Mejor dicho", "el informe final", "El informe final"),
            (
                "Me refería a",
                "la reunión del viernes",
                "La reunión del viernes",
            ),
            ("Digo", "el jueves", "El jueves"),
        ] {
            let mut session = CorrectionSession::new();
            let first = session.accept(prefix);
            assert!(first.pending, "prefijo {prefix:?} debe quedar pendiente");
            let completed = session.accept(replacement);
            assert_eq!(completed.text, expected, "para {prefix:?}");
            assert!(!completed.pending);
        }
    }

    #[test]
    fn session_supports_three_turn_correction() {
        let mut session = CorrectionSession::new();
        assert!(session.accept("No, así no dije").pending);
        assert!(session.accept("lo que quería decir era").pending);
        let completed = session.accept("la reunión del viernes");
        assert_eq!(completed.text, "La reunión del viernes");
        assert_eq!(
            completed
                .correction
                .as_ref()
                .map(|c| c.replacement.as_str()),
            Some("La reunión del viernes")
        );
    }
}
