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

use crate::domain::corrective::Diff;

/// Endpoint + modelo del corrector. Lo rellena `Config::llm_settings()` con
/// los defaults locales; `base_url`/`model`/`api_key`/`timeout_secs` son
/// override desde config.json.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LlmConfig {
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
    let value: serde_json::Value = serde_json::from_str(&body_raw)
        .map_err(|e| format!("JSON de respuesta inválido: {e}"))?;

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
        find: value.get("find").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        replace: value
            .get("replace")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_diff_plano() {
        let diff = parse_diff(r#"{"find": "4", "replace": "2"}"#).expect("json plano");
        assert_eq!(diff, Diff { find: "4".into(), replace: "2".into() });
    }

    #[test]
    fn parse_diff_con_fences_de_markdown() {
        let content = "```json\n{\"find\": \"la reunión\", \"replace\": \"el informe\"}\n```";
        let diff = parse_diff(content).expect("fences");
        assert_eq!(diff, Diff { find: "la reunión".into(), replace: "el informe".into() });
    }

    #[test]
    fn parse_diff_con_texto_alrededor() {
        let content = "Claro: {\"find\": \"jueves\", \"replace\": \"viernes\"} hecho.";
        let diff = parse_diff(content).expect("texto alrededor");
        assert_eq!(diff, Diff { find: "jueves".into(), replace: "viernes".into() });
    }

    #[test]
    fn parse_diff_vacio_cuando_no_localiza() {
        let diff = parse_diff(r#"{"find": "", "replace": ""}"#).expect("json vacío");
        assert_eq!(diff, Diff { find: String::new(), replace: String::new() });
    }

    #[test]
    fn parse_diff_tolerante_a_clave_faltante() {
        // El contrato exige find+replace, pero si el modelo omite una clave no
        // debemos romper: queda vacía y apply_diff la descarta si no aplica.
        let diff = parse_diff(r#"{"find": "4"}"#).expect("solo find");
        assert_eq!(diff, Diff { find: "4".into(), replace: String::new() });
    }

    #[test]
    fn parse_diff_rechaza_no_json() {
        assert!(parse_diff("no lo encuentro").is_err());
    }

    #[test]
    fn llm_config_defaults_local() {
        let cfg = LlmConfig::default();
        assert_eq!(cfg.base_url, "http://127.0.0.1:8081/v1");
        assert_eq!(cfg.model, "ornith-1.0-9b");
        assert_eq!(cfg.api_key, None);
        assert_eq!(cfg.timeout_secs, 25, "timeout por defecto: 25 s");
    }
}