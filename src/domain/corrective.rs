//! Corrección hablada con LLM — capa v2 de docs/CORRECCION-VOZ.md.
//!
//! La capa v1 (`correction.rs`) resuelve por reglas los turnos tipo
//! «no, quería decir X». Este módulo decide cuándo un turno NO lo resuelven
//! las reglas y huele a corrección («no no lo último está mal, es 2 en vez
//! de 4») → `NeedsLlm`: el LLM recibe el último turno dictado + la
//! instrucción y devuelve un diff `{find, replace}` que aquí se valida y
//! aplica. Lógica 100% pura (sin I/O, sin red): el cliente HTTP vive en
//! `src/llm.rs`.
//!
//! Contrato de aplicación (§3.3 de la spec): `find` debe ser subcadena
//! literal del texto objetivo; si no lo es, la corrección se descarta y el
//! turno se trata como dictado normal (nunca se muta ni se pierde texto).

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
    /// Subcadena literal (tal cual) que debe existir en el texto a corregir.
    pub find: String,
    /// Texto que sustituye a `find` (vacío = quitar).
    pub replace: String,
}

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("patrón correctivo inválido")
}

// Señales §2. Regla de decisión (nivel B): (negación | verbo correctivo) Y
// (numeral | referencia anafórica). Peso por diseño: precisión > recall — un
// falso positivo cuesta una llamada al LLM que devolverá `find` vacío y el
// turno se conserva; un falso negativo solo significa no corregir.
const NEGATION: &str = r"(?i)\b(no|mal|me equivoqu[eé]|equivocad[oa]|est[aá] mal|as[ií] no)\b";
const CORRECTIVE_VERB: &str =
    r"(?i)\b(corr[ií]ge|corrig[eé]|corr[ií]geme|cambia|cambie|pon|quita|qu[ií]tame|borra|elimina|sustituye|sustituir)\b";
const NUMERAL: &str = r"(?i)(\b\d+(?:[.,]\d+)?\b|en vez de|en lugar de|en su lugar|dos|tres|cuatro|cinco|seis|siete|ocho|nueve|diez|once|doce|trece|veinte|treinta|cuarenta|cincuenta|sesenta|setenta|ochenta|noventa|cien|mil)\b";
const REFERENCE: &str = r"(?i)\b(lo [úu]ltimo|la [úu]ltima palabra|la [úu]ltima frase|eso|esto|lo otro|esta palabra|esta frase|esa palabra|ese n[uú]mero|el n[uú]mero ese|lo que acabo de decir|aquello)\b";

/// Clasificador de dos niveles (§2): solo enruta al LLM los turnos con
/// (negación o verbo correctivo) Y (numeral o referencia al texto previo).
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
/// ocurrencia de la subcadena literal `find`. Devuelve `None` si `find` está
/// vacío o no es subcadena literal → corrección descartada, el texto NO se
/// muta (§3.3.1).
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
/// ocurrencia de `old_turn` (la recién dictada y ya corregida) por
/// `new_turn`. Devuelve `None` si `old_turn` está vacío o ya no aparece.
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

    // ── Clasificador: dictado normal nunca se marca como corrección (≥5) ──

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
    fn numera_por_si_solo_es_dictado_normal() {
        // §2: el numeral solo no basta — hace falta negación o verbo correctivo.
        for turn in ["a las 4 en punto", "son las 9 y media", "el informe 2026"] {
            assert_eq!(classify(turn), CorrectiveDecision::Plain, "{turn:?}");
        }
    }

    #[test]
    fn negacion_sin_objetivo_es_dictado_normal() {
        // «no» idiomático en dictado cotidiano, sin numeral ni referencia.
        for turn in ["no me gusta el café", "no sé si vendrá mañana", "mal no está"] {
            assert_eq!(classify(turn), CorrectiveDecision::Plain, "{turn:?}");
        }
    }

    // ── Clasificador: intención correctiva → LLM (≥5) ─────────────────────

    #[test]
    fn intencion_correctiva_se_envia_al_llm() {
        for turn in [
            // Caso canónico de la spec (§5).
            "no no lo último está mal es 2 en vez de 4",
            // Negación + referencia anafórica.
            "esto no, era lo otro",
            "eso no, quería decir el otro día",
            // Verbo correctivo + numeral/referencia.
            "quita la última palabra",
            "cambia eso",
            "pon dos en vez de cuatro",
            // Negación + numeral.
            "me equivoqué, ese número no es",
            "no era 4, era 2",
        ] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "debe ir al LLM: {turn:?}");
        }
    }

    #[test]
    fn verbos_correctivos_con_objetivo_se_envian_al_llm() {
        for turn in ["borra la última frase", "sustituye eso", "quítame el número ese"] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "{turn:?}");
        }
    }

    // ── apply_diff: sustitución correcta (≥5) ─────────────────────────────

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
    fn apply_diff_quita_texto_con_replace_vacio() {
        let diff = Diff { find: " el jueves".into(), replace: String::new() };
        assert_eq!(
            apply_diff("la reunión es el jueves a las 4", &diff).as_deref(),
            Some("la reunión es a las 4")
        );
    }

    #[test]
    fn apply_diff_sustituye_la_primera_ocurrencia() {
        // §3.3: primera ocurrencia; el prompt pide que find sea único para
        // numerales («lo último» suele ser la última, pero el contrato puede
        // pedir una subcadena más larga que la haga única).
        let diff = Diff { find: "4".into(), replace: "2".into() };
        assert_eq!(
            apply_diff("toma 4, no 4", &diff).as_deref(),
            Some("toma 2, no 4")
        );
    }

    // ── apply_diff: diff inválido → None, nunca muta ──────────────────────

    #[test]
    fn diff_invalido_no_muta_texto() {
        let ausente = Diff { find: "no existe".into(), replace: "X".into() };
        assert_eq!(apply_diff("la reunión es a las 4", &ausente), None);
        let vacio = Diff { find: String::new(), replace: "X".into() };
        assert_eq!(apply_diff("la reunión es a las 4", &vacio), None);
        // El texto original queda intacto tras los intentos.
        assert_eq!(apply_diff("la reunión es a las 4", &ausente), None);
    }

    // ── replace_last_turn: última ocurrencia del turno en el transcript ───

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

    // ── Caso canónico de la spec (§7) ─────────────────────────────────────

    #[test]
    fn caso_canonico_completo_cuatro_a_dos() {
        let transcript = "La reunión es a las 4";
        let turn = "La reunión es a las 4";
        let instruction = "no no lo último está mal es 2 en vez de 4";

        assert_eq!(classify(instruction), CorrectiveDecision::NeedsLlm);
        let after = apply_diff(turn, &Diff { find: "4".into(), replace: "2".into() })
            .expect("find debe ser subcadena literal");
        assert_eq!(after, "La reunión es a las 2");
        let fixed = replace_last_turn(transcript, turn, &after).expect("turno presente");
        assert_eq!(fixed, "La reunión es a las 2");
    }

    // ── Undo (spec §4.3): restaura `before` tras `after` ──────────────────

    #[test]
    fn undo_restaura_el_estado_anterior() {
        let transcript = "La reunión es a las 4";
        let turn = "La reunión es a las 4";
        let after = apply_diff(turn, &Diff { find: "4".into(), replace: "2".into() }).unwrap();
        let fixed = replace_last_turn(transcript, turn, &after).unwrap();

        // Deshacer = reemplazar el turno corregido por el original.
        let undone = replace_last_turn(&fixed, &after, turn).unwrap();
        assert_eq!(undone, transcript);
    }

    #[test]
    fn apply_diff_preserva_el_resto_del_texto() {
        // Solo cambia el fragmento objetivo; ni prefijo ni sufijo se tocan
        // (importante para el undo de 1 nivel en app.rs).
        let diff = Diff { find: "4".into(), replace: "2".into() };
        let after = apply_diff("la reunión es a las 4 en punto", &diff).unwrap();
        assert_eq!(after, "la reunión es a las 2 en punto");
        let restored = apply_diff(&after, &Diff { find: "2".into(), replace: "4".into() }).unwrap();
        assert_eq!(restored, "la reunión es a las 4 en punto");
    }
}