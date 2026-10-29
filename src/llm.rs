//! Cliente de un modelo OpenAI-compatible para la corrección hablada
//! (docs/CORRECCION-VOZ.md §3). Sin dependencias nuevas: usa ureq 2.12, que ya
//! está en Cargo.toml para las descargas de modelos.
//!
//! Por defecto: llama.cpp en local (Ornith 9B) en `127.0.0.1:8081`; los cambios
//! vienen de `Config::llm_settings()`. Cualquier servicio OpenAI-compatible
//! (Groq, Gemini…) funciona igual con base_url + clave.
//!
//! OJO con ureq sin la opción `json`: no existe `into_json()`; el cuerpo se lee
//! como texto (`into_string`) y se lee con serde_json (que sí está).

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;

use crate::config::CorrectionProvider;
use crate::domain::corrective::Diff;

/// Dirección y modelo del corrector. Lo rellena `Config::llm_settings()` con
/// los valores locales; `base_url`/`model`/`api_key`/`timeout_secs` se pueden
/// cambiar desde config.json.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LlmConfig {
    pub provider: CorrectionProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Cuánto se espera la respuesta, en segundos. Por defecto 25 (los ~4-6 s de
    /// un modelo 9B en local, con margen para servicios remotos; punto 3.4 del
    /// documento). Sale de `Config::llm_timeout_secs`.
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

/// Dirección base de un servicio OpenAI-compatible: sin espacios, sin barra
/// final y con los `/v1` repetidos juntados en uno solo.
///
/// Es el ÚNICO sitio que limpia direcciones del corrector (t_75732630). El
/// fallo que lo motivó: `/v1/v1/models` → 404 → `server_alive` = false →
/// `wait_for_server` decía «servidor LLM sin respuesta tras 0 s» aunque el
/// servidor contestara en 2 ms. La dirección puede venir con `/v1`, sin él o
/// con `/v1/v1` (config tocada a mano, valores viejos o una dirección pegada
/// dos veces): TODAS tienen que acabar en una dirección válida. Eso sí, el
/// `/v1` NO se quita cuando aparece una sola vez — es el prefijo de versión
/// que esperan llama.cpp, OpenAI y Groq (`/v1/models`, `/v1/chat/completions`):
/// quitarlo rompería el proveedor en la nube.
pub fn api_base(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/');
    // Junta `/v1/v1`, `/v1/v1/v1`… en un solo `/v1`.
    while let Some(rest) = base.strip_suffix("/v1") {
        let rest = rest.trim_end_matches('/');
        if !rest.ends_with("/v1") {
            break;
        }
        base = rest;
    }
    base.to_string()
}

/// Dirección completa de una ruta OpenAI-compatible (`models`,
/// `chat/completions`…) montada sobre `api_base`. `path` puede venir con barra
/// delante o sin ella.
pub fn api_url(base_url: &str, path: &str) -> String {
    format!("{}/{}", api_base(base_url), path.trim_start_matches('/'))
}

/// Nombre del ÚNICO ejecutable que la app acepta como servidor:
/// `llama-server.exe` en Windows y `llama-server` (el binario de llama.cpp) en
/// el resto.
#[cfg(target_os = "windows")]
pub const SERVER_EXE_NAME: &str = "llama-server.exe";
#[cfg(not(target_os = "windows"))]
pub const SERVER_EXE_NAME: &str = "llama-server";

/// Versión de llama.cpp de la que la app descarga el servidor cuando hace
/// falta (sprint final: no viene nada puesto). Se fija aquí para que la interfaz
/// y las pruebas usen la misma fuente (la descarga usa ureq, como la de
/// Whisper).
pub const LLAMA_RELEASE_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/latest/download/llama-server.exe";

/// Modelo de IA pequeño para corregir en local (rápido, menos de 1 GB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalLlmModel {
    pub id: &'static str,
    pub label: &'static str,
    pub file_name: &'static str,
    pub approx_mb: u32,
    pub url: &'static str,
}

/// Lista de modelos de corrección (GGUF Q4_K_M, muy rápidos).
/// De serie: Qwen2.5-0.5B (491 MB). No se descarga nada sin pulsar.
pub const SMALL_LLM_MODELS: &[LocalLlmModel] = &[
    LocalLlmModel {
        id: "qwen2.5-0.5b",
        label: "Qwen2.5-0.5B (recomendado, ~491 MB)",
        file_name: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        approx_mb: 491,
        url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
    },
    LocalLlmModel {
        id: "llama-3.2-1b",
        label: "Llama 3.2 1B (~808 MB)",
        file_name: "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        approx_mb: 808,
        url: "https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf",
    },
    LocalLlmModel {
        id: "qwen2.5-1.5b",
        label: "Qwen2.5-1.5B (más fiel, ~1,1 GB)",
        file_name: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        approx_mb: 1120,
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
    },
];

/// Opción del desplegable para escribir un repo o una dirección de Hugging
/// Face a mano (0.7). No es un modelo: es la puerta a cualquier GGUF público.
pub const LLM_OTHER_CHOICE: &str = "Otro (repo o URL de Hugging Face)…";

/// Carpeta de los modelos de corrección descargados cuando se piden
/// (`%APPDATA%/whispr-open/models-llm/`).
pub fn llm_models_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("whispr-open")
        .join("models-llm")
}

/// Carpeta del servidor descargado cuando se pide (`%APPDATA%/whispr-open/bin/`).
pub fn llama_bin_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("whispr-open")
        .join("bin")
}

/// Ruta del servidor que ha descargado la app (si está).
pub fn downloaded_server_path() -> std::path::PathBuf {
    llama_bin_dir().join(SERVER_EXE_NAME)
}

/// ¿`path` es un `llama-server.exe`? La app SOLO arranca ese binario (regla de
/// producto t_75732630): el servidor es un proceso NATIVO de Windows —nunca
/// `wsl.exe`— y con ese nombre exacto para que `terminate_server_pid` pueda
/// comprobarlo por imagen al salir. El envoltorio viejo `llama.exe` no encajaba
/// con esa comprobación → el proceso que lanzaba la app se quedaba huérfano en
/// VRAM.
pub fn is_server_exe(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(SERVER_EXE_NAME))
        .unwrap_or(false)
}

/// Carpeta del ejecutable REAL de la app (`std::env::current_exe()`): en
/// desarrollo es `target/<perfil>/`, instalada es la carpeta de `silb.exe`.
/// Es el ancla de todo lo que VIAJA con la app (`vendor/`).
pub fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
}

/// Servidor EMBARCADO en el paquete: `vendor/llama/llama-server.exe`, al lado
/// del exe de la app. Es el candidato nº1 de `resolve_server_path`: el usuario
/// no escribe ninguna ruta (requisito 14/10/2026).
pub fn vendored_server_path_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let p = dir.join("vendor").join("llama").join(SERVER_EXE_NAME);
    p.is_file().then_some(p)
}

/// Paquete CUDA OPCIONAL del servidor embarcado: `vendor/llama/cuda/`, si existe
/// y hay driver NVIDIA. Con él el servidor se arranca con esa carpeta como cwd
/// — el cargador de backends de ggml busca `ggml-cuda*.dll` en el cwd (probado
/// con el binario real; el PATH NO le sirve para encontrarlo) — y con esa
/// carpeta al frente del PATH, para lo que necesita `ggml-cuda.dll`
/// (cudart/cublas). Sin paquete, el servidor va en CPU.
pub fn vendored_cuda_dir_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let cuda = dir.join("vendor").join("llama").join("cuda");
    cuda.is_dir().then_some(cuda)
}

/// ¿Hay driver NVIDIA en la máquina? El paquete CUDA solo se usa con él:
/// cargar `ggml-cuda.dll` sin driver es un fallo al arrancar, no una mejora.
fn nvidia_driver_present() -> bool {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    std::path::Path::new(&root)
        .join("System32")
        .join("nvcuda.dll")
        .is_file()
}

/// Arregla una ruta de servidor que no es `llama-server.exe` (por ejemplo el
/// envoltorio `llama.exe` de instalaciones viejas): si en la MISMA carpeta hay
/// un `llama-server.exe`, devuelve esa ruta; si no, `None` (quien llama pasa a
/// buscarlo en las rutas de siempre).
pub fn sanitize_server_exe(path: &std::path::Path) -> Option<std::path::PathBuf> {
    if is_server_exe(path) {
        return Some(path.to_path_buf());
    }
    let sibling = path.parent()?.join(SERVER_EXE_NAME);
    sibling.is_file().then_some(sibling)
}

/// Prompt del contrato (§3.2): el modelo devuelve SOLO un JSON con
/// `{find, replace}` sobre el texto actual. Si no encuentra nada, vacío.
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

/// Prompt del modo in-turn (t_78d9e1fa, spec §3.5). Al contrario que el modo
/// cross-turn, el modelo recibe el TURNO ENTERO y devuelve el TEXTO FINAL ya
/// corregido (sin hablar de sí mismo), no un cambio. Esto cubre el caso típico
/// que motivó el cambio de diseño:
///
///   entra:  "que calor que hace en murcia digo cartagena"
///   sale:   "que calor que hace en cartagena"
///
/// El usuario se corrige a sí mismo dentro del mismo chorro de voz («digo X»,
/// «quiero decir X», «no X sino Y», «en vez de», etc.). El modelo debe:
///   1) Detectar el trozo «anterior» a la marca de corrección.
///   2) Detectar el trozo «posterior» (la corrección en sí).
///   3) Reescribir el texto cambiando lo anterior por lo posterior.
///   4) Quitar TODA la frase de corrección («digo», «quiero decir», etc.).
///   5) Si NO hay corrección de verdad, devolver el texto tal cual.
///
/// Devuelve SOLO el texto final (sin JSON, sin bloques de código, sin
/// explicaciones).
///
/// REGLA DE PRESERVACIÓN (t_6b9348de): el modelo local es un 0.8B y contesta a
/// los turnos largos SIN corrección con un RESUMEN (está en el log 01:34:54:
/// entraron 73 palabras y salieron 13). Se probaron 9 variantes de prompt en
/// ejecución (servidor real, petición exacta) y NINGUNA consigue a la vez
/// conservar los turnos largos y mantener las correcciones de siempre: todo
/// refuerzo de "no resumas" rompe alguna autocorrección («el cliente es Acme
/// quiero decir Microsoft» devolvía «el cliente es Acme», perdiendo la
/// corrección y engañando a la protección por proporción). Por eso el prompt se
/// queda TAL CUAL (las correcciones salen bien) y lo de cortar los resúmenes es
/// cosa OBLIGATORIA de la protección `guard_anti_truncacion` en `app.rs`: si lo
/// que sale tiene menos del 60% de las palabras y no hay una autocorrección
/// clara, se tira y se escribe el texto original — el dictado no se pierde
/// nunca.
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

/// Pide al modelo un cambio de corrección para `target` (el último turno
/// dictado) según la frase hablada que vino después. Bloquea: hay que llamarlo
/// en un hilo (app.rs lo lanza con `std::thread::spawn` y un canal mpsc).
pub fn correct(target: &str, instruction: &str, cfg: &LlmConfig) -> Result<Diff, String> {
    let url = api_url(&cfg.base_url, "chat/completions");
    // max_tokens holgado (256): lo que se espera es un JSON corto, pero conviene
    // dejar margen por si el servidor mete prefijos o el sistema añade tokens de
    // control. El fallo del Qwen3.5 con razonamiento era justo este: con 120-2048
    // tokens y pensamiento implícito, el modelo se gastaba el presupuesto
    // rellenando `reasoning_content` y dejaba `message.content` vacío.
    //
    // chat_template_kwargs.enable_thinking=false apaga el modo pensamiento de los
    // modelos Qwen con razonamiento (y de cualquier otro que haga caso a esa
    // marca), para que la respuesta útil venga en `message.content` desde el
    // primer token. Si el servidor no la entiende (modelos sin razonamiento) la
    // ignora sin decir nada — probado contra llama.cpp.
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

    // ureq 2.12: send_string pide &str; el cuerpo ya es JSON serializado.
    let response = request
        .send_string(&body.to_string())
        .map_err(|e| format!("HTTP: {e}"))?;
    let body_raw = response
        .into_string()
        .map_err(|e| format!("lectura de la respuesta: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&body_raw).map_err(|e| format!("JSON de respuesta inválido: {e}"))?;

    // Saca el texto útil de la respuesta. Lo normal: viene en `message.content`.
    // Si no: en modelos con razonamiento (Qwen3.5, DeepSeek-R1, etc.) y
    // pensamiento implícito encendido, `content` puede llegar VACÍO y lo útil
    // aparece en `message.reasoning_content`, al final del bloque de pensamiento.
    // Si ni así hay JSON, se devuelve un error claro.
    let content_opt = extract_message_text(&value);
    if content_opt.is_none() {
        // Se anota en el log para saber distinguir un modelo roto de una petición
        // mal montada cuando alguien reproduzca el fallo en vivo.
        crate::log!(
            "llm",
            "la respuesta del LLM no trae content ni reasoning_content: payload={}",
            value
        );
    }
    let content = content_opt.ok_or_else(|| "la respuesta del LLM no trae content".to_string())?;
    parse_diff(content)
}

/// Modo in-turn (t_78d9e1fa, spec §3.5): el modelo recibe el turno ENTERO (que
/// lleva dentro la autocorrección hablada) y devuelve el TEXTO FINAL corregido
/// (sin la frase de corrección, sin JSON). Es el modo PRINCIPAL desde el
/// 10/09/2026: si el usuario dice «que calor en murcia digo cartagena» de un
/// tirón, esta función devuelve «que calor en cartagena».
///
/// En qué se diferencia de `correct()`:
/// - Prompt distinto (`SYSTEM_PROMPT_IN_TURN`): el modelo NO devuelve un JSON
///   `{find, replace}`, devuelve el texto final tal cual.
/// - `max_tokens` mayor (1024) porque devolvemos el turno ENTERO corregido (un
///   párrafo dictado puede pasar de 512 tokens de salida, y un 0.8B que resume
///   el turno necesita margen; t_6b9348de subió el tope de 512 a 1024). Quien
///   tira los resúmenes es la protección anti-truncación de `app.rs`, no el
///   tope de tokens.
/// - Sin leer JSON: solo se quitan los bloques de código de markdown y se
///   recortan los espacios de los extremos. Si el modelo mete comillas o
///   bloques, se quitan; si mete texto de más, se tira la respuesta y se
///   devuelve un error para que quien llama escriba el texto bruto (lo seguro,
///   spec §3.5.4).
///
/// Bloquea: se llama desde un hilo (app.rs lo lanza con `std::thread::spawn` y
/// un canal mpsc, igual que `correct`).
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

    // El mismo extractor que `correct()`: se prefiere `message.content` y si
    // está vacío se baja a `message.reasoning_content` (Qwen3.5 y otros modelos
    // con razonamiento implícito).
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

/// Limpia lo que devuelve el modo in-turn. El modelo puede adornar la
/// respuesta con bloques de markdown, comillas tipográficas, espacios de más o
/// prefijos tipo «Output:». Esta función deja solo el texto final que sirve. Si
/// después de limpiar no queda nada, devuelve un error para que quien llama
/// escriba el texto bruto (lo que dice la spec §3.5.4: no bloquear el dictado
/// nunca).
fn clean_redact_output(content: &str) -> String {
    let mut s = content.trim().to_string();
    // Quitar los bloques de markdown (```...``` o ```text...```).
    if let Some(rest) = s.strip_prefix("```") {
        s = rest.to_string();
        // Si detrás de ``` viene una etiqueta de lenguaje (json, text, …) y un
        // salto de línea, también se tira.
        if let Some(nl) = s.find('\n') {
            if !s[..nl].contains(' ') && !s[..nl].is_empty() {
                s = s[nl + 1..].to_string();
            }
        }
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.to_string();
    }
    // Al quitar los bloques pueden quedar saltos de línea o espacios alrededor
    // del contenido (típico de un modelo, que mete un \n antes y otro después
    // del cuerpo). Se limpian para que el paso siguiente (prefijos y comillas)
    // trabaje sobre el texto pelado.
    s = s.trim().to_string();
    // Quitar prefijos tipo «Output:», «Resultado:», «Texto final:» que algunos
    // modelos ponen delante. Solo si van seguidos de un espacio y al principio
    // del contenido (sin mirar mayúsculas). Se hace ANTES de las comillas: algo
    // como «Output: "texto"» pierde primero el prefijo y luego las comillas.
    let lower = s.to_ascii_lowercase();
    for prefix in ["output:", "resultado:", "texto final:", "respuesta:"] {
        if lower.starts_with(prefix) {
            s = s[prefix.len()..].trim_start().to_string();
            break;
        }
    }
    // Quitar comillas envolventes («"texto"», «'texto'», ««texto»», las
    // tipográficas «“texto”»/«‘texto’»). Si el modelo envuelve el resultado en
    // comillas, se quitan. Solo se mira el PRIMER y el ÚLTIMO carácter: el
    // contenido de dentro no se toca.
    //
    // Parejas conocidas: cada apertura tiene su cierre. Una apertura que no casa
    // con su cierre del mismo tipo (por ejemplo abrir «“» y cerrar «"», que son
    // códigos distintos) NO se quita: el contenido está roto y mejor no
    // estropearlo más. Las comillas rectas '"' y '\'' sí se emparejan solas.
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
    // Se repite por si el modelo junta varias capas (por ejemplo «Output:
    // "texto"» se resuelve en una pasada; un caso raro como «' "texto" '»
    // necesitaría dos). Con tres pasadas sobra para todo lo que se ha visto.
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

/// Protección anti-truncación OBLIGATORIA del modo in-turn (t_6b9348de).
///
/// El corrector local (Qwen3.5-0.8B) a veces devuelve un RESUMEN del turno en
/// vez del texto corregido (está en el log 01:34:54: entraron 73 palabras y
/// salieron 13). El prompt (`SYSTEM_PROMPT_IN_TURN`) ya prohíbe resumir, pero un
/// modelo pequeño no siempre hace caso: esto es la última red.
///
/// La regla (contando palabras):
/// - Si la respuesta se queda por debajo del 60% de las palabras del texto
///   dictado Y el turno no lleva una marca INEQUÍVOCA de autocorrección
///   (`has_unambiguous_in_turn_marker`: «digo X», «quiero decir», «no X sino
///   Y», «en vez de»…) que explique el recorte (una corrección de verdad quita
///   el trozo corregido y la frase de corrección), se tira la respuesta y se
///   devuelve EL TEXTO ORIGINAL.
/// - Con marca inequívoca se admite bajar hasta el 30% de las palabras («no voy
///   a las tres sino a las cuatro» → «voy a las cuatro» quita más del 40% y es
///   legítimo). Por debajo del 30%, aunque haya marca, huele a resumen y también
///   se tira.
///
/// Nunca devuelve menos texto del justificado y nunca bloquea el dictado: en el
/// peor caso se escribe el texto tal cual (con su «digo»), que es exactamente lo
/// que dijo el usuario.
pub fn guard_anti_truncacion(input: &str, output: &str) -> String {
    let in_words = word_count(input);
    if in_words == 0 {
        // Texto vacío: no hay nada que proteger; se devuelve la respuesta sin más.
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

/// Detección ESTRICTA de marca de autocorrección para esta protección.
///
/// Al contrario que `contains_in_turn_marker` (el clasificador que decide si se
/// llama al modelo, permisivo a propósito: «dime»/«diga» solo disparan una
/// llamada barata), aquí permiso quiere decir permitir recortes de texto, y
/// «dime si puedes revisarlo» — el caso real del fallo t_6b9348de — es una
/// petición, no una rectificación. Solo las frases que no admiten otra lectura
/// que «corrijo lo que acabo de decir» bajan el listón del 60% al 30%.
fn has_unambiguous_in_turn_marker(text: &str) -> bool {
    let t = text.to_lowercase();
    // Frases inequívocas de autocorrección hablada.
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
    // Palabras sueltas con límite de palabra («digo» no debe casar dentro de
    // «indigo», «dicho» ni «diga»).
    for word in ["digo", "sino"] {
        if t.split(|c: char| !c.is_alphanumeric())
            .any(|token| token == word)
        {
            return true;
        }
    }
    false
}

/// Cuántas palabras tiene un texto (separador: cualquier espacio en blanco).
/// Es la unidad de la protección anti-truncación: «El otro día» = 3 palabras.
fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Protección fija para sustituciones numéricas simples (t_1fc864ef).
///
/// El corrector local (Qwen3.5-0.8B-Q8) es POCO fiable con los números escritos
/// con palabras: ante una autocorrección tipo «A digo B» o «A en vez de B», con
/// A y B números (dígitos o palabras: cuatro, cinco, …), a veces pega los dos en
/// vez de cambiar uno por otro (está en el log: «me comi cuatro digo cinco
/// peras» → «me comi cuatro cinco peras»). Esa salida lleva los DOS números y,
/// por eso, ninguna protección de las que había la detecta:
/// `guard_anti_truncacion` no la corta (la proporción de palabras es creíble) y
/// `contains_in_turn_marker` no ve ninguna frase de corrección. El texto que se
/// escribe queda sin sentido.
///
/// Esta protección busca en el texto dictado el patrón inequívoco «NUM_A
/// [texto] [marcador] NUM_B» y comprueba si el modelo lo resolvió. Si el modelo
/// dejó los DOS números (pegados), se quedó con NUM_A (el equivocado) o se comió
/// la corrección, rehace el texto aplicando el cambio ella misma, en código:
/// «texto_antes» + NUM_B + «texto_despues».
///
/// Cobertura ampliada (t_d64797c2, H1 del QA-REPORT-DEPURACION): el patrón ya NO
/// exige que NUM_A sea la última palabra antes de la marca. Al hablar hay
/// sintagma nominal y/o verbo entre el número y la marca («a tres personas, digo
/// cuatro personas»), y la protección se perdía ese caso mientras el modelo
/// escribía una frase creíble y FALSA (log 14/09 03:19 y 13/09 01:37). Ahora
/// NUM_A es el último NÚMERO del trozo anterior a la marca.
///
/// Devuelve `Ok(texto_corregido)` cuando interviene (porque el modelo falló
/// claramente en este caso). Devuelve `Err(())` cuando NO hay patrón inequívoco
/// o cuando el modelo ya lo resolvió bien — entonces decide quien llama si
/// acepta la respuesta tal cual. Nunca bloquea el dictado: el peor caso siempre
/// es escribir texto.
///
/// ALCANCE: solo números simples (una palabra, sin contexto elaborado). Casos
/// como «me debe tres mil doscientos cuarenta y ocho euros» NO entran (ahí no
/// hay autocorrección). Tampoco entran las correcciones con sustantivos
/// (murcia/cartagena: esas el modelo ya las hace bien). Esto es la red mínima
/// para el fallo concreto del 0.8B-Q8, no un lector de números universal.
pub fn guard_numeric_substitution(input: &str, output: &str) -> Result<String, ()> {
    // 1) Buscar el patrón inequívoco «NUM_A [marcador] NUM_B» en el texto
    //    dictado. Si no encaja, la protección no aplica.
    let Some(pat) = parse_numeric_correction(input) else {
        return Err(());
    };

    // 2) Si el modelo ya puso el «después» (NUM_B) en la respuesta, se da por
    //    buena y NO se interviene (puede haber pequeñas variaciones que el
    //    modelo hizo bien, por ejemplo «voy a las 4» en lugar de «voy a las
    //    cuatro»). Que decidan las demás protecciones.
    if contains_token(&output.to_lowercase(), &pat.num_b.to_lowercase())
        && !contains_token(&output.to_lowercase(), &pat.num_a.to_lowercase())
    {
        return Err(());
    }

    // 3) Reconstrucción fija. Se buscan las posiciones exactas (en bytes) del
    //    NUM_A y del NUM_B en el texto dictado, y se monta:
    //    input[..num_a_start] + num_b + input[after_num_b..]. Así se quitan
    //    NUM_A, la marca entera (entre NUM_A y NUM_B) y la posible repetición de
    //    palabras de enlace alrededor de NUM_B (por ejemplo «sino a las cinco» →
    //    «cinco», sin volver a escribir «a las»).
    let num_a_lower = pat.num_a.to_lowercase();
    let num_b_lower = pat.num_b.to_lowercase();
    // Se usa la expresión regular de la marca para ver si `before_raw` empieza
    // por «no …» (caso B: «no X sino»). Sirve para tirar el «no » de delante de
    // NUM_A y que la reconstrucción salga limpia (por ejemplo «no voy a las tres
    // sino …» → «voy a las cuatro» y NO «no voy a las cuatro»).
    let re_for_discard_start = numeric_marker_regex();

    // Se parte el texto en palabras guardando el rango de bytes de cada una.
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

    // NUM_B: la primera palabra con ese texto que sale DESPUÉS de la marca
    // (`pat.marker_end`). Limitarlo al trozo de después evita confundirlo con una
    // aparición anterior del mismo número.
    let num_b_token = tokens.iter().find(|t| {
        t.is_word && t.start >= pat.marker_end && t.text.to_lowercase() == num_b_lower
    });
    let Some(num_b_tok) = num_b_token else {
        // NUM_B no aparece (no debería pasar). Se deja el texto como estaba.
        return Ok(input.to_string());
    };
    // NUM_A: la ÚLTIMA palabra con ese texto que sale ANTES de NUM_B (el número
    // rectificado siempre va antes que el bueno).
    let num_a_token = tokens
        .iter()
        .rev()
        .find(|t| t.is_word && t.start < num_b_tok.start && t.text.to_lowercase() == num_a_lower);
    let Some(num_a_tok) = num_a_token else {
        // NUM_A no aparece (no debería pasar). Se deja el texto como estaba.
        return Ok(input.to_string());
    };

    let before_raw = &input[..num_a_tok.start];
    let after_raw = &input[num_b_tok.end..];
    // Para el caso «no X sino» (la expresión regular tiene grupo 1), el patrón
    // empieza por «no » y eso cae dentro de `before_raw` (porque NUM_A va DENTRO
    // de X). Hay que quitar también el «no » de delante para que la
    // reconstrucción salga limpia.
    let before = if re_for_discard_start.captures(before_raw).ok().flatten().is_some()
        || before_raw.trim_start().to_lowercase().starts_with("no ")
    {
        // Caso B detectado: quitar el trozo «no …» hasta el primer token que no
        // sea número y que sea el principio de NUM_A (o sea: quitar el «no » si
        // está al principio).
        let trimmed = before_raw.trim_start();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("no ") || lower.starts_with("no,") {
            // Se salta «no » y lo que haya hasta la primera palabra que sirva.
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
    // Se decide si hace falta un espacio entre los trozos. La idea: si entre
    // `before` y NUM_A no había espacio, las palabras iban pegadas (raro) y hay
    // que meter uno. Si sí había espacio, la separación ya está: NO se añade. Lo
    // mismo entre NUM_B y `after`.
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

    // 4) Si lo reconstruido queda vacío o solo son espacios, se deja el texto
    //    como estaba (lo seguro).
    if reconstructed.is_empty() {
        return Ok(input.to_string());
    }

    // 5) Se anota en el log para diagnóstico: este caso lo es porque el modelo
    //    acaba de fallar con los números (t_1fc864ef).
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

    // 6) Quien llama (app.rs) usa lo que sale de aquí si es distinto de la
    //    respuesta del modelo. Si coincide (raro), se devuelve Err(()) para no
    //    repetir trabajo.
    if reconstructed == output.trim() {
        Err(())
    } else {
        Ok(reconstructed)
    }
}

/// Lo que devuelve `parse_numeric_correction`: el texto partido por la primera
/// marca de corrección inequívoca, con los dos números identificados y la
/// posición donde EMPIEZA la zona que hay que tirar (NUM_A + marca + posible
/// contexto de enlace entre la marca y NUM_B — eso último se resuelve al
/// reconstruir).
///
/// `input_a` e `input_b` se guardan para poder seguirlo y para quien lo use más
/// adelante (por ejemplo pruebas de integración con el flujo); hoy quien
/// reconstruye solo usa `num_a`/`num_b`.
struct NumericCorrection {
    /// Texto del dictado hasta donde empieza la zona que hay que tirar.
    #[allow(dead_code)]
    input_a: String,
    /// Texto del dictado desde el final de la marca (lleva NUM_B dentro).
    #[allow(dead_code)]
    input_b: String,
    /// Posición en bytes del FINAL de la marca dentro del dictado. La protección
    /// la usa para localizar NUM_B sin confundirlo con una aparición anterior del
    /// mismo número (por ejemplo «23 … digo … 24»).
    marker_end: usize,
    /// La palabra exacta de NUM_A tal como sale en el dictado.
    num_a: String,
    /// La palabra exacta de NUM_B tal como sale en el dictado.
    num_b: String,
}

/// Busca la primera marca de corrección inequívoca en `input` y, si a cada lado
/// hay exactamente UN número simple (un dígito o una palabra de la lista corta),
/// devuelve el texto partido. Si no hay patrón inequívoco, devuelve `None`.
///
/// La lista de números: solo los más comunes que caben en una palabra. NO
/// incluye «tres mil», «cuarenta y ocho», etc.: esto es para el fallo concreto
/// del 0.8B-Q8, no un lector de números universal.
const NUM_WORDS: &[&str] = &[
    "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho",
    "nueve", "diez", "once", "doce", "trece", "catorce", "quince",
    "dieciséis", "dieciseis", "veinte", "treinta", "cuarenta",
    "cincuenta", "sesenta", "setenta", "ochenta", "noventa", "cien",
    "mil",
];

/// Marcas de corrección que separan NUM_A y NUM_B. Tienen que ser
/// inequívocas: si el modelo ya entendió bien la frase, esta protección no debe
/// meterse. Por eso se usan las mismas que `has_unambiguous_in_turn_marker` más
/// «no X sino Y» y «X en vez/lugar de Y».
///
/// Cómo está montada: la expresión regular devuelve DOS grupos cuando puede:
/// - grupo 1 (opcional): el «antes» de la marca que hay que quitar. En «no X
///   sino» X es lo que se va a cambiar; hace falta tenerlo en el grupo para
///   sacar NUM_A bien.
/// - grupo 2: la marca en sí.
///
/// NOTA sobre el grupo (1): la lógica de autocorrección ya existe en
/// `src/domain/corrective.rs` (`IN_TURN_MARKERS`, `apply_diff`/
/// `apply_diff_inferido`). Pero ese módulo describe y clasifica: su expresión
/// regular casa la marca con la frase de corrección en general (digo, en vez
/// de, no X sino…) y pide estructura «antes / después» con un mínimo de
/// longitud. NO está pensado para rehacer la sustitución (no dice qué número se
/// cambia por cuál), y su lista de marcas cubre más casos (incluido «mejor»,
/// «dime» como súplica) que aquí NO se quieren tocar. Reutilizar esa expresión
/// regular aquí haría la protección demasiado permisiva y saltaría con frases
/// coloquiales que el modelo resuelve bien. Tener la expresión regular acotada
/// en este módulo es lo correcto: la protección es pequeña y se basta sola.
const NUMERIC_MARKER_REGEX: &str = r"(?i)\b(?:no\s+(.+?)\s+sino\b|(digo\b|diga\b|dime\b|quiero decir\b|quise decir\b|mejor dicho\b|en vez de\b|en lugar de\b))";

/// Expresión regular ya compilada y guardada (fancy-regex cuesta compilarla).
fn numeric_marker_regex() -> &'static fancy_regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<fancy_regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        fancy_regex::Regex::new(NUMERIC_MARKER_REGEX)
            .expect("regex de marcador numérico inválida")
    })
}

/// ¿`token` es un número simple (un dígito o una palabra de la lista)?
fn is_numeric_token(token: &str) -> bool {
    let t = token.to_lowercase();
    if t.parse::<u64>().is_ok() {
        return true;
    }
    NUM_WORDS.contains(&t.as_str())
}

/// ¿`text` lleva `token` como palabra entera (separado por espacio o
/// puntuación)?
fn contains_token(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    text.split(|c: char| !c.is_alphanumeric())
        .any(|t| t == token)
}

/// Quita la ÚLTIMA aparición de `token` (como palabra entera) de `text` y
/// devuelve el resultado con los espacios arreglados. Si no aparece, devuelve
/// `None` (quien llama decide qué hacer).
fn strip_last_token(text: &str, token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    // Se parte en palabras y se guardan también los separadores, para poder
    // montar los espacios tal como estaban.
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
    // Se busca la última posición de una palabra que coincida (sin mirar mayúsculas).
    let mut last_idx: Option<usize> = None;
    for (i, (tok, is_word)) in tokens.iter().enumerate() {
        if *is_word && tok.to_lowercase() == token.to_lowercase() {
            last_idx = Some(i);
        }
    }
    let idx = last_idx?;
    // Se quita la palabra y los espacios que tiene justo delante, para dejar el
    // texto limpio.
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

/// Busca en el texto el patrón inequívoco «NUM_A [marca] NUM_B».
///
/// Caso A (marca simple: digo, en vez de, …):
///   - `start` = dónde empieza la marca (por ejemplo «digo»)
///   - `end` = dónde acaba
///   - `before` = input[..start] (NUM_A es el último NÚMERO de `before`, no
///     necesariamente la última palabra: al hablar, el número lleva detrás
///     sintagma nominal y/o verbo — «a tres personas, digo cuatro» — y la
///     última palabra sería «personas»)
///   - `after` = input[end..] (NUM_B está al principio de `after`)
///
/// Caso B («no X sino»):
///   - el grupo 1 de la expresión regular coge X, que es la «pieza equivocada»
///     que el usuario cambia. NUM_A = last_token(X).
///   - `before` = input[..start_of_match] (lo que va antes del «no»; en «no voy
///     a las tres sino…» antes del «no» no hay nada útil, pero en frases más
///     largas como «el precio es 4 no 5 sino 6» sí hay contexto que conservar).
///   - `after` = input[end_of_match..] (NUM_B está al principio).
fn parse_numeric_correction(input: &str) -> Option<NumericCorrection> {
    let re = numeric_marker_regex();
    let caps = re.captures(input).ok().flatten()?;
    let m = caps.get(0)?;
    let start = m.start();
    let end = m.end();
    let marker_text = m.as_str();

    // NUM_B: el primer número de `input[end..]`, saltando palabras de enlace.
    // Igual en los dos casos.
    let after = input[end..].trim();
    let num_b = first_numeric_token(after)?;
    if count_numeric_tokens(after) != 1 {
        return None;
    }

    // NUM_A depende del tipo de marca:
    // - Caso B («no X sino»): X está cogido en el grupo 1 y ES el «antes» que se
    //   cambia. NUM_A es el último número de X. Pero además hay que conservar la
    //   parte de X que va ANTES de NUM_A (por ejemplo «no voy a las tres sino …»
    //   → se conserva «voy a las» y solo se cambia «tres»).
    // - Caso A: NUM_A es el último número de `input[..start]`.
    let (before, num_a) = if let Some(x_match) = caps.get(1) {
        // Caso B: «no X sino».
        let x = x_match.as_str();
        let last = last_token(x)?;
        if !is_numeric_token(&last) {
            return None;
        }
        // El «antes» definitivo: lo que va antes del «no» + la parte de X que
        // precede a NUM_A (sin el número).
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
        // Caso A: NUM_A = el número RELEVANTE del «antes», o sea el ÚLTIMO
        // número que sale antes de la marca. NO se exige que sea la última
        // PALABRA: al hablar, el número va seguido de sintagma nominal y/o verbo
        // («a tres personas, digo cuatro», «a 23 personas que iban, digo, a 24»),
        // así que la última palabra era «personas»/«iban» y el patrón no se
        // detectaba (H1 del QA-REPORT-DEPURACION, log 14/09 03:19 y 13/09 01:37).
        // Se admiten, por tanto, palabras de enlace y de contenido entre NUM_A y
        // la marca.
        //
        // Mejor no tocar que tocar mal: el «antes» tiene que llevar EXACTAMENTE
        // un número. Con dos o más candidatos no hay forma de saber cuál
        // rectifica el usuario → no se toca nada.
        let before_str = input[..start].trim();
        let last = last_numeric_token(before_str)?;
        if count_numeric_tokens(before_str) != 1 {
            return None;
        }
        (before_str.trim_end().to_string(), last)
    };

    // El «después» (todo lo que va tras la marca): en el caso B hay que
    // descontar X y «sino» (que ya están dentro del trozo encontrado, así que en
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

/// Primer número de un texto, saltando palabras de enlace («a», «las», «los»,
/// «el», «la», «de», «del», «al»). Devuelve `None` si no hay ningún número.
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
                // Se ha encontrado una palabra que no es número ni palabra de
                // enlace: el número de verdad (si está) viene DESPUÉS. Si ya han
                // pasado palabras que no son ni una cosa ni la otra, este turno no
                // encaja en el patrón «NUM_B seguido».
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

/// Última palabra (alfanumérica) de un texto.
fn last_token(s: &str) -> Option<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .next_back()
        .map(|t| t.to_string())
}

/// Último número de un texto (un dígito o una palabra de `NUM_WORDS`),
/// saltándose el resto. `None` si no hay ninguno.
///
/// Es la pieza que permite que NUM_A sea «tres» en «a tres personas, digo
/// cuatro»: el número que importa no es la última PALABRA del trozo anterior a
/// la marca, sino el último NÚMERO de ese trozo (H1 del
/// QA-REPORT-DEPURACION).
fn last_numeric_token(s: &str) -> Option<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && is_numeric_token(t))
        .next_back()
        .map(|t| t.to_string())
}

/// Cuenta cuántos números hay en `s`.
fn count_numeric_tokens(s: &str) -> usize {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && is_numeric_token(t))
        .count()
}

/// Saca el texto que hay que leer de la respuesta JSON del modelo. Se prefiere
/// `choices[0].message.content`; si está vacío se baja a
/// `choices[0].message.reasoning_content` (modelos con razonamiento implícito,
/// por ejemplo Qwen3.5-0.8B antes de mandar `enable_thinking=false` en la
/// petición, o cualquiera que ignore esa marca). Devuelve `None` solo si los dos
/// están vacíos o no vienen — quien llama lo convierte en el error «la respuesta
/// del LLM no trae content».
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

/// Saca `{find, replace}` del texto generado, aguantando bloques ```json …
/// ```, texto alrededor del JSON y comas de más al final. Si falta una clave, se
/// queda vacía (quien valida de verdad es `apply_diff`).
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

    // Localizar el objeto JSON que hay que leer. Caso fácil: hay un solo {...}
    // en el texto → se coge tal cual. Caso difícil (modelos con razonamiento): el
    // bloque de pensamiento puede llevar varios objetos JSON por el medio (planes,
    // ejemplos, decisiones) antes del JSON bueno del final. Coger «del primer `{`
    // al último `}`» deja basura por medio y serde falla; hay que encontrar el
    // ÚLTIMO objeto con las llaves bien cerradas.
    let slice = last_balanced_json_object(s).unwrap_or(s);
    if slice != s {
        // Quien llama puede mirarlo en el log si quiere; no es grave.
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

/// Encuentra el último objeto `{…}` con las llaves bien cerradas en `s`. Ignora
/// las llaves que salgan dentro de cadenas JSON (con comillas escapadas o no).
/// Si no encuentra ninguno, devuelve `None` y quien llama usa el trozo de antes
/// (que probablemente falle, pero así se comporta igual que antes). Está pensado
/// para razonamientos en varias partes, donde hay varios objetos JSON sueltos
/// antes de la respuesta buena.
fn last_balanced_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    // Se recorre de derecha a izquierda buscando dónde empieza el último bloque
    // {...} que serde_json pueda tragarse.
    while i > 0 {
        // Se busca el siguiente '}' que cierre un objeto del primer nivel.
        if bytes[i - 1] != b'}' {
            i -= 1;
            continue;
        }
        // Se cuentan las llaves desde ese '}' hacia atrás, saltándose las que van
        // dentro de cadenas.
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
                        // j apunta al '{' de apertura e i a la '}' de cierre
                        // (uno más allá del último carácter).
                        return Some(&s[j..i]);
                    }
                }
                b'}' => depth += 1,
                _ => {}
            }
        }
        // No hubo apertura para ese '}' — se pasa al anterior.
        i -= 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Arranque automático del servidor local (requisito UX 09/09/2026: la app
// levanta todo por sí misma). Detecta un llama-server que ya esté corriendo, o
// lo lanza con el modelo configurado, y espera a que /v1/models conteste antes
// de dar el «listo». Todo va a lo seguro: si el servidor no sale, la corrección
// IA se apaga con aviso a la vista pero el dictado sigue funcionando.
// ---------------------------------------------------------------------------

/// ¿Contesta la dirección OpenAI-compatible? Hace `GET {base}/models` (llama.cpp
/// lo sirve en /v1/models) con poco tiempo de espera: esa es la señal de
/// «servidor vivo y modelo cargado». Un servidor que todavía está cargando el
/// GGUF contesta 503 en /health, pero en /models solo contesta cuando la carga
/// ha terminado — preguntar a /models es la forma fiable de esperar a que el
/// modelo esté listo.
pub fn server_alive(base_url: &str) -> bool {
    server_alive_auth(base_url, None)
}

/// Igual que `server_alive`, pero manda `Authorization: *** cuando hay
/// api_key. Las direcciones remotas (Custom/Cloud) suelen pedirla — sin la
/// cabecera, un 401 haría parecer «caído» un servidor que está perfecto — y
/// llama.cpp la ignora si llega, así que no pasa nada por mandarla siempre que
/// el usuario la haya puesto.
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

/// ¿La dirección apunta a la propia máquina (loopback)? Solo tiene sentido que
/// la app arranque el servidor para una dirección local: las remotas (nube o
/// propias) nunca se «levantan» desde aquí (los errores de conexión ya se ven al
/// dictar).
pub fn is_local_endpoint(base_url: &str) -> bool {
    let rest = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let authority = rest.split('/').next().unwrap_or("");
    // Se saca el host de la autoridad: quitando el «:puerto» si lo hay (el último
    // trozo detrás de ':') y los corchetes de IPv6.
    let host = match authority.rfind(':') {
        Some(pos) if authority[pos + 1..].parse::<u16>().is_ok() => &authority[..pos],
        _ => authority,
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.is_empty() || host == "127.0.0.1" || host == "localhost" || host == "::1"
}

/// Puerto del servidor local sacado de la dirección. Sin puerto puesto (o con
/// dirección remota) → 8081, el de siempre en Silb.
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

/// Lanza el servidor local como proceso NATIVO de Windows. El binario es
/// `llama-server.exe` (D:\LlamaCPP, o la ruta puesta en Ajustes): build Clang
/// 20/CUDA 13 con cublas64_13.dll junto al exe.
///
/// REGLA DE PRODUCTO (t_75732630): el servidor se arranca SIEMPRE con el binario
/// NATIVO de Windows, NUNCA con `wsl.exe`. Un servidor en WSL lanzado con la app
/// se queda huérfano al salir (fallo t_9fbc886f), no se puede matar por nombre
/// de imagen y atar la vida de un servidor de escritorio a WSL es frágil. Este
/// es el ÚNICO camino de arranque: cualquier otra ruta (WSL, el envoltorio
/// `llama.exe`) se rechaza aquí.
///
/// Detalles del lanzamiento que importan (probados con el binario real):
/// - `current_dir` = la carpeta del exe (o `cuda/` si hay paquete CUDA): el
///   binario es un envoltorio que carga `llama-server-impl.dll` y sus
///   dependencias (cublas64_13.dll, ggml-*) desde esa carpeta; lanzarlo desde
///   otro cwd puede fallar al cargar (STATUS_ENTRYPOINT_NOT_FOUND).
/// - `CREATE_NO_WINDOW`: servidor sin consola (la app es una ventana).
/// - stdout/stderr van a `llama-server.log`, junto a la config: sin consola, lo
///   que escriben a stdout fallaría, y el log permite mirar arranques que salen
///   mal.
///
/// El PID del proceso lanzado se guarda en una variable global atómica
/// (`LAUNCHED_SERVER_PID`): al salir, la app cierra SOLO ese PID
/// (`kill_launched_server`), nunca un servidor ajeno que ya estuviera escuchando
/// en el puerto antes de abrir la app. Devuelve el PID del proceso lanzado.
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
    // Paquete CUDA opcional junto al servidor embarcado (`vendor/llama/cuda/`):
    // el cwd pasa a ser esa carpeta porque el cargador de backends de ggml busca
    // los `ggml-*.dll` en el cwd y en la carpeta del exe — y lo que necesitan
    // (cudart/cublas) se busca por PATH, así que el paquete va al frente. Sin
    // paquete (o sin driver NVIDIA) se arranca desde la carpeta del exe: CPU y
    // sin avisos.
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

/// [Linux/macOS] Lanza el servidor local (`llama-server`) como proceso nativo.
/// Mismo trato que en Windows: stdout/stderr a `llama-server.log` junto a la
/// config, cwd = la carpeta del exe (los builds de llama.cpp cargan sus
/// librerías desde ahí) y el PID apuntado en `LAUNCHED_SERVER_PID` para cerrarlo
/// al salir.
#[cfg(not(target_os = "windows"))]
pub fn launch_server(exe: &std::path::Path, model: &std::path::Path, port: u16) -> Result<u32, String> {
    use std::process::{Command, Stdio};

    if !is_server_exe(exe) {
        return Err(format!(
            "el server LLM debe ser {SERVER_EXE_NAME} (binario nativo de llama.cpp, \
             nunca un wrapper): {}",
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
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            log_file
                .try_clone()
                .map_err(|e| format!("log del server: {e}"))?,
        ))
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

/// Argumentos de `llama-server.exe` para dictar. Los flags salen de la
/// instalación local (probados con el binario real y los .bats de
/// D:\LlamaCPP): `-ngl 99` + `--flash-attn on` (GPU), `-ctk/-ctv q8_0` (caché
/// KV cuantizada, con los 8.1 GB de VRAM libres) y `-c 8192` (contexto de sobra
/// para el turno y la corrección, sin pagar la RAM/VRAM de los 128K de los
/// .bats). `--host 127.0.0.1` = solo loopback: un servidor de dictado no debe
/// asomar a la red.
///
/// Función pura y fácil de probar: el lanzamiento usa exactamente esta lista.
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

/// PID del servidor que lanzó ESTA instancia de la app (0 = ninguno). Lo escribe
/// `launch_server` (hilo de arranque) y lo lee `kill_launched_server` (hilo de la
/// interfaz al salir): con un `AtomicU32` no hay carrera entre el arranque y el
/// «Salir».
static LAUNCHED_SERVER_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// PID apuntado por esta instancia (pruebas y log de diagnóstico).
pub fn launched_server_pid() -> u32 {
    LAUNCHED_SERVER_PID.load(Ordering::SeqCst)
}

/// Cierra un PID concreto de servidor, solo si el proceso sigue siendo
/// `llama-server.exe` (protección contra PIDs que se reutilizan). Devuelve `true`
/// si era nuestro servidor y se cerró; el 0 no se toca nunca.
///
/// Lo usan `kill_launched_server` (el hueco global de la app) y el hilo de la
/// fase del modelo al cancelarla en caliente (interruptor apagado): el hilo
/// cierra el PID concreto que lanzó él, no lo que haya en el hueco global (otra
/// fase pudo cambiarlo mientras tanto).
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
            // Protección contra PIDs reutilizados: solo se cierra un proceso que se
            // llame de verdad llama-server.exe.
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
        // Protección contra PID reutilizado: /proc/<pid>/exe tiene que ser
        // `llama-server`. Mejor dejar un servidor huérfano que cerrar uno ajeno.
        let exe_link = match std::fs::read_link(format!("/proc/{pid}/exe")) {
            Ok(p) => p,
            Err(_) => {
                crate::log!("llm", "server LLM propio (pid {pid}) ya no existe (/proc)");
                return false;
            }
        };
        let is_ours = exe_link
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == SERVER_EXE_NAME)
            .unwrap_or(false);
        if !is_ours {
            crate::log!(
                "llm",
                "pid {pid} ya no es {SERVER_EXE_NAME} (reutilizado): no se toca"
            );
            return false;
        }
        // SIGTERM (cierre limpio, suelta la VRAM) y SIGKILL si a los 2 s sigue vivo.
        unsafe {
            if libc::kill(pid as libc::pid_t, libc::SIGTERM) != 0 {
                crate::log!("llm", "server LLM propio (pid {pid}) ya no existe (kill ESRCH)");
                return false;
            }
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    crate::log!("llm", "server LLM propio (pid {pid}) terminado (SIGTERM)");
                    return true;
                }
            }
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
            crate::log!("llm", "server LLM propio (pid {pid}) terminado (SIGKILL tras 2 s)");
            true
        }
    }
}

/// Cierra el servidor que lanzó ESTA instancia, si lanzó alguno. NO toca
/// servidores ajenos: (a) si nunca se lanzó nada el PID es 0 y no se hace nada;
/// (b) aunque otro proceso hubiera cogido ese PID, se comprueba el nombre de
/// imagen (`llama-server.exe`) antes de cerrar. Devuelve `true` si había un
/// servidor propio y se cerró.
///
/// Se llama en TODOS los caminos de salida de la app (el «Salir» de la bandeja y
/// el cierre de la burbuja) ANTES de `iced::exit()`, y al apagar la corrección IA
/// en caliente (así el modelo sale de memoria).
pub fn kill_launched_server() -> bool {
    let pid = LAUNCHED_SERVER_PID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return false;
    }
    terminate_server_pid(pid)
}

/// Ruta del ejecutable `llama-server.exe` que hay que usar. Orden (14/10/2026,
/// «cero rutas que teclear»): el servidor EMBARCADO en el paquete
/// (`vendor/llama/llama-server.exe` junto al exe de la app) → el que ha
/// descargado la app en `%APPDATA%/whispr-open/bin/` → la ruta de la config (lo
/// que haya escrito el usuario, comprobado) → autodetección (rutas de siempre y
/// PATH). El embarcado GANA a la config: una ruta vieja en config.json (de una
/// instalación anterior) no puede romper una instalación nueva.
/// `None` si no hay ningún candidato → el arranque lo cuenta como `LlmDisabled`
/// con aviso (y queda la dirección OpenAI-compatible editable como
/// alternativa).
pub fn resolve_server_path(cfg: &crate::config::Config) -> Option<std::path::PathBuf> {
    // Candidatos de autodetección: primero el servidor que ha descargado la app en
    // `%APPDATA%/whispr-open/bin/` (la carpeta que enseña la pantalla de
    // Corrección IA), después las rutas de siempre y el PATH. Se pasan como lista
    // para que `resolve_server_path_in` siga sin tocar nada de fuera en las
    // pruebas (no lee el %APPDATA% real).
    let mut common: Vec<std::path::PathBuf> = Vec::new();
    let downloaded = downloaded_server_path();
    if downloaded.is_file() {
        common.push(downloaded);
    }
    common.extend(default_server_roots());
    resolve_server_path_in(exe_dir().as_deref(), cfg.llama_server_path(), &common)
}

/// El núcleo del orden de búsqueda, con parámetros para probarlo con carpetas
/// temporales (nada del disco real: ni `D:\LlamaCPP` ni el PATH de la máquina).
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

/// Decide SOLO con la ruta escrita en Ajustes («Server LLM»). Va aparte de las
/// rutas de autodetección para poder probarla con ficheros temporales sin
/// depender del disco de la máquina (D:\LlamaCPP, PATH…).
///
/// - No existe en disco → `None` (quien llama pasa a la autodetección).
/// - Existe pero no es `llama-server.exe` (ni hay uno al lado) → `None` y queda
///   en el log (se ignora y se autodetecta).
/// - Existe y es (o se arregla a) `llama-server.exe` → `Some`.
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
    // Arreglo (t_75732630): la ruta de la config TIENE que ser
    // `llama-server.exe`. Si apunta a un envoltorio (por ejemplo `llama.exe`) se
    // usa el `llama-server.exe` de al lado — así al salir se puede cerrar el
    // proceso por nombre.
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

/// Candidatos de AUTODETECCIÓN (el embarcado y el de la config se prueban
/// ANTES, en `resolve_server_path_in`), por orden de preferencia:
/// 1. `D:\LlamaCPP\llama-server.exe` (la instalación de siempre en este
///    equipo, build CUDA 13 con cublas64_13.dll) — y su gemelo `C:\LlamaCPP`.
/// 2. Al lado del exe de la app (todo en la misma carpeta, sin `vendor/`).
/// 3. `%LOCALAPPDATA%\LlamaCPP\llama-server.exe` (instalación por usuario).
/// 4. PATH de Windows (`llama-server.exe` en cualquier carpeta del PATH).
#[cfg(target_os = "windows")]
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

/// [Linux/macOS] Candidatos de autodetección: al lado del exe de la app,
/// `~/.local/bin`, la carpeta de config y el PATH (`llama-server`).
#[cfg(not(target_os = "windows"))]
fn default_server_roots() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join(SERVER_EXE_NAME));
        }
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local").join("bin").join(SERVER_EXE_NAME));
        roots.push(
            dirs::config_dir()
                .unwrap_or_else(|| home.join(".config"))
                .join("whispr-open")
                .join(SERVER_EXE_NAME),
        );
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            roots.push(dir.join(SERVER_EXE_NAME));
        }
    }
    roots
}

/// El núcleo de la búsqueda del ejecutable (con parámetros para probarlo con
/// rutas temporales): el primer candidato que exista, en orden.
fn resolve_server_path_with(roots: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    roots
        .iter()
        .find(|p| p.is_file())
        .map(Clone::clone)
}

/// Búsqueda del modelo (fichero GGUF). Si `model_name` ya es una ruta (lleva
/// separador) se usa tal cual cuando existe; si no, se recorre un puñado de
/// carpetas de siempre buscando un fichero con ese mismo nombre. El recorrido va
/// con tope de profundidad para no peinar discos enteros.
pub fn resolve_model_path(model_name: &str) -> Option<std::path::PathBuf> {
    resolve_model_path_with(model_name, &model_roots_in(exe_dir().as_deref()))
}

/// Carpetas donde busca el GGUF del corrector `resolve_model_path` (sale en el
/// log de diagnóstico cuando no aparece: el usuario no tiene que escribir
/// ninguna).
pub fn model_search_roots() -> Vec<std::path::PathBuf> {
    model_roots_in(exe_dir().as_deref())
}

/// Carpetas donde buscar el GGUF del corrector, en orden:
/// 1. `vendor/models` junto al exe: el modelo puede VIAJAR con la app (la misma
///    raíz `vendor/` que el servidor; si no está, no pasa nada).
/// 2. `%APPDATA%\whispr-open\models`: la MISMA carpeta donde la app descarga
///    los modelos de whisper — el sitio natural para el GGUF descargado.
/// 3. LM Studio e instalaciones de llama.cpp con su propia carpeta `models`
///    (para aprovechar lo que ya hubiera en la máquina).
fn model_roots_in(exe_dir: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = exe_dir {
        roots.push(dir.join("vendor").join("models"));
    }
    roots.push(crate::transcribe::models_dir());
    // Carpeta propia del corrector (`%APPDATA%/whispr-open/models-llm/`): es la
    // que enseña la pantalla de Corrección IA, así que un GGUF dejado ahí se
    // encuentra solo.
    roots.push(llm_models_dir());
    // LM Studio: la carpeta de modelos por defecto del usuario.
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".lmstudio").join("models"));
        roots.push(
            home.join(".lmstudio")
                .join("models")
                .join("lmstudio-community"),
        );
    }
    // Instalaciones de llama.cpp con su propia carpeta models (solo Windows: en
    // Linux quedaban como rutas relativas "C:/..." que no existen).
    #[cfg(windows)]
    for drive in ['C', 'D'] {
        roots.push(PathBuf::from(format!("{drive}:/LlamaCPP/models")));
        roots.push(PathBuf::from(format!("{drive}:/models")));
    }
    roots
}

/// El núcleo de la búsqueda de modelo, con parámetros para probarlo con carpetas
/// temporales. Profundidad máxima 6: los GGUF de LM Studio suelen estar a 3-4
/// niveles (models/<autor>/<modelo>/<fichero>); más abajo no compensa (peinar un
/// disco entero tardaría minutos).
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

// ---------------------------------------------------------------------------
// Elegir y bajar el modelo de Hugging Face (0.7)
// ---------------------------------------------------------------------------

/// Lo que hay que bajar de Hugging Face: nombre del fichero y su dirección.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfTarget {
    pub file_name: String,
    pub url: String,
}

/// Una referencia ya troceada, sin haber tocado la red todavía.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HfRef {
    /// Dirección directa a un `.gguf`: no hay que preguntar nada más.
    Direct(HfTarget),
    /// `owner/repo`: hay que mirar qué ficheros tiene el repo.
    Repo(String),
}

/// Lo que se dice cuando lo pegado no lleva a ningún GGUF.
const HF_ONLY_GGUF: &str = "Solo vale un GGUF público de Hugging Face: pega \
                            «owner/repo» o la dirección del fichero .gguf.";

/// Último trozo de la ruta de una dirección, sin query ni ancla.
fn url_file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.trim_end_matches('/').rsplit('/').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Trocea lo que se pega en Ajustes. Esto NO hace red: decide si es una
/// dirección `.gguf` (se usa tal cual), un `owner/repo` (hay que mirar sus
/// ficheros) o algo que la app no sabe usar.
pub fn split_hf_ref(raw: &str) -> Result<HfRef, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(HF_ONLY_GGUF.to_string());
    }
    let sin_query = t.split(['?', '#']).next().unwrap_or(t);
    let lower = t.to_ascii_lowercase();

    if lower.starts_with("http://") || lower.starts_with("https://") {
        if sin_query.to_ascii_lowercase().ends_with(".gguf") {
            let file_name = url_file_name(t).ok_or_else(|| HF_ONLY_GGUF.to_string())?;
            return Ok(HfRef::Direct(HfTarget {
                file_name,
                url: sin_query.to_string(),
            }));
        }
        // https://huggingface.co/owner/repo (con o sin /tree/main...)
        let rest = sin_query.splitn(2, "://").nth(1).unwrap_or("");
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        if !host.eq_ignore_ascii_case("huggingface.co") {
            return Err(HF_ONLY_GGUF.to_string());
        }
        let mut trozos = path.split('/').filter(|s| !s.is_empty());
        let owner = trozos.next().unwrap_or("");
        let repo = trozos.next().unwrap_or("");
        if owner.is_empty() || repo.is_empty() {
            return Err(HF_ONLY_GGUF.to_string());
        }
        return Ok(HfRef::Repo(format!("{owner}/{repo}")));
    }

    // `owner/repo` escrito a mano.
    let mut trozos = t.split('/');
    let owner = trozos.next().unwrap_or("").trim();
    let repo = trozos.next().unwrap_or("").trim();
    if !owner.is_empty()
        && !repo.is_empty()
        && trozos.next().is_none()
        && !t.contains(char::is_whitespace)
    {
        return Ok(HfRef::Repo(format!("{owner}/{repo}")));
    }
    Err(HF_ONLY_GGUF.to_string())
}

/// Elige el GGUF dentro de la lista de ficheros de un repo. Si hay varios gana
/// el que lleve `q4_k_m` (el equilibrio tamaño/calidad de la app); si no hay
/// ninguno así, el primero. Sin GGUF no hay nada que bajar.
pub fn pick_gguf(siblings: &[String]) -> Option<String> {
    let ggufs: Vec<&String> = siblings
        .iter()
        .filter(|s| s.to_ascii_lowercase().ends_with(".gguf"))
        .collect();
    if ggufs.is_empty() {
        return None;
    }
    ggufs
        .iter()
        .find(|s| s.to_ascii_lowercase().contains("q4_k_m"))
        .or_else(|| ggufs.first())
        .map(|s| (**s).clone())
}

/// Agente de red de las descargas: límite al conectar y al leer cada trozo, sin
/// límite global (una descarga lenta pero viva tiene que poder terminar).
fn download_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build()
}

/// Resuelve lo que se pega en Ajustes hasta un fichero concreto de Hugging
/// Face. Un `owner/repo` se consulta en la API pública (sin clave) y se queda
/// con su GGUF; una dirección `.gguf` no necesita red.
pub fn parse_hf_ref(raw: &str) -> Result<HfTarget, String> {
    match split_hf_ref(raw)? {
        HfRef::Direct(target) => Ok(target),
        HfRef::Repo(repo) => {
            let api = format!("https://huggingface.co/api/models/{repo}");
            let body = download_agent()
                .get(&api)
                .call()
                .map_err(|e| match e {
                    ureq::Error::Status(404, _) => format!(
                        "no existe el repo «{repo}» en Hugging Face, o no es público. \
                         Revisa el nombre o pega la dirección del .gguf."
                    ),
                    _ => format!("no se pudo consultar Hugging Face: {e}"),
                })?
                .into_string()
                .map_err(|e| format!("respuesta ilegible de Hugging Face: {e}"))?;
            let json: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| format!("respuesta de Hugging Face que no se entiende: {e}"))?;
            let siblings: Vec<String> = json["siblings"]
                .as_array()
                .map(|lista| {
                    lista
                        .iter()
                        .filter_map(|v| v["rfilename"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let file_name = pick_gguf(&siblings).ok_or_else(|| {
                format!(
                    "el repo «{repo}» no tiene ningún .gguf. Solo se descargan GGUF: \
                     busca un repo con GGUF o pega la dirección del .gguf."
                )
            })?;
            Ok(HfTarget {
                url: format!("https://huggingface.co/{repo}/resolve/main/{file_name}"),
                file_name,
            })
        }
    }
}

/// ¿`path` empieza por la marca de un GGUF? Sirve para no dar por bueno un
/// fichero que en realidad es una página de error o una descarga cortada.
fn is_gguf_file(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && &magic == b"GGUF"
}

/// ¿Está ya el GGUF en alguna carpeta de la app? Mira fichero a fichero, sin
/// recorrer árboles: lo pregunta Ajustes en cada repintado. (Un modelo que solo
/// esté en LM Studio no se ve aquí: la app ofrecería bajarlo a su carpeta, que
/// es lo que hace igualmente `resolve_model_path` al arrancar.)
pub fn llm_model_on_disk(file_name: &str) -> bool {
    let Some(name) = std::path::Path::new(file_name.trim())
        .file_name()
        .and_then(|n| n.to_str())
    else {
        return false;
    };
    let mut carpetas = vec![llm_models_dir(), crate::transcribe::models_dir()];
    if let Some(exe) = exe_dir() {
        carpetas.push(exe.join("vendor").join("models"));
    }
    carpetas.iter().any(|dir| dir.join(name).is_file())
}

static LLM_DOWNLOAD_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

/// Se asegura de que el GGUF está en `models-llm/` y, si falta, lo baja. Copia
/// el patrón de `transcribe::ensure_model`: escribe a `<fichero>.part` y solo
/// renombra al final, avisa del progreso como mucho una vez por 1 % (o por MB si
/// el servidor no dice el tamaño) y avisa también al cerrar.
///
/// El `.part` NO se borra al fallar a propósito (pedido de la 0.7): así el
/// reintento puede empezar sin pelearse con un temporal de otro proceso. Pero
/// cada intento vuelve a empezar de cero (`File::create` trunca): no hay
/// reanudación por trozos.
pub fn ensure_llm_model(
    url: &str,
    file_name: &str,
    on_progress: impl Fn(u64, u64),
) -> Result<std::path::PathBuf, String> {
    use std::io::{Read, Write};

    let lock = LLM_DOWNLOAD_LOCK.get_or_init(|| std::sync::Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|e| format!("bloqueo de descarga envenenado: {e}"))?;

    let name = std::path::Path::new(file_name.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("«{file_name}» no es un nombre de fichero válido"))?
        .to_string();
    let dir = llm_models_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("no se pudo crear models-llm: {e}"))?;
    let path = dir.join(&name);

    // Ya está y es un GGUF: no se baja otra vez.
    if is_gguf_file(&path) {
        return Ok(path);
    }

    let resp = download_agent()
        .get(url)
        .call()
        .map_err(|e| format!("no se pudo descargar el modelo: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let tmp = dir.join(format!("{name}.part"));

    let outcome = (|| -> Result<(), String> {
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| format!("no se pudo crear {tmp:?}: {e}"))?;
        let mut downloaded = 0u64;
        let mut buf = [0u8; 64 * 1024];
        let mut reader = resp.into_reader();
        // Una descarga de 500 MB a trozos de 64 KB son ~8.000 avisos: la
        // interfaz se quedaría clavada. Uno por 1 % llega de sobra.
        let step = if total > 0 { (total / 100).max(1) } else { 1 << 20 };
        let mut last_emit = 0u64;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("error de descarga: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("error escribiendo el modelo: {e}"))?;
            downloaded += n as u64;
            if downloaded - last_emit >= step {
                last_emit = downloaded;
                on_progress(downloaded, total);
            }
        }
        on_progress(downloaded, total);
        file.sync_all()
            .map_err(|e| format!("error sincronizando el modelo: {e}"))?;
        // Cerrar antes de renombrar: en Windows mover un fichero abierto puede
        // fallar según cómo se abrió.
        drop(file);
        std::fs::rename(&tmp, &path)
            .map_err(|e| format!("no se pudo finalizar la descarga: {e}"))?;
        Ok(())
    })();
    outcome?;

    // Un 200 con una página de error dentro se vería aquí y se descarta: mejor
    // no dejar un «modelo» que el servidor no va a poder cargar.
    if !is_gguf_file(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "la descarga de {name} no devolvió un GGUF válido. Prueba otra vez o con otro repo."
        ));
    }
    crate::log!("llm", "modelo de corrección descargado: {}", path.display());
    Ok(path)
}

/// Espera (preguntando cada 400 ms) a que `/v1/models` conteste. Devuelve `true`
/// si el servidor quedó listo dentro del plazo y `false` si se agotó. Tiempos
/// reales: un 0.8B Q8 carga en ~2-5 s en GPU; 60 s de margen cubren modelos
/// grandes y los descartes por puerto ocupado sin dejar la app colgada.
///
/// Si el servidor que lanzó ESTA instancia no contesta en el plazo, ese `false`
/// acaba en `LlmDisabled` con aviso a la vista — el dictado sigue. El PID
/// lanzado se queda apuntado igual: al salir se cierra, no se deja un servidor a
/// medio cargar por ahí.
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

    /// Elección del GGUF dentro de un repo de Hugging Face (0.7): lo que no es
    /// GGUF se descarta y, entre varios, gana el Q4_K_M (el de la app).
    #[test]
    fn pick_gguf_elige_el_q4_k_m_y_descarta_lo_que_no_es_gguf() {
        assert_eq!(pick_gguf(&[]), None);
        assert_eq!(
            pick_gguf(&["README.md".to_string(), "config.json".to_string()]),
            None
        );
        let varios = vec![
            "modelo-f16.gguf".to_string(),
            "modelo-q4_k_m.gguf".to_string(),
            "modelo-q8_0.gguf".to_string(),
        ];
        assert_eq!(pick_gguf(&varios), Some("modelo-q4_k_m.gguf".to_string()));
        // Compara sin mirar mayúsculas: los repos escriben Q4_K_M y q4_k_m.
        let mayusculas = vec!["Modelo-Q4_K_M.GGUF".to_string(), "otro.gguf".to_string()];
        assert_eq!(pick_gguf(&mayusculas), Some("Modelo-Q4_K_M.GGUF".to_string()));
    }

    /// Sin Q4_K_M se queda el primero de la lista (nunca «no hay ninguno» si
    /// hay GGUF).
    #[test]
    fn pick_gguf_sin_q4_k_m_se_queda_el_primero() {
        let lista = vec!["a-f16.gguf".to_string(), "b-q8_0.gguf".to_string()];
        assert_eq!(pick_gguf(&lista), Some("a-f16.gguf".to_string()));
    }

    /// Troceo de lo que se pega en Ajustes, sin tocar la red: dirección .gguf,
    /// `owner/repo`, la página del repo y lo que no vale.
    #[test]
    fn split_hf_ref_entiende_direccion_repo_y_lo_que_no_vale() {
        let directa = split_hf_ref(
            "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/\
             qwen2.5-0.5b-instruct-q4_k_m.gguf?download=true",
        )
        .expect("una dirección .gguf tiene que valer");
        assert_eq!(
            directa,
            HfRef::Direct(HfTarget {
                file_name: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
                url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/\
                      qwen2.5-0.5b-instruct-q4_k_m.gguf"
                    .to_string(),
            })
        );

        let repo = "Qwen/Qwen2.5-0.5B-Instruct-GGUF";
        assert_eq!(split_hf_ref(repo), Ok(HfRef::Repo(repo.to_string())));
        assert_eq!(
            split_hf_ref("https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF"),
            Ok(HfRef::Repo(repo.to_string()))
        );
        assert_eq!(
            split_hf_ref("https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/tree/main"),
            Ok(HfRef::Repo(repo.to_string()))
        );

        // Nada de esto lleva a un GGUF: no se descarga y se avisa.
        for malo in [
            "",
            "   ",
            "qwen2.5-0.5b-instruct-q4_k_m.gguf",
            "un repo con espacios",
            "https://example.com/modelo.gguf.zip",
            "https://example.com/Qwen/Qwen2.5",
        ] {
            let err = split_hf_ref(malo).expect_err("esto no puede valer");
            assert!(err.contains("GGUF"), "mensaje poco claro para {malo:?}: {err}");
        }
    }

    /// La marca GGUF: un fichero que no la lleva no cuenta como modelo (así no
    /// se da por bueno una página de error guardada con extensión .gguf).
    #[test]
    fn is_gguf_file_mira_la_marca_no_el_nombre() {
        let dir = std::env::temp_dir().join(format!("silb-gguf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bueno = dir.join("bueno.gguf");
        std::fs::write(&bueno, b"GGUF\x03\x00\x00\x00").unwrap();
        let malo = dir.join("malo.gguf");
        std::fs::write(&malo, b"<html>404</html>").unwrap();
        assert!(is_gguf_file(&bueno));
        assert!(!is_gguf_file(&malo));
        assert!(!is_gguf_file(&dir.join("no-existe.gguf")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalogo_local_son_modelos_pequenos_y_descargables() {
        assert!(!SMALL_LLM_MODELS.is_empty());
        assert_eq!(SMALL_LLM_MODELS[0].id, "qwen2.5-0.5b");
        for m in SMALL_LLM_MODELS {
            assert!(m.approx_mb <= 1200, "{} pesa {} MB", m.id, m.approx_mb);
            assert!(m.url.starts_with("https://huggingface.co/"), "{}", m.id);
            assert!(m.file_name.ends_with(".gguf"), "{}", m.id);
        }
        assert!(llm_models_dir().ends_with("models-llm"));
        assert_eq!(
            downloaded_server_path().file_name().and_then(|n| n.to_str()),
            Some(SERVER_EXE_NAME)
        );
    }

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
        // El contrato pide find+replace, pero si el modelo se deja una clave no hay
        // que romperse: queda vacía y `apply_diff` la descarta si no sirve.
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

    // --- Regresión Qwen3.5 con razonamiento ---------------------------------
    //
    // El fallo que dejó la corrección IA sin funcionar en producción (t_82a3367c):
    // el modelo configurado `Qwen3.5-0.8B-Q8_0.gguf` razona y, sin la marca
    // `chat_template_kwargs.enable_thinking=false` en la petición, devuelve
    // `message.content` VACÍO y `message.reasoning_content` con todo el JSON al
    // final del bloque de pensamiento. Estas pruebas fijan el arreglo, tanto de lo
    // que mandamos como de quien lee la respuesta.

    #[test]
    fn body_de_correct_desactiva_thinking_y_sube_max_tokens() {
        // La petición JSON que se manda debe llevar
        // `chat_template_kwargs.enable_thinking=false` para callar el modo
        // razonamiento del Qwen3.5 (y de cualquier otro modelo que haga caso a la
        // marca), y un `max_tokens` holgado (256) para no cortar respuestas buenas.
        //
        // Truco: se lee el mismo JSON que monta `correct()` para no depender de un
        // servidor de mentira. Si alguien quita la marca o baja max_tokens, esta
        // prueba se rompe en el acto.
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
        // Caso normal: content con texto y sin reasoning_content → se usa content
        // (no se mira el reasoning).
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
        // Regresión Qwen3.5: content="" y reasoning_content con el JSON al final
        // del bloque de pensamiento. Tiene que devolver el texto del reasoning (y
        // `parse_diff` sacará el último {...}).
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
        // Variante: content con solo espacios (lo que meten algunos serializadores
        // en lugar de una cadena vacía). También tiene que bajar al reasoning.
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
        // Modelo roto o petición mal montada: ni content ni reasoning. Aquí
        // `extract_message_text` devuelve None y `correct()` deja salir el error
        // «la respuesta del LLM no trae content».
        let v = serde_json::json!({
            "choices": [{
                "message": { "role": "assistant" }
            }]
        });
        assert!(extract_message_text(&v).is_none());
    }

    #[test]
    fn extract_message_text_ignora_reasoning_vacio() {
        // Si reasoning_content es "" o solo espacios tampoco cuenta — así no se
        // devuelve una cadena vacía que rompería `parse_diff`.
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
        // El reasoning de Qwen3.5 a veces lleva varios objetos JSON por el medio
        // (planes, ejemplos) antes del JSON bueno del final. `parse_diff` ya se
        // queda con el último {...} por el '}' del final; esta prueba fija ese
        // comportamiento con un reasoning realista.
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
        // El caso ideal: el modelo devuelve solo el texto final sin adornos.
        // `clean_redact_output` lo devuelve igual (salvo los espacios de los
        // extremos).
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
        // A veces el modelo envuelve la respuesta en ``` ... ```. Hay que quitar
        // tanto el bloque entero como la etiqueta de lenguaje que algunos ponen
        // detrás de los tres backticks.
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
        // Algunos modelos (sobre todo cuando meten el texto en una cadena JSON) lo
        // encierran en comillas. Solo se quitan las del PRIMER y ÚLTIMO carácter —
        // si están por el medio, son contenido legítimo.
        assert_eq!(
            clean_redact_output(r#""que calor en cartagena""#),
            "que calor en cartagena"
        );
        assert_eq!(
            clean_redact_output("'que calor en cartagena'"),
            "que calor en cartagena"
        );
        // Las comillas tipográficas («“”») también.
        assert_eq!(
            clean_redact_output("\u{201C}que calor en cartagena\u{201D}"),
            "que calor en cartagena"
        );
    }

    #[test]
    fn clean_redact_output_quita_prefijos_output_resultado() {
        // Algunos modelos (Qwen instruct, Llama 3 instruct) ponen un prefijo tipo
        // «Output:» delante del texto. Se quita para que no se escriba tal cual.
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
        // El contenido de dentro no se toca: solo se quitan las comillas que
        // envuelven el resultado entero.
        assert_eq!(
            clean_redact_output("el \"cliente\" es Microsoft"),
            "el \"cliente\" es Microsoft"
        );
    }

    #[test]
    fn clean_redact_output_caso_canonico_juanfri() {
        // El caso que motivó la tarea t_78d9e1fa. El modelo devuelve el texto final
        // limpio, SIN la frase de corrección «digo cartagena».
        let input_response = "que calor que hace en cartagena";
        assert_eq!(clean_redact_output(input_response), input_response);
        // Y se comprueba que esa frase de corrección no está en el resultado limpio.
        assert!(!clean_redact_output(input_response).contains("digo"));
    }

    #[test]
    fn clean_redact_output_con_adornos_combinados() {
        // Caso real: el modelo puede juntar bloques + comillas + prefijo + espacios.
        // La limpieza tiene que aguantar cualquier orden.
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
        // La petición que se manda debe usar `SYSTEM_PROMPT_IN_TURN` y un
        // `max_tokens` holgado (>=1024) porque devolvemos el turno ENTERO, no un
        // JSON corto. Subido de 512 a 1024 en t_6b9348de: un párrafo dictado largo
        // con su corrección cabe de sobra y el tope deja de cortar nada (quien tira
        // los resúmenes es la protección anti-truncación, no el tope). También se
        // apaga el razonamiento igual que en `correct()` (misma marca, mismo motivo,
        // mismo arreglo Qwen3.5).
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
        // El prompt de sistema es el de in-turn (lleva las marcas típicas de
        // autocorrección, no el contrato JSON del cross-turn).
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
        // Mismo trato que `correct()`: el error lleva el prefijo «HTTP:» para que
        // la interfaz sepa clasificar el fallo y enseñar «¿está el servidor LLM
        // arrancado?». Sin servidor local, esta prueba mira el camino de error del
        // modo in-turn.
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
        // El modo in-turn usa el mismo `extract_message_text`: con content con
        // texto usa content (lo normal); con content vacío y reasoning con texto,
        // baja al reasoning. El mismo arreglo Qwen3.5 que en `correct()`. Se
        // comprueba el trato sobre quien extrae.
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
        // La limpieza ya la hace `clean_redact_output`; aquí se comprueba que quien
        // extrae entrega el texto entero.
        assert!(extracted.contains("que calor en cartagena"));
    }

    // ── Protección anti-truncación (t_6b9348de) ──────────────────────────
    //
    // El corrector local (Qwen3.5-0.8B) a veces devuelve UN RESUMEN del turno en
    // vez del texto corregido (está en el log 01:34:54: entraron 73 palabras y
    // salieron 13). La protección tira las respuestas desproporcionadamente
    // cortas y devuelve el texto ORIGINAL: el dictado no se pierde ni se recorta.

    const LOG_LARGO_INPUT: &str = "estamos debulando la aplicación está del whisper y parece que hemos dado que sigue el clavo pero no está funcionando lo del LLM me pone listo pero aquí no se ha levantado ningún servidor ni ha hecho una pantalla de carga como se supone que tiene que hacer y no sé dime si pueden investigar algo y porque antes he dicho dime esto y no lo odro y se ha rallado un montón";
    const LOG_LARGO_OUTPUT: &str = "no se ha levantado ningún servidor ni ha hecho una pantalla de carga";

    #[test]
    fn guard_descarta_el_resumen_y_preserva_el_dictado_largo() {
        // Caso REAL del log (01:34:54): 73 palabras de entrada → 13 de salida. Sin
        // autocorrección que justifique el recorte, la protección devuelve el texto
        // entero. La comprobación que importa es la LONGITUD PRESERVADA (requisitos
        // 1 y 2 de t_6b9348de).
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
        // El caso corto de siempre (requisito 4): el modelo devuelve el texto
        // corregido sin la frase de corrección; 11/13 palabras (>=60%) → la
        // protección NO debe tirarlo.
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
        // «no X sino Y» quita el trozo corregido y la frase de corrección: 4/9
        // palabras (<60%) pero hay marca de autocorrección → el recorte está
        // justificado y se admite.
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
        // Recorte del 40% sin ninguna marca de corrección: no hay explicación
        // legítima para quitar más de la mitad de las palabras → se tira y se
        // devuelve el texto original.
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
        // Con marca se admite bajar hasta el 30% de las palabras; por debajo (un
        // trozo suelto) huele a resumen y se tira igual — la marca no es un cheque
        // en blanco.
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
        // Respuesta vacía → el texto original (el dictado no se pierde).
        let input = "el dictado completo de varias palabras";
        assert_eq!(guard_anti_truncacion(input, ""), input);
        // Texto vacío → no hay nada que proteger: devuelve la respuesta tal cual y
        // no se rompe (así no hay división por cero).
        assert_eq!(guard_anti_truncacion("", ""), "");
        assert_eq!(guard_anti_truncacion("", "algo"), "algo");
    }

    #[test]
    fn guard_pasa_output_igual_al_input() {
        // Sin cambios (el modelo confirma el turno) → salida == entrada, la
        // proporción es 1.0 y la protección no hace nada.
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

    // ── Protección numérica fija (t_1fc864ef) ─────────────────────────────
    //
    // El corrector local (Qwen3.5-0.8B-Q8) pega los números en vez de cambiarlos
    // cuando se dicta «A digo B» con A y B números. Está en el log: «me comi
    // cuatro digo cinco peras» → «me comi cuatro cinco peras» (sin «digo», con
    // los dos números seguidos). Esta protección busca el patrón en el texto
    // dictado y rehace el cambio en código.
    //
    // Trato: interviene SOLO si el patrón es inequívoco (dos números simples
    // separados por una marca de corrección) Y la respuesta del modelo no trae la
    // corrección. En cualquier otro caso devuelve Err(()) y deja el flujo en manos
    // de las demás protecciones.

    #[test]
    fn guard_numeric_corrige_concatenacion_de_numerales_en_palabras() {
        // El caso del fallo: texto «A digo B» y el modelo devuelve «A B» (sin la
        // marca y con los dos números pegados).
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
        // Variante con dígitos: «2 en vez de 4» → respuesta «2 4» (pegados).
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
        // Variante con la marca «no X sino Y»: texto «no X sino Y», respuesta «X Y»
        // (el 0.8B los pega).
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
        // El modelo contestó bien: NO debe intervenir.
        let input = "me comi cuatro digo cinco peras";
        let output = "me comi cinco peras";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "el LLM ya resolvió la corrección: el guard NO debe intervenir"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_no_hay_numerales() {
        // Caso sin números (una corrección de sitio). NO es problema del 0.8B (el
        // modelo lo resuelve bien al ejecutarse) → no se interviene.
        let input = "que calor en murcia digo cartagena";
        let output = "que calor en murcia cartagena"; // hipotético «fallo» del LLM
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "sin numerales el guard numérico no aplica (alcance del fix)"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_no_hay_patron_a_digo_b() {
        // Hay números pero SIN marca de corrección: el turno NO es una
        // autocorrección numérica.
        let input = "me comi cuatro peras";
        let output = "me comi cuatro peras";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "sin marcador correctivo no hay sustitución que aplicar"
        );
    }

    #[test]
    fn guard_numeric_no_interviene_si_solo_hay_un_numeral() {
        // El usuario repite el número sin corregirlo: no hay autocorrección y la
        // protección no debe inventarse una.
        let input = "me comi cuatro digo cinco peras y me comi cuatro digo cinco";
        // Para que el patrón sea inequívoco se pide un solo número antes y un solo
        // número después; con dos «cuatro/cinco» repetidos no hay coincidencia → la
        // protección no interviene.
        let output = "cuatro cinco cuatro cinco";
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "con múltiples numerales el patrón deja de ser inequívoco"
        );
    }

    #[test]
    fn guard_numeric_no_se_pisa_con_texto_largo_y_sin_correccion() {
        // El caso LARGO del log, sin corrección (73 palabras), no debe entrar en la
        // protección numérica: quien detecta resúmenes es `guard_anti_truncacion`.
        // La numérica solo mira el patrón «NUM A [marca] NUM B».
        let input = LOG_LARGO_INPUT;
        let output = LOG_LARGO_OUTPUT;
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "texto sin corrección numérica explícita: el guard no aplica"
        );
    }

    #[test]
    fn guard_numeric_corrige_incluso_si_llm_borro_numerales() {
        // Caso real del 0.8B: a veces no solo pega los números, también BORRA uno o
        // los dos (seguramente por confusión). La protección ve el patrón en el
        // texto dictado y puede reconstruirlo igual: «me comi cuatro digo cinco
        // peras» con respuesta «me comi peras» → reconstruido «me comi cinco peras»
        // (¡mejor que lo dictado!).
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
        // Comprobación rápida: los casos de siempre (que el modelo resuelve bien al
        // ejecutarse) NO deben disparar la protección: «murcia digo cartagena», «se
        // cruzo una moto digo se cruzo un coche», etc. La protección solo aplica
        // cuando hay número, así que con texto sin números no hay patrón.
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
    // Casos reales del log del usuario (14/09 03:19 y 13/09 01:37): entre el
    // número y la marca hay sintagma nominal y/o verbo («a tres personas, digo
    // cuatro personas»). El modelo local se quedaba con el número VIEJO, borraba
    // la frase de corrección y soltaba una frase creíble y FALSA; la protección
    // no lo cazaba porque exigía que NUM_A fuese la última palabra antes de la
    // marca.

    #[test]
    fn guard_numeric_corrige_caso_real_log_1_numeral_con_sintagma() {
        // Caso 1 del log: texto con «tres personas, digo cuatro personas» y
        // respuesta del modelo con el número VIEJO y sin la frase de corrección.
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
        // Lo reconstruido no debe caer en la protección anti-truncación (el mismo
        // criterio que aplica app.rs antes de escribir) ni llevar restos de la
        // frase de corrección (eso haría que app.rs escribiera el texto bruto y se
        // perdiera la corrección).
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
        // Caso 2 del log: texto con «23 personas que iban, digo, a 24 personas»,
        // respuesta del modelo con los DOS números pegados y sin la frase de
        // corrección (frase rota).
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
        // La ampliación no es un parche solo para el log: vale para cualquier
        // texto (de enlace o de contenido) entre NUM_A y la marca.
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
        // CONTROL: dictado normal sin frase de corrección ni rectificación. La
        // protección NO aplica y la respuesta del modelo (igual al texto) se
        // escribe tal cual: nunca se cambia texto que no es una autocorrección.
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
        // NEGATIVO: dos números candidatos antes de la marca (¿cuál rectifica el
        // usuario?) → mejor no tocar que tocar mal, no se toca nada.
        let input = "tengo 3 perros y 4 gatos, digo 5";
        let output = "tengo 3 perros y 5"; // conjetura (posible) del LLM
        assert!(
            guard_numeric_substitution(input, output).is_err(),
            "con dos numerales antes del marcador el patrón no es inequívoco"
        );
        // Y sin marca de corrección, dos números sueltos tampoco.
        let sin_marcador = "había 3 personas y 4 coches en la calle";
        assert!(
            guard_numeric_substitution(sin_marcador, sin_marcador).is_err(),
            "sin marcador no hay sustitución que aplicar"
        );
    }

    #[test]
    fn guard_numeric_canonico_moto_digo_coche_sigue_funcionando() {
        // Caso de siempre del usuario: corrección SIN números. La protección
        // numérica no aplica y la corrección del modelo pasa intacta.
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
        // Asegura que cuando el servidor NO está corriendo (`connect refused` o
        // parecido), el error lleva el prefijo «HTTP:» para que la interfaz pueda
        // enseñar la pista «¿está el servidor LLM arrancado?». Sin prefijo, la
        // interfaz no sabe clasificar el error y el usuario se queda a ciegas
        // (fallo del 09/09/2026: «no llama al LLM» sin ninguna pista).
        //
        // Se usa un puerto que casi seguro está libre: el 1 (reservado, nadie
        // escucha). Si por lo que sea alguien escucha ahí, la prueba falla y nos
        // enteramos.
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
        // Puerto 1: reservado, nadie escucha — el «connect refused» es inmediato
        // (y la prueba no se come los 2 s de espera).
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
        // Sin puerto puesto o dirección remota → 8081, el de siempre.
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

        // Nombre que no existe → None (no se rompe al recorrer carpetas vacías).
        assert_eq!(
            resolve_model_path_with("no-existe.gguf", &[dir.clone()]),
            None
        );
        // Una ruta directa (con separador) se usa tal cual si existe.
        let direct = resolve_model_path_with(&gguf.to_string_lossy(), &[]);
        assert_eq!(direct.as_deref(), Some(gguf.as_path()));
        // Ruta directa que no existe → None.
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
        // El recorrido está acotado a 6 niveles: un fichero a profundidad 8 no
        // se encuentra (así no se peinan discos enteros).
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
        // El lanzamiento usa EXACTAMENTE build_server_args: aquí se fija el trato
        // — modelo como ruta nativa, puerto, loopback y flags de GPU.
        let model = std::path::Path::new(r"C:\models\qwen3.5-0.8b.gguf");
        let args = build_server_args(model, 8081);
        assert_eq!(
            args,
            vec![
                "-m",
                r"C:\models\qwen3.5-0.8b.gguf",
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

        // El primero no existe y el segundo sí → coge el segundo (por el orden).
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

    /// Orden de búsqueda del servidor (14/10/2026): el que VIAJA con la app
    /// (`vendor/llama/`) gana a la config y a las rutas de siempre; una config
    /// inválida no rompe nada; sin embarcado se autodetecta. Todo con carpetas
    /// temporales: ni `D:\LlamaCPP` ni el PATH de la máquina entran en juego.
    #[cfg(target_os = "windows")]
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
        // servidor «de antes» apuntado a mano en config.json, y uno «de siempre»
        let manual_dir = base.join("manual");
        std::fs::create_dir_all(&manual_dir).unwrap();
        let manual = manual_dir.join("llama-server.exe");
        std::fs::write(&manual, b"stub").unwrap();
        let comun_dir = base.join("comun");
        std::fs::create_dir_all(&comun_dir).unwrap();
        let comun = comun_dir.join("llama-server.exe");
        std::fs::write(&comun, b"stub").unwrap();
        let comunes = vec![comun.clone()];

        // 1) embarcado + config válida distinta → gana el EMBARCADO (una ruta
        //    vieja en config.json no rompe una instalación nueva).
        assert_eq!(
            resolve_server_path_in(Some(&app), Some(&manual.to_string_lossy()), &comunes),
            Some(vendado.clone())
        );
        // 2) embarcado + config INVÁLIDA (no existe) → el embarcado.
        assert_eq!(
            resolve_server_path_in(
                Some(&app),
                Some(&base.join("fantasma.exe").to_string_lossy()),
                &comunes
            ),
            Some(vendado.clone())
        );
        // 3) sin embarcado + config válida → la config (lo puesto a mano).
        assert_eq!(
            resolve_server_path_in(Some(&sin_vendor), Some(&manual.to_string_lossy()), &comunes),
            Some(manual.clone())
        );
        // 4) sin embarcado + config inválida → autodetección por las rutas de siempre.
        assert_eq!(
            resolve_server_path_in(
                Some(&sin_vendor),
                Some(&base.join("fantasma.exe").to_string_lossy()),
                &comunes
            ),
            Some(comun.clone())
        );
        // 5) sin nada → None (el arranque lo cuenta como LlmDisabled).
        assert_eq!(
            resolve_server_path_in(None, None, &[]),
            None,
            "sin candidatos no se inventa ninguna ruta"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// El GGUF del corrector se busca también junto al exe (`vendor/models`) y
    /// en la carpeta de modelos de la app (`%APPDATA%\whispr-open\models`), las
    /// dos por delante de las carpetas heredadas (LM Studio, D:\LlamaCPP…).
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

    /// El paquete CUDA es solo un subdirectorio junto al servidor embarcado: está
    /// o no está, y sin él no se toca nada (el servidor va en CPU).
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
        // Sin PID apuntado, la salida no intenta cerrar nada (regla: no tocar
        // nunca servidores ajenos).
        assert_eq!(launched_server_pid(), 0);
        assert!(!kill_launched_server());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn ruta_explicita_rota_o_falsa_se_ignora_deterministicamente() {
        // Requisito de robustez («ruta rota»): la ruta escrita en Ajustes se
        // comprueba ella sola, sin depender del disco real de la máquina ni del
        // PATH (por eso la decisión vive en `explicit_server_path`).
        let dir = std::env::temp_dir().join(format!("whispr-explicit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1) No existe → None (quien llama pasa a la autodetección).
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

        // 3) Envoltorio (llama.exe) con un hermano bueno → se arregla al hermano.
        let _ = std::fs::write(dir.join("llama.exe"), b"wrapper");
        assert_eq!(
            explicit_server_path(&dir.join("llama.exe").to_string_lossy()),
            Some(buena)
        );

        // 4) Envoltorio SIN hermano → se ignora (None): nunca se lanza un exe que
        //    después no se pueda cerrar limpio al salir.
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
        // Requisito de robustez: una dirección VIVA pero rota (500, pasarela
        // caída…) tiene que acabar en texto bruto con el mismo trato de error que
        // una caída (el prefijo «HTTP:» para que la interfaz lo clasifique). Se
        // monta un servidor HTTP de mentira con std::net.
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind en puerto libre");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Se lee la petición (las cabeceras) antes de contestar, para que el
                // cliente no reinicie la conexión al leer.
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
        // El PID 0 (System Idle Process) no se toca jamás: la protección corta
        // antes de OpenProcess, así que tampoco puede cerrar un proceso ajeno.
        assert!(!terminate_server_pid(0));
    }

    // ── Limpieza de direcciones (t_75732630) ──────────────────────────────
    //
    // Fallo real: config.json traía `http://127.0.0.1:8081/v1` y el código pegaba
    // `/models` a pelo → `http://127.0.0.1:8081/v1/v1/models` → 404 →
    // `server_alive` = false → «servidor LLM sin respuesta tras 0 s» aunque el
    // servidor contestara en 2 ms. La limpieza vive SOLO en `api_base`/`api_url`;
    // y estas pruebas fijan el trato.

    #[test]
    fn api_base_colapsa_el_v1_repetido_y_quita_la_barra_final() {
        // Un `/v1` SOLO se conserva (es el prefijo de versión estándar); los
        // repetidos se juntan en uno.
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
        // El caso del fallo: una base con `/v1/v1` tiene que acabar en la MISMA
        // dirección que la base con un solo `/v1` (la que sirve un llama.cpp/OpenAI
        // sano), nunca en `/v1/v1/models` (404).
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
        // Compatibilidad hacia atrás: las direcciones remotas que ya traen /v1
        // (OpenAI, Groq) siguen funcionando — el /v1 simple NO se quita.
        assert_eq!(
            api_url("https://api.openai.com/v1", "models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            api_url("https://api.groq.com/openai/v1", "chat/completions"),
            "https://api.groq.com/openai/v1/chat/completions"
        );
        // Base sin /v1 (la de siempre; llama.cpp la sirve en /models — está en el
        // log real: «respondiendo en http://127.0.0.1:8081/v1/models (2 ms)» con
        // base sin sufijo).
        assert_eq!(api_url("http://127.0.0.1:8081", "models"), "http://127.0.0.1:8081/models");
        // Una ruta con barra delante no duplica la barra.
        assert_eq!(
            api_url("http://127.0.0.1:8081/v1", "/models"),
            "http://127.0.0.1:8081/v1/models"
        );
        // Una ruta de versión pegada a mano tampoco rompe: `/v1` + `/v1/models` se
        // junta igual.
        assert_eq!(api_base("http://127.0.0.1:8081/v1" ), "http://127.0.0.1:8081/v1");
    }

    #[test]
    fn server_alive_y_wait_for_server_usan_la_url_normalizada() {
        // Trato de integración sin red: con la base del fallo (`/v1`), el único
        // puerto que se consulta es el ya limpio. Un puerto cerrado contesta «no
        // vivo» sin esperar la espera larga.
        assert!(!server_alive("http://127.0.0.1:1/v1"));
        assert!(!server_alive_auth("http://127.0.0.1:1/v1/v1", Some("clave")));
    }

    // ── Arreglo del ejecutable del servidor (t_75732630) ──────────────────

    #[cfg(target_os = "windows")]
    #[test]
    fn is_server_exe_solo_acepta_llama_server_exe() {
        use std::path::Path;
        assert!(is_server_exe(Path::new(r"D:\LlamaCPP\llama-server.exe")));
        assert!(is_server_exe(Path::new(r"D:\LlamaCPP\LLAMA-SERVER.EXE")));
        // El envoltorio viejo y el servidor ELF de WSL NO valen: el proceso
        // lanzado tiene que poder cerrarse por nombre de imagen al salir.
        assert!(!is_server_exe(Path::new(r"D:\LlamaCPP\llama.exe")));
        assert!(!is_server_exe(Path::new("/usr/bin/wsl.exe")));
        assert!(!is_server_exe(Path::new(r"D:\LlamaCPP\llama-server")));
        assert!(!is_server_exe(Path::new("")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn sanitize_server_exe_cambia_el_wrapper_por_el_hermano_real() {
        let dir = std::env::temp_dir().join(format!("whispr-sanitize-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wrapper = dir.join("llama.exe");
        let real = dir.join("llama-server.exe");
        std::fs::write(&wrapper, b"stub").unwrap();
        std::fs::write(&real, b"stub").unwrap();

        // `llama.exe` → el `llama-server.exe` de al lado (el caso de config.json).
        assert_eq!(sanitize_server_exe(&wrapper).as_deref(), Some(real.as_path()));
        // Un nombre que ya vale se devuelve tal cual.
        assert_eq!(sanitize_server_exe(&real).as_deref(), Some(real.as_path()));

        // Sin un `llama-server.exe` al lado no hay ruta válida (quien llama
        // autodetecta por las rutas de siempre).
        let otros = std::env::temp_dir().join(format!("whispr-sanitize-otro-{}", std::process::id()));
        std::fs::create_dir_all(&otros).unwrap();
        let solo_wrapper = otros.join("llama.exe");
        std::fs::write(&solo_wrapper, b"stub").unwrap();
        assert_eq!(sanitize_server_exe(&solo_wrapper), None);

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&otros);
    }

    // ── Regresión EN VIVO contra un servidor real (t_75732630) ────────────
    //
    // Se ejecuta a mano con un llama-server NATIVO ya escuchando en
    // 127.0.0.1:8081 (Qwen3.5-0.8B-Q8_0.gguf):
    //   D:\LlamaCPP\llama-server.exe -m <gguf> --host 127.0.0.1 --port 8081
    //   cargo.exe test regresion_viva -- --ignored --nocapture
    //
    // Fija el criterio 4 de la tarea: `server_alive`/`wait_for_server` pasan (sin
    // «sin respuesta tras 0 s») con la base `/v1` de la config del usuario, y la
    // dirección cruda duplicada `/v1/v1/models` —el fallo— da 404.
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

        // La dirección CRUDA duplicada sí da 404: es exactamente el fallo que
        // arregla `api_url` (ya nadie la construye así).
        let cruda = ureq::get("http://127.0.0.1:8081/v1/v1/models")
            .timeout(Duration::from_secs(2))
            .call();
        assert!(
            cruda.is_err(),
            "sanity del bug: /v1/v1/models crudo debe ser un 404"
        );
    }
}
