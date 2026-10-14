//! Espacio de trabajo de Ajustes: el frame `3:604` de Figma.
//!
//! Fuente de verdad de medidas y colores: `design/openwhispr-figma/raw/
//! ajustes-design-context.txt` (capa a capa), `metadata-arbol-capas.txt`
//! (coordenadas) y `raw/ajustes.png` (render). Todo el tono sale de
//! [`crate::theme`]: aquí no hay literales de color.
//!
//! Composición (1200×820): cabecera de página, navegación de pestañas (43 px,
//! subrayado de 2 px en la activa), contenido de la pestaña activa con scroll y
//! el pie con el atajo en keycaps. Las tarjetas de sección y sus filas de
//! preferencia son los primitivos del diseño ([`section_block`],
//! [`design_card`], [`preference_row`], [`wide_row`]).
//!
//! El estado sigue en [`WhisprApp`] (`settings_open`, `settings_section`,
//! `palette_inputs`, …); este módulo solo dibuja. Las claves y los valores de
//! `config` no cambian respecto a la UI anterior: es capa visual y orden.
//!
//! Nota sobre las pestañas: el frame trae `Transcripción · Diccionario
//! personal · Muletillas · General`; la app conserva sus cinco secciones
//! (`General · Apariencia · Dictado · Corrección IA · Avanzado`) porque el
//! reparto de contenido es el de siempre — el diseño aporta la forma, la altura
//! y los estados (ver el cierre de la tarjeta).

use iced::border::Radius;
use iced::font::Weight;
use iced::widget::{
    button, checkbox, column, container, horizontal_rule, pick_list, row, scrollable, slider, text,
    text_input, toggler, Column, Row, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Padding};

use crate::app::{
    auto_server_placeholder, dictionary_list, filler_lists, language_hint, language_label,
    llm_status_ui, ui_text, FillerCategory, Message, SettingsSection, WhisprApp,
};
use crate::config::{
    BubbleCorner, BubbleHideTimeout, BubbleSize, BubbleStyle, CorrectionProvider, FontSize,
    ThemeMode,
};
use crate::theme::{self, Chrome, DesignType, UiPalette};
use crate::transcribe::Model;
use crate::ui::icons;

/// Ancho de los controles de una fila de preferencia (el `Selection` de 250 px
/// del frame Ajustes).
const CONTROL_WIDTH: f32 = 250.0;

/// Alto de la caja de una pestaña de Ajustes (el `Settings navigation` del
/// frame mide 43 px).
const TAB_BOX_HEIGHT: f32 = 43.0;

/// Peso `Medium` para los rótulos del diseño (la app no bundlea Inter: se le
/// pide el peso al family por defecto del sistema, ver `fonts.rs`).
fn medium() -> Font {
    Font {
        weight: Weight::Medium,
        ..Font::DEFAULT
    }
}

/// Rótulo de sección del diseño (12 px, secundario) + su tarjeta, separados
/// por los 10 px que mide el frame.
fn section_block<'a>(
    label: &str,
    card: Element<'a, Message>,
    c: Chrome,
    ty: DesignType,
) -> Element<'a, Message> {
    column![
        text(label.to_string())
            .size(ty.body)
            .font(medium())
            .color(c.secondary),
        card,
    ]
    .spacing(10.0)
    .width(Length::Fill)
    .into()
}

/// Tarjeta del diseño (`#1d1e24` + borde `#303139` + radio 12 = [`theme::card`])
/// con sus filas separadas por el hairline interno del frame (entre filas, ni
/// antes de la primera ni tras la última).
fn design_card<'a>(rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut card = Column::new().width(Length::Fill);
    for (i, row) in rows.into_iter().enumerate() {
        if i > 0 {
            card = card.push(horizontal_rule(1).style(theme::divider_rule));
        }
        card = card.push(row);
    }
    container(card).width(Length::Fill).style(theme::card).into()
}

/// Fila de preferencia del diseño (68 px, px 20 / py 15): rótulo 14 px medium
/// con su ayuda 12 px tenue a la izquierda y el control a la derecha.
fn preference_row<'a>(
    label: &str,
    help: Option<&str>,
    control: Element<'a, Message>,
    c: Chrome,
    ty: DesignType,
) -> Element<'a, Message> {
    let mut left = column![text(label.to_string())
        .size(ty.nav)
        .font(medium())
        .color(c.text)]
    .spacing(4.0)
    .width(Length::Fill);
    if let Some(help) = help {
        left = left.push(text(help.to_string()).size(ty.body).color(c.faint));
    }
    container(
        row![left, control]
            .spacing(20.0)
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fixed(68.0))
    .padding(Padding {
        top: 15.0,
        right: 20.0,
        bottom: 15.0,
        left: 20.0,
    })
    .into()
}

/// Fila de tarjeta sin control a la derecha: el contenido ocupa todo el ancho
/// (listas, formularios y textos largos) con el mismo inset que las filas de
/// preferencia.
fn wide_row<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .padding(Padding {
            top: 15.0,
            right: 20.0,
            bottom: 15.0,
            left: 20.0,
        })
        .into()
}

impl WhisprApp {
    /// Espacio de trabajo de Ajustes (frame `3:604` de Figma): cabecera de
    /// página, navegación de pestañas, contenido de la pestaña activa (con
    /// scroll) y pie con el atajo y el estado de guardado.
    pub(crate) fn settings_view(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let heading = column![
            text("Ajustes").size(ty.title).color(c.text),
            text("Tu forma de dictar, a tu medida.")
                .size(ty.control)
                .color(c.secondary),
        ]
        .spacing(6.0)
        .width(Length::Fill);

        // Solo el contenido de la pestaña activa hace scroll: la cabecera, la
        // navegación y el pie del diseño quedan siempre a la vista.
        let body = container(
            scrollable(self.settings_body(c, ty, t))
                .style(theme::scrollable)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill);

        column![
            heading,
            settings_nav(self.settings_section, c, ty),
            body,
            self.settings_footer(c, ty),
        ]
        .spacing(22.0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// Contenido de la pestaña activa. Cada sección es una función privada: si
    /// en el futuro se mueve un control, se mueve a la sección correcta sin
    /// tocar las demás.
    fn settings_body(&self, c: Chrome, ty: DesignType, t: &UiPalette) -> Element<'_, Message> {
        match self.settings_section {
            SettingsSection::General => self.settings_section_general(c, ty, t),
            SettingsSection::Apariencia => self.settings_section_appearance(c, ty, t),
            SettingsSection::Dictado => self.settings_section_dictation(c, ty, t),
            SettingsSection::CorreccionIA => self.settings_section_correction(c, ty, t),
            SettingsSection::Avanzado => self.settings_section_advanced(c, ty, t),
        }
    }

    /// `Settings footer` del diseño: atajo de dictado en keycaps a la izquierda
    /// y el estado de guardado a la derecha.
    fn settings_footer(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let mut keys = row![].spacing(4.0).align_y(Alignment::Center);
        for key in self.hotkey_label.split('+').map(str::trim) {
            keys = keys.push(
                container(ui_text(key).size(ty.chrome).color(c.secondary))
                    .padding(Padding {
                        top: theme::KEYCAP_PADDING[0],
                        right: theme::KEYCAP_PADDING[1],
                        bottom: theme::KEYCAP_PADDING[0],
                        left: theme::KEYCAP_PADDING[1],
                    })
                    .style(theme::keycap),
            );
        }
        row![
            icons::icon(icons::KEYBOARD, 15.0),
            text("Atajo de dictado").size(ty.chrome).color(c.secondary),
            keys,
            Space::with_width(Length::Fill),
            icons::icon(icons::CHECK, 13.0),
            text("Cambios guardados").size(ty.chrome).color(c.faint),
        ]
        .spacing(10.0)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
    }

    /// ── Sección «General» ────────────────────────────────────────────────
    /// Contenido del frame Ajustes de Figma (la pestaña de transcripción):
    /// «Audio e idioma», «Modelo de voz» y «Al terminar el dictado», más los
    /// bloques que la app tiene aquí desde siempre (diccionario y muletillas).
    /// Solo capa visual: mismas claves y valores de config, mismos mensajes.
    pub(crate) fn settings_section_general(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        let mut col = Column::new().spacing(20.0).width(Length::Fill);

        // ── Audio e idioma ──────────────────────────────────────────────
        // Sin micrófonos detectados no hay nada que elegir: se dice, en vez de
        // pintar un selector vacío (mismo aviso que traía la ayuda de siempre).
        let mic_control: Element<'_, Message> = if self.input_devices.is_empty() {
            text("(sin micrófonos detectados)")
                .size(ty.control)
                .color(c.faint)
                .into()
        } else {
            pick_list(
                self.input_devices.clone(),
                self.config.input_device.clone(),
                Message::InputDeviceSelected,
            )
            .placeholder("(por defecto)")
            .width(Length::Fixed(CONTROL_WIDTH))
            .text_size(ty.control)
            .padding(theme::FIELD_PADDING)
            .style(theme::pick_list)
            .into()
        };
        // El hint de idioma ("default: es", "whisper decide") cuelga del campo
        // como ayuda de la fila: el frame de Ajustes no lo trae, pero es
        // información que la app ya daba.
        let lang_hint = language_hint(&self.config.language);
        let lang_help = if lang_hint.is_empty() {
            None
        } else {
            Some(lang_hint.as_str())
        };
        // Idioma: el getter traduce "auto" a None y cualquier otro valor a
        // Some(BCP-47) que se pasa a whisper.
        let lang_control: Element<'_, Message> = pick_list(
            crate::app::LANGUAGE_OPTIONS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            Some(language_label(&self.config.language)),
            Message::LanguageSelected,
        )
        .placeholder("Idioma")
        .width(Length::Fixed(CONTROL_WIDTH))
        .text_size(ty.control)
        .padding(theme::FIELD_PADDING)
        .style(theme::pick_list)
        .into();
        col = col.push(section_block(
            "Audio e idioma",
            design_card(vec![
                preference_row(
                    "Micrófono",
                    Some("Audio normalizado a 16 kHz."),
                    mic_control,
                    c,
                    ty,
                ),
                preference_row("Idioma de transcripción", lang_help, lang_control, c, ty),
            ]),
            c,
            ty,
        ));

        // ── Modelo de voz ───────────────────────────────────────────────
        // Divergencia con el frame: el diseño muestra el modelo instalado y un
        // botón «Cambiar modelo»; aquí el control sigue siendo el selector de
        // modelos de siempre (no se inventa un flujo de instalación).
        let model = self.config.selected_model();
        let model_label = model.label().to_string();
        let model_size = model.approx_mb();
        let model_control: Element<'_, Message> = pick_list(
            Model::all().to_vec(),
            Some(model),
            Message::ModelSelected,
        )
        .placeholder("Modelo")
        .width(Length::Fixed(CONTROL_WIDTH))
        .text_size(ty.control)
        .padding(theme::FIELD_PADDING)
        .style(theme::pick_list)
        .into();
        let model_tile = container(icons::icon(icons::CPU, 21.0))
            .width(42.0)
            .height(42.0)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_th| container::Style {
                background: Some(Background::Color(c.keycap)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: Radius::from(theme::RADIUS_MD),
                },
                ..container::Style::default()
            });
        let model_row = container(
            row![
                model_tile,
                column![
                    text(model_label).size(ty.nav).font(medium()).color(c.text),
                    text(format!("{model_size} MB · Se ejecuta sin conexión"))
                        .size(ty.body)
                        .color(c.faint),
                ]
                .spacing(5.0)
                .width(Length::Fill),
                model_control,
            ]
            .spacing(16.0)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding(20.0);
        col = col.push(section_block(
            "Modelo de voz",
            design_card(vec![model_row.into()]),
            c,
            ty,
        ));

        // ── Al terminar el dictado ──────────────────────────────────────
        let ai_enabled = self.config.ai_correction_enabled();
        let ai_control: Element<'_, Message> = row![
            text(if ai_enabled { "Activada" } else { "Desactivada" })
                .size(ty.chrome)
                .color(c.faint),
            toggler(ai_enabled)
                .on_toggle(Message::ToggleAiCorrection)
                .size(22.0)
                .style(theme::toggler),
        ]
        .spacing(20.0)
        .align_y(Alignment::Center)
        .into();
        let cleanup_control: Element<'_, Message> = toggler(self.config.cleanup_enabled())
            .on_toggle(Message::ToggleCleanup)
            .size(22.0)
            .style(theme::toggler)
            .into();
        let inject_control: Element<'_, Message> = toggler(self.config.auto_inject.unwrap_or(true))
            .on_toggle(Message::ToggleAutoInject)
            .size(22.0)
            .style(theme::toggler)
            .into();
        col = col.push(section_block(
            "Al terminar el dictado",
            design_card(vec![
                preference_row(
                    "Corrección IA",
                    Some("Corrige el texto con un modelo local. Opcional."),
                    ai_control,
                    c,
                    ty,
                ),
                preference_row(
                    "Limpiar muletillas",
                    Some("Elimina «eh», «em» y las expresiones que elijas."),
                    cleanup_control,
                    c,
                    ty,
                ),
                preference_row(
                    "Autoinyectar al terminar",
                    Some("Escribe el texto en la app activa al terminar el turno."),
                    inject_control,
                    c,
                    ty,
                ),
            ]),
            c,
            ty,
        ));

        // ── Diccionario personal ────────────────────────────────────────
        let entry_row = row![
            text_input("término", &self.entry_from)
                .on_input(Message::EntryFromChanged)
                .on_submit(Message::AddEntry)
                .style(theme::text_input),
            text_input("reemplazo", &self.entry_to)
                .on_input(Message::EntryToChanged)
                .on_submit(Message::AddEntry)
                .style(theme::text_input),
            button("Añadir")
                .on_press(Message::AddEntry)
                .style(theme::primary),
        ]
        .spacing(theme::SPACE_2);
        let mut dict_rows = vec![
            wide_row(entry_row.into()),
            wide_row(dictionary_list(&self.config.dictionary, fs)),
            wide_row(
                row![
                    Space::with_width(Length::Fill),
                    button("Importar")
                        .on_press(Message::ImportDictionary)
                        .style(theme::secondary),
                    button("Exportar")
                        .on_press(Message::ExportDictionary)
                        .style(theme::secondary),
                ]
                .spacing(theme::SPACE_2)
                .into(),
            ),
        ];
        // Feedback de import/export: vive aquí para que sea visible
        // mientras la sección General esté abierta.
        if let Some(corr) = &self.last_correction {
            if corr.contains("Diccionario") {
                dict_rows.push(wide_row(text(corr).size(fs.sm).color(t.ok).into()));
            }
        }
        col = col.push(section_block(
            "Diccionario personal",
            design_card(dict_rows),
            c,
            ty,
        ));

        // ── Muletillas ──────────────────────────────────────────────────
        let filler_row = row![
            text_input("muletilla (ej. «vale»)", &self.filler_input)
                .on_input(Message::FillerInputChanged)
                .on_submit(Message::AddFiller)
                .style(theme::text_input),
            pick_list(
                FillerCategory::ALL.to_vec(),
                Some(self.filler_category),
                Message::FillerCategorySelected,
            )
            .placeholder("Categoría")
            .width(Length::Fixed(140.0))
            .text_size(ty.control)
            .padding(theme::FIELD_PADDING)
            .style(theme::pick_list),
            button("Añadir")
                .on_press(Message::AddFiller)
                .style(theme::primary),
        ]
        .spacing(theme::SPACE_2);
        let mut filler_rows = vec![
            wide_row(filler_row.into()),
            wide_row(filler_lists(&self.config.fillers(), fs)),
        ];
        // Botón «Por defecto»: solo aparece si el usuario ha personalizado las
        // listas (config.fillers = Some).
        if self.config.fillers.is_some() {
            filler_rows.push(wide_row(
                row![button("Por defecto")
                    .on_press(Message::ResetFillers)
                    .style(theme::secondary)]
                .into(),
            ));
        }
        col = col.push(section_block("Muletillas", design_card(filler_rows), c, ty));
        col.into()
    }

    /// ── Sección «Apariencia» ────────────────────────────────────────────
    /// Tema (global) y todo lo relativo a la burbuja flotante: estilo,
    /// tamaño, esquina, opacidad y auto-ocultar. La fuente va aquí porque
    /// es un ajuste visual y la regla de producto dice que cada opción
    /// técnica necesita label claro + ayuda corta, y la fuente es el
    /// ajuste visual más impactante de la app.
    pub(crate) fn settings_section_appearance(
        &self,
        _c: Chrome,
        _ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        let mut col = Column::new()
            .spacing(theme::SPACE_2)
            .width(Length::Fill);
        col = col.push(
            text("Tema")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );
        col = col.push(
            row![
                text("Tema")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [ThemeMode::Auto, ThemeMode::Claro, ThemeMode::Oscuro].to_vec(),
                    Some(self.config.theme_mode()),
                    Message::ThemeModeChanged,
                )
                .placeholder("Tema")
                .width(Length::Fill)
                .style(theme::pick_list),
                if self.config.theme_mode() == ThemeMode::Auto {
                    text("sigue a Windows").size(fs.xxs).color(t.dim)
                } else {
                    text("").size(fs.xxs).color(t.dim)
                },
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            row![
                text("Fuente")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [
                        FontSize::Small,
                        FontSize::Normal,
                        FontSize::Large,
                        FontSize::XLarge,
                    ]
                    .to_vec(),
                    Some(self.config.font_size()),
                    Message::FontSizeChanged,
                )
                .placeholder("Tamaño de fuente")
                .width(Length::Fill)
                .style(theme::pick_list),
                text(format!("{}×", self.config.font_size().factor()))
                    .size(fs.xxs)
                    .color(t.dim),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text("Burbuja flotante")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );
        col = col.push(
            row![
                text("Estilo")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [BubbleStyle::Solid, BubbleStyle::Ring, BubbleStyle::Minimal].to_vec(),
                    Some(self.config.bubble_style()),
                    Message::BubbleStyleChanged,
                )
                .placeholder("Estilo")
                .width(Length::Fill)
                .style(theme::pick_list),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            row![
                text("Tamaño")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [BubbleSize::Small, BubbleSize::Mid, BubbleSize::Large].to_vec(),
                    Some(self.config.bubble_size()),
                    Message::BubbleSizeChanged,
                )
                .placeholder("Tamaño")
                .width(Length::Fill)
                .style(theme::pick_list),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            row![
                text("Esquina")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [
                        BubbleCorner::TopRight,
                        BubbleCorner::TopLeft,
                        BubbleCorner::BottomRight,
                        BubbleCorner::BottomLeft,
                    ]
                    .to_vec(),
                    Some(self.config.bubble_corner()),
                    Message::BubbleCornerChanged,
                )
                .placeholder("Esquina")
                .width(Length::Fill)
                .style(theme::pick_list),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        // Opacidad: slider 20–100 %; el getter clampea aunque el slider
        // nunca emita fuera de rango.
        let op = u32::from(self.config.bubble_opacity());
        col = col.push(
            row![
                text(format!("Opacidad: {op}%"))
                    .size(fs.sm)
                    .color(t.text)
                    .width(Length::Fixed(120.0)),
                slider(20..=100, op, Message::BubbleOpacityChanged)
                    .step(1u32)
                    .width(Length::Fill),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            row![
                text("Auto-ocultar")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [
                        BubbleHideTimeout::Never,
                        BubbleHideTimeout::S5,
                        BubbleHideTimeout::S10,
                        BubbleHideTimeout::S30,
                        BubbleHideTimeout::S60,
                    ]
                    .to_vec(),
                    Some(self.config.bubble_hide_timeout()),
                    Message::BubbleHideTimeoutChanged,
                )
                .placeholder("Auto-ocultar")
                .width(Length::Fill)
                .style(theme::pick_list),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        // ── Personalización III: imágenes del usuario ──────────────────────
        col = col.push(self.settings_section_images(t, fs));
        // ── Personalización II: paleta editable ─────────────────────────────
        col = col.push(self.settings_section_palette(t, fs));
        // El diseño del frame Ajustes no define esta pestaña: se le da la
        // misma tarjeta que a las secciones del frame y su contenido se queda
        // como estaba.
        design_card(vec![wide_row(col.into())])
    }

    /// Bloque «Imágenes» dentro de Apariencia (Personalización III): el
    /// usuario puede elegir su propia imagen de fondo de la ventana y un
    /// sprite (estático o animado por frames) para la burbuja. Las rutas
    /// se persisten como dato (nunca hardcoded); si fallan al renderizar,
    /// el getter correspondiente devuelve `None` y la app pinta el
    /// render por defecto (sin ventana negra ni crash). El botón
    /// «Quitar» de cada control limpia la ruta en `config.json` y deja
    /// el aspecto original.
    pub(crate) fn settings_section_images(
        &self,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Element<'_, Message> {
        let mut col = Column::new()
            .spacing(theme::SPACE_2)
            .width(Length::Fill);

        // Título.
        col = col.push(
            text("Imágenes")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );
        // Subtítulo: la regla del producto «toda opción técnica necesita
        // label claro + ayuda corta». Lo técnico vive detrás de
        // «Ajustes avanzados», pero aquí la regla también aplica
        // (el formato de frames es una decisión de producto, no de
        // motor).
        col = col.push(
            text("Tu propia imagen de fondo y un sprite (estático o \
                  animado por frames) para la burbuja. Si el archivo \
                  no existe o no es una imagen soportada, la app \
                  vuelve al aspecto original.")
                .size(fs.xxs)
                .color(t.dim),
        );

        // ── Imagen de fondo ──────────────────────────────────────────────
        let bg_label = match self.config.bg_image_path() {
            Some(p) => p,
            None => "(sin imagen; usando el fondo del tema)",
        };
        col = col.push(
            row![
                text("Fondo de la ventana")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(160.0)),
                button(text("Elegir…").size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::BgImageBrowse)
                    .style(theme::secondary),
                button(text("Quitar").size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::BgImageClear)
                    .style(theme::secondary),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(format!("Ruta actual: {}", bg_label))
                .size(fs.xxs)
                .color(t.dim),
        );

        // ── Sprite estático de la burbuja ─────────────────────────────────
        let bubble_img_label = match self.config.bubble_image_path() {
            Some(p) => p,
            None => "(sin sprite; usando el render vectorial)",
        };
        col = col.push(
            row![
                text("Sprite de la burbuja")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(160.0)),
                button(text("Elegir…").size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::BubbleImageBrowse)
                    .style(theme::secondary),
                button(text("Quitar").size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::BubbleImageClear)
                    .style(theme::secondary),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(format!("Ruta actual: {}", bubble_img_label))
                .size(fs.xxs)
                .color(t.dim),
        );

        // ── Carpeta de frames (animación) ─────────────────────────────────
        let frames_label = match self.config.bubble_frames_path() {
            Some(p) => p,
            None => "(sin carpeta; solo sprite estático)",
        };
        col = col.push(
            row![
                text("Frames de animación")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(160.0)),
                button(text("Elegir carpeta…").size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::BubbleFramesBrowse)
                    .style(theme::secondary),
                button(text("Quitar").size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::BubbleFramesClear)
                    .style(theme::secondary),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(format!("Ruta actual: {}", frames_label))
                .size(fs.xxs)
                .color(t.dim),
        );

        // ── FPS de la animación ───────────────────────────────────────────
        let fps = self.config.bubble_animation_fps();
        col = col.push(
            row![
                text(format!("Velocidad (FPS): {fps}"))
                    .size(fs.sm)
                    .color(t.text)
                    .width(Length::Fixed(180.0)),
                slider(
                    u32::from(crate::custom_image::MIN_BUBBLE_FPS)
                        ..=u32::from(crate::custom_image::MAX_BUBBLE_FPS),
                    u32::from(fps),
                    Message::BubbleFpsChanged,
                )
                .step(1u32)
                .width(Length::Fill),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text("Frames por segundo de la animación de la burbuja \
                  (rango 1–30; el default es 12). Cambiar la ruta o \
                  pulsar «Quitar» detiene la animación sin coste extra.")
                .size(fs.xxs)
                .color(t.dim),
        );

        col.into()
    }

    /// Bloque «Paleta» dentro de la sección Apariencia (R7-II). Muestra
    /// presets de un clic, una fila por campo editable (hex input + swatch
    /// de previsualización + botón «Restaurar»), y botones de Importar /
    /// Exportar a JSON. El aviso de contraste WCAG aparece bajo el campo
    /// correspondiente cuando texto/principal baja de 3:1 sobre el fondo
    /// activo (no bloquea: solo avisa).
    pub(crate) fn settings_section_palette(
        &self,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Element<'_, Message> {
        let mut col = Column::new()
            .spacing(theme::SPACE_2)
            .width(Length::Fill);

        // Título de la sub-sección.
        col = col.push(
            text("Paleta")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );

        // Subtítulo explicativo.
        col = col.push(
            text("Cambia cada color por separado o usa un preset. Hex \
                  «#RRGGBB» o «#RGB»; los valores no válidos se ignoran.")
                .size(fs.xxs)
                .color(t.dim),
        );

        // Presets (un clic = aplica la paleta entera al modo activo).
        col = col.push(self.palette_preset_buttons(fs, t));

        // Cabecera de la tabla de campos.
        col = col.push(
            text("Campos")
                .size(fs.md)
                .color(t.text),
        );

        // 12 filas: una por PaletteField.
        for field in theme::PaletteField::ALL {
            col = col.push(self.palette_field_row(*field, t, fs));
        }

        // Acciones globales: Importar / Restaurar todo.
        col = col.push(self.palette_actions_row(t, fs));

        col.into()
    }

    /// Botonera horizontal con los 5 presets. Cada botón aplica la paleta
    /// resuelta del preset sobre el modo activo (DARK o LIGHT según el
    /// `ThemeMode` del usuario en este momento).
    pub(crate) fn palette_preset_buttons(
        &self,
        fs: theme::FontSizes,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let mut presets_row = Row::new()
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center);
        for preset in theme::PalettePreset::ALL {
            let label = preset.label();
            let preset_copy = *preset;
            presets_row = presets_row.push(
                button(text(label).size(fs.xxs))
                    .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                    .on_press(Message::PalettePresetApply(preset_copy))
                    .style(theme::secondary),
            );
        }
        // Swatch预览: muestra el accent del modo activo como
        // recordatorio visual («el preset tocará esto»).
        let swatch = container(Space::new(Length::Fixed(16.0), Length::Fixed(16.0)))
            .style(move |_th| {
                let p = self.resolved_palette();
                container::Style {
                    background: Some(Background::Color(p.accent)),
                    border: Border {
                        color: p.border,
                        width: 1.0,
                        radius: Radius::from(3.0),
                    },
                    ..Default::default()
                }
            });
        let _ = t; // t solo se usa en la cabecera (no en esta sub-helper)
        Row::new()
            .push(presets_row)
            .push(Space::new(
                Length::Fixed(theme::SPACE_2 as f32),
                Length::Fixed(0.0),
            ))
            .push(swatch)
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center)
            .into()
    }

    /// Una fila de la tabla de campos: label + input hex + swatch + botón
    /// restaurar + (si aplica) aviso de contraste WCAG.
    pub(crate) fn palette_field_row(
        &self,
        field: theme::PaletteField,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Element<'_, Message> {
        // Texto actual del input: lo que el usuario está tecleando, o el
        // hex persistido si no hay buffer, o vacío (= default).
        let current_text = self
            .palette_inputs
            .get(&field)
            .cloned()
            .or_else(|| self.palette_field_persisted(field))
            .unwrap_or_default();

        // Color que se previsualiza en el swatch:
        //   - si el input tiene un hex parseable, ese color;
        //   - si no, el color resuelto (override o default).
        let swatch_color = theme::parse_hex(&current_text)
            .unwrap_or_else(|| self.resolved_color_for_field(field));

        let contrast_warn = self.contrast_warning_for(field, swatch_color);

        let label_text = text(field.label())
            .size(fs.sm)
            .color(t.text)
            .width(Length::Fixed(150.0));

        let input_box: Element<'_, Message> = text_input(
            field.help(),
            &current_text,
        )
        .on_input(move |v| Message::PaletteFieldChanged(field, v))
        .on_submit(Message::PaletteFieldApply(field))
        .padding(theme::SPACE_2 as f32)
        .width(Length::Fixed(120.0))
        .style(theme::text_input)
        .into();

        let apply_btn = button(text("Aplicar").size(fs.xxs))
            .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
            .on_press(Message::PaletteFieldApply(field))
            .style(theme::primary);

        let reset_btn = button(text("Restaurar").size(fs.xxs))
            .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
            .on_press(Message::PaletteFieldReset(field))
            .style(theme::secondary);

        // Swatch: muestra el color actual (resolved o en-vuelo).
        let border_color = t.border;
        let swatch_color_for_closure = swatch_color;
        let swatch = container(
            Space::new(
                Length::Fixed(20.0),
                Length::Fixed(20.0),
            ),
        )
        .style(move |_th| container::Style {
            background: Some(Background::Color(swatch_color_for_closure)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Radius::from(4.0),
            },
            ..Default::default()
        });

        let mut row = Row::new()
            .push(label_text)
            .push(input_box)
            .push(Space::new(
                Length::Fixed(theme::SPACE_2 as f32),
                Length::Fixed(0.0),
            ))
            .push(apply_btn)
            .push(reset_btn)
            .push(Space::new(
                Length::Fixed(theme::SPACE_2 as f32),
                Length::Fixed(0.0),
            ))
            .push(swatch)
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center);

        row = if let Some(warn) = contrast_warn {
            row.push(
                text(warn)
                    .size(fs.xxs)
                    .color(t.warn)
                    .width(Length::Fill),
            )
        } else {
            row.push(Space::new(Length::Fill, Length::Fixed(0.0)))
        };

        Column::new()
            .push(row)
            .spacing(theme::SPACE_1 as f32)
            .into()
    }

    /// Devuelve el hex persistido del campo (o None si está en default).
    pub(crate) fn palette_field_persisted(&self, field: theme::PaletteField) -> Option<String> {
        let cfg = self.config.palette.as_ref()?;
        let raw = match field {
            theme::PaletteField::Bg => &cfg.bg,
            theme::PaletteField::Surface => &cfg.surface,
            theme::PaletteField::SurfaceAlt => &cfg.surface_alt,
            theme::PaletteField::Text => &cfg.text,
            theme::PaletteField::Dim => &cfg.dim,
            theme::PaletteField::Accent => &cfg.accent,
            theme::PaletteField::AccentHover => &cfg.accent_hover,
            theme::PaletteField::AccentText => &cfg.accent_text,
            theme::PaletteField::Ok => &cfg.ok,
            theme::PaletteField::Warn => &cfg.warn,
            theme::PaletteField::Err => &cfg.err,
            theme::PaletteField::Border => &cfg.border,
        };
        raw.clone()
    }

    /// Devuelve el color resuelto (con overrides aplicados) para un campo.
    pub(crate) fn resolved_color_for_field(&self, field: theme::PaletteField) -> Color {
        let p = self.resolved_palette();
        match field {
            theme::PaletteField::Bg => p.bg,
            theme::PaletteField::Surface => p.surface,
            theme::PaletteField::SurfaceAlt => p.surface_alt,
            theme::PaletteField::Text => p.text,
            theme::PaletteField::Dim => p.dim,
            theme::PaletteField::Accent => p.accent,
            theme::PaletteField::AccentHover => p.accent_hover,
            theme::PaletteField::AccentText => p.accent_text,
            theme::PaletteField::Ok => p.ok,
            theme::PaletteField::Warn => p.warn,
            theme::PaletteField::Err => p.err,
            theme::PaletteField::Border => p.border,
        }
    }

    /// Si el campo es «texto» o «dim» y su contraste contra el fondo activo
    /// es < 3:1, devuelve un mensaje de aviso (None en caso contrario).
    /// El requisito es no bloquear al usuario: solo avisa.
    pub(crate) fn contrast_warning_for(
        &self,
        field: theme::PaletteField,
        candidate: Color,
    ) -> Option<String> {
        let bg = self.resolved_palette().bg;
        let ratio = theme::contrast_ratio(candidate, bg);
        let must_warn = matches!(
            field,
            theme::PaletteField::Text | theme::PaletteField::Dim
        ) && ratio < 3.0;
        if must_warn {
            Some(format!(
                "⚠ Contraste {ratio:.2}:1 (texto sobre fondo, mínimo WCAG 3:1). \
                 Se aplica igualmente; ajusta hex o restaura el campo."
            ))
        } else {
            None
        }
    }

    /// Botones «Importar…» / «Exportar…» / «Restaurar todo».
    pub(crate) fn palette_actions_row(
        &self,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Element<'_, Message> {
        let import_btn = button(text("Importar JSON…").size(fs.sm))
            .padding([theme::SPACE_2 as f32, theme::SPACE_3 as f32])
            .on_press(Message::PaletteImport)
            .style(theme::secondary);
        let export_btn = button(text("Exportar JSON…").size(fs.sm))
            .padding([theme::SPACE_2 as f32, theme::SPACE_3 as f32])
            .on_press(Message::PaletteExport)
            .style(theme::secondary);
        let reset_all_btn = button(text("Restaurar todo").size(fs.sm))
            .padding([theme::SPACE_2 as f32, theme::SPACE_3 as f32])
            .on_press(Message::PaletteResetAll)
            .style(theme::secondary);
        Column::new()
            .push(
                Row::new()
                    .push(import_btn)
                    .push(Space::new(
                        Length::Fixed(theme::SPACE_2 as f32),
                        Length::Fixed(0.0),
                    ))
                    .push(export_btn)
                    .push(Space::new(Length::Fill, Length::Fixed(0.0)))
                    .push(reset_all_btn)
                    .spacing(theme::SPACE_2)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                text("Importar funde los campos presentes del JSON con tu paleta \
                      actual (los ausentes se mantienen). Exportar guarda la la palette \
                      RESUELTA (con defaults ya aplicados) para que sea \
                      directamente compartible.")
                    .size(fs.xxs)
                    .color(t.dim),
            )
            .spacing(theme::SPACE_2)
            .into()
    }

    /// ── Sección «Dictado» ────────────────────────────────────────────────
    /// Calidad (beam size), hilos de CPU y resumen informativo de la
    /// inyección (formato, retardo y filtro de ventana). Lo técnico del
    /// motor de transcripción vive aquí: si el usuario nunca toca la IA,
    /// no lo verá al pasar por Corrección IA.
    pub(crate) fn settings_section_dictation(
        &self,
        _c: Chrome,
        _ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        let beam = self.config.beam_size();
        let threads = self.config.n_threads();
        let beam_row = row![
            text(format!("Calidad: {beam}"))
                .size(fs.sm)
                .color(t.text)
                .width(Length::Fixed(120.0)),
            slider(1..=8, beam, Message::BeamSizeChanged)
                .step(1u32)
                .width(Length::Fill),
        ]
        .spacing(theme::SPACE_2)
        .align_y(iced::Alignment::Center);
        let threads_row = row![
            text(format!("Hilos de CPU: {threads}"))
                .size(fs.sm)
                .color(t.text)
                .width(Length::Fixed(120.0)),
            slider(1..=16, threads, Message::ThreadsChanged)
                .step(1u32)
                .width(Length::Fill),
        ]
        .spacing(theme::SPACE_2)
        .align_y(iced::Alignment::Center);

        // Resumen informativo de la inyección (formato / retardo / filtro
        // de ventana activa): sigue siendo informativo aquí, igual que en
        // el antiguo panel. La edición de estos parámetros es interna a
        // `config.inject_settings()`; no se exponen controles en la UI
        // porque están fijados por el producto (requisito UX 10/09/2026).
        let injection_label = {
            let inj = self.config.inject_settings();
            let destino = match inj.window_filter.as_deref() {
                Some(f) => format!("solo «{f}»"),
                None => "cualquier app".to_string(),
            };
            format!(
                "Inyección: {} · retardo {} ms · {}",
                inj.format.label(),
                inj.delay.as_millis(),
                destino,
            )
        };

        let mut col = Column::new()
            .spacing(theme::SPACE_2)
            .width(Length::Fill);
        col = col.push(
            text("Calidad y rendimiento")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );
        col = col.push(beam_row);
        col = col.push(
            text(
                "Calidad de transcripción (beam size) vs velocidad: \
                 1 = la más rápida, 8 = la más fiel. Rango 1–8, \
                 recomendado: 5."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(threads_row);
        col = col.push(
            text(
                "Núcleos de CPU que usa Whisper al transcribir. \
                 Sube si el equipo va justo de velocidad; baja para \
                 dejar recursos al resto. Rango 1–16, recomendado: 4."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(horizontal_rule(1).style(theme::rule));
        col = col.push(
            text("Inyección en la app activa")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );
        col = col.push(text(injection_label).size(fs.xxs).color(t.dim));
        design_card(vec![wide_row(col.into())])
    }

    /// ── Sección «Corrección IA» ─────────────────────────────────────────
    /// Todo el corrector LLM en un único sitio: estado en vivo, toggle,
    /// proveedor, endpoint, modelo, API key, ruta del server local y los
    /// detalles técnicos (timeout, sensibilidad, confirmar). Es la
    /// pestaña más densa porque el corrector tiene muchas opciones, pero
    /// ahora están agrupadas y no se mezclan con el motor de voz ni con
    /// la apariencia.
    pub(crate) fn settings_section_correction(
        &self,
        _c: Chrome,
        _ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        // Estado en vivo (misma lógica pura que el chip del header):
        // «Corrección IA: activa · <modelo>» o «inactiva — <motivo>».
        let status_label = {
            let ui = llm_status_ui(
                self.config.ai_correction_enabled(),
                &self.llm_status,
                &self.config.llm_settings().model,
            );
            let color = match ui.kind {
                "ok" => t.ok,
                "warn" => t.warn,
                _ => t.dim,
            };
            text(ui.label).size(fs.sm).color(color)
        };

        let mut col = Column::new()
            .spacing(theme::SPACE_2)
            .width(Length::Fill);
        col = col.push(status_label);
        col = col.push(
            checkbox(
                "Corrección IA: una IA revisa y corrige lo que dictas",
                self.config.ai_correction_enabled(),
            )
            .on_toggle(Message::ToggleAiCorrection)
            .style(theme::checkbox),
        );
        col = col.push(
            text(
                "Al terminar un turno, la IA lo revisa y corrige («digo X», «no, \
                 era Y»…). Si el servidor de IA no está disponible, el dictado \
                 se inserta entero, sin corregir."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(
            row![
                text("Proveedor")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                pick_list(
                    [
                        CorrectionProvider::Default,
                        CorrectionProvider::Cloud,
                        CorrectionProvider::Custom,
                    ]
                    .to_vec(),
                    Some(self.config.correction_provider()),
                    Message::CorrectionProviderChanged,
                )
                .placeholder("Proveedor")
                .width(Length::Fill)
                .style(theme::pick_list),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(
                "Quién corrige lo que dictas: Local — llama.cpp, el que viene \
                 con la app (sin internet, recomendado) · Custom LLM — tu \
                 propio endpoint OpenAI-compatible · Cloud — OpenAI. \
                 Por defecto: Local."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(
            row![
                text("Endpoint LLM")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                text_input(
                    "http://127.0.0.1:8081/v1",
                    self.config.llm_base_url.as_deref().unwrap_or(""),
                )
                .on_input(Message::LlmEndpointChanged)
                .style(theme::text_input)
                .width(Length::Fill),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(
                "Dirección del servidor de IA (API compatible con OpenAI). \
                 Local: http://127.0.0.1:8081/v1."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(
            row![
                text("Modelo IA")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                text_input(
                    "ornith-1.0-9b / gpt-4o-mini",
                    self.config.llm_model.as_deref().unwrap_or(""),
                )
                .on_input(Message::LlmModelChanged)
                .style(theme::text_input)
                .width(Length::Fill),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(
                "El modelo que corrige. Debe estar en tu servidor local o ser \
                 uno de los que ofrece la nube."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        // API key: solo con proveedor que la exija (Custom — opcional si
        // el server no la pide — y Cloud, obligatoria). Con Local el
        // server local no la usa. Persistencia en Credential Manager de
        // Windows, NUNCA en config.json.
        if self.config.correction_provider() != CorrectionProvider::Default {
            col = col.push(
                row![
                    text("API key")
                        .size(fs.sm)
                        .color(t.dim)
                        .width(Length::Fixed(96.0)),
                    text_input(
                        "opcional (Custom) · obligatoria (Cloud)",
                        self.config.llm_api_key.as_deref().unwrap_or("")
                    )
                    .on_input(Message::LlmApiKeyChanged)
                    .secure(true)
                    .style(theme::text_input)
                    .width(Length::Fill),
                ]
                .spacing(theme::SPACE_2)
                .align_y(iced::Alignment::Center),
            );
            col = col.push(
                text(
                    "Clave del endpoint si la exige (Custom: opcional · \
                     Cloud: obligatoria). Se guarda en el Administrador \
                     de credenciales de Windows, nunca en la config."
                )
                .size(fs.xxs)
                .color(t.dim),
            );
        }
        col = col.push(
            row![
                text("Server LLM")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                text_input(
                    &auto_server_placeholder(&self.config),
                    self.config.llama_server_path.as_deref().unwrap_or(""),
                )
                .on_input(Message::LlmServerPathChanged)
                .style(theme::text_input)
                .width(Length::Fill),
                button("Examinar…")
                    .on_press(Message::LlmServerPathBrowse)
                    .style(theme::secondary),
                button(ui_text("✕"))
                    .on_press(Message::LlmServerPathReset),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(
                "Ejecutable llama-server.exe que la app lanza al arrancar. \
                 Vacío = autodetección (D:\\LlamaCPP, junto a la app…). Solo \
                 indícalo aquí si la app no lo encuentra sola."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(horizontal_rule(1).style(theme::rule));
        col = col.push(
            text("Detalles técnicos")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
        );
        // Timeout: slider 1..=120 s (25 por defecto). El getter clampea.
        col = col.push(
            row![
                text("Espera máx. (s)")
                    .size(fs.sm)
                    .color(t.dim)
                    .width(Length::Fixed(96.0)),
                slider(
                    1..=120,
                    self.config.llm_timeout_secs().unwrap_or(25).clamp(1, 120) as u32,
                    Message::LlmTimeoutChanged,
                )
                .step(1u32)
                .width(Length::Fill),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(
                "Espera máxima de la IA antes de dar el turno por \
                 definitivo: si lo supera, se inserta sin corregir. \
                 Rango 1–120 s, recomendado: 25 s."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        // Umbral del clasificador: 0..=100. Default 50. Etiqueta llana
        // «sensibilidad» (requisito UX 10/09/2026: «umbral clasif.» no
        // dice nada).
        col = col.push(
            row![
                text(format!("Sensibilidad: {}", self.config.llm_classifier_threshold()))
                    .size(fs.sm)
                    .color(t.text)
                    .width(Length::Fixed(120.0)),
                slider(
                    0..=100,
                    u32::from(self.config.llm_classifier_threshold()),
                    Message::LlmThresholdChanged,
                )
                .step(1u32)
                .width(Length::Fill),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            text(
                "Con qué frecuencia se envía un turno a la IA: 50 = \
                 casi siempre que hay una señal de corrección, 100 = \
                 casi nunca. Rango 0–100, recomendado: 50."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        col = col.push(
            checkbox(
                "Confirmar antes de aplicar",
                self.config.ai_correction_confirm(),
            )
            .on_toggle(Message::LlmConfirmToggled)
            .style(theme::checkbox),
        );
        col = col.push(
            text(
                "Pide tu visto bueno (Aplicar/Descartar) antes de \
                 modificar el texto. Recomendado si prefieres revisar \
                 lo que corrige la IA."
            )
            .size(fs.xxs)
            .color(t.dim),
        );
        design_card(vec![wide_row(col.into())])
    }

    /// ── Sección «Avanzado» ──────────────────────────────────────────────
    /// Reservada para opciones transversales que no encajan en las
    /// secciones anteriores. Tras el refactor por pestañas queda vacía;
    /// se mantiene visible (en vez de oculta) porque la regla de
    /// producto dice que toda opción técnica necesita label claro, y
    /// este lands de aterrizaje es la casa natural de futuras opciones
    /// avanzadas (p. ej. reset completo, importación/export global,
    /// diagnóstico del server LLM). En fases siguientes puede poblarse.
    pub(crate) fn settings_section_advanced(
        &self,
        _c: Chrome,
        _ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        // H2 (QA 24/09/2026): el usuario no encontraba el log. Su ruta se
        // muestra aquí y hay un botón que abre la carpeta en el Explorador
        // (también viven ahí config.json y assets/).
        let log_path = crate::log::log_path();
        let debug_on = crate::log::debug_enabled();
        let registro = column![
            text("Dónde queda todo")
                .size(fs.lg)
                .font(Font::MONOSPACE)
                .color(t.text),
            text("Registro de la app")
                .size(fs.sm)
                .color(t.dim),
            text(log_path.to_string_lossy().into_owned()).size(fs.sm),
            text(
                "Rota solo: al pasar de 1 MB el fichero pasa a .1 y el .1 a \
                 .2, así que nunca crece sin límite. Las trazas de alta \
                 frecuencia (cada movimiento al arrastrar la burbuja) solo se \
                 escriben con la variable de entorno WHISPR_DEBUG=1."
            )
            .size(fs.sm),
            row![
                button(ui_text("Abrir carpeta del registro"))
                    .on_press(Message::OpenLogFolder)
                    .padding(theme::SECONDARY_BUTTON_PADDING)
                    .style(theme::primary),
                text(if debug_on {
                    "WHISPR_DEBUG activado: el registro es detallado"
                } else {
                    "WHISPR_DEBUG desactivado"
                })
                .size(fs.xxs)
                .color(if debug_on { t.accent_text } else { t.dim }),
            ]
            .spacing(theme::SPACE_3)
            .align_y(iced::Alignment::Center),
            text("Imágenes y animación propias").size(fs.sm).color(t.dim),
            text(
                "Al elegir una imagen o una carpeta de frames en Apariencia, \
                 la app guarda una COPIA en la carpeta de arriba (assets/): \
                 puedes vaciar Descargas sin perder tu personalización."
            )
            .size(fs.sm),
        ]
        .spacing(theme::SPACE_3);

        design_card(vec![wide_row(
            container(registro).width(Length::Fill).into(),
        )])
    }
}

/// Navegación de Ajustes del diseño (`Settings navigation`, 43 px): etiqueta de
/// 13 px por pestaña (activa en texto primario, el resto en secundario) y el
/// subrayado de 2 px del acento colgando del borde inferior de la activa.
/// Debajo va el hairline que cierra la fila, como en el frame.
fn settings_nav(active: SettingsSection, c: Chrome, ty: DesignType) -> Element<'static, Message> {
    let mut tabs = row![].spacing(28.0).align_y(Alignment::End).height(TAB_BOX_HEIGHT);
    for section in SettingsSection::ALL {
        let is_active = section == active;
        // La caja de la pestaña ES la etiqueta (+ el subrayado de 2 px en su
        // borde inferior, pintado por `theme::tab_underline`). Sin hijos Fill
        // dentro: el ancho sale del texto y «Corrección IA» no parte en dos
        // líneas (con un hijo Fill, iced re-mide el texto a su ancho exacto y
        // el salto de línea es cuestión de redondeo).
        let tab = container(
            text(section.label())
                .size(ty.control)
                .font(if is_active { medium() } else { Font::DEFAULT })
                .color(if is_active {
                    c.text
                } else {
                    c.secondary
                }),
        )
        .height(Length::Fixed(TAB_BOX_HEIGHT))
        .align_y(Alignment::Center)
        .style(move |th| container::Style {
            background: Some(theme::tab_underline(th, is_active, TAB_BOX_HEIGHT)),
            ..container::Style::default()
        });
        tabs = tabs.push(
            button(tab)
                .padding(0)
                .on_press(Message::SettingsSectionSelected(section))
                .style(move |th, status| theme::settings_tab(th, status, is_active)),
        );
    }
    column![
        row![tabs, Space::with_width(Length::Fill)].height(TAB_BOX_HEIGHT),
        horizontal_rule(1).style(theme::rule),
    ]
    .into()
}
