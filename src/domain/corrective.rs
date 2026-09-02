//! Corrección hablada con LLM — capa v2 de docs/CORRECCION-VOZ.md.
//!
//! La capa v1 (`correction.rs`) resuelve por reglas los turnos tipo
//! «no, quería decir X». Este módulo decide cuándo un turno NO lo resuelven
//! las reglas y huele a corrección («no no lo último está mal, es 2 en vez
//! de 4») → `NeedsLlm`: el LLM recibe el último turno dictado + la
//! instrucción y devuelve un diff `{find, replace}` que aquí se valida y
//! aplica. Lógica 100% pura (sin I/O): el cliente HTTP vive en `src/llm.rs`.

use fancy_regex::Regex;

/// Decisión del clasificador barato (§2 de la spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectiveDecision {
    /// Dictado normal → `append_transcript` como siempre.
    Plain,
    /// Intención correctiva que las reglas no cubren → pipeline LLM.
    NeedsLlm,
}

/// Diff de sustitución devuelto por el LLM (contrato §3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    /// Subcadena literal (tal cual) en el texto a corregir.
    pub find: String,
    /// Texto que sustituye a `find` (vacío = quitar).
    pub replace: String,
}

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("patrón correctivo inválido")
}

// Señales §2. Peso por diseño: la regla de decisión es
// (negación | verbo correctivo) Y (numeral | referencia anafórica).
const NEGATION: &str = r"(?i)\b(no|mal|me equivoqu[eé]|equivocad[oa]|est[aá] mal|as[ií] no)\b";
const CORRECTIVE_VERB: &str =
    r"(?i)\b(corr[ií]ge|corrig[eé]|corr[ií]geme|cambia|cambie|pon|quita|qu[ií]tame|borra|elimina|sustituye|sustituir)\b";
const NUMERAL: &str = r"(?i)(\b\d+(?:[.,]\d+)?\b|en vez de|en lugar de|en su lugar|dos|tres|cuatro|cinco|seis|siete|ocho|nueve|diez|once|doce|trece|veinte|treinta|cuarenta|cincuenta|sesenta|setenta|ochenta|noventa|cien|mil)\b";
const REFERENCE: &str = r"(?i)\b(lo [úu]ltimo|la [úu]ltima palabra|la [úu]ltima frase|eso|esto|lo otro|esta palabra|esta frase|esa palabra|ese n[uú]mero|lo que acabo de decir|aquello)\b";

/// Clasificador de dos niveles (§2): solo envía al LLM los turnos con
/// (negación o verbo correctivo) Y (numeral o referencia al texto previo).
/// Falso positivo tolerable: el contrato del LLM devuelve `find` vacío si no
/// localiza nada, así que nunca se borra texto a ciegas. Precisión > recall.
pub fn classify(turn: &str) -> CorrectiveDecision {
    let neg = rx(NEGATION).is_match(turn).unwrap_or(false);
    let verb = rx(CORRECTIVE_VERB).is_match(turn).unwrap_or(false);
    let num = rx(NUMERAL).is_match(turn).unwrap_or(false);
    let reference = rx(REFERENCE).is_match(turn).unwrap_or(false);

    if (neg || verb) && (num || reference) {
        CorrectiveDecision::NeedsLlm
    } else {
        CorrectiveDecision::Plain
    }
}

/// Aplica el diff al texto (normalmente `last_turn`): sustituye la PRIMERA
/// ocurrencia de la subcadena literal `find`. Devuelve `None` si `find` no
/// está presente (o está vacío) → corrección descartada, nunca se muta.
///
/// ponytail: primera ocurrencia, no última. Para numerales «lo último» el
/// prompt ya pide que `find` sea único; si algún día hace falta, la última
/// ocurrencia se consigue con `rfind` en 2 líneas.
pub fn apply_diff(text: &str, diff: &Diff) -> Option<String> {
    if diff.find.is_empty() {
        return None;
    }
    let idx = text.find(&diff.find)?;
    let mut out = String::with_capacity(text.len() - diff.find.len() + diff.replace.len());
    out.push_str(&text[..idx]);
    out.push_str(&diff.replace);
    out.push_str(&text[idx + diff.find.len()..]);
    Some(out)
}

/// El transcript siempre termina con «… {last_turn}»: sustituye la ÚLTIMA
/// ocurrencia de `old_turn` (la recién dictada) por `new_turn` corregido.
pub fn replace_last_turn(transcript: &str, old_turn: &str, new_turn: &str) -> Option<String> {
    if old_turn.is_empty() {
        return None;
    }
    let ts = transcript.trim_end();
    let start = ts.rfind(old_turn)?;
    let mut out = String::with_capacity(ts.len() - old_turn.len() + new_turn.len());
    out.push_str(&ts[..start]);
    out.push_str(new_turn);
    out.push_str(&ts[start + old_turn.len()..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictado_normal_no_se_marca_como_correccion() {
        for turn in [
            "la reunión es el jueves",
            "tengo dos reuniones mañana",
            "no pasa nada",
            "cambia el canal de la tele",
            "esto es importante para mañana",
            "quiero comprar una mesa de cuatro patas",
        ] {
            assert_eq!(classify(turn), CorrectiveDecision::Plain, "debe ser Plain: {turn:?}");
        }
    }

    #[test]
    fn intencion_correctiva_se_envia_al_llm() {
        for turn in [
            // Caso canónico de la spec (§5).
            "no no lo último está mal es 2 en vez de 4",
            // Referencia anafórica + negación.
            "esto no, era lo otro",
            "eso no, quería decir el otro día",
            // Verbo correctivo + referencia/numeral.
            "quita la última palabra",
            "cambia eso",
            "pon dos en vez de cuatro",
            "me equivoqué, ese número no es",
        ] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "debe ir al LLM: {turn:?}");
        }
    }

    #[test]
    fn apply_diff_sustituye_numerales() {
        let diff = Diff { find: "4".into(), replace: "2".into() };
        assert_eq!(
            apply_diff("la reunión es a las 4 en punto", &diff).as_deref(),
            Some("la reunión es a las 2 en punto")
        );
    }

    #[test]
    fn apply_diff_sustituye_palabras_numero() {
        let diff = Diff { find: "cuatro".into(), replace: "dos".into() };
        assert_eq!(
            apply_diff("la reunión es a las cuatro en punto", &diff).as_deref(),
            Some("la reunión es a las dos en punto")
        );
    }

    #[test]
    fn apply_diff_sustituye_frase_corta() {
        let diff = Diff { find: "el informe semanal".into(), replace: "el informe del jueves".into() };
        assert_eq!(
            apply_diff("hay que revisar el informe semanal antes del viernes", &diff).as_deref(),
            Some("hay que revisar el informe del jueves antes del viernes")
        );
    }

    #[test]
    fn diff_invalido_no_muta_texto() {
        let diff = Diff { find: "no existe".into(), replace: "X".into() };
        assert_eq!(apply_diff("la reunión es a las 4", &diff), None);
        let vacio = Diff { find: String::new(), replace: "X".into() };
        assert_eq!(apply_diff("la reunión es a las 4", &vacio), None);
    }

    #[test]
    fn apply_diff_quitar_texto() {
        let diff = Diff { find: " el jueves".into(), replace: String::new() };
        assert_eq!(
            apply_diff("la reunión es el jueves a las 4", &diff).as_deref(),
            Some("la reunión es a las 4")
        );
    }

    #[test]
    fn replace_last_turn_actualiza_el_final_del_transcript() {
        let transcript = "Hola. La reunión es a las 4";
        assert_eq!(
            replace_last_turn(transcript, "La reunión es a las 4", "La reunión es a las 2").as_deref(),
            Some("Hola. La reunión es a las 2")
        );
    }

    #[test]
    fn replace_last_turn_usa_la_ultima_ocurrencia() {
        let transcript = "Toma 4. No, toma 4";
        assert_eq!(
            replace_last_turn(transcript, "No, toma 4", "No, toma 2").as_deref(),
            Some("Toma 4. No, toma 2")
        );
    }

    #[test]
    fn replace_last_turn_old_vacio_o_ausente() {
        assert_eq!(replace_last_turn("Hola", "", "X"), None);
        assert_eq!(replace_last_turn("Hola", "no está", "X"), None);
    }
}