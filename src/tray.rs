//! Icono de la bandeja del sistema (el área de iconos de Windows) con menú:
//! «Mostrar/Ocultar ventana» y «Salir».
//!
//! Cómo encaja con Iced 0.13.1 / winit 0.30.13 — comprobado leyendo el código
//! de los crates que fija Cargo.lock, no de memoria:
//! - En Windows, `tray-icon` 0.24 crea su propia ventana oculta
//!   (`tray_icon_app`, con CreateWindowExW) en el HILO QUE LA LLAMA y no abre
//!   hilos: los mensajes del sistema (WM_TRAYICON, WM_COMMAND del menú) llegan
//!   a esa ventana y los reparte el bucle de eventos de winit, que corre en el
//!   mismo hilo donde `iced_winit::program::run` crea la app. Por eso la
//!   bandeja se crea en `WhisprApp::new()` (hilo principal, antes de que
//!   arranque el bucle): los mensajes se apuntan y se entregan en cuanto el
//!   bucle empieza.
//! - Los clics del menú llegan por un canal general (`MenuEvent::receiver`,
//!   rellenado por muda 0.19), igual que el atajo global
//!   (`GlobalHotKeyEvent::receiver`): la app los recoge en su `Poll` de 100 ms.
//!   No hay que tocar el bucle de winit ni usar `EventLoopProxy`.
//! - El icono se dibuja en código (RGBA 32×32, círculo violeta #7C6CFF, el
//!   mismo acento que la burbuja): `Icon::from_rgba` en Windows pasa de RGBA a
//!   BGRA y monta la máscara; no hacen falta ficheros.
//!
//! PORT A LINUX (t_7ac526a3, estado «pendiente de la bandeja del sistema»):
//! - `tray-icon` 0.24 solo soporta Linux con AppIndicator/KSNI (docs.rs
//!   0.24.2: «Linux (gtk Only)»): pide GTK3 + libappindicator/libayatana +
//!   libxdo instalados Y un programa que haga de bandeja (waybar con
//!   appindicator), que Hyprland no trae de fábrica.
//! - Son dos puntos frágiles (dependencias de GTK al compilar y un programa
//!   de bandeja al ejecutar): el port decide, con honestidad, quedarse en un
//!   modo sin bandeja en Linux sin compilar la dependencia (ver Cargo.toml,
//!   detrás de `cfg(not(target_os = "linux"))`), dejar la ventana + el atajo +
//!   «Salir» en la interfaz como único camino y dejar la bandeja real
//!   PENDIENTE. El modo sin bandeja cumple lo mismo (las constantes,
//!   `SystemTray::new()` que devuelve `Err` con sentido, campos que no hacen
//!   nada) para que `app.rs` no tenga que llevar cfg.
//! - Para activarla algún día: instalar gtk3/libappindicator-gtk3/libxdo con
//!   pacman, volver a poner la dependencia y quitar los cfg de las piezas marcadas abajo.

#[cfg(not(target_os = "linux"))]
use crossbeam_channel::Receiver;
#[cfg(not(target_os = "linux"))]
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
#[cfg(not(target_os = "linux"))]
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Id fijo del ítem «Mostrar/Ocultar ventana» (viaja en `MenuEvent.id`).
pub const MENU_TOGGLE_ID: &str = "toggle-main";
/// Id fijo del ítem «Salir».
pub const MENU_QUIT_ID: &str = "quit";
/// Id del ítem de estado (solo lectura; enseña cómo va el arranque).
pub const MENU_STATUS_ID: &str = "status";

/// Texto del ítem según el estado de la ventana principal.
pub const MENU_LABEL_HIDDEN: &str = "Mostrar ventana";
pub const MENU_LABEL_VISIBLE: &str = "Ocultar ventana";
/// Texto del ítem de estado según la fase de arranque (petición de UX del
/// 09/09/2026: el «listo» de la app también se tiene que ver en la bandeja).
pub const MENU_LABEL_LOADING: &str = "Estado: cargando…";
pub const MENU_LABEL_READY: &str = "Estado: listo";
pub const MENU_LABEL_READY_NO_LLM: &str = "Estado: listo · corrección IA desactivada";
pub const MENU_LABEL_STARTUP_ERROR: &str = "Estado: error de arranque";

/// La bandeja. Tiene que vivir lo mismo que la app: si se suelta, el icono
/// desaparece del área de iconos.
#[cfg(not(target_os = "linux"))]
pub struct SystemTray {
    // El icono no se usa directamente; se guarda para que no se destruya.
    _tray: TrayIcon,
    /// Ítem de estado (desactivado): enseña el arranque (cargando/listo).
    pub status_item: MenuItem,
    /// Ítem «Mostrar/Ocultar ventana», para cambiar su texto según el estado.
    pub toggle_item: MenuItem,
    /// Canal general de eventos del menú (la app los recoge).
    pub events: Receiver<MenuEvent>,
}

/// Modo sin bandeja en Linux: la bandeja de verdad queda PENDIENTE
/// (AppIndicator + un programa de bandeja). Cumple lo mismo que en Windows
/// (existen los mismos campos públicos para que `app.rs` compile sin cfg),
/// pero, al no haber icono, los ítems no van a ningún sitio y el canal de
/// eventos está siempre vacío (el `Poll` de app.rs no recibe nada y no cuesta
/// nada).
///
/// `SystemTray::new()` devuelve este modo y NO un `Err`: al arrancar en
/// Hyprland hay que decir «sin bandeja, lo demás funciona» con un log, no
/// enseñar una fila de error permanente en Ajustes (es la misma idea de
/// «degradar avisando, no fallar en silencio» que usa el repo).
#[cfg(target_os = "linux")]
pub struct SystemTray {
    /// Ítem de estado (desactivado): enseña el arranque (cargando/listo).
    /// Aquí no hay ítem de verdad: es un hueco que no hace nada.
    pub status_item: TrayMenuItem,
    /// Ítem «Mostrar/Ocultar ventana», para cambiar su texto según el estado.
    pub toggle_item: TrayMenuItem,
    /// Canal general de eventos del menú (siempre vacío aquí).
    pub events: crossbeam_channel::Receiver<TrayMenuEvent>,
}

#[cfg(not(target_os = "linux"))]
impl SystemTray {
    /// Crea el icono y el menú. Llámalo desde el hilo principal (el mismo del
    /// bucle de winit): ver la explicación del módulo.
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
            .with_tooltip("Silb — dictado local")
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

/// Modo sin bandeja de `SystemTray::new()` en Linux. NO falla (ver el módulo)
/// y deja en el log cómo está el tema: lo pendiente es la bandeja del sistema,
/// no la interfaz.
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

/// Nombres que existen solo para que la API siga igual: aquí no hay ítems de
/// verdad. `set_text` no hace nada — el `Poll` de app.rs/set_tray_status lo
/// llama sin saber que la bandeja es simulada, y la llamada tiene que costar
/// cero y compilar.
#[cfg(target_os = "linux")]
pub struct TrayMenuItem;

#[cfg(target_os = "linux")]
impl TrayMenuItem {
    /// No hace nada aquí (mantiene la forma de `menu::MenuItem::set_text`).
    pub fn set_text(&self, _text: impl Into<String>) {}
}

/// Hueco del evento de menú en el modo sin bandeja (el canal real está
/// siempre vacío; existe para que `tray.events.try_recv()` compile).
#[cfg(target_os = "linux")]
pub struct TrayMenuEvent {
    pub id: &'static str,
}

/// Icono 32×32 RGBA dibujado en código: círculo violeta #7C6CFF (el acento de
/// la app) con el borde suavizado 1 px, sobre fondo transparente.
///
/// En Linux el modo sin bandeja no lo usa (no hay icono que pintar), pero la
/// función se queda en los dos sistemas: no depende de nada, se puede probar y
/// es la pieza que reaprovechará la bandeja real cuando se active
/// (Icon::from_rgba lo pinta igual).
#[cfg(not(target_os = "linux"))]
fn circle_icon() -> Result<Icon, String> {
    let rgba = circle_rgba();
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| format!("rgba no válido: {e}"))
}
#[cfg(target_os = "linux")]
fn circle_icon() -> Result<Vec<u8>, String> {
    // En Linux no hay icono del sistema: se devuelve el búfer tal cual (la
    // bandeja real, cuando llegue, lo envolvería en Icon::from_rgba aquí).
    Ok(circle_rgba())
}

const SIZE: u32 = 32;

/// Búfer RGBA 32×32 del círculo (función suelta, se puede probar sin plataforma).
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
            // Suavizado: el borde del círculo pasa de un color a otro en 1 px.
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
        // Esquina: transparente (queda fuera del círculo).
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
        // Los textos del arranque son todos distintos (la bandeja los cambia).
        assert_ne!(MENU_LABEL_LOADING, MENU_LABEL_READY);
        assert_ne!(MENU_LABEL_READY, MENU_LABEL_READY_NO_LLM);
        assert_ne!(MENU_LABEL_LOADING, MENU_LABEL_READY_NO_LLM);
    }

    // Test solo para Windows/macOS: en Linux el modo sin bandeja no usa
    // Icon::from_rgba (tray-icon no se compila ahí). El test original
    // «circle_icon_builds» queda cubierto en Windows; en Linux la misma cuenta
    // la hace el modo sin bandeja. Ver docs PORT-LINUX.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn circle_icon_builds() {
        // from_rgba comprueba el tamaño contra el búfer (32×32).
        let _ = circle_icon().expect("el icono debe generarse sin error");
    }
}
