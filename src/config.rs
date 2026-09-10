//! Config persistente de la app (reemplaza localStorage de la versión web).
//! Archivo JSON en %APPDATA%/whispr-open/config.json: diccionario, modelo
//! seleccionado y preferencias. Correcciones best-effort: nunca rompe el arranque.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::cleanup::FillerWords;
use crate::domain::dictionary::DictionaryEntry;
use crate::transcribe::Model;

/// Proveedor de corrección IA. Permite elegir entre el LLM local por
/// defecto (llama.cpp, el que trae la app), un proveedor cloud
/// (OpenAI) o un endpoint propio OpenAI-compatible (Custom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorrectionProvider {
    Default,
    Cloud,
    Custom,
}

impl CorrectionProvider {
    /// Etiqueta del selector de Ajustes. Los nombres son los del
    /// requisito UX 10/09/2026: dejar claro que Local es el que viene
    /// con la app, y que Custom es cualquier endpoint OpenAI-compatible.
    pub fn label(self) -> &'static str {
        match self {
            CorrectionProvider::Default => "Local (llama.cpp, el que viene con la app)",
            CorrectionProvider::Cloud => "Cloud (OpenAI)",
            CorrectionProvider::Custom => "Custom LLM (endpoint OpenAI-compatible)",
        }
    }
}

impl std::fmt::Display for CorrectionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Modo de tema de la app. `Auto` sigue el tema del SISTEMA Windows
/// (`AppsUseLightTheme` del registro, cacheado en el Poll cada 5 s);
/// `Claro`/`Oscuro` fuerzan un modo manual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Auto,
    Claro,
    Oscuro,
}

impl ThemeMode {
    /// Siguiente modo en el ciclo del botón del header: Auto → Claro → Oscuro.
    pub fn next(self) -> ThemeMode {
        match self {
            ThemeMode::Auto => ThemeMode::Claro,
            ThemeMode::Claro => ThemeMode::Oscuro,
            ThemeMode::Oscuro => ThemeMode::Auto,
        }
    }

    /// ¿Este modo fuerza un lado concreto? `Auto` no fuerza (None): decide el
    /// sistema operativo.
    pub fn forced_dark(self) -> Option<bool> {
        match self {
            ThemeMode::Auto => None,
            ThemeMode::Claro => Some(false),
            ThemeMode::Oscuro => Some(true),
        }
    }

    /// Etiqueta en español para el `pick_list` de Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Claro => "Claro",
            ThemeMode::Oscuro => "Oscuro",
        }
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for ThemeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(ThemeMode::Auto),
            "claro" | "light" => Ok(ThemeMode::Claro),
            "oscuro" | "dark" => Ok(ThemeMode::Oscuro),
            other => Err(format!("modo de tema desconocido: {other}")),
        }
    }
}

/// Estilo visual de la burbuja flotante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleStyle {
    Solid,
    Ring,
    Minimal,
}

impl BubbleStyle {
    /// Etiqueta en español para el `pick_list` de Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            BubbleStyle::Solid => "Sólida",
            BubbleStyle::Ring => "Anillo",
            BubbleStyle::Minimal => "Minimal",
        }
    }
}

impl std::fmt::Display for BubbleStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Tamaño de fuente global de la UI (Ajustes → Apariencia). Factores sobre
/// los tamaños base de `theme.rs`: 0.85 / 1.0 / 1.15 / 1.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontSize {
    Small,
    Normal,
    Large,
    XLarge,
}

impl FontSize {
    /// Factor multiplicador sobre los tamaños base de `theme.rs`.
    pub fn factor(self) -> f32 {
        match self {
            FontSize::Small => 0.85,
            FontSize::Normal => 1.0,
            FontSize::Large => 1.15,
            FontSize::XLarge => 1.3,
        }
    }

    /// Etiqueta en español para el `pick_list` de Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            FontSize::Small => "Pequeña",
            FontSize::Normal => "Normal",
            FontSize::Large => "Grande",
            FontSize::XLarge => "Muy grande",
        }
    }
}

impl std::fmt::Display for FontSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Tamaño del círculo de la burbuja flotante, en px lógicos.
/// La ventana acepta el rango completo 48–72 (min/max) para que el resize en
/// caliente por `window::resize` no se tope con el clamp de WM_GETMINMAXINFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleSize {
    Small,
    Mid,
    Large,
}

impl BubbleSize {
    /// Diámetro del círculo en px lógicos.
    pub fn px(self) -> f32 {
        match self {
            BubbleSize::Small => 48.0,
            BubbleSize::Mid => 56.0,
            BubbleSize::Large => 72.0,
        }
    }

    /// Etiqueta en español para el `pick_list` de Ajustes (con su tamaño).
    pub fn label(self) -> String {
        format!("{} px", self.px() as u16)
    }
}

impl std::fmt::Display for BubbleSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// Esquina del monitor donde se ancla la burbuja (posición inicial y destino
/// de «mover a esquina» al cambiar la selección en Ajustes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleCorner {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl BubbleCorner {
    /// Etiqueta en español para el `pick_list` de Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            BubbleCorner::TopRight => "Arriba derecha",
            BubbleCorner::TopLeft => "Arriba izquierda",
            BubbleCorner::BottomRight => "Abajo derecha",
            BubbleCorner::BottomLeft => "Abajo izquierda",
        }
    }
}

impl std::fmt::Display for BubbleCorner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Auto-ocultar la burbuja tras un rato sin dictar. Se re-muestra al pulsar
/// el micrófono (o al abrir la burbuja).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BubbleHideTimeout {
    #[serde(rename = "nunca")]
    Never,
    #[serde(rename = "5")]
    S5,
    #[serde(rename = "10")]
    S10,
    #[serde(rename = "30")]
    S30,
    #[serde(rename = "60")]
    S60,
}

impl BubbleHideTimeout {
    /// Segundos tras el último turno antes de ocultar la burbuja; `None`
    /// para «Nunca».
    pub fn secs(self) -> Option<u64> {
        match self {
            BubbleHideTimeout::Never => None,
            BubbleHideTimeout::S5 => Some(5),
            BubbleHideTimeout::S10 => Some(10),
            BubbleHideTimeout::S30 => Some(30),
            BubbleHideTimeout::S60 => Some(60),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BubbleHideTimeout::Never => "Nunca",
            BubbleHideTimeout::S5 => "5 s",
            BubbleHideTimeout::S10 => "10 s",
            BubbleHideTimeout::S30 => "30 s",
            BubbleHideTimeout::S60 => "60 s",
        }
    }
}

impl std::fmt::Display for BubbleHideTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// PartialEq para los tests de degradación elegante (corrupta/vacía →
// defaults comparables por valor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
    /// `#[serde(default)]` en TODOS los campos (incluida la lista): un
    /// config.json sin la clave `dictionary` (versiones antiguas, edición a
    /// mano, escritura parcial) no debe tirar a la basera el resto de
    /// ajustes del usuario — sin esto, el parse entero fallaba y TODO caía
    /// a defaults (contradecía la promesa del README de ignorar solo lo
    /// ausente). Requisito de robustez R2.
    #[serde(default)]
    pub dictionary: Vec<DictionaryEntry>,
    pub model: Option<String>,
    /// Limpieza de muletillas (toggle «Limpiar muletillas» en Ajustes).
    /// `None` = activa (default: se limpian las muletillas comunes).
    pub cleanup_enabled: Option<bool>,
    pub auto_inject: Option<bool>,
    pub input_device: Option<String>,
    /// Tema oscuro por defecto (2026: dark mode default); toggle persistido.
    pub dark_mode: Option<bool>,
    /// Corrección IA: post-procesado con LLM local de los turnos con
    /// intención correctiva («no no, era 2 en vez de 4»). Toggle en Ajustes;
    /// spec en docs/CORRECCION-VOZ.md. Default: activada.
    pub ai_correction_enabled: Option<bool>,
    /// Proveedor del corrector: local por defecto, cloud OpenAI-compatible o
    /// endpoint personalizado. `None` conserva el método local histórico.
    pub correction_provider: Option<CorrectionProvider>,
    /// Endpoint OpenAI-compatible del corrector. Se usa en modo Custom y,
    /// si está presente, también permite sustituir el endpoint cloud.
    /// `http://127.0.0.1:8081/v1` (llama.cpp/Ornith en WSL, alcanzable desde
    /// Windows por localhost forwarding). Groq/Gemini con su base_url.
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
    /// API key en memoria. No se serializa: en Windows se guarda en
    /// Credential Manager y se migra cualquier valor legado al cargar/guardar.
    #[serde(default, skip_serializing)]
    pub llm_api_key: Option<String>,
    /// Timeout de la llamada HTTP al LLM corrector, en segundos. Default
    /// 25 s (en el límite de los ~4-6 s esperados + margen para endpoints
    /// remotos Groq/Gemini; spec §3.4). Valores ≤ 0 caen al default.
    pub llm_timeout_secs: Option<u64>,
    /// Ruta del ejecutable del server LLM local (`llama-server.exe`) que la
    /// app lanza al arrancar cuando el endpoint local no responde. `None` =
    /// autodetección por rutas comunes (D:\LlamaCPP, junto al exe de la app,
    /// %LOCALAPPDATA%\LlamaCPP, PATH). Configurable en Ajustes avanzados
    /// (ampliación de alcance 10/09/2026: en otro PC la ruta será distinta).
    pub llama_server_path: Option<String>,
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
    /// Campo legado de configuración. Whisper siempre recibe 16 kHz; se
    /// conserva para no romper configs antiguas, pero ya no se expone en UI.
    pub auto_resample_to_16k: Option<bool>,
    // ── Apariencia (tema / fuente / burbuja) ────────────────────────────────
    /// Tema claro/oscuro/auto. `None` = legacy: se deriva de `dark_mode`
    /// (el toggle antiguo) para no cambiar el comportamiento de configs
    /// viejas. Al elegir modo en la UI se escribe `theme_mode` y `dark_mode`
    /// queda como fallback para configs futuras sin el campo.
    pub theme_mode: Option<ThemeMode>,
    /// Tamaño de fuente global (Ajustes → Apariencia). `None` = Normal.
    pub font_size: Option<FontSize>,
    /// Tamaño del círculo de la burbuja flotante. `None` = 56 px (Mid).
    pub bubble_size: Option<BubbleSize>,
    /// Esquina del monitor donde se ancla la burbuja. `None` = arriba
    /// derecha (posición histórica).
    pub bubble_corner: Option<BubbleCorner>,
    /// Opacidad de la burbuja en % (rango 20..=100). `None` = 100.
    pub bubble_opacity: Option<u8>,
    /// Estilo visual de la burbuja. `None` = sólida (compatibilidad histórica).
    pub bubble_style: Option<BubbleStyle>,
    /// Auto-ocultar la burbuja tras N segundos sin dictar. `None` = nunca.
    /// Se re-muestra al pulsar el micrófono o al abrir la burbuja.
    pub bubble_hide_timeout: Option<BubbleHideTimeout>,
    /// Posición física (px de pantalla) de la burbuja arrastrada por el
    /// usuario. Pareja: si solo uno está presente se ignoran ambos (mover
    /// sin arrastre no persiste). Al elegir una esquina en Ajustes se
    /// limpian: la esquina vuelve a mandar en el siguiente arranque.
    pub bubble_x: Option<i32>,
    pub bubble_y: Option<i32>,
    /// Muletillas personalizadas (Ajustes → Muletillas). `None` = las
    /// listas por defecto de `cleanup.rs` (eh/um/uh/o sea/you know ·
    /// este/esto/tipo/like/pues · bueno/pues). Se persiste solo cuando el
    /// usuario toca las listas; el botón «Por defecto» lo vuelve a None.
    /// Los configs legacy que ya traían `fillers` vuelven a usarlos.
    pub fillers: Option<FillerWords>,
    /// Estado del panel «Ajustes avanzados» de la ventana de Ajustes
    /// (colapsado/expandido). Decisión de diseño 09/09/2026 (re-agrupado
    /// 10/09/2026): lo técnico (beam, hilos, timeout, umbral, API key,
    /// confirmar, fuente, inyección…) queda oculto tras un panel que por
    /// defecto está colapsado; lo común (modelo, corrección IA, ruta del
    /// server LLM) vive en primer nivel. `None` = colapsado. Solo
    /// afecta a la UI: no cambia ninguna opción de comportamiento.
    pub advanced_settings_open: Option<bool>,
}

pub fn config_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("config.json")
}

/// Ruta del diccionario personal exportable/importable (JSON).
/// Mismo directorio base que `config_path`; el fichero vive junto a la
/// config para que un backup de `%APPDATA%/whispr-open/` lo incluya todo.
pub fn dictionary_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("dictionary.json")
}

#[cfg(target_os = "windows")]
const API_KEY_CREDENTIAL_TARGET: &str = "WhisprOpen/llm-api-key";

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
fn load_api_key_secret() -> Option<String> {
    use winapi::um::wincred::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC};

    unsafe {
        let target = wide(API_KEY_CREDENTIAL_TARGET);
        let mut raw: *mut CREDENTIALW = std::ptr::null_mut();
        if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) == 0 || raw.is_null() {
            return None;
        }
        let credential = &*raw;
        let bytes = std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        );
        let value = String::from_utf8(bytes.to_vec()).ok();
        CredFree(raw as *mut _);
        value.filter(|key| !key.is_empty())
    }
}

#[cfg(not(target_os = "windows"))]
fn load_api_key_secret() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn save_api_key_secret(value: Option<&str>) {
    use winapi::shared::minwindef::DWORD;
    use winapi::um::wincred::{
        CredDeleteW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    unsafe {
        let target = wide(API_KEY_CREDENTIAL_TARGET);
        match value.filter(|key| !key.is_empty()) {
            Some(key) => {
                let bytes = key.as_bytes();
                let mut credential: CREDENTIALW = std::mem::zeroed();
                credential.Type = CRED_TYPE_GENERIC;
                credential.TargetName = target.as_ptr() as *mut _;
                credential.CredentialBlobSize = bytes.len() as DWORD;
                credential.CredentialBlob = bytes.as_ptr() as *mut _;
                credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
                if CredWriteW(&mut credential, 0) == 0 {
                    crate::log!(
                        "config",
                        "no se pudo guardar la API key en Credential Manager"
                    );
                }
            }
            None => {
                let _ = CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn save_api_key_secret(_value: Option<&str>) {}

impl Config {
    /// Parsea el contenido de config.json con degradación elegante: un JSON
    /// corrupto, vacío o de una versión desconocida NO rompe el arranque —
    /// cae a los defaults (los campos válidos que se reconozcan se mantienen
    /// por el `#[serde(default)]` de cada campo). Extraída como función pura
    /// para testear el comportamiento sin depender de %APPDATA%.
    pub fn parse_or_default(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Carga la config de `%APPDATA%\whispr-open\config.json` con degradación
    /// elegante: fichero ausente, vacío o corrupto → defaults (nunca rompe
    /// el arranque). La API key se lee del Credential Manager de Windows.
    pub fn load() -> Self {
        let path = config_path();
        let mut cfg: Self = match std::fs::read_to_string(&path) {
            // Corrupto/vacío/versión anterior → defaults; nunca un pánico
            // ni un arranque roto (requisito de robustez).
            Ok(raw) => Self::parse_or_default(&raw),
            Err(_) => Self::default(),
        };
        // La key nueva vive en Credential Manager; el campo legado del JSON
        // solo se conserva para migrar configuraciones antiguas al guardar.
        if cfg.llm_api_key.is_none() {
            cfg.llm_api_key = load_api_key_secret();
        }
        cfg
    }

    /// Persiste la config a `config.json` (creando el directorio si falta)
    /// y la API key al Credential Manager (o la borra si el usuario la
    /// vació). Los fallos de escritura se ignoran a propósito: perder la
    /// config no debe tumbar la sesión de dictado.
    pub fn save(&self) {
        save_api_key_secret(self.llm_api_key.as_deref());
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, raw);
        }
    }

    /// Modelo whisper efectivo: el de config si es válido, o Small (base
    /// de mejor calidad en español; la GPU lo corre en tiempo real).
    pub fn selected_model(&self) -> Model {
        match self.model.as_deref() {
            Some("tiny") => Model::Tiny,
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
    pub fn correction_provider(&self) -> CorrectionProvider {
        self.correction_provider
            .unwrap_or(CorrectionProvider::Default)
    }

    /// Configuración efectiva del corrector IA (`llm::LlmConfig`): defaults
    /// por proveedor, overridables por los campos individuales de config
    /// (endpoint, modelo, timeout).
    pub fn llm_settings(&self) -> crate::llm::LlmConfig {
        let provider = self.correction_provider();
        let (default_url, default_model, needs_key) = match provider {
            CorrectionProvider::Default => ("http://127.0.0.1:8081/v1", "ornith-1.0-9b", false),
            CorrectionProvider::Cloud => ("https://api.openai.com/v1", "gpt-4o-mini", true),
            CorrectionProvider::Custom => ("http://127.0.0.1:8081/v1", "ornith-1.0-9b", true),
        };
        crate::llm::LlmConfig {
            provider,
            base_url: self
                .llm_base_url
                .clone()
                .filter(|url| !url.trim().is_empty())
                .unwrap_or_else(|| default_url.to_string()),
            model: self
                .llm_model
                .clone()
                .filter(|model| !model.trim().is_empty())
                .unwrap_or_else(|| default_model.to_string()),
            api_key: if needs_key {
                self.llm_api_key.clone()
            } else {
                None
            },
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

    /// Ruta explícita del server LLM (`llama-server.exe`) elegida en
    /// Ajustes. Cadena en blanco/ausente = autodetección (el hueco vacío
    /// de un campo borrado no se persiste como ruta inválida).
    ///
    /// La RESOLUCIÓN final (config → rutas comunes → PATH) vive en
    /// `llm::resolve_server_path`, que además valida que el fichero exista:
    /// aquí solo devolvemos lo que pidió el usuario.
    pub fn llama_server_path(&self) -> Option<&str> {
        self.llama_server_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Sanea y PERSISTE la ruta del server LLM (t_75732630). La app solo
    /// arranca el `llama-server.exe` NATIVO de Windows, así que una ruta
    /// que apunte a un wrapper (`llama.exe`, el valor que traía config.json)
    /// o a un fichero inexistente deja de ser válida: se corrige a la mejor
    /// ruta utilizable —el `llama-server.exe` hermano o la autodetección—
    /// y se escribe en config.json. Sin esta corrección el proceso lanzado
    /// no casaba con la validación por nombre de imagen de
    /// `terminate_server_pid` y quedaba huérfano en VRAM al salir.
    ///
    /// Devuelve la ruta válida resultante (`None` si no hay server
    /// instalado: la corrección IA se desactiva con aviso, el dictado sigue).
    pub fn heal_llama_server_path(&mut self) -> Option<PathBuf> {
        let resolved = crate::llm::resolve_server_path(self)?;
        let healthy = self
            .llama_server_path()
            .map(|actual| {
                let p = std::path::Path::new(actual);
                p.is_file() && crate::llm::is_server_exe(p)
            })
            .unwrap_or(false);
        if !healthy {
            crate::log!(
                "config",
                "ruta del server LLM saneada: «{}» → {}",
                self.llama_server_path().unwrap_or("(autodetección)"),
                resolved.display()
            );
            self.llama_server_path = Some(resolved.to_string_lossy().into_owned());
            self.save();
        }
        Some(resolved)
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

    /// Compatibilidad con configs antiguas. El pipeline siempre remuestrea
    /// a 16 kHz antes de invocar Whisper.
    pub fn auto_resample_16k(&self) -> bool {
        true
    }

    /// ¿Limpieza de muletillas activa? Default: sí (se limpian las
    /// muletillas comunes; el toggle de Ajustes puede desactivarla).
    pub fn cleanup_enabled(&self) -> bool {
        self.cleanup_enabled.unwrap_or(true)
    }

    /// Muletillas efectivas: las personalizadas de Ajustes o, si no hay
    /// (`None`), los defaults de `cleanup.rs` (las listas originales).
    pub fn fillers(&self) -> FillerWords {
        self.fillers.clone().unwrap_or_default()
    }

    /// Modo de tema efectivo. `None` (config vieja sin el campo) se deriva
    /// del `dark_mode` legacy: true → Oscuro, false → Claro, ausente →
    /// Oscuro (default 2026). El modo `Auto` delega en el sistema.
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme_mode.unwrap_or_else(|| {
            if self.dark_mode.unwrap_or(true) {
                ThemeMode::Oscuro
            } else {
                ThemeMode::Claro
            }
        })
    }

    /// ¿Modo oscuro efectivo? `system_dark` es el estado del SISTEMA (leído
    /// por `theme::system_dark_mode()` y cacheado en el Poll del App cada
    /// 5 s). Solo se consulta cuando el modo es Auto.
    pub fn resolved_dark(&self, system_dark: bool) -> bool {
        match self.theme_mode() {
            ThemeMode::Auto => system_dark,
            ThemeMode::Claro => false,
            ThemeMode::Oscuro => true,
        }
    }

    /// Tamaño de fuente global. Default: Normal (factor 1.0 = baseline).
    pub fn font_size(&self) -> FontSize {
        self.font_size.unwrap_or(FontSize::Normal)
    }

    /// Panel «Ajustes avanzados» de Ajustes expandido. Default: colapsado
    /// (las opciones técnicas quedan ocultas hasta que el usuario lo abre).
    pub fn advanced_open(&self) -> bool {
        self.advanced_settings_open.unwrap_or(false)
    }

    /// Tamaño del círculo de la burbuja. Default: 56 px (Mid).
    pub fn bubble_size(&self) -> BubbleSize {
        self.bubble_size.unwrap_or(BubbleSize::Mid)
    }

    /// Esquina de anclaje de la burbuja. Default: arriba derecha.
    pub fn bubble_corner(&self) -> BubbleCorner {
        self.bubble_corner.unwrap_or(BubbleCorner::TopRight)
    }

    /// Opacidad de la burbuja en % (20..=100). Default: 100 (opaca).
    pub fn bubble_opacity(&self) -> u8 {
        self.bubble_opacity.unwrap_or(100).clamp(20, 100)
    }

    /// Estilo de la burbuja. Default: sólida.
    pub fn bubble_style(&self) -> BubbleStyle {
        self.bubble_style.unwrap_or(BubbleStyle::Solid)
    }

    /// Auto-ocultar de la burbuja. Default: nunca.
    pub fn bubble_hide_timeout(&self) -> BubbleHideTimeout {
        self.bubble_hide_timeout.unwrap_or(BubbleHideTimeout::Never)
    }

    /// Posición persistida de la burbuja (px físicos de pantalla). Solo Some
    /// si la pareja x/y está completa: un solo campo no significa nada.
    pub fn bubble_position(&self) -> Option<(i32, i32)> {
        match (self.bubble_x, self.bubble_y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
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
        assert!(cfg.cleanup_enabled(), "default: limpiar muletillas comunes");
        assert_eq!(cfg.fillers(), FillerWords::default());
    }

    #[test]
    fn config_path_lives_under_appdata() {
        assert!(config_path().to_string_lossy().contains("whispr-open"));
    }

    #[test]
    fn invalid_model_falls_back_to_small() {
        let cfg = Config {
            model: Some("gigante".into()),
            ..Default::default()
        };
        assert_eq!(cfg.selected_model(), Model::Small);
    }

    #[test]
    fn dark_mode_por_defecto() {
        let cfg = Config::default();
        assert!(cfg.is_dark(), "2026: dark mode default");
        let claro = Config {
            dark_mode: Some(false),
            ..Default::default()
        };
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
        let c = Config {
            inject_format: Some("escaneo".into()),
            ..Default::default()
        };
        assert_eq!(
            c.inject_settings().format,
            crate::inject::InjectFormat::Paste
        );
        let c = Config {
            inject_window_filter: Some("  ".into()),
            ..Default::default()
        };
        assert_eq!(c.inject_settings().window_filter, None);
    }

    #[test]
    fn atajo_global_default_y_personalizado() {
        let cfg = Config::default();
        assert_eq!(cfg.shortcut(), "Ctrl+Shift+D");
        let c = Config {
            shortcut: Some("Alt+Space".into()),
            ..Default::default()
        };
        assert_eq!(c.shortcut(), "Alt+Space");
        // En blanco/ausente → default (la config nunca rompe el arranque).
        let c = Config {
            shortcut: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(c.shortcut(), "Ctrl+Shift+D");
    }

    #[test]
    fn correction_ia_defaults_on_y_local() {
        let cfg = Config::default();
        assert!(
            cfg.ai_correction_enabled(),
            "corrección IA activada por defecto"
        );
        let llm = cfg.llm_settings();
        assert_eq!(llm.base_url, "http://127.0.0.1:8081/v1");
        assert_eq!(llm.model, "ornith-1.0-9b");
        assert_eq!(llm.api_key, None);
        assert_eq!(cfg.correction_provider(), CorrectionProvider::Default);
        // Override de endpoint (p. ej. cualquier API OpenAI-compatible) via config.json.
        let remoto = Config {
            correction_provider: Some(CorrectionProvider::Custom),
            llm_base_url: Some("https://api.example.com/v1".into()),
            llm_model: Some("custom-model".into()),
            llm_api_key: Some("clave".into()),
            ..Default::default()
        };
        let l2 = remoto.llm_settings();
        assert_eq!(l2.base_url, "https://api.example.com/v1");
        assert_eq!(l2.model, "custom-model");
        assert_eq!(
            l2.api_key.as_deref(),
            Some("clave"),
            "custom también puede requerir autenticación"
        );
        let cloud = Config {
            correction_provider: Some(CorrectionProvider::Cloud),
            llm_api_key: Some("clave".into()),
            ..Default::default()
        };
        let cloud_settings = cloud.llm_settings();
        assert_eq!(cloud_settings.base_url, "https://api.openai.com/v1");
        assert_eq!(cloud_settings.model, "gpt-4o-mini");
        assert_eq!(cloud_settings.api_key.as_deref(), Some("clave"));
        // Toggle off persistido.
        let off = Config {
            ai_correction_enabled: Some(false),
            ..Default::default()
        };
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
        let c = Config {
            llm_timeout_secs: Some(45),
            ..Default::default()
        };
        assert_eq!(c.llm_timeout_secs(), Some(45));
        assert_eq!(c.llm_settings().timeout_secs, 45);
        // 0 cae al default (timeout de 0 cortaría toda llamada).
        let c = Config {
            llm_timeout_secs: Some(0),
            ..Default::default()
        };
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
        let c = Config {
            language: None,
            ..Default::default()
        };
        assert_eq!(c.language(), Some("es"));
        let c = Config {
            language: Some("".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), Some("es"));
        let c = Config {
            language: Some("  ".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), Some("es"));

        // "auto" (cualquier casing) → autodetección (None).
        let c = Config {
            language: Some("auto".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), None);
        let c = Config {
            language: Some("AUTO".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), None);
        let c = Config {
            language: Some("Auto".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), None);
        let c = Config {
            language: Some("  auto  ".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), None);

        // Overrides legítimos (trimeo + BCP-47).
        let c = Config {
            language: Some("en".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), Some("en"));
        let c = Config {
            language: Some("  fr  ".into()),
            ..Default::default()
        };
        assert_eq!(c.language(), Some("fr"));
    }

    #[test]
    fn beam_size_default_y_rango_valido() {
        // Default 5 (lo que ya usaba la app).
        assert_eq!(Config::default().beam_size(), 5);

        // Valores válidos: 1..=8.
        for n in 1..=8 {
            let c = Config {
                beam_size: Some(n),
                ..Default::default()
            };
            assert_eq!(c.beam_size(), n, "beam_size válido: {n}");
        }
        // Fuera de rango → default (la config nunca rompe el arranque).
        let c = Config {
            beam_size: Some(0),
            ..Default::default()
        };
        assert_eq!(c.beam_size(), 5);
        let c = Config {
            beam_size: Some(9),
            ..Default::default()
        };
        assert_eq!(c.beam_size(), 5);
        let c = Config {
            beam_size: Some(999),
            ..Default::default()
        };
        assert_eq!(c.beam_size(), 5);
        let c = Config {
            beam_size: None,
            ..Default::default()
        };
        assert_eq!(c.beam_size(), 5);
    }

    #[test]
    fn n_threads_default_y_rango() {
        // Default 4 (lo que ya usaba la app).
        assert_eq!(Config::default().n_threads(), 4);

        // Mínimo 1.
        let c = Config {
            n_threads: Some(1),
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 1);
        // Valores razonables.
        let c = Config {
            n_threads: Some(8),
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 8);
        let c = Config {
            n_threads: Some(16),
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 16);
        // Tope blando 32 (más hilos no aportan pero se permite).
        let c = Config {
            n_threads: Some(64),
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 32);
        // 0 y None → default.
        let c = Config {
            n_threads: Some(0),
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 4);
        let c = Config {
            n_threads: None,
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 4);
    }

    #[test]
    fn legacy_resampling_setting_is_ignored() {
        // Whisper siempre exige 16 kHz: el campo legacy ya no puede desactivar
        // el remuestreo aunque un config.json antiguo lo contenga.
        assert!(Config::default().auto_resample_16k());
        let disabled = Config {
            auto_resample_to_16k: Some(false),
            ..Default::default()
        };
        assert!(disabled.auto_resample_16k());
        let enabled = Config {
            auto_resample_to_16k: Some(true),
            ..Default::default()
        };
        assert!(enabled.auto_resample_16k());
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
        assert!(leido.auto_resample_16k());

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

    #[test]
    fn theme_mode_legacy_y_explicito() {
        // Default (sin nada): Oscuro (2026: dark por defecto).
        assert_eq!(Config::default().theme_mode(), ThemeMode::Oscuro);
        // Legacy dark_mode=false → Claro (config vieja con toggle en claro).
        let claro = Config {
            dark_mode: Some(false),
            ..Default::default()
        };
        assert_eq!(claro.theme_mode(), ThemeMode::Claro);
        // theme_mode explícito manda sobre dark_mode.
        let c = Config {
            dark_mode: Some(true),
            theme_mode: Some(ThemeMode::Claro),
            ..Default::default()
        };
        assert_eq!(c.theme_mode(), ThemeMode::Claro);
        // FromStr tolera casing y alias inglés (usa "dark"/"light").
        assert_eq!("auto".parse::<ThemeMode>(), Ok(ThemeMode::Auto));
        assert_eq!("OSCURO".parse::<ThemeMode>(), Ok(ThemeMode::Oscuro));
        assert_eq!("light".parse::<ThemeMode>(), Ok(ThemeMode::Claro));
        assert!("neon".parse::<ThemeMode>().is_err());
    }

    #[test]
    fn resolved_dark_sigue_al_modo_y_al_sistema() {
        // Auto: decide el sistema (cacheado en el Poll).
        let auto = Config {
            theme_mode: Some(ThemeMode::Auto),
            ..Default::default()
        };
        assert!(auto.resolved_dark(true));
        assert!(!auto.resolved_dark(false));
        // Claro/Oscuro: fuerzan sin mirar al sistema.
        let claro = Config {
            theme_mode: Some(ThemeMode::Claro),
            ..Default::default()
        };
        assert!(
            !claro.resolved_dark(true),
            "claro gana aunque el sistema sea oscuro"
        );
        let oscuro = Config {
            theme_mode: Some(ThemeMode::Oscuro),
            ..Default::default()
        };
        assert!(
            oscuro.resolved_dark(false),
            "oscuro gana aunque el sistema sea claro"
        );
        // Default (None) = Oscuro, ignora al sistema.
        assert!(Config::default().resolved_dark(false));
    }

    #[test]
    fn ciclo_de_tema_auto_claro_oscuro() {
        let m = ThemeMode::Auto;
        assert_eq!(m.next(), ThemeMode::Claro);
        assert_eq!(m.next().next(), ThemeMode::Oscuro);
        assert_eq!(m.next().next().next(), ThemeMode::Auto, "ciclo cerrado");
        assert_eq!(ThemeMode::Oscuro.next(), ThemeMode::Auto);
        // forced_dark: solo Auto delega.
        assert_eq!(ThemeMode::Auto.forced_dark(), None);
        assert_eq!(ThemeMode::Claro.forced_dark(), Some(false));
        assert_eq!(ThemeMode::Oscuro.forced_dark(), Some(true));
    }

    #[test]
    fn font_size_default_y_factores() {
        assert_eq!(Config::default().font_size(), FontSize::Normal);
        assert_eq!(Config::default().font_size().factor(), 1.0);
        assert_eq!(FontSize::Small.factor(), 0.85);
        assert_eq!(FontSize::Large.factor(), 1.15);
        assert_eq!(FontSize::XLarge.factor(), 1.3);
        let c = Config {
            font_size: Some(FontSize::XLarge),
            ..Default::default()
        };
        assert_eq!(c.font_size(), FontSize::XLarge);
    }

    #[test]
    fn ajustes_avanzados_colapsados_por_defecto_y_persistidos() {
        // Default: colapsado (el usuario normal no ve opciones técnicas).
        assert!(!Config::default().advanced_open());
        // Config legacy (sin el campo): también colapsado al deserializar.
        // (Los campos viejos como `dictionary` son requeridos; lo que se
        // prueba es que el campo NUEVO es opcional.)
        let legacy: Config = serde_json::from_str(r#"{"dictionary":[]}"#).unwrap();
        assert!(!legacy.advanced_open());
        // Expandido → serializado → vuelve a leerse expandido.
        let cfg = Config {
            advanced_settings_open: Some(true),
            ..Default::default()
        };
        let raw = serde_json::to_string(&cfg).unwrap();
        let vuelta: Config = serde_json::from_str(&raw).unwrap();
        assert!(vuelta.advanced_open());
    }

    #[test]
    fn burbuja_accessors_defaults_y_clamps() {
        let cfg = Config::default();
        assert_eq!(cfg.bubble_size(), BubbleSize::Mid);
        assert_eq!(cfg.bubble_size().px(), 56.0);
        assert_eq!(cfg.bubble_corner(), BubbleCorner::TopRight);
        assert_eq!(cfg.bubble_opacity(), 100);
        assert_eq!(cfg.bubble_hide_timeout(), BubbleHideTimeout::Never);
        assert_eq!(cfg.bubble_position(), None, "sin arrastre no hay posición");
        // Opacidad fuera de rango (config.json a mano) se clampea a 20..=100.
        let baja = Config {
            bubble_opacity: Some(5),
            ..Default::default()
        };
        assert_eq!(baja.bubble_opacity(), 20);
        let alta = Config {
            bubble_opacity: Some(255),
            ..Default::default()
        };
        assert_eq!(alta.bubble_opacity(), 100);
        // Posición solo con la pareja completa.
        let solo_x = Config {
            bubble_x: Some(10),
            ..Default::default()
        };
        assert_eq!(solo_x.bubble_position(), None);
        let par = Config {
            bubble_x: Some(10),
            bubble_y: Some(20),
            ..Default::default()
        };
        assert_eq!(par.bubble_position(), Some((10, 20)));
        // Timeouts: Nunca = None, el resto segundos exactos.
        assert_eq!(BubbleHideTimeout::Never.secs(), None);
        assert_eq!(BubbleHideTimeout::S5.secs(), Some(5));
        assert_eq!(BubbleHideTimeout::S60.secs(), Some(60));
    }

    #[test]
    fn serializa_y_relee_campos_nuevos_de_ui() {
        // Roundtrip por JSON (lo que pasa con config.json en disco).
        let original = Config {
            theme_mode: Some(ThemeMode::Auto),
            font_size: Some(FontSize::Large),
            bubble_size: Some(BubbleSize::Large),
            bubble_corner: Some(BubbleCorner::BottomLeft),
            bubble_opacity: Some(80),
            bubble_hide_timeout: Some(BubbleHideTimeout::S10),
            bubble_x: Some(1400),
            bubble_y: Some(920),
            ..Default::default()
        };
        let raw = serde_json::to_string(&original).expect("serializa");
        let leida: Config = serde_json::from_str(&raw).expect("deserializa");
        assert_eq!(leida.theme_mode(), ThemeMode::Auto);
        assert_eq!(leida.font_size(), FontSize::Large);
        assert_eq!(leida.bubble_size(), BubbleSize::Large);
        assert_eq!(leida.bubble_corner(), BubbleCorner::BottomLeft);
        assert_eq!(leida.bubble_opacity(), 80);
        assert_eq!(leida.bubble_style(), BubbleStyle::Solid);
        assert_eq!(leida.bubble_hide_timeout(), BubbleHideTimeout::S10);
        assert_eq!(leida.bubble_position(), Some((1400, 920)));

        // Compatibilidad hacia atrás: un config.json viejo sin los campos
        // nuevos se deserializa sin error y los getters devuelven los defaults.
        let viejo = r#"{"dictionary":[]}"#;
        let cfg: Config = serde_json::from_str(viejo).expect("json viejo");
        assert_eq!(cfg.theme_mode(), ThemeMode::Oscuro, "legacy → dark");
        assert_eq!(cfg.font_size(), FontSize::Normal);
        assert_eq!(cfg.bubble_size(), BubbleSize::Mid);
        assert_eq!(cfg.bubble_corner(), BubbleCorner::TopRight);
        assert_eq!(cfg.bubble_opacity(), 100);
        assert_eq!(cfg.bubble_hide_timeout(), BubbleHideTimeout::Never);
    }

    #[test]
    fn correction_provider_defaults_and_roundtrip() {
        let cfg = Config::default();
        assert_eq!(cfg.correction_provider(), CorrectionProvider::Default);
        let original = Config {
            correction_provider: Some(CorrectionProvider::Cloud),
            llm_base_url: Some("https://api.example.com/v1".into()),
            llm_model: Some("model".into()),
            ..Default::default()
        };
        let raw = serde_json::to_string(&original).unwrap();
        let loaded: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(loaded.correction_provider(), CorrectionProvider::Cloud);
    }

    #[test]
    fn llama_server_path_roundtrip_y_legacy() {
        // Default: autodetección (None) — sin campo no hay ruta fija.
        assert_eq!(Config::default().llama_server_path(), None);
        // Ruta explícita → se persiste y se relee.
        let cfg = Config {
            llama_server_path: Some("D:\\LlamaCPP\\llama-server.exe".into()),
            ..Default::default()
        };
        assert_eq!(
            cfg.llama_server_path(),
            Some("D:\\LlamaCPP\\llama-server.exe")
        );
        let raw = serde_json::to_string(&cfg).unwrap();
        let leida: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            leida.llama_server_path(),
            Some("D:\\LlamaCPP\\llama-server.exe")
        );
        // Config legacy (sin el campo) se deserializa → None (autodetección).
        let legacy: Config = serde_json::from_str(r#"{ "dictionary": [] }"#).unwrap();
        assert_eq!(legacy.llama_server_path(), None);
        // Campo en blanco → None (autodetección; no persiste basura).
        assert_eq!(
            Config {
                llama_server_path: Some("   ".into()),
                ..Default::default()
            }
            .llama_server_path(),
            None
        );
    }

    #[test]
    fn api_key_never_serializes_to_config_json() {
        let cfg = Config {
            llm_api_key: Some("secret-value".into()),
            correction_provider: Some(CorrectionProvider::Cloud),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).expect("config serializable");
        assert!(!json.contains("secret-value"));
        assert!(!json.contains("llm_api_key"));

        // El formato legado se sigue leyendo para poder migrarlo, pero nunca
        // vuelve a salir al serializar.
        let legacy: Config =
            serde_json::from_str(r#"{"dictionary":[],"llm_api_key":"legacy-secret"}"#)
                .expect("legacy config");
        assert_eq!(legacy.llm_api_key.as_deref(), Some("legacy-secret"));
        let migrated = serde_json::to_string(&legacy).expect("migrated config");
        assert!(!migrated.contains("legacy-secret"));
    }

    #[test]
    fn tiny_model_can_be_selected_from_config() {
        let cfg = Config {
            model: Some("tiny".into()),
            ..Default::default()
        };
        assert_eq!(cfg.selected_model(), Model::Tiny);
    }

    #[test]
    fn legacy_filler_settings_are_honored_when_loading() {
        // Config legacy de Juanfri (muletillas desactivadas + listas vacías):
        // el refactor auditado las ignoraba; ahora deben cargar SIN ROMPER y
        // volver a mandar (cleanup desactivado, fillers vacíos, no defaults).
        let raw = r#"{"dictionary":[],"cleanup_enabled":false,"fillers":{"unambiguous":[],"ambiguous":[],"start_of_turn":[]}}"#;
        let cfg: Config = serde_json::from_str(raw).expect("legacy config");
        assert!(
            !cfg.cleanup_enabled(),
            "legacy cleanup_enabled=false se honra"
        );
        assert!(cfg.fillers().unambiguous.is_empty());
        assert!(cfg.fillers().ambiguous.is_empty());
        assert!(cfg.fillers().start_of_turn.is_empty());
    }

    #[test]
    fn filler_defaults_limpian_muletillas_comunes() {
        // Config sin el campo → default sensato: limpiar las muletillas
        // comunes por defecto, con opción de desactivar en Ajustes.
        let cfg: Config = serde_json::from_str(r#"{"dictionary":[]}"#).expect("json");
        assert!(cfg.cleanup_enabled());
        assert_eq!(
            cfg.fillers().unambiguous,
            ["eh", "um", "uh", "o sea", "you know"]
        );
        assert_eq!(
            cfg.fillers().ambiguous,
            ["este", "esto", "tipo", "like", "pues"]
        );
        // Un `fillers` parcial cae por campo al default (semántica de ee614dc).
        let parcial: Config =
            serde_json::from_str(r#"{"dictionary":[],"fillers":{"unambiguous":["vale"]}}"#)
                .expect("json");
        assert_eq!(parcial.fillers().unambiguous, ["vale"]);
        assert_eq!(
            parcial.fillers().ambiguous,
            ["este", "esto", "tipo", "like", "pues"]
        );
    }

    #[test]
    fn fillers_roundtrip_a_traves_del_json() {
        // Lo que se ve en Ajustes se persiste y se relee igual.
        let original = Config {
            cleanup_enabled: Some(false),
            fillers: Some(FillerWords {
                unambiguous: vec!["vale".into()],
                ambiguous: vec![],
                start_of_turn: vec!["pues".into()],
            }),
            ..Default::default()
        };
        let raw = serde_json::to_string(&original).expect("serializa");
        let leida: Config = serde_json::from_str(&raw).expect("deserializa");
        assert!(!leida.cleanup_enabled());
        assert_eq!(leida.fillers().unambiguous, ["vale"]);
        assert!(leida.fillers().ambiguous.is_empty());
        assert_eq!(leida.fillers().start_of_turn, ["pues"]);
    }

    #[test]
    fn dictionary_path_apunta_a_appdata() {
        let p = dictionary_path();
        let s = p.to_string_lossy();
        assert!(s.contains("whispr-open"), "{s}");
        assert!(s.ends_with("dictionary.json"), "{s}");
    }

    #[test]
    fn config_corrupta_vacia_y_antigua_nunca_rompen_el_arranque() {
        // Requisito de robustez: cualquier config.json dañado cae a defaults
        // sin pánico ni arranque roto. Casos: vacío, corrupto a mitad de
        // escritura (corte de luz), basura total, y un JSON válido con
        // claves de una versión futura desconocida.
        assert_eq!(Config::parse_or_default(""), Config::default());
        assert_eq!(
            Config::parse_or_default("{ \"model\": \"sm"),
            Config::default(),
            "JSON cortado a medias → defaults"
        );
        assert_eq!(
            Config::parse_or_default("esto no es json en absoluto"),
            Config::default()
        );
        assert_eq!(
            Config::parse_or_default("\u{0}\u{0}\u{0}"),
            Config::default(),
            "bytes nulos (fichero truncado por corte de luz) → defaults"
        );
        // Claves desconocidas se ignoran y las conocidas se respetan.
        let futura = Config::parse_or_default(
            r#"{"model":"tiny","claves_del_futuro":{"x":1}}"#,
        );
        assert_eq!(futura.selected_model(), Model::Tiny);
        // Una config SIN la clave `dictionary` (versión antigua, edición a
        // mano) conserva el resto de ajustes: el parse no falla entero.
        let sin_dictionary = Config::parse_or_default(r#"{"model":"base"}"#);
        assert_eq!(
            sin_dictionary.selected_model(),
            Model::Base,
            "un campo ausente no tira el resto de la config"
        );
        assert!(sin_dictionary.dictionary.is_empty());
    }
}
