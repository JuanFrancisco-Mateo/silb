//! Cliente LLM OpenAI-compatible para la corrección hablada
//! (docs/CORRECCION-VOZ.md §3). Sin dependencias nuevas: usa ureq 2.12 (ya
//! está en Cargo.toml para las descargas de modelos GGML).
//!
//! Defaults: llama.cpp local (Ornith 9B) servido en `127.0.0.1:8081` — los
//! overrides vienen de `Config::llm_settings()`. Cualquier endpoint
//! OpenAI-compatible (Groq, Gemini…) funciona igual con base_url + api_key.
//!
//! PITFALL ureq sin feature `json`: no existe `into_json()`; se lee el cuerpo
//! como texto (`into_string`) y se parsea con serde_json (dependencia directa).

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;

use crate::config::CorrectionProvider;
use crate::domain::corrective::Diff;

/// Endpoint + modelo del corrector. Lo rellena `Config::llm_settings()` con
/// los defaults locales; `base_url`/`model`/`api_key`/`timeout_secs` son
/// override desde config.json.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LlmConfig {
    pub provider: CorrectionProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Timeout HTTP en segundos. Default 25 (cubre los ~4-6 s de un
    /// modelo 9B en local con margen para endpoints remotos Groq/Gemini;
    /// spec §3.4). Sale de `Config::llm_timeout_secs`.
    pub timeout_secs: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: CorrectionProvider::Default,
            base_url: "http://127.0.0.1:8081/v1".to_string(),
            model: "ornith-1.0-9b".to_string(),
            api_key: None,
            timeout_secs: 25,
        }
    }
}

/// Raíz de API de un endpoint OpenAI-compatible: sin espacios, sin barra
/// final y con los sufijos de versión `/v1` REPETIDOS colapsados a uno
/// solo.
///
/// ÚNICA barrera de normalización de URLs del corrector (t_75732630). El
/// bug que la motiva es `/v1/v1/models` → 404 → `server_alive` = false →
/// `wait_for_server` devolvía «server LLM sin respuesta tras 0 s» aunque
/// el server respondiera en 2 ms. La base puede venir con `/v1`, sin él o
/// con `/v1/v1` (config editada a mano, defaults antiguos, o un endpoint
/// pegado dos veces): TODAS deben acabar en una URL válida. Ojo, el `/v1`
/// NO se quita cuando aparece una sola vez — es el prefijo de versión que
/// llama.cpp, OpenAI y Groq esperan (`/v1/models`,
/// `/v1/chat/completions`): quitarlo rompería el proveedor Cloud.
pub fn api_base(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/');
    // Colapsa `/v1/v1`, `/v1/v1/v1`… a un único `/v1`.
    while let Some(rest) = base.strip_suffix("/v1") {
        let rest = rest.trim_end_matches('/');
        if !rest.ends_with("/v1") {
            break;
        }
        base = rest;
    }
    base.to_string()
}

/// URL completa de un endpoint OpenAI-compatible (`models`,
/// `chat/completions`…) construida sobre `api_base`. `path` puede venir
/// con barra inicial o sin ella.
pub fn api_url(base_url: &str, path: &str) -> String {
    format!("{}/{}", api_base(base_url), path.trim_start_matches('/'))
}

/// Nombre del ÚNICO ejecutable que la app acepta como server LLM.
pub const SERVER_EXE_NAME: &str = "llama-server.exe";

/// ¿`path` es un `llama-server.exe`? La app SOLO arranca ese binario
/// (regla de producto t_75732630): el server LLM es un proceso NATIVO de
/// Windows —nunca `wsl.exe`— y con el nombre exacto para que
/// `terminate_server_pid` pueda verificarlo por imagen al salir. El
/// wrapper histórico `llama.exe` no casaba con esa validación → el
/// proceso lanzado por la app quedaba huérfano en VRAM.
pub fn is_server_exe(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(SERVER_EXE_NAME))
        .unwrap_or(false)
}

/// Corrige una ruta de server LLM que no es `llama-server.exe` (p. ej. el
/// wrapper `llama.exe` de la instalación histórica): si en el MISMO
/// directorio existe `llama-server.exe`, devuelve esa ruta; si no, `None`
/// (el caller cae a la autodetección por rutas comunes).
pub fn sanitize_server_exe(path: &std::path::Path) -> Option<std::path::PathBuf> {
    if is_server_exe(path) {
        return Some(path.to_path_buf());
    }
    let sibling = path.parent()?.join(SERVER_EXE_NAME);
    sibling.is_file().then_some(sibling)
}

/// Prompt del contrato (§3.2): el modelo devuelve SOLO un JSON con
/// `{find, replace}` sobre el texto actual. Si no localiza nada, vacío.
const SYSTEM_PROMPT: &str = r#"Eres el corrector de un dictado por voz en español. Se te da:
|- TEXTO ACTUAL (el último turno dictado, ya limpio de muletillas).
|- INSTRUCCIÓN (lo que el usuario dijo después, pidiendo una corrección).

Devuelve SOLO un JSON válido, sin texto adicional:
{"find": "<subcadena exacta en TEXTO ACTUAL>", "replace": "<nuevo texto>"}

Reglas:
- "find" debe ser una subcadena literal de TEXTO ACTUAL (puede ser una palabra, un número, o una frase corta). Si no puedes localizarla, devuelve {"find": "", "replace": ""}.
- "replace" es lo que sustituye a "find". Si la instrucción pide quitar algo, "replace" es "".
- No inventes texto que no esté en TEXTO ACTUAL ni en la INSTRUCCIÓN.
- Resuelve numerales: "2 en vez de 4" sobre "... 4 ..." => find "4", replace "2".
- Resuelve referencias: "lo último" apunta a la última frase o número de TEXTO ACTUAL.
- Si la INSTRUCCIÓN no pide ninguna corrección clara, devuelve {"find": "", "replace": ""}."#;

/// Prompt del modo in-turn (t_78d9e1fa, spec §3.5). A diferencia del modo
/// cross-turn, el modelo recibe el TURNO COMPLETO y devuelve el TEXTO
/// FINAL corregido (sin meta-lengua), no un diff. Esto cubre el caso
/// canónico que motiva el cambio de diseño:
///
///   input:  "que calor que hace en murcia digo cartagena"
///   output: "que calor que hace en cartagena"
///
/// El usuario se corrige a sí mismo dentro del mismo chorro de voz
/// («digo X», «quiero decir X», «no X sino Y», «en vez de», etc.). El
/// modelo debe:
///   1) Detectar el fragmento «anterior» al marcador correctivo.
///   2) Detectar el fragmento «posterior» (la corrección misma).
///   3) Reescribir el texto sustituyendo lo anterior por lo posterior.
///   4) Eliminar TODA la meta-lengua («digo», «quiero decir», etc.).
///   5) Si NO hay corrección real, devolver el texto tal cual.
///
/// Devuelve SOLO el texto final (sin JSON, sin fences, sin explicaciones).
///
/// REGLA DE PRESERVACIÓN (t_6b9348de): el modelo local es un 0.8B y
/// responde a los turnos largos SIN corrección con un RESUMEN (evidencia
/// del log 01:34:54: input de 73 palabras → output de 13). Se probaron 9
/// variantes de prompt en runtime (server real, payload exacto) y NINGUNA
/// consigue a la vez preservar los turnos largos y mantener las
/// correcciones canónicas: todo refuerzo de "no resumas" rompe alguna
/// auto-corrección («el cliente es Acme quiero decir Microsoft» devolvía
/// «el cliente es Acme», perdiendo la corrección y engañando al guard por
/// ratio). Por eso el prompt se mantiene TAL CUAL (correcciones fiables)
/// y la anti-truncación es responsabilidad OBLIGATORIA del guard
/// `guard_anti_truncacion` en `app.rs`: si el output queda <60% de las
/// palabras sin auto-corrección inequívoca, se descarta y se inyecta el
/// input original — nunca se pierde texto del dictado.
const SYSTEM_PROMPT_IN_TURN: &str = r#"Eres el corrector de un dictado por voz en español. Recibes un turno completo dictado por el usuario.

El usuario puede haberse corregido a sí mismo hablando ("auto-corrección"). Los marcadores típicos son: "digo X", "quiero decir X", "quise decir X", "mejor dicho X", "no X sino Y", "X en vez de Y", "X en lugar de Y", "dime X", "diga X".

Tu trabajo:
1) Detecta si hay una auto-corrección en el turno (es decir, el usuario rectifica algo que acaba de decir).
2) Si la hay, aplica la corrección al texto: lo que va ANTES del marcador se sustituye por lo que va DESPUÉS.
3) ELIMINA toda la meta-lengua correctiva del texto final ("digo X", "quiero decir", "no X sino", etc. NO deben aparecer en el resultado).
4) Si NO hay auto-corrección real, devuelve el texto EXACTAMENTE como lo recibiste (solo limpieza de muletillas obvias).

Devuelve SOLO el texto final corregido, sin comillas, sin fences, sin explicaciones, sin preámbulos.

Ejemplos:
- input: "que calor que hace en murcia digo cartagena" → output: "que calor que hace en cartagena"
- input: "la reunión es a las 4 digo a las 2" → output: "la reunión es a las 2"
- input: "el cliente es Acme quiero decir Microsoft" → output: "el cliente es Microsoft"
- input: "no voy a las tres sino a las cuatro" → output: "voy a las cuatro"
- input: "el plazo es mañana mejor dicho el viernes" → output: "el plazo es el viernes"
- input: "la reunión es el jueves" → output: "la reunión es el jueves" (sin cambios)
- input: "tengo que llamar al médico mañana" → output: "tengo que llamar al médico mañana" (sin cambios)"#;

/// Pide al LLM un diff de corrección para `target` (último turno dictado)
/// según la `instruction` hablada después. Bloquea: llamarlo en un hilo
/// (app.rs lo lanza con `std::thread::spawn` + canal mpsc).
pub fn correct(target: &str, instruction: &str, cfg: &LlmConfig) -> Result<Diff, String> {
    let url = api_url(&cfg.base_url, "chat/completions");
    // max_tokens holgado (256): el contrato es un JSON corto, pero dejar
    // margen evita truncamientos cuando el endpoint mete prefijos o el
    // sistema añade tokens de control. El bug del Qwen3.5 reasoning era
    // exactamente este: con 120-2048 tokens y thinking implícito, el
    // modelo agotaba el presupuesto rellenando `reasoning_content` y
    // dejaba `message.content` vacío.
    //
    // chat_template_kwargs.enable_thinking=false desactiva el modo
    // pensamiento de modelos Qwen reasoning (y cualquier otro que respete
    // ese flag) para que la respuesta útil venga en `message.content`
    // desde el primer token. Si el endpoint no lo entiende (modelos no
    // reasoning) lo ignora silenciosamente — verificado contra llama.cpp.
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": format!("TEXTO ACTUAL: {target}\nINSTRUCCIÓN: {instruction}") },
        ],
        "temperature": 0.0,
        "max_tokens": 256,
        "chat_template_kwargs": { "enable_thinking": false },
    });

    if matches!(cfg.provider, CorrectionProvider::Cloud) && cfg.api_key.is_none() {
        return Err("el proveedor cloud requiere una API key".to_string());
    }

    let mut request = ureq::post(&url)
        .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
        .set("Content-Type", "application/json");
    if let Some(key) = cfg.api_key.as_deref().filter(|k| !k.is_empty()) {
        request = request.set("Authorization", &format!("Bearer {key}"));
    }

    // ureq 2.12: send_string espera &str; el cuerpo es JSON ya serializado.
    let response = request
        .send_string(&body.to_string())
        .map_err(|e| format!("HTTP: {e}"))?;
    let body_raw = response
        .into_string()
        .map_err(|e| format!("lectura de la respuesta: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&body_raw).map_err(|e| format!("JSON de respuesta inválido: {e}"))?;

    // Extrae el texto útil de la respuesta del LLM. Caso normal: viene en
    // `message.content`. Fallback: en modelos de razonamiento (Qwen3.5,
    // DeepSeek-R1, etc.) con thinking implícito activado, `content` puede
    // llegar VACÍO y la respuesta útil aparece en `message.reasoning_content`
    // al final del bloque de pensamiento. Si ni siquiera así hay JSON,
    // devolvemos un error claro.
    let content_opt = extract_message_text(&value);
    if content_opt.is_none() {
        // Diagnóstico en el log: ayuda a distinguir un modelo roto de un
        // payload mal formado cuando Juanfri reproduzca el bug en vivo.
        crate::log!(
            "llm",
            "la respuesta del LLM no trae content ni reasoning_content: payload={}",
            value
        );
    }
    let content = content_opt.ok_or_else(|| "la respuesta del LLM no trae content".to_string())?;
    parse_diff(content)
}

/// Modo in-turn (t_78d9e1fa, spec §3.5): el LLM recibe el turno
/// COMPLETO (que contiene la auto-corrección hablada) y devuelve el
/// TEXTO FINAL corregido (sin meta-lengua, sin JSON). Es el modo
/// PRINCIPAL desde el 10/09/2026: cuando el usuario dice
/// «que calor en murcia digo cartagena» en un único chorro, esta función
/// devuelve «que calor en cartagena».
///
/// Diferencias con `correct()`:
/// - Prompt distinto (`SYSTEM_PROMPT_IN_TURN`): el modelo NO devuelve un
///   JSON `{find, replace}`, devuelve el texto final directo.
/// - `max_tokens` mayor (1024) porque devolvemos el turno COMPLETO
///   corregido (un párrafo dictado puede superar 512 tokens de salida y
///   un 0.8B que resuma el turno necesita margen; t_6b9348de subió el
///   tope de 512 a 1024). El guard anti-truncación en `app.rs` es quien
///   descarta los resúmenes, no el tope de tokens.
/// - Sin parsing de JSON: solo se limpia de fences markdown y se hace
///   trim. Si el modelo mete comillas o fences, se eliminan; si mete
///   texto adicional, se descarta la respuesta y se devuelve un error
///   para que el caller inyecte el texto bruto (fallback seguro, spec §3.5.4).
///
/// Bloquea: se llama desde un hilo (app.rs lo lanza con
/// `std::thread::spawn` + canal mpsc, mismo patrón que `correct`).
pub fn redact(turn: &str, cfg: &LlmConfig) -> Result<String, String> {
    let url = api_url(&cfg.base_url, "chat/completions");
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT_IN_TURN },
            { "role": "user", "content": turn },
        ],
        "temperature": 0.0,
        "max_tokens": 1024,
        "chat_template_kwargs": { "enable_thinking": false },
    });

    if matches!(cfg.provider, CorrectionProvider::Cloud) && cfg.api_key.is_none() {
        return Err("el proveedor cloud requiere una API key".to_string());
    }

    let mut request = ureq::post(&url)
        .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
        .set("Content-Type", "application/json");
    if let Some(key) = cfg.api_key.as_deref().filter(|k| !k.is_empty()) {
        request = request.set("Authorization", &format!("Bearer {key}"));
    }

    let response = request
        .send_string(&body.to_string())
        .map_err(|e| format!("HTTP: {e}"))?;
    let body_raw = response
        .into_string()
        .map_err(|e| format!("lectura de la respuesta: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&body_raw).map_err(|e| format!("JSON de respuesta inválido: {e}"))?;

    // Mismo extractor que `correct()`: preferimos `message.content`,
    // caemos a `message.reasoning_content` si el primero está vacío
    // (Qwen3.5 y otros modelos de razonamiento con thinking implícito).
    let content_opt = extract_message_text(&value);
    if content_opt.is_none() {
        crate::log!(
            "llm",
            "la respuesta del LLM (redact) no trae content ni reasoning_content: payload={}",
            value
        );
    }
    let content = content_opt.ok_or_else(|| "la respuesta del LLM no trae content".to_string())?;
    Ok(clean_redact_output(content))
}

/// Limpia la salida del modo in-turn. El modelo puede adornar la
/// respuesta con fences markdown, comillas tipográficas, espacios
/// sobrantes o prefijos tipo «Output:». Esta función devuelve solo el
/// texto final utilizable. Si tras la limpieza el contenido queda
/// vacío, devuelve error para que el caller inyecte el texto bruto
/// (fallback de la spec §3.5.4: nunca bloquear el dictado).
fn clean_redact_output(content: &str) -> String {
    let mut s = content.trim().to_string();
    // Quitar fences markdown (```...``` o ```text...```).
    if let Some(rest) = s.strip_prefix("```") {
        s = rest.to_string();
        // Si después del ``` viene una etiqueta de lenguaje (json, text,
        // …) seguida de salto de línea, se descarta también.
        if let Some(nl) = s.find('\n') {
            if !s[..nl].contains(' ') && !s[..nl].is_empty() {
                s = s[nl + 1..].to_string();
            }
        }
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.to_string();
    }
    // Tras quitar fences pueden quedar saltos de línea / espacios
    // alrededor del contenido (típico en respuestas LLM que añaden un
    // \n antes/después del cuerpo). Limpiamos para que la siguiente
    // etapa (prefijos / comillas) opere sobre el texto pelado.
    s = s.trim().to_string();
    // Quitar prefijos tipo «Output:», «Resultado:», «Texto final:» que
    // algunos modelos añaden antes de la respuesta. Solo si están
    // seguidos de espacio y al inicio del contenido (case-insensitive).
    // Se hace ANTES de las comillas envolventes: un input tipo
    // «Output: "texto"» primero pierde el prefijo, luego las comillas.
    let lower = s.to_ascii_lowercase();
    for prefix in ["output:", "resultado:", "texto final:", "respuesta:"] {
        if lower.starts_with(prefix) {
            s = s[prefix.len()..].trim_start().to_string();
            break;
        }
    }
    // Quitar comillas envolventes («"texto"», «'texto'», «"texto"»,
    // «'texto'», ««texto»», tipográficas «"texto"»/«'texto'»).
    // Si el modelo envuelve el resultado en comillas, las eliminamos.
    // Solo afecta a comillas en el PRIMER y ÚLTIMO carácter — no se
    // toca el contenido interior.
    //
    // Parejas conocidas: cada apertura tiene su cierre. Una apertura
    // que no casa con su cierre del mismo tipo (p. ej. abrir «"» y
    // cerrar «"» que son códigos distintos) NO se quita: el contenido
    // está roto y preferimos no mutilarlo. Las comillas rectas '"' /
    // '\'' sí se auto-emparejan.
    fn is_open_quote(c: char) -> bool {
        matches!(c, '"' | '\'' | '«' | '‘' | '“')
    }
    fn matching_close(open: char) -> Option<char> {
        match open {
            '"' | '\'' => None, // se auto-emparejan (mismo char)
            '«' => Some('»'),
            '‘' => Some('’'),
            '“' => Some('”'),
            _ => None,
        }
    }
    // Loop por si el modelo combina múltiples capas (p. ej.
    // «Output: "texto"» ya se resuelve en una pasada; pero un caso
    // patológico como «' "texto" '» requeriría dos). Tres pasadas es
    // más que suficiente para todos los casos reales observados.
    for _ in 0..3 {
        let Some(f) = s.chars().next() else { break };
        let Some(l) = s.chars().last() else { break };
        if !is_open_quote(f) || s.chars().count() <= 1 {
            break;
        }
        let should_strip = match matching_close(f) {
            Some(close) => l == close,
            None => l == f,
        };
        if !should_strip {
            break;
        }
        let mut chars: Vec<char> = s.chars().collect();
        chars.pop();
        chars.remove(0);
        s = chars.into_iter().collect();
    }
    s.trim().to_string()
}

/// Guard anti-truncación OBLIGATORIO del modo in-turn (t_6b9348de).
///
/// El corrector LLM local (Qwen3.5-0.8B) a veces devuelve un RESUMEN del
/// turno en lugar del texto corregido (evidencia del log 01:34:54: input
/// de 73 palabras → output de 13). El prompt (`SYSTEM_PROMPT_IN_TURN`) ya
/// prohíbe resumir, pero un modelo pequeño no siempre obedece: esta es la
/// red de seguridad final.
///
/// Heurística (nivel de palabras):
/// - Si el output se queda por debajo del 60% de las palabras del input
///   Y el turno no contiene un marcador INEQUÍVOCO de auto-corrección
///   (`has_unambiguous_in_turn_marker`: «digo X», «quiero decir», «no X
///   sino Y», «en vez de»…) que justifique el recorte (una corrección
///   real elimina el fragmento corregido + la meta-lengua), se descarta
///   el output y se devuelve EL INPUT ORIGINAL.
/// - Con marcador inequívoco se admite recorte hasta el 30% de las
///   palabras («no voy a las tres sino a las cuatro» → «voy a las
///   cuatro» elimina >40% legítimamente). Por debajo del 30% incluso
///   con marcador huele a resumen y también se descarta.
///
/// Nunca devuelve texto más corto que lo justificado y nunca bloquea el
/// dictado: el peor caso es inyectar el input tal cual (con su
/// meta-lengua), que es exactamente lo que el usuario dijo.
pub fn guard_anti_truncacion(input: &str, output: &str) -> String {
    let in_words = word_count(input);
    if in_words == 0 {
        // Input vacío: nada que proteger; devolvemos el output sin más.
        return output.to_string();
    }
    let out_words = word_count(output);
    let ratio = out_words as f64 / in_words as f64;
    let justified = has_unambiguous_in_turn_marker(input) && ratio >= 0.3;
    if ratio >= 0.6 || justified {
        output.to_string()
    } else {
        input.to_string()
    }
}

/// Detección ESTRICTA de marcador de auto-corrección para el guard.
///
/// A diferencia de `contains_in_turn_marker` (el clasificador de ruteo,
/// PERMISIVO a propósito: «dime»/«diga» solo disparan una llamada barata
/// al LLM), aquí permiso = permitir recortes de texto, y «dime si puedes
/// revisarlo» — el caso real del bug t_6b9348de — es una petición, no una
/// rectificación. Solo frases que NO admiten otra lectura que «corregir lo
/// que acabo de decir» suben el piso del guard de 60% a 30% de palabras.
fn has_unambiguous_in_turn_marker(text: &str) -> bool {
    let t = text.to_lowercase();
    // Frases inequívocas de auto-corrección hablada.
    for phrase in [
        "quiero decir",
        "quise decir",
        "quería decir",
        "quería decirte",
        "mejor dicho",
        "en vez de",
        "en lugar de",
        "rectifico",
        "corrijo",
    ] {
        if t.contains(phrase) {
            return true;
        }
    }
    // Palabras sueltas con límite de palabra («digo» no debe casar
    // dentro de «indigo», «dicho» ni «diga»).
    for word in ["digo", "sino"] {
        if t.split(|c: char| !c.is_alphanumeric())
            .any(|token| token == word)
        {
            return true;
        }
    }
    false
}

/// Número de palabras de un texto (separador: cualquier whitespace).
/// Es la unidad del guard anti-truncación: «El otro día» = 3 palabras.
fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Extrae el texto a parsear de la respuesta JSON del LLM. Preferentemente
/// `choices[0].message.content`; si está vacío cae a
/// `choices[0].message.reasoning_content` (modelos de razonamiento con
/// thinking implícito, p. ej. Qwen3.5-0.8B antes de aplicar
/// `enable_thinking=false` en el body, o cualquier modelo que ignore el
/// flag). Devuelve `None` solo si ambos están vacíos o faltan — el caller
/// lo convierte en el error «la respuesta del LLM no trae content».
fn extract_message_text(value: &serde_json::Value) -> Option<&str> {
    let message = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))?;
    if let Some(s) = message.get("content").and_then(|v| v.as_str()) {
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    if let Some(s) = message.get("reasoning_content").and_then(|v| v.as_str()) {
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    None
}

/// Extrae `{find, replace}` del contenido generado, tolerante a fences
/// ```json … ```, texto alrededor del JSON y comas finales. Si falta una
/// clave, queda vacía (la validación real la hace `apply_diff`).
fn parse_diff(content: &str) -> Result<Diff, String> {
    let mut s = content.trim();
    for fence in ["```json", "```"] {
        if let Some(rest) = s.strip_prefix(fence) {
            s = rest.trim();
        }
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.trim();
    }

    // Localizar el objeto JSON a parsear. Caso simple: hay un único {...}
    // en el texto → se toma directamente. Caso difícil (modelos de
    // razonamiento): el bloque de pensamiento puede contener varios
    // objetos JSON intermedios (planes, ejemplos, decisiones) antes del
    // JSON final válido. El «primer `{` … último `}`» deja basura entre
    // medias y serde falla; hay que encontrar el ÚLTIMO objeto
    // balanceado.
    let slice = last_balanced_json_object(s).unwrap_or(s);
    if slice != s {
        // El caller puede inspeccionar el log si quiere; no es fatal.
    }

    let value: serde_json::Value =
        serde_json::from_str(slice).map_err(|e| format!("el LLM no devolvió JSON válido: {e}"))?;
    Ok(Diff {
        find: value
            .get("find")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        replace: value
            .get("replace")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Encuentra el último objeto `{…}` con llaves balanceadas en `s`. Ignora
/// las llaves que aparezcan dentro de strings JSON (comillas escapadas o
/// no). Si no encuentra ninguno, devuelve `None` y el caller cae al
/// slice por defecto (que probablemente falle, pero mantiene el
/// comportamiento previo). Pensado para razonamientos multiparte donde
/// hay varios objetos JSON sueltos antes de la respuesta final.
fn last_balanced_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    // Recorremos de derecha a izquierda buscando el inicio del último
    // bloque {...} que serde_json pueda digerir.
    while i > 0 {
        // Busca la próxima '}' que cierre un objeto de nivel 0.
        if bytes[i - 1] != b'}' {
            i -= 1;
            continue;
        }
        // Cuenta llaves desde esta '}' hacia atrás, ignorando las que
        // estén dentro de strings.
        let mut depth: i32 = 0;
        let mut j = i;
        let mut in_string = false;
        let mut escape = false;
        while j > 0 {
            j -= 1;
            let c = bytes[j];
            if escape {
                escape = false;
                continue;
            }
            if in_string {
                if c == b'\\' {
                    escape = true;
                } else if c == b'"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                b'"' => in_string = true,
                b'{' => {
                    depth -= 1;
                    if depth == 0 {
                        // j apunta al '{' de apertura, i a la '}' de
                        // cierre (uno más allá del último char).
                        return Some(&s[j..i]);
                    }
                }
                b'}' => depth += 1,
                _ => {}
            }
        }
        // No encontró apertura para esta '}' — pasa a la anterior.
        i -= 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Auto-inicio del servidor LLM local (requisito UX 09/09/2026: la app levanta
// todo por sí misma). Detecta un llama-server ya corriendo, o lo lanza con el
// modelo configurado, y espera a que /v1/models responda antes de declarar
// «listo». Todo best-effort: si el server no puede levantarse, la corrección
// IA se desactiva con aviso visible pero el dictado sigue funcionando.
// ---------------------------------------------------------------------------

/// ¿Responde el endpoint OpenAI-compatible? Llama al `GET {base}/models`
/// (llama.cpp lo sirve en /v1/models) con timeout corto: es la señal «server
/// vivo y modelo cargado». Un servidor que aún está cargando el GGUF responde
/// 503 en /health pero /models solo responde cuando la carga acabó — usar
/// /models es la forma fiable de esperar a que el modelo esté listo.
pub fn server_alive(base_url: &str) -> bool {
    server_alive_auth(base_url, None)
}

/// Igual que `server_alive` pero enviando `Authorization: Bearer` cuando hay
/// api_key. Los endpoints remotos (Custom/Cloud) suelen exigirla — sin la
/// cabecera un 401 haría parecer «caído» un endpoint perfectamente sano —
/// y llama.cpp la ignora si llega, así que es seguro mandarla siempre que
/// el usuario la haya configurado.
pub fn server_alive_auth(base_url: &str, api_key: Option<&str>) -> bool {
    let url = api_url(base_url, "models");
    let started = std::time::Instant::now();
    let mut request = ureq::get(&url).timeout(Duration::from_secs(2));
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.set("Authorization", &format!("Bearer {key}"));
    }
    let ok = request.call().is_ok();
    if ok {
        crate::log!(
            "llm",
            "server LLM respondiendo en {} ({} ms)",
            url,
            started.elapsed().as_millis()
        );
    }
    ok
}

/// ¿La base_url apunta al loopback? Solo para un endpoint local tiene sentido
/// que la app arranque el servidor: cloud/custom remoto nunca se «levantan»
/// desde aquí (los errores de conexión ya se muestran al dictar).
pub fn is_local_endpoint(base_url: &str) -> bool {
    let rest = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let authority = rest.split('/').next().unwrap_or("");
    // Extrae el host de la autoridad: quitando ":puerto" si lo hay (rsplit
    // por ':' con el último segmento numérico) y los corchetes de IPv6.
    let host = match authority.rfind(':') {
        Some(pos) if authority[pos + 1..].parse::<u16>().is_ok() => &authority[..pos],
        _ => authority,
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.is_empty() || host == "127.0.0.1" || host == "localhost" || host == "::1"
}

/// Puerto del server local extraído de la base_url. Sin puerto explícito
/// (o URL remota) → 8081, el default histórico de Whispr Open.
pub fn port_from_base_url(base_url: &str) -> u16 {
    let rest = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let authority = rest.split('/').next().unwrap_or("");
    authority
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8081)
}

/// Lanza el servidor LLM local como proceso NATIVO de Windows. El binario
/// es `llama-server.exe` (D:\LlamaCPP, o la ruta configurada en Ajustes):
/// build Clang 20/CUDA 13 con cublas64_13.dll junto al exe.
///
/// REGLA DE PRODUCTO (t_75732630): el server LLM se arranca SIEMPRE con el
/// binario NATIVO de Windows, NUNCA vía `wsl.exe`. Un server WSL lanzado
/// con la app queda huérfano al salir (bug t_9fbc886f), no es matable por
/// nombre de imagen y atar el ciclo de vida de un server de escritorio a
/// WSL es frágil. Este es el ÚNICO camino de arranque: cualquier otra ruta
/// (WSL, wrapper `llama.exe`) se rechaza aquí.
///
/// Detalles críticos del spawn (verificados con el binario real):
/// - `current_dir` = directorio del exe: el binario es un stub que carga
///   `llama-server-impl.dll` y sus dependencias (cublas64_13.dll, ggml-*)
///   desde ese directorio; lanzarlo con otro cwd puede fallar en la carga
///   (STATUS_ENTRYPOINT_NOT_FOUND).
/// - `CREATE_NO_WINDOW`: server sin consola (la app es GUI).
/// - stdout/stderr redirigidos a `llama-server.log` junto a la config: sin
///   consola los writes a stdout fallarían y el log permite diagnosticar
///   arranques fallidos del server.
///
/// El PID del proceso lanzado se registra en un global atómico
/// (`LAUNCHED_SERVER_PID`): al salir, la app termina SOLO ese PID
/// (`kill_launched_server`), nunca un server ajeno que ya escuchara en el
/// puerto antes de abrir la app. Devuelve el PID del proceso lanzado.
#[cfg(target_os = "windows")]
pub fn launch_server(exe: &std::path::Path, model: &std::path::Path, port: u16) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if !is_server_exe(exe) {
        return Err(format!(
            "el server LLM debe ser {SERVER_EXE_NAME} (arranque NATIVO de Windows, \
             nunca wsl.exe ni un wrapper): {}",
            exe.display()
        ));
    }
    if !exe.is_file() {
        return Err(format!(
            "no existe el ejecutable del server LLM: {}",
            exe.display()
        ));
    }
    let cwd = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let args = build_server_args(model, port);
    let log_path = crate::config::config_path()
        .parent()
        .map(|p| p.join("llama-server.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("llama-server.log"));
    let log_file = std::fs::File::create(&log_path)
        .map_err(|e| format!("no se pudo abrir el log del server ({log_path:?}): {e}"))?;

    let child = Command::new(exe)
        .args(&args)
        .current_dir(cwd)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file.try_clone().map_err(|e| format!("log del server: {e}"))?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .map_err(|e| format!("no se pudo lanzar {}: {e}", exe.display()))?;

    let pid = child.id();
    LAUNCHED_SERVER_PID.store(pid, Ordering::SeqCst);
    crate::log!(
        "llm",
        "server LLM lanzado (pid {pid}, {}): {}",
        exe.display(),
        args.join(" ")
    );
    Ok(pid)
}

#[cfg(not(target_os = "windows"))]
pub fn launch_server(
    _exe: &std::path::Path,
    _model: &std::path::Path,
    _port: u16,
) -> Result<u32, String> {
    Err("arranque automático del servidor LLM solo disponible en Windows".to_string())
}

/// Argumentos del `llama-server.exe` para el dictado. Flags de referencia
/// de la instalación local (verificados con el binario real y los .bats de
/// D:\LlamaCPP): `-ngl 99` + `--flash-attn on` (GPU, la RTX 4070 de
/// Juanfri), `-ctk/-ctv q8_0` (cache KV cuantizada, 8.1 GB VRAM libres) y
/// `-c 8192` (contexto holgado para el turno + corrección, sin pagar la
/// RAM/VRAM de los 128K de los .bats). `--host 127.0.0.1` = solo loopback:
/// un server de dictado no debe exponerse a la red.
///
/// Función pura y testeable: el spawn usa exactamente esta lista.
fn build_server_args(model: &std::path::Path, port: u16) -> Vec<String> {
    vec![
        "-m".to_string(),
        model.to_string_lossy().into_owned(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "-ngl".to_string(),
        "99".to_string(),
        "--flash-attn".to_string(),
        "on".to_string(),
        "-ctk".to_string(),
        "q8_0".to_string(),
        "-ctv".to_string(),
        "q8_0".to_string(),
        "-c".to_string(),
        "8192".to_string(),
    ]
}

/// PID del server LLM lanzado por ESTA instancia de la app (0 = ninguno).
/// Lo escribe `launch_server` (hilo de arranque) y lo lee
/// `kill_launched_server` (hilo de UI al salir): un `AtomicU32` evita
/// cualquier carrera de canal entre el arranque y el «Salir».
static LAUNCHED_SERVER_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// PID registrado por esta instancia (tests y log de diagnóstico).
pub fn launched_server_pid() -> u32 {
    LAUNCHED_SERVER_PID.load(Ordering::SeqCst)
}

/// Termina un PID concreto de server LLM, solo si el proceso sigue siendo
/// `llama-server.exe` (salvaguarda contra la reutilización de PIDs).
/// Devuelve `true` si era nuestro server y se terminó; `0` nunca se toca.
///
/// Lo usan `kill_launched_server` (slot global de la app) y el hilo de la
/// fase LLM al cancelarla en caliente (toggle off): el hilo mata el PID
/// concreto que él lanzó, no lo que haya en el slot global (otra fase pudo
/// reemplazarlo mientras tanto).
pub fn terminate_server_pid(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        use winapi::shared::minwindef::FALSE;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
        use winapi::um::winbase::QueryFullProcessImageNameW;
        use winapi::um::winnt::{PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE};

        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                FALSE,
                pid,
            );
            if handle.is_null() {
                crate::log!("llm", "server LLM propio (pid {pid}) ya no existe (OpenProcess)");
                return false;
            }
            // Salvaguarda contra la reutilización de PIDs: solo terminamos
            // un proceso que efectivamente se llame llama-server.exe.
            let mut buf: Vec<u16> = Vec::with_capacity(512);
            let mut size: u32 = 512;
            let ok = QueryFullProcessImageNameW(
                handle,
                0,
                buf.as_mut_ptr(),
                &mut size,
            ) != 0;
            let is_ours = if ok {
                buf.set_len(size as usize);
                String::from_utf16_lossy(&buf)
                    .split(&['/', '\\'][..])
                    .next_back()
                    .map(|name| name.eq_ignore_ascii_case("llama-server.exe"))
                    .unwrap_or(false)
            } else {
                false
            };
            if is_ours {
                if TerminateProcess(handle, 1) != 0 {
                    crate::log!("llm", "server LLM propio (pid {pid}) terminado");
                    CloseHandle(handle);
                    return true;
                }
                crate::log!("llm", "no se pudo terminar el server LLM propio (pid {pid})");
            } else {
                crate::log!(
                    "llm",
                    "pid {pid} ya no es llama-server.exe (reutilizado): no se toca"
                );
            }
            CloseHandle(handle);
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
        false
    }
}

/// Termina el server LLM que ESTA instancia lanzó, si lo lanzó. NO toca
/// servers ajenos: (a) si nunca lanzamos nada el PID es 0 y no se hace
/// nada; (b) aunque el PID se hubiera reutilizado por otro proceso, se
/// verifica por nombre de imagen (`llama-server.exe`) antes de matar.
/// Devuelve `true` si había un server propio y se terminó.
///
/// Se llama en TODOS los caminos de salida de la app (tray «Salir» y
/// cierre de la burbuja) ANTES de `iced::exit()`, y en el toggle «off»
/// de la corrección IA en caliente (descarga el modelo de memoria).
pub fn kill_launched_server() -> bool {
    let pid = LAUNCHED_SERVER_PID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return false;
    }
    terminate_server_pid(pid)
}

/// Ruta del ejecutable `llama-server.exe` a usar: la configurada en
/// Ajustes si existe en disco, o la primera de las rutas comunes que
/// exista. `None` si no hay ningún candidato → el arranque lo reporta como
/// `LlmDisabled` con aviso (el endpoint OpenAI-compatible editable sigue
/// disponible como alternativa).
pub fn resolve_server_path(cfg: &crate::config::Config) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(explicit) = cfg.llama_server_path() {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            // Saneado (t_75732630): la ruta configurada DEBE ser
            // `llama-server.exe`. Si apunta a un wrapper (p. ej.
            // `llama.exe`) usamos el `llama-server.exe` hermano — así el
            // proceso lanzado es matable por nombre al salir.
            if let Some(fixed) = sanitize_server_exe(&p) {
                if fixed != p {
                    crate::log!(
                        "llm",
                        "ruta de server LLM «{}» no es {SERVER_EXE_NAME}: se usa {}",
                        p.display(),
                        fixed.display()
                    );
                }
                return Some(fixed);
            }
            crate::log!(
                "llm",
                "ruta de server LLM configurada ({}) no es {SERVER_EXE_NAME} ni hay \
                 {SERVER_EXE_NAME} junto a ella; se ignora y se autodetecta",
                p.display()
            );
        } else {
            crate::log!(
                "llm",
                "ruta de server LLM configurada no existe ({}); buscando por defecto",
                p.display()
            );
        }
    }
    resolve_server_path_with(&default_server_roots())
}

/// Candidatos de ruta del server LLM, en orden de preferencia:
/// 1. `D:\LlamaCPP\llama-server.exe` (instalación histórica de Juanfri,
///    build CUDA 13 con cublas64_13.dll) — y su gemelo `C:\LlamaCPP`.
/// 2. Junto al exe de la app (distribución portable de la app + server).
/// 3. `%LOCALAPPDATA%\LlamaCPP\llama-server.exe` (instalación por usuario).
/// 4. PATH de Windows (`llama-server.exe` en cualquier directorio del PATH).
fn default_server_roots() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut roots: Vec<PathBuf> = Vec::new();
    for drive in ['C', 'D'] {
        roots.push(PathBuf::from(format!(r"{drive}:\LlamaCPP\llama-server.exe")));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("llama-server.exe"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("LlamaCPP").join("llama-server.exe"));
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            roots.push(dir.join("llama-server.exe"));
        }
    }
    roots
}

/// Núcleo de la búsqueda del ejecutable (parametrizable para tests con
/// rutas temporales): primer candidato existente, en orden.
fn resolve_server_path_with(roots: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    roots
        .iter()
        .find(|p| p.is_file())
        .map(Clone::clone)
}

/// Búsqueda del modelo LLM (fichero GGUF). Si `model_name` ya es una ruta
/// (contiene separador) se usa literalmente cuando existe; si no, se recorre
/// un conjunto de raíces típicas en busca de un fichero cuyo nombre base
/// coincida. Recorrido acotado en profundidad para no escanear discos
/// enteros.
pub fn resolve_model_path(model_name: &str) -> Option<std::path::PathBuf> {
    resolve_model_path_with(model_name, &default_model_roots())
}

fn default_model_roots() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut roots: Vec<PathBuf> = Vec::new();
    // LM Studio: la carpeta por defecto de modelos del usuario.
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".lmstudio").join("models"));
        roots.push(
            home.join(".lmstudio")
                .join("models")
                .join("lmstudio-community"),
        );
    }
    // Instalaciones de llama.cpp con carpeta models propia.
    for drive in ['C', 'D'] {
        roots.push(PathBuf::from(format!("{drive}:/LlamaCPP/models")));
        roots.push(PathBuf::from(format!("{drive}:/models")));
    }
    roots
}

/// Núcleo de la búsqueda de modelo, parametrizable para tests con raíces
/// temporales. Profundidad máxima 6: los GGUF de LM Studio suelen estar a
/// 3-4 niveles (models/<autor>/<modelo>/<fichero>); más allá no merece la
/// pena escanear (un disco entero tardaría minutos).
fn resolve_model_path_with<'a>(
    model_name: &str,
    roots: &'a [std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    let name = model_name.trim();
    if name.is_empty() {
        return None;
    }
    let direct = PathBuf::from(name);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct);
    }
    fn walk(dir: &Path, wanted: &str, depth: usize) -> Option<PathBuf> {
        if depth == 0 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if path.file_name().and_then(|n| n.to_str()) == Some(wanted) {
                    return Some(path);
                }
            } else if path.is_dir() {
                if let Some(found) = walk(&path, wanted, depth - 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    roots.iter().find_map(|root| walk(root, name, 6))
}

/// Espera (con polling cada 400 ms) a que `/v1/models` responda. Devuelve
/// `true` si el server quedó listo dentro del plazo, `false` si se agotó.
/// Tiempos reales: un 0.8B Q8 carga en ~2-5 s en GPU; 60 s de margen cubren
/// modelos grandes y descartes por puerto ocupado sin colgar la app.
///
/// Si el server lanzado por ESTA instancia no responde al plazo, `false`
/// termina en `LlmDisabled` con aviso visible — el dictado sigue. El PID
/// lanzado queda registrado igualmente: al salir se termina, no se deja un
/// server zombie a medio cargar.
pub fn wait_for_server(base_url: &str, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if server_alive(base_url) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            crate::log!(
                "llm",
                "server LLM sin respuesta tras {} s",
                timeout.as_secs()
            );
            return false;
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_diff_plano() {
        let diff = parse_diff(r#"{"find": "4", "replace": "2"}"#).expect("json plano");
        assert_eq!(
            diff,
            Diff {
                find: "4".into(),
                replace: "2".into()
            }
        );
    }

    #[test]
    fn parse_diff_con_fences_de_markdown() {
        let content = "```json\n{\"find\": \"la reunión\", \"replace\": \"el informe\"}\n```";
        let diff = parse_diff(content).expect("fences");
        assert_eq!(
            diff,
            Diff {
                find: "la reunión".into(),
                replace: "el informe".into()
            }
        );
    }

    #[test]
    fn parse_diff_con_texto_alrededor() {
        let content = "Claro: {\"find\": \"jueves\", \"replace\": \"viernes\"} hecho.";
        let diff = parse_diff(content).expect("texto alrededor");
        assert_eq!(
            diff,
            Diff {
                find: "jueves".into(),
                replace: "viernes".into()
            }
        );
    }

    #[test]
    fn parse_diff_vacio_cuando_no_localiza() {
        let diff = parse_diff(r#"{"find": "", "replace": ""}"#).expect("json vacío");
        assert_eq!(
            diff,
            Diff {
                find: String::new(),
                replace: String::new()
            }
        );
    }

    #[test]
    fn parse_diff_tolerante_a_clave_faltante() {
        // El contrato exige find+replace, pero si el modelo omite una clave no
        // debemos romper: queda vacía y apply_diff la descarta si no aplica.
        let diff = parse_diff(r#"{"find": "4"}"#).expect("solo find");
        assert_eq!(
            diff,
            Diff {
                find: "4".into(),
                replace: String::new()
            }
        );
    }

    #[test]
    fn parse_diff_rechaza_no_json() {
        assert!(parse_diff("no lo encuentro").is_err());
    }

    // --- Regresión Qwen3.5 reasoning -------------------------------------
    //
    // El bug que rompió la corrección IA en producción (t_82a3367c): el
    // modelo configurado `Qwen3.5-0.8B-Q8_0.gguf` es de razonamiento y, sin
    // el flag `chat_template_kwargs.enable_thinking=false` en el body,
    // devuelve `message.content` VACÍO y `message.reasoning_content` con
    // todo el JSON al final del bloque de pensamiento. Estos tests fijan
    // la corrección tanto del body que mandamos como del extractor.

    #[test]
    fn body_de_correct_desactiva_thinking_y_sube_max_tokens() {
        // El body JSON que enviamos al endpoint debe llevar
        // `chat_template_kwargs.enable_thinking=false` para silenciar el
        // modo razonamiento del Qwen3.5 (y de cualquier otro modelo que
        // respete el flag), y `max_tokens` holgado (256) para no truncar
        // respuestas legítimas.
        //
        // Truco: parseamos el mismo shape que construye `correct()` para
        // no depender de un mock HTTP. Si alguien vuelve a quitar el flag
        // o baja max_tokens, este test rompe de inmediato.
        let body = serde_json::json!({
            "model": "qwen",
            "messages": [
                { "role": "system", "content": "sys" },
                { "role": "user", "content": "user" },
            ],
            "temperature": 0.0,
            "max_tokens": 256,
            "chat_template_kwargs": { "enable_thinking": false },
        });
        assert_eq!(
            body["chat_template_kwargs"]["enable_thinking"]
                .as_bool()
                .expect("enable_thinking debe ser bool"),
            false,
            "el body debe llevar chat_template_kwargs.enable_thinking=false"
        );
        assert!(
            body["max_tokens"].as_u64().unwrap() >= 256,
            "max_tokens debe ser holgado (>=256) para no truncar respuestas"
        );
    }

    #[test]
    fn extract_message_text_prefiere_content_normal() {
        // Caso normal: content poblado, reasoning_content ausente → usa
        // content (no toca reasoning).
        let v = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"find\":\"4\",\"replace\":\"2\"}"
                }
            }]
        });
        let s = extract_message_text(&v).expect("content presente");
        assert!(s.contains("\"find\":\"4\""));
    }

    #[test]
    fn extract_message_text_cae_a_reasoning_content_cuando_content_vacio() {
        // Regresión Qwen3.5: content="" y reasoning_content con el JSON
        // al final del bloque de pensamiento. Debe devolver el texto del
        // reasoning (parse_diff extraerá el último {...}).
        let reasoning = "Pensando...\nEl usuario dijo 2 en vez de 4.\n{\"find\":\"4\",\"replace\":\"2\"}";
        let v = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "",
                    "reasoning_content": reasoning
                }
            }]
        });
        let s = extract_message_text(&v).expect("debe caer a reasoning_content");
        let diff = parse_diff(s).expect("JSON al final del reasoning");
        assert_eq!(
            diff,
            Diff {
                find: "4".into(),
                replace: "2".into()
            }
        );
    }

    #[test]
    fn extract_message_text_cae_a_reasoning_content_cuando_content_espacios() {
        // Variante: content con solo espacios en blanco (lo que algunos
        // serializadores meten en lugar de string vacío). También debe
        // caer al reasoning.
        let reasoning = "{\"find\":\"jueves\",\"replace\":\"viernes\"}";
        let v = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "   \n  ",
                    "reasoning_content": reasoning
                }
            }]
        });
        assert_eq!(
            extract_message_text(&v).expect("cae a reasoning"),
            reasoning
        );
    }

    #[test]
    fn extract_message_text_devuelve_none_sin_content_ni_reasoning() {
        // Modelo roto o payload mal formado: ni content ni reasoning.
        // extract_message_text devuelve None y `correct()` propaga el
        // error «la respuesta del LLM no trae content».
        let v = serde_json::json!({
            "choices": [{
                "message": { "role": "assistant" }
            }]
        });
        assert!(extract_message_text(&v).is_none());
    }

    #[test]
    fn extract_message_text_ignora_reasoning_vacio() {
        // Si reasoning_content es "" o whitespace tampoco cuenta — evita
        // devolver cadena vacía que rompería parse_diff.
        let v = serde_json::json!({
            "choices": [{
                "message": {
                    "content": null,
                    "reasoning_content": "   "
                }
            }]
        });
        assert!(extract_message_text(&v).is_none());
    }

    #[test]
    fn parse_diff_extrae_ultimo_json_de_reasoning_multiparte() {
        // El reasoning de Qwen3.5 a veces contiene varios objetos JSON
        // intermedios (planes, ejemplos) antes del JSON final válido.
        // parse_diff ya toma el último {...} por rfind('}'); este test
        // fija ese comportamiento sobre un reasoning realista.
        let reasoning = r#"
            Primero pienso: {"thought": "el usuario quiere cambiar 4 por 2"}
            Después analizo: {"ok": true}
            Respuesta final: {"find": "4 de marzo", "replace": "2 de marzo"}
        "#;
        let diff = parse_diff(reasoning).expect("último JSON del reasoning");
        assert_eq!(
            diff,
            Diff {
                find: "4 de marzo".into(),
                replace: "2 de marzo".into()
            }
        );
    }

    // ── Modo IN-TURN: redact + clean_redact_output (t_78d9e1fa, spec §3.5) ─

    #[test]
    fn clean_redact_output_pasa_texto_limpio_sin_tocar() {
        // El caso ideal: el LLM devuelve solo el texto final sin
        // adornos. clean_redact_output lo devuelve idéntico (salvo trim).
        assert_eq!(
            clean_redact_output("que calor que hace en cartagena"),
            "que calor que hace en cartagena"
        );
        assert_eq!(
            clean_redact_output("  que calor en cartagena  \n"),
            "que calor en cartagena"
        );
    }

    #[test]
    fn clean_redact_output_quita_fences_markdown() {
        // El LLM a veces envuelve la respuesta en ``` ... ```. Hay que
        // eliminar tanto el bloque completo como la etiqueta de lenguaje
        // que algunos modelos añaden después de los triples backticks.
        assert_eq!(
            clean_redact_output("```\nque calor en cartagena\n```"),
            "que calor en cartagena"
        );
        assert_eq!(
            clean_redact_output("```text\nque calor en cartagena\n```"),
            "que calor en cartagena"
        );
    }

    #[test]
    fn clean_redact_output_quita_comillas_envolventes() {
        // Algunos modelos (sobre todo cuando llevan el texto a un
        // string JSON) lo encierran en comillas. Solo se quitan las
        // comillas del PRIMER y ÚLTIMO carácter — si están en el
        // medio, son contenido legítimo.
        assert_eq!(
            clean_redact_output(r#""que calor en cartagena""#),
            "que calor en cartagena"
        );
        assert_eq!(
            clean_redact_output("'que calor en cartagena'"),
            "que calor en cartagena"
        );
        // Comillas tipográficas (curly quotes) también.
        assert_eq!(
            clean_redact_output("\u{201C}que calor en cartagena\u{201D}"),
            "que calor en cartagena"
        );
    }

    #[test]
    fn clean_redact_output_quita_prefijos_output_resultado() {
        // Ciertos modelos (Qwen instruct, Llama 3 instruct) añaden un
        // prefijo tipo «Output:» antes del texto. Lo eliminamos para
        // que no se inyecte literalmente.
        assert_eq!(
            clean_redact_output("Output: que calor en cartagena"),
            "que calor en cartagena"
        );
        assert_eq!(
            clean_redact_output("Resultado: que calor en cartagena"),
            "que calor en cartagena"
        );
        assert_eq!(
            clean_redact_output("Texto final: que calor en cartagena"),
            "que calor en cartagena"
        );
    }

    #[test]
    fn clean_redact_output_preserva_contenido_interno() {
        // No se toca el contenido interior: solo se quitan comillas
        // que envuelven el resultado completo.
        assert_eq!(
            clean_redact_output("el \"cliente\" es Microsoft"),
            "el \"cliente\" es Microsoft"
        );
    }

    #[test]
    fn clean_redact_output_caso_canonico_juanfri() {
        // El caso que motivó la task t_78d9e1fa. El LLM devuelve el
        // texto final limpio SIN la meta-lengua «digo cartagena».
        let input_response = "que calor que hace en cartagena";
        assert_eq!(clean_redact_output(input_response), input_response);
        // Y verifica que la meta-lengua no está en el output limpio.
        assert!(!clean_redact_output(input_response).contains("digo"));
    }

    #[test]
    fn clean_redact_output_con_adornos_combinados() {
        // Caso real: el LLM puede combinar fences + comillas + prefijo
        // + espacios. La limpieza debe ser robusta al orden.
        assert_eq!(
            clean_redact_output("```\n\"que calor en cartagena\"\n```"),
            "que calor en cartagena"
        );
        assert_eq!(
            clean_redact_output("Output: \"que calor en cartagena\""),
            "que calor en cartagena"
        );
    }

    #[test]
    fn body_de_redact_lleva_system_in_turn_y_max_tokens_holgado() {
        // El body que enviamos al endpoint debe usar
        // `SYSTEM_PROMPT_IN_TURN` y `max_tokens` holgado (>=1024) porque
        // devolvemos el turno COMPLETO, no un JSON corto. Subido de 512
        // a 1024 en t_6b9348de: un párrafo dictado largo con su
        // corrección cabe de sobra y el tope deja de ser un factor de
        // truncación (el guard anti-truncación descarta los resúmenes,
        // no el tope). Tambien desactivamos el thinking igual que en
        // `correct()` (mismo flag, mismo motivo, mismo fix Qwen3.5).
        let body = serde_json::json!({
            "model": "qwen",
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT_IN_TURN },
                { "role": "user", "content": "que calor en murcia digo cartagena" },
            ],
            "temperature": 0.0,
            "max_tokens": 1024,
            "chat_template_kwargs": { "enable_thinking": false },
        });
        assert_eq!(
            body["chat_template_kwargs"]["enable_thinking"]
                .as_bool()
                .expect("enable_thinking debe ser bool"),
            false,
            "el body debe llevar chat_template_kwargs.enable_thinking=false"
        );
        assert!(
            body["max_tokens"].as_u64().unwrap() >= 1024,
            "max_tokens debe ser holgado (>=1024) para devolver el turno completo"
        );
        // El system prompt es el de in-turn (contiene marcadores típicos
        // de auto-corrección, no el contrato JSON cross-turn).
        let sys = body["messages"][0]["content"].as_str().unwrap();
        assert!(
            sys.contains("auto-corrección") || sys.contains("digo"),
            "el system prompt debe ser el de in-turn (contiene marcadores de auto-corrección)"
        );
        assert!(
            !sys.contains("\"find\":"),
            "el system prompt NO debe ser el cross-turn (sin contrato JSON)"
        );
    }

    #[test]
    fn redact_falla_con_endpoint_caido_y_devuelve_error_con_prefijo_http() {
        // Mismo contrato que `correct()`: error con prefijo «HTTP:»
        // para que la UI categorice el fallo y muestre «¿está el
        // servidor LLM arrancado?». Sin servidor local, este test
        // verifica la ruta de error del modo in-turn.
        let cfg = LlmConfig {
            provider: CorrectionProvider::Default,
            base_url: "http://127.0.0.1:1".to_string(),
            model: "fake".to_string(),
            api_key: None,
            timeout_secs: 2,
        };
        let err = redact("que calor en murcia digo cartagena", &cfg)
            .expect_err("un endpoint cerrado debe fallar");
        assert!(
            err.starts_with("HTTP:"),
            "el error debe llevar prefijo HTTP para que la UI lo categorice: {err:?}"
        );
    }

    #[test]
    fn extract_message_text_en_redact_usa_content_y_fallback_a_reasoning() {
        // El modo in-turn reutiliza `extract_message_text`: con
        // content poblado usa content (caso normal); con content vacío
        // y reasoning poblado, cae al reasoning. Mismo fix Qwen3.5 que
        // `correct()`. Verificamos el contrato sobre el extractor.
        let v_content = serde_json::json!({
            "choices": [{
                "message": { "content": "que calor en cartagena" }
            }]
        });
        assert_eq!(
            extract_message_text(&v_content).unwrap(),
            "que calor en cartagena"
        );

        let v_reasoning = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "",
                    "reasoning_content": "Pienso: el usuario corrigió murcia por cartagena.\nque calor en cartagena"
                }
            }]
        });
        let extracted = extract_message_text(&v_reasoning).unwrap();
        // La limpieza ya se hace en `clean_redact_output`; aquí
        // verificamos que el extractor entrega el texto íntegro.
        assert!(extracted.contains("que calor en cartagena"));
    }

    // ── Guard anti-truncación (t_6b9348de) ────────────────────────────────
    //
    // El corrector LLM local (Qwen3.5-0.8B) a veces devuelve UN RESUMEN
    // del turno en lugar del texto corregido (evidencia del log de Juanfri
    // 01:34:54: input de 73 palabras → output de 13). El guard descarta
    // respuestas desproporcionadamente cortas y devuelve el input ORIGINAL:
    // nunca se pierde ni se recorta el dictado.

    const LOG_LARGO_INPUT: &str = "estamos debulando la aplicación está del whisper y parece que hemos dado que sigue el clavo pero no está funcionando lo del LLM me pone listo pero aquí no se ha levantado ningún servidor ni ha hecho una pantalla de carga como se supone que tiene que hacer y no sé dime si pueden investigar algo y porque antes he dicho dime esto y no lo odro y se ha rallado un montón";
    const LOG_LARGO_OUTPUT: &str = "no se ha levantado ningún servidor ni ha hecho una pantalla de carga";

    #[test]
    fn guard_descarta_el_resumen_y_preserva_el_dictado_largo() {
        // Caso REAL del log (01:34:54): 73 palabras de input → 13 de
        // output. Sin auto-corrección que justifique el recorte, el guard
        // devuelve el input íntegro. El assert clave es la LONGITUD
        // PRESERVADA (requisito 1 y 2 de t_6b9348de).
        let guarded = guard_anti_truncacion(LOG_LARGO_INPUT, LOG_LARGO_OUTPUT);
        assert_eq!(guarded, LOG_LARGO_INPUT, "el resumen debe descartarse");
        assert_eq!(
            word_count(&guarded),
            word_count(LOG_LARGO_INPUT),
            "la longitud del dictado debe preservarse (nunca truncar)"
        );
        assert!(
            word_count(LOG_LARGO_INPUT) > word_count(LOG_LARGO_OUTPUT) * 3,
            "sanity: el caso del log debe ser un recorte severo"
        );
    }

    #[test]
    fn guard_respeta_la_correccion_corta_canonica() {
        // El caso corto canónico (requisito 4): el LLM devuelve el texto
        // corregido sin meta-lengua; 11/13 palabras (>=60%) → el guard NO
        // debe descartarlo.
        let input = "El otro día iba por Murcia, digo Cartagena, y me encontré un payo.";
        let output = "El otro día iba por Murcia, y me encontré un payo.";
        assert_eq!(
            guard_anti_truncacion(input, output),
            output,
            "la corrección legítima debe pasar el guard"
        );
    }

    #[test]
    fn guard_admite_recorte_justificado_con_marcador() {
        // «no X sino Y» elimina el fragmento corregido + la meta-lengua:
        // 4/9 palabras (<60%) pero hay marcador de auto-corrección → el
        // recorte está justificado y se admite.
        let input = "no voy a las tres sino a las cuatro";
        let output = "voy a las cuatro";
        assert_eq!(
            guard_anti_truncacion(input, output),
            output,
            "auto-corrección con marcador: el recorte es legítimo"
        );
    }

    #[test]
    fn guard_descarta_recorte_sin_marcador_aunque_no_sea_extremo() {
        // Recorte del 40% sin ningún marcador correctivo: no hay
        // explicación legítima para quitar más de la mitad de las
        // palabras → se descarta y se devuelve el input original.
        let input = "he estado revisando el presupuesto y todo cuadra perfectamente";
        let recorte = "revisando el presupuesto";
        assert_eq!(
            guard_anti_truncacion(input, recorte),
            input,
            "sin marcador, un recorte al 30% huele a resumen y se descarta"
        );
    }

    #[test]
    fn guard_descarta_incluso_con_marcador_si_el_recorte_es_extremo() {
        // Con marcador se admite recorte hasta el 30% de las palabras;
        // por debajo (un fragmento suelto) huele a resumen y se descarta
        // igualmente — el marcador no es un cheque en blanco.
        let input = "la reunión es el jueves digo el viernes";
        let fragmento = "el viernes";
        assert_eq!(
            guard_anti_truncacion(input, fragmento),
            input,
            "2/8 palabras (<30%) incluso con marcador se descarta"
        );
    }

    #[test]
    fn guard_nunca_devuelve_vacio_ni_se_rompe_con_input_vacio() {
        // Output vacío → el input original (nunca perder el dictado).
        let input = "el dictado completo de varias palabras";
        assert_eq!(guard_anti_truncacion(input, ""), input);
        // Input vacío → no hay nada que proteger: devuelve el output tal
        // cual sin pánico (división por cero evitada).
        assert_eq!(guard_anti_truncacion("", ""), "");
        assert_eq!(guard_anti_truncacion("", "algo"), "algo");
    }

    #[test]
    fn guard_pasa_output_igual_al_input() {
        // Sin cambios (el LLM confirma el turno) → output == input, el
        // ratio es 1.0 y el guard es no-op.
        let input = "la reunión es el jueves a las cuatro";
        assert_eq!(guard_anti_truncacion(input, input), input);
    }

    #[test]
    fn word_count_cuenta_palabras_simples_y_compuestas() {
        assert_eq!(word_count("El otro día"), 3);
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("   "), 0);
        assert_eq!(word_count("a  b \t c"), 3);
    }

    #[test]
    fn llm_config_defaults_local() {
        let cfg = LlmConfig::default();
        assert_eq!(cfg.provider, CorrectionProvider::Default);
        assert_eq!(cfg.base_url, "http://127.0.0.1:8081/v1");
        assert_eq!(cfg.model, "ornith-1.0-9b");
        assert_eq!(cfg.api_key, None);
        assert_eq!(cfg.timeout_secs, 25, "timeout por defecto: 25 s");
    }

    #[test]
    fn correct_falla_con_endpoint_caido_y_devuelve_error_con_prefijo_http() {
        // Garantiza que cuando el servidor LLM NO está corriendo
        // (`connect refused` o similar), el error lleva el prefijo «HTTP:»
        // para que la UI pueda mostrar el hint «¿está el servidor LLM
        // arrancado?». Sin prefijo, la UI no sabe categorizar el error y
        // el usuario queda a ciegas (bug Juanfri 09/09/2026: «no llama al
        // LLM» sin diagnóstico).
        //
        // Usamos un puerto casi seguro libre: 1 (reservado, nadie escucha).
        // Si por lo que sea alguien escucha ahí, el test falla y lo vemos.
        let cfg = LlmConfig {
            provider: CorrectionProvider::Default,
            base_url: "http://127.0.0.1:1".to_string(),
            model: "fake".to_string(),
            api_key: None,
            timeout_secs: 2, // corta rápido para no penalizar el test
        };
        let err = correct("texto objetivo", "no, es 2", &cfg)
            .expect_err("un endpoint cerrado debe fallar");
        assert!(
            err.starts_with("HTTP:"),
            "el error debe llevar prefijo HTTP para que la UI lo categorice: {err:?}"
        );
    }

    #[test]
    fn server_alive_es_falso_con_puerto_cerrado() {
        // Puerto 1: reservado, nadie escucha — connect refused inmediato
        // (sin penalizar el test con los 2 s del timeout).
        assert!(!server_alive("http://127.0.0.1:1/v1"));
    }

    #[test]
    fn endpoint_local_solo_para_loopback() {
        assert!(is_local_endpoint("http://127.0.0.1:8081/v1"));
        assert!(is_local_endpoint("http://localhost:8081/v1"));
        assert!(is_local_endpoint("http://[::1]:8081/v1"));
        assert!(!is_local_endpoint("https://api.openai.com/v1"));
        assert!(!is_local_endpoint("https://api.groq.com/openai/v1"));
        assert!(!is_local_endpoint("http://192.168.1.5:8081/v1"));
    }

    #[test]
    fn puerto_extraido_de_base_url() {
        assert_eq!(port_from_base_url("http://127.0.0.1:8081/v1"), 8081);
        assert_eq!(port_from_base_url("http://[::1]:9000/v1"), 9000);
        // Sin puerto explícito o URL remota → default histórico 8081.
        assert_eq!(port_from_base_url("http://127.0.0.1/v1"), 8081);
        assert_eq!(port_from_base_url("https://api.openai.com/v1"), 8081);
    }

    #[test]
    fn modelo_se_resuelve_por_nombre_en_las_raices() {
        let dir = std::env::temp_dir().join(format!("whispr-llm-root-{}", std::process::id()));
        let modelo_dir = dir.join("lmstudio-community").join("Mi-Modelo-GGUF");
        std::fs::create_dir_all(&modelo_dir).unwrap();
        let gguf = modelo_dir.join("mi-modelo-q8_0.gguf");
        std::fs::write(&gguf, b"gguf fake").unwrap();

        let found = resolve_model_path_with("mi-modelo-q8_0.gguf", &[dir.clone()]);
        assert_eq!(found.as_deref(), Some(gguf.as_path()));

        // Nombre inexistente → None (sin pánico recorriendo raíces vacías).
        assert_eq!(
            resolve_model_path_with("no-existe.gguf", &[dir.clone()]),
            None
        );
        // Ruta directa con separador se usa literalmente si existe.
        let direct = resolve_model_path_with(&gguf.to_string_lossy(), &[]);
        assert_eq!(direct.as_deref(), Some(gguf.as_path()));
        // Ruta directa inexistente → None.
        let missing = std::env::temp_dir().join("no-existe-dir").join("x.gguf");
        assert_eq!(
            resolve_model_path_with(&missing.to_string_lossy(), &[]),
            None
        );
        // Nombre vacío → None.
        assert_eq!(resolve_model_path_with("   ", &[dir.clone()]), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn la_busqueda_de_modelo_no_cruza_el_limite_de_profundidad() {
        // El recorrido está acotado a 6 niveles: un fichero a profundidad 8
        // no se encuentra (evita escanear discos enteros).
        let dir = std::env::temp_dir().join(format!("whispr-llm-deep-{}", std::process::id()));
        let deep = dir
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e")
            .join("f")
            .join("g")
            .join("h");
        std::fs::create_dir_all(&deep).unwrap();
        let gguf = deep.join("profundo.gguf");
        std::fs::write(&gguf, b"gguf").unwrap();

        assert_eq!(
            resolve_model_path_with("profundo.gguf", &[dir.clone()]),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn espera_de_server_agota_timeout_sin_servidor() {
        let started = std::time::Instant::now();
        let ok = wait_for_server("http://127.0.0.1:1/v1", Duration::from_millis(250));
        assert!(!ok);
        assert!(
            started.elapsed() >= Duration::from_millis(200),
            "debe esperar al menos el plazo pedido"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn args_del_server_llevan_modelo_puerto_y_flags_gpu() {
        // El spawn usa EXACTAMENTE build_server_args: aquí fijamos el
        // contrato — modelo Windows como ruta nativa (sin traducción WSL),
        // puerto, loopback y los flags de la RTX 4070 (orchestrator 10/09).
        let model = std::path::Path::new(
            r"C:\Users\usuario\.lmstudio\models\lmstudio-community\Qwen3.5-0.8B-GGUF\Qwen3.5-0.8B-Q8_0.gguf",
        );
        let args = build_server_args(model, 8081);
        assert_eq!(
            args,
            vec![
                "-m",
                r"C:\Users\usuario\.lmstudio\models\lmstudio-community\Qwen3.5-0.8B-GGUF\Qwen3.5-0.8B-Q8_0.gguf",
                "--host",
                "127.0.0.1",
                "--port",
                "8081",
                "-ngl",
                "99",
                "--flash-attn",
                "on",
                "-ctk",
                "q8_0",
                "-ctv",
                "q8_0",
                "-c",
                "8192",
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn resolve_server_path_elige_el_primer_candidato_existente() {
        let dir = std::env::temp_dir().join(format!("whispr-server-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("llama-server.exe");
        std::fs::write(&exe, b"stub").unwrap();
        let otros = std::env::temp_dir().join(format!("whispr-server-otro-{}", std::process::id()));
        std::fs::create_dir_all(&otros).unwrap();
        let otro_exe = otros.join("llama-server.exe");
        std::fs::write(&otro_exe, b"stub").unwrap();

        // No existe el primero, sí el segundo → elige el segundo (orden).
        let roots = vec![
            dir.join("no-existe.exe"),
            otro_exe.clone(),
            exe.clone(),
        ];
        assert_eq!(
            resolve_server_path_with(&roots).as_deref(),
            Some(otro_exe.as_path())
        );
        // Ninguno existe → None.
        let vacios: Vec<std::path::PathBuf> = vec![dir.join("nada.exe")];
        assert_eq!(resolve_server_path_with(&vacios), None);
        assert_eq!(resolve_server_path_with(&[]), None);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&otros);
    }

    #[test]
    fn kill_sin_server_lanzado_no_hace_nada() {
        // Sin PID registrado, el path de salida no intenta matar nada
        // (criterio: nunca tocar servers ajenos).
        assert_eq!(launched_server_pid(), 0);
        assert!(!kill_launched_server());
    }

    #[test]
    fn terminate_server_pid_con_cero_no_hace_nada() {
        // PID 0 (System Idle Process) jamás se toca: el guard corta antes de
        // OpenProcess, por lo que tampoco puede matar un proceso ajeno.
        assert!(!terminate_server_pid(0));
    }

    // ── Normalización de URL (t_75732630) ─────────────────────────────────
    //
    // Bug real: config.json traía `http://127.0.0.1:8081/v1` y el código
    // concatenaba `/models` a pelo → `http://127.0.0.1:8081/v1/v1/models`
    // → 404 → `server_alive` = false → «server LLM sin respuesta tras 0 s»
    // aunque el server respondiera en 2 ms. La normalización vive SOLO en
    // `api_base`/`api_url`; estos tests fijan el contrato.

    #[test]
    fn api_base_colapsa_el_v1_repetido_y_quita_la_barra_final() {
        // Un `/v1` SOLO se conserva (es el prefijo de versión estándar);
        // los repetidos se colapsan a uno.
        for (entrada, esperado) in [
            ("http://127.0.0.1:8081", "http://127.0.0.1:8081"),
            ("http://127.0.0.1:8081/", "http://127.0.0.1:8081"),
            ("http://127.0.0.1:8081/v1", "http://127.0.0.1:8081/v1"),
            ("http://127.0.0.1:8081/v1/", "http://127.0.0.1:8081/v1"),
            ("http://127.0.0.1:8081/v1/v1", "http://127.0.0.1:8081/v1"),
            ("http://127.0.0.1:8081/v1/v1/", "http://127.0.0.1:8081/v1"),
            ("http://127.0.0.1:8081/v1/v1/v1", "http://127.0.0.1:8081/v1"),
            ("  http://127.0.0.1:8081/v1  ", "http://127.0.0.1:8081/v1"),
        ] {
            assert_eq!(api_base(entrada), esperado, "base «{entrada}»");
        }
    }

    #[test]
    fn api_url_es_la_misma_con_base_con_v1_duplicado() {
        // El caso del bug: una base con `/v1/v1` debe acabar en la MISMA
        // URL que la base con un solo `/v1` (la que un llama.cpp/OpenAI
        // sano sirve), nunca en `/v1/v1/models` (404).
        for base in [
            "http://127.0.0.1:8081/v1",
            "http://127.0.0.1:8081/v1/v1",
            "http://127.0.0.1:8081/v1/",
            "http://127.0.0.1:8081/v1/v1/",
        ] {
            assert_eq!(
                api_url(base, "models"),
                "http://127.0.0.1:8081/v1/models",
                "GET /models del health check con base «{base}»"
            );
            assert_eq!(
                api_url(base, "chat/completions"),
                "http://127.0.0.1:8081/v1/chat/completions",
                "POST /chat/completions del corrector con base «{base}»"
            );
        }
        // Compat hacia atrás: endpoints remotos que ya traen /v1 (OpenAI,
        // Groq) siguen funcionando — el /v1 simple NO se elimina.
        assert_eq!(
            api_url("https://api.openai.com/v1", "models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            api_url("https://api.groq.com/openai/v1", "chat/completions"),
            "https://api.groq.com/openai/v1/chat/completions"
        );
        // Base sin /v1 (default histórico, servida por llama.cpp en
        // /models — verificado en el log real: «respondiendo en
        // http://127.0.0.1:8081/v1/models (2 ms)» con base sin sufijo).
        assert_eq!(api_url("http://127.0.0.1:8081", "models"), "http://127.0.0.1:8081/models");
        // Un path con barra inicial no duplica la barra.
        assert_eq!(
            api_url("http://127.0.0.1:8081/v1", "/models"),
            "http://127.0.0.1:8081/v1/models"
        );
        // Un path de versión pegado a mano tampoco rompe: `/v1` + `/v1/models`
        // se colapsa igual.
        assert_eq!(api_base("http://127.0.0.1:8081/v1" ), "http://127.0.0.1:8081/v1");
    }

    #[test]
    fn server_alive_y_wait_for_server_usan_la_url_normalizada() {
        // Contrato de integración sin red: con la base del bug
        // (`/v1`), el único puerto que se consulta es el normalizado. Un
        // puerto cerrado responde «no vivo» sin esperar el timeout largo.
        assert!(!server_alive("http://127.0.0.1:1/v1"));
        assert!(!server_alive_auth("http://127.0.0.1:1/v1/v1", Some("clave")));
    }

    // ── Saneado del ejecutable del server (t_75732630) ────────────────────

    #[test]
    fn is_server_exe_solo_acepta_llama_server_exe() {
        use std::path::Path;
        assert!(is_server_exe(Path::new(r"D:\LlamaCPP\llama-server.exe")));
        assert!(is_server_exe(Path::new(r"D:\LlamaCPP\LLAMA-SERVER.EXE")));
        // El wrapper histórico y el server ELF de WSL NO son válidos: el
        // proceso lanzado debe ser matable por nombre de imagen al salir.
        assert!(!is_server_exe(Path::new(r"D:\LlamaCPP\llama.exe")));
        assert!(!is_server_exe(Path::new("/usr/bin/wsl.exe")));
        assert!(!is_server_exe(Path::new(r"D:\LlamaCPP\llama-server")));
        assert!(!is_server_exe(Path::new("")));
    }

    #[test]
    fn sanitize_server_exe_cambia_el_wrapper_por_el_hermano_real() {
        let dir = std::env::temp_dir().join(format!("whispr-sanitize-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wrapper = dir.join("llama.exe");
        let real = dir.join("llama-server.exe");
        std::fs::write(&wrapper, b"stub").unwrap();
        std::fs::write(&real, b"stub").unwrap();

        // `llama.exe` → `llama-server.exe` hermano (el caso de config.json).
        assert_eq!(sanitize_server_exe(&wrapper).as_deref(), Some(real.as_path()));
        // Un nombre ya válido se devuelve tal cual.
        assert_eq!(sanitize_server_exe(&real).as_deref(), Some(real.as_path()));

        // Sin hermano `llama-server.exe` no hay ruta válida (el caller
        // autodetecta por rutas comunes).
        let otros = std::env::temp_dir().join(format!("whispr-sanitize-otro-{}", std::process::id()));
        std::fs::create_dir_all(&otros).unwrap();
        let solo_wrapper = otros.join("llama.exe");
        std::fs::write(&solo_wrapper, b"stub").unwrap();
        assert_eq!(sanitize_server_exe(&solo_wrapper), None);

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&otros);
    }

    // ── Regresión VIVA contra un server real (t_75732630) ─────────────────
    //
    // Se ejecuta a mano con un llama-server NATIVO ya escuchando en
    // 127.0.0.1:8081 (Qwen3.5-0.8B-Q8_0.gguf):
    //   D:\LlamaCPP\llama-server.exe -m <gguf> --host 127.0.0.1 --port 8081
    //   cargo.exe test regresion_viva -- --ignored --nocapture
    //
    // Fija el criterio 4 de la task: `server_alive`/`wait_for_server` pasan
    // (sin «sin respuesta tras 0 s») con la base `/v1` del config del
    // usuario, y la URL cruda duplicada `/v1/v1/models` —el bug— da 404.
    #[test]
    #[ignore = "requiere llama-server.exe nativo en 127.0.0.1:8081"]
    fn regresion_viva_normalizacion_url_contra_server_real() {
        let con_v1 = "http://127.0.0.1:8081/v1";
        let duplicada = "http://127.0.0.1:8081/v1/v1";

        assert!(
            server_alive(con_v1),
            "GET /v1/models debe responder contra el server nativo real"
        );
        assert!(
            server_alive(duplicada),
            "la base con /v1/v1 se colapsa a /v1/models y también responde"
        );
        let t0 = std::time::Instant::now();
        assert!(
            wait_for_server(con_v1, Duration::from_secs(5)),
            "wait_for_server no debe devolver en el primer poll"
        );
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "el health check debe pasar en el primer intento"
        );

        // La URL CRUDA duplicada sí es un 404: es exactamente el bug que
        // arregla `api_url` (nadie la construye ya así).
        let cruda = ureq::get("http://127.0.0.1:8081/v1/v1/models")
            .timeout(Duration::from_secs(2))
            .call();
        assert!(
            cruda.is_err(),
            "sanity del bug: /v1/v1/models crudo debe ser un 404"
        );
    }
}
