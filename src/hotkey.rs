//! Atajo global del dictado (se pulsa y suelta), configurable: `shortcut` en
//! config.json (por ejemplo "Ctrl+Shift+D", "Alt+Space", "Ctrl+Alt+F12"); por
//! defecto Ctrl+Shift+D.
//!
//! Dos formas de hacerlo (port a Linux t_7ac526a3):
//!
//! - **Windows (y el resto de sistemas que cubre el crate)**: el crate
//!   `global-hotkey` registra el atajo en el sistema (en Linux con sesión
//!   X11 lo coge X11). Los eventos llegan por un canal:
//!   `GlobalHotKeyEvent::receiver()`. Igual que hacía el plugin de Tauri.
//!
//! - **Linux/Wayland (Hyprland)**: NO se pueden capturar teclas de otras
//!   apps (Wayland lo prohíbe por seguridad). Se usa una vía distinta,
//!   «hyprland»: la app abre un socket local en
//!   `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock` y un atajo de
//!   `hyprland.conf` le escribe un byte (ver docs/PORT-LINUX/HOTKEY-HYPR.md):
//!
//!     bind = $mainMod SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock
//!
//!   El socket se escucha en su propio hilo y entrega eventos del MISMO tipo
//!   que en Windows (`GlobalHotKeyEvent`, con los campos públicos id/state)
//!   por el mismo canal, de modo que `app.rs` y `ui/poll.rs` no notan la
//!   diferencia. En Hyprland el id sale del `shortcut` del config para que el
//!   mismo atajo tenga el mismo id en todas las plataformas (hay tests).
//!
//! Si el valor del config no vale o la vía falla (no se puede crear el
//! socket, el atajo ya lo tiene otra app en Windows), se vuelve al de por
//! defecto o a un modo sin atajo: los botones de la interfaz siguen
//! funcionando, solo que no hay atajo global.

use crossbeam_channel::Receiver;
#[cfg(not(target_os = "linux"))]
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
#[cfg(not(target_os = "linux"))]
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent};

/// Atajo de por defecto (también se usa si `shortcut` falta en el config).
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+D";

/// Encargado del atajo global. En Windows mantiene vivo el registro de
/// global-hotkey; en Linux/Wayland levanta el que escucha el socket de
/// Hyprland (`HotkeySocket`, más abajo).
pub struct HotkeyManager {
    // [Windows] El registro tiene que seguir vivo: si se suelta, el atajo se pierde.
    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    manager: Option<GlobalHotKeyManager>,
    /// [Windows] Atajo leído y registrado: su id sirve para filtrar los eventos
    /// del canal y su etiqueta se enseña en la interfaz ("Atajo global: …").
    #[cfg(not(target_os = "linux"))]
    hotkey: HotKey,
    /// [Linux/Hyprland] El que escucha el socket. Vive en un hilo propio; se
    /// guarda para que no se muera (si se suelta, se cierra el socket).
    #[cfg(target_os = "linux")]
    listener: Option<HotkeySocketListener>,
    /// Canal de eventos de la vía activa (Windows: el general de
    /// global-hotkey; Linux: el del socket de Hyprland).
    rx: Receiver<global_hotkey::GlobalHotKeyEvent>,
    /// Etiqueta del atajo activo (por ejemplo "Ctrl+Shift+D"): la interfaz la
    /// enseña en el hueco del atajo y en Ajustes.
    label: String,
    /// Id del atajo activo, igual en las dos vías (en Windows es el que da el
    /// crate; en Linux sale del `shortcut` con un hash estable de la etiqueta).
    id: u32,
}

/// Ruta del socket de Hyprland. La app crea la carpeta y abre el socket; el
/// atajo de hyprland.conf solo escribe bytes sueltos ("toggle\n"). Está
/// explicado en docs/PORT-LINUX/HOTKEY-HYPR.md.
pub const HOTKEY_SOCKET_FILENAME: &str = "hotkey.sock";

/// Lo que escribe el atajo de Hyprland. Cualquier línea no vacía que no sea
/// "off" cuenta como pulsación (así no se rompe con textos raros).
pub const HOTKEY_MSG_TOGGLE: &str = "toggle";

/// Ruta del socket: `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock`.
/// Si no está XDG_RUNTIME_DIR (raro en Wayland), se usa `~/.cache`.
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
    /// Activa el atajo `shortcut` en el sistema. `Err` con mensaje en español
    /// si el formato no vale o ya lo tiene otra app; quien llama se queda en
    /// [`stub`](Self::stub) en vez de abortar.
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

    /// Linux/Wayland: vía «hyprland». Pone en marcha el que escucha el socket.
    /// `Err` si no se puede crear el socket (permisos, XDG_RUNTIME_DIR sin
    /// permiso de escritura, etc.); igual que `register` en Windows, quien
    /// llama se queda en el modo sin atajo.
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
        // E0382: se copia el canal ANTES de mover `listener` al constructor
        // (se evalúa en orden y `listener: Some(listener)` se queda el valor
        // antes de que `rx:` pueda pedirlo prestado).
        let rx = listener.rx.clone();
        Ok(Self {
            listener: Some(listener),
            rx,
            label: parsed,
            id,
        })
    }

    /// Modo sin atajo, para cuando el registro falla: los botones de la
    /// interfaz siguen funcionando, solo que no hay atajo global. En Linux
    /// además deja de escuchar el socket (el atajo del conf apuntaría a un
    /// socket muerto; ahí se queda en «sin atajo» y el usuario lo ve en la
    /// interfaz).
    pub fn stub(shortcut: &str) -> Self {
        #[cfg(target_os = "linux")]
        let parsed = parse_shortcut(shortcut);
        #[cfg(target_os = "linux")]
        {
            Self {
                listener: None,
                rx: crossbeam_channel::unbounded().1,
                // E0382: se copia por el orden en que se evalúa el constructor —
                // `label` se queda `parsed` antes de que `id` lo pida prestado
                // (el compilador sugiere `parsed.clone()`).
                label: parsed.clone(),
                id: hotkey_id_from_label(&parsed),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let hotkey = parse_shortcut(shortcut);
            Self {
                manager: None,
                hotkey,
                rx: GlobalHotKeyEvent::receiver().clone(),
                label: hotkey_label(hotkey),
                id: hotkey.id(),
            }
        }
    }

    /// Canal de eventos globales. La interfaz escucha, filtra por [`Self::id`]
    /// y cambia el estado. En Windows es el canal general del crate; en Linux
    /// es el de dentro del que escucha el socket.
    pub fn listen(&self) -> Receiver<global_hotkey::GlobalHotKeyEvent> {
        self.rx.clone()
    }

    /// Id del atajo activo: solo los eventos con este id son de nuestro atajo
    /// (cualquier otro id del canal se ignora).
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Etiqueta para la interfaz (por ejemplo "Ctrl+Shift+D").
    pub fn label(&self) -> String {
        self.label.clone()
    }
}

/// Hash estable (FNV-1a de 32 bits) de la etiqueta del atajo. En Linux el id
/// de los eventos que crea el socket es ESTE hash, de modo que (a) sale igual
/// en cada arranque (los tests de app.rs/poll.rs comparan
/// `ev.id == hotkey.id()`) y (b) el mismo atajo da el mismo id en las dos
/// plataformas (así un config compartido funciona igual en Windows y Linux).
fn hotkey_id_from_label(label: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5; // FNV offset basis
    for byte in label.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193); // FNV prime
    }
    hash
}

// ───────────────────────── Linux / Hyprland — socket local ──────────────────────

#[cfg(target_os = "linux")]
mod hyprland {
    use super::*;
    use crossbeam_channel::Sender;

    /// Escucha el socket de Hyprland: un hilo que acepta conexiones en bucle
    /// y, por cada una, lee una línea ("toggle\n" u otra) y mete un
    /// `GlobalHotKeyEvent { id: hash_de_atajo, state: Pressed }` en el canal
    /// de la app.
    pub struct HotkeySocketListener {
        /// Canal de la app (el que se reparte con HotkeyManager::listen).
        pub rx: Receiver<global_hotkey::GlobalHotKeyEvent>,
        /// Hilo que acepta conexiones; se guarda para que siga vivo mientras
        /// viva el encargado.
        #[allow(dead_code)]
        worker: std::thread::JoinHandle<()>,
    }

    impl HotkeySocketListener {
        /// Crea el socket (borra antes el que haya quedado de otra ejecución
        /// y lo abre en la ruta) y lanza el hilo que acepta conexiones. `Err` =
        /// mensaje en español para el log; quien llama se queda en el modo sin
        /// atajo.
        ///
        /// `event_id`: id del atajo ya leído (hash de la etiqueta, ver
        /// `hotkey_id_from_label`). Los eventos que emite el hilo llevan ese id
        /// para que el `Poll` de la interfaz (que filtra por `hotkey.id()`) los
        /// reconozca.
        pub fn spawn(event_id: u32) -> Result<Self, String> {
            let path = hotkey_socket_path();
            // La carpeta tiene que existir antes de abrir el socket: se crea.
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("no se pudo crear {}: {e}", parent.display()))?;
            }
            // Un socket viejo de otra ejecución impediría abrirlo («Address
            // already in use»): la app que lo tenía ya no está, así que está
            // sin dueño. Se limpia: se borra antes de abrir.
            let _ = std::fs::remove_file(&path);
            let listener = std::os::unix::net::UnixListener::bind(&path)
                .map_err(|e| format!("no se pudo crear el socket {path:?}: {e}"))?;
            crate::log!("hotkey", "socket de hotkey listo en {}", path.display());

            // Permisos: el socket hereda los del usuario (022 típico → 755).
            // Cualquier proceso del MISMO usuario puede escribir «toggle»; los
            // de otras cuentas no (la carpeta whispr-open es del usuario). En
            // Hyprland el que escribe es el atajo de hyprland.conf, lanzado por
            // el mismo usuario que la app.

            let (tx, rx) = crossbeam_channel::unbounded::<global_hotkey::GlobalHotKeyEvent>();
            let worker = std::thread::Builder::new()
                .name("whispr-hyprland-hotkey".to_string())
                .spawn(move || serve(listener, tx, event_id))
                .map_err(|e| format!("no se pudo lanzar el hilo del socket: {e}"))?;
            Ok(Self { rx, worker })
        }
        /// Ruta del socket, para el log y la documentación.
        pub fn path(&self) -> std::path::PathBuf {
            hotkey_socket_path()
        }
    }

    /// Bucle del hilo: aceptar, leer todo lo que llega y sacar UN evento
    /// `Pressed` por cada línea. `echo toggle > socket` = una pulsación por
    /// cada vez que se lanza el atajo (una conexión por escritura; si el shell
    /// juntara dos escrituras en la misma conexión, cada línea «toggle» cuenta
    /// igual: fiable).
    ///
    /// Palabra reservada: «off» no cambia nada (está en HOTKEY-HYPR.md; sirve
    /// para probar o vaciar el socket sin efecto). Cualquier otra línea no
    /// vacía cuenta como pulsación (aguanta textos de más, mensajes con fecha,
    /// etc.) y queda en el log.
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
                // Un evento Pressed por línea. El `Poll` de la interfaz
                // (ui/poll.rs) cambia el estado al ver UNO; si llegan dos en la
                // misma tanda de 100 ms, la marca `toggled` los junta en uno
                // solo: igual que en Windows no se puede pulsar dos veces
                // dentro de la misma vuelta del `Poll`.
                let _ = tx.send(global_hotkey::GlobalHotKeyEvent {
                    id: event_id,
                    state: global_hotkey::HotKeyState::Pressed,
                });
            }
        }
        // Si el bucle termina, es que se cerró el socket (se soltó el que lo
        // escuchaba cuando muere el encargado): se limpia el fichero para que
        // la próxima ejecución no herede un socket muerto.
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
    // Aquí no se exige una sintaxis: la etiqueta se ordena (se corta por '+'
    // y se pone cada trozo con mayúscula inicial) para que "ctrl+shift+d" y
    // "Ctrl+Shift+D" den el MISMO id (importante para el hash).
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
    // Orden fijo Ctrl+Shift+Alt+Super (igual que en Windows, ver
    // hotkey_label más abajo) para que un config compartido dé el mismo id en
    // las dos plataformas.
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
    // La referencia es la constante de arriba (hay tests). Leer una constante
    // no puede fallar en la práctica, pero no se quiere un pánico al arrancar:
    // si una actualización del crate cambiara el formato aceptado, se construye
    // el MISMO atajo con la API (que no falla) en vez de tumbar el proceso.
    DEFAULT_SHORTCUT.parse::<HotKey>().unwrap_or_else(|_| {
        crate::log!(
            "hotkey",
            "el default «{DEFAULT_SHORTCUT}» dejó de parsear; se construye por API"
        );
        HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD)
    })
}

/// Etiqueta al estilo de Windows («Ctrl+Shift+D») a partir del atajo ya
/// leído, sin importar el orden ni el estilo con que el usuario lo escribió
/// en el config. [Windows/macOS; en Linux ya sale la etiqueta hecha].
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
        // Orden fijo: Ctrl+Shift+Alt+Win, dé igual cómo se escriba.
        assert_eq!(
            hotkey_label(parse_shortcut("Ctrl+Alt+Shift+Space")),
            "Ctrl+Shift+Alt+Space"
        );
        // Los espacios de alrededor no rompen la lectura.
        assert_eq!(
            hotkey_label(parse_shortcut("  Ctrl+Shift+D  ")),
            "Ctrl+Shift+D"
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn invalido_cae_al_default() {
        // Sin tecla principal, trozo vacío o tecla que no existe.
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
        // Otro atajo → otro id: los eventos se filtran por él.
        assert_ne!(a.id(), parse_shortcut("Alt+Space").id());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn fallback_por_api_es_el_mismo_atajo_que_el_default_parseado() {
        // El atajo por defecto ya no puede dar pánico. Si la lectura de la
        // constante se rompiera (cambio de crate), construirlo con la API tiene
        // que dar EXACTAMENTE el mismo atajo (misma etiqueta y mismo id) para
        // que la interfaz y el filtrado de eventos no cambien.
        let por_api = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
        let por_parse = DEFAULT_SHORTCUT.parse::<HotKey>().unwrap();
        assert_eq!(por_api.id(), por_parse.id());
        assert_eq!(hotkey_label(por_api), hotkey_label(por_parse));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn label_del_manager_refleja_lo_registrado() {
        let stub = HotkeyManager::stub("ctrl+alt+d");
        assert_eq!(stub.label(), "Ctrl+Alt+D");
    }

    // ── Linux/Hyprland: id fijo por etiqueta ────────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn hyprland_el_id_es_determinista_y_distinto_por_atajo() {
        let a = hotkey_id_from_label("Ctrl+Shift+D");
        let b = hotkey_id_from_label("Ctrl+Shift+D");
        assert_eq!(a, b, "mismo atajo → mismo id");
        assert_ne!(a, hotkey_id_from_label("Alt+Space"), "distinto atajo → distinto id");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hyprland_stub_nunca_puede_emitir_eventos() {
        // El canal del modo sin atajo se crea vacío y nunca se escribe: el
        // `Poll` de la interfaz dará `Err` siempre (como «sin atajo global»).
        let stub = HotkeyManager::stub("ctrl+alt+d");
        assert!(stub.rx.try_recv().is_err());
        assert_eq!(stub.id(), hotkey_id_from_label("Ctrl+Alt+D"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hyprland_socket_path_vive_bajo_xdg_runtime() {
        // En el entorno de tests XDG_RUNTIME_DIR suele existir (Wayland); si
        // no, se usa ~/.cache. En los dos casos la ruta tiene que terminar en
        // whispr-open/hotkey.sock.
        let path = hotkey_socket_path();
        assert!(path.ends_with(HOTKEY_SOCKET_FILENAME));
        assert!(path.to_string_lossy().contains("whispr-open"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hyprland_parse_normaliza_orden_y_caso_y_da_ids_estables() {
        // El mismo config escrito de otra forma tiene que dar el MISMO id
        // (config compartida Windows/Linux).
        assert_eq!(parse_shortcut("ctrl+shift+d"), parse_shortcut("Ctrl+Shift+D"));
        assert_eq!(parse_shortcut("Shift+D+Ctrl"), "Ctrl+Shift+D");
        let id1 = hotkey_id_from_label(&parse_shortcut("ctrl+shift+d"));
        let id2 = hotkey_id_from_label(&parse_shortcut("  Ctrl + Shift + D  "));
        assert_eq!(id1, id2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hyprland_vacio_o_invalido_cae_al_default() {
        assert_eq!(parse_shortcut(""), DEFAULT_SHORTCUT);
        assert_eq!(parse_shortcut("   "), DEFAULT_SHORTCUT);
        // Una palabra rara que no es modificador también se pone con mayúscula
        // inicial (KeyD, etc.).
        assert_eq!(parse_shortcut("ctrl+d"), "Ctrl+D");
    }
}
