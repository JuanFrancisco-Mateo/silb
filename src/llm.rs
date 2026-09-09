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
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": format!("TEXTO ACTUAL: {target}\nINSTRUCCIÓN: {instruction}") },
        ],
        "temperature": 0.0,
        "max_tokens": 120,
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

    let content = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "la respuesta del LLM no trae content".to_string())?;
    parse_diff(content)
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

    // Si sobran palabras fuera del objeto, quedarse con el primer {…}.
    let slice = match (s.find('{'), s.rfind('}')) {
        (Some(a), Some(b)) if b > a => &s[a..=b],
        _ => s,
    };

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
