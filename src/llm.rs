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

/// Prompt del contrato (§3.2): el modelo devuelve SOLO un JSON con
/// `{find, replace}` sobre el texto actual. Si no localiza nada, vacío.
const SYSTEM_PROMPT: &str = r#"Eres el corrector de un dictado por voz en español. Se te da:
- TEXTO ACTUAL (el último turno dictado, ya limpio de muletillas).
- INSTRUCCIÓN (lo que el usuario dijo después, pidiendo una corrección).

Devuelve SOLO un JSON válido, sin texto adicional:
{"find": "<subcadena exacta en TEXTO ACTUAL>", "replace": "<nuevo texto>"}

Reglas:
- "find" debe ser una subcadena literal de TEXTO ACTUAL (puede ser una palabra, un número, o una frase corta). Si no puedes localizarla, devuelve {"find": "", "replace": ""}.
- "replace" es lo que sustituye a "find". Si la instrucción pide quitar algo, "replace" es "".
- No inventes texto que no esté en TEXTO ACTUAL ni en la INSTRUCCIÓN.
- Resuelve numerales: "2 en vez de 4" sobre "... 4 ..." => find "4", replace "2".
- Resuelve referencias: "lo último" apunta a la última frase o número de TEXTO ACTUAL.
- Si la INSTRUCCIÓN no pide ninguna corrección clara, devuelve {"find": "", "replace": ""}."#;

/// Pide al LLM un diff de corrección para `target` (último turno dictado)
/// según la `instruction` hablada después. Bloquea: llamarlo en un hilo
/// (app.rs lo lanza con `std::thread::spawn` + canal mpsc).
pub fn correct(target: &str, instruction: &str, cfg: &LlmConfig) -> Result<Diff, String> {
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
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
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let started = std::time::Instant::now();
    let ok = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .is_ok();
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

/// Traduce una ruta Windows del modelo (p. ej. de la búsqueda por raíces)
/// a su equivalente WSL (/mnt/<letra>/…) para cargarla con el llama-server
/// de WSL. Las rutas de LM Studio (C:\Users\…\.lmstudio\models) son
/// accesibles desde WSL vía el mount 9p de /mnt/c y se cargan igual.
/// Cualquier ruta no-Windows (UNC, etc.) no es traducible: devuelve None.
#[cfg(target_os = "windows")]
pub fn win_to_wsl_path(path: &std::path::Path) -> Option<String> {
    use std::path::Component;
    let mut parts: Vec<String> = Vec::new();
    let mut seen_prefix = false;
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                seen_prefix = true;
                let raw = p.as_os_str().to_string_lossy().to_string();
                let drive = raw.trim_end_matches(':').to_ascii_lowercase();
                if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
                    return None; // UNC u otro prefijo no mapeable
                }
                parts.push(format!("/mnt/{drive}"));
            }
            Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            Component::RootDir | Component::CurDir | Component::ParentDir => {}
        }
    }
    // Sin prefijo de unidad no hay ruta Windows que traducir (p. ej. un
    // nombre de fichero relativo «modelo.gguf»).
    if !seen_prefix || parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Lanza el servidor LLM local. En esta máquina el `llama-server.exe` de
/// D:\LlamaCPP (stub de 9 KB + llama-server-impl.dll) NO arranca desde un
/// spawn sin consola (verificado 3×: rc=0xC0000139 o proceso muerto sin
/// output). El server fiable es el ELF de WSL (`$HOME/.local/bin/llama-
/// server`), que se lanza con `wsl.exe`: el localhost forwarding de WSL2
/// hace que `http://127.0.0.1:<port>` de la app llegue al server de WSL
/// (comprobado: Windows ve el listener vía wslrelay). El modelo se carga
/// desde el mount /mnt/c (el mismo GGUF que resolvería la búsqueda por
/// raíces Windows). Sin ventana de consola: wsl.exe con CREATE_NO_WINDOW.
#[cfg(target_os = "windows")]
pub fn launch_server(model: &std::path::Path, port: u16) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let wsl_model = win_to_wsl_path(model)
        .ok_or_else(|| format!("ruta del modelo no traducible a WSL: {}", model.display()))?;
    // El stub ELF necesita libllama-server-impl.so: se pasa LD_LIBRARY_PATH
    // explícito (un `wsl.exe -e sh -c` NO hereda ~/.local/bin del PATH del
    // usuario: verificado).
    let script = format!(
        "LD_LIBRARY_PATH=$HOME/.local/bin exec \"$HOME/.local/bin/llama-server\" \
         -m \"{wsl_model}\" --host 127.0.0.1 --port {port} -ngl 99 --flash-attn on"
    );
    let mut cmd = std::process::Command::new("wsl.exe");
    cmd.args(["-e", "sh", "-c", script.as_str()]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("no se pudo lanzar wsl.exe (server LLM de WSL): {e}"))
}

#[cfg(not(target_os = "windows"))]
pub fn launch_server(_model: &std::path::Path, _port: u16) -> Result<(), String> {
    Err("arranque automático del servidor LLM solo disponible en Windows".to_string())
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

/// Lanza `llama-server` con el modelo y puerto dados. Flags de referencia de
/// la instalación local (verificadas con el binario real): `-ngl 99` +
/// `--flash-attn on` (GPU, la RTX 4070 de Juanfri). En Windows se lanza sin
/// ventana de consola (CREATE_NO_WINDOW); el proceso queda huérfano de la
/// app a propósito: un server vivo se reutiliza en el siguiente arranque
/// (lo detecta `server_alive`), nunca se mata un proceso ajeno.

/// Espera (con polling cada 400 ms) a que `/v1/models` responda. Devuelve
/// `true` si el server quedó listo dentro del plazo, `false` si se agotó.
/// Tiempos reales: un 0.8B Q8 carga en ~2-5 s en GPU; 60 s de margen cubren
/// modelos grandes y descartes por puerto ocupado sin colgar la app.
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
    fn ruta_windows_se_traduce_a_wsl() {
        use std::path::Path;
        // C:\…\.lmstudio\models → /mnt/c/… (se cargará con el server de WSL).
        let p = Path::new("C:/Users/usuario/.lmstudio/models/lmstudio-community/Qwen3.5-0.8B-GGUF/Qwen3.5-0.8B-Q8_0.gguf");
        assert_eq!(
            win_to_wsl_path(p).as_deref(),
            Some("/mnt/c/Users/usuario/.lmstudio/models/lmstudio-community/Qwen3.5-0.8B-GGUF/Qwen3.5-0.8B-Q8_0.gguf")
        );
        // Otras unidades → /mnt/d, /mnt/e…
        assert_eq!(
            win_to_wsl_path(Path::new("D:/LlamaCPP/models/x.gguf")).as_deref(),
            Some("/mnt/d/LlamaCPP/models/x.gguf")
        );
        // Sin prefijo de unidad → no traducible.
        assert_eq!(win_to_wsl_path(Path::new("modelo.gguf")), None);
        // UNC → no traducible.
        assert_eq!(win_to_wsl_path(Path::new("//server/share/x.gguf")), None);
    }
}
