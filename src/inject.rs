//! Inyección de texto en la aplicación activa (la que tiene el foco).
//! Port del command `type_text` de la versión Tauri: copia al portapapeles
//! con arboard y simula Ctrl+V con enigo. Solo Windows.
//!
//! Comportamiento configurable (config.json → `Config::inject_settings`):
//! - formato: `Paste` (portapapeles + Ctrl+V, el de siempre) o `Type`
//!   (tecleo directo con enigo, sin tocar el portapapeles del usuario);
//! - retardo opcional antes de pegar/teclear (ms; 0 = sin retardo);
//! - filtro por título de la ventana activa: si no coincide, NO se inyecta
//!   (y se devuelve `InjectOutcome::Skipped` para avisar sin tratarlo como
//!   error).

use std::time::Duration;

/// Formato de inyección del turno en la app activa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectFormat {
    /// Copia al portapapeles (arboard) y pega con Ctrl+V. Default: la app
    /// destino recibe el texto como pegado nativo (funciona en casi todo).
    Paste,
    /// Teclea el texto carácter a carácter con enigo (entrada unicode), sin
    /// pisar el portapapeles del usuario. Más lento, pero no clobberea lo
    /// que el usuario tuviera copiado.
    Type,
}

impl InjectFormat {
    /// Etiqueta en español del selector «Formato de inyección» (Ajustes).
    pub fn label(self) -> &'static str {
        match self {
            InjectFormat::Paste => "Pegar (Ctrl+V)",
            InjectFormat::Type => "Teclear",
        }
    }
}

/// Comportamiento de inyección (lo construye `Config::inject_settings`).
#[derive(Debug, Clone)]
pub struct InjectSettings {
    pub format: InjectFormat,
    /// Retardo antes de pegar/teclear: deja que la app destino asiente el
    /// foco (0 = sin retardo). Default 40 ms.
    pub delay: Duration,
    /// Filtro por título de la ventana activa (substring, sin distinguir
    /// mayúsculas): `None`/vacío = inyectar en cualquier app.
    pub window_filter: Option<String>,
}

impl Default for InjectSettings {
    fn default() -> Self {
        Self {
            format: InjectFormat::Paste,
            delay: Duration::from_millis(40),
            window_filter: None,
        }
    }
}

/// Resultado de la inyección: distinguir el éxito real de «omitida por
/// filtro» (un aviso, no un error técnico) y de «no había ventana activa».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectOutcome {
    Injected,
    /// La ventana activa (título) no pasó el filtro configurado.
    Skipped(String),
    /// No había NINGUNA ventana con el foco al inyectar (escritorio al
    /// descubierto, transición entre apps). Pegar a ciegas no tiene
    /// destino: mejor avisar y dejar el texto en el portapapeles (lo copia
    /// la app al recibir este resultado).
    NoForegroundWindow,
}

impl InjectOutcome {
    /// Línea de feedback para la UI: español, corta y accionable (requisito
    /// de robustez — ningún resultado se queda mudo).
    pub fn notice(&self) -> String {
        match self {
            InjectOutcome::Injected => "Texto escrito en la app activa ✓".to_string(),
            InjectOutcome::Skipped(_) => {
                "Inyección omitida: la app activa no coincide con el filtro de ventana"
                    .to_string()
            }
            InjectOutcome::NoForegroundWindow => {
                "No había ninguna ventana activa: el texto quedó copiado en el portapapeles"
                    .to_string()
            }
        }
    }
}

/// True si el título de la ventana activa pasa el filtro (substring,
/// case-insensitive). Filtro ausente o en blanco → siempre pasa.
pub fn window_matches_filter(title: &str, filter: Option<&str>) -> bool {
    match filter.map(str::trim).filter(|f| !f.is_empty()) {
        None => true,
        Some(f) => title.to_lowercase().contains(&f.to_lowercase()),
    }
}

/// Copia `text` al portapapeles (best-effort, para cuando la inyección no
/// puede hacerse: sin ventana activa, app destino que no acepta pegado…).
/// El texto nunca se pierde si el usuario puede pegarlo a mano donde quiera.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use arboard::Clipboard;
    let mut clipboard =
        Clipboard::new().map_err(|err| format!("no se pudo abrir el portapapeles: {err}"))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|err| format!("no se pudo copiar el texto al portapapeles: {err}"))
}

/// Stub en otros SO (contrato idéntico).
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn copy_to_clipboard(_text: &str) -> Result<(), String> {
    Err("el portapapeles solo está disponible en Windows".to_string())
}

/// Escribe `text` en la app que tiene el foco, según `opts` (formato, retardo
/// y filtro de ventana).
///
/// Devuelve `Ok(InjectOutcome)` con el desenlace real —«omitida por filtro» y
/// «sin ventana activa» NO son errores— o `Err` con mensaje en español si
/// falla el portapapeles o el simulador de teclado. Nunca simula teclado a
/// ciegas: sin ventana con el foco no hay destino donde pegar.
#[cfg(target_os = "windows")]
pub fn inject_text_into_active_app(
    text: &str,
    opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    // Sin NINGUNA ventana con el foco no hay dónde pegar: no se simula
    // teclado a ciegas (el Ctrl+V iría a ninguna parte o al escritorio).
    // El texto queda a disposición por el portapapeles (lo copia la app).
    let Some(title) = foreground_window_title() else {
        crate::log!("inject", "no hay ventana con el foco; inyección omitida");
        return Ok(InjectOutcome::NoForegroundWindow);
    };

    // Filtro por ventana activa ANTES de tocar portapapeles/teclado: si la
    // app con foco no coincide con `window_filter`, el turno no se inyecta.
    if !window_matches_filter(&title, opts.window_filter.as_deref()) {
        crate::log!(
            "inject",
            "ventana activa «{title}» no pasa el filtro «{}»; inyección omitida",
            opts.window_filter.as_deref().unwrap_or_default()
        );
        return Ok(InjectOutcome::Skipped(title));
    }

    // Retardo configurable: deja que la app destino asiente el foco.
    std::thread::sleep(opts.delay);

    match opts.format {
        InjectFormat::Paste => {
            let mut clipboard = Clipboard::new()
                .map_err(|err| format!("no se pudo abrir el portapapeles: {err}"))?;
            clipboard
                .set_text(text.to_owned())
                .map_err(|err| format!("no se pudo copiar el texto al portapapeles: {err}"))?;

            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|err| format!("no se pudo inicializar el simulador de teclado: {err}"))?;
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|err| format!("no se pudo simular la pulsación de Ctrl: {err}"))?;
            // Ctrl+V con garantía de soltar: si el Click falla y devolvemos
            // el error sin más, el usuario se quedaría con Ctrl PRESIONADO
            // (corrompería todo lo que teclee después). Se suelta SIEMPRE,
            // incluso en error, y solo entonces se reporta el fallo.
            let clicked = enigo.key(Key::V, Direction::Click);
            let released = enigo.key(Key::Control, Direction::Release);
            clicked.map_err(|err| format!("no se pudo simular Ctrl+V: {err}"))?;
            released.map_err(|err| format!("no se pudo soltar Ctrl tras el pegado: {err}"))?;
            Ok(InjectOutcome::Injected)
        }
        InjectFormat::Type => {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|err| format!("no se pudo inicializar el simulador de teclado: {err}"))?;
            // Modo teclear: un fallo a mitad deja la nota escrita A MEDIOS
            // en la app destino. No se puede deshacer desde aquí; el mensaje
            // le dice al usuario cómo salir del paso (Ctrl+Z en la destino).
            enigo.text(text).map_err(|err| {
                format!("no se pudo teclear el texto (puede haber quedado a medias; Ctrl+Z en la app destino lo deshace): {err}")
            })?;
            Ok(InjectOutcome::Injected)
        }
    }
}

/// Título de la ventana con foco (foreground) vía Win32. `None` si no hay
/// ninguna; título vacío si la ventana no tiene texto.
#[cfg(target_os = "windows")]
fn foreground_window_title() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use winapi::um::winuser::{GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return Some(String::new());
        }
        buf.truncate(n as usize);
        Some(OsString::from_wide(&buf).to_string_lossy().into_owned())
    }
}

/// Stub fuera de Windows: la inyección (portapapeles + enigo) es de
/// escritorio Windows; aquí siempre `Err`, con el mismo contrato.
#[cfg(not(target_os = "windows"))]
pub fn inject_text_into_active_app(
    _text: &str,
    _opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    Err("la inyección de texto solo está disponible en Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_filtro_siempre_pasa() {
        assert!(window_matches_filter("Visual Studio Code", None));
        assert!(window_matches_filter("cualquier cosa", Some("")));
        assert!(window_matches_filter("cualquier cosa", Some("   ")));
    }

    #[test]
    fn filtro_es_substring_sin_mayusculas() {
        assert!(window_matches_filter("Notas - OneNote", Some("onenote")));
        assert!(window_matches_filter("Notas - OneNote", Some("ONE")));
        assert!(!window_matches_filter("Notas - OneNote", Some("word")));
        assert!(!window_matches_filter("Bloc de notas", Some("writ")));
    }

    #[test]
    fn defaults_paste_con_retardo_y_sin_filtro() {
        let d = InjectSettings::default();
        assert_eq!(d.format, InjectFormat::Paste);
        assert_eq!(d.delay.as_millis(), 40);
        assert_eq!(d.window_filter, None);
    }

    #[test]
    fn labels_de_formato() {
        assert_eq!(InjectFormat::Paste.label(), "Pegar (Ctrl+V)");
        assert_eq!(InjectFormat::Type.label(), "Teclear");
    }

    #[test]
    fn cada_resultado_de_inyeccion_tiene_su_aviso_en_espanol() {
        // Requisito de robustez: ningún resultado se queda mudo; los tres
        // casos terminan en una frase corta y accionable para la UI.
        assert_eq!(
            InjectOutcome::Injected.notice(),
            "Texto escrito en la app activa ✓"
        );
        let skipped = InjectOutcome::Skipped("Bloc de notas".into()).notice();
        assert!(skipped.contains("filtro"), "{skipped}");
        let no_window = InjectOutcome::NoForegroundWindow.notice();
        assert!(no_window.contains("portapapeles"), "{no_window}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn stub_falla_fuera_de_windows() {
        let opts = InjectSettings::default();
        assert!(inject_text_into_active_app("hola", &opts).is_err());
    }
}
