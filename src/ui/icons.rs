//! Iconos del rediseño (pantalla Dictado de Figma).
//!
//! Los 19 SVG del marco se sacaron de Figma
//! (`design/openwhispr-figma/raw/icons/`) y se copiaron a `assets/icons/`;
//! aquí van dentro del binario con `include_bytes!` (sin abrir ficheros ni
//! depender de rutas). Son iconos lucide de un color: el color viene DENTRO
//! del SVG (`stroke="white"` en los que van sobre el acento, `#A9A8B5`/`#7F808D`
//! en los demás), así que no hay que teñir nada desde el código: el color de
//! cada icono es parte del diseño.
//!
//! El control `svg` de iced dibuja el vector al tamaño pedido, así que el
//! mismo icono vale a 12, 15, 18 o 30 px sin pixelarse.
//! un mismo icono sirve a 12, 15, 18 o 30 px sin pixelarse.

use iced::widget::{svg, Svg};
use iced::Color;

// ── Barra de ventana ────────────────────────────────────────────────────────
pub const MINUS: &[u8] = include_bytes!("../../assets/icons/minus-98f6e.svg");
pub const SQUARE: &[u8] = include_bytes!("../../assets/icons/square-22899.svg");
pub const X: &[u8] = include_bytes!("../../assets/icons/x-3d4bd.svg");

// ── Marca y navegación ──────────────────────────────────────────────────────
pub const WHISPR_SYMBOL: &[u8] = include_bytes!("../../assets/icons/whisprsymbol-7fe91.svg");
pub const AUDIO_LINES: &[u8] = include_bytes!("../../assets/icons/audiolines-661af.svg");
pub const SLIDERS: &[u8] = include_bytes!("../../assets/icons/slidershorizontal-5bb3a.svg");
pub const SLIDERS_SM: &[u8] = include_bytes!("../../assets/icons/slidershorizontal1-11748.svg");
pub const CIRCLE_HELP: &[u8] = include_bytes!("../../assets/icons/circlehelp-15650.svg");

// ── Tarjeta de grabación y transcripción ────────────────────────────────────
pub const MIC: &[u8] = include_bytes!("../../assets/icons/mic-d85a7.svg");
pub const MIC_FIELD: &[u8] = include_bytes!("../../assets/icons/mic-76fb7.svg");
pub const STATUS_DOT: &[u8] = include_bytes!("../../assets/icons/statusindicator-81828.svg");
pub const CHEVRON_DOWN: &[u8] = include_bytes!("../../assets/icons/chevrondown-249a1.svg");
pub const LANGUAGES: &[u8] = include_bytes!("../../assets/icons/languages-33cf3.svg");
pub const CPU: &[u8] = include_bytes!("../../assets/icons/cpu-c7217.svg");
/// Teclado del pie de Ajustes (`Atajo de dictado`, 15 px).
pub const KEYBOARD: &[u8] = include_bytes!("../../assets/icons/keyboard-0ab31.svg");
/// Check del pie de Ajustes (`Cambios guardados`, 13 px).
pub const CHECK: &[u8] = include_bytes!("../../assets/icons/check1-0d81a.svg");
pub const TRASH: &[u8] = include_bytes!("../../assets/icons/trash2-7f4ec.svg");
pub const COPY: &[u8] = include_bytes!("../../assets/icons/copy-43812.svg");
pub const PENCIL: &[u8] = include_bytes!("../../assets/icons/pencil-d24ce.svg");
pub const SPARKLES: &[u8] = include_bytes!("../../assets/icons/sparkles-db3e2.svg");
pub const HARD_DRIVE: &[u8] = include_bytes!("../../assets/icons/harddrive-1d1c3.svg");

/// Icono vectorial cuadrado de `size` px. El [`Handle`] apunta a los bytes
/// metidos en el binario: no hay fichero que abrir ni ruta que pueda faltar.
///
/// [`Handle`]: iced::widget::svg::Handle
pub fn icon(bytes: &'static [u8], size: f32) -> Svg<'static, iced::Theme> {
    svg(svg::Handle::from_memory(bytes))
        .width(size)
        .height(size)
        .content_fit(iced::ContentFit::Contain)
}

/// Icono vectorial teñido con `color`: el filtro de iced cambia el RGB de los
/// píxeles opacos del SVG, así el mismo trazo vale para el icono activo
/// (acento), el inactivo (texto secundario) o el deshabilitado (tenue).
///
/// Es lo que pide el diseño: el icono de «Dictado» va en acento cuando la
/// sección está activa y en secundario cuando no, y los iconos de acciones
/// (`Limpiar`, `Copiar texto`) toman el tono del texto de su botón en vez de
/// quedarse blancos sobre un botón deshabilitado.
pub fn icon_colored(bytes: &'static [u8], size: f32, color: Color) -> Svg<'static, iced::Theme> {
    svg(svg::Handle::from_memory(bytes))
        .width(size)
        .height(size)
        .content_fit(iced::ContentFit::Contain)
        .style(move |_theme, _status| svg::Style { color: Some(color) })
}
