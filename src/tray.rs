//! Bandeja del sistema (icono en el tray de Windows) con menú contextual:
//! «Mostrar/Ocultar ventana» y «Salir».
//!
//! Integración con Iced 0.13.1 / winit 0.30.13 — verificada leyendo el código
//! real de los crates en Cargo.lock (nunca de memoria):
//! - En Windows, `tray-icon` 0.24 crea una ventana oculta propia
//!   (`tray_icon_app`, vía CreateWindowExW) en el HILO LLAMANTE y no lanza
//!   hilos: los mensajes del shell (WM_TRAYICON, WM_COMMAND del menú) llegan
//!   a esa ventana y los reparte el bucle de eventos win32 de winit, que corre
//!   en el mismo hilo donde `iced_winit::program::run` crea la app. Por eso la
//!   bandeja se crea en `WhisprApp::new()` (hilo principal, antes de que
//!   arranque el bucle): los eventos se encolan y se entregan en cuanto el
//!   bucle corre.
//! - Los clics de menú se reciben por canal global (`MenuEvent::receiver`,
//!   canal crossbeam, relleno por muda 0.19 en su wndproc), exactamente el
//!   mismo patrón que el atajo global (`GlobalHotKeyEvent::receiver`): la app
//!   los drena en su Poll de 100 ms. No hace falta tocar el bucle de winit ni
//!   usar `EventLoopProxy`.
//! - El icono se genera en código (RGBA 32×32, círculo violeta #7C6CFF, el
//!   mismo acento que la burbuja): `Icon::from_rgba` en Windows convierte
//!   RGBA→BGRA y monta la AND-mask, sin premultiplicar; no necesita assets.
//!
//! PORT LINUX (t_7ac526a3, estado «pendiente backend de sistema»):
//! - `tray-icon` 0.24 soporta Linux SOLO vía AppIndicator/KSNI (docs.rs
//!   0.24.2: «Linux (gtk Only)»): exige GTK3 + libappindicator/libayatana +
//!   libxdo instaladas en el sistema Y un host de StatusNotifier (waybar con
//!   soporte appindicator) que Hyprland no trae de fábrica.
//! - Factor de fragilidad doble (deps GTK en build + host de tray en
//!   runtime): decisión honesta del port es degradar a stub en Linux sin
//!   compilar la dependencia (ver Cargo.toml, dependencia detrás de
//!   `cfg(not(target_os = "linux"))`), dejar la ventana + atajo + Salir en la
//!   UI como único camino, y dejar el backend real PENDIENTE. El stub cumple
//!   el MISMO contrato (constantes, `SystemTray::new()` que devuelve `Err`
//!   razonable, campos inoperantes) para que `app.rs` (fuera del reparto de
//!   este port) no tenga que llevar cfg.
//! - Cuando se quiera activar: instalar gtk3/libappindicator-gtk3/libxdo por
//!   pacman, reactivar la dependencia y des-cfg las piezas marcadas abajo.

#[cfg(not(target_os = "linux"))]
use crossbeam_channel::Receiver;
#[cfg(not(target_os = "linux"))]
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
#[cfg(not(target_os = "linux"))]
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Id estable del ítem «Mostrar/Ocultar ventana» (viaja en `MenuEvent.id`).
pub const MENU_TOGGLE_ID: &str = "toggle-main";
/// Id estable del ítem «Salir».
pub const MENU_QUIT_ID: &str = "quit";
/// Id del ítem de estado (solo lectura; refleja el arranque de la app).
pub const MENU_STATUS_ID: &str = "status";

/// Texto del ítem según el estado de la ventana principal.
pub const MENU_LABEL_HIDDEN: &str = "Mostrar ventana";
pub const MENU_LABEL_VISIBLE: &str = "Ocultar ventana";
/// Texto del ítem de estado según la fase de arranque (requisito UX
/// 09/09/2026: el "listo" de la app debe verse también en el tray).
pub const MENU_LABEL_LOADING: &str = "Estado: cargando…";
pub const MENU_LABEL_READY: &str = "Estado: listo";
pub const MENU_LABEL_READY_NO_LLM: &str = "Estado: listo · corrección IA desactivada";
pub const MENU_LABEL_STARTUP_ERROR: &str = "Estado: error de arranque";

/// Bandeja del sistema. Se mantiene viva mientras la app viva: si se suelta,
/// el icono desaparece del tray.
#[cfg(not(target_os = "linux"))]
pub struct SystemTray {
    // El icono no se usa directamente; se conserva para que no se destruya.
    _tray: TrayIcon,
    /// Ítem de estado (deshabilitado): refleja el arranque (cargando/listo).
    pub status_item: MenuItem,
    /// Ítem «Mostrar/Ocultar ventana», para cambiar su texto según el estado.
    pub toggle_item: MenuItem,
    /// Canal global de eventos del menú contextual (se drena en la app).
    pub events: Receiver<MenuEvent>,
}

/// Stub Linux: la bandeja real queda PENDIENTE (AppIndicator + host de
/// StatusNotifier). Cumple el contrato de Windows (los mismos campos
/// públicos existen para que `app.rs` compile sin cfg) pero, al no haber
/// icono, los ítems no van a ninguna parte y el canal de eventos está
/// siempre vacío (el Poll de app.rs drena 0 mensajes y no cuesta nada).
///
/// `SystemTray::new()` devuelve el stub y NO un `Err`: el arranque en
/// Hyprland debe declarar «sin bandeja, todo lo demás funciona» con un log,
/// no mostrar una fila de error permanente en Ajustes (esto ya lo decidía
/// así el patrón de «degradación avisada en vez de silent failure» del repo).
#[cfg(target_os = "linux")]
pub struct SystemTray {
    /// Ítem de estado (deshabilitado): refleja el arranque (cargando/listo).
    /// En el stub no existe un ítem real: es un marcador inoperante.
    pub status_item: TrayMenuItem,
    /// Ítem «Mostrar/Ocultar ventana», para cambiar su texto según el estado.
    pub toggle_item: TrayMenuItem,
    /// Canal global de eventos del menú contextual (siempre vacío en stub).
    pub events: crossbeam_channel::Receiver<TrayMenuEvent>,
}

#[cfg(not(target_os = "linux"))]
impl SystemTray {
    /// Crea el icono y el menú. Llámalo desde el hilo principal (el mismo
    /// donde corre el bucle de winit): ver doc del módulo.
    pub fn new() -> Result<Self, String> {
        let menu = Menu::new();
        let status_item = MenuItem::with_id(MENU_STATUS_ID, MENU_LABEL_LOADING, false, None);
        let toggle_item = MenuItem::with_id(MENU_TOGGLE_ID, MENU_LABEL_VISIBLE, true, None);
        let quit_item = MenuItem::with_id(MENU_QUIT_ID, "Salir", true, None);
        let separator = PredefinedMenuItem::separator();
        menu.append_items(&[
            &status_item,
            &separator,
            &toggle_item,
            &separator,
            &quit_item,
        ])
        .map_err(|e| format!("no se pudo construir el menú de bandeja: {e}"))?;

        let icon = circle_icon().map_err(|e| format!("icono de bandeja: {e}"))?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("Whispr Open — dictado local")
            // El clic izquierdo también abre el menú (además del derecho).
            .with_menu_on_left_click(true)
            .build()
            .map_err(|e| format!("no se pudo crear el icono de bandeja: {e}"))?;

        crate::log!(
            "tray",
            "icono de bandeja listo (menú: estado, mostrar/ocultar, salir)"
        );

        Ok(Self {
            _tray: tray,
            status_item,
            toggle_item,
            events: MenuEvent::receiver().clone(),
        })
    }
}

/// Stub Linux de `SystemTray::new()`. NO falla (ver doc del módulo) y loguea
/// el estado real: la pendiente es el backend del sistema, no la UI.
#[cfg(target_os = "linux")]
impl SystemTray {
    pub fn new() -> Result<Self, String> {
        crate::log!(
            "tray",
            "bandeja de sistema NO disponible en Hyprland/Wayland (backend \
             AppIndicator pendiente, docs/PORT-LINUX/AUDIO-RUNTIME.md): la app \
             funciona con ventana + atajo; «Salir» está en la propia interfaz"
        );
        Ok(SystemTray {
            status_item: TrayMenuItem,
            toggle_item: TrayMenuItem,
            events: crossbeam_channel::never(), // nunca entrega mensajes, barato
        })
    }
}

/// Alias compilables solo para mantener la API: en el stub los ítems no
/// existen. `set_text` es un no-op — el Poll de app.rs/set_tray_status lo
/// llama sin tener cómo saber que el tray es simulado, y la llamada debe
/// restar coste cero no compile-error.
#[cfg(target_os = "linux")]
pub struct TrayMenuItem;

#[cfg(target_os = "linux")]
impl TrayMenuItem {
    /// No-op en el stub (conserva la API de `menu::MenuItem::set_text`).
    pub fn set_text(&self, _text: impl Into<String>) {}
}

/// Marcador del evento de menú en el stub (el canal real está siempre vacío;
/// existe para que `tray.events.try_recv()` compile).
#[cfg(target_os = "linux")]
pub struct TrayMenuEvent {
    pub id: &'static str,
}

/// Icono 32×32 RGBA generado en código: círculo violeta #7C6CFF (acento de la
/// app) con borde suavizado de 1 px, sobre fondo transparente.
///
/// En Linux el stub NO lo usa (no hay icono que pintar), pero la función se
/// mantiene en ambos targets: es pura, testable y es la pieza que reutilizará
/// el backend real cuando se active (Icon::from_rgba la pinta igual).
#[cfg(not(target_os = "linux"))]
fn circle_icon() -> Result<Icon, String> {
    let rgba = circle_rgba();
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| format!("rgba no válido: {e}"))
}
#[cfg(target_os = "linux")]
fn circle_icon() -> Result<Vec<u8>, String> {
    // Sin icono nativo en Linux: devuelto tal cual el buffer (el backend
    // real futuro lo envolvería en Icon::from_rgba aquí en el único sitio).
    Ok(circle_rgba())
}

const SIZE: u32 = 32;

/// Buffer RGBA 32×32 del círculo (función pura, testeable sin plataforma).
fn circle_rgba() -> Vec<u8> {
    const ACCENT: [u8; 3] = [0x7C, 0x6C, 0xFF]; // theme::ACCENT

    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = SIZE as f32 / 2.0 - 1.5;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            // Antialiasing: transición lineal de 1 px en el borde del círculo.
            let alpha = if distance <= radius - 0.5 {
                255.0
            } else if distance >= radius + 0.5 {
                0.0
            } else {
                (radius + 0.5 - distance) * 255.0
            };
            rgba.extend_from_slice(&[ACCENT[0], ACCENT[1], ACCENT[2], alpha as u8]);
        }
    }

    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_rgba_is_32x32_with_filled_center() {
        let rgba = circle_rgba();
        assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
        // Centro: violeta opaco (#7C6CFF alpha=255 en el píxel (16,16)).
        let center_idx = ((16 * SIZE + 16) * 4) as usize;
        assert_eq!(&rgba[center_idx..center_idx + 3], &[0x7C, 0x6C, 0xFF]);
        assert_eq!(rgba[center_idx + 3], 255);
        // Esquina: transparente (fuera del círculo).
        let corner_idx = 0usize;
        assert_eq!(rgba[corner_idx + 3], 0);
    }

    #[test]
    fn menu_ids_match() {
        assert_eq!(MENU_TOGGLE_ID, "toggle-main");
        assert_eq!(MENU_QUIT_ID, "quit");
        assert_eq!(MENU_STATUS_ID, "status");
        assert_ne!(MENU_TOGGLE_ID, MENU_QUIT_ID);
        assert_ne!(MENU_STATUS_ID, MENU_TOGGLE_ID);
        assert_ne!(MENU_STATUS_ID, MENU_QUIT_ID);
        // Los labels de arranque son todos distintos (el tray los conmuta).
        assert_ne!(MENU_LABEL_LOADING, MENU_LABEL_READY);
        assert_ne!(MENU_LABEL_READY, MENU_LABEL_READY_NO_LLM);
        assert_ne!(MENU_LABEL_LOADING, MENU_LABEL_READY_NO_LLM);
    }

    // Test solo Windows/macOS: en Linux el stub no usa Icon::from_rgba
    // (crítica del port: tray-icon no se compila ahí). El test original
    // «circle_icon_builds» queda cubierto en el target Windows; en Linux
    // la misma lógica la pinta el stub. Ver docs PORT-LINUX.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn circle_icon_builds() {
        // from_rgba valida las dimensiones contra el buffer (32×32).
        let _ = circle_icon().expect("el icono debe generarse sin error");
    }
}
