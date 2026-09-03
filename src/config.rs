//! Config persistente de la app (reemplaza localStorage de la versión web).
//! Archivo JSON en %APPDATA%/whispr-open/config.json: diccionario, modelo
//! seleccionado y preferencias. Correcciones best-effort: nunca rompe el arranque.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::dictionary::DictionaryEntry;
use crate::transcribe::Model;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub dictionary: Vec<DictionaryEntry>,
    pub model: Option<String>,
    pub cleanup_enabled: Option<bool>,
    pub auto_inject: Option<bool>,
    pub input_device: Option<String>,
    /// Tema oscuro por defecto (2026: dark mode default); toggle persistido.
    pub dark_mode: Option<bool>,
    /// Corrección IA: post-procesado con LLM local de los turnos con
    /// intención correctiva («no no, era 2 en vez de 4»). Toggle en Ajustes;
    /// spec en docs/CORRECCION-VOZ.md. Default: activada.
    pub ai_correction_enabled: Option<bool>,
    /// Override del endpoint OpenAI-compatible del corrector. Default:
    /// `http://127.0.0.1:8081/v1` (llama.cpp/Ornith en WSL, alcanzable desde
    /// Windows por localhost forwarding). Groq/Gemini con su base_url.
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
    pub llm_api_key: Option<String>,
    /// Timeout de la llamada HTTP al LLM corrector, en segundos. Default
    /// 25 s (en el límite de los ~4-6 s esperados + margen para endpoints
    /// remotos Groq/Gemini; spec §3.4). Valores ≤ 0 caen al default.
    pub llm_timeout_secs: Option<u64>,
    /// Umbral de sensibilidad del clasificador correctivo (spec §2:
    /// «el umbral debe favorecer la precisión sobre el recall»). Rango
    /// 0..=100; default 50 (= conducta previa: todo lo que pasa la regla
    /// de los dos niveles va al LLM). Subir = más conservador (menos
    /// falsos positivos al LLM, más dictado normal por defecto). El
    /// clasificador mapea el nº de señales presentes a {50, 75, 100},
    /// así que el rango efectivo del usuario va de 50 (permisivo) a 100
    /// (imposible, nunca se enruta). 0 lo trata el clasificador como
    /// pasivo (atajo equivalente al toggle, conservando el resto de la
    /// config).
    pub llm_classifier_threshold: Option<u8>,
    /// Si true, las correcciones IA NO se aplican a ciegas: al volver del
    /// LLM la app muestra el diff en el panel de feedback con botones
    /// «Aplicar» / «Descartar». Default false (aplicar + aviso + undo,
    /// spec §4.2: «por fluidez»). El undo de 1 nivel sigue disponible
    /// también en modo confirmar.
    pub ai_correction_confirm: Option<bool>,
    /// Atajo global de dictado, formato `Mod1+Mod2+Tecla` (p. ej.
    /// "Ctrl+Shift+D", "Alt+Space", "Ctrl+Alt+F12"). Se registra al arrancar;
    /// si el valor es inválido o el atajo ya lo usa otra app, se vuelve al
    /// default y la UI sigue funcionando (botones). Default: Ctrl+Shift+D.
    pub shortcut: Option<String>,
    /// Formato de inyección del turno en la app activa: "paste" (default)
    /// copia al portapapeles y pega con Ctrl+V; "type" teclea el texto
    /// directamente (enigo, sin tocar el portapapeles del usuario).
    pub inject_format: Option<String>,
    /// Retardo en ms antes de pegar/teclear (la app destino asienta el foco).
    /// Default 40; `0` = sin retardo.
    pub inject_delay_ms: Option<u64>,
    /// Filtro por título de la ventana activa (substring, sin distinguir
    /// mayúsculas): si el título no lo contiene, el turno NO se inyecta.
    /// Vacío/ausente = inyectar en cualquier app.
    pub inject_window_filter: Option<String>,
    /// Idioma de la transcripción. Vacío o "auto" (sin distinguir
    /// mayúsculas) = autodetección de whisper (se pasa `None` al motor);
    /// cualquier otro valor se trimea y se trata como BCP-47 (`"es"`,
    /// `"en"`, `"fr"`, …). Valores desconocidos NO se validan aquí:
    /// whisper los acepta y puede caer a `"en"` silenciosamente.
    /// Default: `"es"` (lo que la app asumía hardcoded).
    pub language: Option<String>,
    /// Beam size de `SamplingStrategy::BeamSearch` de whisper. Rango útil
    /// 1..=8 (1 = greedy equivalente); >8 es prohibitivamente lento sin
    /// ganancia apreciable. Default: 5 (lo que ya usaba la app como
    /// constante). Fuera de rango cae al default.
    pub beam_size: Option<u32>,
    /// Hilos del decoder de whisper. Más hilos = más paralelismo, pero el
    /// cuello suele estar en GPU (small + cuda). Default: 4 (lo que ya
    /// usaba la app como constante). El sistema físico lo respeta.
    pub n_threads: Option<u32>,
    /// Remuestreo automático del audio del micro a 16 kHz (lo que exige
    /// whisper.cpp). Default: true. La opción existe como perilla
    /// informativa: si está apagada y el dispositivo NO está a 16 kHz,
    /// whisper fallaría; la app registra un aviso y remuestrea igual.
    pub auto_resample_to_16k: Option<bool>,
}

pub fn config_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("config.json")
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, raw);
        }
    }

    pub fn selected_model(&self) -> Model {
        match self.model.as_deref() {
            Some("base") => Model::Base,
            Some("small") => Model::Small,
            // Default (sin config) y valores inválidos: Small. Base/tiny en
            // español producen errores fonéticos (buscar→boscar); small es el
            // mínimo razonable y la RTX 4070 lo corre en tiempo real.
            _ => Model::Small,
        }
    }

    /// Tema: dark por defecto (2026); el toggle persiste `dark_mode`.
    pub fn is_dark(&self) -> bool {
        self.dark_mode.unwrap_or(true)
    }

    /// Corrección IA activada (toggle en Ajustes; default: on).
    pub fn ai_correction_enabled(&self) -> bool {
        self.ai_correction_enabled.unwrap_or(true)
    }

    /// Atajo global de dictado: valor de `shortcut` en config.json o el
    /// default Ctrl+Shift+D si está ausente/en blanco. El parseo fino (y el
    /// volver a default si el string es inválido) vive en `hotkey.rs`.
    pub fn shortcut(&self) -> String {
        match self.shortcut.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => crate::hotkey::DEFAULT_SHORTCUT.to_string(),
        }
    }

    /// Comportamiento de inyección desde config.json: formato (paste/type),
    /// retardo en ms (default 40, 0 = sin retardo) y filtro por título de la
    /// ventana activa. Valores inválidos/ausentes → defaults seguros.
    pub fn inject_settings(&self) -> crate::inject::InjectSettings {
        crate::inject::InjectSettings {
            format: match self.inject_format.as_deref() {
                Some("type") => crate::inject::InjectFormat::Type,
                _ => crate::inject::InjectFormat::Paste,
            },
            delay: Duration::from_millis(self.inject_delay_ms.unwrap_or(40)),
            window_filter: self
                .inject_window_filter
                .as_deref()
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(str::to_string),
        }
    }

    /// Configuración del LLM corrector. Defaults: llama.cpp local en WSL
    /// (Ornith 9B, puerto 8081); overridables en config.json. El timeout
    /// sale de `llm_timeout_secs` (default 25 s).
    pub fn llm_settings(&self) -> crate::llm::LlmConfig {
        crate::llm::LlmConfig {
            base_url: self
                .llm_base_url
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:8081/v1".to_string()),
            model: self
                .llm_model
                .clone()
                .unwrap_or_else(|| "ornith-1.0-9b".to_string()),
            api_key: self.llm_api_key.clone(),
            timeout_secs: self.llm_timeout_secs().unwrap_or(25),
        }
    }

    /// Timeout HTTP del corrector en segundos. Default 25; 0 cae al
    /// default también (un timeout de 0 cortaría toda llamada).
    pub fn llm_timeout_secs(&self) -> Option<u64> {
        match self.llm_timeout_secs {
            Some(t) if t > 0 => Some(t),
            _ => None,
        }
    }

    /// Umbral del clasificador (0..=100). Default 50 (= conducta previa:
    /// todo lo que pasa la regla de los dos niveles va al LLM). Subir = más
    /// conservador (precision > recall, spec §2). El clasificador mapea el
    /// nº de señales presentes a {50, 75, 100}, así que el rango efectivo
    /// del usuario va de 50 (permisivo) a 100 (imposible, nunca se enruta).
    /// 0 lo trata el clasificador como pasivo.
    pub fn llm_classifier_threshold(&self) -> u8 {
        self.llm_classifier_threshold.unwrap_or(50)
    }

    /// Modo «confirmar antes de aplicar»: si true, el diff del LLM se
    /// muestra con botones «Aplicar» / «Descartar» en vez de aplicarse
    /// automáticamente (spec §4.2). Default false.
    pub fn ai_correction_confirm(&self) -> bool {
        self.ai_correction_confirm.unwrap_or(false)
    }

    /// Idioma para `whisper_full_params::language`. Vacío o "auto" (sin
    /// distinguir mayúsculas) = autodetección (el motor NO recibe
    /// `set_language` y whisper decide). Cualquier otro valor se trimea
    /// y se devuelve como BCP-47 (`"es"`, `"en"`, …). Valores
    /// desconocidos NO se validan aquí: whisper los acepta.
    ///
    /// Default seguro cuando el campo está ausente o vacío:
    /// `Some("es")` (lo que la app asumía hardcoded).
    pub fn language(&self) -> Option<&str> {
        match self.language.as_deref().map(str::trim) {
            Some(s) if s.is_empty() => Some("es"),
            Some(s) if s.eq_ignore_ascii_case("auto") => None,
            Some(s) => Some(s),
            None => Some("es"),
        }
    }

    /// Beam size efectivo del SamplingStrategy. Por defecto 5 (lo que ya
    /// usaba la app como constante). Clamp 1..=8: 1 = greedy equivalente,
    /// >8 es prohibitivamente lento en CPU y apenas gana calidad. Valores
    /// fuera de rango caen al default.
    pub fn beam_size(&self) -> u32 {
        match self.beam_size {
            Some(n @ 1..=8) => n,
            _ => 5,
        }
    }

    /// Hilos del decoder de whisper. Por defecto 4 (lo que ya usaba la
    /// app como constante). Mínimo 1; tope blando 32 (más hilos no
    /// aportan, pero se permite por si el usuario sabe lo que hace).
    pub fn n_threads(&self) -> u32 {
        match self.n_threads {
            Some(n) if n >= 1 => n.min(32),
            _ => 4,
        }
    }

    /// ¿Remuestrear el audio del micro a 16 kHz automáticamente?
    /// Por defecto true. Whisper.cpp exige 16 kHz; si el dispositivo NO
    /// está a esa frecuencia y la opción está apagada, el caller
    /// (`audio::AudioCapture::stop`) registra un aviso y devuelve las
    /// muestras al rate nativo — el llamador puede detectar el problema
    /// y forzar el remuestreo si va a pasarle las muestras a whisper.
    pub fn auto_resample_16k(&self) -> bool {
        self.auto_resample_to_16k.unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let cfg = Config::default();
        assert_eq!(cfg.selected_model(), Model::Small);
        assert!(cfg.dictionary.is_empty());
        assert_eq!(cfg.cleanup_enabled, None);
    }

    #[test]
    fn config_path_lives_under_appdata() {
        assert!(config_path().to_string_lossy().contains("whispr-open"));
    }

    #[test]
    fn invalid_model_falls_back_to_small() {
        let cfg = Config { model: Some("gigante".into()), ..Default::default() };
        assert_eq!(cfg.selected_model(), Model::Small);
    }

    #[test]
    fn dark_mode_por_defecto() {
        let cfg = Config::default();
        assert!(cfg.is_dark(), "2026: dark mode default");
        let claro = Config { dark_mode: Some(false), ..Default::default() };
        assert!(!claro.is_dark());
    }

    #[test]
    fn inyeccion_defaults_y_overrides() {
        let cfg = Config::default();
        let s = cfg.inject_settings();
        assert_eq!(s.format, crate::inject::InjectFormat::Paste);
        assert_eq!(s.delay.as_millis(), 40);
        assert_eq!(s.window_filter, None);

        let c = Config {
            inject_format: Some("type".into()),
            inject_delay_ms: Some(0),
            inject_window_filter: Some(" Visual Studio Code ".into()),
            ..Default::default()
        };
        let s = c.inject_settings();
        assert_eq!(s.format, crate::inject::InjectFormat::Type);
        assert_eq!(s.delay.as_millis(), 0);
        assert_eq!(s.window_filter.as_deref(), Some("Visual Studio Code"));

        // Formato desconocido → paste (default seguro); filtro en blanco → sin filtro.
        let c = Config { inject_format: Some("escaneo".into()), ..Default::default() };
        assert_eq!(c.inject_settings().format, crate::inject::InjectFormat::Paste);
        let c = Config { inject_window_filter: Some("  ".into()), ..Default::default() };
        assert_eq!(c.inject_settings().window_filter, None);
    }

    #[test]
    fn atajo_global_default_y_personalizado() {
        let cfg = Config::default();
        assert_eq!(cfg.shortcut(), "Ctrl+Shift+D");
        let c = Config { shortcut: Some("Alt+Space".into()), ..Default::default() };
        assert_eq!(c.shortcut(), "Alt+Space");
        // En blanco/ausente → default (la config nunca rompe el arranque).
        let c = Config { shortcut: Some("   ".into()), ..Default::default() };
        assert_eq!(c.shortcut(), "Ctrl+Shift+D");
    }

    #[test]
    fn correction_ia_defaults_on_y_local() {
        let cfg = Config::default();
        assert!(cfg.ai_correction_enabled(), "corrección IA activada por defecto");
        let llm = cfg.llm_settings();
        assert_eq!(llm.base_url, "http://127.0.0.1:8081/v1");
        assert_eq!(llm.model, "ornith-1.0-9b");
        assert_eq!(llm.api_key, None);
        // Override de endpoint (p. ej. Groq/Gemini) via config.json.
        let remoto = Config {
            llm_base_url: Some("https://api.groq.com/openai/v1".into()),
            llm_model: Some("llama-3.3-70b-versatile".into()),
            llm_api_key: Some("clave".into()),
            ..Default::default()
        };
        let l2 = remoto.llm_settings();
        assert_eq!(l2.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(l2.model, "llama-3.3-70b-versatile");
        assert_eq!(l2.api_key.as_deref(), Some("clave"));
        // Toggle off persistido.
        let off = Config { ai_correction_enabled: Some(false), ..Default::default() };
        assert!(!off.ai_correction_enabled());
    }

    #[test]
    fn timeout_umbral_y_confirm_tienen_defaults_seguros() {
        let cfg = Config::default();
        // Default 25 s y aplicado al LlmConfig.
        assert_eq!(cfg.llm_timeout_secs(), None, "None = cae al default de 25");
        assert_eq!(cfg.llm_settings().timeout_secs, 25);
        // Default 50 del clasificador (= conducta previa: pasa todo lo que
        // cumple los dos niveles).
        assert_eq!(cfg.llm_classifier_threshold(), 50);
        // Default false: aplicar + aviso + undo (spec §4.2: fluidez).
        assert!(!cfg.ai_correction_confirm());

        // Override de timeout desde config.json.
        let c = Config { llm_timeout_secs: Some(45), ..Default::default() };
        assert_eq!(c.llm_timeout_secs(), Some(45));
        assert_eq!(c.llm_settings().timeout_secs, 45);
        // 0 cae al default (timeout de 0 cortaría toda llamada).
        let c = Config { llm_timeout_secs: Some(0), ..Default::default() };
        assert_eq!(c.llm_timeout_secs(), None);
        assert_eq!(c.llm_settings().timeout_secs, 25);

        // Override del umbral y modo confirmar.
        let c = Config {
            llm_classifier_threshold: Some(85),
            ai_correction_confirm: Some(true),
            ..Default::default()
        };
        assert_eq!(c.llm_classifier_threshold(), 85);
        assert!(c.ai_correction_confirm());
    }

    #[test]
    fn language_default_es_y_auto_y_overrides() {
        // Default seguro: español (lo que la app asumía hardcoded).
        let cfg = Config::default();
        assert_eq!(cfg.language(), Some("es"));

        // Vacío o None también caen al default español (la config nunca
        // rompe el arranque; el usuario debe poner "auto" explícitamente).
        let c = Config { language: None, ..Default::default() };
        assert_eq!(c.language(), Some("es"));
        let c = Config { language: Some("".into()), ..Default::default() };
        assert_eq!(c.language(), Some("es"));
        let c = Config { language: Some("  ".into()), ..Default::default() };
        assert_eq!(c.language(), Some("es"));

        // "auto" (cualquier casing) → autodetección (None).
        let c = Config { language: Some("auto".into()), ..Default::default() };
        assert_eq!(c.language(), None);
        let c = Config { language: Some("AUTO".into()), ..Default::default() };
        assert_eq!(c.language(), None);
        let c = Config { language: Some("Auto".into()), ..Default::default() };
        assert_eq!(c.language(), None);
        let c = Config { language: Some("  auto  ".into()), ..Default::default() };
        assert_eq!(c.language(), None);

        // Overrides legítimos (trimeo + BCP-47).
        let c = Config { language: Some("en".into()), ..Default::default() };
        assert_eq!(c.language(), Some("en"));
        let c = Config { language: Some("  fr  ".into()), ..Default::default() };
        assert_eq!(c.language(), Some("fr"));
    }

    #[test]
    fn beam_size_default_y_rango_valido() {
        // Default 5 (lo que ya usaba la app).
        assert_eq!(Config::default().beam_size(), 5);

        // Valores válidos: 1..=8.
        for n in 1..=8 {
            let c = Config { beam_size: Some(n), ..Default::default() };
            assert_eq!(c.beam_size(), n, "beam_size válido: {n}");
        }
        // Fuera de rango → default (la config nunca rompe el arranque).
        let c = Config { beam_size: Some(0), ..Default::default() };
        assert_eq!(c.beam_size(), 5);
        let c = Config { beam_size: Some(9), ..Default::default() };
        assert_eq!(c.beam_size(), 5);
        let c = Config { beam_size: Some(999), ..Default::default() };
        assert_eq!(c.beam_size(), 5);
        let c = Config { beam_size: None, ..Default::default() };
        assert_eq!(c.beam_size(), 5);
    }

    #[test]
    fn n_threads_default_y_rango() {
        // Default 4 (lo que ya usaba la app).
        assert_eq!(Config::default().n_threads(), 4);

        // Mínimo 1.
        let c = Config { n_threads: Some(1), ..Default::default() };
        assert_eq!(c.n_threads(), 1);
        // Valores razonables.
        let c = Config { n_threads: Some(8), ..Default::default() };
        assert_eq!(c.n_threads(), 8);
        let c = Config { n_threads: Some(16), ..Default::default() };
        assert_eq!(c.n_threads(), 16);
        // Tope blando 32 (más hilos no aportan pero se permite).
        let c = Config { n_threads: Some(64), ..Default::default() };
        assert_eq!(c.n_threads(), 32);
        // 0 y None → default.
        let c = Config { n_threads: Some(0), ..Default::default() };
        assert_eq!(c.n_threads(), 4);
        let c = Config { n_threads: None, ..Default::default() };
        assert_eq!(c.n_threads(), 4);
    }

    #[test]
    fn auto_resample_16k_default_y_personalizado() {
        // Default: true (whisper.cpp exige 16 kHz; la app ya remuestreaba).
        assert!(Config::default().auto_resample_16k());
        let c = Config { auto_resample_to_16k: Some(false), ..Default::default() };
        assert!(!c.auto_resample_16k());
        let c = Config { auto_resample_to_16k: Some(true), ..Default::default() };
        assert!(c.auto_resample_16k());
    }

    #[test]
    fn serializa_y_relee_campos_nuevos_de_transcripcion() {
        // Roundtrip por JSON (lo que pasa con config.json en disco).
        let original = Config {
            language: Some("en".into()),
            beam_size: Some(3),
            n_threads: Some(8),
            auto_resample_to_16k: Some(false),
            ..Default::default()
        };
        let raw = serde_json::to_string(&original).expect("serializa");
        let leido: Config = serde_json::from_str(&raw).expect("deserializa");
        assert_eq!(leido.language(), Some("en"));
        assert_eq!(leido.beam_size(), 3);
        assert_eq!(leido.n_threads(), 8);
        assert!(!leido.auto_resample_16k());

        // Compatibilidad hacia atrás: un config.json viejo sin los campos
        // nuevos se deserializa sin error y los getters devuelven los defaults.
        // (Los campos «viejos» como `dictionary` sí son requeridos; lo que
        // se prueba aquí es que los NUEVOS campos son opcionales.)
        let viejo = r#"{"dictionary":[]}"#;
        let cfg: Config = serde_json::from_str(viejo).expect("json viejo");
        assert_eq!(cfg.language(), Some("es"));
        assert_eq!(cfg.beam_size(), 5);
        assert_eq!(cfg.n_threads(), 4);
        assert!(cfg.auto_resample_16k());
        assert!(cfg.dictionary.is_empty());
    }
}
