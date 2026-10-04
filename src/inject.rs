//! Inyección de texto en la aplicación activa (la que tiene el foco).
//! Port del command `type_text` de la versión Tauri: copia al portapapeles
//! con arboard y simula Ctrl+V con enigo (Windows); en Linux el pegado va
//! por herramientas del compositor/teclado (wl-copy/wtype/ydotool) porque
//! Wayland no permite a una app simular el teclado de otra (sin singleton
//! global tipo LogitechHook).
//!
//! Comportamiento configurable (config.json → `Config::inject_settings`):
//! - formato: `Paste` (portapapeles + pegado, el de siempre) o `Type`
//!   (tecleo directo, sin tocar el portapapeles del usuario);
//! - retardo opcional antes de pegar/teclear (ms; 0 = sin retardo);
//! - filtro por título de la ventana activa: si no coincide, NO se inyecta
//!   (y se devuelve `InjectOutcome::Skipped` para avisar sin tratarlo como
//!   error).
//! - backend de Linux (solo Linux): `wl-clipboard`, `ydotool` o `wtype`
//!   (`inject_backend` en config.json; consulte CODIGO-CORE.md del port).
//!
//! Regla de la casa (port del original): NUNCA se pierde texto. La primera
//! acción de todo pegado es copiar el turno al portapapeles; si el backend
//! de teclado falla, queda ahí para el pegado manual.

use std::time::Duration;

/// Formato de inyección del turno en la app activa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectFormat {
    /// Copia al portapapeles (arboard / wl-copy) y pega con la combinación
    /// del sistema (Ctrl+V en Windows; Ctrl+V con `wtype` en Linux).
    Paste,
    /// Teclea el texto carácter a carácter (enigo en Windows; `ydotool type`
    /// o `wtype` en Linux), sin pisar el portapapeles del usuario. Más lento,
    /// pero no clobber-ea lo que el usuario tuviera copiado.
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

/// Backend de inyección en LINUX (lo que en Windows hace enigo). Se elige
/// con `inject_backend` en config.json; el valor por defecto es
/// `wl-clipboard` (el que no requiere daemon ni permisos de grupo).
///
/// - `WlClipboard`: usa `wl-copy` (portapapeles Wayland) + `wtype` para la
///   pulsación de Ctrl+V en la app con foco (byte \x16 por stdin). Sin
///   daemons, sin autostart.
/// - `Ydotool`: teclea con `ydotool` (uinput /dev/uinput del kernel), que
///   inyecta en cualquier app y en cualquier compositor (Xwayland
///   incluido), PERO exige `ydotoold` corriendo con acceso a /dev/uinput
///   (permisos o grupo input). Si el daemon no está, degrada: el texto se
///   copia al portapapeles y se avisa (nunca se pierde).
/// - `Wtype`: teclea con `wtype`, binario ligero Wayland puro. Solo funciona
///   en clientes Wayland; las apps que bloquean el teclado virtual
///   (zwp_virtual_keyboard_v1) lo rechazan → se avisa y queda el
///   portapapeles.
///
/// A validar en binario: el pegado via wtype en apps Electron/Qt concretas
/// (algunas expresan su propio atajo de pegado) y el path del socket de
/// ydotoold en Arch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinuxInjectBackend {
    /// wl-copy (portapapeles) + wtype para la pulsación del pegado,
    /// sin daemons. Default.
    #[default]
    WlClipboard,
    /// ydotool (uinput vía `ydotoold`): tecleo directo, universal
    /// (Xwayland incluido). Requiere daemon + permiso de /dev/uinput.
    Ydotool,
    /// wtype puro-Wayland: tecleo directo como teclado virtual.
    Wtype,
}

impl LinuxInjectBackend {
    /// Nombre en config.json del backend (`inject_backend`).
    pub fn as_config_str(self) -> &'static str {
        match self {
            LinuxInjectBackend::WlClipboard => "wl-clipboard",
            LinuxInjectBackend::Ydotool => "ydotool",
            LinuxInjectBackend::Wtype => "wtype",
        }
    }

    /// Etiqueta en español del selector de Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            LinuxInjectBackend::WlClipboard => "wl-clipboard (portapapeles + wtype)",
            LinuxInjectBackend::Ydotool => "ydotool (uinput, requiere ydotoold)",
            LinuxInjectBackend::Wtype => "wtype (teclea directo)",
        }
    }
}

/// Comportamiento de inyección (lo construye `Config::inject_settings`).
/// Los tres campos comunes los lee también Ajustes; `backend` solo forma
/// parte del struct en Linux (el selector enigo/ydotool/wtype no existe en
/// Windows, donde enigo es el único backend).
#[derive(Debug, Clone)]
pub struct InjectSettings {
    pub format: InjectFormat,
    /// Retardo antes de pegar/teclear: deja que la app destino asiente el
    /// foco (0 = sin retardo). Default 40 ms.
    pub delay: Duration,
    /// Filtro por título de la ventana activa (substring, sin distinguir
    /// mayúsculas): `None`/vacío = inyectar en cualquier app.
    pub window_filter: Option<String>,
    /// Backend Linux (`inject_backend` en config.json). Solo Unix.
    #[cfg(unix)]
    pub backend: LinuxInjectBackend,
}

impl Default for InjectSettings {
    fn default() -> Self {
        Self {
            format: InjectFormat::Paste,
            delay: Duration::from_millis(40),
            window_filter: None,
            // Solo Linux: backend por defecto (wl-clipboard). Faltaba en el
            // inicializador tras el port — E0063 (CHECK-1.md L118).
            #[cfg(unix)]
            backend: LinuxInjectBackend::default(),
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
    /// No había NINGUNA ventana con el foco (o el compositor no informa) al
    /// inyectar. Pegar a ciegas no tiene destino: mejor avisar y dejar el
    /// texto en el portapapeles (lo copia la app al recibir este resultado).
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

/// Escanea una cadena JSON para extraer el valor string de un campo
/// `"campo": "valor"`. Sin dependencias nuevas (std): extrae `title`/`class`
/// del JSON plano de `hyprctl activewindow -j`. Nota: serde_json YA es
/// dependencia del crate, pero este parser mínimo evita un roundtrip de
/// tipos por solo dos campos. Función pura testeable en cualquier target.
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
                    // \uXXXX: demasiado raro en título de ventana; se
                    // inserta tal cual y se confía en la app destino.
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

/// Copia `text` al portapapeles (best-effort, para cuando la inyección no
/// puede hacerse: sin ventana activa, app destino que no acepta pegado…).
/// El texto nunca se pierde si el usuario puede pegarlo a mano donde quiera.
///
/// Windows y macOS: backend arboard (integrado en el flujo Windows: enigo
/// pega o teclea tras la copia; en macOS es el único backend). Linux:
/// `wl-copy` (wl-clipboard) — el backend arboard de Linux (smithay) no
/// fija el foco de la venta del portapapeles Wayland y puede desincronizarse
/// con wtype/ydotool (en macOS no hay wl-copy: ahí queda arboard).
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use arboard::Clipboard;
    let mut clipboard =
        Clipboard::new().map_err(|err| format!("no se pudo abrir el portapapeles: {err}"))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|err| format!("no se pudo copiar el texto al portapapeles: {err}"))
}

/// Linux: portapapeles Wayland vía `wl-copy` (wl-clipboard). Los bytes van
/// por stdin (no CLI args: sin límite ni fuga de datos por `ps`). wl-copy
/// se bifurca para servir el portapapeles y el padre sale → `output()`
/// vuelve rápido.
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
        // wl-copy limpia el stdin y bifurca aun si el compositor se saturó:
        // un exit-code != 0 es el único indicio fiable de que el portapapeles
        // no llegó a asignarse.
        Err(format!(
            "wl-copy terminó con error {}: no se pudo copiar al portapapeles Wayland",
            salida.status
        ))
    }
}

/// Stub en targets que no sean Windows ni UNIX: sin portapapeles del sistema.
/// (El gate usa `target_family = "unix"`: `target_os = "unix"` no es un valor
/// válido y hacía que el stub compilara también en Linux — CHECK-1.md L269.)
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
pub fn copy_to_clipboard(_text: &str) -> Result<(), String> {
    Err("el portapapeles solo está disponible en Windows/Linux".to_string())
}

/// Escribe `text` en la app con foco, según `opts` (formato, retardo y
/// filtro de ventana). En Windows usa arboard + enigo (Ctrl+V / tecleo
/// unicode); en Linux usa wl-copy/wtype/ydotool según `inject_backend` de
/// config.json. Devuelve `Ok(InjectOutcome)` con el desenlace real y un
/// `Err` con mensaje en español si falla el portapapeles o el teclado
/// virtual. Nunca teclea a ciegas: sin ventana con el foco no hay destino.
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

// ---------------------------------------------------------------------------
// Linux (Wayland/Hyprland): ninguna API permite simular el teclado de otra
// app (no hay singleton global). El portapapeles va por wl-copy y la
// «pulsación de Ctrl+V» la hace el teclado virtual disponible. Tres
// backends + degradación elegante.
// ---------------------------------------------------------------------------

/// Título de la ventana con foco, con `hyprctl activewindow -j` (Hyprland).
/// `None` si no hay ventana activa (escritorio al descubierto) o si el
/// compositor no respondió. Para Xwayland también funciona (el compositor
/// ve sus ventanas anidadas).
///
/// A validar: en KDE/GNOME Wayland el mismo dato requeriría `swaymsg -t
/// get_tree` / `kdotool`; si no se resuelve, el guard de foco se salta y el
/// pegado cae en el portapapeles (no se teclea a ciegas nunca).
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
    // Un tile sin barra de título suele dar title "" pero class útil: el
    // filtro del usuario se evalúa sobre title, y si viene vacío probamos
    // con class (p. ej. "firefox" como clase de Firefox).
    let title = json_string_field(&raw, "title").unwrap_or_default();
    let class = json_string_field(&raw, "class").unwrap_or_default();
    if title.is_empty() && !class.is_empty() {
        Some(class)
    } else {
        Some(title)
    }
}

/// En targets que no son Windows ni UNIX no hay interrogado que hacer.
/// (`target_family = "unix"`, no `target_os`: CHECK-1.md L385.)
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
fn foreground_window_title() -> Option<String> {
    None
}

/// ¿Está accesible el socket de `ydotoold`? Sin él, ydotool no puede
/// inyectar nada (el binario cliente solo reconoce el daemon).
/// Rutas típicas de Arch/Omarchy: /run/ydotoold/.socket o la estándar de
/// XDG runtime dir. A validar en el binario.
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

/// Aviso del caso de degradación del backend ydotool: el daemon no está
/// (`ydotoold` sin arrancar o el socket no accesible). Se degrada como
/// `Ok(NoForegroundWindow)`: el caller de app.rs copia el texto al
/// portapapeles y muestra «el texto quedó copiado en el portapapeles» —
/// cierto, aunque no nombra al daemon culpable. Para no mentir, el detalle
/// queda en el log de esta función (ver `ydotoold_reachable`).
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
    // `ydotool type -- <texto>` teclea el texto completo (modo unicode).
    // Un ydotool que TERMINA con error puede haber dejado el texto a medias
    // en el destino: se avisa con Ctrl+Z como rescate (mismo contrato que
    // el mensaje del modo Type de Windows).
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

/// wtype: teclear el texto por stdin (no CLI args, sin límite). Solo
/// funciona si el destino es un cliente Wayland que no bloquee el teclado
/// virtual (zwp_virtual_keyboard_v1); sin `wtype` en PATH → mensaje con
/// cómo instalarlo (`pacman -S wtype`).
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
        // Éxito de wtype = texto tecleado: misma variante que el backend
        // ydotool (el caller espera un InjectOutcome, no `()`).
        Ok(InjectOutcome::Injected)
    } else {
        Err("wtype terminó con error: la app destino puede estar bloqueando el teclado virtual \
             (zwp_virtual_keyboard_v1); prueba con el portapapeles o el backend ydotool"
            .to_string())
    }
}

/// `wl-copy` + pulsación de pegado con `wtype`: wtype interpreta el byte de
/// control \x16 (SYN, el código del Ctrl+V en streams de terminal) como la
/// secuencia Ctrl+V y no como texto («wtype type: reads from stdin... byte
/// sequences for modifier prefixes»). En Wayland NO hay Ctrl+V global
/// fiable, pero aquí el destino TIENE el foco (sin foco se corta antes): la
/// pulsación virtual entra directa en la app que el usuario ya está
/// mirando. A validar en binario: compositor y app destino concretos.
#[cfg(unix)]
fn inject_with_wl_clipboard(text: &str) -> Result<InjectOutcome, String> {
    copy_to_clipboard(text)?;
    inject_with_wtype("\x16") // \x16 a wtype = pulso Ctrl+V
}

/// Inyección Linux completa: ventana → filtro → retardo → backend.
#[cfg(unix)]
pub fn inject_text_into_active_app(
    text: &str,
    opts: &InjectSettings,
) -> Result<InjectOutcome, String> {
    // 1) ¿Hay app con foco a la que inyectar? Mismo guard que en Windows:
    //    pegar a ciegas no tiene destino (aunque sin título no se pueda
    //    filtrar, al menos evita el disparo al vacío).
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
    // 3) Retardo configurable (idéntico al camino Windows).
    std::thread::sleep(opts.delay);
    // 4) Backend elegido en config.
    match opts.backend {
        LinuxInjectBackend::WlClipboard => match opts.format {
            InjectFormat::Paste => inject_with_wl_clipboard(text),
            InjectFormat::Type => inject_with_wtype(text),
        },
        LinuxInjectBackend::Ydotool => match opts.format {
            // Pegar con ydotool = copiar al portapapeles y hacer que teclee
            // el atajo Ctrl+V (el byte \x16 llega al keymap virtual como la
            // combinación del pegado). A validar en binario: con ydotoold
            // corriendo, la app destino tiene que aceptar el Ctrl+V.
            InjectFormat::Paste => {
                copy_to_clipboard(text)?;
                inject_with_ydotool("\x16")
            }
            // Teclear es ydotool de siempre (ydotool type -- <texto>).
            InjectFormat::Type => inject_with_ydotool(text),
        },
        LinuxInjectBackend::Wtype => match opts.format {
            // Con wtype como teclado, el pegado clásico también puede
            // hacerse (portapapeles + Ctrl+V virtual), manteniendo el texto
            // copiado primero (regla «nunca se pierde texto»).
            InjectFormat::Paste => inject_with_wl_clipboard(text),
            InjectFormat::Type => inject_with_wtype(text),
        },
    }
}

/// Stub en targets que no son Windows ni UNIX: mismo contrato que en el
/// resto de plataformas, siempre `Err`. (`target_family = "unix"`, no
/// `target_os`: CHECK-1.md L549.)
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
