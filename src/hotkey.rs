//! Atajo global de dictado (toggle), configurable: `shortcut` en config.json
//! (p.ej. "Ctrl+Shift+D", "Alt+Space", "Ctrl+Alt+F12"); default Ctrl+Shift+D.
//!
//! DÚOS BACK-END (port Linux t_7ac526a3):
//!
//! - **Windows (y el resto de plataformas del crate)**: el crate
//!   `global-hotkey` registra el atajo en el sistema (X11 grab en Linux con
//!   sesión X11). Los eventos llegan por canal:
//!   `GlobalHotKeyEvent::receiver()`. Igual que hacía el plugin de Tauri.
//!
//! - **Linux/Wayland (Hyprland)**: NO hay grab global — el modelo de
//!   seguridad de Wayland prohíbe capturar teclas de otras apps. Backend
//!   config-driven «hyprland»: la app crea un UNIX socket en
//!   `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock` y un bind externo de
//!   `hyprland.conf` le escribe un byte (ver docs/PORT-LINUX/HOTKEY-HYPR.md):
//!
//!     bind = $mainMod SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock
//!
//!   El socket escucha en su propio hilo y entrega eventos del MISMO tipo
//!   que Windows (`GlobalHotKeyEvent`, con campos públicos id/state) por el
//!   mismo canal crossbeam, de modo que `app.rs` / `ui/poll.rs` no distinguen
//!   backends. En Hyprland el ID se hace determinista con el parse del
//!   `shortcut` de config para que los ids de un mismo atajo coincidan entre
//!   plataformas (tests lo fijan).
//!
//! Si el valor de config es inválido o el backend falla (socket no
//! creable, atajo ya ocupado por otra app en Windows), se vuelve al default
//! / modo stub: los botones de la UI siguen funcionando, simplemente no hay
//! atajo global.

use crossbeam_channel::Receiver;
#[cfg(not(target_os = "linux"))]
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
#[cfg(not(target_os = "linux"))]
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent};

/// Atajo por defecto (también es el valor de `shortcut` ausente en config).
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+D";

/// Manager del atajo global. En Windows mantiene vivo el registro de
/// global-hotkey; en Linux/Wayland levanta el listener del UNIX socket de
/// Hyprland (`HotkeySocket`, modulo abajo).
pub struct HotkeyManager {
    // [Windows] Registro vivo: si se cae, el atajo se desregistra.
    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    manager: Option<GlobalHotKeyManager>,
    /// [Windows] Atajo parseado y registrado: su id filtra los eventos del
    /// canal y su etiqueta canónica alimenta la UI ("Atajo global: …").
    #[cfg(not(target_os = "linux"))]
    hotkey: HotKey,
    /// [Linux/Hyprland] Listener del UNIX socket. Vive en un hilo propio;
    /// se conserva para que no muera (drop del listener cierra el socket).
    #[cfg(target_os = "linux")]
    listener: Option<HotkeySocketListener>,
    /// Canal de eventos del backend activo (Windows: el global de
    /// global-hotkey; Linux: el canal propio del socket Hyprland).
    rx: Receiver<global_hotkey::GlobalHotKeyEvent>,
    /// Etiqueta canónica del atajo activo (p.ej. "Ctrl+Shift+D"): la UI la
    /// pinta en el placeholder y en Ajustes.
    label: String,
    /// Id determinista del atajo activo (mismo algoritmo en ambos
    /// backends — en Windows es el id del crate, en Linux el derivado del
    /// parse del shortcut via hash estable del label).
    id: u32,
}

/// Ruta del UNIX socket de Hyprland. La app la abre en modo "mkdir + bind";
/// el bind de hyprland.conf SOLO escribe bytes finitos ("toggle\n"). Se
/// documentada en docs/PORT-LINUX/HOTKEY-HYPR.md.
pub const HOTKEY_SOCKET_FILENAME: &str = "hotkey.sock";

/// Token escrito por el bind de Hyprland. Cualquier byte/linea no vacía
/// que no sea "off" cuenta como toggle (fault-tolerant).
pub const HOTKEY_MSG_TOGGLE: &str = "toggle";

/// Ruta del socket: `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock`.
/// Fallback: si XDG_RUNTIME_DIR no está (raro en Wayland), `~/.cache`.
pub fn hotkey_socket_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            crate::log!(
                "hotkey",
                "XDG_RUNTIME_DIR no seteado; usando ~/.cache/whispr-open"
            );
            dirs::cache_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join("whispr-open")
        });
    base.join("whispr-open").join(HOTKEY_SOCKET_FILENAME)
}

impl HotkeyManager {
    /// Activa el atajo `shortcut` en el backend del sistema. `Err` con
    /// mensaje en español si el formato no es válido o ya lo tiene otra
    /// app; el llamador degrada a [`stub`](Self::stub) en vez de abortar.
    #[cfg(not(target_os = "linux"))]
    pub fn register(shortcut: &str) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("no se pudo iniciar el gestor de atajos: {e}"))?;

        let hotkey = parse_shortcut(shortcut);
        manager
            .register(hotkey)
            .map_err(|e| format!("no se pudo registrar {}: {e}", hotkey_label(hotkey)))?;

        Ok(Self {
            manager: Some(manager),
            hotkey,
            rx: GlobalHotKeyEvent::receiver().clone(),
            label: hotkey_label(hotkey),
            id: hotkey.id(),
        })
    }

    /// Linux/Wayland: backend «hyprland». Activa el listener del UNIX
    /// socket. `Err` si no se puede crear el socket (permisos,
    /// XDG_RUNTIME_DIR sin permiso de escritura, etc.) — igual que el
    /// `register`
    /// de Windows, el caller degrada a stub.
    #[cfg(target_os = "linux")]
    pub fn register(shortcut: &str) -> Result<Self, String> {
        let listener = HotkeySocketListener::spawn(hotkey_id_from_label(&parse_shortcut(
            shortcut,
        )))
        .map_err(|e| {
            format!(
                "no se pudo abrir el socket de Hyprland ({}): {e}",
                hotkey_socket_path().display()
            )
        })?;
        let parsed = parse_shortcut(shortcut);
        let id = hotkey_id_from_label(&parsed);
        crate::log!(
            "hotkey",
            "backend hyprland activo: escuchando {} (atajo «{}»)",
            hotkey_socket_path().display(),
            parsed
        );
        Ok(Self {
            listener: Some(listener),
            rx: listener.rx.clone(),
            label: parsed,
            id,
        })
    }

    /// Stub para cuando el registro falla: siguen funcionando los botones
    /// de la UI, simplemente no hay atajo global. En Linux el stub TAMBIÉN
    /// deja de escuchar el socket (el bind del conf se quedaría apuntando
    /// a un socket muerto; ahí se degrada en «sin atajo» y el usuario lo
    /// ve en la UI).
    pub fn stub(shortcut: &str) -> Self {
        let parsed = parse_shortcut(shortcut);
        #[cfg(target_os = "linux")]
        {
            Self {
                listener: None,
                rx: crossbeam_channel::unbounded().1,
                label: parsed,
                id: hotkey_id_from_label(&parsed),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self {
                manager: None,
                hotkey: parse_shortcut(shortcut),
                rx: crossbeam_channel::unbounded().1,
                label: parsed,
                id: parse_shortcut(shortcut).id(),
            }
        }
    }

    /// Canal de eventos globales. La UI escucha, filtra por [`Self::id`]
    /// y hace toggle. En Windows es el canal global del crate; en Linux
    /// es el canal interno del listener del socket.
    pub fn listen(&self) -> Receiver<global_hotkey::GlobalHotKeyEvent> {
        self.rx.clone()
    }

    /// Id del atajo activo: solo los eventos con este id son de nuestro
    /// atajo (cualquier otro id del canal se ignora).
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Etiqueta canónica para la UI (p.ej. "Ctrl+Shift+D").
    pub fn label(&self) -> String {
        self.label.clone()
    }
}

/// Hash estable (FNV-1a 32-bit) de la etiqueta canónica del atajo. En
/// Linux el id de los eventos construidos por el socket es ESTE hash, de
/// modo que (a) es determinista entre ejecuciones (los tests de
/// app.rs/poll.rs comparan `ev.id == hotkey.id()`), y (b) coincide con el
/// mismo shortcut => mismo id en ambas plataformas (suficiente para que
/// una config compartida funcione igual en Windows y Linux).
fn hotkey_id_from_label(label: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5; // FNV offset basis
    for byte in label.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193); // FNV prime
    }
    hash
}

// ───────────────────────── Linux / Hyprland — listener UNIX socket ─────────────

#[cfg(target_os = "linux")]
mod hyprland {
    use super::*;
    use crossbeam_channel::Sender;

    /// Listener del UNIX socket de Hyprland: un hilo que hace `accept` en
    /// bucle y, por cada conexión, lee una línea ("toggle\n" u otra) y
    /// mete un `GlobalHotKeyEvent { id: hash_de_atajo, state: Pressed }`
    /// en el canal de la app.
    pub struct HotkeySocketListener {
        /// Canal de la app (el que se reparte via HotkeyManager::listen).
        pub rx: Receiver<global_hotkey::GlobalHotKeyEvent>,
        /// JoinHandle del hilo de accept; se conserva para garantizar la
        /// vida del hilo mientras el manager viva.
        #[allow(dead_code)]
        worker: std::thread::JoinHandle<()>,
    }

    impl HotkeySocketListener {
        /// Crea el socket (unlink previo si quedó de una ejecución anterior
        /// y bind a la ruta) y lanza el hilo de accept. Err = mensaje en
        /// español para el log + caller degrada a stub.
        ///
        /// `event_id`: id determinista del atajo parseado (hash del label,
        /// ver `hotkey_id_from_label`): los eventos que el hilo emite
        /// llevan ese id para que el Poll de la UI (que filtra por
        /// `hotkey.id()`) los reconozca.
        pub fn spawn(event_id: u32) -> Result<Self, String> {
            let path = hotkey_socket_path();
            // El directorio padre debe existir antes del bind; crearlo.
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("no se pudo crear {}: {e}", parent.display()))?;
            }
            // Un socket viejo de una ejecución anterior bloquearía el bind
            // con «Address already in use»: la app que lo tenía murió, así
            // que su socket está sin dueño. Sanear: unlink antes de bind.
            let _ = std::fs::remove_file(&path);
            let listener = std::os::unix::net::UnixListener::bind(&path)
                .map_err(|e| format!("no se pudo crear el socket {path:?}: {e}"))?;
            crate::log!("hotkey", "socket de hotkey listo en {}", path.display());

            // Permisos: el socket hereda la umask del usuario (022 típica →
            // 755). Todo proceso del MISMO usuario puede escribir «toggle»;
            // los de otras cuentas no (el directorio whispr-open es del
            // usuario). En Hyprland el write llega del bind de hyprland.conf,
            // lanzado por el MISMO usuario que la app.

            let (tx, rx) = crossbeam_channel::unbounded::<global_hotkey::GlobalHotKeyEvent>();
            let worker = std::thread::Builder::new()
                .name("whispr-hyprland-hotkey".to_string())
                .spawn(move || serve(listener, tx, event_id))
                .map_err(|e| format!("no se pudo lanzar el hilo del socket: {e}"))?;
            Ok(Self { rx, worker })
        }
        /// Ruta del socket para el log y la doc.
        pub fn path(&self) -> std::path::PathBuf {
            hotkey_socket_path()
        }
    }

    /// Bucle del hilo worker: accept → leer el stream completo → UN evento
    /// `Pressed` por cada línea recibida. `echo toggle > socket` = UN
    /// toggle por ejecución del bind (un connect por write; si el bind
    /// disparase muy rápido y el shell agrupara dos writes en el mismo
    /// connect, cada línea «toggle» es un toggle: fiable).
    ///
    /// Palabra reservada: «off» no togglea (documentado en HOTKEY-HYPR.md;
    /// sirve para probar/purgar el socket sin efecto). Cualquier otra
    /// línea NO vacía se trata como toggle (fault-tolerante con ecos con
    /// texto extra, mensajes con fecha, etc.) y queda en el log.
    fn serve(
        listener: std::os::unix::net::UnixListener,
        tx: Sender<global_hotkey::GlobalHotKeyEvent>,
        event_id: u32,
    ) {
        use std::io::Read;

        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    crate::log!("hotkey", "accept del socket hotkey falló: {e}");
                    continue;
                }
            };
            let mut buf = [0u8; 256];
            let mut data = Vec::new();
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break, // EOF: el cliente cerró (echo normal)
                    Ok(n) => data.extend_from_slice(&buf[..n]),
                    Err(e) => {
                        crate::log!("hotkey", "lectura del socket hotkey falló: {e}");
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&data);
            for line in text.lines() {
                let cmd = line.trim();
                if cmd.is_empty() {
                    continue;
                }
                if cmd == "off" {
                    crate::log!("hotkey", "socket recibió «off» (reservado; sin efecto)");
                    continue;
                }
                if cmd != HOTKEY_MSG_TOGGLE {
                    crate::log!(
                        "hotkey",
                        "socket recibió «{cmd}» (desconocido; tratado como toggle)"
                    );
                }
                // Un evento Pressed por línea. El Poll de la UI (ui/poll.rs)
                // hace toggle al ver UN; si llegan dos en el mismo batch de
                // 100 ms, el flag `toggled` colapsa a UNO solo: igual que
                // en Windows no se puede pulsar dos veces dentro del mismo
                // tick del Poll.
                let _ = tx.send(global_hotkey::GlobalHotKeyEvent {
                    id: event_id,
                    state: global_hotkey::HotKeyState::Pressed,
                });
            }
        }
        // Salida del bucle = el socket se cerró (drop del listener cuando
        // manager muere): limpiar el fichero para que la próxima
        // ejecución no herede un socket muerto.
        let _ = std::fs::remove_file(hotkey_socket_path());
    }
}

#[cfg(target_os = "linux")]
use hyprland::HotkeySocketListener;

#[cfg(target_os = "linux")]
fn parse_shortcut(shortcut: &str) -> String {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return DEFAULT_SHORTCUT.to_string();
    }
    // No se exige una sintaxis aquí: el label se normaliza (split por
    // '+', título por token) para que "ctrl+shift+d" y "Ctrl+Shift+D"
    // den el MISMO id determinista (importante para el hash).
    let mut parts: Vec<String> = trimmed
        .split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    // Orden canónico Ctrl+Shift+Alt+Super (igual que el backend Windows,
    // ver hotkey_label abajo) para que una config compartida dé el mismo
    // id en ambas plataformas.
    let order = ["Ctrl", "Shift", "Alt", "Super", "Win", "Meta"];
    parts.sort_by_key(|p| order.iter().position(|o| o.eq_ignore_ascii_case(p)).unwrap_or(order.len()));
    if parts.is_empty() {
        return DEFAULT_SHORTCUT.to_string();
    }
    parts.join("+")
}

#[cfg(not(target_os = "linux"))]
fn parse_shortcut(shortcut: &str) -> HotKey {
    let trimmed = shortcut.trim();
    match trimmed.parse::<HotKey>() {
        Ok(h) => h,
        Err(e) => {
            crate::log!(
                "hotkey",
                "atajo «{trimmed}» no válido ({e}); usando el default {DEFAULT_SHORTCUT}"
            );
            default_hotkey()
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn default_hotkey() -> HotKey {
    // Fuente de verdad: la constante de arriba (validada por tests). El
    // parse de una constante no puede fallar en la práctica, pero el
    // requisito de robustez veta pánicos en el arranque: si una
    // actualización del crate cambiara el formato aceptado, se construye
    // el MISMO atajo por API (infalible) en vez de tumbar el proceso.
    DEFAULT_SHORTCUT.parse::<HotKey>().unwrap_or_else(|_| {
        crate::log!(
            "hotkey",
            "el default «{DEFAULT_SHORTCUT}» dejó de parsear; se construye por API"
        );
        HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD)
    })
}

/// Etiqueta canónica Windows-style («Ctrl+Shift+D») a partir del atajo
/// parseado, independiente del orden/estilo que el usuario escribió en
/// config. [Windows/macOS; en Linux el parse devuelve ya un label
/// canónico String].
#[cfg(not(target_os = "linux"))]
fn hotkey_label(h: HotKey) -> String {
    let mut parts: Vec<String> = Vec::new();
    if h.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if h.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if h.mods.contains(Modifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if h.mods.contains(Modifiers::SUPER) {
        parts.push("Win".to_string());
    }
    parts.push(key_label(h.key));
    parts.join("+")
}

/// Nombre corto de tecla: `Code::Display` da "KeyD"/"Digit5"/"Space"/"F12".
#[cfg(not(target_os = "linux"))]
fn key_label(code: Code) -> String {
    let s = code.to_string();
    if let Some(rest) = s.strip_prefix("Key") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("Digit") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("Numpad") {
        return format!("Num{rest}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn default_es_ctrl_shift_d() {
        let h = default_hotkey();
        assert_eq!(h.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(h.key, Code::KeyD);
        assert_eq!(hotkey_label(h), "Ctrl+Shift+D");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn atajos_personalizados_se_parsean_y_etiquetan() {
        assert_eq!(hotkey_label(parse_shortcut("Alt+Space")), "Alt+Space");
        assert_eq!(hotkey_label(parse_shortcut("Ctrl+Alt+F12")), "Ctrl+Alt+F12");
        assert_eq!(hotkey_label(parse_shortcut("shift+KeyK")), "Shift+K");
        // Orden canónico fijo: Ctrl+Shift+Alt+Win, da igual el orden de entrada.
        assert_eq!(
            hotkey_label(parse_shortcut("Ctrl+Alt+Shift+Space")),
            "Ctrl+Shift+Alt+Space"
        );
        // Espacios alrededor no rompen el parseo.
        assert_eq!(
            hotkey_label(parse_shortcut("  Ctrl+Shift+D  ")),
            "Ctrl+Shift+D"
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn invalido_cae_al_default() {
        // Sin tecla principal, token vacío, tecla inexistente.
        assert_eq!(hotkey_label(parse_shortcut("Ctrl+Shift+")), "Ctrl+Shift+D");
        assert_eq!(hotkey_label(parse_shortcut("")), "Ctrl+Shift+D");
        assert_eq!(hotkey_label(parse_shortcut("zzz")), "Ctrl+Shift+D");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn id_determinista_por_atajo() {
        let a = parse_shortcut("Ctrl+Shift+D");
        let b = parse_shortcut("Ctrl+Shift+D");
        assert_eq!(a.id(), b.id());
        // Distinto atajo → distinto id: los eventos se filtran por él.
        assert_ne!(a.id(), parse_shortcut("Alt+Space").id());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn fallback_por_api_es_el_mismo_atajo_que_el_default_parseado() {
        // Requisito de robustez: default_hotkey() ya no puede pánico. Si el
        // parse de la constante llegara a romperse (cambio de crate), la
        // construcción por API debe producir EXACTAMENTE el mismo atajo
        // (misma etiqueta y mismo id) para que la UI y el filtrado de
        // eventos no cambien de comportamiento.
        let por_api = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
        let por_parse = DEFAULT_SHORTCUT.parse::<HotKey>().unwrap();
        assert_eq!(por_api.id(), por_parse.id());
        assert_eq!(hotkey_label(por_api), hotkey_label(por_parse));
    }

    #[test]
    fn label_del_manager_refleja_lo_registrado() {
        let stub = HotkeyManager::stub("ctrl+alt+d");
        assert_eq!(stub.label(), "Ctrl+Alt+D");
    }

    // ── Linux/Hyprland: id determinista por etiqueta ──────────────────────

    #[test]
    fn hyprland_el_id_es_determinista_y_distinto_por_atajo() {
        let a = hotkey_id_from_label("Ctrl+Shift+D");
        let b = hotkey_id_from_label("Ctrl+Shift+D");
        assert_eq!(a, b, "mismo atajo → mismo id");
        assert_ne!(a, hotkey_id_from_label("Alt+Space"), "distinto atajo → distinto id");
    }

    #[test]
    fn hyprland_stub_nunca_puede_emitir_eventos() {
        // El canal del stub se crea vacío y jamás se le escribe: el Poll
        // de la UI dará Err siempre (equivalente a «sin atajo global»).
        let stub = HotkeyManager::stub("ctrl+alt+d");
        assert!(stub.rx.try_recv().is_err());
        assert_eq!(stub.id(), hotkey_id_from_label("Ctrl+Alt+D"));
    }

    #[test]
    fn hyprland_socket_path_vive_bajo_xdg_runtime() {
        // En el entorno de tests XDG_RUNTIME_DIR suele existir (Wayland);
        // si no existe, el fallback es ~/.cache. En ambos casos la ruta
        // debe terminar en whispr-open/hotkey.sock.
        let path = hotkey_socket_path();
        assert!(path.ends_with(HOTKEY_SOCKET_FILENAME));
        assert!(path.to_string_lossy().contains("whispr-open"));
    }

    #[test]
    fn hyprland_parse_normaliza_orden_y_caso_y_da_ids_estables() {
        // La misma config escrita distinta debe dar el MISMO id (config
        // compartida Windows/Linux).
        assert_eq!(parse_shortcut("ctrl+shift+d"), parse_shortcut("Ctrl+Shift+D"));
        assert_eq!(parse_shortcut("Shift+D+Ctrl"), "Ctrl+Shift+D");
        let id1 = hotkey_id_from_label(&parse_shortcut("ctrl+shift+d"));
        let id2 = hotkey_id_from_label(&parse_shortcut("  Ctrl + Shift + D  "));
        assert_eq!(id1, id2);
    }

    #[test]
    fn hyprland_vacio_o_invalido_cae_al_default() {
        assert_eq!(parse_shortcut(""), DEFAULT_SHORTCUT);
        assert_eq!(parse_shortcut("   "), DEFAULT_SHORTCUT);
        // Un label con excepcional que no es modificador igual se
        // normaliza a mayúscula inicial (KeyD, etc).
        assert_eq!(parse_shortcut("ctrl+d"), "Ctrl+D");
    }
}
