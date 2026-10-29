//! Corrección hablada con un modelo de lenguaje — capa v2 de docs/CORRECCION-VOZ.md.
//!
//! La capa v1 (`correction.rs`) resuelve por reglas los turnos tipo
//! «no, quería decir X». Este módulo decide cuándo un turno NO lo resuelven
//! las reglas y suena a corrección («no no lo último está mal, es 2 en vez
//! de 4») → `NeedsLlm`: el modelo recibe el último turno dictado y la
//! instrucción, y devuelve un cambio `{find, replace}` que aquí se comprueba
//! y se aplica. Nada de red ni de ficheros: el cliente HTTP está en
//! `src/llm.rs`.
//!
//! Cómo se aplica (punto 3.3 del documento): `find` tiene que aparecer tal
//! cual en el texto; si no aparece, la corrección se tira y el turno se trata
//! como un dictado normal (nunca se cambia ni se pierde texto).

use fancy_regex::Regex;

/// Lo que decide el clasificador barato (punto 2 del documento).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectiveDecision {
    /// Dictado normal → `append_transcript` como siempre.
    Plain,
    /// Corrección que las reglas no cubren → pasa por el modelo.
    NeedsLlm,
}

/// Cambio de sustitución que devuelve el modelo (contrato del punto 3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    /// Trozo de texto (tal cual) que tiene que estar en el texto a corregir.
    pub find: String,
    /// Texto que entra en lugar de `find` (vacío = quitarlo).
    pub replace: String,
}

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("patrón correctivo inválido")
}

// Señales (punto 2). Regla de decisión (nivel B): (negación | verbo de
// corrección | marca de sustitución) Y (número | referencia a algo ya dicho).
// Se prefiere no equivocarse a acertar siempre: un falso positivo cuesta una
// llamada al modelo que devolverá `find` vacío y el turno se queda igual; un
// falso negativo solo significa no corregir.
//
// `SUBSTITUTION_MARKER` cubre «X en vez de Y», «X en lugar de Y», «X en su
// lugar», «sustituir X por Y» y «cambiar X por Y» sin verbo aparte. Son
// correcciones por sí solas: «2 en vez de 4» ya pide cambiar un 4 por un 2,
// sin un «no» antes. Si no las contáramos como negación o verbo, el caso del
// día a día que motiva la corrección IA (punto 5) se quedaría en `Plain` y
// nunca llegaría al modelo.
const NEGATION: &str = r"(?i)\b(no|mal|me equivoqu[eé]|equivocad[oa]|est[aá] mal|as[ií] no)\b";
const CORRECTIVE_VERB: &str = r"(?i)\b(corr[ií]ge|corrig[eé]|corr[ií]geme|cambia|cambie|pon|quita|qu[ií]tame|borra|elimina|sustituye|sustituir|di que|sea)\b";
const SUBSTITUTION_MARKER: &str = r"(?i)\b(en vez de|en lugar de|en su lugar|cambia por|cámbialo por|sustitúyelo por|reemplaza con)\b";
const NUMERAL: &str = r"(?i)(\b\d+(?:[.,]\d+)?\b|dos|tres|cuatro|cinco|seis|siete|ocho|nueve|diez|once|doce|trece|veinte|treinta|cuarenta|cincuenta|sesenta|setenta|ochenta|noventa|cien|mil)\b";
const REFERENCE: &str = r"(?i)\b(lo [úu]ltimo|la [úu]ltima palabra|la [úu]ltima frase|eso|esto|lo otro|esta palabra|esta frase|esa palabra|ese n[uú]mero|el n[uú]mero ese|lo que acabo de decir|aquello)\b";

/// Clasificador de dos niveles (punto 2): solo manda al modelo los turnos con
/// (negación o verbo de corrección) Y (número o referencia a lo ya dictado).
///
/// `threshold` (0..=100) regula lo sensible que es: cuando un turno pasa la
/// regla de los dos niveles, se puntúa por las señales que lleva
/// (negación/verbo/número/referencia) sobre 4. Como la regla pide mínimo 2
/// señales, el resultado queda entre 50 y 100. El umbral decide si se manda:
/// 50 por defecto (todo lo que pasa la regla va al modelo, como antes); más
/// alto = más prudente (mejor no pasarse que corregir de más, punto 2). 0 =
/// apagado (no manda nada; es como desactivar la corrección IA sin tocar el
/// interruptor).
pub fn classify(turn: &str) -> CorrectiveDecision {
    classify_with(turn, 50)
}

/// Versión con el umbral a mano (la usa `app.rs` con el valor de
/// `Config::llm_classifier_threshold`). La misma regla de dos niveles,
/// más el filtro de confianza por encima.
pub fn classify_with(turn: &str, threshold: u8) -> CorrectiveDecision {
    let neg = rx(NEGATION).is_match(turn).unwrap_or(false);
    let verb = rx(CORRECTIVE_VERB).is_match(turn).unwrap_or(false);
    // `subst` cubre «X en vez de Y» / «sustituir X por Y» SIN verbo
    // aparte. Para la regla de los dos niveles cuenta como verbo de
    // corrección: el turno ya ES una corrección.
    let subst = rx(SUBSTITUTION_MARKER).is_match(turn).unwrap_or(false);
    let num = rx(NUMERAL).is_match(turn).unwrap_or(false);
    let reference = rx(REFERENCE).is_match(turn).unwrap_or(false);

    // Regla de los dos niveles (punto 2): sin (negación|verbo|subst) o sin
    // (número|referencia) NO se manda al modelo, porque un falso positivo es
    // peor que un falso negativo (un acierto por casualidad nos cuesta una
    // llamada y un `find` vacío).
    //
    // Excepción para la marca de sustitución: «X en vez de Y» ya es la
    // sustitución en sí, así que con la marca más número o referencia se
    // cumple el segundo nivel sin necesidad de negación o verbo explícito.
    // Sin número ni referencia ni negación ni verbo, el turno es solo un
    // «en vez de» coloquial (por ejemplo «en vez de descansar prefiero
    // esto»), que NO es una corrección: sigue siendo Plain.
    if !(neg || verb || subst) {
        return CorrectiveDecision::Plain;
    }
    if subst && !neg && !verb && !(num || reference) {
        // Sustitución suelta sin objetivo: NO mandar.
        return CorrectiveDecision::Plain;
    }
    if !(num || reference) {
        return CorrectiveDecision::Plain;
    }
    if threshold == 0 {
        return CorrectiveDecision::Plain;
    }

    // Puntuación (punto 2): cuántas señales hay
    // (negación/verbo/subst/número/referencia). Mínimo 2 (la regla de los
    // dos niveles). Se suma 1 cuando el «subst» es la única señal de
    // corrección y viene con número o referencia: «X en vez de Y» cuenta
    // como si fueran 3 señales → 75, que pasa el 50 por defecto (y así el
    // caso «2 en vez de 4» sigue como antes). Subir el umbral = más prudente.
    let base_score = [neg, verb, subst, num, reference]
        .iter()
        .filter(|x| **x)
        .count() as u16;
    let score = if subst && (num || reference) && !neg && !verb {
        base_score + 1 // bonus: «X en vez de Y» cuenta doble por construcción
    } else {
        base_score
    };
    let ratio: u16 = match score {
        2 => 50,
        3 => 75,
        4 => 90,
        _ => 100, // score >= 5
    };
    if ratio >= u16::from(threshold) {
        CorrectiveDecision::NeedsLlm
    } else {
        CorrectiveDecision::Plain
    }
}

// ── Auto-corrección DENTRO del mismo turno (t_78d9e1fa) ──────────────────
//
// El caso que dio pie a esta capa: el usuario dicta TODO de un tirón y se
// corrige sobre la marcha:
//
//   «que calor que hace en murcia digo cartagena»
//
// El clasificador de un turno a otro (`classify`) NO lo ve: no hay
// referencia a lo dicho antes (no hay «lo último», no hay «eso») ni una
// negación al principio. Sin darse cuenta de la autocorrección, el turno se
// queda en `Plain` y la app escribe tal cual «que calor que hace en murcia
// digo cartagena», con el «digo cartagena» que NO debe salir en el texto.
//
// `classify_in_turn` reconoce las marcas típicas de autocorrección dentro
// del mismo turno:
//
//   - «digo X» / «digo: X» / «diga X» / «dime X»
//   - «quiero decir X» / «quise decir X» / «mejor dicho X»
//   - «no X sino Y» / «no X, Y» / «X no, Y»
//   - «mejor X» / «mejor: X»
//   - «X en vez de Y» / «X en lugar de Y»
//
// A diferencia de `classify`, NO pide número ni referencia: el «antes» y el
// «después» están en el mismo turno. La marca de corrección basta por sí
// sola; el modelo se encarga de ver qué se sustituye (el texto anterior a
// la marca) y por qué (el texto posterior).
//
// Filtros de seguridad: el turno tiene que tener una longitud mínima
// razonable (con menos de 6 caracteres nunca es una corrección: «digo sí»,
// «no no», etc.). Además tiene que quedar algo de contenido «anterior» a la
// marca (al menos 3 caracteres) y algo «posterior» (al menos 1 carácter)
// para que el modelo tenga con qué trabajar.
//
// Esta capa es el modo PRINCIPAL desde el 10/09/2026 (decisión de
// Juanfri). El modo de un turno a otro (`classify`) se queda para frases
// como «no no lo último está mal, es 2 en vez de 4», donde la corrección
// apunta a un turno anterior.
// la corrección apunta a un turno anterior.
const IN_TURN_MARKERS: &str = r"(?i)\b(digo\b|diga\b|dime\b|mejor dicho\b|quiero decir\b|quise decir\b|no,?\s*sino\b|mejor:|mejor\b.+?\bque\b|en vez de\b|en lugar de\b)\b";

/// ¿El turno trae una autocorrección que se resuelve DENTRO del propio
/// turno (no apunta a uno anterior)? Si devuelve `true`, el modelo recibe
/// el turno entero y devuelve el texto final ya corregido (sin la
/// meta-lengua). Si devuelve `false`, se pasa al clasificador de un turno
/// a otro (`classify`/`classify_with`), que es el camino de antes.
///
/// Reglas (documento del task t_78d9e1fa):
/// - El turno tiene que llevar una marca de autocorrección (la regla de arriba).
/// - Longitud mínima 6 caracteres (un «digo sí» no es corrección, es respuesta).
/// - Antes de la primera marca tiene que haber ≥ 3 caracteres (el «antes»
///   que se va a sustituir).
/// - Después de la última marca tiene que haber ≥ 1 carácter (el «después»
///   que sustituye).
///
/// Tests en `tests`: el caso de Juanfri y variantes coloquiales.
pub fn classify_in_turn(turn: &str) -> bool {
    classify_in_turn_with(turn, true)
}

/// Versión con el interruptor a mano para que `app.rs` pregunte al config si
/// el modo dentro del turno está activo (sí por defecto desde el 10/09/2026;
/// si algún día queremos dejar apagarlo, será este parámetro y NO un cambio
/// de comportamiento por la cara).
pub fn classify_in_turn_with(turn: &str, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    let trimmed = turn.trim();
    if trimmed.len() < 6 {
        return false;
    }
    let marker_re = rx(IN_TURN_MARKERS);
    let m = match marker_re.find(trimmed) {
        Ok(Some(m)) => m,
        _ => return false,
    };
    let start = m.start();
    let end = m.end();
    // Antes de la marca: lo ya dicho, candidato a que lo sustituyan.
    // Con menos de 3 caracteres no hay nada útil que corregir.
    let before = trimmed[..start].trim();
    if before.len() < 3 {
        return false;
    }
    // Después de la marca: la corrección en sí. Con 1 carácter basta para
    // saber que está («digo 5» ya lleva «5»).
    let after = trimmed[end..].trim();
    if after.is_empty() {
        return false;
    }
    true
}

/// ¿El texto lleva todavía una marca dentro del turno (meta-lengua)? Lo usa
/// `app.rs` como red de seguridad después de llamar al modelo: si el
/// corrector no limpió la meta-lengua, devolvemos el texto de entrada y
/// evitamos escribir algo que lleve «digo X» tal cual (eso estropearía la
/// corrección y liaría al usuario).
///
/// Se diferencia de `classify_in_turn` en que este ayudante es más flojo:
/// solo busca la marca en cualquier parte del texto (sin pedir contenido
/// antes ni después). Con eso basta para comprobar que el modelo ha
/// quitado la meta-lengua.
pub fn contains_in_turn_marker(text: &str) -> bool {
    rx(IN_TURN_MARKERS).is_match(text).unwrap_or(false)
}

/// Quita del texto la meta-lengua de la corrección y deja solo lo útil
/// (la frase ya corregida). Lo usa `app.rs` para enseñar el antes en el
/// aviso de la interfaz («Auto-corregido IA: «X» → «Y»») cuando el turno
/// original llevaba una marca dentro.
///
/// Cómo funciona: busca la primera marca y devuelve todo lo que va ANTES.
/// Si el modelo ya aplicó la corrección, el turno original tenía la forma
/// «A digo B»: aquí se saca «A» como el «antes» que se enseña. Si por lo
/// que sea no hay marca, devuelve el texto tal cual.
///
/// OJO: esto no deshace lo que hizo el corrector, solo sirve para enseñar
/// el cambio de forma legible. Quien corrige de verdad es el modelo.
pub fn strip_in_turn_meta(text: &str) -> String {
    let trimmed = text.trim();
    let marker_re = rx(IN_TURN_MARKERS);
    let m = match marker_re.find(trimmed) {
        Ok(Some(m)) => m,
        _ => return text.to_string(),
    };
    let before = trimmed[..m.start()].trim();
    if before.is_empty() {
        text.to_string()
    } else {
        format!("{}…", before)
    }
}

/// Aplica el cambio al texto (normalmente `last_turn`): sustituye la PRIMERA
/// aparición de `find` tal cual. Devuelve `None` si `find` está vacío o no
/// aparece → corrección descartada y el texto NO se toca (punto 3.3.1).
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

/// El texto dictado siempre acaba en «… {last_turn}»: sustituye la ÚLTIMA
/// aparición de `old_turn` (el recién dictado y ya corregido) por
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

    // ── Clasificador: un dictado normal nunca se marca como corrección (≥5) ──

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
            assert_eq!(
                classify(turn),
                CorrectiveDecision::Plain,
                "debe ser Plain: {turn:?}"
            );
        }
    }

    #[test]
    fn numera_por_si_solo_es_dictado_normal() {
        // Punto 2: el número solo no basta, hace falta negación o verbo.
        for turn in ["a las 4 en punto", "son las 9 y media", "el informe 2026"] {
            assert_eq!(classify(turn), CorrectiveDecision::Plain, "{turn:?}");
        }
    }

    #[test]
    fn negacion_sin_objetivo_es_dictado_normal() {
        // «no» de relleno en el habla diaria, sin número ni referencia.
        for turn in [
            "no me gusta el café",
            "no sé si vendrá mañana",
            "mal no está",
        ] {
            assert_eq!(classify(turn), CorrectiveDecision::Plain, "{turn:?}");
        }
    }

    // ── Clasificador: intención de corregir → modelo (≥5) ───────────────────

    #[test]
    fn intencion_correctiva_se_envia_al_llm() {
        for turn in [
            // El caso de ejemplo del documento (punto 5).
            "no no lo último está mal es 2 en vez de 4",
            // Negación más referencia a lo dicho.
            "esto no, era lo otro",
            "eso no, quería decir el otro día",
            // Verbo de corrección más número o referencia.
            "quita la última palabra",
            "cambia eso",
            "pon dos en vez de cuatro",
            // Negación más número.
            "me equivoqué, ese número no es",
            "no era 4, era 2",
        ] {
            assert_eq!(
                classify(turn),
                CorrectiveDecision::NeedsLlm,
                "debe ir al LLM: {turn:?}"
            );
        }
    }

    #[test]
    fn verbos_correctivos_con_objetivo_se_envian_al_llm() {
        for turn in [
            "borra la última frase",
            "sustituye eso",
            "quítame el número ese",
        ] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "{turn:?}");
        }
    }

    // ── Sustitución sin verbo aparte (fallo de Juanfri, 09/09/2026) ─────────
    //
    // Antes del arreglo, «2 en vez de 4» se veía como (subst no encajaba y
    // no había verbo) → (false || false) → Plain. La frase de todos los días
    // que motiva la corrección IA (punto 5) nunca llegaba al modelo. La marca
    // nueva `SUBSTITUTION_MARKER` cubre «X en vez de Y» (y variantes) como
    // señal suficiente para la regla de los dos niveles. Estos tests protegen
    // esa conducta: si alguien recorta la regla a como estaba, fallan al
    // momento.

    #[test]
    fn correccion_minima_con_en_vez_de_llega_al_llm() {
        // El caso que reportó Juanfri como «no llama al modelo». Ahora va.
        for turn in [
            "2 en vez de 4",
            "cinco en vez de seis",
            "pon 2 en vez de 4", // verb + subst + num → también
            "5 en lugar de 4",
            "5 en su lugar",
            "pon 2 en lugar de 4",
            "cámbialo por 5",
        ] {
            assert_eq!(
                classify(turn),
                CorrectiveDecision::NeedsLlm,
                "debe enrutarse al LLM: {turn:?}"
            );
        }
    }

    #[test]
    fn en_vez_de_sin_numeral_o_referencia_no_es_correccion() {
        // «X en vez de Y» sin número ni referencia a lo dicho no tiene de
        // dónde agarrarse para buscar `find`: el clasificador lo trata como
        // Plain. Así se filtran frases coloquiales («lo hice en vez de
        // descansar») que NO corrigen el último turno.
        //
        // «en vez de eso prefiero callar» SÍ encaja en REFERENCE («eso») y por
        // tanto va al modelo: el usuario está señalando algo del turno
        // anterior. Lo dejamos fuera del grupo Plain (es la conducta correcta,
        // no un fallo).
        for turn in ["lo hice en vez de descansar"] {
            assert_eq!(
                classify(turn),
                CorrectiveDecision::Plain,
                "debe ser Plain: {turn:?}"
            );
        }
        // Comprobación: la variante con referencia sí va al modelo.
        assert_eq!(
            classify("en vez de eso prefiero callar"),
            CorrectiveDecision::NeedsLlm,
            "con referencia, va al LLM"
        );
    }

    #[test]
    fn correccion_con_solo_negacion_y_numeral_llega_al_llm() {
        // Variantes coloquiales que ya estaban en los tests de antes pero que
        // conviene dejar fijadas con su forma exacta.
        for turn in ["no, es 3", "no es 4", "no, era 5"] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "{turn:?}");
        }
    }

    #[test]
    fn correccion_con_cambia_por_y_verb_llega_al_llm() {
        // «cambia X por Y» lleva verbo + marca de sustitución + números:
        // puntúa alto y va seguro al modelo.
        for turn in ["cambia 5 por 6", "cambia cuatro por cinco"] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "{turn:?}");
        }
    }

    #[test]
    fn por_aislado_no_es_marcador_de_sustitucion() {
        // «por» se usa demasiado en español («voy por el pan», «gracias por»,
        // «por la mañana»). La marca pide la construcción entera
        // «cambia por» / «en vez de» / etc.
        for turn in [
            "voy por el pan",
            "gracias por todo",
            "por la mañana",
            "pasado por agua",
        ] {
            assert_eq!(
                classify(turn),
                CorrectiveDecision::Plain,
                "no debe enrutar: {turn:?}"
            );
        }
    }

    #[test]
    fn marcador_sustitucion_sin_objetivo_es_dictado_normal() {
        // «en vez de» sin número ni referencia: no pasa la regla de los dos
        // niveles (no hay qué sustituir). Sirve de red para frases
        // coloquiales donde «en vez de» es narrativo y no corrige nada.
        for turn in ["lo hice en vez de descansar", "lo prefiero en vez de eso"] {
            // «lo prefiero en vez de eso» lleva referencia («eso»), así que en
            // realidad SÍ pasa la regla y va al modelo (por construcción). Lo
            // dejamos fuera del grupo Plain para no mentir sobre la conducta:
            // si el usuario tiene contexto y dice «eso» refiriéndose al turno
            // anterior, sí queremos mandarlo.
            if turn.contains("eso") || turn.contains("esto") {
                continue;
            }
            assert_eq!(classify(turn), CorrectiveDecision::Plain, "{turn:?}");
        }
    }

    #[test]
    fn umbral_cero_tambien_apaga_la_correccion_por_sustitucion() {
        // Aunque la señal «subst» pase la regla de los dos niveles, un
        // umbral de 0 sigue siendo el atajo «corrección IA apagada».
        for turn in ["2 en vez de 4", "5 en lugar de 4", "cambia 5 por 6"] {
            assert_eq!(
                classify_with(turn, 0),
                CorrectiveDecision::Plain,
                "umbral 0 nunca enruta: {turn:?}"
            );
        }
    }

    #[test]
    fn umbral_alto_puede_filtrar_la_sustitucion_minima() {
        // «2 en vez de 4» saca 3/5 (75) gracias al extra. Con umbral 80 no
        // pasa; con 70 sí. El test va en los dos sentidos para pillar
        // cambios de puntuación sin querer.
        let minimal = "2 en vez de 4";
        assert_eq!(
            classify_with(minimal, 50),
            CorrectiveDecision::NeedsLlm,
            "default pasa"
        );
        assert_eq!(
            classify_with(minimal, 70),
            CorrectiveDecision::NeedsLlm,
            "70 pasa (ratio 75)"
        );
        assert_eq!(
            classify_with(minimal, 80),
            CorrectiveDecision::Plain,
            "80 cae (ratio 75 < 80)"
        );
        // El caso del documento saca más (4 señales) y aguanta más.
        let canonico = "no no lo último está mal es 2 en vez de 4";
        assert_eq!(
            classify_with(canonico, 80),
            CorrectiveDecision::NeedsLlm,
            "canónico aguanta 80"
        );
    }

    // ── Umbral del clasificador (punto 2): regula la sensibilidad ───────────

    #[test]
    fn clasificador_default_thresholds_son_consistentes() {
        // classify(turn) == classify_with(turn, 50) por contrato: el valor
        // por defecto deja la conducta de antes (todo lo que pasa la regla
        // de los dos niveles va al modelo).
        for turn in [
            "no no lo último está mal es 2 en vez de 4",
            "esto no, era lo otro",
            "no era 4, era 2",
            "la reunión es el jueves", // Plain
            "no me gusta el café",     // Plain
        ] {
            assert_eq!(classify(turn), classify_with(turn, 50), "{turn:?}");
        }
    }

    #[test]
    fn umbral_cero_apaga_la_correccion() {
        // Punto 2: umbral 0 = clasificador apagado (atajo igual que quitar la corrección).
        for turn in [
            "no no lo último está mal es 2 en vez de 4",
            "esto no, era lo otro",
            "no era 4, era 2",
            "borra la última frase",
        ] {
            assert_eq!(
                classify_with(turn, 0),
                CorrectiveDecision::Plain,
                "umbral 0 nunca enruta: {turn:?}"
            );
        }
    }

    #[test]
    fn umbral_alto_filtra_los_casos_de_2_senales() {
        // Con umbral > 50 (3/5=75, 4/5=90) los casos de solo 2 señales (50) se
        // quedan fuera. Subir el umbral = más prudente (mejor no pasarse,
        // punto 2).
        let ambiguo = "no era 4, era 2"; // neg + num = 2/5 → ratio 50
        assert_eq!(classify(ambiguo), CorrectiveDecision::NeedsLlm);
        // Con umbral 60, 50 < 60 → NO se manda.
        assert_eq!(classify_with(ambiguo, 60), CorrectiveDecision::Plain);
        // Con umbral 80 (muy prudente) tampoco.
        assert_eq!(classify_with(ambiguo, 80), CorrectiveDecision::Plain);
        // El caso del documento (4/5 → 90) sigue pasando con 80: solo un umbral
        // muy alto (≥ 95) lo filtra. Antes (sin la marca de sustitución) era
        // 3/4 → 75 → se quedaba fuera con 80. Ahora el extra del subst lo hace
        // más resistente, siguiendo la lógica del punto 2 (mejor no pasarse:
        // si el turno huele a varias señales, nos quedamos con la corrección).
        let canonico = "no no lo último está mal es 2 en vez de 4";
        assert_eq!(classify_with(canonico, 60), CorrectiveDecision::NeedsLlm);
        assert_eq!(classify_with(canonico, 80), CorrectiveDecision::NeedsLlm);
        // Solo el umbral máximo (≥ 95) lo descarta.
        assert_eq!(classify_with(canonico, 95), CorrectiveDecision::Plain);
    }

    #[test]
    fn umbral_bajo_no_relaja_la_regla_de_los_dos_niveles() {
        // Aunque el umbral sea bajo, sigue valiendo la regla base: sin
        // (negación|verbo) o sin (número|referencia) NO se manda (punto 2:
        // «lo más seguro es tratar el turno como dictado normal»).
        for turn in [
            "no me gusta el café",
            "tengo dos reuniones",
            "cambia el canal",
        ] {
            for threshold in [10, 25, 50] {
                assert_eq!(
                    classify_with(turn, threshold),
                    CorrectiveDecision::Plain,
                    "debe seguir siendo Plain: {turn:?} @ threshold={threshold}"
                );
            }
        }
    }

    #[test]
    fn clasificador_default_es_el_50_que_preserva_la_conducta_previa() {
        // El contrato: classify(t) == classify_with(t, 50). Lo comprobamos con
        // una mezcla realista: el caso del documento y el «borra la última
        // frase» que la v1 ya mandaba.
        for turn in [
            "no no lo último está mal es 2 en vez de 4",
            "borra la última frase",
            "no era 4, era 2",
            "sustituye eso",
        ] {
            assert_eq!(classify(turn), CorrectiveDecision::NeedsLlm, "{turn:?}");
            assert_eq!(
                classify_with(turn, 50),
                CorrectiveDecision::NeedsLlm,
                "{turn:?}"
            );
        }
    }

    // ── apply_diff: sustitución correcta (≥5) ───────────────────────────────

    #[test]
    fn apply_diff_sustituye_numerales() {
        let diff = Diff {
            find: "4".into(),
            replace: "2".into(),
        };
        assert_eq!(
            apply_diff("la reunión es a las 4 en punto", &diff).as_deref(),
            Some("la reunión es a las 2 en punto")
        );
    }

    #[test]
    fn apply_diff_sustituye_palabras_numero() {
        let diff = Diff {
            find: "cuatro".into(),
            replace: "dos".into(),
        };
        assert_eq!(
            apply_diff("la reunión es a las cuatro en punto", &diff).as_deref(),
            Some("la reunión es a las dos en punto")
        );
    }

    #[test]
    fn apply_diff_sustituye_frase_corta() {
        let diff = Diff {
            find: "el informe semanal".into(),
            replace: "el informe del jueves".into(),
        };
        assert_eq!(
            apply_diff(
                "hay que revisar el informe semanal antes del viernes",
                &diff
            )
            .as_deref(),
            Some("hay que revisar el informe del jueves antes del viernes")
        );
    }

    #[test]
    fn apply_diff_quita_texto_con_replace_vacio() {
        let diff = Diff {
            find: " el jueves".into(),
            replace: String::new(),
        };
        assert_eq!(
            apply_diff("la reunión es el jueves a las 4", &diff).as_deref(),
            Some("la reunión es a las 4")
        );
    }

    #[test]
    fn apply_diff_sustituye_la_primera_ocurrencia() {
        // Punto 3.3: primera aparición; el prompt pide que find sea único para
        // los números («lo último» suele ser lo último, pero el contrato puede
        // pedir un trozo más largo que lo haga único).
        let diff = Diff {
            find: "4".into(),
            replace: "2".into(),
        };
        assert_eq!(
            apply_diff("toma 4, no 4", &diff).as_deref(),
            Some("toma 2, no 4")
        );
    }

    // ── apply_diff: cambio inválido → None, nunca toca el texto ─────────────

    #[test]
    fn diff_invalido_no_muta_texto() {
        let ausente = Diff {
            find: "no existe".into(),
            replace: "X".into(),
        };
        assert_eq!(apply_diff("la reunión es a las 4", &ausente), None);
        let vacio = Diff {
            find: String::new(),
            replace: "X".into(),
        };
        assert_eq!(apply_diff("la reunión es a las 4", &vacio), None);
        // El texto original queda intacto después de los intentos.
        assert_eq!(apply_diff("la reunión es a las 4", &ausente), None);
    }

    // ── replace_last_turn: última aparición del turno en el texto dictado ────

    #[test]
    fn replace_last_turn_actualiza_el_final_del_transcript() {
        let transcript = "Hola. La reunión es a las 4";
        assert_eq!(
            replace_last_turn(transcript, "La reunión es a las 4", "La reunión es a las 2")
                .as_deref(),
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

    // ── Caso de ejemplo del documento (punto 7) ─────────────────────────────

    #[test]
    fn caso_canonico_completo_cuatro_a_dos() {
        let transcript = "La reunión es a las 4";
        let turn = "La reunión es a las 4";
        let instruction = "no no lo último está mal es 2 en vez de 4";

        assert_eq!(classify(instruction), CorrectiveDecision::NeedsLlm);
        let after = apply_diff(
            turn,
            &Diff {
                find: "4".into(),
                replace: "2".into(),
            },
        )
        .expect("find debe ser subcadena literal");
        assert_eq!(after, "La reunión es a las 2");
        let fixed = replace_last_turn(transcript, turn, &after).expect("turno presente");
        assert_eq!(fixed, "La reunión es a las 2");
    }

    // ── Deshacer (punto 4.3): vuelve a `before` después de `after` ──────────

    #[test]
    fn undo_restaura_el_estado_anterior() {
        let transcript = "La reunión es a las 4";
        let turn = "La reunión es a las 4";
        let after = apply_diff(
            turn,
            &Diff {
                find: "4".into(),
                replace: "2".into(),
            },
        )
        .unwrap();
        let fixed = replace_last_turn(transcript, turn, &after).unwrap();

        // Deshacer = cambiar el turno corregido por el original.
        let undone = replace_last_turn(&fixed, &after, turn).unwrap();
        assert_eq!(undone, transcript);
    }

    #[test]
    fn apply_diff_preserva_el_resto_del_texto() {
        // Solo cambia el trozo en cuestión; ni el principio ni el final se
        // tocan (importante para deshacer un solo paso en app.rs).
        let diff = Diff {
            find: "4".into(),
            replace: "2".into(),
        };
        let after = apply_diff("la reunión es a las 4 en punto", &diff).unwrap();
        assert_eq!(after, "la reunión es a las 2 en punto");
        let restored = apply_diff(
            &after,
            &Diff {
                find: "2".into(),
                replace: "4".into(),
            },
        )
        .unwrap();
        assert_eq!(restored, "la reunión es a las 4 en punto");
    }

    // ── Auto-corrección DENTRO del turno (t_78d9e1fa) ──────────────────────
    //
    // Modo principal desde el 10/09/2026. El usuario dicta y se corrige
    // sobre la marcha en el MISMO turno («que calor en murcia digo
    // cartagena»). El clasificador de un turno a otro NO lo ve porque no hay
    // referencia a lo dicho antes: la corrección apunta al trozo ANTERIOR del
    // propio turno. `classify_in_turn` lo reconoce por las marcas típicas de
    // meta-lengua hablada.

    #[test]
    fn caso_canonico_in_turn_murcia_a_cartagena() {
        // El caso que dio pie al task t_78d9e1fa, transcrito tal cual.
        assert!(classify_in_turn(
            "que calor que hace en murcia digo cartagena"
        ));
    }

    #[test]
    fn in_turn_detecta_marcadores_tipicos_de_auto_correccion() {
        for turn in [
            // «digo X»: la forma más natural de corregirse sobre la marcha.
            "el informe es de enero digo febrero",
            "voy a ir a madrid digo a barcelona",
            // «quiero decir X»: variante formal.
            "el plazo es mañana quiero decir el viernes",
            // «quise decir X»: en pasado.
            "nací en el 80 quise decir en el 85",
            // «mejor dicho X».
            "la reunión es en sala 3 mejor dicho sala 5",
            // «mejor X»: esta NO la cubrimos dentro del turno porque es
            // demasiado ambigua («mejor libro», «mejórate», «el mejor de
            // todos»). El modelo la maneja por el prompt si aparece con un
            // sustantivo detrás, pero el clasificador no la usa como señal
            // porque da demasiados falsos positivos. Si algún día se quiere
            // cubrir, añadir una marca clara tipo «mejor: X» o «es mejor X»
            // a la regla.
            // «X en vez de Y» dentro del turno (sin referencia a lo anterior).
            "el presupuesto es 1000 en vez de 500",
            // «dime X» coloquial.
            "nos vemos el lunes dime el martes",
            // «diga X» / «diga: X» (variante, por ejemplo el eco de una instrucción).
            "el password es 1234 diga 5678",
        ] {
            assert!(
                classify_in_turn(turn),
                "debe detectarse como auto-corrección in-turn: {turn:?}"
            );
        }
    }

    #[test]
    fn in_turn_no_detecta_dictado_normal_sin_meta_lengua() {
        // Turnos del día a día sin marcas de corrección: SIEMPRE Plain.
        // Estos son los falsos positivos que hay que blindar.
        for turn in [
            "la reunión es el jueves a las cuatro",
            "tengo que llamar al médico mañana",
            "digo que sí a todo", // «digo» aquí NO es corrección (no corrige nada)
            "el niño dice que quiere ir al parque",
            "quiero decir que sí es una opción",
            "mejórate pronto",
            "el mejor libro que he leído",
            "no me molesta", // «no» aislado sin «sino»
            "no voy a ir",
            "hoy hace calor en murcia", // sin marcador correctivo
            "en lugar de ir al cine prefiero cenar", // «en lugar de» coloquial sin meta-corrección
        ] {
            assert!(
                !classify_in_turn(turn),
                "NO debe detectarse como corrección in-turn: {turn:?}"
            );
        }
    }

    #[test]
    fn in_turn_respeta_longitud_minima_y_contenido_anterior() {
        // Los turnos demasiado cortos no son corrección (un «digo sí» es
        // respuesta, no rectificación). El clasificador pide ≥ 6 caracteres y
        // ≥ 3 antes de la marca.
        assert!(!classify_in_turn("digo 5"), "demasiado corto");
        assert!(!classify_in_turn("no, 5"), "sin 'antes' suficiente");
        assert!(!classify_in_turn("si digo 5"), "solo 2 chars antes");
    }

    #[test]
    fn in_turn_detecta_diga_idiomático_para_que_el_llm_decida() {
        // «que diga lo que quiera» lleva «diga», que es una marca. Antes hay 3
        // caracteres («que»), después 12 («lo que quiera»): pasa los filtros.
        // El clasificador lo marca como candidato a reescribir; el modelo (no
        // el clasificador) decide si de verdad hay corrección. Si no la hay,
        // devuelve el texto sin cambios: el coste es una llamada al modelo por
        // cada «diga» del habla diaria. Es un precio aceptable: mejor una
        // llamada de más que perder la autocorrección legítima «el informe es
        // de enero digo febrero». El clasificador debe ser PERMISIVO al
        // detectar; el modelo tiene la última palabra sobre si hay que
        // reescribir.
        assert!(classify_in_turn("que diga lo que quiera"));
    }

    #[test]
    fn in_turn_exige_contenido_despues_del_marcador() {
        // «digo» / «mejor» sin nada después NO es corrección (el modelo no
        // tendría con qué sustituir).
        assert!(!classify_in_turn("el informe es de enero digo"));
        assert!(!classify_in_turn("el plazo es mañana mejor"));
        assert!(!classify_in_turn("mejor dicho "));
    }

    #[test]
    fn in_turn_disabled_desactiva_la_deteccion() {
        // El interruptor del modo dentro del turno: si enabled=false, siempre
        // devuelve false (se va al camino de un turno a otro / Plain). Así se
        // puede apagar este modo en el futuro sin tocar el clasificador de un
        // turno a otro.
        assert!(!classify_in_turn_with(
            "que calor en murcia digo cartagena",
            false
        ));
    }

    #[test]
    fn in_turn_no_se_solapa_con_cross_turn_en_casos_conocidos() {
        // El clasificador dentro del turno pilla el caso de Juanfri (sin
        // referencia previa) que el de un turno a otro NO pilla. Y un caso puro
        // de un turno a otro (sin «digo» ni «quiero decir», solo referencia a
        // lo dicho) NO lo pilla el de dentro del turno.
        //
        // Caso mixto: «no no lo último está mal es 2 en vez de 4» lo pillan LOS
        // DOS, el de un turno a otro (referencia + subst) y el de dentro del
        // turno («en vez de» es una marca de dentro). En `app.rs` el orden de
        // decisión es primero el de dentro del turno (t_78d9e1fa), así que ese
        // turno entra por el camino de reescritura. Aquí solo dejamos apuntado
        // el solapamiento: las dos detecciones valen.
        let in_turn_only = "que calor en murcia digo cartagena";
        let cross_turn_only = "no no lo último está mal es 4";
        let both = "no no lo último está mal es 2 en vez de 4";

        assert!(classify_in_turn(in_turn_only));
        assert_eq!(classify(in_turn_only), CorrectiveDecision::Plain);

        assert!(!classify_in_turn(cross_turn_only));
        assert_eq!(classify(cross_turn_only), CorrectiveDecision::NeedsLlm);

        // Solapamiento esperado: los dos clasificadores lo ven, y está bien.
        assert!(classify_in_turn(both));
        assert_eq!(classify(both), CorrectiveDecision::NeedsLlm);
    }

    #[test]
    fn in_turn_murcia_a_cartagena_es_la_redaccion_correcta() {
        // Documento entero del task t_78d9e1fa, no solo el clasificador:
        // comprueba que el FLUJO completo (dentro del turno → reescritura con
        // el modelo → texto limpio) da lo que se espera. La parte del modelo se
        // simula devolviendo directamente el texto corregido; el contrato es
        // que `redact()` quita espacios sobrantes y devuelve exactamente ese
        // texto.
        //
        // El test de verdad de `redact` está en src/llm.rs (quita espacios y
        // usa el texto de entrada si no hay nada). Aquí se prueba la lógica de
        // coordinación.
        let raw_turn = "que calor que hace en murcia digo cartagena";
        assert!(classify_in_turn(raw_turn));
        // Lo que el modelo debería devolver (contrato del prompt de reescritura):
        let expected_clean = "que calor que hace en cartagena";
        assert_ne!(
            raw_turn, expected_clean,
            "el input debe llevar meta-lengua; el output NO"
        );
        assert!(
            !expected_clean.contains("digo"),
            "la meta-lengua debe desaparecer"
        );
    }

    // ── contains_in_turn_marker / strip_in_turn_meta (t_78d9e1fa) ──────────
    //
    // Ayudantes que usa `app.rs` como red de seguridad después del modelo:
    // si la respuesta del corrector todavía lleva meta-lengua, no la
    // escribimos (sería peor que el texto de entrada).

    #[test]
    fn contains_in_turn_marker_detecta_meta_lengua() {
        // Caso típico: el modelo «no hizo nada» y dejó la marca.
        assert!(contains_in_turn_marker(
            "que calor en murcia digo cartagena"
        ));
        assert!(contains_in_turn_marker("voy a casa quiero decir al cine"));
        assert!(contains_in_turn_marker("el plazo es hoy mejor dicho mañana"));
        assert!(contains_in_turn_marker("voy a las 3 en vez de a las 4"));
    }

    #[test]
    fn contains_in_turn_marker_no_falsa_alarma_en_texto_limpio() {
        // Si el modelo aplicó bien la corrección, NO debe quedar ninguna marca
        // en el resultado. La función tiene que distinguirlo sin dudas.
        for clean in [
            "que calor en cartagena",
            "la reunión es a las 4",
            "voy al cine esta noche",
            "el plazo es mañana", // ya corregido
        ] {
            assert!(
                !contains_in_turn_marker(clean),
                "texto limpio no debe disparar el detector: {clean:?}"
            );
        }
    }

    #[test]
    fn contains_in_turn_marker_es_mas_laxo_que_classify_in_turn() {
        // El detector solo mira que la marca esté, sin pedir contenido antes ni
        // después. Así cubre el caso límite en el que el modelo deja solo el
        // trozo de meta-lengua suelto. `classify_in_turn` los rechazaría por su
        // filtro mínimo; `contains_in_turn_marker` los ve como sobrante.
        assert!(!classify_in_turn("digo"));
        assert!(contains_in_turn_marker("digo"));
    }

    #[test]
    fn strip_in_turn_meta_devuelve_el_antes_con_elipsis() {
        // Para el feedback de UI: «Auto-corregido IA: «X…» → «Y»».
        let original = "que calor en murcia digo cartagena";
        let stripped = strip_in_turn_meta(original);
        assert!(
            stripped.starts_with("que calor en murcia"),
            "debe quedarse con el fragmento anterior: {stripped:?}"
        );
        assert!(
            stripped.ends_with("…"),
            "marcador visual de texto recortado"
        );
    }

    #[test]
    fn strip_in_turn_meta_devuelve_el_input_si_no_hay_marcador() {
        // Sin marca, el ayudante no hace nada: devuelve el texto tal cual (no
        // hay nada que sacar).
        let normal = "la reunión es a las 4";
        assert_eq!(strip_in_turn_meta(normal), normal);
    }
}
