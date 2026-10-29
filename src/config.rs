//! Configuración que se guarda de la app (sustituye al localStorage de la web).
//! Un JSON en %APPDATA%/whispr-open/config.json: diccionario, modelo elegido y
//! preferencias. Si algo falla se queda lo que se pueda: nunca rompe el arranque.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::cleanup::FillerWords;
use crate::domain::dictionary::DictionaryEntry;
#[cfg(unix)]
use crate::inject::LinuxInjectBackend;
use crate::theme::PaletteConfig;
use crate::transcribe::Model;

/// De dónde sale la corrección IA: Local (llama.cpp descargado cuando hace
/// falta, del sprint final) o Remoto (API de OpenAI o un servidor propio).
/// `Default` es el nombre viejo de Local; `Cloud`/`Custom` son las dos formas
/// de trabajar en remoto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorrectionProvider {
    Default,
    Cloud,
    Custom,
}

impl CorrectionProvider {
    /// Texto del selector de Ajustes (sprint final: solo Local/Remoto).
    pub fn label(self) -> &'static str {
        match self {
            CorrectionProvider::Default => "Local (en este equipo)",
            CorrectionProvider::Cloud => "Remoto (OpenAI)",
            CorrectionProvider::Custom => "Remoto (endpoint propio)",
        }
    }

    /// ¿La corrección es local? Solo `Default`.
    pub fn is_local(self) -> bool {
        matches!(self, CorrectionProvider::Default)
    }
}

impl std::fmt::Display for CorrectionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Modo de tema. `Auto` sigue el tema del SISTEMA (`AppsUseLightTheme` del
/// registro, mirado cada 5 s en el `Poll`); `Claro`/`Oscuro` lo fijan a mano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Auto,
    Claro,
    Oscuro,
}

impl ThemeMode {
    /// Siguiente modo al pulsar el botón de la cabecera: Auto → Claro → Oscuro.
    pub fn next(self) -> ThemeMode {
        match self {
            ThemeMode::Auto => ThemeMode::Claro,
            ThemeMode::Claro => ThemeMode::Oscuro,
            ThemeMode::Oscuro => ThemeMode::Auto,
        }
    }

    /// ¿Este modo fija un lado? `Auto` no fija nada (None): decide el sistema
    /// operativo.
    pub fn forced_dark(self) -> Option<bool> {
        match self {
            ThemeMode::Auto => None,
            ThemeMode::Claro => Some(false),
            ThemeMode::Oscuro => Some(true),
        }
    }

    /// Texto en español para el desplegable de Ajustes.
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

/// Aspecto de la burbuja flotante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleStyle {
    Solid,
    Ring,
    Minimal,
}

impl BubbleStyle {
    /// Texto en español para el desplegable de Ajustes.
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

/// Tamaño de letra de toda la interfaz (Ajustes → Apariencia). Se multiplica
/// sobre los tamaños base de `theme.rs`: 0.85 / 1.0 / 1.15 / 1.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontSize {
    Small,
    Normal,
    Large,
    XLarge,
}

impl FontSize {
    /// Cuánto multiplica sobre los tamaños base de `theme.rs`.
    pub fn factor(self) -> f32 {
        match self {
            FontSize::Small => 0.85,
            FontSize::Normal => 1.0,
            FontSize::Large => 1.15,
            FontSize::XLarge => 1.3,
        }
    }

    /// Texto en español para el desplegable de Ajustes.
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

/// Tamaño del círculo de la burbuja, en px lógicos.
/// La ventana acepta todo el rango 48–72 para que el cambio de tamaño en
/// caliente con `window::resize` no choque con el límite de WM_GETMINMAXINFO.
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

    /// Texto en español para el desplegable de Ajustes (con su tamaño).
    pub fn label(self) -> String {
        format!("{} px", self.px() as u16)
    }
}

impl std::fmt::Display for BubbleSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// Esquina de la pantalla donde se pone la burbuja (posición al arrancar y
/// destino de «mover a esquina» al cambiar la opción en Ajustes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleCorner {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl BubbleCorner {
    /// Texto en español para el desplegable de Ajustes.
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

/// Esconder la burbuja sola tras un rato sin dictar. Vuelve a salir al pulsar
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
    /// Segundos desde el último turno antes de esconder la burbuja; `None`
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

    /// Texto del selector «Ocultar la burbuja tras…» (incluye «Nunca»).
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

/// Qué hace la IA con cada turno (0.7.1). Tres modos, elegidos en Ajustes.
///
/// El modo no cambia el motor: cambia lo que se le pide al modelo y cuándo se
/// le llama. `Off` no llama a nadie; `Correccion` solo cuando el clasificador
/// ve una auto-corrección hablada; `Reescritura` en CADA turno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMode {
    /// Nadie toca el texto: el dictado se escribe tal cual.
    Off,
    /// La IA arregla las auto-correcciones («digo X», «no X sino Y»).
    Correccion,
    /// La IA reescribe el turno entero, limpio y puntuado.
    Reescritura,
}

impl AiMode {
    /// Los tres modos, en el orden en que salen en los selectores.
    pub const ALL: [AiMode; 3] = [AiMode::Off, AiMode::Correccion, AiMode::Reescritura];

    /// Texto corto del selector.
    pub fn label(self) -> &'static str {
        match self {
            AiMode::Off => "Desactivada",
            AiMode::Correccion => "Corrección",
            AiMode::Reescritura => "Reescritura",
        }
    }

    /// Explicación de una línea para la nota de debajo del selector.
    pub fn description(self) -> &'static str {
        match self {
            AiMode::Off => "El dictado se escribe tal cual.",
            AiMode::Correccion => {
                "La IA arregla las auto-correcciones («digo X»). Solo actúa cuando las detecta."
            }
            AiMode::Reescritura => {
                "La IA reescribe el turno entero, limpio y puntuado. Tarda algo más en cada dictado."
            }
        }
    }

    /// Nombre del modo para el log (`modo=reescritura`): el mismo que se guarda
    /// en el config.json, sin acentos.
    pub fn id(self) -> &'static str {
        match self {
            AiMode::Off => "off",
            AiMode::Correccion => "correccion",
            AiMode::Reescritura => "reescritura",
        }
    }
}

impl std::fmt::Display for AiMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Ajustes guardados del usuario (`%APPDATA%/whispr-open/config.json`).
///
/// Todos los campos son opcionales y caen a su valor por defecto: un
/// config.json incompleto, viejo o roto no rompe la app (ver `load`).
///
/// Se deriva `PartialEq` para los tests de cuando la config viene rota o vacía
/// (así se pueden comparar los valores por defecto).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
    /// `#[serde(default)]` en TODOS los campos (incluida la lista): un
    /// config.json sin la clave `dictionary` (versiones viejas, editado a mano,
    /// escrito a medias) no debe tirar el resto de ajustes del usuario — sin
    /// esto, al fallar el parse entero se perdía TODO y volvían los valores por
    /// defecto (contradecía lo que promete el README: ignorar solo lo que
    /// falta). Requisito de robustez R2.
    #[serde(default)]
    pub dictionary: Vec<DictionaryEntry>,
    pub model: Option<String>,
    /// Limpieza de muletillas (interruptor «Limpiar muletillas» en Ajustes).
    /// `None` = activa (por defecto se quitan las muletillas comunes).
    pub cleanup_enabled: Option<bool>,
    pub auto_inject: Option<bool>,
    pub input_device: Option<String>,
    /// Tema oscuro por defecto (2026: dark mode de serie); el interruptor se guarda.
    pub dark_mode: Option<bool>,
    /// Corrección IA: pasar por el modelo local los turnos que suenan a
    /// corrección («no no, era 2 en vez de 4»). Interruptor en Ajustes; está
    /// explicado en docs/CORRECCION-VOZ.md. Por defecto: activada.
    ///
    /// Desde la 0.7.1 el ajuste de verdad es `ai_mode`; este campo se sigue
    /// escribiendo para que una versión anterior del programa entienda el mismo
    /// config.json (`false` = apagada, `true` = corrección).
    pub ai_correction_enabled: Option<bool>,
    /// Qué hace la IA con cada turno (0.7.1): apagada, corrección de las
    /// auto-correcciones o reescritura del turno entero.
    ///
    /// Si no está en el JSON (config de la 0.7 o anterior) el modo se deriva de
    /// `ai_correction_enabled` — ver `ai_mode()`. No se serializa cuando vale
    /// `None`, para no ensuciar el config.json con un `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_mode: Option<AiMode>,
    /// De dónde saca el corrector: local por defecto, la nube de OpenAI o un
    /// servidor propio. `None` mantiene el método local de siempre.
    pub correction_provider: Option<CorrectionProvider>,
    /// Dirección del corrector compatible con OpenAI. Se usa en modo Custom y,
    /// si está puesta, también deja cambiar la de la nube.
    /// `http://127.0.0.1:8081/v1` (llama.cpp/Ornith en WSL, que se alcanza desde
    /// Windows por localhost forwarding). Groq/Gemini con su base_url.
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
    /// Clave de la API en memoria. No se guarda en el JSON: en Windows va al
    /// Credential Manager y cualquier valor viejo del JSON se pasa allí al
    /// cargar o guardar.
    #[serde(default, skip_serializing)]
    pub llm_api_key: Option<String>,
    /// Cuánto se espera la respuesta del modelo corrector, en segundos. Por
    /// defecto 25 s (los turnos tardan unos 4-6 s, más margen para Groq/Gemini;
    /// punto 3.4 del documento). Valores ≤ 0 usan el de por defecto.
    pub llm_timeout_secs: Option<u64>,
    /// Ruta del servidor LLM puesta A MANO (de cuando se elegía en Ajustes con
    /// «Examinar…»). Desde el 14/10/2026 el servidor VIAJA con la app
    /// (`vendor/llama/`, ver `llm::resolve_server_path`, que lo mira PRIMERO):
    /// este campo solo existe para no romper una instalación vieja sin
    /// `vendor/`, donde el servidor está en otro sitio. La interfaz ya no lo
    /// escribe; `heal_llama_server_path` limpia el valor viejo al arrancar.
    pub llama_server_path: Option<String>,
    /// Umbral de sensibilidad del clasificador de correcciones (punto 2:
    /// «mejor no equivocarse que acertar siempre»). Rango 0..=100; por defecto
    /// 50 (= como antes: todo lo que pasa la regla de los dos niveles va al
    /// modelo). Más alto = más prudente (menos falsos positivos al modelo y más
    /// dictado normal). El clasificador lleva el número de señales a {50, 75,
    /// 100}, así que el rango real va de 50 (permisivo) a 100 (imposible, nunca
    /// se manda). 0 el clasificador lo trata como apagado (atajo igual que
    /// quitar la corrección, sin tocar el resto de la config).
    pub llm_classifier_threshold: Option<u8>,
    /// Si está activo, las correcciones IA NO se aplican a ciegas: al volver del
    /// modelo, la app enseña el cambio en el panel de avisos con botones
    /// «Aplicar» / «Descartar». Por defecto no (aplicar, avisar y poder
    /// deshacer; punto 4.2: «por fluidez»). El deshacer de un paso sigue
    /// estando también en modo confirmar.
    pub ai_correction_confirm: Option<bool>,
    /// Atajo global del dictado, con la forma `Mod1+Mod2+Tecla` (por ejemplo
    /// "Ctrl+Shift+D", "Alt+Space", "Ctrl+Alt+F12"). Se registra al arrancar; si
    /// el valor no vale o el atajo ya lo usa otra app, se vuelve al de por
    /// defecto y la interfaz sigue funcionando (botones). Por defecto: Ctrl+Shift+D.
    pub shortcut: Option<String>,
    /// Cómo se escribe el turno en la app activa: "paste" (por defecto) lo
    /// copia al portapapeles y pega con Ctrl+V; "type" teclea el texto tal cual
    /// (con enigo, sin tocar el portapapeles del usuario).
    pub inject_format: Option<String>,
    /// Milisegundos de espera antes de pegar o teclear (para que la app de
    /// destino coja el foco). Por defecto 40; `0` = sin espera.
    pub inject_delay_ms: Option<u64>,
    /// Filtro por el título de la ventana activa (basta con que lo contenga, sin
    /// mirar mayúsculas): si el título no lo lleva, el turno NO se escribe.
    /// Vacío o ausente = escribir en cualquier app.
    pub inject_window_filter: Option<String>,
    /// Solo Linux: cómo se escribe el turno en la app activa. Valores válidos
    /// (`inject_backend` en config.json):
    /// - `"wl-clipboard"` (por defecto): wl-copy al portapapeles y wtype para
    ///   la pulsación de pegar. Sin programas de fondo ni permisos de más.
    /// - `"ydotool"`: teclea por uinput (vale para todo, Xwayland incluido);
    ///   pide `ydotoold` en marcha — sin él se copia al portapapeles y se avisa
    ///   (la app no teclea a ciegas).
    /// - `"wtype"`: teclea con el teclado virtual de Wayland; algunas apps lo
    ///   bloquean (zwp_virtual_keyboard_v1) → portapapeles.
    /// Cualquier otro valor usa el de por defecto (la config nunca rompe).
    pub inject_backend: Option<String>,
    /// Idioma de la transcripción. Vacío o "auto" (sin mirar mayúsculas) =
    /// que whisper lo detecte (se le pasa `None`); cualquier otro valor se limpia
    /// y se toma como código de idioma (`"es"`, `"en"`, `"fr"`, …). Los valores
    /// raros NO se comprueban aquí: whisper los acepta y puede acabar en `"en"`
    /// sin avisar. Por defecto: `"es"` (lo que la app daba por hecho).
    pub language: Option<String>,
    /// Beam size de `SamplingStrategy::BeamSearch` de whisper. Rango útil
    /// 1..=8 (1 es como ir a lo rápido); por encima de 8 va lentísimo sin ganar
    /// apenas calidad. Por defecto 5 (lo que ya usaba la app). Fuera de rango
    /// se usa el de por defecto.
    pub beam_size: Option<u32>,
    /// Hilos del decodificador de whisper. Más hilos = más a la vez, pero el
    /// cuello suele estar en la GPU (small + cuda). Por defecto 4 (lo que ya
    /// usaba la app). Se respeta lo que diga el sistema.
    pub n_threads: Option<u32>,
    /// Campo viejo de configuración. Whisper siempre recibe 16 kHz; se guarda
    /// para no romper configs antiguas, pero ya no sale en la interfaz.
    pub auto_resample_to_16k: Option<bool>,
    // ── Apariencia (tema / fuente / burbuja) ────────────────────────────────
    /// Tema claro/oscuro/auto. `None` = config vieja: se saca de `dark_mode`
    /// (el interruptor de antes) para no cambiar el comportamiento. Al elegir
    /// modo en la interfaz se escribe `theme_mode` y `dark_mode` queda por si
    /// aparece una config sin el campo.
    pub theme_mode: Option<ThemeMode>,
    /// Tamaño de letra de toda la interfaz (Ajustes → Apariencia). `None` = Normal.
    pub font_size: Option<FontSize>,
    /// Tamaño del círculo de la burbuja. `None` = 56 px (Mid).
    pub bubble_size: Option<BubbleSize>,
    /// Esquina donde se pone la burbuja. `None` = arriba
    /// derecha (donde estaba siempre).
    pub bubble_corner: Option<BubbleCorner>,
    /// Opacidad de la burbuja en % (de 20 a 100). `None` = 100.
    pub bubble_opacity: Option<u8>,
    /// Aspecto de la burbuja. `None` = sólida (como antes).
    pub bubble_style: Option<BubbleStyle>,
    /// Esconder la burbuja tras N segundos sin dictar. `None` = nunca.
    /// Vuelve a salir al pulsar el micrófono o al abrir la burbuja.
    pub bubble_hide_timeout: Option<BubbleHideTimeout>,
    /// Dónde está la burbuja en pantalla (px) después de arrastrarla. Van en
    /// pareja: si solo está uno, se ignoran los dos (mover sin arrastrar no se
    /// guarda). Al elegir una esquina en Ajustes se borran: la esquina manda
    /// otra vez al arrancar.
    pub bubble_x: Option<i32>,
    pub bubble_y: Option<i32>,
    /// Muletillas propias (Ajustes → Muletillas). `None` = las listas de
    /// `cleanup.rs` (eh/um/uh/o sea/you know · este/esto/tipo/like/pues ·
    /// bueno/pues). Solo se guarda cuando el usuario toca las listas; el botón
    /// «Por defecto» lo devuelve a None. Los configs viejos que ya traían
    /// `fillers` vuelven a usarlas.
    pub fillers: Option<FillerWords>,
    /// Estado del panel «Ajustes avanzados» (plegado o abierto). Decisión de
    /// diseño del 09/09/2026 (reagrupado el 10/09/2026): lo técnico (beam, hilos,
    /// espera, umbral, clave de API, confirmar, fuente, escritura…) queda
    /// escondido tras un panel plegado por defecto; lo normal (modelo,
    /// corrección IA, ruta del servidor LLM) se ve sin abrir nada. `None` =
    /// plegado. Solo afecta a la interfaz: no cambia ninguna opción.
    pub advanced_settings_open: Option<bool>,
    /// Colores cambiados a mano en la paleta (R7 — Personalización II). Cada
    /// campo es un hex opcional (`"#RRGGBB"` o `"#RGB"`); si falta o no vale,
    /// se usa el color del modo activo (un hex basura no rompe nada). `None` =
    /// paleta de siempre = como antes de R7-II. Los campos `None` se dejan
    /// fuera del JSON para que el config quede limpio.
    pub palette: Option<PaletteConfig>,
    // ── Personalización III: imágenes del usuario ─────────────────────────
    /// Ruta (absoluta o relativa a donde se ejecuta) de la imagen de fondo de
    /// la ventana principal. `None` = fondo del tema. La ruta se guarda TAL
    /// CUAL; se comprueba al dibujar, en `custom_image::resolve_static`: si falla
    /// → se pinta el fondo del tema, sin romper la interfaz.
    pub bg_image_path: Option<String>,
    /// Ruta de la imagen fija de la burbuja. Si está, manda sobre la animación;
    /// si no, se mira `bubble_frames_path`. Se comprueba y se cae al aspecto de
    /// siempre en `custom_image::resolve_static` / `resolve_frames`.
    pub bubble_image_path: Option<String>,
    /// Carpeta con la secuencia de PNG (u otro formato aceptado) para animar
    /// la burbuja. Orden por nombre; cada `1000/fps` ms se pasa a la siguiente
    /// (y vuelve a empezar). Vacía o sin poner → se usa `bubble_image_path` (o
    /// el dibujo de siempre).
    pub bubble_frames_path: Option<String>,
    /// Velocidad de la animación de la burbuja en imágenes por segundo. Por
    /// defecto 12; rango 1..=30 (se comprueba al leerlo y al mover el control).
    /// Fuera de rango se usa el de por defecto.
    pub bubble_animation_fps: Option<u8>,
}

/// Ruta del `config.json` del usuario (junto a `dictionary.json`).
///
/// Windows: `%APPDATA%\whispr-open\config.json` (lo da `dirs::data_dir`).
/// Linux: `~/.config/whispr-open/config.json` (respeta `$XDG_CONFIG_HOME`).
///
/// Si el sistema no da carpeta de datos, se usa la carpeta actual: la app sigue
/// funcionando, solo que guarda la config junto al ejecutable.
#[cfg(target_os = "windows")]
pub fn config_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("config.json")
}

/// Linux: la config va a XDG (no a datos); doc-comments/docsUPDATED-2026-10-03.
#[cfg(not(target_os = "windows"))]
pub fn config_path() -> PathBuf {
    crate::paths::app_config_dir().join("config.json")
}

/// Ruta del diccionario personal que se puede exportar e importar (JSON).
/// La misma carpeta base que `config_path`; el fichero vive junto al config para
/// que una copia de la carpeta base lo tenga todo.
#[cfg(target_os = "windows")]
pub fn dictionary_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("dictionary.json")
}

/// Linux: junto al config (XDG_CONFIG_HOME), igual que en Windows
/// (los dos ficheros uno al lado del otro).
#[cfg(not(target_os = "windows"))]
pub fn dictionary_path() -> PathBuf {
    crate::paths::app_config_dir().join("dictionary.json")
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
    /// Lee el contenido de config.json aguantando lo que venga: un JSON roto,
    /// vacío o de una versión desconocida NO rompe el arranque — se usan los
    /// valores por defecto (lo que sí se reconoce se queda, por el
    /// `#[serde(default)]` de cada campo). Va aparte para poder probarlo sin
    /// depender de %APPDATA%.
    pub fn parse_or_default(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Carga la config de `%APPDATA%\whispr-open\config.json` aguantando lo que
    /// venga: fichero que falta, vacío o roto → valores por defecto (nunca rompe
    /// el arranque). La clave de la API se lee del Credential Manager de Windows.
    pub fn load() -> Self {
        let path = config_path();
        let mut cfg: Self = match std::fs::read_to_string(&path) {
            // Roto, vacío o de una versión anterior → valores por defecto;
            // nunca un pánico ni un arranque roto.
            Ok(raw) => Self::parse_or_default(&raw),
            Err(_) => Self::default(),
        };
        // La clave nueva vive en el Credential Manager; el campo viejo del JSON
        // solo se queda para pasar configs antiguas al guardar.
        if cfg.llm_api_key.is_none() {
            cfg.llm_api_key = load_api_key_secret();
        }
        cfg
    }

    /// Guarda la config en `config.json` (crea la carpeta si falta) y la clave
    /// de la API en el Credential Manager (o la borra si el usuario la dejó
    /// vacía). Si falla la escritura se ignora a propósito: perder la config no
    /// puede tumbar la sesión de dictado.
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

    /// Modelo de whisper que se usa: el del config si vale, o Small (la mejor
    /// calidad en español; la GPU lo mueve en tiempo real).
    pub fn selected_model(&self) -> Model {
        match self.model.as_deref() {
            Some("tiny") => Model::Tiny,
            Some("base") => Model::Base,
            Some("small") => Model::Small,
            // Sin config y con valores raros: Small. Base y tiny se equivocan
            // con sonidos del español (buscar→boscar); small es el mínimo
            // razonable y la RTX 4070 lo mueve en tiempo real.
            _ => Model::Small,
        }
    }

    /// Tema: oscuro por defecto (2026); el interruptor guarda `dark_mode`.
    pub fn is_dark(&self) -> bool {
        self.dark_mode.unwrap_or(true)
    }

    /// Corrección IA activada (interruptor de Ajustes; por defecto: sí).
    ///
    /// Desde la 0.7.1 es «hay algún modo de IA que no sea apagada»: de aquí
    /// cuelga el ciclo de vida del servidor (`app.rs`), y en Reescritura el
    /// servidor también tiene que estar encendido.
    pub fn ai_correction_enabled(&self) -> bool {
        self.ai_mode() != AiMode::Off
    }

    /// Qué hace la IA con cada turno. Si el JSON no trae `ai_mode` (config de
    /// la 0.7 o anterior) se deriva del interruptor `ai_correction_enabled`,
    /// que venía encendido de serie: encendido = Corrección, apagado = Off.
    pub fn ai_mode(&self) -> AiMode {
        self.ai_mode.unwrap_or(if self.ai_correction_enabled.unwrap_or(true) {
            AiMode::Correccion
        } else {
            AiMode::Off
        })
    }

    /// ¿El modo elegido reescribe el turno entero? (Se llama al modelo en CADA
    /// turno y con otro prompt.)
    pub fn ai_rewrites(&self) -> bool {
        self.ai_mode() == AiMode::Reescritura
    }

    /// Fija el modo y deja `ai_correction_enabled` en consonancia, para que una
    /// versión anterior del programa lea el mismo config.json y no pierda el
    /// ajuste.
    pub fn set_ai_mode(&mut self, mode: AiMode) {
        self.ai_mode = Some(mode);
        self.ai_correction_enabled = Some(mode != AiMode::Off);
    }

    /// Atajo global: el `shortcut` del config.json o Ctrl+Shift+D si falta o
    /// está en blanco. La lectura fina (y volver al de por defecto si el texto no
    /// vale) está en `hotkey.rs`.
    pub fn shortcut(&self) -> String {
        match self.shortcut.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => crate::hotkey::DEFAULT_SHORTCUT.to_string(),
        }
    }

    /// Cómo se escribe el turno, sacado del config.json: formato (paste/type),
    /// espera en ms (40 por defecto, 0 = ninguna), filtro por título de la
    /// ventana activa y —solo Linux— por dónde escribir (wl-clipboard/ydotool/
    /// wtype). Valores raros o que faltan → los de por defecto, que son seguros.
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
            // Solo Linux: leer el backend y quedarse con el de por defecto si
            // el valor no se reconoce (las reglas están en el comentario del
            // enum). En Windows el campo no existe.
            #[cfg(unix)]
            backend: match self.inject_backend.as_deref() {
                Some("ydotool") => LinuxInjectBackend::Ydotool,
                Some("wtype") => LinuxInjectBackend::Wtype,
                _ => LinuxInjectBackend::WlClipboard,
            },
        }
    }

    /// Configuración del modelo que corrige. Por defecto: llama.cpp local en
    /// WSL (Ornith 9B, puerto 8081); se puede cambiar en config.json. La espera
    /// sale de `llm_timeout_secs` (25 s por defecto).
    pub fn correction_provider(&self) -> CorrectionProvider {
        self.correction_provider
            .unwrap_or(CorrectionProvider::Default)
    }

    /// Configuración real del corrector (`llm::LlmConfig`): valores por defecto
    /// según el proveedor, cambiables con los campos del config (dirección,
    /// modelo, espera).
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

    /// Espera de la llamada al corrector, en segundos. Por defecto 25; un 0
    /// también usa el de por defecto (con 0 no se llegaría a llamar nunca).
    pub fn llm_timeout_secs(&self) -> Option<u64> {
        match self.llm_timeout_secs {
            Some(t) if t > 0 => Some(t),
            _ => None,
        }
    }

    /// Ruta puesta a mano del servidor LLM (`llama-server.exe`) desde Ajustes.
    /// En blanco o sin poner = que se busque solo (un campo vacío no se guarda
    /// como ruta mala).
    ///
    /// La búsqueda de verdad (dentro de la app → config → rutas habituales →
    /// PATH) está en `llm::resolve_server_path`, que además mira que el fichero
    /// exista: aquí solo se devuelve lo que hubiera guardado.
    pub fn llama_server_path(&self) -> Option<&str> {
        self.llama_server_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Limpia una config VIEJA del servidor LLM (14/10/2026, «cero rutas que
    /// teclear»). El servidor VIAJA con la app (`vendor/llama/`) y el campo
    /// `llama_server_path` ya no se edita en la interfaz: aquí no se intenta
    /// «arreglar» la ruta (eso era lo de antes: resolver y guardar), se BORRA
    /// cuando no vale — no existe, no es `llama-server.exe`, o hay un servidor
    /// dentro de la app que la deja sin efecto (una ruta de una instalación
    /// vieja no puede romper una nueva).
    ///
    /// Se queda cuando sigue sirviendo Y no hay servidor dentro de la app: una
    /// instalación vieja (sin `vendor/`) con el servidor en otro sitio sigue
    /// funcionando con su ruta a mano.
    ///
    /// Devuelve la ruta que se va a usar (`None` si no hay servidor en ningún
    /// sitio: la corrección IA se apaga con aviso, el dictado sigue).
    pub fn heal_llama_server_path(&mut self) -> Option<PathBuf> {
        let vendado = crate::llm::exe_dir()
            .as_deref()
            .and_then(crate::llm::vendored_server_path_in)
            .is_some();
        let utilizable = self
            .llama_server_path()
            .map(|actual| {
                let p = std::path::Path::new(actual);
                p.is_file() && crate::llm::is_server_exe(p)
            })
            .unwrap_or(false);
        if self.llama_server_path.is_some() && (!utilizable || vendado) {
            crate::log!(
                "config",
                "ruta del server LLM obsoleta, se limpia: «{}» (el server va en el paquete)",
                self.llama_server_path().unwrap_or("")
            );
            self.llama_server_path = None;
            self.save();
        }
        crate::llm::resolve_server_path(self)
    }

    /// Umbral del clasificador (0..=100). Por defecto 50 (= como antes: todo lo
    /// que pasa la regla de los dos niveles va al modelo). Más alto = más
    /// prudente (mejor no pasarse, punto 2). El clasificador lleva el número de
    /// señales a {50, 75, 100}, así que el rango real va de 50 (permisivo) a 100
    /// (imposible, nunca se manda). 0 lo trata como apagado.
    pub fn llm_classifier_threshold(&self) -> u8 {
        self.llm_classifier_threshold.unwrap_or(50)
    }

    /// Modo «confirmar antes de aplicar»: si está, el cambio del modelo se
    /// enseña con botones «Aplicar» / «Descartar» en vez de aplicarse solo
    /// (punto 4.2). Por defecto no.
    pub fn ai_correction_confirm(&self) -> bool {
        self.ai_correction_confirm.unwrap_or(false)
    }

    /// Idioma para `whisper_full_params::language`. Vacío o "auto" (sin mirar
    /// mayúsculas) = que lo detecte solo (al motor NO se le pone `set_language`
    /// y decide whisper). Cualquier otro valor se limpia y se devuelve como
    /// código de idioma (`"es"`, `"en"`, …). Los valores raros NO se comprueban
    /// aquí: whisper los acepta.
    ///
    /// Si el campo falta o está vacío, se usa `Some("es")` (lo que la app daba
    /// por hecho).
    pub fn language(&self) -> Option<&str> {
        match self.language.as_deref().map(str::trim) {
            Some(s) if s.is_empty() => Some("es"),
            Some(s) if s.eq_ignore_ascii_case("auto") => None,
            Some(s) => Some(s),
            None => Some("es"),
        }
    }

    /// Beam size que se usa. Por defecto 5 (lo que ya usaba la app). Se limita
    /// a 1..=8: 1 va como el modo rápido, y por encima de 8 es lentísimo en CPU
    /// sin ganar apenas calidad. Fuera de rango se usa el de por defecto.
    pub fn beam_size(&self) -> u32 {
        match self.beam_size {
            Some(n @ 1..=8) => n,
            _ => 5,
        }
    }

    /// Hilos del decodificador de whisper. Por defecto 4 (lo que ya usaba la
    /// app). Mínimo 1; tope blando 32 (más no aportan, pero se deja por si el
    /// usuario sabe lo que hace).
    pub fn n_threads(&self) -> u32 {
        match self.n_threads {
            Some(n) if n >= 1 => n.min(32),
            _ => 4,
        }
    }

    /// Para no romper configs antiguas. Antes de llamar a Whisper siempre se
    /// pasa el audio a 16 kHz.
    pub fn auto_resample_16k(&self) -> bool {
        true
    }

    /// ¿Limpieza de muletillas activa? Por defecto sí (se quitan las comunes;
    /// el interruptor de Ajustes puede apagarla).
    pub fn cleanup_enabled(&self) -> bool {
        self.cleanup_enabled.unwrap_or(true)
    }

    /// Muletillas que se usan: las de Ajustes o, si no hay (`None`), las de
    /// `cleanup.rs` (las listas de siempre).
    pub fn fillers(&self) -> FillerWords {
        self.fillers.clone().unwrap_or_default()
    }

    /// Modo de tema que se usa. `None` (config vieja sin el campo) se saca del
    /// `dark_mode` de antes: true → Oscuro, false → Claro, sin campo → Oscuro
    /// (por defecto en 2026). El modo `Auto` lo decide el sistema.
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme_mode.unwrap_or_else(|| {
            if self.dark_mode.unwrap_or(true) {
                ThemeMode::Oscuro
            } else {
                ThemeMode::Claro
            }
        })
    }

    /// ¿Está en oscuro? `system_dark` es el estado del SISTEMA (lo lee
    /// `theme::system_dark_mode()` y se guarda en el `Poll` cada 5 s). Solo se
    /// mira cuando el modo es Auto.
    pub fn resolved_dark(&self, system_dark: bool) -> bool {
        match self.theme_mode() {
            ThemeMode::Auto => system_dark,
            ThemeMode::Claro => false,
            ThemeMode::Oscuro => true,
        }
    }

    /// Tamaño de letra de toda la interfaz. Por defecto: Normal (1.0, la base).
    pub fn font_size(&self) -> FontSize {
        self.font_size.unwrap_or(FontSize::Normal)
    }

    /// Panel «Ajustes avanzados» abierto. Por defecto plegado (lo técnico queda
    /// escondido hasta que el usuario lo abre).
    pub fn advanced_open(&self) -> bool {
        self.advanced_settings_open.unwrap_or(false)
    }

    /// Tamaño del círculo de la burbuja. Por defecto: 56 px (Mid).
    pub fn bubble_size(&self) -> BubbleSize {
        self.bubble_size.unwrap_or(BubbleSize::Mid)
    }

    /// Esquina donde se ancla la burbuja. Por defecto: arriba derecha.
    pub fn bubble_corner(&self) -> BubbleCorner {
        self.bubble_corner.unwrap_or(BubbleCorner::TopRight)
    }

    /// Opacidad de la burbuja en % (20..=100). Por defecto: 100 (opaca).
    pub fn bubble_opacity(&self) -> u8 {
        self.bubble_opacity.unwrap_or(100).clamp(20, 100)
    }

    /// Aspecto de la burbuja. Por defecto: sólida.
    pub fn bubble_style(&self) -> BubbleStyle {
        self.bubble_style.unwrap_or(BubbleStyle::Solid)
    }

    /// Esconder la burbuja sola. Por defecto: nunca.
    pub fn bubble_hide_timeout(&self) -> BubbleHideTimeout {
        self.bubble_hide_timeout.unwrap_or(BubbleHideTimeout::Never)
    }

    /// Dónde quedó la burbuja (px de pantalla). Solo hay valor si están las dos
    /// coordenadas: una sola no dice nada.
    pub fn bubble_position(&self) -> Option<(i32, i32)> {
        match (self.bubble_x, self.bubble_y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    }

    /// Ruta de la imagen de fondo (en blanco o sin poner = sin imagen). Aquí NO
    /// se comprueba: eso se hace al dibujar (`custom_image::resolve_static`),
    /// para no gastar disco al arrancar y para que un fichero recién elegido en
    /// el diálogo esté disponible aunque el sistema todavía no lo tenga apuntado.
    pub fn bg_image_path(&self) -> Option<&str> {
        self.bg_image_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Ruta de la imagen fija de la burbuja. Igual que `bg_image_path`: se
    /// comprueba al dibujar.
    pub fn bubble_image_path(&self) -> Option<&str> {
        self.bubble_image_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Carpeta de imágenes para animar la burbuja. Se comprueba al dibujar: si
    /// la carpeta no existe o está vacía → no hay animación (se usa la imagen
    /// fija o el dibujo de siempre).
    pub fn bubble_frames_path(&self) -> Option<&str> {
        self.bubble_frames_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Imágenes por segundo de la burbuja. Por defecto 12; fuera del rango
    /// 1..=30 también se usa el de por defecto. El ajuste fino (`clamp_fps`)
    /// está en `custom_image::clamp_fps`, para que el módulo y la interfaz usen
    /// la misma función.
    pub fn bubble_animation_fps(&self) -> u8 {
        crate::custom_image::clamp_fps(self.bubble_animation_fps.unwrap_or(
            crate::custom_image::DEFAULT_BUBBLE_FPS,
        ))
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

        // Formato raro → paste (seguro); filtro en blanco → sin filtro.
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

    #[cfg(unix)]
    #[test]
    fn inject_backend_linux_defaults_y_overrides() {
        // Por defecto: wl-clipboard (sin programas de fondo ni permisos de más).
        assert_eq!(
            Config::default().inject_settings().backend,
            crate::inject::LinuxInjectBackend::WlClipboard
        );
        // Cambios que sí valen.
        for (raw, esperado) in [
            ("ydotool", crate::inject::LinuxInjectBackend::Ydotool),
            ("wtype", crate::inject::LinuxInjectBackend::Wtype),
            ("wl-clipboard", crate::inject::LinuxInjectBackend::WlClipboard),
        ] {
            let c = Config {
                inject_backend: Some(raw.into()),
                ..Default::default()
            };
            assert_eq!(c.inject_settings().backend, esperado, "backend {raw}");
        }
        // Valor raro → el de por defecto (la config nunca rompe el arranque).
        let raro = Config {
            inject_backend: Some("xdotool-mola".into()),
            ..Default::default()
        };
        assert_eq!(
            raro.inject_settings().backend,
            crate::inject::LinuxInjectBackend::WlClipboard
        );
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
        // En blanco o sin poner → el de por defecto (la config nunca rompe el arranque).
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
        // Cambiar la dirección (por ejemplo cualquier API compatible con OpenAI) desde config.json.
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
        // Corrección IA apagada y guardada.
        let off = Config {
            ai_correction_enabled: Some(false),
            ..Default::default()
        };
        assert!(!off.ai_correction_enabled());
    }

    /// (0.7.1) Una config de la 0.7 (sin `ai_mode`) no pierde el ajuste: si
    /// tenía la corrección encendida, el modo es «Corrección»; si la tenía
    /// apagada, «Desactivada». Y el modo nuevo manda sobre el interruptor viejo.
    #[test]
    fn ai_mode_se_deriva_del_interruptor_de_la_config_vieja() {
        let vieja_on: Config =
            serde_json::from_str(r#"{"dictionary":[],"ai_correction_enabled":true}"#)
                .expect("json de la 0.7");
        assert_eq!(vieja_on.ai_mode(), AiMode::Correccion);
        assert!(vieja_on.ai_correction_enabled());

        let vieja_off: Config =
            serde_json::from_str(r#"{"dictionary":[],"ai_correction_enabled":false}"#)
                .expect("json de la 0.7");
        assert_eq!(vieja_off.ai_mode(), AiMode::Off);
        assert!(!vieja_off.ai_correction_enabled());

        // Sin nada puesto: corregir (lo que hacía la app hasta ahora).
        assert_eq!(Config::default().ai_mode(), AiMode::Correccion);

        // El campo nuevo, tal cual lo escribe la app, manda.
        let nueva: Config = serde_json::from_str(
            r#"{"dictionary":[],"ai_correction_enabled":true,"ai_mode":"reescritura"}"#,
        )
        .expect("json de la 0.7.1");
        assert_eq!(nueva.ai_mode(), AiMode::Reescritura);
        assert!(nueva.ai_rewrites());
        // Reescritura también es «IA encendida»: el servidor tiene que vivir.
        assert!(nueva.ai_correction_enabled());
    }

    /// (0.7.1) Elegir un modo deja el interruptor viejo en consonancia y el
    /// `ai_mode` se guarda con el nombre acordado (`off`/`correccion`/`reescritura`).
    #[test]
    fn set_ai_mode_guarda_los_dos_campos() {
        for (modo, guardado) in [
            (AiMode::Off, "off"),
            (AiMode::Correccion, "correccion"),
            (AiMode::Reescritura, "reescritura"),
        ] {
            let mut cfg = Config::default();
            cfg.set_ai_mode(modo);
            assert_eq!(cfg.ai_mode(), modo);
            assert_eq!(cfg.ai_correction_enabled, Some(modo != AiMode::Off));
            let raw = serde_json::to_string(&cfg).expect("serializa");
            assert!(
                raw.contains(&format!(r#""ai_mode":"{guardado}""#)),
                "el JSON debe llevar ai_mode {guardado}: {raw}"
            );
            // Y vuelve a entrar igual (el JSON es redondo).
            let vuelta: Config = serde_json::from_str(&raw).expect("deserializa");
            assert_eq!(vuelta.ai_mode(), modo);
        }
        // Sin tocar el modo no se ensucia el JSON con un null.
        let raw = serde_json::to_string(&Config::default()).expect("serializa");
        assert!(!raw.contains("ai_mode"), "no debe aparecer ai_mode: {raw}");
    }

    #[test]
    fn timeout_umbral_y_confirm_tienen_defaults_seguros() {
        let cfg = Config::default();
        // 25 s por defecto, aplicado al LlmConfig.
        assert_eq!(cfg.llm_timeout_secs(), None, "None = cae al default de 25");
        assert_eq!(cfg.llm_settings().timeout_secs, 25);
        // 50 por defecto del clasificador (= como antes: pasa todo lo que
        // cumple los dos niveles).
        assert_eq!(cfg.llm_classifier_threshold(), 50);
        // Falso por defecto: aplicar, avisar y poder deshacer (punto 4.2: fluidez).
        assert!(!cfg.ai_correction_confirm());

        // Cambiar la espera desde config.json.
        let c = Config {
            llm_timeout_secs: Some(45),
            ..Default::default()
        };
        assert_eq!(c.llm_timeout_secs(), Some(45));
        assert_eq!(c.llm_settings().timeout_secs, 45);
        // 0 usa el de por defecto (con 0 no se llegaría a llamar nunca).
        let c = Config {
            llm_timeout_secs: Some(0),
            ..Default::default()
        };
        assert_eq!(c.llm_timeout_secs(), None);
        assert_eq!(c.llm_settings().timeout_secs, 25);

        // Cambiar el umbral y el modo confirmar.
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
        // Por defecto, español (lo que la app daba por hecho).
        let cfg = Config::default();
        assert_eq!(cfg.language(), Some("es"));

        // Vacío o None también acaban en español (la config nunca rompe el
        // arranque; para detectar el idioma hay que poner "auto" a mano).
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

        // "auto" (con cualquier mayúscula) → que lo detecte solo (None).
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

        // Cambios que sí valen (se limpian y se pasan como código de idioma).
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
        // 5 por defecto (lo que ya usaba la app).
        assert_eq!(Config::default().beam_size(), 5);

        // Valores que valen: 1..=8.
        for n in 1..=8 {
            let c = Config {
                beam_size: Some(n),
                ..Default::default()
            };
            assert_eq!(c.beam_size(), n, "beam_size válido: {n}");
        }
        // Fuera de rango → el de por defecto (la config nunca rompe el arranque).
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
        // 4 por defecto (lo que ya usaba la app).
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
        // Tope blando 32 (más no aportan, pero se permite).
        let c = Config {
            n_threads: Some(64),
            ..Default::default()
        };
        assert_eq!(c.n_threads(), 32);
        // 0 y None → el de por defecto.
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
        // Whisper siempre pide 16 kHz: el campo viejo ya no puede desactivar
        // el cambio de frecuencia aunque un config.json antiguo lo traiga.
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
        // Guardar y volver a leer por JSON (lo que pasa con el fichero en disco).
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

        // Compatibilidad: un config.json viejo sin los campos nuevos se lee sin
        // error y los getters dan los valores por defecto. (Los campos viejos
        // como `dictionary` sí son obligatorios; aquí se prueba que los NUEVOS
        // son opcionales.)
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
        // Por defecto (sin nada): Oscuro (2026: dark de serie).
        assert_eq!(Config::default().theme_mode(), ThemeMode::Oscuro);
        // `dark_mode=false` de una config vieja → Claro.
        let claro = Config {
            dark_mode: Some(false),
            ..Default::default()
        };
        assert_eq!(claro.theme_mode(), ThemeMode::Claro);
        // `theme_mode` puesto gana sobre `dark_mode`.
        let c = Config {
            dark_mode: Some(true),
            theme_mode: Some(ThemeMode::Claro),
            ..Default::default()
        };
        assert_eq!(c.theme_mode(), ThemeMode::Claro);
        // `FromStr` aguanta mayúsculas y el alias en inglés ("dark"/"light").
        assert_eq!("auto".parse::<ThemeMode>(), Ok(ThemeMode::Auto));
        assert_eq!("OSCURO".parse::<ThemeMode>(), Ok(ThemeMode::Oscuro));
        assert_eq!("light".parse::<ThemeMode>(), Ok(ThemeMode::Claro));
        assert!("neon".parse::<ThemeMode>().is_err());
    }

    #[test]
    fn resolved_dark_sigue_al_modo_y_al_sistema() {
        // Auto: lo decide el sistema (se guarda en el `Poll`).
        let auto = Config {
            theme_mode: Some(ThemeMode::Auto),
            ..Default::default()
        };
        assert!(auto.resolved_dark(true));
        assert!(!auto.resolved_dark(false));
        // Claro/Oscuro: lo fijan sin mirar al sistema.
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
        // Por defecto (None) = Oscuro, sin mirar al sistema.
        assert!(Config::default().resolved_dark(false));
    }

    #[test]
    fn ciclo_de_tema_auto_claro_oscuro() {
        let m = ThemeMode::Auto;
        assert_eq!(m.next(), ThemeMode::Claro);
        assert_eq!(m.next().next(), ThemeMode::Oscuro);
        assert_eq!(m.next().next().next(), ThemeMode::Auto, "ciclo cerrado");
        assert_eq!(ThemeMode::Oscuro.next(), ThemeMode::Auto);
        // `forced_dark`: solo Auto mira al sistema.
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
        // Por defecto: plegado (el usuario normal no ve opciones técnicas).
        assert!(!Config::default().advanced_open());
        // Config vieja (sin el campo): también plegado al leerla. (Los campos
        // viejos como `dictionary` son obligatorios; aquí se prueba que el campo
        // NUEVO es opcional.)
        let legacy: Config = serde_json::from_str(r#"{"dictionary":[]}"#).unwrap();
        assert!(!legacy.advanced_open());
        // Abierto → se guarda → se vuelve a leer abierto.
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
        // Opacidad fuera de rango (config.json tocado a mano) se lleva a 20..=100.
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
        // La posición solo se guarda con las dos coordenadas.
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
        // Esperas: Nunca = None, el resto los segundos exactos.
        assert_eq!(BubbleHideTimeout::Never.secs(), None);
        assert_eq!(BubbleHideTimeout::S5.secs(), Some(5));
        assert_eq!(BubbleHideTimeout::S60.secs(), Some(60));
    }

    #[test]
    fn serializa_y_relee_campos_nuevos_de_ui() {
        // Guardar y volver a leer por JSON (lo que pasa con el fichero en disco).
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

        // Compatibilidad: un config.json viejo sin los campos nuevos se lee sin
        // error y los getters dan los valores por defecto.
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
        // Por defecto: que lo busque solo (None) — sin campo no hay ruta fija.
        assert_eq!(Config::default().llama_server_path(), None);
        // Ruta puesta a mano → se guarda y se vuelve a leer.
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
        // Config vieja (sin el campo) se lee → None (que lo busque solo).
        let legacy: Config = serde_json::from_str(r#"{ "dictionary": [] }"#).unwrap();
        assert_eq!(legacy.llama_server_path(), None);
        // Campo en blanco → None (no se guarda basura).
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

        // El formato viejo se sigue leyendo para poder pasarlo, pero nunca
        // vuelve a salir al guardar.
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
    fn image_paths_roundtrip_y_legacy() {
        // Por defecto: ninguna ruta (todo a None).
        assert_eq!(Config::default().bg_image_path(), None);
        assert_eq!(Config::default().bubble_image_path(), None);
        assert_eq!(Config::default().bubble_frames_path(), None);
        assert_eq!(
            Config::default().bubble_animation_fps(),
            crate::custom_image::DEFAULT_BUBBLE_FPS
        );

        // Cuatro rutas puestas → se guardan y se releen.
        let cfg = Config {
            bg_image_path: Some("C:\\fondo.png".into()),
            bubble_image_path: Some("C:\\bubble.png".into()),
            bubble_frames_path: Some("C:\\frames".into()),
            bubble_animation_fps: Some(24),
            ..Default::default()
        };
        assert_eq!(cfg.bg_image_path(), Some("C:\\fondo.png"));
        assert_eq!(cfg.bubble_image_path(), Some("C:\\bubble.png"));
        assert_eq!(cfg.bubble_frames_path(), Some("C:\\frames"));
        assert_eq!(cfg.bubble_animation_fps(), 24);
        let raw = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(reloaded.bg_image_path(), Some("C:\\fondo.png"));
        assert_eq!(reloaded.bubble_image_path(), Some("C:\\bubble.png"));
        assert_eq!(reloaded.bubble_frames_path(), Some("C:\\frames"));
        assert_eq!(reloaded.bubble_animation_fps(), 24);

        // Cadenas en blanco o con espacios → None (no se guardan rutas que no
        // sirven y que harían saltar el aspecto de siempre).
        let blank = Config {
            bg_image_path: Some("   ".into()),
            bubble_image_path: Some(String::new()),
            bubble_frames_path: Some("\t".into()),
            ..Default::default()
        };
        assert_eq!(blank.bg_image_path(), None);
        assert_eq!(blank.bubble_image_path(), None);
        assert_eq!(blank.bubble_frames_path(), None);

        // Config vieja (sin los campos nuevos) se lee y los getters dan los
        // valores por defecto (compatibilidad).
        let legacy: Config =
            serde_json::from_str(r#"{ "dictionary": [] }"#).unwrap();
        assert_eq!(legacy.bg_image_path(), None);
        assert_eq!(legacy.bubble_image_path(), None);
        assert_eq!(legacy.bubble_frames_path(), None);
        assert_eq!(
            legacy.bubble_animation_fps(),
            crate::custom_image::DEFAULT_BUBBLE_FPS
        );

        // Imágenes por segundo fuera de rango → las de por defecto (se ajusta en un solo sitio).
        let high = Config {
            bubble_animation_fps: Some(99),
            ..Default::default()
        };
        assert_eq!(
            high.bubble_animation_fps(),
            crate::custom_image::DEFAULT_BUBBLE_FPS
        );
        let zero = Config {
            bubble_animation_fps: Some(0),
            ..Default::default()
        };
        assert_eq!(
            zero.bubble_animation_fps(),
            crate::custom_image::DEFAULT_BUBBLE_FPS
        );
    }

    #[test]
    fn legacy_filler_settings_are_honored_when_loading() {
        // Config vieja de Juanfri (muletillas apagadas y listas vacías): el
        // reparto que se revisó las ignoraba; ahora tienen que cargar SIN ROMPER
        // y volver a guardarse (limpieza apagada, listas vacías, no las de serie).
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
        // Config sin el campo → lo sensato: limpiar las muletillas comunes por
        // defecto, con opción de apagarlo en Ajustes.
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
        // Un `fillers` a medias se completa campo a campo (como en ee614dc).
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
        // Lo que se ve en Ajustes se guarda y se vuelve a leer igual.
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
        // Cualquier config.json roto cae a los valores por defecto sin pánico
        // ni arranque roto. Casos: vacío, roto a mitad de escritura (corte de
        // luz), basura total, y un JSON válido con claves de una versión futura
        // que no conocemos.
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
        // Las claves que no se conocen se ignoran y las que sí, se respetan.
        let futura = Config::parse_or_default(
            r#"{"model":"tiny","claves_del_futuro":{"x":1}}"#,
        );
        assert_eq!(futura.selected_model(), Model::Tiny);
        // Una config SIN la clave `dictionary` (versión vieja, editada a mano)
        // conserva el resto de ajustes: el parse no falla entero.
        let sin_dictionary = Config::parse_or_default(r#"{"model":"base"}"#);
        assert_eq!(
            sin_dictionary.selected_model(),
            Model::Base,
            "un campo ausente no tira el resto de la config"
        );
        assert!(sin_dictionary.dictionary.is_empty());
    }

    #[test]
    fn palette_roundtrip_y_legacy() {
        // Por defecto: la paleta entera por defecto (None).
        assert_eq!(Config::default().palette, None);

        // Config personalizada → se guarda y se vuelve a leer igual.
        let cfg = Config {
            palette: Some(PaletteConfig {
                accent: Some("#00AAFF".into()),
                text: Some("#101010".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let raw = serde_json::to_string(&cfg).unwrap();
        let leida: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            leida.palette.as_ref().and_then(|p| p.accent.clone()),
            Some("#00AAFF".into())
        );
        assert_eq!(
            leida.palette.as_ref().and_then(|p| p.text.clone()),
            Some("#101010".into())
        );
        // Los campos None se dejan fuera del JSON para que el config quede limpio.
        assert!(!raw.contains("\"bg\":"));
        assert!(!raw.contains("\"surface\":"));

        // Config vieja sin el campo → None (todo por defecto).
        let legacy: Config = serde_json::from_str(r#"{ "dictionary": [] }"#).unwrap();
        assert_eq!(legacy.palette, None);

        // Un PaletteConfig con todos los campos None = is_empty y se omite.
        let vacia = Config {
            palette: Some(PaletteConfig::default()),
            ..Default::default()
        };
        let raw_vacia = serde_json::to_string(&vacia).unwrap();
        // El PaletteConfig de dentro se guarda como {} (todos sus campos son
        // None); pero el Option<PaletteConfig> = Some → aparece.
        assert!(raw_vacia.contains("\"palette\":{"));
    }
}
