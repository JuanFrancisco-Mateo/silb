//! Barra de pestañas compartida por Ajustes y la ventana principal.
//!
//! Antes de esta pieza cada superficie dibujaba su propia fila de pestañas con
//! estilos duplicados en línea. Aquí vive una sola implementación: la pestaña
//! activa lleva el indicador `●`, fondo `surface_alt` y borde de acento (todo
//! desde `theme`, cero literales mágicos), y las inactivas el botón secundario
//! de siempre. Así la ventana principal habla el mismo idioma que Ajustes
//! («secciones claras, no una columna de cosas»).
//!
//! `●` no existe en la fuente por defecto de Windows: la etiqueta se dibuja con
//! [`crate::app::ui_text`] (`Shaping::Advanced`) para que el fallback de fuente
//! emoji lo resuelva.

use iced::widget::{button, container, row};
use iced::{Element, Length};

use crate::app::{ui_text, Message};
use crate::theme;

/// Una pestaña: etiqueta legible, mensaje que emite al pulsarla y si es la
/// sección activa.
pub struct Tab {
    /// Texto visible (en español, sin iconos: accesible sin color).
    pub label: String,
    /// Mensaje que se emite al pulsarla.
    pub message: Message,
    /// ¿Es la sección activa?
    pub active: bool,
}

impl Tab {
    pub fn new(label: impl Into<String>, message: Message, active: bool) -> Tab {
        Tab {
            label: label.into(),
            message,
            active,
        }
    }
}

/// Dibuja la fila de pestañas a partir de sus `Tab`. El ancho es `Fill` para
/// que la fila ocupe el ancho de su contenedor (Ajustes y la ventana principal
/// la alinean igual).
pub fn tab_bar<'a>(tabs: Vec<Tab>, fs: theme::FontSizes) -> Element<'a, Message> {
    let mut bar = row![].spacing(theme::SPACE_1).align_y(iced::Alignment::Center);
    for tab in tabs {
        let active = tab.active;
        let label = if active {
            format!("● {}", tab.label)
        } else {
            tab.label
        };
        let item: Element<'a, Message> = button(ui_text(label).size(fs.sm))
            .on_press(tab.message)
            .style(move |th, status| theme::tab(th, status, active))
            .into();
        bar = bar.push(item);
    }
    container(bar)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 0.0,
            bottom: theme::SPACE_1 as f32,
            left: 0.0,
            right: 0.0,
        })
        .into()
}
