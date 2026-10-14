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
    button, column, container, horizontal_rule, pick_list, row, scrollable, slider, text,
    text_input, toggler, Column, Row, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Padding};

use crate::app::{
    dictionary_list, filler_lists, language_hint, language_label, llm_status_ui, ui_text,
    FillerCategory, Message, SettingsSection, WhisprApp,
};
use crate::config::{
    BubbleCorner, BubbleHideTimeout, BubbleSize, BubbleStyle, CorrectionProvider, FontSize,
    ThemeMode,
};
use crate::theme::{self, Chrome, DesignType, UiPalette};
use crate::transcribe::Model;
use crate::ui::icons;

/// Ancho de la columna de control (nodo `3:660`, el `Selection` de 250 px del
/// frame Ajustes). Sale de [`theme::GRID_CONTROL_WIDTH`]: en esta vista no se
/// escribe ni una medida a mano.
const CONTROL_WIDTH: f32 = theme::GRID_CONTROL_WIDTH;

/// Peso `Medium` para los rótulos del diseño (la app no bundlea Inter: se le
/// pide el peso al family por defecto del sistema, ver `fonts.rs`).
fn medium() -> Font {
    Font {
        weight: Weight::Medium,
        ..Font::DEFAULT
    }
}

/// Rótulo de sección del diseño (12 px, secundario) + su tarjeta, separados
/// por los 10 px que mide el frame (nodo `3:654`).
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
    .spacing(theme::GRID_SECTION_TITLE_GAP)
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
    container(card)
        .width(Length::Fill)
        .style(theme::card)
        .into()
}

/// Columna de control de la rejilla: ancho FIJO (nodo `3:660`, 250 px) con el
/// control pegado a su borde derecho. Es lo que hace que selects, campos,
/// botones, interruptores e iconos de acción terminen en la MISMA x en las 5
/// pestañas (el `Selection` de `3:667` comparte borde derecho con el switch de
/// `3:692` y con el botón `Action` de `3:682`).
fn control_slot<'a>(control: Element<'a, Message>) -> Element<'a, Message> {
    container(control)
        .width(Length::Fixed(CONTROL_WIDTH))
        .align_x(Alignment::End)
        .into()
}

/// Interruptor de la rejilla: el switch pegado al borde derecho de la columna
/// (nodo `3:692`: 36×22 con su borde derecho en la misma x que los selects) y,
/// si lo hay, el texto de estado a su izquierda (nodo `3:691`, borde derecho a
/// 20 px del switch). La fila es de ancho propio y la alinea `control_slot`, así
/// que un estado más largo («Desactivada») NO desplaza el switch.
fn toggle_control<'a>(
    state: Option<&str>,
    value: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
    c: Chrome,
    ty: DesignType,
) -> Element<'a, Message> {
    let switch = toggler(value)
        .on_toggle(on_toggle)
        .size(theme::GRID_TOGGLE_SIZE)
        // iced reserva `spacing`/2 = 8 px para su etiqueta interna aunque no la
        // use: sin ponerlo a 0, el switch dibujado queda 8 px antes del borde
        // derecho de la columna.
        .spacing(0.0)
        .style(theme::toggler);
    match state {
        Some(state) => row![
            text(state.to_string())
                .size(ty.chrome)
                .color(c.faint)
                .align_x(Alignment::End),
            switch,
        ]
        .spacing(theme::GRID_TOGGLE_GAP)
        .align_y(Alignment::Center)
        .into(),
        None => row![switch].align_y(Alignment::Center).into(),
    }
}

/// Hueco de la columna de control para las filas informativas (sin control):
/// el rótulo conserva los 606 px de la rejilla (nodo `3:657`).
fn no_control<'a>() -> Element<'a, Message> {
    Space::new(0.0, 0.0).into()
}

/// Fila de preferencia de la rejilla (nodo `3:656`, 68 px): rótulo de 14 px
/// medium con su ayuda de 12 px tenue a la izquierda y el control en la columna
/// fija de la derecha, centrado verticalmente. Alto FIJO: la ayuda es de UNA
/// línea; las explicaciones largas van en [`note_row`].
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
    .width(Length::Fixed(theme::GRID_LABEL_WIDTH));
    if let Some(help) = help {
        left = left.push(text(help.to_string()).size(ty.body).color(c.faint));
    }
    container(
        row![left, control_slot(control)]
            .spacing(theme::GRID_ROW_PAD_H)
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fixed(theme::GRID_ROW_HEIGHT))
    .padding(row_padding())
    .into()
}

/// Explicación larga de la app (el diseño no las trae): mismo inset horizontal
/// que las filas (nodo `3:656`, x=20) y alto del contenido, para que el texto
/// pueda ocupar las líneas que necesite sin recortarse en una fila de 68 px.
fn note_row<'a>(content: &str, color: Color, ty: DesignType) -> Element<'a, Message> {
    container(text(content.to_string()).size(ty.body).color(color))
        .width(Length::Fill)
        .padding(row_padding())
        .into()
}

/// Inset de una fila de tarjeta: 15 px arriba y abajo, 20 a los lados (nodo
/// `3:656`: rótulo en x=20, y=15). Uno solo para todas las filas de las 5
/// pestañas.
fn row_padding() -> Padding {
    Padding {
        top: theme::GRID_ROW_PAD_V,
        right: theme::GRID_ROW_PAD_H,
        bottom: theme::GRID_ROW_PAD_V,
        left: theme::GRID_ROW_PAD_H,
    }
}

/// Fila de tarjeta sin control a la derecha: el contenido ocupa todo el ancho
/// (listas, formularios y compuestos) con el mismo inset que las filas de
/// preferencia.
fn wide_row<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .padding(row_padding())
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
        .spacing(theme::GRID_BLOCK_GAP)
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
        let model_control: Element<'_, Message> =
            pick_list(Model::all().to_vec(), Some(model), Message::ModelSelected)
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
        // Fila del modelo instalado (nodo `3:672`): 82 px = icono de 42 px + 20
        // arriba y abajo; el botón/selector de la derecha acaba en el mismo
        // borde que la columna de control de la rejilla (`3:682` x=770 → 896).
        let model_row = container(
            row![
                model_tile,
                column![
                    text(model_label).size(ty.nav).font(medium()).color(c.text),
                    text(format!("{model_size} MB"))
                        .size(ty.body)
                        .color(c.faint),
                ]
                .spacing(5.0)
                .width(Length::Fill),
                control_slot(model_control),
            ]
            .spacing(16.0)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fixed(theme::GRID_MODEL_ROW_HEIGHT))
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: theme::GRID_ROW_PAD_H,
            bottom: 0.0,
            left: theme::GRID_ROW_PAD_H,
        });
        col = col.push(section_block(
            "Modelo de voz",
            design_card(vec![model_row.into()]),
            c,
            ty,
        ));

        // ── Al terminar el dictado ──────────────────────────────────────
        let ai_enabled = self.config.ai_correction_enabled();
        // Interruptores de la rejilla: el switch siempre en la columna de
        // control y el estado («Activada»/«Desactivada») a su izquierda, que
        // no lo desplaza (nodos `3:691` y `3:692`).
        let ai_control = toggle_control(
            Some(if ai_enabled {
                "Activada"
            } else {
                "Desactivada"
            }),
            ai_enabled,
            Message::ToggleAiCorrection,
            c,
            ty,
        );
        let cleanup_control = toggle_control(
            None,
            self.config.cleanup_enabled(),
            Message::ToggleCleanup,
            c,
            ty,
        );
        let inject_control = toggle_control(
            None,
            self.config.auto_inject.unwrap_or(true),
            Message::ToggleAutoInject,
            c,
            ty,
        );
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
    /// Tema, burbuja flotante, imágenes propias y paleta editable, con la MISMA
    /// rejilla que General (columna de control de 250 px, filas de 68 px,
    /// tarjetas con el mismo radio, borde e inset). Es la pestaña que se había
    /// quedado con las medidas viejas: controles a lo ancho de la fila y
    /// ayudas sueltas fuera de la columna del rótulo.
    pub(crate) fn settings_section_appearance(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        let mut col = Column::new()
            .spacing(theme::GRID_SECTION_GAP)
            .width(Length::Fill);

        // ── Tema ────────────────────────────────────────────────────────
        // El aviso «sigue a Windows» y el factor de fuente eran textos sueltos
        // tras el select: pasan a la ayuda de su fila (nodo `3:659`), que es
        // donde el diseño pone la descripción.
        let theme_help = if self.config.theme_mode() == ThemeMode::Auto {
            Some("sigue a Windows")
        } else {
            None
        };
        let font_help = format!("{}×", self.config.font_size().factor());
        col = col.push(section_block(
            "Tema",
            design_card(vec![
                preference_row(
                    "Tema",
                    theme_help,
                    pick_list(
                        [ThemeMode::Auto, ThemeMode::Claro, ThemeMode::Oscuro].to_vec(),
                        Some(self.config.theme_mode()),
                        Message::ThemeModeChanged,
                    )
                    .placeholder("Tema")
                    .width(Length::Fill)
                    .text_size(ty.control)
                    .padding(theme::FIELD_PADDING)
                    .style(theme::pick_list)
                    .into(),
                    c,
                    ty,
                ),
                preference_row(
                    "Fuente",
                    Some(&font_help),
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
                    .text_size(ty.control)
                    .padding(theme::FIELD_PADDING)
                    .style(theme::pick_list)
                    .into(),
                    c,
                    ty,
                ),
            ]),
            c,
            ty,
        ));

        // ── Burbuja flotante ────────────────────────────────────────────
        // Los controles ya no ocupan la fila entera: van en la columna de
        // control (los selects) o dentro de ella (los sliders), con el valor
        // en el rótulo.
        let op = u32::from(self.config.bubble_opacity());
        col = col.push(section_block(
            "Burbuja flotante",
            design_card(vec![
                preference_row(
                    "Estilo",
                    None,
                    pick_list(
                        [BubbleStyle::Solid, BubbleStyle::Ring, BubbleStyle::Minimal].to_vec(),
                        Some(self.config.bubble_style()),
                        Message::BubbleStyleChanged,
                    )
                    .placeholder("Estilo")
                    .width(Length::Fill)
                    .text_size(ty.control)
                    .padding(theme::FIELD_PADDING)
                    .style(theme::pick_list)
                    .into(),
                    c,
                    ty,
                ),
                preference_row(
                    "Tamaño",
                    None,
                    pick_list(
                        [BubbleSize::Small, BubbleSize::Mid, BubbleSize::Large].to_vec(),
                        Some(self.config.bubble_size()),
                        Message::BubbleSizeChanged,
                    )
                    .placeholder("Tamaño")
                    .width(Length::Fill)
                    .text_size(ty.control)
                    .padding(theme::FIELD_PADDING)
                    .style(theme::pick_list)
                    .into(),
                    c,
                    ty,
                ),
                preference_row(
                    "Esquina",
                    None,
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
                    .text_size(ty.control)
                    .padding(theme::FIELD_PADDING)
                    .style(theme::pick_list)
                    .into(),
                    c,
                    ty,
                ),
                // Opacidad: slider 20–100 %; el getter clampea aunque el slider
                // nunca emita fuera de rango.
                preference_row(
                    &format!("Opacidad: {op}%"),
                    None,
                    slider(20..=100, op, Message::BubbleOpacityChanged)
                        .step(1u32)
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    c,
                    ty,
                ),
                preference_row(
                    "Auto-ocultar",
                    None,
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
                    .text_size(ty.control)
                    .padding(theme::FIELD_PADDING)
                    .style(theme::pick_list)
                    .into(),
                    c,
                    ty,
                ),
            ]),
            c,
            ty,
        ));

        // ── Imágenes (Personalización III) ──────────────────────────────
        col = col.push(section_block(
            "Imágenes",
            design_card(self.settings_section_images(c, ty, t)),
            c,
            ty,
        ));
        // ── Paleta (Personalización II) ─────────────────────────────────
        col = col.push(section_block(
            "Paleta",
            self.settings_palette_card(c, ty, t, fs),
            c,
            ty,
        ));
        col.into()
    }

    /// Filas de la tarjeta «Imágenes» dentro de Apariencia (Personalización
    /// III): el usuario puede elegir su propia imagen de fondo de la ventana y
    /// un sprite (estático o animado por frames) para la burbuja. Las rutas se
    /// persisten como dato (nunca hardcoded); si fallan al renderizar, el
    /// getter correspondiente devuelve `None` y la app pinta el render por
    /// defecto (sin ventana negra ni crash). El botón «Quitar» de cada control
    /// limpia la ruta en `config.json` y deja el aspecto original.
    ///
    /// Devuelve FILAS de tarjeta (no un `Element`): la rejilla de las 5
    /// pestañas las coloca con el mismo inset que todas las demás, y las rutas
    /// y explicaciones largas van en `note_row` (una fila de 68 px solo aguanta
    /// una línea de ayuda).
    pub(crate) fn settings_section_images(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Vec<Element<'_, Message>> {
        let fs = theme::font_sizes(self.config.font_size().factor());
        let mut rows: Vec<Element<'_, Message>> = vec![note_row(
            "Tu propia imagen de fondo y un sprite (estático o animado por \
             frames) para la burbuja. Si el archivo no existe o no es una \
             imagen soportada, la app vuelve al aspecto original.",
            c.faint,
            ty,
        )];

        // ── Imagen de fondo de la ventana ────────────────────────────────
        let bg_label = match self.config.bg_image_path() {
            Some(p) => p,
            None => "(sin imagen; usando el fondo del tema)",
        };
        rows.push(preference_row(
            "Fondo de la ventana",
            None,
            self.image_controls("Elegir…", Message::BgImageBrowse, Message::BgImageClear, fs),
            c,
            ty,
        ));
        rows.push(note_row(&format!("Ruta actual: {bg_label}"), t.dim, ty));

        // ── Sprite estático de la burbuja ────────────────────────────────
        let bubble_img_label = match self.config.bubble_image_path() {
            Some(p) => p,
            None => "(sin sprite; usando el render vectorial)",
        };
        rows.push(preference_row(
            "Sprite de la burbuja",
            None,
            self.image_controls(
                "Elegir…",
                Message::BubbleImageBrowse,
                Message::BubbleImageClear,
                fs,
            ),
            c,
            ty,
        ));
        rows.push(note_row(
            &format!("Ruta actual: {bubble_img_label}"),
            t.dim,
            ty,
        ));

        // ── Carpeta de frames (animación) ────────────────────────────────
        let frames_label = match self.config.bubble_frames_path() {
            Some(p) => p,
            None => "(sin carpeta; solo sprite estático)",
        };
        rows.push(preference_row(
            "Frames de animación",
            None,
            self.image_controls(
                "Elegir carpeta…",
                Message::BubbleFramesBrowse,
                Message::BubbleFramesClear,
                fs,
            ),
            c,
            ty,
        ));
        rows.push(note_row(&format!("Ruta actual: {frames_label}"), t.dim, ty));

        // ── FPS de la animación ──────────────────────────────────────────
        let fps = self.config.bubble_animation_fps();
        rows.push(preference_row(
            &format!("Velocidad (FPS): {fps}"),
            None,
            slider(
                u32::from(crate::custom_image::MIN_BUBBLE_FPS)
                    ..=u32::from(crate::custom_image::MAX_BUBBLE_FPS),
                u32::from(fps),
                Message::BubbleFpsChanged,
            )
            .step(1u32)
            .width(Length::Fixed(CONTROL_WIDTH))
            .into(),
            c,
            ty,
        ));
        rows.push(note_row(
            "Frames por segundo de la animación de la burbuja (rango 1–30; el \
             default es 12). Cambiar la ruta o pulsar «Quitar» detiene la \
             animación sin coste extra.",
            c.faint,
            ty,
        ));
        rows
    }

    /// Control de una fila de imagen: «Elegir…» + «Quitar», pegados al borde
    /// derecho de la columna de control (el mismo borde que los selects y los
    /// interruptores).
    fn image_controls<'a>(
        &self,
        abrir_label: &str,
        abrir: Message,
        quitar: Message,
        fs: theme::FontSizes,
    ) -> Element<'a, Message> {
        row![
            button(text(abrir_label.to_string()).size(fs.xxs))
                .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                .on_press(abrir)
                .style(theme::secondary),
            button(text("Quitar").size(fs.xxs))
                .padding([theme::SPACE_1 as f32, theme::SPACE_2 as f32])
                .on_press(quitar)
                .style(theme::secondary),
        ]
        .spacing(theme::SPACE_2)
        .align_y(Alignment::Center)
        .into()
    }

    /// Tarjeta «Paleta» de Apariencia (R7-II). Muestra presets de un clic, una
    /// fila por campo editable (hex + swatch + «Aplicar»/«Restaurar») y las
    /// acciones de Importar/Exportar/Restaurar todo. El aviso de contraste WCAG
    /// sale como nota bajo el campo cuando texto/principal baja de 3:1 sobre el
    /// fondo activo (no bloquea: solo avisa).
    ///
    /// Es la única sub-tabla de la app (el frame Ajustes no trae paleta): sus
    /// filas alinean el compuesto de controles al MISMO borde derecho que la
    /// columna de control de la rejilla, y el rótulo va a la izquierda.
    pub(crate) fn settings_palette_card(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Element<'_, Message> {
        let mut rows: Vec<Element<'_, Message>> = vec![
            note_row(
                "Cambia cada color por separado o usa un preset. Hex «#RRGGBB» o \
                 «#RGB»; los valores no válidos se ignoran.",
                c.faint,
                ty,
            ),
            wide_row(self.palette_preset_buttons(fs, t)),
            wide_row(
                text("Campos")
                    .size(ty.body)
                    .font(medium())
                    .color(c.secondary)
                    .into(),
            ),
        ];
        // 12 filas: una por PaletteField.
        for field in theme::PaletteField::ALL {
            rows.extend(self.palette_field_rows(*field, c, ty, t, fs));
        }
        rows.push(wide_row(self.palette_actions_row(t, fs)));
        rows.push(note_row(
            "Importar funde los campos presentes del JSON con tu paleta actual \
             (los ausentes se mantienen). Exportar guarda la paleta RESUELTA \
             (con defaults ya aplicados) para que sea directamente compartible.",
            c.faint,
            ty,
        ));
        design_card(rows)
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
        let swatch =
            container(Space::new(Length::Fixed(16.0), Length::Fixed(16.0))).style(move |_th| {
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

    /// Filas de la tabla de campos de la paleta para un color: el rótulo a la
    /// izquierda y, pegados al BORDE DERECHO de la tarjeta (el mismo borde que
    /// la columna de control de la rejilla), el hex + «Aplicar» + «Restaurar» +
    /// el swatch de previsualización. El aviso de contraste, cuando toca, sale
    /// como nota bajo la fila (no estira la fila ni le cambia el borde).
    pub(crate) fn palette_field_rows(
        &self,
        field: theme::PaletteField,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Vec<Element<'_, Message>> {
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
        let swatch_color =
            theme::parse_hex(&current_text).unwrap_or_else(|| self.resolved_color_for_field(field));

        let contrast_warn = self.contrast_warning_for(field, swatch_color);

        let label_text = text(field.label())
            .size(ty.nav)
            .font(medium())
            .color(c.text);

        let input_box: Element<'_, Message> = text_input(field.help(), &current_text)
            .on_input(move |v| Message::PaletteFieldChanged(field, v))
            .on_submit(Message::PaletteFieldApply(field))
            .padding(theme::FIELD_PADDING)
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
        let swatch =
            container(Space::new(Length::Fixed(20.0), Length::Fixed(20.0))).style(move |_th| {
                container::Style {
                    background: Some(Background::Color(swatch_color_for_closure)),
                    border: Border {
                        color: border_color,
                        width: 1.0,
                        radius: Radius::from(4.0),
                    },
                    ..Default::default()
                }
            });

        let row = wide_row(
            row![
                label_text,
                Space::new(Length::Fill, Length::Fixed(0.0)),
                input_box,
                apply_btn,
                reset_btn,
                Space::new(Length::Fixed(theme::SPACE_2 as f32), Length::Fixed(0.0)),
                swatch,
            ]
            .spacing(theme::SPACE_2)
            .align_y(Alignment::Center)
            .into(),
        );

        let mut rows = vec![row];
        if let Some(warn) = contrast_warn {
            rows.push(note_row(&warn, t.warn, ty));
        }
        rows
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
        let must_warn =
            matches!(field, theme::PaletteField::Text | theme::PaletteField::Dim) && ratio < 3.0;
        if must_warn {
            Some(format!(
                "⚠ Contraste {ratio:.2}:1 (texto sobre fondo, mínimo WCAG 3:1). \
                 Se aplica igualmente; ajusta hex o restaura el campo."
            ))
        } else {
            None
        }
    }

    /// Botones «Importar…» / «Exportar…» / «Restaurar todo» (la explicación de
    /// qué hace cada uno va como nota de la tarjeta, no aquí).
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
        let _ = t;
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
            .align_y(Alignment::Center)
            .into()
    }

    /// ── Sección «Dictado» ────────────────────────────────────────────────
    /// Calidad (beam size), hilos de CPU y el resumen de la inyección, con la
    /// rejilla de las 5 pestañas: los dos sliders en la columna de control, su
    /// explicación como nota bajo cada fila y la inyección (que no tiene
    /// controles, los fija el producto) como nota informativa.
    pub(crate) fn settings_section_dictation(
        &self,
        c: Chrome,
        ty: DesignType,
        _t: &UiPalette,
    ) -> Element<'_, Message> {
        let beam = self.config.beam_size();
        let threads = self.config.n_threads();

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
            .spacing(theme::GRID_SECTION_GAP)
            .width(Length::Fill);
        col = col.push(section_block(
            "Calidad y rendimiento",
            design_card(vec![
                preference_row(
                    &format!("Calidad: {beam}"),
                    None,
                    slider(1..=8, beam, Message::BeamSizeChanged)
                        .step(1u32)
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    c,
                    ty,
                ),
                note_row(
                    "Calidad de transcripción (beam size) vs velocidad: 1 = la \
                     más rápida, 8 = la más fiel. Rango 1–8, recomendado: 5.",
                    c.faint,
                    ty,
                ),
                preference_row(
                    &format!("Hilos de CPU: {threads}"),
                    None,
                    slider(1..=16, threads, Message::ThreadsChanged)
                        .step(1u32)
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    c,
                    ty,
                ),
                note_row(
                    "Núcleos de CPU que usa Whisper al transcribir. Sube si el \
                     equipo va justo de velocidad; baja para dejar recursos al \
                     resto. Rango 1–16, recomendado: 4.",
                    c.faint,
                    ty,
                ),
            ]),
            c,
            ty,
        ));
        // La inyección no expone controles (los fija el producto): su resumen
        // va como nota de la sección, con el mismo inset que las filas.
        col = col.push(section_block(
            "Inyección en la app activa",
            design_card(vec![note_row(&injection_label, c.faint, ty)]),
            c,
            ty,
        ));
        col.into()
    }

    /// ── Sección «Corrección IA» ─────────────────────────────────────────
    /// Todo el corrector LLM con la rejilla de las 5 pestañas: cada opción es
    /// una fila (rótulo + control en la columna fija) y las explicaciones largas
    /// van como notas bajo su fila. El estado en vivo va arriba, como nota con
    /// el color del estado.
    pub(crate) fn settings_section_correction(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        // Estado en vivo (misma lógica pura que el chip del header):
        // «Corrección IA: activa · <modelo>» o «inactiva — <motivo>».
        let (status_text, status_color) = {
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
            (ui.label, color)
        };

        let mut col = Column::new()
            .spacing(theme::GRID_SECTION_GAP)
            .width(Length::Fill);
        col = col.push(section_block(
            "Corrección IA",
            design_card(
                vec![
                    note_row(&status_text, status_color, ty),
                    // El interruptor va a la columna de control (el mismo sitio que
                    // en General: los toggles de todas las secciones comparten
                    // columna, nodo `3:692`).
                    preference_row(
                        "Corrección IA: una IA revisa y corrige lo que dictas",
                        None,
                        toggle_control(
                            None,
                            self.config.ai_correction_enabled(),
                            Message::ToggleAiCorrection,
                            c,
                            ty,
                        ),
                        c,
                        ty,
                    ),
                    note_row(
                        "Al terminar un turno, la IA lo revisa y corrige («digo X», \
                     «no, era Y»…). Si el servidor de IA no está disponible, el \
                     dictado se inserta entero, sin corregir.",
                        c.faint,
                        ty,
                    ),
                    preference_row(
                        "Proveedor",
                        None,
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
                        .text_size(ty.control)
                        .padding(theme::FIELD_PADDING)
                        .style(theme::pick_list)
                        .into(),
                        c,
                        ty,
                    ),
                    note_row(
                        "Quién corrige lo que dictas: Local — llama.cpp, el que viene \
                     con la app (sin internet, recomendado) · Custom LLM — tu \
                     propio endpoint OpenAI-compatible · Cloud — OpenAI. Por \
                     defecto: Local.",
                        c.faint,
                        ty,
                    ),
                    preference_row(
                        "Endpoint LLM",
                        None,
                        text_input(
                            "http://127.0.0.1:8081/v1",
                            self.config.llm_base_url.as_deref().unwrap_or(""),
                        )
                        .on_input(Message::LlmEndpointChanged)
                        .padding(theme::FIELD_PADDING)
                        .size(ty.control)
                        .style(theme::text_input)
                        .width(Length::Fill)
                        .into(),
                        c,
                        ty,
                    ),
                    note_row(
                        "Dirección del servidor de IA (API compatible con OpenAI). \
                     Local: http://127.0.0.1:8081/v1.",
                        c.faint,
                        ty,
                    ),
                    preference_row(
                        "Modelo IA",
                        None,
                        text_input(
                            "ornith-1.0-9b / gpt-4o-mini",
                            self.config.llm_model.as_deref().unwrap_or(""),
                        )
                        .on_input(Message::LlmModelChanged)
                        .padding(theme::FIELD_PADDING)
                        .size(ty.control)
                        .style(theme::text_input)
                        .width(Length::Fill)
                        .into(),
                        c,
                        ty,
                    ),
                    note_row(
                        "El modelo que corrige. Debe estar en tu servidor local o ser \
                     uno de los que ofrece la nube.",
                        c.faint,
                        ty,
                    ),
                    // El resto de la tarjeta: la API key (que solo existe con
                    // proveedor que la exija). El server LLM ya no tiene fila:
                    // viaja en el paquete (`vendor/llama/`) y no se configura.
                ]
                .into_iter()
                .chain(self.api_key_rows(c, ty))
                .collect(),
            ),
            c,
            ty,
        ));

        // ── Detalles técnicos ───────────────────────────────────────────
        // Timeout: slider 1..=120 s (25 por defecto). El getter clampea.
        col = col.push(section_block(
            "Detalles técnicos",
            design_card(vec![
                preference_row(
                    "Espera máx. (s)",
                    None,
                    slider(
                        1..=120,
                        self.config.llm_timeout_secs().unwrap_or(25).clamp(1, 120) as u32,
                        Message::LlmTimeoutChanged,
                    )
                    .step(1u32)
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    c,
                    ty,
                ),
                note_row(
                    "Espera máxima de la IA antes de dar el turno por \
                     definitivo: si lo supera, se inserta sin corregir. \
                     Rango 1–120 s, recomendado: 25 s.",
                    c.faint,
                    ty,
                ),
                // Umbral del clasificador: 0..=100. Default 50. Etiqueta llana
                // «sensibilidad» (requisito UX 10/09/2026: «umbral clasif.» no
                // dice nada).
                preference_row(
                    &format!("Sensibilidad: {}", self.config.llm_classifier_threshold()),
                    None,
                    slider(
                        0..=100,
                        u32::from(self.config.llm_classifier_threshold()),
                        Message::LlmThresholdChanged,
                    )
                    .step(1u32)
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    c,
                    ty,
                ),
                note_row(
                    "Con qué frecuencia se envía un turno a la IA: 50 = \
                     casi siempre que hay una señal de corrección, 100 = \
                     casi nunca. Rango 0–100, recomendado: 50.",
                    c.faint,
                    ty,
                ),
                preference_row(
                    "Confirmar antes de aplicar",
                    None,
                    toggle_control(
                        None,
                        self.config.ai_correction_confirm(),
                        Message::LlmConfirmToggled,
                        c,
                        ty,
                    ),
                    c,
                    ty,
                ),
                note_row(
                    "Pide tu visto bueno (Aplicar/Descartar) antes de \
                     modificar el texto. Recomendado si prefieres revisar \
                     lo que corrige la IA.",
                    c.faint,
                    ty,
                ),
            ]),
            c,
            ty,
        ));
        col.into()
    }

    /// Filas de la API key: solo con proveedor que la exija (Custom — opcional
    /// si el server no la pide — y Cloud, obligatoria). Con Local el server no
    /// la usa, así que no se pinta nada. Persistencia en el Credential Manager
    /// de Windows, NUNCA en `config.json`.
    fn api_key_rows(&self, c: Chrome, ty: DesignType) -> Vec<Element<'_, Message>> {
        if self.config.correction_provider() == CorrectionProvider::Default {
            return Vec::new();
        }
        vec![
            preference_row(
                "API key",
                None,
                text_input(
                    "opcional (Custom) · obligatoria (Cloud)",
                    self.config.llm_api_key.as_deref().unwrap_or(""),
                )
                .on_input(Message::LlmApiKeyChanged)
                .secure(true)
                .padding(theme::FIELD_PADDING)
                .size(ty.control)
                .style(theme::text_input)
                .width(Length::Fill)
                .into(),
                c,
                ty,
            ),
            note_row(
                "Clave del endpoint si la exige (Custom: opcional · \
                 Cloud: obligatoria). Se guarda en el Administrador \
                 de credenciales de Windows, nunca en la config.",
                c.faint,
                ty,
            ),
        ]
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
        c: Chrome,
        ty: DesignType,
        _t: &UiPalette,
    ) -> Element<'_, Message> {
        // H2 (QA 24/09/2026): el usuario no encontraba el log. Su ruta se
        // muestra aquí (como ayuda de su fila) y hay un botón que abre la
        // carpeta en el Explorador (también viven ahí config.json y assets/).
        let log_path = crate::log::log_path().to_string_lossy().into_owned();
        let debug_on = crate::log::debug_enabled();
        let debug_state = if debug_on {
            "WHISPR_DEBUG activado: el registro es detallado"
        } else {
            "WHISPR_DEBUG desactivado"
        };
        section_block(
            "Dónde queda todo",
            design_card(vec![
                preference_row(
                    "Registro de la app",
                    Some(&log_path),
                    button(ui_text("Abrir carpeta del registro"))
                        .on_press(Message::OpenLogFolder)
                        .padding(theme::SECONDARY_BUTTON_PADDING)
                        .style(theme::primary)
                        .into(),
                    c,
                    ty,
                ),
                note_row(
                    "Rota solo: al pasar de 1 MB el fichero pasa a .1 y el .1 a \
                     .2, así que nunca crece sin límite. Las trazas de alta \
                     frecuencia (cada movimiento al arrastrar la burbuja) solo se \
                     escriben con la variable de entorno WHISPR_DEBUG=1.",
                    c.faint,
                    ty,
                ),
                preference_row("Registro detallado", Some(debug_state), no_control(), c, ty),
                preference_row("Imágenes y animación propias", None, no_control(), c, ty),
                note_row(
                    "Al elegir una imagen o una carpeta de frames en Apariencia, \
                     la app guarda una COPIA en la carpeta de arriba (assets/): \
                     puedes vaciar Descargas sin perder tu personalización.",
                    c.faint,
                    ty,
                ),
            ]),
            c,
            ty,
        )
    }
}

/// Navegación de Ajustes del diseño (`Settings navigation`, 43 px): etiqueta de
/// 13 px por pestaña (activa en texto primario, el resto en secundario) y el
/// subrayado de 2 px del acento colgando del borde inferior de la activa.
/// Debajo va el hairline que cierra la fila, como en el frame.
fn settings_nav(active: SettingsSection, c: Chrome, ty: DesignType) -> Element<'static, Message> {
    let mut tabs = row![]
        .spacing(theme::GRID_TAB_GAP)
        .align_y(Alignment::End)
        .height(theme::GRID_TAB_HEIGHT);
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
                .color(if is_active { c.text } else { c.secondary }),
        )
        .height(Length::Fixed(theme::GRID_TAB_HEIGHT))
        .align_y(Alignment::Center)
        .style(move |th| container::Style {
            background: Some(theme::tab_underline(th, is_active, theme::GRID_TAB_HEIGHT)),
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
        row![tabs, Space::with_width(Length::Fill)].height(theme::GRID_TAB_HEIGHT),
        horizontal_rule(1).style(theme::rule),
    ]
    .into()
}
