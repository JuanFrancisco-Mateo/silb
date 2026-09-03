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
/// filtro» (un aviso, no un error técnico).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectOutcome {
    Injected,
    /// La ventana activa (título) no pasó el filtro configurado.
    Skipped(String),
}

/// True si el título de la ventana activa pasa el filtro (substring,
/// case-insensitive). Filtro ausente o en blanco → siempre pasa.
pub fn window_matches_filter(title: &str, filter: Option<&str>) -> bool {
    match filter.map(str::trim).filter(|f| !f.is_empty()) {
        None => true,
        Some(f) => title.to_lowercase().contains(&f.to_lowercase()),
    }
}

#[cfg(target_os = "windows")]
pub fn inject_text_into_active_app(
    text: &str,
    opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    // Filtro por ventana activa ANTES de tocar portapapeles/teclado: si la
    // app con foco no coincide con `window_filter`, el turno no se inyecta.
    if let Some(title) = foreground_window_title() {
        if !window_matches_filter(&title, opts.window_filter.as_deref()) {
            crate::log!(
                "inject",
                "ventana activa «{title}» no pasa el filtro «{}»; inyección omitida",
                opts.window_filter.as_deref().unwrap_or_default()
            );
            return Ok(InjectOutcome::Skipped(title));
        }
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
            enigo
                .key(Key::V, Direction::Click)
                .map_err(|err| format!("no se pudo simular Ctrl+V: {err}"))?;
            enigo
                .key(Key::Control, Direction::Release)
                .map_err(|err| format!("no se pudo soltar Ctrl: {err}"))?;
            Ok(InjectOutcome::Injected)
        }
        InjectFormat::Type => {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|err| format!("no se pudo inicializar el simulador de teclado: {err}"))?;
            enigo
                .text(text)
                .map_err(|err| format!("no se pudo teclear el texto: {err}"))?;
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

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn stub_falla_fuera_de_windows() {
        let opts = InjectSettings::default();
        assert!(inject_text_into_active_app("hola", &opts).is_err());
    }
}