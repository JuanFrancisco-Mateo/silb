//! Escribir el texto en la app que tiene el foco.
//! Viene del comando `type_text` de la versión Tauri: copia al portapapeles
//! con arboard y simula Ctrl+V con enigo (Windows); en Linux el pegado va por
//! herramientas del entorno (wl-copy/wtype/ydotool) porque Wayland no deja
//! que una app pulse teclas en otra.
//!
//! Se puede cambiar (config.json → `Config::inject_settings`):
//! - formato: `Paste` (portapapeles y pegar, lo de siempre) o `Type`
//!   (teclear directamente, sin tocar el portapapeles del usuario);
//! - espera opcional antes de pegar o teclear (ms; 0 = ninguna);
//! - filtro por título de la ventana activa: si no coincide, NO se escribe
//!   (y se devuelve `InjectOutcome::Skipped` para avisar sin tratarlo como
//!   error);
//! - en Linux, por dónde escribir: `wl-clipboard`, `ydotool` o `wtype`
//!   (`inject_backend` en config.json; ver CODIGO-CORE.md del port).
//!
//! Regla de la casa: NUNCA se pierde texto. Lo primero que hace cualquier
//! pegado es copiar el turno al portapapeles; si lo de teclear falla, ahí
//! se queda para pegarlo a mano.

use std::time::Duration;

/// Cómo se escribe el turno en la app activa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectFormat {
    /// Copia al portapapeles (arboard / wl-copy) y pega con el atajo del
    /// sistema (Ctrl+V en Windows; Ctrl+V con `wtype` en Linux).
    Paste,
    /// Teclea el texto letra a letra (enigo en Windows; `ydotool type` o
    /// `wtype` en Linux), sin pisar el portapapeles del usuario. Más lento, pero
    /// no le quita lo que tuviera copiado.
    Type,
}

impl InjectFormat {
    /// Texto en español del selector «Formato de inyección» (Ajustes).
    pub fn label(self) -> &'static str {
        match self {
            InjectFormat::Paste => "Pegar (Ctrl+V)",
            InjectFormat::Type => "Teclear",
        }
    }
}

/// Cómo se escribe en LINUX (lo que en Windows hace enigo). Se elige con
/// `inject_backend` en config.json; por defecto `wl-clipboard` (el que no
/// pide programas de fondo ni permisos de grupo).
///
/// - `WlClipboard`: usa `wl-copy` (portapapeles de Wayland) y `wtype` para la
///   pulsación de Ctrl+V en la app con foco (el byte \x16 por stdin). Sin
///   programas de fondo.
/// - `Ydotool`: teclea con `ydotool` (uinput, /dev/uinput del sistema), que
///   escribe en cualquier app y en cualquier entorno (Xwayland incluido), PERO
///   pide `ydotoold` en marcha con acceso a /dev/uinput (permisos o grupo
///   input). Si el programa no está, se queda a medias: el texto se copia al
///   portapapeles y se avisa (nunca se pierde).
/// - `Wtype`: teclea con `wtype`, un programa ligero de Wayland puro. Solo va
///   en apps de Wayland; las que bloquean el teclado virtual
///   (zwp_virtual_keyboard_v1) lo rechazan → se avisa y queda el portapapeles.
///
/// A validar en el binario: pegar con wtype en apps Electron/Qt concretas
/// (algunas tienen su propio atajo de pegado) y la ruta del socket de ydotoold
/// en Arch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinuxInjectBackend {
    /// wl-copy (portapapeles) + wtype para la pulsación de pegar, sin
    /// programas de fondo. Por defecto.
    #[default]
    WlClipboard,
    /// ydotool (uinput con `ydotoold`): tecleo directo, vale para todo
    /// (Xwayland incluido). Pide el programa de fondo y permiso sobre /dev/uinput.
    Ydotool,
    /// wtype puro de Wayland: tecleo directo como teclado virtual.
    Wtype,
}

impl LinuxInjectBackend {
    /// Nombre del backend en config.json (`inject_backend`).
    pub fn as_config_str(self) -> &'static str {
        match self {
            LinuxInjectBackend::WlClipboard => "wl-clipboard",
            LinuxInjectBackend::Ydotool => "ydotool",
            LinuxInjectBackend::Wtype => "wtype",
        }
    }

    /// Texto en español del selector de Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            LinuxInjectBackend::WlClipboard => "wl-clipboard (portapapeles + wtype)",
            LinuxInjectBackend::Ydotool => "ydotool (uinput, requiere ydotoold)",
            LinuxInjectBackend::Wtype => "wtype (teclea directo)",
        }
    }
}

/// Cómo se escribe (lo arma `Config::inject_settings`). Los tres campos
/// comunes los lee también Ajustes; `backend` solo existe en Linux (el
/// selector enigo/ydotool/wtype no pinta nada en Windows, donde enigo es la
/// única forma).
#[derive(Debug, Clone)]
pub struct InjectSettings {
    pub format: InjectFormat,
    /// Espera antes de pegar o teclear: deja que la app de destino coja el
    /// foco (0 = sin espera). Por defecto 40 ms.
    pub delay: Duration,
    /// Filtro por el título de la ventana activa (basta con que lo contenga,
    /// sin mirar mayúsculas): `None`/vacío = escribir en cualquier app.
    pub window_filter: Option<String>,
    /// Cómo se escribe en Linux (`inject_backend` en config.json). Solo Unix.
    #[cfg(unix)]
    pub backend: LinuxInjectBackend,
}

impl Default for InjectSettings {
    fn default() -> Self {
        Self {
            format: InjectFormat::Paste,
            delay: Duration::from_millis(40),
            window_filter: None,
            // Solo Linux: por dónde escribir por defecto (wl-clipboard).
            // Faltaba al construir la estructura tras el port — E0063.
            #[cfg(unix)]
            backend: LinuxInjectBackend::default(),
        }
    }
}

/// Resultado de escribir: distingue el éxito de «no se hizo por el filtro»
/// (un aviso, no un error) y de «no había ventana activa».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectOutcome {
    Injected,
    /// La ventana activa (su título) no pasó el filtro del config.
    Skipped(String),
    /// No había NINGUNA ventana con el foco (o el sistema no lo dice) al
    /// escribir. Pegar a ciegas no tiene destino: mejor avisar y dejar el texto
    /// en el portapapeles (la app lo copia al recibir este resultado).
    NoForegroundWindow,
}

impl InjectOutcome {
    /// Frase para el aviso de la interfaz: en español, corta y útil (ningún
    /// resultado se queda sin decir nada).
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

/// True si el título de la ventana activa pasa el filtro (basta con que lo
/// contenga, sin mirar mayúsculas). Sin filtro o en blanco → pasa siempre.
pub fn window_matches_filter(title: &str, filter: Option<&str>) -> bool {
    match filter.map(str::trim).filter(|f| !f.is_empty()) {
        None => true,
        Some(f) => title.to_lowercase().contains(&f.to_lowercase()),
    }
}

/// Saca el valor de un campo `"campo": "valor"` de un texto JSON. Sin
/// dependencias nuevas (std): saca `title`/`class` del JSON de
/// `hyprctl activewindow -j`. Nota: serde_json ya está en el proyecto, pero
/// este lector mínimo evita convertir tipos por solo dos campos. Función
/// suelta, se puede probar en cualquier sistema.
pub fn json_string_field(raw: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let start = raw.find(&needle)? + needle.len();
    let rest = &raw[start..];
    let colon = rest.find(':')?;
    let rest = &rest[colon + 1..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => {
                let esc = chars.next()?;
                out.push(match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '"' => '"',
                    '\\' => '\\',
                    // \uXXXX: demasiado raro en un título de ventana; se
                    // mete tal cual y se confía en la app de destino.
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        let cp = u32::from_str_radix(&hex, 16).ok()?;
                        out.push(char::from_u32(cp)?);
                        continue;
                    }
                    other => other,
                });
            }
            other => out.push(other),
        }
    }
    None
}

/// Copia `text` al portapapeles (si se puede, para cuando no se puede
/// escribir: sin ventana activa, app de destino que no acepta pegado…).
/// Así el texto no se pierde: el usuario puede pegarlo a mano donde quiera.
///
/// Windows y macOS: arboard (en Windows enigo pega o teclea tras la copia; en
/// macOS es la única forma). Linux: `wl-copy` (wl-clipboard) — el arboard de
/// Linux (smithay) no fija el foco de la ventana del portapapeles de Wayland y
/// se puede descolocar con wtype/ydotool (en macOS no hay wl-copy: ahí queda
/// arboard).
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use arboard::Clipboard;
    let mut clipboard =
        Clipboard::new().map_err(|err| format!("no se pudo abrir el portapapeles: {err}"))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|err| format!("no se pudo copiar el texto al portapapeles: {err}"))
}

/// Linux: portapapeles de Wayland con `wl-copy` (wl-clipboard). El texto va
/// por stdin (no como argumento: sin límite y sin que se vea en `ps`). wl-copy
/// se queda de fondo sirviendo el portapapeles y el padre sale, así que
/// `output()` vuelve rápido.
#[cfg(target_os = "linux")]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            format!("no se pudo lanzar wl-copy para el portapapeles (`pacman -S wl-clipboard`): {err}")
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|err| format!("no se pudo escribir el texto en wl-copy: {err}"))?;
    }
    let salida = child
        .wait_with_output()
        .map_err(|err| format!("wl-copy terminó mal: {err}"))?;
    if salida.status.success() {
        Ok(())
    } else {
        // wl-copy limpia el stdin y se queda de fondo aunque el entorno se
        // sature: un código de salida distinto de 0 es la única señal fiable de
        // que el portapapeles no llegó a copiarse.
        Err(format!(
            "wl-copy terminó con error {}: no se pudo copiar al portapapeles Wayland",
            salida.status
        ))
    }
}

/// Versión vacía para sistemas que no son Windows ni Unix: sin portapapeles
/// del sistema. (La condición usa `target_family = "unix"`: `target_os =
/// "unix"` no es un valor válido y hacía que esta versión se compilara también
/// en Linux.)
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
pub fn copy_to_clipboard(_text: &str) -> Result<(), String> {
    Err("el portapapeles solo está disponible en Windows/Linux".to_string())
}

/// Escribe `text` en la app con foco, según `opts` (formato, espera y filtro
/// de ventana). En Windows usa arboard + enigo (Ctrl+V o tecleo unicode); en
/// Linux usa wl-copy/wtype/ydotool según `inject_backend` del config.json.
/// Devuelve `Ok(InjectOutcome)` con lo que pasó de verdad y un `Err` con
/// mensaje en español si falla el portapapeles o el teclado virtual. Nunca
/// teclea a ciegas: sin ventana con el foco no hay destino.
#[cfg(target_os = "windows")]
pub fn inject_text_into_active_app(
    text: &str,
    opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    // Sin NINGUNA ventana con el foco no hay dónde pegar: no se pulsa teclado
    // a ciegas (el Ctrl+V iría a ninguna parte o al escritorio). El texto queda
    // en el portapapeles (lo copia la app).
    let Some(title) = foreground_window_title() else {
        crate::log!("inject", "no hay ventana con el foco; inyección omitida");
        return Ok(InjectOutcome::NoForegroundWindow);
    };

    // El filtro de ventana se mira ANTES de tocar portapapeles o teclado: si
    // la app con foco no coincide con `window_filter`, el turno no se escribe.
    if !window_matches_filter(&title, opts.window_filter.as_deref()) {
        crate::log!(
            "inject",
            "ventana activa «{title}» no pasa el filtro «{}»; inyección omitida",
            opts.window_filter.as_deref().unwrap_or_default()
        );
        return Ok(InjectOutcome::Skipped(title));
    }

    // Espera del ajuste: deja que la app de destino coja el foco.
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
            // Ctrl+V con garantía de soltar: si el clic falla y se devuelve el
            // error sin más, el usuario se queda con Ctrl PULSADO (estropearía
            // todo lo que teclee después). Se suelta SIEMPRE, también al fallar,
            // y solo entonces se cuenta el fallo.
            let clicked = enigo.key(Key::V, Direction::Click);
            let released = enigo.key(Key::Control, Direction::Release);
            clicked.map_err(|err| format!("no se pudo simular Ctrl+V: {err}"))?;
            released.map_err(|err| format!("no se pudo soltar Ctrl tras el pegado: {err}"))?;
            Ok(InjectOutcome::Injected)
        }
        InjectFormat::Type => {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|err| format!("no se pudo inicializar el simulador de teclado: {err}"))?;
            // Modo teclear: un fallo a mitad deja la nota escrita A MEDIAS en
            // la app de destino. Desde aquí no se puede deshacer; el mensaje le
            // dice al usuario cómo salir del paso (Ctrl+Z en la app de destino).
            enigo.text(text).map_err(|err| {
                format!("no se pudo teclear el texto (puede haber quedado a medias; Ctrl+Z en la app destino lo deshace): {err}")
            })?;
            Ok(InjectOutcome::Injected)
        }
    }
}

// ---------------------------------------------------------------------------
// Linux (Wayland/Hyprland): no hay forma de pulsar teclas en otra app (no hay
// un atajo global del sistema). El portapapeles va por wl-copy y la «pulsación
// de Ctrl+V» la hace el teclado virtual que haya. Tres formas y, si falla, se
// queda el texto copiado.
// ---------------------------------------------------------------------------

/// Título de la ventana con foco, con `hyprctl activewindow -j` (Hyprland).
/// `None` si no hay ventana activa (escritorio a la vista) o si el entorno no
/// contestó. Con Xwayland también va (el entorno ve sus ventanas).
///
/// A validar: en KDE/GNOME Wayland el mismo dato pediría `swaymsg -t get_tree`
/// / `kdotool`; si no se consigue, se salta la comprobación del foco y el pegado
/// se queda en el portapapeles (nunca se teclea a ciegas).
/// Título de la ventana con foco, por Win32. `None` si no hay ninguna; título
/// vacío si la ventana no tiene texto.
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

#[cfg(unix)]
fn foreground_window_title() -> Option<String> {
    let output = std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    // Una ventana sin barra de título suele dar title "" pero una class útil:
    // el filtro del usuario se mira sobre title y, si viene vacío, se prueba
    // con class (por ejemplo "firefox" como clase de Firefox).
    let title = json_string_field(&raw, "title").unwrap_or_default();
    let class = json_string_field(&raw, "class").unwrap_or_default();
    if title.is_empty() && !class.is_empty() {
        Some(class)
    } else {
        Some(title)
    }
}

/// En sistemas que no son Windows ni Unix no hay nada que preguntar. (La
/// condición es `target_family = "unix"`, no `target_os`.)
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
fn foreground_window_title() -> Option<String> {
    None
}

/// ¿Se puede llegar al socket de `ydotoold`? Sin él, ydotool no puede
/// escribir nada (el programa cliente solo habla con el de fondo). Rutas
/// típicas en Arch/Omarchy: /run/ydotoold/.socket o la de XDG runtime dir.
/// A validar en el binario.
#[cfg(unix)]
fn ydotoold_reachable() -> bool {
    use std::path::Path;
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    [
        Path::new("/run/ydotoold/.socket"),
        Path::new("/run/user/1000/.ydotool_socket"),
        &xdg.join(".ydotool_socket"),
    ]
    .iter()
    .any(|path| path.exists())
}

/// Aviso para cuando falla el modo ydotool: no está el programa de fondo
/// (`ydotoold` sin arrancar o el socket sin acceso). Se trata como
/// `Ok(NoForegroundWindow)`: quien llama (app.rs) copia el texto al
/// portapapeles y enseña «el texto quedó copiado» — cierto, aunque no nombra
/// al culpable. Para no mentir, el detalle se queda en el log de esta función.
#[cfg(unix)]
fn inject_with_ydotool(text: &str) -> Result<InjectOutcome, String> {
    if !ydotoold_reachable() {
        crate::log!(
            "inject",
            "backend ydotool sin ydotoold corriendo: el texto queda en el portapapeles \
             (arranque del daemon: `ydotoold &` y permiso de grupo input en /dev/uinput)"
        );
        return Ok(InjectOutcome::NoForegroundWindow);
    }
    // `ydotool type -- <texto>` teclea el texto entero (modo unicode).
    // Un ydotool que TERMINA con error puede haber dejado el texto a medias en
    // el destino: se avisa con Ctrl+Z como rescate (igual que el mensaje del
    // modo Type de Windows).
    let status = std::process::Command::new("ydotool")
        .arg("type")
        .arg("--key-delay")
        .arg("12")
        .arg("--")
        .arg(text)
        .status()
        .map_err(|err| format!("no se pudo lanzar ydotool (paquete `ydotool`): {err}"))?;
    if !status.success() {
        return Err("ydotool terminó con error: el texto puede haber quedado a medias (Ctrl+Z en la app destino lo deshace)".to_string());
    }
    Ok(InjectOutcome::Injected)
}

/// wtype: teclear el texto por stdin (no como argumento, sin límite). Solo va
/// si el destino es una app de Wayland que no bloquee el teclado virtual
/// (zwp_virtual_keyboard_v1); sin `wtype` en el PATH → mensaje con cómo
/// instalarlo (`pacman -S wtype`).
#[cfg(unix)]
fn inject_with_wtype(text: &str) -> Result<InjectOutcome, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("wtype")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            format!("no se pudo lanzar wtype (paquete `wtype` de Arch extra): {err}")
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|err| format!("no se pudo escribir el texto en wtype: {err}"))?;
    }
    let salida = child
        .wait_with_output()
        .map_err(|err| format!("wtype terminó mal: {err}"))?;
    if salida.status.success() {
        // Si wtype va bien, el texto está tecleado: la misma variante que el
        // modo ydotool (quien llama espera un InjectOutcome, no `()`).
        Ok(InjectOutcome::Injected)
    } else {
        Err("wtype terminó con error: la app destino puede estar bloqueando el teclado virtual \
             (zwp_virtual_keyboard_v1); prueba con el portapapeles o el backend ydotool"
            .to_string())
    }
}

/// `wl-copy` y la pulsación de pegar con `wtype`: wtype entiende el byte de
/// control \x16 (SYN, el código del Ctrl+V en terminales) como Ctrl+V y no
/// como texto. En Wayland NO hay un Ctrl+V global fiable, pero aquí el destino
/// TIENE el foco (sin foco se corta antes): la pulsación entra directa en la
/// app que el usuario ya está mirando. A validar en el binario: entorno y app
/// de destino concretos.
#[cfg(unix)]
fn inject_with_wl_clipboard(text: &str) -> Result<InjectOutcome, String> {
    copy_to_clipboard(text)?;
    inject_with_wtype("\x16") // \x16 a wtype = pulso Ctrl+V
}

/// Escritura completa en Linux: ventana → filtro → espera → forma de escribir.
#[cfg(unix)]
pub fn inject_text_into_active_app(
    text: &str,
    opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    // 1) ¿Hay app con foco donde escribir? Misma comprobación que en Windows:
    //    pegar a ciegas no tiene destino (aunque sin título no se pueda filtrar,
    //    al menos se evita pulsar al aire).
    let Some(title) = foreground_window_title() else {
        crate::log!(
            "inject",
            "no hay ventana con el foco (o sin hyprctl); inyección omitida"
        );
        return Ok(InjectOutcome::NoForegroundWindow);
    };
    // 2) Filtro por título ANTES de tocar portapapeles o teclado.
    if !window_matches_filter(&title, opts.window_filter.as_deref()) {
        crate::log!(
            "inject",
            "ventana activa «{title}» no pasa el filtro «{}»; inyección omitida",
            opts.window_filter.as_deref().unwrap_or_default()
        );
        return Ok(InjectOutcome::Skipped(title));
    }
    // 3) Espera del ajuste (igual que en Windows).
    std::thread::sleep(opts.delay);
    // 4) Forma de escribir elegida en el config.
    match opts.backend {
        LinuxInjectBackend::WlClipboard => match opts.format {
            InjectFormat::Paste => inject_with_wl_clipboard(text),
            InjectFormat::Type => inject_with_wtype(text),
        },
        LinuxInjectBackend::Ydotool => match opts.format {
            // Pegar con ydotool = copiar al portapapeles y hacer que teclee
            // el atajo Ctrl+V (el byte \x16 llega al teclado virtual como la
            // combinación de pegar). A validar en el binario: con ydotoold en
            // marcha, la app de destino tiene que aceptar el Ctrl+V.
            InjectFormat::Paste => {
                copy_to_clipboard(text)?;
                inject_with_ydotool("\x16")
            }
            // Teclear es el ydotool de siempre (ydotool type -- <texto>).
            InjectFormat::Type => inject_with_ydotool(text),
        },
        LinuxInjectBackend::Wtype => match opts.format {
            // Con wtype como teclado, el pegado clásico también se puede
            // hacer (portapapeles + Ctrl+V virtual), dejando el texto copiado
            // primero (regla «nunca se pierde texto»).
            InjectFormat::Paste => inject_with_wl_clipboard(text),
            InjectFormat::Type => inject_with_wtype(text),
        },
    }
}

/// Versión vacía para sistemas que no son Windows ni Unix: mismo contrato,
/// siempre `Err`. (`target_family = "unix"`, no `target_os`.)
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
pub fn inject_text_into_active_app(
    _text: &str,
    _opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    Err("la inyección de texto está disponible en Windows y Linux".to_string())
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
    fn labels_de_backend_linux() {
        assert_eq!(
            LinuxInjectBackend::WlClipboard.as_config_str(),
            "wl-clipboard"
        );
        assert_eq!(LinuxInjectBackend::Ydotool.as_config_str(), "ydotool");
        assert_eq!(LinuxInjectBackend::Wtype.as_config_str(), "wtype");
    }

    #[test]
    fn cada_resultado_de_inyeccion_tiene_su_aviso_en_espanol() {
        // Ningún resultado se queda sin decir nada: los tres casos acaban en
        // una frase corta y útil para la interfaz.
        assert_eq!(
            InjectOutcome::Injected.notice(),
            "Texto escrito en la app activa ✓"
        );
        let skipped = InjectOutcome::Skipped("Bloc de notas".into()).notice();
        assert!(skipped.contains("filtro"), "{skipped}");
        let no_window = InjectOutcome::NoForegroundWindow.notice();
        assert!(no_window.contains("portapapeles"), "{no_window}");
    }

    #[test]
    fn json_string_field_saca_el_campo_de_hyprctl_activewindow() {
        // Muestra real (recortada) de `hyprctl activewindow -j`.
        let raw = r#"{"address":"0x5f33e1d5f60","title":" • Minano - kanban — Zona","initialTitle":"Minano","class":"Minano","xwayland":false}"#;
        assert_eq!(
            json_string_field(raw, "title"),
            Some(" • Minano - kanban — Zona".to_string())
        );
        assert_eq!(json_string_field(raw, "class"), Some("Minano".to_string()));
        assert_eq!(json_string_field(raw, "missing"), None);
        // Escapes: \n y \" dentro del título.
        let con_escapes = r#"{"title":"línea\nvacia y \"cita\""}"#;
        assert_eq!(
            json_string_field(con_escapes, "title"),
            Some("línea\nvacia y \"cita\"".to_string())
        );
    }

    #[test]
    fn backend_labels_no_vacios() {
        for b in [
            LinuxInjectBackend::WlClipboard,
            LinuxInjectBackend::Ydotool,
            LinuxInjectBackend::Wtype,
        ] {
            assert!(!b.label().is_empty());
            assert!(!b.as_config_str().is_empty());
        }
    }
}
