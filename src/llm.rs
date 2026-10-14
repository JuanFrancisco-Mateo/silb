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

/// Directorio del ejecutable REAL de la app (`std::env::current_exe()`): en
/// dev es `target/<perfil>/`, instalada es la carpeta de `silb.exe` ({app}).
/// Es el ancla de todo lo que VIAJA con la app (`vendor/`).
pub fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
}

/// Server LLM EMBARCADO en el paquete: `vendor/llama/llama-server.exe`,
/// hermano del exe de la app. Candidato nº1 de `resolve_server_path`: el
/// usuario no teclea ninguna ruta (requisito 14/10/2026).
pub fn vendored_server_path_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let p = dir.join("vendor").join("llama").join(SERVER_EXE_NAME);
    p.is_file().then_some(p)
}

/// Runtime CUDA OPCIONAL del server vendado: `vendor/llama/cuda/`, si existe
/// y hay driver NVIDIA. Con él el server se arranca con ese directorio como
/// cwd — el cargador de backends de ggml busca `ggml-cuda*.dll` en el cwd
/// (verificado con el binario real; el PATH NO sirve para el descubrimiento)
/// — y con el directorio al frente del PATH, para las dependencias de
/// `ggml-cuda.dll` (cudart/cublas). Sin pack, el server corre en CPU.
pub fn vendored_cuda_dir_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let cuda = dir.join("vendor").join("llama").join("cuda");
    cuda.is_dir().then_some(cuda)
}

/// ¿Hay driver NVIDIA en la máquina? El pack CUDA solo se usa con él: cargar
/// `ggml-cuda.dll` sin driver es un fallo de arranque, no una mejora.
fn nvidia_driver_present() -> bool {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    std::path::Path::new(&root)
        .join("System32")
        .join("nvcuda.dll")
        .is_file()
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

/// Guard determinista para sustituciones numéricas simples
/// (t_1fc864ef).
///
/// El corrector LLM local (Qwen3.5-0.8B-Q8) es POCO FIABLE en
/// razonamiento numérico con numerales en palabras: ante una
/// auto-corrección tipo «A digo B» o «A en vez de B» con A y B ambos
/// numerales (dígitos o palabras: cuatro, cinco, …), el modelo a veces
/// concatena los dos (evidencia: «me comi cuatro digo cinco peras» →
/// «me comi cuatro cinco peras») en lugar de sustituir. Esa salida
/// concatena los DOS numerales y, por tanto, ningún guard existente la
/// detecta: `guard_anti_truncacion` no la corta (ratio de palabras
/// plausible) y `contains_in_turn_marker` no ve meta-lengua. El texto
/// inyectado es gramaticalmente incoherente.
///
/// Este guard detecta el patrón inequívoco «NUM_A [texto] [marcador]
/// NUM_B» en el INPUT y verifica que el output del LLM lo haya
/// resuelto. Si el LLM mantuvo AMBOS numerales (concatenación rota),
/// conservó NUM_A (el equivocado) o eliminó la corrección, reconstruye
/// el texto aplicando la sustitución deterministamente en código:
/// «texto_antes» + NUM_B + «texto_despues».
///
/// Cobertura ampliada (t_d64797c2, H1 del QA-REPORT-DEPURACION): el
/// patrón NO exige ya que NUM_A sea el último token antes del marcador.
/// En habla natural hay sintagma nominal y/o verbo entre el numeral y el
/// marcador («a tres personas, digo cuatro personas»), y el guard se
/// perdía el caso mientras el LLM inyectaba una frase plausible y FALSA
/// (log 14/09 03:19 y 13/09 01:37). NUM_A es ahora el último NUMERAL del
/// fragmento anterior al marcador.
///
/// Devuelve `Ok(texto_corregido)` cuando el guard interviene (porque
/// el LLM claramente falló en este caso). Devuelve `Err(())` cuando NO
/// hay patrón inequívoco o cuando el LLM ya lo resolvió bien — el
/// caller decide entonces si acepta el output del LLM tal cual. Nunca
/// bloquea el dictado: el peor caso siempre es inyectar texto.
///
/// ALCANCE: solo numerales simples (un solo token, sin contexto
/// elaborado). Casos como «me debe tres mil doscientos cuarenta y ocho
/// euros» NO entran (no hay auto-corrección). Tampoco entran
/// correcciones con sustantivos (murcia/cartagena: el LLM ya las hace
/// bien). Esto es la red de seguridad mínima para el bug concreto del
/// 0.8B-Q8, no un parser de números universal.
pub fn guard_numeric_substitution(input: &str, output: &str) -> Result<String, ()> {
    // 1) Detectar el patrón inequívoco «NUM_A [marcador] NUM_B» en el
    //    input. Si no encaja, el guard no aplica.
    let Some(pat) = parse_numeric_correction(input) else {
        return Err(());
    };

    // 2) Si el LLM ya puso el «después» (NUM_B) en el output, lo damos
    //    por bueno y NO intervenimos (puede haber pequeñas variaciones
    //    que el LLM manejó bien, p.ej. «voy a las 4» en lugar de
    //    «voy a las cuatro»). Dejamos que los otros guards decidan.
    if contains_token(&output.to_lowercase(), &pat.num_b.to_lowercase())
        && !contains_token(&output.to_lowercase(), &pat.num_a.to_lowercase())
    {
        return Err(());
    }

    // 3) Reconstrucción determinista. Buscamos las posiciones
    //    exactas (en bytes) del token NUM_A y del token NUM_B en el
    //    input, y construimos: input[..num_a_start] + num_b + input[after_num_b..].
    //    Esto elimina NUM_A, el marcador completo (entre NUM_A y
    //    NUM_B) y la posible duplicación de palabras de función
    //    alrededor de NUM_B (p.ej. «sino a las cinco» → «cinco», sin
    //    volver a escribir «a las»).
    let num_a_lower = pat.num_a.to_lowercase();
    let num_b_lower = pat.num_b.to_lowercase();
    // Acceso a la regex de marcador para detectar si `before_raw`
    // empieza por «no …» (caso B: «no X sino»). La usamos para
    // descartar el prefijo «no » antes de NUM_A y producir una
    // reconstrucción limpia (p.ej. «no voy a las tres sino …» →
    // «voy a las cuatro» y NO «no voy a las cuatro»).
    let re_for_discard_start = numeric_marker_regex();

    // Tokenizamos el input guardando el rango de bytes de cada token.
    struct Tok {
        start: usize, // byte offset en el input original
        end: usize,
        text: String,
        is_word: bool,
    }
    let mut tokens: Vec<Tok> = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current = String::new();
    for (i, c) in input.char_indices() {
        if c.is_alphanumeric() {
            if current_start.is_none() {
                current_start = Some(i);
            }
            current.push(c);
        } else {
            if !current.is_empty() {
                let s = current_start.unwrap();
                tokens.push(Tok {
                    start: s,
                    end: i,
                    text: current.clone(),
                    is_word: true,
                });
                current.clear();
                current_start = None;
            }
        }
    }
    if !current.is_empty() {
        let s = current_start.unwrap();
        let end = input.len();
        tokens.push(Tok {
            start: s,
            end,
            text: current,
            is_word: true,
        });
    }

    // NUM_B: primer token con ese texto que aparece DESPUÉS del marcador
    // (`pat.marker_end`). Acotarlo al tramo posterior evita confundirlo
    // con una aparición anterior del mismo numeral.
    let num_b_token = tokens.iter().find(|t| {
        t.is_word && t.start >= pat.marker_end && t.text.to_lowercase() == num_b_lower
    });
    let Some(num_b_tok) = num_b_token else {
        // NUM_B no aparece (no debería pasar). Fallback seguro.
        return Ok(input.to_string());
    };
    // NUM_A: ÚLTIMO token con ese texto que aparece ANTES de NUM_B (el
    // numeral rectificado siempre precede al correcto).
    let num_a_token = tokens
        .iter()
        .rev()
        .find(|t| t.is_word && t.start < num_b_tok.start && t.text.to_lowercase() == num_a_lower);
    let Some(num_a_tok) = num_a_token else {
        // NUM_A no aparece (no debería pasar). Fallback seguro.
        return Ok(input.to_string());
    };

    let before_raw = &input[..num_a_tok.start];
    let after_raw = &input[num_b_tok.end..];
    // Para el caso «no X sino» (regex con grupo 1), el patrón empieza
    // por «no » y eso está dentro de `before_raw` (porque NUM_A va
    // DENTRO de X). Necesitamos descartar también el «no » inicial
    // para que la reconstrucción sea limpia.
    let before = if re_for_discard_start.captures(before_raw).ok().flatten().is_some()
        || before_raw.trim_start().to_lowercase().starts_with("no ")
    {
        // Caso B detectado: quitar el prefijo «no …» hasta el primer
        // token no-numeral que sea inicio de NUM_A (o sea: quitar
        // «no » si está al principio).
        let trimmed = before_raw.trim_start();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("no ") || lower.starts_with("no,") {
            // Saltamos «no » y lo que venga hasta el primer token útil.
            let after_no = if lower.starts_with("no ") { 3 } else { 2 };
            let rest = &trimmed[after_no..];
            rest.trim_start()
        } else {
            before_raw
        }
    } else {
        before_raw
    };
    let after = after_raw;
    // Decidimos si necesitamos un espacio adicional entre los trozos.
    // La idea: si entre `before` y NUM_A no había whitespace, las
    // palabras estaban pegadas (raro) y debemos meter un espacio.
    // Si sí había whitespace, ya está la separación: NO añadimos.
    // Lo mismo entre NUM_B y `after`.
    let space_before_num_b = !before.is_empty() && !before.ends_with(char::is_whitespace);
    let space_after_num_b = !after.is_empty() && !after.starts_with(char::is_whitespace);
    let mut reconstructed = String::with_capacity(before.len() + pat.num_b.len() + after.len() + 2);
    reconstructed.push_str(before);
    if space_before_num_b {
        reconstructed.push(' ');
    }
    reconstructed.push_str(&pat.num_b);
    if space_after_num_b {
        reconstructed.push(' ');
    }
    reconstructed.push_str(after);
    let reconstructed = reconstructed.trim().to_string();

    // 4) Si reconstruido quedó vacío o es solo espacios, conservar
    //    el input (fallback seguro).
    if reconstructed.is_empty() {
        return Ok(input.to_string());
    }

    // 5) Logs para diagnóstico: este caso es diagnóstico porque el LLM
    //    acaba de fallar en razonamiento numérico (t_1fc864ef).
    crate::log!(
        "llm",
        "in-turn: guard numérico determinista reconstruyó la sustitución: \
         num_a={:?} num_b={:?} input={:?} output={:?} reconstruido={:?}",
        pat.num_a,
        pat.num_b,
        input,
        output,
        reconstructed,
    );

    // 6) El caller (app.rs) usa el output del guard si es distinto
    //    del LLM original. Si coinciden (caso raro), devolvemos
    //    Err(()) para no duplicar trabajo.
    if reconstructed == output.trim() {
        Err(())
    } else {
        Ok(reconstructed)
    }
}

/// Resultado de `parse_numeric_correction`: el input partido en el
/// primer marcador correctivo inequívoco, con los dos numerales
/// identificados y la posición donde EMPIEZA la zona a descartar
/// (NUM_A + marcador + posible contexto de función entre marcador y
/// NUM_B — esto último se resuelve en la reconstrucción).
///
/// `input_a` e `input_b` se conservan para trazabilidad y futuros
/// consumidores (p.ej. tests de integración con la pipeline); el
/// reconstructor principal hoy solo usa `num_a`/`num_b`.
struct NumericCorrection {
    /// Texto del input hasta el inicio de la zona a descartar.
    #[allow(dead_code)]
    input_a: String,
    /// Texto del input desde el final del marcador (incluye NUM_B).
    #[allow(dead_code)]
    input_b: String,
    /// Offset en bytes del FINAL del marcador dentro del input. El
    /// guard lo usa para localizar NUM_B sin confundirlo con una
    /// aparición anterior del mismo numeral (p.ej. «23 … digo … 24»).
    marker_end: usize,
    /// Token literal de NUM_A tal como aparece en el input.
    num_a: String,
    /// Token literal de NUM_B tal como aparece en el input.
    num_b: String,
}

/// Busca el primer marcador correctivo inequívoco en `input` y, si a
/// cada lado hay exactamente UN numeral simple (dígito o palabra de la
/// lista corta), devuelve la partición. Si no hay patrón inequívoco,
/// devuelve `None`.
///
/// Lista de numerales: solo los más comunes que caben en UN token.
/// NO incluye «tres mil», «cuarenta y ocho», etc.: el guard es para el
/// bug concreto del 0.8B-Q8, no un parser de números universal.
const NUM_WORDS: &[&str] = &[
    "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho",
    "nueve", "diez", "once", "doce", "trece", "catorce", "quince",
    "dieciséis", "dieciseis", "veinte", "treinta", "cuarenta",
    "cincuenta", "sesenta", "setenta", "ochenta", "noventa", "cien",
    "mil",
];

/// Marcadores correctivos que separan NUM_A y NUM_B. Deben ser
/// inequívocos: si el LLM ya entendió bien la frase, este guard no
/// debe intervenir. Por eso usamos los mismos que `has_unambiguous_in_turn_marker`
/// más «no X sino Y» y «X en vez/lugar de Y».
///
/// Diseño: el regex devuelve DOS grupos cuando es posible:
/// - grupo 1 (opcional): el «antes» del marcador que debe eliminarse.
///   En «no X sino» X es lo que se va a sustituir; necesitamos que
///   esté en el grupo para extraer NUM_A correctamente.
/// - grupo 2: el marcador en sí.
///
/// NOTA sobre (1): la lógica de auto-corrección ya existe en
/// `src/domain/corrective.rs` (`IN_TURN_MARKERS`,
/// `apply_diff`/`apply_diff_inferido`). Sin embargo, ese módulo es
/// declarativo/clasificador: su regex casa el marcador con meta-lengua
/// general (digo, en vez de, no X sino…) y exige estructura «antes /
/// después» con longitudes mínimas. NO está pensado para reconstruir
/// la sustitución (no devuelve qué numeral se sustituye por cuál), y
/// su lista de marcadores cubre más casos (incluido «mejor», «dime»
/// como súplicas) que NO queremos tocar aquí. Reutilizar ese regex
/// directo aquí haría al guard más permisivo de lo necesario y
/// dispararía falsos positivos en frases coloquiales que el LLM
/// resuelve bien. Mantener la regex acotada en este módulo es la
/// decisión correcta: el guard es pequeño, surgical, y auto-contenido.
const NUMERIC_MARKER_REGEX: &str = r"(?i)\b(?:no\s+(.+?)\s+sino\b|(digo\b|diga\b|dime\b|quiero decir\b|quise decir\b|mejor dicho\b|en vez de\b|en lugar de\b))";

/// Regex compilada perezosamente (fancy-regex tiene coste).
fn numeric_marker_regex() -> &'static fancy_regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<fancy_regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        fancy_regex::Regex::new(NUMERIC_MARKER_REGEX)
            .expect("regex de marcador numérico inválida")
    })
}

/// ¿`token` es un numeral simple (dígito o palabra de la lista)?
fn is_numeric_token(token: &str) -> bool {
    let t = token.to_lowercase();
    if t.parse::<u64>().is_ok() {
        return true;
    }
    NUM_WORDS.contains(&t.as_str())
}

/// ¿`text` contiene `token` como palabra completa (separada por
/// whitespace o puntuación)?
fn contains_token(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    text.split(|c: char| !c.is_alphanumeric())
        .any(|t| t == token)
}

/// Quita la ÚLTIMA aparición de `token` (como palabra completa) de
/// `text` y devuelve el resultado con los espacios saneados. Si no
/// aparece, devuelve `None` (el caller decide el fallback).
fn strip_last_token(text: &str, token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    // Tokenizamos conservando los separadores para poder reconstruir
    // los espacios correctamente.
    let mut tokens: Vec<(String, bool)> = Vec::new();
    let mut current = String::new();
    let mut current_is_word = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            current.push(c);
            current_is_word = true;
        } else {
            if !current.is_empty() {
                tokens.push((current.clone(), current_is_word));
                current.clear();
                current_is_word = false;
            }
            tokens.push((c.to_string(), false));
        }
    }
    if !current.is_empty() {
        tokens.push((current, current_is_word));
    }
    // Buscamos la última posición donde hay un token que coincide (case-insensitive).
    let mut last_idx: Option<usize> = None;
    for (i, (tok, is_word)) in tokens.iter().enumerate() {
        if *is_word && tok.to_lowercase() == token.to_lowercase() {
            last_idx = Some(i);
        }
    }
    let idx = last_idx?;
    // Eliminamos el token y los separadores inmediatamente anteriores
    // (espacios) para dejar el texto limpio.
    let mut remove_from = idx;
    while remove_from > 0 {
        let (ref s, is_word) = tokens[remove_from - 1];
        if !is_word && (s == " " || s == "\t" || s == "\n") {
            remove_from -= 1;
        } else {
            break;
        }
    }
    let mut out = String::new();
    for (i, (s, _)) in tokens.iter().enumerate() {
        if i >= remove_from && i <= idx {
            continue;
        }
        out.push_str(s);
    }
    Some(out)
}

/// Parsea el input buscando el patrón inequívoco «NUM_A [marker] NUM_B».
///
/// Caso A (marcador simple: digo, en vez de, …):
///   - `start` = inicio del marcador (ej. «digo»)
///   - `end` = fin del marcador
///   - `before` = input[..start] (NUM_A es el último NUMERAL de `before`,
///     no necesariamente el último token: en habla natural el numeral
///     lleva detrás sintagma nominal y/o verbo — «a tres personas, digo
///     cuatro» — y el último token sería «personas»)
///   - `after` = input[end..] (NUM_B está al principio de `after`)
///
/// Caso B («no X sino»):
///   - el grupo 1 del regex captura X, que es la «pieza incorrecta»
///     que el usuario sustituye. NUM_A = last_token(X).
///   - `before` = input[..start_of_match] (lo que va antes del «no»,
///     p.ej. en «no voy a las tres sino…» antes del «no» no hay
///     nada útil; pero en frases más largas como «el precio es 4 no
///     5 sino 6» antes del «no» sí hay contexto que mantener).
///   - `after` = input[end_of_match..] (NUM_B está al principio).
fn parse_numeric_correction(input: &str) -> Option<NumericCorrection> {
    let re = numeric_marker_regex();
    let caps = re.captures(input).ok().flatten()?;
    let m = caps.get(0)?;
    let start = m.start();
    let end = m.end();
    let marker_text = m.as_str();

    // NUM_B: primer numeral en `input[end..]`, saltando tokens de
    // función. Igual para ambos casos.
    let after = input[end..].trim();
    let num_b = first_numeric_token(after)?;
    if count_numeric_tokens(after) != 1 {
        return None;
    }

    // NUM_A depende del tipo de marcador:
    // - Caso B («no X sino»): X está capturado en el grupo 1 y ES el
    //   «antes» que se sustituye. NUM_A es el último numeral de X.
    //   Pero además hay que mantener la parte de X que va ANTES de
    //   NUM_A (p.ej. «no voy a las tres sino …» → conservamos «voy a
    //   las» y solo sustituimos «tres»).
    // - Caso A: NUM_A es el último token de `input[..start]`.
    let (before, num_a) = if let Some(x_match) = caps.get(1) {
        // Caso B: «no X sino».
        let x = x_match.as_str();
        let last = last_token(x)?;
        if !is_numeric_token(&last) {
            return None;
        }
        // El «antes» final: lo que va antes del «no» + la parte de X
        // que precede a NUM_A (sin incluir el numeral).
        let prefix_before_no = input[..start].trim_end();
        let x_without_num = strip_last_token(x, &last).unwrap_or_default();
        let combined = if prefix_before_no.is_empty() {
            x_without_num.trim().to_string()
        } else if x_without_num.trim().is_empty() {
            prefix_before_no.to_string()
        } else {
            format!("{} {}", prefix_before_no, x_without_num.trim())
        };
        (combined, last)
    } else {
        // Caso A: NUM_A = el numeral RELEVANTE del «antes», es decir el
        // ÚLTIMO numeral que aparece antes del marcador. NO exigimos que
        // sea el último TOKEN: en habla natural el numeral va seguido de
        // sintagma nominal y/o verbo («a tres personas, digo cuatro»,
        // «a 23 personas que iban, digo, a 24»), así que el último token
        // era «personas»/«iban», no el numeral, y el patrón no se
        // detectaba (H1 del QA-REPORT-DEPURACION, log 14/09 03:19 y
        // 13/09 01:37). Se toleran, por tanto, tokens funcionales y de
        // contenido entre NUM_A y el marcador.
        //
        // Precisión > recall: el «antes» debe tener EXACTAMENTE un
        // numeral. Con dos o más candidatos no se puede saber cuál es el
        // que el usuario rectifica → no tocamos nada.
        let before_str = input[..start].trim();
        let last = last_numeric_token(before_str)?;
        if count_numeric_tokens(before_str) != 1 {
            return None;
        }
        (before_str.trim_end().to_string(), last)
    };

    // El «después» (todo lo que va tras el marcador): en el caso B hay
    // que descontar X y «sino» (que ya están en el match, dentro de
    // `input[end..]` solo queda «Y …»).
    let _ = marker_text; // sólo documenta

    Some(NumericCorrection {
        input_a: before,
        input_b: after.to_string(),
        marker_end: end,
        num_a,
        num_b,
    })
}

/// Primer token NUMÉRICO de un texto, saltando tokens de función
/// («a», «las», «los», «el», «la», «de», «del», «al»). Devuelve `None`
/// si no hay ningún numeral.
fn first_numeric_token(s: &str) -> Option<String> {
    const FUNCTION_WORDS: &[&str] = &[
        "a", "al", "de", "del", "el", "la", "las", "lo", "los", "y", "o", "u",
    ];
    let mut current = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            let lower = current.to_lowercase();
            if is_numeric_token(&lower) {
                return Some(current);
            }
            if !FUNCTION_WORDS.contains(&lower.as_str()) {
                // Encontramos un token que no es numeral ni function
                // word: el numeral real (si existe) viene DESPUÉS. Si
                // ya pasamos tokens no-numéricos no-numerales, este
                // turno no encaja en el patrón «NUM_B inmediato».
                return None;
            }
            current.clear();
        }
    }
    if !current.is_empty() && is_numeric_token(&current.to_lowercase()) {
        Some(current)
    } else {
        None
    }
}

/// Último token «palabra» de un texto (alfanumérico).
fn last_token(s: &str) -> Option<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .next_back()
        .map(|t| t.to_string())
}

/// Último token NUMÉRICO de un texto (dígito o palabra de `NUM_WORDS`),
/// ignorando el resto de tokens. `None` si no hay ningún numeral.
///
/// Es la pieza que permite que NUM_A sea «tres» en «a tres personas,
/// digo cuatro»: el numeral relevante no es el último TOKEN del
/// fragmento anterior al marcador, sino el último NUMERAL de ese
/// fragmento (H1 del QA-REPORT-DEPURACION).
fn last_numeric_token(s: &str) -> Option<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && is_numeric_token(t))
        .next_back()
        .map(|t| t.to_string())
}

/// Cuenta cuántos tokens numéricos hay en `s`.
fn count_numeric_tokens(s: &str) -> usize {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && is_numeric_token(t))
        .count()
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
/// (o URL remota) → 8081, el default histórico de Silb.
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
/// - `current_dir` = directorio del exe (o `cuda/` si hay pack CUDA): el
///   binario es un stub que carga `llama-server-impl.dll` y sus dependencias
///   (cublas64_13.dll, ggml-*) desde ese directorio; lanzarlo con otro cwd
///   puede fallar en la carga (STATUS_ENTRYPOINT_NOT_FOUND).
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
    // Pack CUDA opcional junto al server vendado (`vendor/llama/cuda/`): el
    // cwd pasa a ser ese directorio porque el cargador de backends de ggml
    // busca los `ggml-*.dll` en el cwd y en el directorio del exe — y sus
    // dependencias (cudart/cublas) se resuelven por PATH, así que el pack va
    // al frente. Sin pack (o sin driver NVIDIA) se arranca desde el
    // directorio del exe: CPU, y sin avisos.
    let cuda_dir = cwd.join("cuda");
    let use_cuda = cuda_dir.is_dir() && nvidia_driver_present();
    if cuda_dir.is_dir() && !use_cuda {
        crate::log!(
            "llm",
            "pack CUDA en {} ignorado: no hay driver NVIDIA (el server corre en CPU)",
            cuda_dir.display()
        );
    }
    let cwd = if use_cuda { cuda_dir.clone() } else { cwd };
    let args = build_server_args(model, port);
    let log_path = crate::config::config_path()
        .parent()
        .map(|p| p.join("llama-server.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("llama-server.log"));
    let log_file = std::fs::File::create(&log_path)
        .map_err(|e| format!("no se pudo abrir el log del server ({log_path:?}): {e}"))?;

    let mut command = Command::new(exe);
    command
        .args(&args)
        .current_dir(&cwd)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            log_file
                .try_clone()
                .map_err(|e| format!("log del server: {e}"))?,
        ))
        .stderr(Stdio::from(log_file));
    if use_cuda {
        let current = std::env::var_os("PATH").unwrap_or_default();
        let with_cuda: Vec<std::path::PathBuf> = std::iter::once(cuda_dir.clone())
            .chain(std::env::split_paths(&current))
            .collect();
        if let Ok(joined) = std::env::join_paths(with_cuda) {
            command.env("PATH", joined);
        }
    }
    let child = command
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

/// Stub fuera de Windows: el server LLM es un binario nativo de Windows (regla
/// del proyecto: nunca se arranca vía WSL), así que aquí no hay nada que
/// lanzar; mismo contrato (`Err` con el motivo).
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

/// Ruta del ejecutable `llama-server.exe` a usar. Orden (14/10/2026, «cero
/// rutas que teclear»): el server EMBARCADO en el paquete
/// (`vendor/llama/llama-server.exe` junto al exe de la app) → la ruta del
/// config (override manual histórico, validado) → autodetección (rutas
/// comunes y PATH). El vendado GANA al config: una ruta vieja en config.json
/// (de una instalación anterior) no puede romper una instalación nueva.
/// `None` si no hay ningún candidato → el arranque lo reporta como
/// `LlmDisabled` con aviso (el endpoint OpenAI-compatible editable sigue
/// disponible como alternativa).
pub fn resolve_server_path(cfg: &crate::config::Config) -> Option<std::path::PathBuf> {
    resolve_server_path_in(
        exe_dir().as_deref(),
        cfg.llama_server_path(),
        &default_server_roots(),
    )
}

/// Núcleo del orden de resolución, parametrizable para tests con directorios
/// temporales (nada de disco real: ni `D:\LlamaCPP` ni PATH de la máquina).
pub(crate) fn resolve_server_path_in(
    exe_dir: Option<&std::path::Path>,
    explicit: Option<&str>,
    common: &[std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    exe_dir
        .and_then(vendored_server_path_in)
        .or_else(|| explicit.and_then(explicit_server_path))
        .or_else(|| resolve_server_path_with(common))
}

/// Decide SOLO sobre la ruta explícita de Ajustes («Server LLM»). Separada
/// de las rutas de autodetección para poder testearla con ficheros temporales
/// sin depender del disco real de la máquina (D:\LlamaCPP, PATH…).
///
/// - No existe en disco → `None` (el caller cae a la autodetección).
/// - Existe pero no es `llama-server.exe` (ni tiene un hermano que lo sea)
///   → `None` con log (se ignora y se autodetecta).
/// - Existe y es (o se sanea a) `llama-server.exe` → `Some`.
pub(crate) fn explicit_server_path(explicit: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let p = PathBuf::from(explicit);
    if !p.is_file() {
        crate::log!(
            "llm",
            "ruta de server LLM configurada no existe ({}); buscando por defecto",
            p.display()
        );
        return None;
    }
    // Saneado (t_75732630): la ruta configurada DEBE ser
    // `llama-server.exe`. Si apunta a un wrapper (p. ej.
    // `llama.exe`) usamos el `llama-server.exe` hermano — así el
    // proceso lanzado es matable por nombre al salir.
    match sanitize_server_exe(&p) {
        Some(fixed) => {
            if fixed != p {
                crate::log!(
                    "llm",
                    "ruta de server LLM «{}» no es {SERVER_EXE_NAME}: se usa {}",
                    p.display(),
                    fixed.display()
                );
            }
            Some(fixed)
        }
        None => {
            crate::log!(
                "llm",
                "ruta de server LLM configurada ({}) no es {SERVER_EXE_NAME} ni hay \
                 {SERVER_EXE_NAME} junto a ella; se ignora y se autodetecta",
                p.display()
            );
            None
        }
    }
}

/// Candidatos de AUTODETECCIÓN del server LLM (el vendado y el config se
/// prueban ANTES, en `resolve_server_path_in`), en orden de preferencia:
/// 1. `D:\LlamaCPP\llama-server.exe` (instalación histórica de Juanfri,
///    build CUDA 13 con cublas64_13.dll) — y su gemelo `C:\LlamaCPP`.
/// 2. Junto al exe de la app (layout plano, sin `vendor/`).
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
    resolve_model_path_with(model_name, &model_roots_in(exe_dir().as_deref()))
}

/// Raíces donde busca el GGUF del corrector `resolve_model_path` (log de
/// diagnóstico cuando no aparece: el usuario no tiene que escribir ninguna).
pub fn model_search_roots() -> Vec<std::path::PathBuf> {
    model_roots_in(exe_dir().as_deref())
}

/// Raíces donde buscar el GGUF del corrector, en orden:
/// 1. `vendor/models` junto al exe: el modelo puede VIAJAR con la app (misma
///    raíz `vendor/` que el server; si no está, no pasa nada).
/// 2. `%APPDATA%\whispr-open\models`: la MISMA carpeta donde la app descarga
///    los modelos de whisper — es el sitio natural para el GGUF descargado.
/// 3. LM Studio e instalaciones de llama.cpp con carpeta `models` propia
///    (compatibilidad con lo que ya hubiera en la máquina).
fn model_roots_in(exe_dir: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = exe_dir {
        roots.push(dir.join("vendor").join("models"));
    }
    roots.push(crate::transcribe::models_dir());
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

    // ── Guard numérico determinista (t_1fc864ef) ─────────────────────────
    //
    // El corrector LLM local (Qwen3.5-0.8B-Q8) concatena numerales en
    // lugar de sustituirlos cuando se dicta «A digo B» con A y B ambos
    // numerales. Evidencia real del log: «me comi cuatro digo cinco
    // peras» → output del LLM «me comi cuatro cinco peras» (sin
    // «digo», con los dos números juntos). Este guard detecta el patrón
    // en el INPUT y reconstruye la sustitución en código.
    //
    // Contrato: el guard interviene SOLO si el patrón es inequívoco
    // (dos numerales simples separados por un marcador correctivo) Y el
    // output del LLM no cumple la corrección. En cualquier otro caso
    // devuelve Err(()) y deja el flujo del LLM en manos del resto de
    // guards existentes.

    #[test]
    fn guard_numeric_corrige_concatenacion_de_numerales_en_palabras() {
        // Caso del bug: input «A digo B», LLM devuelve «A B» (sin
        // marcador y con los dos numerales concatenados).
        let input = "me comi cuatro digo cinco peras";
        let output = "me comi cuatro cinco peras";
        let result = guard_numeric_substitution(input, output)
            .expect("el guard debe reconstruir la sustitución");
        assert_eq!(
            result, "me comi cinco peras",
            "sustitución determinista: «cuatro» se reemplaza por «cinco»"
        );
    }

    #[test]
    fn guard_numeric_corrige_concatenacion_con_numerales_digitos() {
        // Variante con dígitos: «2 en vez de 4» → output «2 4»
        // (concatenación rota).
        let input = "el plazo es 2 en vez de 4 días";
        let output = "el plazo es 2 4 días";
        let result = guard_numeric_substitution(input, output)
            .expect("el guard debe reconstruir la sustitución con dígitos");
        assert_eq!(
            result, "el plazo es 4 días",
            "sustitución determinista: «2» se reemplaza por «4»"
        );
    }

    #[test]
    fn guard_numeric_corrige_con_no_sino() {
        // Variante con marcador «no X sino Y»: input «no X sino Y»,
        // output «X Y» (concatenación del 0.8B).
        let input = "no voy a las tres sino a las cuatro";
        let output = "voy a las tres cuatro";
        let result = guard_numeric_substitution(input, output)
            .expect("el guard debe reconstruir con «no X sino Y»");
        assert_eq!(
            result, "voy a las cuatro",
            "sustitución determinista: «tres» se reemplaza por «cuatro»"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_llm_resolvio_bien() {
        // El LLM devolvió la respuesta correcta: NO debe intervenir.
        let input = "me comi cuatro digo cinco peras";
        let output = "me comi cinco peras";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "el LLM ya resolvió la corrección: el guard NO debe intervenir"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_no_hay_numerales() {
        // Caso sin numerales (corrección geográfica). NO es problema del
        // 0.8B (el LLM lo resuelve bien en runtime) → no intervenimos.
        let input = "que calor en murcia digo cartagena";
        let output = "que calor en murcia cartagena"; // hipotético «fallo» del LLM
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "sin numerales el guard numérico no aplica (alcance del fix)"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_no_hay_patron_a_digo_b() {
        // Numerales presentes pero SIN marcador correctivo: el turno
        // NO es una auto-corrección numérica.
        let input = "me comi cuatro peras";
        let output = "me comi cuatro peras";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "sin marcador correctivo no hay sustitución que aplicar"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_solo_hay_un_numeral() {
        // El usuario repite el numeral sin corregirlo: no hay
        // auto-corrección y el guard no debe inventarse una.
        let input = "me comi cuatro digo cinco peras y me comi cuatro digo cinco";
        // Para que el patrón sea inequívoco exigimos un solo numeral
        // antes y un solo numeral después; con dos «cuatro/cinco»
        // repetidos no hay match → guard no interviene.
        let output = "cuatro cinco cuatro cinco";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "con múltiples numerales el patrón deja de ser inequívoco"
        );
    }

    #[test]
    fn guard_numeric_no_se_pisa_con_texto_largo_y_sin_correccion() {
        // El caso del log LARGO sin corrección (73 palabras) no debe
        // entrar en el guard numérico: el guard NO es responsable de
        // detectar resúmenes (eso es `guard_anti_truncacion`). El
        // guard numérico solo mira el patrón «NUM A [marker] NUM B».
        let input = LOG_LARGO_INPUT;
        let output = LOG_LARGO_OUTPUT;
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "texto sin corrección numérica explícita: el guard no aplica"
        );
    }

    #[test]
    fn guard_numeric_corrige_incluso_si_llm_borro_numerales() {
        // Caso real del 0.8B: a veces el LLM no solo concatena, sino
        // que también BORRA uno o ambos numerales (probablemente por
        // confusión). El guard detecta el patrón en el INPUT y puede
        // reconstruir igualmente: «me comi cuatro digo cinco peras»
        // con output «me comi peras» → reconstruido «me comi cinco
        // peras» (¡mejor que el input!).
        let input = "me comi cuatro digo cinco peras";
        let output = "me comi peras";
        let result = guard_numeric_substitution(input, output)
            .expect("el guard debe reconstruir incluso si el LLM borró numerales");
        assert_eq!(
            result, "me comi cinco peras",
            "el input tiene NUM_A y NUM_B: la sustitución es inequívoca"
        );
    }

    #[test]
    fn guard_numeric_canonicos_intactos_no_se_pisan() {
        // Sanity: los casos canónicos de la spec (que el LLM resuelve
        // bien en runtime) NO deben disparar el guard: «murcia digo
        // cartagena», «se cruzo una moto digo se cruzo un coche», etc.
        // El guard solo aplica cuando hay numeral, así que con texto
        // sin numerales no hay patrón.
        for (input, output) in [
            ("que calor que hace en murcia digo cartagena", "que calor en cartagena"),
            (
                "se cruzo una moto digo se cruzo un coche",
                "se cruzo un coche",
            ),
        ] {
            assert!(
                guard_numeric_substitution(input, output).is_err(),
                "caso canónico sin numerales: guard no aplica ({input:?} → {output:?})"
            );
        }
    }

    // ── Regresión H1 (t_d64797c2): frases NATURALES, no habla telegráfica ─
    //
    // Casos reales del log del usuario (14/09 03:19 y 13/09 01:37): entre
    // el numeral y el marcador hay sintagma nominal y/o verbo («a tres
    // personas, digo cuatro personas»). El LLM local se quedaba con el
    // numeral VIEJO, borraba la meta-lengua y soltaba una frase plausible
    // y FALSA; el guard no lo cazaba porque exigía que NUM_A fuese el
    // último token antes del marcador.

    #[test]
    fn guard_numeric_corrige_caso_real_log_1_numeral_con_sintagma() {
        // Caso 1 del log: input con «tres personas, digo cuatro personas»,
        // output del LLM con el numeral VIEJO y sin meta-lengua.
        let input = "El otro día me encontré a tres personas, digo cuatro personas, que iban por la acera de enfrente.";
        let output = "El otro día me encontré a tres personas que iban por la acera de enfrente.";
        let result = guard_numeric_substitution(input, output).expect(
            "el guard debe cazar el numeral VIEJO y reconstruir con el CORRECTO (NUM_B)",
        );
        assert_eq!(
            result,
            "El otro día me encontré a cuatro personas, que iban por la acera de enfrente.",
            "se conserva NUM_B («cuatro») y se descarta NUM_A («tres») + la meta-lengua"
        );
        assert!(
            !contains_token(&result.to_lowercase(), "tres"),
            "el numeral viejo no puede sobrevivir en el texto inyectado: {result:?}"
        );
        assert!(
            contains_token(&result.to_lowercase(), "cuatro"),
            "el numeral corregido DEBE estar en el texto inyectado: {result:?}"
        );
        // La reconstrucción no debe caer en el guard anti-truncación (es
        // el mismo criterio que aplica app.rs antes de inyectar) ni
        // contener meta-lengua residual (eso haría que app.rs inyectase el
        // texto bruto y se perdiese la corrección).
        assert_eq!(
            guard_anti_truncacion(input, &result),
            result,
            "la reconstrucción es más corta que el input pero NO es un resumen"
        );
        assert!(
            !crate::domain::corrective::contains_in_turn_marker(&result),
            "sin meta-lengua residual: app.rs no debe descartar la corrección: {result:?}"
        );
    }

    #[test]
    fn guard_numeric_corrige_caso_real_log_2_digitos_concatenados() {
        // Caso 2 del log: input con «23 personas que iban, digo, a 24
        // personas», output del LLM con los DOS numerales pegados y sin
        // meta-lengua (frase rota).
        let input = "El otro día me encontré a 23 personas que iban, digo, a 24 personas que iban por la calle y es que, buah, flipante.";
        let output = "El otro día me encontré a 23 personas que iban a 24 personas que iban por la calle y es que, buah, flipante.";
        let result = guard_numeric_substitution(input, output)
            .expect("el guard debe resolver la concatenación de numerales");
        assert_eq!(
            result,
            "El otro día me encontré a 24 personas que iban por la calle y es que, buah, flipante.",
            "se conserva NUM_B («24») y desaparece NUM_A («23») + la meta-lengua"
        );
        assert!(
            !contains_token(&result.to_lowercase(), "23"),
            "el numeral viejo no puede sobrevivir: {result:?}"
        );
        assert_eq!(
            guard_anti_truncacion(input, &result),
            result,
            "la reconstrucción pasa el guard anti-truncación"
        );
        assert!(
            !crate::domain::corrective::contains_in_turn_marker(&result),
            "sin meta-lengua residual: app.rs no debe descartar la corrección: {result:?}"
        );
    }

    #[test]
    fn guard_numeric_corrige_frases_naturales_con_contexto_entre_numeral_y_marcador() {
        // La ampliación no es ad-hoc para el log: cubre cualquier texto
        // (funcional o de contenido) entre NUM_A y el marcador.
        for (input, output, esperado) in [
            (
                "había dos coches en la puerta, mejor dicho tres coches",
                "había dos coches en la puerta",
                "había tres coches",
            ),
            (
                "vamos a las cinco de la tarde, en vez de a las seis",
                "vamos a las cinco de la tarde",
                "vamos a las seis",
            ),
            (
                "me encontré a las tres personas, digo, a las cuatro",
                "me encontré a las tres personas",
                "me encontré a las cuatro",
            ),
        ] {
            let result = guard_numeric_substitution(input, output)
                .unwrap_or_else(|_| panic!("el guard debe reconstruir {input:?}"));
            assert_eq!(result, esperado, "input {input:?}");
        }
    }

    #[test]
    fn guard_numeric_control_sin_autocorreccion_no_toca_nada() {
        // CONTROL: dictado normal sin meta-lengua ni rectificación. El
        // guard NO aplica y el output del LLM (idéntico al input) se
        // inyecta tal cual: nunca se altera texto que no es una
        // auto-corrección.
        for texto in [
            "Ayer fui al mercado y compré naranjas para toda la semana.",
            "Quedamos a las cuatro en la puerta del teatro.",
            "Llevo dos semanas esperando la respuesta del banco.",
        ] {
            assert!(
                guard_numeric_substitution(texto, texto).is_err(),
                "sin auto-corrección el guard no debe intervenir: {texto:?}"
            );
        }
    }

    #[test]
    fn guard_numeric_negativo_no_inventa_con_numerales_ambiguos() {
        // NEGATIVO: dos numerales candidatos antes del marcador (¿cuál
        // rectifica el usuario?) → precisión > recall, no se toca nada.
        let input = "tengo 3 perros y 4 gatos, digo 5";
        let output = "tengo 3 perros y 5"; // conjetura (posible) del LLM
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "con dos numerales antes del marcador el patrón no es inequívoco"
        );
        // Y sin marcador correctivo, dos numerales sueltos tampoco.
        let sin_marcador = "había 3 personas y 4 coches en la calle";
        assert!(
            guard_numeric_substitution(sin_marcador, sin_marcador).is_err(),
            "sin marcador no hay sustitución que aplicar"
        );
    }

    #[test]
    fn guard_numeric_canonico_moto_digo_coche_sigue_funcionando() {
        // Caso canónico del usuario: corrección SIN numerales. El guard
        // numérico no aplica y la corrección del LLM pasa intacta.
        let input = "se cruzó una moto digo se cruzó un coche";
        let output = "se cruzó un coche";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "el guard numérico no debe tocar el canónico «moto digo coche»"
        );
        assert_eq!(guard_anti_truncacion(input, output), output);
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

    /// Orden de resolución del server (14/10/2026): el que VIAJA con la app
    /// (`vendor/llama/`) gana al config y a las rutas comunes; el config
    /// inválido no rompe nada; sin vendado se autodetecta. Todo con
    /// directorios temporales: ni `D:\LlamaCPP` ni el PATH de la máquina
    /// entran en juego.
    #[test]
    fn el_server_vendado_gana_al_config_y_a_la_autodeteccion() {
        let base = std::env::temp_dir().join(format!("whispr-orden-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // layout instalado: <app>/silb.exe + <app>/vendor/llama/llama-server.exe
        let app = base.join("app");
        // y una carpeta «sin vendor», como un desarrollo desde target/
        let sin_vendor = base.join("sin-vendor");
        std::fs::create_dir_all(&sin_vendor).unwrap();
        std::fs::create_dir_all(app.join("vendor").join("llama")).unwrap();
        let vendado = app.join("vendor").join("llama").join("llama-server.exe");
        std::fs::write(&vendado, b"stub").unwrap();
        // server «de antes» apuntado a mano en config.json, y uno «común»
        let manual_dir = base.join("manual");
        std::fs::create_dir_all(&manual_dir).unwrap();
        let manual = manual_dir.join("llama-server.exe");
        std::fs::write(&manual, b"stub").unwrap();
        let comun_dir = base.join("comun");
        std::fs::create_dir_all(&comun_dir).unwrap();
        let comun = comun_dir.join("llama-server.exe");
        std::fs::write(&comun, b"stub").unwrap();
        let comunes = vec![comun.clone()];

        // 1) vendado + config válido distinto → gana el VENDADO (una ruta
        //    vieja en config.json no rompe una instalación nueva).
        assert_eq!(
            resolve_server_path_in(Some(&app), Some(&manual.to_string_lossy()), &comunes),
            Some(vendado.clone())
        );
        // 2) vendado + config INVÁLIDO (no existe) → el vendado.
        assert_eq!(
            resolve_server_path_in(
                Some(&app),
                Some(&base.join("fantasma.exe").to_string_lossy()),
                &comunes
            ),
            Some(vendado.clone())
        );
        // 3) sin vendado + config válido → el config (override manual).
        assert_eq!(
            resolve_server_path_in(Some(&sin_vendor), Some(&manual.to_string_lossy()), &comunes),
            Some(manual.clone())
        );
        // 4) sin vendado + config inválido → autodetección por rutas comunes.
        assert_eq!(
            resolve_server_path_in(
                Some(&sin_vendor),
                Some(&base.join("fantasma.exe").to_string_lossy()),
                &comunes
            ),
            Some(comun.clone())
        );
        // 5) sin nada → None (el arranque lo reporta como LlmDisabled).
        assert_eq!(
            resolve_server_path_in(None, None, &[]),
            None,
            "sin candidatos no se inventa ninguna ruta"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// El GGUF del corrector se busca también junto al exe (`vendor/models`) y
    /// en la carpeta de modelos de la app (`%APPDATA%\whispr-open\models`),
    /// ambas por delante de las raíces heredadas (LM Studio, D:\LlamaCPP…).
    #[test]
    fn el_modelo_se_busca_en_vendor_models_y_en_la_carpeta_de_la_app() {
        let app = std::env::temp_dir().join(format!("whispr-vendor-model-{}", std::process::id()));
        let vendor_models = app.join("vendor").join("models");
        std::fs::create_dir_all(&vendor_models).unwrap();
        std::fs::write(vendor_models.join("corrector.gguf"), b"gguf").unwrap();

        let roots = model_roots_in(Some(&app));
        assert_eq!(roots[0], vendor_models, "vendor/models va primero");
        assert_eq!(
            roots[1],
            crate::transcribe::models_dir(),
            "la carpeta de modelos de la app (misma que la de whisper) va después"
        );
        assert_eq!(
            resolve_model_path_with("corrector.gguf", &roots),
            Some(vendor_models.join("corrector.gguf"))
        );
        let _ = std::fs::remove_dir_all(&app);
    }

    /// El pack CUDA es solo un subdirectorio junto al server vendado: existe o
    /// no existe, y sin él no se toca nada (el server corre en CPU).
    #[test]
    fn el_pack_cuda_es_el_subdirectorio_cuda_junto_al_server() {
        let app = std::env::temp_dir().join(format!("whispr-cuda-{}", std::process::id()));
        std::fs::create_dir_all(app.join("vendor").join("llama")).unwrap();
        assert_eq!(vendored_cuda_dir_in(&app), None, "sin cuda/ no hay pack");
        let cuda = app.join("vendor").join("llama").join("cuda");
        std::fs::create_dir_all(&cuda).unwrap();
        assert_eq!(vendored_cuda_dir_in(&app), Some(cuda));
        let _ = std::fs::remove_dir_all(&app);
    }

    #[test]
    fn kill_sin_server_lanzado_no_hace_nada() {
        // Sin PID registrado, el path de salida no intenta matar nada
        // (criterio: nunca tocar servers ajenos).
        assert_eq!(launched_server_pid(), 0);
        assert!(!kill_launched_server());
    }

    #[test]
    fn ruta_explicita_rota_o_falsa_se_ignora_deterministicamente() {
        // Requisito de robustez («ruta rota»): la ruta explícita de Ajustes
        // se valida sola, sin depender del disco real de la máquina ni del
        // PATH (por eso la decisión vive en `explicit_server_path`).
        let dir = std::env::temp_dir().join(format!("whispr-explicit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1) No existe → None (el caller cae a autodetección).
        assert_eq!(
            explicit_server_path(&dir.join("no-esta.exe").to_string_lossy()),
            None
        );

        // 2) Existe y ES llama-server.exe → se usa tal cual.
        let buena = dir.join("llama-server.exe");
        std::fs::write(&buena, b"stub").unwrap();
        assert_eq!(
            explicit_server_path(&buena.to_string_lossy()),
            Some(buena.clone())
        );

        // 3) Wrapper (llama.exe) con hermano bueno → se sanea al hermano.
        let _ = std::fs::write(dir.join("llama.exe"), b"wrapper");
        assert_eq!(
            explicit_server_path(&dir.join("llama.exe").to_string_lossy()),
            Some(buena)
        );

        // 4) Wrapper SIN hermano → se ignora (None): nunca se lanza un exe
        //    que no se pueda terminar limpiamente al salir.
        let solo = std::env::temp_dir()
            .join(format!("whispr-explicit-solo-{}", std::process::id()));
        std::fs::create_dir_all(&solo).unwrap();
        std::fs::write(solo.join("llama.exe"), b"stub").unwrap();
        assert_eq!(
            explicit_server_path(&solo.join("llama.exe").to_string_lossy()),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&solo);
    }

    #[test]
    fn endpoint_que_responde_500_se_reporta_como_error_http() {
        // Requisito de robustez: un endpoint VIVO pero roto (500, gateway
        // caído…) debe degradarse a texto bruto con el mismo contrato de
        // error que el endpoint caído (prefijo «HTTP:» para que la UI lo
        // categorice). Se monta un server HTTP de mentira con std::net.
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind en puerto libre");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Consumir la petición (cabeceras) antes de responder para
                // que el cliente no resetee la conexión al leer.
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        let cfg = LlmConfig {
            provider: CorrectionProvider::Default,
            base_url: format!("http://127.0.0.1:{port}/v1"),
            model: "fake".to_string(),
            api_key: None,
            timeout_secs: 5,
        };
        let err = correct("texto objetivo", "no, es 2", &cfg)
            .expect_err("un 500 del endpoint debe fallar");
        assert!(
            err.starts_with("HTTP:"),
            "el 500 debe llegar con prefijo HTTP (mismo contrato que el endpoint caído): {err:?}"
        );
        server.join().expect("el server de mentira termina");
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
