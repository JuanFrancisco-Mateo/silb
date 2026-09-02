//! Burbuja flotante de dictado (overlay siempre-encima).
//!
//! Segunda ventana Iced del mismo proceso (multi-window de iced 0.13 vía
//! `iced_winit::Program`): pequeña, sin decoración, transparente,
//! always-on-top (`window::Level::AlwaysOnTop`) y circular (acento violeta
//! #7C6CFF; rojo mientras se graba, ámbar transcribiendo).
//!
//! Interacción:
//! - Pulsar = grabar (mismo `Message::StartRecording` de la ventana principal).
//! - Soltar  = transcribir y ESCRIBIR el texto en la app que tiene el foco.
//! - Arrastrar (mover > 7 px lógicos) = mover la burbuja, no dictar.
//!
//! La inyección reutiliza `inject::inject_text_into_active_app` (clipboard +
//! Ctrl+V). Para que el Ctrl+V llegue al campo activo de la otra app y no a la
//! burbuja, esta ventana se marca WS_EX_NOACTIVATE: los clics nunca la activan
//! y el foco del usuario nunca se roba.
//!
//! No se duplica lógica de audio/whisper: la burbuja solo emite los mismos
//! mensajes que el hero de la ventana principal.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use iced::widget::{container, text};
use iced::window;
use iced::{
    Background, Border, Color, Element, Length, Point, Size, Subscription, Task, Theme,
};

use crate::app::{Message, Status, WhisprApp};

/// Tamaño de la ventana de la burbuja (y del círculo), en px lógicos.
pub const BUBBLE_SIZE: f32 = 56.0;

/// Título único de la ventana de la burbuja: sirve para localizarla por HWND
/// (FindWindowW) y aplicarle WS_EX_NOACTIVATE tras crearse.
pub const BUBBLE_TITLE: &str = "WhisprOpen-Bubble";

/// `window::Id` de la burbuja en formato numérico (`Id` es Display). Se guarda
/// aquí porque `iced::event::listen_with` exige una `fn` estática sin
/// capturas; el filtro del subscriptor compara contra este valor.
static BUBBLE_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

/// Configuración de la ventana de la burbuja. Verificado en iced 0.13.1
/// (`iced_core::window::Settings`): `decorations`, `transparent`, `level`
/// (`AlwaysOnTop`) y `platform_specific.skip_taskbar` están soportados y se
/// mapean a winit (`WindowAttributesExtWindows`, `with_window_level`,
/// `with_transparent`).
pub fn bubble_settings() -> window::Settings {
    let mut s = window::Settings::default();
    s.size = Size::new(BUBBLE_SIZE, BUBBLE_SIZE);
    s.min_size = Some(Size::new(BUBBLE_SIZE, BUBBLE_SIZE));
    s.max_size = Some(Size::new(BUBBLE_SIZE, BUBBLE_SIZE));
    // Esquina superior derecha del monitor principal, con un margen.
    s.position = window::Position::SpecificWith(|win, monitor| {
        Point::new(monitor.width - win.width - 24.0, 48.0)
    });
    s.resizable = false;
    s.decorations = false;
    s.transparent = true;
    s.level = window::Level::AlwaysOnTop;
    s.visible = true;
    s.platform_specific.skip_taskbar = true;
    s.platform_specific.undecorated_shadow = false;
    s.platform_specific.drag_and_drop = false;
    // El cierre lo decide la app (Message::WindowCloseRequested → iced::exit);
    // con `true` iced cerraría la ventana sin emitir el evento y el proceso
    // seguiría vivo con la ventana principal.
    s.exit_on_close_request = false;
    s
}

/// Tema de la burbuja: fondo 100 % transparente para que solo se vea el
/// círculo. La `Appearance` (color de limpieza del frame) se deriva de la
/// paleta del tema de cada ventana, así que la ventana principal conserva su
/// fondo opaco.
pub fn bubble_theme() -> Theme {
    use iced::theme::Palette;

    Theme::custom(
        "Whispr Open — burbuja".to_string(),
        Palette {
            background: Color::TRANSPARENT,
            text: Color::WHITE,
            primary: crate::theme::ACCENT,
            success: crate::theme::OK,
            danger: crate::theme::ERR,
        },
    )
}

/// Suscripción a los eventos de ratón de la burbuja: pulso, suelta y
/// movimiento (para detectar arrastre). Filtrada por window::Id, así la
/// ventana principal no se ve afectada.
pub fn bubble_events_subscription(bubble_id: Option<window::Id>) -> Subscription<Message> {
    if let Some(id) = bubble_id {
        BUBBLE_WINDOW_ID.store(id_num(id), Ordering::Relaxed);
    }
    iced::event::listen_with(bubble_event_filter)
}

/// `window::Id` -> u64 (el campo interno de `Id` es privado; `Display` lo
/// expone).
fn id_num(id: window::Id) -> u64 {
    id.to_string().parse().unwrap_or(0)
}

/// Filtro estático de eventos (requisito de `listen_with`): solo reacciona a
/// eventos de ratón dentro de la ventana de la burbuja.
fn bubble_event_filter(
    event: iced::Event,
    _status: iced::event::Status,
    window: window::Id,
) -> Option<Message> {
    if id_num(window) != BUBBLE_WINDOW_ID.load(Ordering::Relaxed) {
        return None;
    }
    use iced::mouse::{self, Button};

    match event {
        iced::Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
            Some(Message::BubblePress)
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
            Some(Message::BubbleRelease)
        }
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::BubbleCursorMoved(position))
        }
        // El cursor salió de la ventana sin soltar: tratar como soltar
        // (si no hay press pendiente, BubbleRelease es un no-op).
        iced::Event::Mouse(mouse::Event::CursorLeft) => Some(Message::BubbleRelease),
        _ => None,
    }
}

/// Vista de la burbuja: círculo violeta con borde claro para contrastar sobre
/// cualquier fondo. Estados: violeta = listo, rojo + punto blanco = grabando,
/// ámbar = transcribiendo / descargando modelo, rojo = error.
impl WhisprApp {
    pub fn bubble_view(&self) -> Element<'static, Message> {
        let recording = matches!(self.bubble_status(), Status::Recording);
        let fill = match self.bubble_status() {
            Status::Recording | Status::Error(_) => crate::theme::ERR,
            Status::Transcribing | Status::Downloading { .. } => crate::theme::WARN,
            Status::Idle => crate::theme::ACCENT,
        };

        // Indicador "REC": punto blanco centrado mientras se graba.
        let center: Element<'static, Message> = if recording {
            container(text(""))
                .width(Length::Fixed(16.0))
                .height(Length::Fixed(16.0))
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::WHITE)),
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: (8.0).into(),
                    },
                    ..Default::default()
                })
                .into()
        } else {
            container(text("")).into()
        };

        container(center)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    color: Color::WHITE,
                    width: 2.0,
                    radius: (BUBBLE_SIZE / 2.0).into(),
                },
                ..Default::default()
            })
            .into()
    }
}

// ---------------------------------------------------------------------------
// Integración multi-ventana: implementación de iced_winit::Program
// ---------------------------------------------------------------------------

impl iced_winit::Program for WhisprApp {
    type Message = Message;
    type Theme = Theme;
    type Renderer = iced::Renderer;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Task<Message>) {
        WhisprApp::new()
    }

    fn title(&self, id: window::Id) -> String {
        if self.is_bubble_window(id) {
            BUBBLE_TITLE.to_string()
        } else {
            "Whispr Open — Dictado local".to_string()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        self.update(message)
    }

    fn view(&self, id: window::Id) -> Element<'_, Message, Theme, iced::Renderer> {
        if self.is_bubble_window(id) {
            self.bubble_view()
        } else {
            self.view()
        }
    }

    fn theme(&self, id: window::Id) -> Theme {
        if self.is_bubble_window(id) {
            bubble_theme()
        } else {
            crate::theme::custom(self.config_is_dark())
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        self.subscription()
    }
}

// ---------------------------------------------------------------------------
// WS_EX_NOACTIVATE: la burbuja nunca roba el foco
// ---------------------------------------------------------------------------

/// Aplica WS_EX_NOACTIVATE (+TOOLWINDOW) a la ventana de la burbuja en cuanto
/// existe. Sin esto, el clic activaría la burbuja y el Ctrl+V simulado caería
/// en ella en vez de en el campo de texto de la otra app.
pub fn apply_noactivate_async() {
    std::thread::spawn(|| {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(hwnd) = find_bubble_hwnd() {
                apply_noactivate(hwnd);
                return;
            }
            if std::time::Instant::now() >= deadline {
                crate::log!(
                    "bubble",
                    "no se encontró la ventana de la burbuja para WS_EX_NOACTIVATE"
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

#[cfg(target_os = "windows")]
type BubbleHwnd = winapi::shared::windef::HWND;

#[cfg(not(target_os = "windows"))]
type BubbleHwnd = ();

#[cfg(target_os = "windows")]
fn find_bubble_hwnd() -> Option<BubbleHwnd> {
    use winapi::um::winuser::FindWindowW;

    let wide: Vec<u16> = BUBBLE_TITLE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    if hwnd.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

#[cfg(not(target_os = "windows"))]
fn find_bubble_hwnd() -> Option<BubbleHwnd> {
    None
}

#[cfg(target_os = "windows")]
fn apply_noactivate(hwnd: BubbleHwnd) {
    use winapi::um::winuser::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };

    unsafe {
        let ex = (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)
            & !WS_EX_APPWINDOW;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        // SWP_FRAMECHANGED hace efectivo el cambio de estilos extendidos.
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        crate::log!("bubble", "burbuja configurada sin activación (exstyle=0x{ex:x})");
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_noactivate(_hwnd: BubbleHwnd) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_title_es_ascii_y_unico() {
        // FindWindowW por título exige coincidencia exacta UTF-16; mantener el
        // título libre de caracteres no ASCII para que la búsqueda sea fiable.
        assert!(BUBBLE_TITLE.is_ascii());
        assert_eq!(BUBBLE_TITLE, "WhisprOpen-Bubble");
    }

    #[test]
    fn burbuja_cuadrada() {
        let s = bubble_settings();
        assert_eq!(s.size.width, BUBBLE_SIZE);
        assert_eq!(s.size.height, BUBBLE_SIZE);
        assert!(!s.decorations);
        assert!(!s.resizable);
    }

    #[test]
    fn burbuja_siempre_encima_y_transparente() {
        let s = bubble_settings();
        assert!(s.transparent);
        assert_eq!(s.level, window::Level::AlwaysOnTop);
        assert_eq!(s.platform_specific.skip_taskbar, true);
    }

    #[test]
    fn posicion_inicial_esquina_superior_derecha() {
        let s = bubble_settings();
        if let window::Position::SpecificWith(f) = s.position {
            let p = f(Size::new(56.0, 56.0), Size::new(1920.0, 1080.0));
            assert!(p.x > 1700.0, "x={} debe quedar cerca del borde derecho", p.x);
            assert!(p.y < 100.0, "y={} debe quedar arriba", p.y);
        } else {
            panic!("la burbuja debe usar Position::SpecificWith");
        }
    }

    #[test]
    fn filtra_eventos_por_ventana() {
        // La suscripción no reacciona a eventos de otras ventanas (no hay forma
        // de inspeccionarla directamente, así que aquí solo se valida el API
        // público: la función acepta None y una Id cualquiera sin pánico).
        let _ = bubble_events_subscription(None);
        let _ = bubble_events_subscription(Some(window::Id::unique()));
    }
}