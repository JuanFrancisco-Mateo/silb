//! Burbuja flotante de dictado (overlay siempre-encima).
//!
//! Segunda ventana Iced del mismo proceso (multi-window de iced 0.13 vía
//! `iced_winit::Program`): pequeña, sin decoración, transparente,
//! always-on-top (`window::Level::AlwaysOnTop`) y circular (acento violeta
//! #7C6CFF; rojo mientras se graba, ámbar transcribiendo).
//!
//! Interacción:
//! - Clic izquierdo (pulsar → soltar) = grabar y transcribir, escribiendo el
//!   texto en la app con foco (comportamiento por defecto; NO se toca).
//! - Arrastrar la burbuja = SOLO con clic derecho mantenido + movimiento
//!   (> 7 px lógicos). El clic derecho simple (sin movimiento) no hace nada.
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
    Alignment, Background, Border, Color, Element, Length, Point, Size, Subscription, Task, Theme,
};

#[cfg(target_os = "windows")]
use winapi::shared::windef::{HWND, POINT, RECT};
#[cfg(target_os = "windows")]
use winapi::um::winuser::{
    GetAsyncKeyState, GetCapture, GetCursorPos, GetMonitorInfoW, GetWindowRect, MonitorFromPoint,
    MonitorFromWindow, ReleaseCapture, SetCapture, SetWindowPos, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, VK_LBUTTON, VK_RBUTTON,
};

use crate::app::{Message, Status, WhisprApp};
use crate::config::{BubbleCorner, Config};

/// Tamaño de la ventana de la burbuja (y del círculo), en px lógicos.
pub const BUBBLE_SIZE: f32 = 56.0;

/// Rango completo de tamaños de la burbuja (48–72): la ventana se crea con
/// min/max = rango entero para que el resize EN CALIENTE (`window::resize` a
/// cualquier `BubbleSize`) no se tope con el clamp de WM_GETMINMAXINFO.
pub const BUBBLE_SIZE_MIN: f32 = 48.0;
pub const BUBBLE_SIZE_MAX: f32 = 72.0;

/// Margen de las esquinas (px lógicos): separación de los bordes del monitor.
pub const BUBBLE_MARGIN: f32 = 24.0;

/// Offset vertical de las esquinas SUPERIORES (px lógicos): deja espacio
/// para la barra de tareas/menús flotantes sin pegar la burbuja al borde.
pub const BUBBLE_TOP_OFFSET: f32 = 48.0;

/// Título único de la ventana de la burbuja: sirve para localizarla por HWND
/// (FindWindowW) y aplicarle WS_EX_NOACTIVATE tras crearse.
pub const BUBBLE_TITLE: &str = "Silb-Bubble";

/// `application_id` de la ventana de la burbuja en Linux (iced 0.13 lo expone
/// en la rama wayland de `PlatformSpecific`; no hay `skip_taskbar`, así que el
/// compositor identifica la ventana por este id). Ver CHECK-1.md L224-226.
#[cfg(target_os = "linux")]
pub const BUBBLE_APP_ID: &str = "whispr-open-bubble";

/// `window::Id` de la burbuja en formato numérico (`Id` es Display). Se guarda
/// aquí porque `iced::event::listen_with` exige una `fn` estática sin
/// capturas; el filtro del subscriptor compara contra este valor.
static BUBBLE_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Arrastre con botón derecho: umbral y máquina de estados
// ---------------------------------------------------------------------------

/// Distancia mínima (px lógicos) de movimiento para considerar que es un
/// arrastre y no un clic (mismo umbral de 7 px que tenía el drag original).
pub const DRAG_THRESHOLD: f32 = 7.0;

/// Mínimo de la burbuja que debe quedar visible dentro del área de trabajo
/// del monitor al arrastrar (px físicos): siempre recuperable con un
/// segundo arrastre. Compartido por el clamp de `drag_target` y sus tests.
const MIN_VISIBLE_PX: i32 = 24;

/// True si el movimiento entre `from` y `to` supera el umbral de arrastre
/// (comparación al cuadrado para evitar la raíz cuadrada).
pub fn is_drag_movement(from: Point, to: Point) -> bool {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    dx * dx + dy * dy > DRAG_THRESHOLD * DRAG_THRESHOLD
}

/// Máquina de estados del arrastre con botón derecho.
///
/// El botón derecho SOLO arrastra la burbuja: al pulsar se queda «armado»,
/// si el cursor se mueve más de `DRAG_THRESHOLD` empieza el arrastre, y al
/// soltar (o cancelar) se cierra todo. Nunca toca audio ni estado de
/// grabación: un clic derecho simple (pulsar y soltar sin mover) no hace
/// nada, y al terminar el arrastre no queda ningún estado residual.
#[derive(Debug, Clone, PartialEq)]
pub struct RightDragState {
    /// Botón derecho pulsado dentro de la burbuja.
    pub pressed: bool,
    /// El movimiento superó el umbral: la burbuja se está arrastrando.
    pub dragging: bool,
    /// Posición del cursor al pulsar (para medir el umbral).
    press_pos: Point,
}

impl Default for RightDragState {
    fn default() -> Self {
        Self {
            pressed: false,
            dragging: false,
            press_pos: Point::ORIGIN,
        }
    }
}

impl RightDragState {
    /// Pulsar el botón derecho: arma el posible arrastre (sin grabar).
    pub fn press(&mut self, pos: Point) {
        self.pressed = true;
        self.dragging = false;
        self.press_pos = pos;
    }

    /// Movimiento con el botón derecho pulsado. Devuelve `true` solo en el
    /// instante en que el movimiento cruza el umbral (arranque del arrastre).
    pub fn move_cursor(&mut self, pos: Point) -> bool {
        if !self.pressed || self.dragging {
            return false;
        }
        if is_drag_movement(self.press_pos, pos) {
            self.dragging = true;
            true
        } else {
            false
        }
    }

    /// Soltar el botón derecho. Devuelve `true` si se estaba arrastrando
    /// (el llamador debe liberar la captura del ratón).
    pub fn release(&mut self) -> bool {
        let was_dragging = self.dragging;
        self.pressed = false;
        self.dragging = false;
        was_dragging
    }

    /// Cancelar sin arrastre (cursor fuera de la ventana, captura perdida).
    pub fn cancel(&mut self) {
        self.pressed = false;
        self.dragging = false;
    }
}

/// Posición inicial de la burbuja para una esquina dada, en px LÓGICOS.
/// Función pura (sin winapi, testeable en cualquier target): `win` es el
/// tamaño de la ventana y `monitor` el del monitor donde cae. Margen
/// `BUBBLE_MARGIN` en los bordes laterales/inferiores y `BUBBLE_TOP_OFFSET`
/// en los superiores.
pub fn initial_position(corner: BubbleCorner, win: Size, monitor: Size) -> Point {
    let x = match corner {
        BubbleCorner::TopRight | BubbleCorner::BottomRight => {
            monitor.width - win.width - BUBBLE_MARGIN
        }
        BubbleCorner::TopLeft | BubbleCorner::BottomLeft => BUBBLE_MARGIN,
    };
    let y = match corner {
        BubbleCorner::TopRight | BubbleCorner::TopLeft => BUBBLE_TOP_OFFSET,
        BubbleCorner::BottomRight | BubbleCorner::BottomLeft => {
            monitor.height - win.height - BUBBLE_MARGIN
        }
    };
    Point::new(x, y)
}

// `window::Position::SpecificWith` exige una `fn` sin capturas; cada esquina
// tiene su wrapper (const-fn pointer) para poder seleccionarla en
// `bubble_settings` sin clausuras que capturen.
fn corner_top_right(win: Size, monitor: Size) -> Point {
    initial_position(BubbleCorner::TopRight, win, monitor)
}
fn corner_top_left(win: Size, monitor: Size) -> Point {
    initial_position(BubbleCorner::TopLeft, win, monitor)
}
fn corner_bottom_right(win: Size, monitor: Size) -> Point {
    initial_position(BubbleCorner::BottomRight, win, monitor)
}
fn corner_bottom_left(win: Size, monitor: Size) -> Point {
    initial_position(BubbleCorner::BottomLeft, win, monitor)
}

/// Configuración de la ventana de la burbuja. Verificado en iced 0.13.1
/// (`iced_core::window::Settings`): `decorations`, `transparent`, `level`
/// (`AlwaysOnTop`) y `platform_specific.skip_taskbar` están soportados y se
/// mapean a winit (`WindowAttributesExtWindows`, `with_window_level`,
/// `with_transparent`).
///
/// En Linux (winit Wayland): `transparent` funciona sobre el alpha del
/// buffer; `level` AlwaysOnTop llega como preferencia al compositor
/// (Hyprland la honra con windowrule o la ignora — se recomienda el
/// `float, pin` por title), `skip_taskbar` el equivalente (dock to all
/// workspaces) lo maneja el compositor. Nada de esto rompe: son
/// preferencias y el fallback es el comportamiento natural de Hyprland.
///
/// Parámetros del Config (Ajustes → Apariencia): tamaño del círculo
/// (`bubble_size`) y esquina de anclaje (`bubble_corner`). El min/max de la
/// ventana es el rango COMPLETO 48–72: si fuese el tamaño fijo, el resize en
/// caliente a otro tamaño chocaría con el clamp de WM_GETMINMAXINFO.
pub fn bubble_settings(cfg: &Config) -> window::Settings {
    let size = cfg.bubble_size().px();
    let corner = cfg.bubble_corner();
    let mut s = window::Settings::default();
    s.size = Size::new(size, size);
    s.min_size = Some(Size::new(BUBBLE_SIZE_MIN, BUBBLE_SIZE_MIN));
    s.max_size = Some(Size::new(BUBBLE_SIZE_MAX, BUBBLE_SIZE_MAX));
    s.position = window::Position::SpecificWith(match corner {
        BubbleCorner::TopRight => corner_top_right,
        BubbleCorner::TopLeft => corner_top_left,
        BubbleCorner::BottomRight => corner_bottom_right,
        BubbleCorner::BottomLeft => corner_bottom_left,
    });
    s.resizable = false;
    s.decorations = false;
    s.transparent = true;
    s.level = window::Level::AlwaysOnTop;
    s.visible = true;
    // Windows: `PlatformSpecific` de iced 0.13 expone aquí skip_taskbar,
    // undecorated_shadow y drag_and_drop. En la rama wayland/linux de iced
    // 0.13 esos campos NO existen (solo `application_id` y
    // `override_redirect`) → E0609 en CHECK-1.md L224-226. Se gatean a
    // Windows para no romper la compilación en Linux.
    #[cfg(target_os = "windows")]
    {
        s.platform_specific.skip_taskbar = true;
        s.platform_specific.undecorated_shadow = false;
        s.platform_specific.drag_and_drop = false;
    }
    // Linux/Wayland (equivalente no-op): el «no aparecer en la barra de
    // tareas / quedar por encima» lo decide el compositor. El único asidero
    // que da iced 0.13 en Linux es `application_id`, que Hyprland usa para
    // las windowrule; `override_redirect` se deja en su default (false).
    // Ver CHECK-1.md L224-226. Nada de esto rompe: hoy no hay regla
    // asociada y el fallback es el comportamiento natural de Hyprland.
    #[cfg(target_os = "linux")]
    {
        s.platform_specific.application_id = BUBBLE_APP_ID.to_string();
    }
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
        "Silb — burbuja".to_string(),
        Palette {
            background: Color::TRANSPARENT,
            text: Color::WHITE,
            primary: crate::theme::ACCENT,
            success: crate::theme::OK,
            danger: crate::theme::ERR,
        },
    )
}

/// Suscripción a los eventos de ratón de la burbuja: pulsos (izquierdo =
/// grabar, derecho = arrastrar), sueltas y movimiento para detectar el
/// arrastre. Filtrada por window::Id, así la ventana principal no se ve
/// afectada.
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
        iced::Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => Some(Message::BubblePress),
        iced::Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
            Some(Message::BubbleRelease)
        }
        iced::Event::Mouse(mouse::Event::ButtonPressed(Button::Right)) => {
            Some(Message::BubbleRightPress)
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(Button::Right)) => {
            Some(Message::BubbleRightRelease)
        }
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::BubbleCursorMoved(position))
        }
        // El cursor salió de la ventana sin soltar: cerrar ambos botones
        // (izquierdo dicta igualmente; derecho cancela cualquier arrastre).
        iced::Event::Mouse(mouse::Event::CursorLeft) => Some(Message::BubbleCursorLeft),
        _ => None,
    }
}

/// Estilos y vista del overlay de la burbuja (el círculo flotante).
impl WhisprApp {
    /// Vista de la burbuja: círculo violeta con borde claro para contrastar
    /// sobre cualquier fondo. Estados: violeta = listo, rojo + punto blanco =
    /// grabando, ámbar = transcribiendo / descargando modelo, rojo = error.
    ///
    /// Personalización III: si el usuario eligió una imagen para la burbuja
    /// (sprite estático o frame actual de una secuencia animada), se pinta
    /// DEBAJO del centro (que mantiene el indicador de estado — punto de
    /// grabación / etiqueta «IA» — SIEMPRE legible). Si la imagen falla al
    /// cargar (ruta inválida al renderizar, formato no soportado), la
    /// burbuja cae al render vectorial sin crashear: `resolve_static` /
    /// `resolve_frames` ya filtraron las rutas malas, y `iced::image`
    /// pinta un placeholder si el path cambió a última hora.
    pub fn bubble_view(&self) -> Element<'static, Message> {
        let style = self.bubble_style();
        let recording = matches!(self.bubble_status(), Status::Recording);
        let size = self.bubble_size_px();
        // Opacidad configurable (Ajustes → Apariencia, slider 20–100 %):
        // alpha por-pixel aplicada al círculo y al borde. El punto «REC»
        // central queda opaco (indicador de estado: debe leerse siempre).
        let opacity = self.bubble_opacity_percent() as f32 / 100.0;
        // Estados de arranque (requisito UX 09/09/2026): ámbar mientras la
        // app carga whisper/LLM, rojo si el arranque falló sin modelo. Una
        // vez Ready, el color lo decide `bubble_status` como siempre.
        let fill = match self.startup_bubble_color() {
            Some(color) => color,
            None => match self.bubble_status() {
                Status::Recording | Status::Error(_) => crate::theme::ERR,
                Status::Transcribing | Status::Correcting | Status::Downloading { .. } => {
                    crate::theme::WARN
                }
                Status::Idle => crate::theme::ACCENT,
            },
        };
        let (fill, border_width) = match style {
            crate::config::BubbleStyle::Solid => (crate::theme::with_alpha(fill, opacity), 2.0),
            crate::config::BubbleStyle::Ring => (Color::TRANSPARENT, 3.0),
            crate::config::BubbleStyle::Minimal => {
                (crate::theme::with_alpha(fill, opacity * 0.82), 0.0)
            }
        };

        // Indicador central: punto blanco mientras se graba; «IA» (oscuro
        // sobre ámbar) mientras el corrector LLM trabaja sobre el último
        // turno (feedback en vivo del dictado: requisito UX 10/09/2026 —
        // aunque la burbuja no tiene texto, la etiqueta IA dice que la
        // corrección LLM está actuando y no que la app está colgada).
        let correcting = matches!(self.bubble_status(), Status::Correcting);
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
        } else if correcting {
            // Etiqueta pequeña sobre una pastilla oscura: legible sobre
            // cualquier relleno (sólido ámbar, ring transparente, minimal).
            container(
                container(text("IA").size(10.0).color(Color::WHITE))
                    .padding([2, 4])
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.10, 0.07, 0.16))),
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: (5.0).into(),
                        },
                        ..Default::default()
                    }),
            )
            .into()
        } else {
            container(text("")).into()
        };

        // Imagen de la burbuja: GIF en memoria > frames legacy > sprite
        // estático > render vectorial (el que define `fill` arriba).
        // El GIF es UN solo archivo (sprint final): se decodifica una vez en
        // `ensure_gif_loaded` y aquí solo se clona el Handle (barato, Arc).
        if !self.gif_frames.is_empty() {
            let idx = self
                .bubble_frame_index
                .min(self.gif_frames.len().saturating_sub(1));
            let handle = self.gif_frames[idx].clone();
            let bg: Element<'static, Message> = container(
                iced::widget::image(handle)
                    .content_fit(iced::ContentFit::Cover)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .opacity(opacity),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
            let center_container: Element<'static, Message> =
                container(center)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .into();
            return iced::widget::stack![bg, center_container].into();
        }
        let bubble_image: Option<std::path::PathBuf> = self
            .bubble_current_frame()
            .or_else(|| self.bubble_static_image());

        if let Some(path) = bubble_image {
            // El centro (REC / IA) SIEMPRE va encima de la imagen:
            // apilamos con `stack!` para que el indicador de estado
            // nunca quede ocluido por el sprite. La imagen se estira
            // con `ContentFit::Cover` y se recorta por el border-radius
            // circular del contenedor exterior (los píxeles fuera del
            // círculo los recorta winit al componer la ventana).
            let handle = iced::widget::image::Handle::from_path(&path);
            let bg: Element<'static, Message> = container(
                iced::widget::image(handle)
                    .content_fit(iced::ContentFit::Cover)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .opacity(opacity),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
            let center_container: Element<'static, Message> =
                container(center)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .into();
            iced::widget::stack![bg, center_container].into()
        } else {
            container(center)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(move |_| container::Style {
                    background: Some(Background::Color(fill)),
                    border: Border {
                        color: crate::theme::with_alpha(Color::WHITE, opacity),
                        width: border_width,
                        radius: (size / 2.0).into(),
                    },
                    ..Default::default()
                })
                .into()
        }
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
            "Silb — Dictado local".to_string()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        WhisprApp::update(self, message)
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
        WhisprApp::subscription(self)
    }
}

// ---------------------------------------------------------------------------
// WS_EX_NOACTIVATE: la burbuja nunca roba el foco
// ---------------------------------------------------------------------------

/// Aplica a la burbuja lo que dicta el `Config`, en cuanto la ventana existe.
///
/// El doc-comment original (Windows) describe el detalle winapi; en Linux el
/// caso es DISTINTO y merece su propia explicación:
/// - El siempre-encima NO lo maneja winit: un `float, pin` por título en
///   hyprland.conf hace el papel de WS_EX_TOPMOST (así se recomienda, con
///   pasos en docs/PORT-LINUX/CODIGO-CORE.md).
/// - WS_EX_NOACTIVATE equivale a NO pedir nunca el foco: ni `gain_focus`,
///   ni `request_user_attention`, ni acciones que roben el foco; en Wayland
///   el foco de otra app pasa por xdg_activation_v1 (token del cliente con
///   foco), así que la app necesita estar activa para pedirlo — en iced 0.13
///   `window::gain_focus` es Unsupported en Wayland (docs de window::Action).
/// - Arrastre sin captura global: ver los stubs de ratón más abajo.
#[cfg(target_os = "windows")]
pub fn apply_bubble_extras(cfg: &Config) {
    let corner = cfg.bubble_corner();
    let size = cfg.bubble_size().px();
    let pos = cfg.bubble_position();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(hwnd) = find_bubble_hwnd() {
                apply_noactivate(hwnd);
                let (x, y) = match pos {
                    Some((x, y)) => (x, y),
                    None => corner_position(corner, hwnd, size),
                };
                let s = size as i32;
                unsafe {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        x,
                        y,
                        s,
                        s,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                crate::log!("bubble", "burbuja aplicada: noact+pos ({x},{y})+tamaño {s}");
                return;
            }
            if std::time::Instant::now() >= deadline {
                crate::log!(
                    "bubble",
                    "no se encontró la ventana de la burbuja para aplicar los extras"
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Linux: los extras nativos (noactivate forzado por exstyle, mover por
/// hwnd) no existen; la ventana nace ya sin robo de foco y Hyprland se
/// encarga del pin. Solo queda el log informativo (la burbuja ya es visible
/// por diseño, con los settings iced de `bubble_settings`).
#[cfg(not(target_os = "windows"))]
pub fn apply_bubble_extras(cfg: &Config) {
    crate::log!(
        "bubble",
        "burbuja Linux: sin WS_EX_NOACTIVATE; el no-robo de foco es el default de \
         Wayland y el always-on-top se recomienda via windowrulev2 (float, pin) \
         por title/class «{BUBBLE_TITLE}»"
    );
    let _ = cfg;
}

/// Mueve la burbuja EN CALIENTE a la esquina seleccionada en Ajustes.
/// Reposiciona sin cambiar el tamaño (el resize es competencia de
/// `BubbleSizeChanged` / `window::resize`). Se usa un hilo porque la ventana
/// puede no existir todavía (mismo patrón que `apply_bubble_extras`).
#[cfg(target_os = "windows")]
pub fn move_to_corner(corner: BubbleCorner, size: f32) {
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(hwnd) = find_bubble_hwnd() {
                let (x, y) = corner_position(corner, hwnd, size);
                unsafe {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        x,
                        y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                crate::log!("bubble", "burbuja movida a {} en ({x},{y})", corner.label());
                return;
            }
            if std::time::Instant::now() >= deadline {
                crate::log!(
                    "bubble",
                    "no se encontró la ventana para mover a la esquina"
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Linux: mover la burbuja a esquina via iced::window::move_to (cambios de
/// esquina desde Ajustes), dependiente de la sincronización con los
/// mensajes de UI. A validar: winit moved-to en Hyprland puede ignorar el
/// destino (su política de posicionamiento); si el turno del usuario lo
/// exige, dejar backoff hasta que Hyprland agregue opciones por IPC.
#[cfg(not(target_os = "windows"))]
pub fn move_to_corner(corner: BubbleCorner, size: f32) {
    let _ = (corner, size);
    // Sin una Task de iced aquí (la firma es sync y sin Message), lo que
    // move_to_corner puede hacer es registrar la intención en el log: la
    // esquina se re-aplica en el próximo BubbleOpened.
    crate::log!(
        "bubble",
        "move_to_corner Linux: no-op (la esquina manda en el próximo BubbleOpened)"
    );
}

/// Posición FÍSICA actual de la ventana de la burbuja (px de pantalla, la
/// esquina superior izquierda). `None` si la ventana aún no existe. La usa
/// `end_bubble_drag` para persistir la posición del último arrastre en el
/// Config (se re-aplica al arrancar vía `apply_bubble_extras`).
#[cfg(target_os = "windows")]
pub fn bubble_window_position() -> Option<(i32, i32)> {
    unsafe {
        let hwnd = find_bubble_hwnd()?;
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
        Some((rect.left, rect.top))
    }
}

/// Linux: sin handle nativo que interrogar, iced/winit no expone la
/// posición sincrónica (GetPosition es una Task asíncrona). Devuelve `None`
/// y el efecto esperado es que NO se persista posición — el arrastre Linux
/// no mueve la ventana nativa (ver `BubbleDrag`), así que no hay posición
/// nueva que guardar.
#[cfg(not(target_os = "windows"))]
pub fn bubble_window_position() -> Option<(i32, i32)> {
    None
}

/// Esquina configurada → posición FÍSICA de la ventana de la burbuja: el
/// área de trabajo (`rcWork`) del monitor que la contiene, más los márgenes.
/// `size` en px físicos (la ventana es cuadrada; el borde superior usa
/// `BUBBLE_TOP_OFFSET` para no tapar menús de la barra de tareas).
#[cfg(target_os = "windows")]
fn corner_position(corner: BubbleCorner, hwnd: BubbleHwnd, size: f32) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return (0, 0);
        }
        let work = info.rcWork;
        let w = (work.right - work.left) as f32;
        let h = (work.bottom - work.top) as f32;
        // `initial_position` trabaja en coordenadas relativas al monitor; el
        // rcWork puede no empezar en (0,0) (multi-monitor): se suma el origen.
        let p = initial_position(corner, Size::new(size, size), Size::new(w, h));
        (work.left + p.x as i32, work.top + p.y as i32)
    }
}

#[cfg(target_os = "windows")]
type BubbleHwnd = HWND;

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
    // En Wayland no hay handle global buscable por título (FindWindowW es
    // winapi); la burbuja se gestiona por `window::Id` de iced en la app.
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
        let ex =
            (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)
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
        crate::log!(
            "bubble",
            "burbuja configurada sin activación (exstyle=0x{ex:x})"
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_noactivate(_hwnd: BubbleHwnd) {}

// ---------------------------------------------------------------------------
// Arrastre con botón derecho (winapi): captura del ratón + seguimiento
// ---------------------------------------------------------------------------

/// Arrastre manual de la burbuja con el botón derecho.
///
/// No se usa `window::drag` a propósito: en Windows envía
/// `WM_NCLBUTTONDOWN` (HTCAPTION) y el bucle modal del sistema espera el
/// botón IZQUIERDO — si se suelta el derecho, la burbuja quedaría congelada
/// esperando un clic izquierdo. En su lugar se captura el ratón
/// (`SetCapture`) y cada movimiento del cursor reposiciona la ventana con
/// `SetWindowPos` hasta soltar. La captura además garantiza que el evento de
/// soltado llegue a la burbuja aunque el cursor quede fuera de su rectángulo.
#[cfg(target_os = "windows")]
pub struct BubbleDrag {
    hwnd: BubbleHwnd,
    /// Esquina superior izquierda de la ventana en el momento de agarrar
    /// (px físicos). ANCLAJE ABSOLUTO: la posición se recalcula siempre como
    /// `grab_window + (cursor_actual - grab_cursor)`, nunca por deltas
    /// acumulados (ver `drag_target`).
    grab_window: (i32, i32),
    /// Posición del cursor al agarrar (px físicos, coordenadas de pantalla).
    grab_cursor: (i32, i32),
    /// Tamaño de la ventana en px físicos (para el clamp multi-monitor).
    size: (i32, i32),
    /// Última posición aplicada a la ventana; evita `SetWindowPos` redundantes.
    last: (i32, i32),
}

/// Stub fuera de Windows: el arrastre manual (winapi puro) no existe; en
/// Linux los stubs de abajo (start/follow/is_captured/end) devuelven
/// valores neutros para no disparar el watchdog de robo de captura del Poll
/// (100 ms) y el movimiento de la ventana queda a cargo del compositor.
#[cfg(not(target_os = "windows"))]
pub struct BubbleDrag {
    _private: (),
}

/// Posición objetivo de la burbuja para un cursor dado (px físicos).
///
/// Anclaje absoluto al punto de agarre (`grab_window + (cursor - grab_cursor)`)
/// con clamp al área de trabajo del monitor (`work` = left, top, right, bottom
/// del `rcWork`). Función pura: no toca winapi, testeable en cualquier target.
///
/// Por qué anclaje absoluto y no delta incremental: cuando la ventana NO se
/// mueve 1:1 con el cursor —al chocar con el clamp o durante un hueco sin
/// eventos (captura robada, alt-tab)— el enfoque `last + dx` acumula error y
/// lo aplica de golpe al siguiente movimiento: la burbuja «se teletransporta»
/// lejos de donde debería estar. Con anclaje absoluto la posición depende SOLO
/// del cursor actual: cualquier evento perdido o clamp se auto-corrige en el
/// siguiente `follow`, sin saltos.
pub fn drag_target(
    grab_window: (i32, i32),
    grab_cursor: (i32, i32),
    cursor: (i32, i32),
    size: (i32, i32),
    work: (i32, i32, i32, i32),
) -> (i32, i32) {
    let x = (grab_window.0 + (cursor.0 - grab_cursor.0))
        .max(work.0 - size.0 + MIN_VISIBLE_PX)
        .min(work.2 - MIN_VISIBLE_PX);
    let y = (grab_window.1 + (cursor.1 - grab_cursor.1))
        .max(work.1 - size.1 + MIN_VISIBLE_PX)
        .min(work.3 - MIN_VISIBLE_PX);
    (x, y)
}

#[cfg(target_os = "windows")]
impl BubbleDrag {
    /// Captura el ratón y prepara el seguimiento. `None` si la ventana de la
    /// burbuja aún no existe (los eventos vienen de ella, así que en la
    /// práctica siempre existe).
    ///
    /// Se llama en el PULSADO del botón derecho (no al cruzar el umbral): con
    /// movimientos rápidos el cursor puede salir de la ventana antes de medir
    /// los 7 px del umbral, y con la captura ya activa los eventos siguen
    /// llegando aunque el cursor esté fuera.
    pub fn start() -> Option<BubbleDrag> {
        unsafe {
            let hwnd = find_bubble_hwnd()?;
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return None;
            }
            let mut cursor: POINT = std::mem::zeroed();
            if GetCursorPos(&mut cursor) == 0 {
                return None;
            }
            let size = (rect.right - rect.left, rect.bottom - rect.top);
            // Ruido de alta frecuencia (H2 del QA 24/09/2026): en modo normal
            // este trazo no se escribe; `WHISPR_DEBUG=1` lo enciende.
            crate::log_debug!(
                "bubble",
                "drag start: ventana=({},{}) size=({},{}) cursor=({},{})",
                rect.left,
                rect.top,
                size.0,
                size.1,
                cursor.x,
                cursor.y
            );
            // Los eventos del ratón se dirigen a la burbuja mientras dure el
            // arrastre, aunque el cursor salga de su rectángulo.
            SetCapture(hwnd);
            Some(BubbleDrag {
                hwnd,
                grab_window: (rect.left, rect.top),
                grab_cursor: (cursor.x, cursor.y),
                size,
                last: (rect.left, rect.top),
            })
        }
    }

    /// Reposiciona la ventana siguiendo al cursor, manteniendo al menos
    /// `MIN_VISIBLE_PX` visibles dentro del área de trabajo del monitor.
    ///
    /// Posición absoluta desde el agarre (no delta acumulado): cada llamada
    /// recalcula el destino solo con el cursor actual, así que aunque el
    /// bucle de eventos se retrase o se pierdan movimientos, la burbuja
    /// siempre aterriza exactamente donde debe estar el cursor. Idempotente:
    /// se puede llamar desde cada `CursorMoved` y desde el poll de 100 ms.
    pub fn follow(&mut self) {
        unsafe {
            let mut cursor: POINT = std::mem::zeroed();
            if GetCursorPos(&mut cursor) == 0 {
                return;
            }
            // El monitor que contiene al cursor: permite arrastrar entre
            // monitores sin que la ventana se pierda fuera de la pantalla.
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return;
            }
            let work = info.rcWork;
            let (x, y) = drag_target(
                self.grab_window,
                self.grab_cursor,
                (cursor.x, cursor.y),
                self.size,
                (work.left, work.top, work.right, work.bottom),
            );
            if (x, y) == self.last {
                return;
            }
            self.last = (x, y);
            // Una línea por evento de ratón (~1.000/s): solo con
            // `WHISPR_DEBUG=1` (H2 del QA 24/09/2026).
            crate::log_debug!(
                "bubble",
                "drag target=({},{}) grab=({},{}) cursor=({},{})",
                x,
                y,
                self.grab_window.0,
                self.grab_window.1,
                cursor.x,
                cursor.y
            );
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// True si el HILO ACTUAL sigue teniendo la captura del ratón en la burbuja.
    ///
    /// El watchdog del poll la consulta: si el sistema roba la captura
    /// (alt-tab, menú del sistema, doble clic en otra ventana, nuevo
    /// `SetCapture` de otra app) el evento de soltado puede no llegar nunca;
    /// `GetCapture` lo detecta ≤ 100 ms después y el arrastre cierra limpio,
    /// sin esperar a un evento fantasma.
    pub fn is_captured(&self) -> bool {
        unsafe { GetCapture() == self.hwnd }
    }

    /// Libera la captura del ratón (fin del arrastre). No-op si el sistema
    /// ya la había robado (ReleaseCapture no hace nada sin captura propia).
    pub fn end(&self) {
        unsafe {
            ReleaseCapture();
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl BubbleDrag {
    /// Stub: fuera de Windows nunca hay un arrastre que iniciar.
    pub fn start() -> Option<BubbleDrag> {
        None
    }
    /// Stub: no-op (no hay ventana nativa que reposicionar).
    pub fn follow(&mut self) {}
    /// Siempre `true`: sin captura real, el watchdog de 100 ms de la UI no
    /// debe interpretar «captura robada» y cancelar un arrastre inexistente.
    pub fn is_captured(&self) -> bool {
        true
    }
    /// Stub: no-op (no hay captura que liberar).
    pub fn end(&self) {}
}

/// True mientras el botón derecho del ratón esté físicamente pulsado.
///
/// Red de seguridad del arrastre (el detector principal de robo de captura es
/// `BubbleDrag::is_captured` / `GetCapture`): si el evento de soltado se pierde
/// (alt-tab, menú del sistema) el poll de 100 ms usa este chequeo para cancelar
/// el arrastre al detectar el botón ya liberado.
#[cfg(target_os = "windows")]
pub fn right_button_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_RBUTTON) as u16) & 0x8000 != 0 }
}

/// Linux: sin poll del estado físico del ratón fuera de la ventana (Wayland
/// no lo expone a apps normales), la red de seguridad afirma «botón
/// pulsado» para no cancelar un arrastre por evento perdido. Concreto: el
/// arrastre Linux queda corto (se puede soltar fuera de la burbuja sin
/// cerrarlo hasta el siguiente BubbleRightRelease); limitación documentada.
#[cfg(not(target_os = "windows"))]
pub fn right_button_down() -> bool {
    true
}

/// Captura el botón izquierdo para que un turno no termine solo porque el
/// cursor salió del overlay mientras el usuario sigue hablando.
#[cfg(target_os = "windows")]
pub fn capture_left_mouse() {
    unsafe {
        if let Some(hwnd) = find_bubble_hwnd() {
            SetCapture(hwnd);
        }
    }
}

/// Linux: la captura global de entrada no existe (Wayland). La consecuencia
/// práctica es pequeña: si el cursor sale de los 56 px de la burbuja
/// mientras se dicta desde ella, el CursorLeft suelta el turno ANTES (sin
/// esperar a que físicamente se libere el botón). El flujo normal (abrir →
/// grabar → soltar dentro de la burbuja) queda igual; limitación en
/// CODIGO-CORE.md.
#[cfg(not(target_os = "windows"))]
pub fn capture_left_mouse() {}

/// Libera cualquier captura creada por la burbuja al finalizar el turno.
#[cfg(target_os = "windows")]
pub fn release_mouse_capture() {
    unsafe {
        ReleaseCapture();
    }
}

/// Stub fuera de Windows (no hay captura que soltar).
#[cfg(not(target_os = "windows"))]
pub fn release_mouse_capture() {}

/// True mientras el botón izquierdo está físicamente pulsado.
#[cfg(target_os = "windows")]
pub fn left_button_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON) as u16) & 0x8000 != 0 }
}

/// Linux: `false` («ya no pulsado») hace que el Poll cierre el turno
/// colgado al detectar el soltado real (BubbleRelease ya llegó o llegará
/// por el canal normal de eventos). Concreto: si el CursorLeft dejó a la
/// burbuja sin ver el soltado, el turno de dictado se cierra limpio en el
/// próximo Poll (100 ms) con lo ya grabado; limitación en CODIGO-CORE.md.
#[cfg(not(target_os = "windows"))]
pub fn left_button_down() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_title_es_ascii_y_unico() {
        // FindWindowW por título exige coincidencia exacta UTF-16; mantener el
        // título libre de caracteres no ASCII para que la búsqueda sea fiable.
        assert!(BUBBLE_TITLE.is_ascii());
        assert_eq!(BUBBLE_TITLE, "Silb-Bubble");
    }

    #[test]
    fn burbuja_cuadrada() {
        let s = bubble_settings(&Config::default());
        assert_eq!(s.size.width, BUBBLE_SIZE);
        assert_eq!(s.size.height, BUBBLE_SIZE);
        assert!(!s.decorations);
        assert!(!s.resizable);
    }

    #[test]
    fn burbuja_siempre_encima_y_transparente() {
        let s = bubble_settings(&Config::default());
        assert!(s.transparent);
        assert_eq!(s.level, window::Level::AlwaysOnTop);
        // `skip_taskbar` solo existe en el PlatformSpecific de Windows; en
        // Linux el equivalente se resuelve con `application_id` (CHECK-1.md
        // L224-226 → E0609).
        #[cfg(target_os = "windows")]
        assert_eq!(s.platform_specific.skip_taskbar, true);
        #[cfg(target_os = "linux")]
        assert_eq!(s.platform_specific.application_id, BUBBLE_APP_ID);
    }

    #[test]
    fn posicion_inicial_esquina_superior_derecha() {
        let s = bubble_settings(&Config::default());
        if let window::Position::SpecificWith(f) = s.position {
            let p = f(Size::new(56.0, 56.0), Size::new(1920.0, 1080.0));
            assert!(
                p.x > 1700.0,
                "x={} debe quedar cerca del borde derecho",
                p.x
            );
            assert!(p.y < 100.0, "y={} debe quedar arriba", p.y);
        } else {
            panic!("la burbuja debe usar Position::SpecificWith");
        }
    }

    // ── initial_position: esquinas puras y dentro del monitor ──

    #[test]
    fn initial_position_mata_a_la_esquina_top_right_historica() {
        // La esquina por defecto reproduce la posición histórica
        // (monitor.width - win.width - 24, 48).
        let p = initial_position(
            BubbleCorner::TopRight,
            Size::new(56.0, 56.0),
            Size::new(1920.0, 1080.0),
        );
        assert_eq!(
            p,
            Point::new(1920.0 - 56.0 - BUBBLE_MARGIN, BUBBLE_TOP_OFFSET)
        );
    }

    #[test]
    fn initial_position_cubre_las_cuatro_esquinas() {
        let win = Size::new(56.0, 56.0);
        let mon = Size::new(1920.0, 1080.0);
        assert_eq!(
            initial_position(BubbleCorner::TopLeft, win, mon),
            Point::new(BUBBLE_MARGIN, BUBBLE_TOP_OFFSET)
        );
        assert_eq!(
            initial_position(BubbleCorner::TopRight, win, mon),
            Point::new(1920.0 - 56.0 - BUBBLE_MARGIN, BUBBLE_TOP_OFFSET)
        );
        assert_eq!(
            initial_position(BubbleCorner::BottomLeft, win, mon),
            Point::new(BUBBLE_MARGIN, 1080.0 - 56.0 - BUBBLE_MARGIN)
        );
        assert_eq!(
            initial_position(BubbleCorner::BottomRight, win, mon),
            Point::new(1920.0 - 56.0 - BUBBLE_MARGIN, 1080.0 - 56.0 - BUBBLE_MARGIN)
        );
    }

    #[test]
    fn initial_position_siempre_cabe_en_el_monitor() {
        // Propiedad: en cualquier esquina y con cualquier tamaño de la burbuja
        // (48/56/72), la burbuja queda íntegra dentro del monitor de 1080p.
        for corner in [
            BubbleCorner::TopRight,
            BubbleCorner::TopLeft,
            BubbleCorner::BottomRight,
            BubbleCorner::BottomLeft,
        ] {
            for size in [48.0, 56.0, 72.0] {
                let win = Size::new(size, size);
                let p = initial_position(corner, win, Size::new(1920.0, 1080.0));
                assert!(
                    p.x >= 0.0 && p.y >= 0.0,
                    "{corner:?}@{size}: {p:?} fuera por arriba/izquierda"
                );
                assert!(
                    p.x + size <= 1920.0 && p.y + size <= 1080.0,
                    "{corner:?}@{size}: {p:?} desborda el monitor"
                );
            }
        }
    }

    #[test]
    fn bubble_settings_usa_tamano_y_esquina_del_config() {
        // Tamaño grande (72 px) + esquina inferior izquierda.
        let cfg = Config {
            bubble_size: Some(crate::config::BubbleSize::Large),
            bubble_corner: Some(BubbleCorner::BottomLeft),
            ..Default::default()
        };
        let s = bubble_settings(&cfg);
        assert_eq!(s.size.width, 72.0);
        assert_eq!(s.size.height, 72.0);
        if let window::Position::SpecificWith(f) = s.position {
            let p = f(Size::new(72.0, 72.0), Size::new(1920.0, 1080.0));
            assert_eq!(p, Point::new(BUBBLE_MARGIN, 1080.0 - 72.0 - BUBBLE_MARGIN));
        } else {
            panic!("la burbuja debe usar Position::SpecificWith");
        }
    }

    #[test]
    fn bubble_settings_rango_completo_48_72() {
        // El min/max de la ventana es TODO el rango de BubbleSize: el resize
        // en caliente (window::resize) no se topa con WM_GETMINMAXINFO.
        let s = bubble_settings(&Config::default());
        let (min, max) = (s.min_size.unwrap(), s.max_size.unwrap());
        assert_eq!(min, Size::new(BUBBLE_SIZE_MIN, BUBBLE_SIZE_MIN));
        assert_eq!(max, Size::new(BUBBLE_SIZE_MAX, BUBBLE_SIZE_MAX));
    }

    #[test]
    fn filtra_eventos_por_ventana() {
        // La suscripción no reacciona a eventos de otras ventanas (no hay forma
        // de inspeccionarla directamente, así que aquí solo se valida el API
        // público: la función acepta None y una Id cualquiera sin pánico).
        // Bajo el mismo FILTER_LOCK que los tests: `Some(id)` pisa
        // BUBBLE_WINDOW_ID (static compartido) y, sin serializar, rompería
        // los otros tests del filtro al correr en paralelo.
        let _guard = FILTER_LOCK.lock().unwrap();
        let _ = bubble_events_subscription(None);
        let _ = bubble_events_subscription(Some(window::Id::unique()));
    }

    // Los tests del filtro comparten `BUBBLE_WINDOW_ID` (static global), así
    // que se serializan con un mutex para no pisarse entre sí.
    static FILTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Estado base para los tests del filtro: dispara a la burbuja real.
    fn filter_id() -> window::Id {
        let id = window::Id::unique();
        let _ = bubble_events_subscription(Some(id));
        id
    }

    #[test]
    fn filtro_mapea_click_derecho_al_arrastre() {
        let _guard = FILTER_LOCK.lock().unwrap();
        let id = filter_id();
        let ev = iced::mouse::Event::ButtonPressed(iced::mouse::Button::Right);
        match bubble_event_filter(iced::Event::Mouse(ev), iced::event::Status::Ignored, id) {
            Some(Message::BubbleRightPress) => {}
            other => panic!("esperaba BubbleRightPress, recibí {other:?}"),
        }
        let ev = iced::mouse::Event::ButtonReleased(iced::mouse::Button::Right);
        match bubble_event_filter(iced::Event::Mouse(ev), iced::event::Status::Ignored, id) {
            Some(Message::BubbleRightRelease) => {}
            other => panic!("esperaba BubbleRightRelease, recibí {other:?}"),
        }
    }

    #[test]
    fn filtro_deja_intacto_click_izquierdo() {
        let _guard = FILTER_LOCK.lock().unwrap();
        let id = filter_id();
        let ev = iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left);
        match bubble_event_filter(iced::Event::Mouse(ev), iced::event::Status::Ignored, id) {
            Some(Message::BubblePress) => {}
            other => panic!("esperaba BubblePress, recibí {other:?}"),
        }
        let ev = iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left);
        match bubble_event_filter(iced::Event::Mouse(ev), iced::event::Status::Ignored, id) {
            Some(Message::BubbleRelease) => {}
            other => panic!("esperaba BubbleRelease, recibí {other:?}"),
        }
    }

    #[test]
    fn filtro_ignora_otras_ventanas() {
        let _guard = FILTER_LOCK.lock().unwrap();
        filter_id(); // el evento se filtra contra `other`, no contra la burbuja
        let other = window::Id::unique();
        let ev = iced::mouse::Event::ButtonPressed(iced::mouse::Button::Right);
        assert!(
            matches!(
                bubble_event_filter(iced::Event::Mouse(ev), iced::event::Status::Ignored, other),
                None
            ),
            "evento de otra ventana debe ignorarse"
        );
        let ev = iced::mouse::Event::CursorLeft;
        assert!(
            matches!(
                bubble_event_filter(iced::Event::Mouse(ev), iced::event::Status::Ignored, other),
                None
            ),
            "evento de otra ventana debe ignorarse"
        );
    }

    #[test]
    fn umbral_de_arrastre_solo_supera_7px() {
        use iced::Point;
        // Por debajo o igual al umbral: no es arrastre (clic con temblor).
        assert!(!is_drag_movement(Point::ORIGIN, Point::ORIGIN));
        assert!(!is_drag_movement(Point::ORIGIN, Point::new(4.0, 4.0))); // 5.7 px
        assert!(!is_drag_movement(Point::ORIGIN, Point::new(7.0, 0.0))); // exacto
                                                                         // Por encima: arrastre.
        assert!(is_drag_movement(Point::ORIGIN, Point::new(8.0, 0.0)));
        assert!(is_drag_movement(Point::ORIGIN, Point::new(5.0, 6.0))); // 7.8 px
        assert!(is_drag_movement(Point::new(10.0, 10.0), Point::ORIGIN));
    }

    #[test]
    fn click_derecho_simple_no_arrastra_y_no_deja_estado() {
        let mut s = RightDragState::default();
        s.press(Point::ORIGIN);
        // Soltar sin mover: no fue arrastre y el estado queda limpio.
        assert!(!s.release(), "el clic simple no debe reportar arrastre");
        assert!(!s.pressed);
        assert!(!s.dragging);
    }

    #[test]
    fn movimiento_por_encima_del_umbral_arranca_el_arrastre() {
        let mut s = RightDragState::default();
        s.press(Point::ORIGIN);
        // Movimiento por debajo del umbral: sigue armado, sin arrastrar.
        assert!(!s.move_cursor(Point::new(3.0, 3.0)));
        assert!(s.pressed);
        assert!(!s.dragging);
        // Cruza el umbral: arranque (true solo una vez).
        assert!(s.move_cursor(Point::new(9.0, 0.0)));
        assert!(s.dragging);
        assert!(!s.move_cursor(Point::new(40.0, 40.0)), "ya arrastrando");
        assert!(s.dragging);
        // Soltar: reporta el arrastre y limpia todo el estado.
        assert!(s.release());
        assert!(!s.pressed);
        assert!(!s.dragging);
    }

    #[test]
    fn estado_limpio_sin_press_previo() {
        let mut s = RightDragState::default();
        assert!(!s.move_cursor(Point::new(50.0, 50.0)));
        assert!(!s.release());
        assert!(!s.pressed && !s.dragging);
    }

    #[test]
    fn cancel_limpia_un_arrastre_en_curso() {
        let mut s = RightDragState::default();
        s.press(Point::ORIGIN);
        s.move_cursor(Point::new(30.0, 0.0));
        assert!(s.dragging);
        s.cancel();
        assert!(!s.pressed);
        assert!(!s.dragging);
    }

    // ── drag_target: anclaje absoluto + clamp (núcleo del fix del salto) ──

    const TAM: (i32, i32) = (56, 56);
    const WORK_1080P: (i32, i32, i32, i32) = (0, 0, 1920, 1040);

    #[test]
    fn drag_target_sigue_al_cursor_desde_el_agarre() {
        // La esquina de la ventana se mueve 1:1 con el cursor a partir del
        // punto de agarre (grab_window, grab_cursor).
        assert_eq!(
            drag_target((100, 100), (120, 128), (650, 500), TAM, WORK_1080P),
            (630, 472)
        );
        // El agarre puede estar en cualquier punto del círculo: la esquina
        // conserva el mismo offset relativo al cursor.
        assert_eq!(
            drag_target((100, 100), (150, 150), (500, 500), TAM, WORK_1080P),
            (450, 450)
        );
    }

    #[test]
    fn drag_target_clamp_mantiene_min_visible() {
        // Cursor lejos a la derecha/abajo: la burbuja conserva MIN_VISIBLE_PX.
        assert_eq!(
            drag_target((0, 0), (0, 0), (99999, 99999), TAM, WORK_1080P),
            (1920 - MIN_VISIBLE_PX, 1040 - MIN_VISIBLE_PX)
        );
        // Cursor lejos a la izquierda/arriba: entra completa en el área (el
        // clamp inferior deja el borde derecho/tamaño tocando el área).
        assert_eq!(
            drag_target((0, 0), (0, 0), (-99999, -99999), TAM, WORK_1080P),
            (0 - 56 + MIN_VISIBLE_PX, 0 - 56 + MIN_VISIBLE_PX)
        );
        // En el centro no hay clamp: posiciones normales.
        assert_eq!(
            drag_target((0, 0), (0, 0), (800, 500), TAM, WORK_1080P),
            (800, 500)
        );
    }

    #[test]
    fn drag_target_no_acumula_error_tras_un_clamp() {
        // Propiedad clave del anclaje absoluto: el destino depende SOLO del
        // cursor actual, no del historial. El enfoque anterior (`last + dx`)
        // acumulaba el error del clamp: si el cursor chocaba contra el borde
        // (la ventana se quedaba clavada) y luego volvía, el delta de retorno
        // incluía la distancia «perdida» y la burbuja saltaba de golpe.
        // Historia 1: el cursor choca a la derecha (ventana clavada en el
        // clamp) y luego vuelve a (800, 500).
        let clamped = drag_target((0, 0), (0, 0), (5000, 500), TAM, WORK_1080P);
        assert_eq!(clamped.0, 1920 - MIN_VISIBLE_PX, "clavada en el borde");
        let back = drag_target((0, 0), (0, 0), (800, 500), TAM, WORK_1080P);
        // Historia 2: el mismo cursor final sin haber pasado por el clamp.
        let direct = drag_target((0, 0), (0, 0), (800, 500), TAM, WORK_1080P);
        assert_eq!(
            back, direct,
            "sin error acumulado: mismo cursor ⇒ misma posición"
        );
        assert_eq!(back, (800, 500));
    }

    #[test]
    fn drag_target_ancla_respecto_al_agarre_aunque_se_pierdan_eventos() {
        // Si el bucle se congela (captura robada) y se pierden movimientos,
        // el siguiente follow recalcula desde el agarre original: la burbuja
        // aterriza donde debe, no donde la dejó el último evento.
        let ancla = drag_target((400, 300), (500, 400), (700, 600), TAM, WORK_1080P);
        assert_eq!(ancla, (600, 500));
        // Tras un hueco de 300 px de cursor (eventos nunca vistos), el primer
        // follow pone la ventana exactamente 100 px arriba-izquierda del cursor.
        let rescate = drag_target((400, 300), (500, 400), (1000, 700), TAM, WORK_1080P);
        assert_eq!(rescate, (900, 600));
    }
}
