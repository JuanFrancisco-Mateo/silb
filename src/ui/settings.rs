//! Espacio de trabajo de Ajustes: la pantalla `3:604` de Figma.
//!
//! Medidas y colores salen de `design/openwhispr-figma/raw/
//! ajustes-design-context.txt` (capa a capa), `metadata-arbol-capas.txt`
//! (coordenadas) y `raw/ajustes.png` (el dibujo). Todo el color sale de
//! [`crate::theme`]: aquí no hay colores sueltos.
//!
//! Cómo está montada (1200×820): cabecera de página, pestañas (43 px, con raya
//! de 2 px en la activa), contenido de la pestaña con scroll y pie con el atajo
//! en teclas. Las tarjetas de sección y sus filas son las piezas del diseño
//! ([`section_block`], [`design_card`], [`preference_row`], [`wide_row`]).
//!
//! El estado sigue en [`WhisprApp`] (`settings_open`, `settings_section`,
//! `palette_inputs`, …); este módulo solo dibuja. Las claves y valores del
//! `config` no cambian respecto a la interfaz anterior: es capa visual y orden.
//!
//! Nota sobre las pestañas: el diseño trae `Transcripción · Diccionario
//! personal · Muletillas · General`; la app mantiene sus cinco secciones
//! (`General · Apariencia · Dictado · Corrección IA · Avanzado`) porque el
//! reparto de contenido es el de siempre — el diseño aporta la forma, la altura
//! y los estados (ver el cierre de la tarjeta).

use iced::border::Radius;
use iced::widget::{
    button, column, container, horizontal_rule, pick_list, row, scrollable, slider, text,
    text_input, toggler, Column, Row, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{
    dictionary_list, filler_lists, language_hint, language_label, llm_status_ui, ui_text,
    FillerCategory, Message, SettingsSection, WhisprApp,
};
use crate::config::{
    BubbleCorner, BubbleHideTimeout, BubbleSize, BubbleStyle, CorrectionProvider, FontSize,
    ThemeMode,
};
use crate::fonts;
use crate::theme::{self, Chrome, DesignType, UiPalette};
use crate::transcribe::Model;
use crate::ui::icons;

/// Ancho de la columna de control (nodo `3:660`, los 250 px del marco de
/// Ajustes). Sale de [`theme::GRID_CONTROL_WIDTH`]: en esta vista no se
/// escribe ni una medida a mano.
const CONTROL_WIDTH: f32 = theme::GRID_CONTROL_WIDTH;

/// Rótulo de sección del diseño (12 px, secundario) más su tarjeta, separados
/// por los 10 px del diseño (nodo `3:654`).
fn section_block<'a>(
    label: &str,
    card: Element<'a, Message>,
    c: Chrome,
    ty: DesignType,
) -> Element<'a, Message> {
    column![
        text(label.to_string())
            .size(ty.body)
            .font(fonts::medium())
            .color(c.secondary),
        card,
    ]
    .spacing(theme::GRID_SECTION_TITLE_GAP)
    .width(Length::Fill)
    .into()
}

/// Tarjeta del diseño (`#1d1e24` + borde `#303139` + esquina 12 = [`theme::card`])
/// con sus filas separadas por la línea fina del diseño (entre filas, ni antes
/// de la primera ni después de la última).
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
/// control pegado a su borde derecho. Gracias a eso los desplegables, campos,
/// botones, interruptores e iconos acaban en la MISMA x en las 5 pestañas (el
/// `Selection` de `3:667` comparte borde derecho con el interruptor de `3:692`
/// y con el botón `Action` de `3:682`).
fn control_slot<'a>(control: Element<'a, Message>) -> Element<'a, Message> {
    container(control)
        .width(Length::Fixed(CONTROL_WIDTH))
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Center)
        .into()
}

/// Interruptor de la rejilla: pegado al borde derecho de la columna (nodo
/// `3:692`: 36×22, con su borde derecho en la misma x que los desplegables) y,
/// si lo hay, el texto de estado a su izquierda (nodo `3:691`, a 20 px del
/// interruptor). La fila tiene ancho propio y la alinea `control_slot`, así que
/// un estado más largo («Desactivada») NO mueve el interruptor.
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
        // use: si no se pone a 0, el interruptor queda 8 px antes del borde
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
/// el rótulo se queda con los 606 px de la rejilla (nodo `3:657`).
fn no_control<'a>() -> Element<'a, Message> {
    Space::new(0.0, 0.0).into()
}

/// Alto de una fila de preferencia: 68 px del diseño con factor 1.0, pero
/// escalado con la letra. La app deja subir el tamaño (Ajustes → Apariencia →
/// Fuente) y una fila de 68 px FIJA recortaba la ayuda de 12 px: con factor
/// 1.15 solo se veía el rótulo. Se calcula con las líneas reales (alto de línea
/// 1.3 de iced) más los 15 px de margen de arriba y abajo.
fn preference_row_height(ty: DesignType, has_help: bool) -> f32 {
    let line = |size: u16| size as f32 * 1.3;
    let content = if has_help {
        line(ty.nav) + 4.0 + line(ty.body)
    } else {
        line(ty.nav)
    };
    (content + 2.0 * theme::GRID_ROW_PAD_V).max(theme::GRID_ROW_HEIGHT)
}

/// Alto de la fila del modelo instalado: 82 px del diseño (82 = icono de 42 +
/// 20 arriba y abajo), ampliado si el bloque de texto (nombre + tamaño) crece
/// con el factor de fuente.
fn model_row_height(ty: DesignType) -> f32 {
    let line = |size: u16| size as f32 * 1.3;
    let text = line(ty.nav) + 5.0 + line(ty.body);
    (text + 40.0).max(theme::GRID_MODEL_ROW_HEIGHT)
}

/// Fila de preferencia de la rejilla (nodo `3:656`, 68 px): rótulo de 14 px
/// medium con su ayuda de 12 px tenue a la izquierda y el control en la columna
/// fija de la derecha, centrado en vertical. El alto sale del tamaño de letra
/// (ver [`preference_row_height`]): la ayuda es de UNA línea; las explicaciones
/// largas van en [`note_row`].
fn preference_row<'a>(
    label: &str,
    help: Option<&str>,
    control: Element<'a, Message>,
    c: Chrome,
    ty: DesignType,
) -> Element<'a, Message> {
    let mut left = column![text(label.to_string())
        .size(ty.nav)
        .font(fonts::medium())
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
    .height(Length::Fixed(preference_row_height(ty, help.is_some())))
    .align_y(Alignment::Center)
    .padding(row_padding())
    .into()
}

/// Explicación larga (el diseño no las trae): el mismo margen lateral que las
/// filas (nodo `3:656`, x=20) y alto del contenido, para que el texto use las
/// líneas que necesite sin recortarse en una fila de 68 px.
fn note_row<'a>(content: &str, color: Color, ty: DesignType) -> Element<'a, Message> {
    // `ui_text` (Shaping::Advanced): las notas llevan símbolos («⚠», «·»,
    // «→») que se buscan en otras fuentes; con el dibujo básico salían como
    // cajas.
    container(ui_text(content.to_string()).size(ty.body).color(color))
        .width(Length::Fill)
        .padding(row_padding())
        .into()
}

/// Margen de una fila de tarjeta: 15 px arriba y abajo, 20 a los lados (nodo
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
/// (listas, formularios y mezclas) con el mismo margen que las filas de
/// preferencia.
fn wide_row<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .padding(row_padding())
        .into()
}

impl WhisprApp {
    /// Espacio de trabajo de Ajustes (pantalla `3:604` de Figma): cabecera de
    /// página, pestañas, contenido de la pestaña activa (con scroll) y pie con el
    /// atajo y el estado de guardado.
    pub(crate) fn settings_view(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        let heading = column![
            text("Ajustes")
                .size(ty.title)
                .font(fonts::semibold())
                .color(c.text),
            text("Tu forma de dictar, a tu medida.")
                .size(ty.control)
                .color(c.secondary),
        ]
        .spacing(6.0)
        .width(Length::Fill);

        // Solo hace scroll el contenido de la pestaña activa: la cabecera, las
        // pestañas y el pie quedan siempre a la vista.
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
    /// algún día se mueve un control, se mueve a su sección sin tocar las demás.
    fn settings_body(&self, c: Chrome, ty: DesignType, t: &UiPalette) -> Element<'_, Message> {
        match self.settings_section {
            SettingsSection::General => self.settings_section_general(c, ty, t),
            SettingsSection::Apariencia => self.settings_section_appearance(c, ty, t),
            SettingsSection::Dictado => self.settings_section_dictation(c, ty, t),
            SettingsSection::CorreccionIA => self.settings_section_correction(c, ty, t),
            SettingsSection::Avanzado => self.settings_section_advanced(c, ty, t),
        }
    }

    /// `Settings footer` del diseño: el atajo de dictado en teclas a la
    /// izquierda y el estado de guardado a la derecha.
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
    /// Contenido de la pantalla de Ajustes de Figma (la pestaña de
    /// transcripción): «Audio e idioma», «Modelo de voz» y «Al terminar el
    /// dictado», más los bloques que la app tiene aquí desde siempre (diccionario
    /// y muletillas). Solo capa visual: mismas claves y valores, mismos mensajes.
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
        // La pista del idioma ("default: es", "whisper decide") va como ayuda
        // de la fila: el diseño no la trae, pero es información que la app ya
        // daba.
        let lang_hint = language_hint(&self.config.language);
        let lang_help = if lang_hint.is_empty() {
            None
        } else {
            Some(lang_hint.as_str())
        };
        // Idioma: el getter pasa "auto" a None y cualquier otro valor a
        // Some(código) que se le da a whisper.
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

        // Se separa del diseño: él enseña el modelo instalado y un botón
        // «Cambiar modelo»; aquí el control sigue siendo el selector de modelos
        // de siempre (no se inventa un flujo de instalación).
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
        // Nombre del modelo + etiqueta «Instalado» (nodo `3:676`/`3:678`): solo
        // si el GGML ya está en el disco. Es lo que enseña el diseño y además
        // evita que el usuario se pregunte si está descargado.
        let mut name_row = row![text(model_label)
            .size(ty.nav)
            .font(fonts::medium())
            .color(c.text)]
        .spacing(12.0)
        .align_y(Alignment::Center);
        if crate::transcribe::model_exists(model) {
            name_row = name_row.push(
                row![
                    icons::icon_colored(icons::CHECK, 12.0, c.accent_active),
                    text("Instalado").size(ty.chrome).color(c.accent_active),
                ]
                .spacing(4.0)
                .align_y(Alignment::Center),
            );
        }
        // Fila del modelo instalado (nodo `3:672`): 82 px = icono de 42 px más
        // 20 arriba y abajo; el botón o selector de la derecha acaba en el mismo
        // borde que la columna de control (`3:682` x=770 → 896).
        let model_row = container(
            row![
                model_tile,
                column![
                    name_row,
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
        .height(Length::Fixed(model_row_height(ty)))
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
        // Interruptores de la rejilla: el interruptor siempre en la columna de
        // control y el estado («Activada»/«Desactivada») a su izquierda, que no
        // lo mueve (nodos `3:691` y `3:692`).
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

        // ── Atajo de dictado (editable desde el sprint final; antes solo en
        // config.json). El campo va en la columna de control (250 px) y los
        // botones en una fila aparte a la derecha: meter el campo y 2 botones en
        // 250 px aplastaba el campo («Ctrl+S…» no se leía).
        let shortcut_value = self
            .shortcut_input
            .clone()
            .unwrap_or_else(|| self.config.shortcut());
        let mut shortcut_rows: Vec<Element<'_, Message>> = vec![
            preference_row(
                "Atajo global",
                Some("P. ej. Ctrl+Shift+D, Alt+Space, Ctrl+Alt+F12."),
                text_input("Ctrl+Shift+D", &shortcut_value)
                    .on_input(Message::ShortcutInputChanged)
                    .on_submit(Message::ShortcutApply)
                    .padding(theme::FIELD_PADDING)
                    .size(ty.control)
                    .style(theme::text_input)
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                c,
                ty,
            ),
            wide_row(
                row![
                    Space::with_width(Length::Fill),
                    button(text("Aplicar").size(ty.control))
                        .on_press(Message::ShortcutApply)
                        .padding(theme::ACTION_BUTTON_PADDING)
                        .style(theme::primary),
                    button(text("Restablecer").size(ty.control))
                        .on_press(Message::ShortcutReset)
                        .padding(theme::ACTION_BUTTON_PADDING)
                        .style(theme::secondary),
                ]
                .spacing(theme::SPACE_2)
                .align_y(Alignment::Center)
                .into(),
            ),
        ];
        if let Some(err) = &self.shortcut_error {
            shortcut_rows.push(note_row(err, t.err, ty));
        } else {
            shortcut_rows.push(note_row(
                &format!("Atajo activo: {}", self.hotkey_label),
                c.faint,
                ty,
            ));
        }
        col = col.push(section_block(
            "Atajo de dictado",
            design_card(shortcut_rows),
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
        // Aviso de importar/exportar: va aquí para que se vea mientras la
        // sección General esté abierta.
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
        // Botón «Por defecto»: solo sale si el usuario ha cambiado las listas
        // (config.fillers = Some).
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
    /// tarjetas con la misma esquina, borde y márgenes). Es la pestaña que se
    /// había quedado con las medidas viejas: controles a lo ancho de la fila y
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
        // El aviso «sigue a Windows» y el factor de letra eran textos sueltos
        // tras el desplegable: pasan a la ayuda de su fila (nodo `3:659`), que es
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
        // Los controles ya no ocupan la fila entera: van en la columna de control
        // (los desplegables) o dentro de ella (los deslizadores), con el valor en
        // el rótulo.
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
                // Opacidad: control de 20–100 %; el getter lo limita aunque el
                // control nunca emita fuera de rango.
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

    /// Filas de la tarjeta «Imágenes» de Apariencia (Personalización III): el
    /// usuario puede elegir su imagen de fondo de la ventana y una imagen (fija
    /// o animada por fotogramas) para la burbuja. Las rutas se guardan como
    /// dato (nunca escritas en el código); si fallan al dibujar, el getter
    /// devuelve `None` y la app pinta el aspecto de siempre (sin ventana negra
    /// ni cierre). El botón «Quitar» de cada control borra la ruta del
    /// `config.json` y deja el aspecto original.
    ///
    /// Devuelve FILAS de tarjeta (no un `Element`): la rejilla de las 5 pestañas
    /// las coloca con el mismo margen que las demás, y las rutas y explicaciones
    /// largas van en `note_row` (una fila de 68 px solo aguanta una línea de
    /// ayuda).
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

        // ── Imagen de la burbuja: UN solo fichero (sprint final) ─────────
        // PNG/JPG/WebP = fija; GIF = animada (se saca a memoria). Las carpetas
        // de imágenes de antes siguen funcionando si ya estaban configuradas,
        // pero la interfaz ya no deja elegirlas (liaba mucho).
        let bubble_img_label = match self.config.bubble_image_path() {
            Some(p) => p,
            None => "(sin imagen; usando el render vectorial)",
        };
        rows.push(preference_row(
            "Imagen de la burbuja",
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
        rows.push(note_row(
            "Un solo archivo: PNG/JPG/WebP para imagen fija, GIF para \
             animación. Los vídeos (WebM/MP4) no están soportados.",
            c.faint,
            ty,
        ));

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
            "Tope de frames por segundo del GIF (rango 1–30; default 12). \
             Quitar la imagen detiene la animación sin coste extra.",
            c.faint,
            ty,
        ));
        rows
    }

    /// Control de una fila de imagen: «Elegir…» y «Quitar», pegados al borde
    /// derecho de la columna de control (el mismo borde que los desplegables y
    /// los interruptores).
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

    /// Tarjeta «Paleta» de Apariencia (R7-II). Enseña presets de un clic, una
    /// fila por campo editable (hex + muestra + «Aplicar»/«Restaurar») y las
    /// acciones de Importar/Exportar/Restaurar todo. El aviso de contraste WCAG
    /// sale como nota bajo el campo cuando texto/principal baja de 3:1 sobre el
    /// fondo activo (no bloquea: solo avisa).
    ///
    /// Es la única sub-tabla de la app (el diseño de Ajustes no trae paleta):
    /// sus filas alinean los controles al MISMO borde derecho que la columna de
    /// control de la rejilla, y el rótulo va a la izquierda.
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
                    .font(fonts::medium())
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
    /// resuelta del preset sobre el modo activo (oscuro o claro según el
    /// `ThemeMode` del usuario en ese momento).
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
        // Muestra de color: enseña el acento del modo activo como
        // recordatorio («el preset tocará esto»).
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
    /// la muestra de color. El aviso de contraste, cuando toca, sale como nota
    /// bajo la fila (no estira la fila ni le cambia el borde).
    pub(crate) fn palette_field_rows(
        &self,
        field: theme::PaletteField,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
        fs: theme::FontSizes,
    ) -> Vec<Element<'_, Message>> {
        // Texto del campo: lo que el usuario está escribiendo, o el hex ya
        // guardado si no hay nada en marcha, o vacío (= por defecto).
        let current_text = self
            .palette_inputs
            .get(&field)
            .cloned()
            .or_else(|| self.palette_field_persisted(field))
            .unwrap_or_default();

        // Color que se enseña en la muestra:
        //   - si lo escrito es un hex válido, ese color;
        //   - si no, el color que se usa (el del usuario o el de serie).
        let swatch_color =
            theme::parse_hex(&current_text).unwrap_or_else(|| self.resolved_color_for_field(field));

        let contrast_warn = self.contrast_warning_for(field, swatch_color);

        let label_text = text(field.label())
            .size(ty.nav)
            .font(fonts::medium())
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

        // Muestra: enseña el color actual (el guardado o el que se está escribiendo).
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

    /// Devuelve el hex guardado del campo (o None si está por defecto).
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

    /// Devuelve el color que se usa (con los cambios del usuario aplicados) para un campo.
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

    /// Si el campo es «texto» o «dim» y su contraste contra el fondo activo es
    /// < 3:1, devuelve un aviso (None si no). Nunca bloquea al usuario: solo
    /// avisa.
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

    /// Botones «Importar…» / «Exportar…» / «Restaurar todo» (lo que hace cada
    /// uno se cuenta como nota de la tarjeta, no aquí).
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
    /// Calidad (beam size), hilos de CPU y el resumen de cómo se escribe, con la
    /// rejilla de las 5 pestañas: los dos deslizadores en la columna de control,
    /// su explicación como nota bajo cada fila y la escritura (que no tiene
    /// controles, la fija el producto) como nota informativa.
    pub(crate) fn settings_section_dictation(
        &self,
        c: Chrome,
        ty: DesignType,
        _t: &UiPalette,
    ) -> Element<'_, Message> {
        let beam = self.config.beam_size();
        let threads = self.config.n_threads();

        // Escritura: sin fila informativa en Ajustes (sprint final: se quita el
        // resumen que sobraba; los parámetros los fija el producto).

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
        // (Sin bloque de inyección: eliminado por redundante.)
        col.into()
    }

    /// ── Sección «Corrección IA» ─────────────────────────────────────────
    /// Todo el corrector con la rejilla de las 5 pestañas: cada opción es una
    /// fila (rótulo + control en la columna fija) y las explicaciones largas van
    /// como notas bajo su fila. El estado en vivo va arriba, como nota con el
    /// color del estado.
    pub(crate) fn settings_section_correction(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &UiPalette,
    ) -> Element<'_, Message> {
        // Estado en vivo (la misma cuenta que la etiqueta de la cabecera):
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
        // Sprint final: solo dos modos. Local = llama.cpp descargado cuando hace
        // falta (nada viene de serie); Remoto = solo clave de API + proveedor.
        let is_local = self.config.correction_provider().is_local();
        let mut main_rows: Vec<Element<'_, Message>> = vec![
                    note_row(&status_text, status_color, ty),
                    // El interruptor va a la columna de control (el mismo sitio
                    // que en General: los interruptores de todas las secciones
                    // comparten columna, nodo `3:692`).
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
                        "Quién corrige lo que dictas: Local — en este equipo, sin \
                     internet (se descarga solo si lo activas) · Remoto — con \
                     tu API key (OpenAI o endpoint propio).",
                        c.faint,
                        ty,
                    ),
        ];
        if is_local {
            main_rows.push(note_row(
                "Modelos pequeños recomendados (<1,2 GB): Qwen2.5-0.5B (~491 MB) · \
                 Llama 3.2 1B (~808 MB) · Qwen2.5-1.5B (~1,1 GB). El server \
                 (llama-server.exe) viaja con la app; el GGUF se busca solo en \
                 vendor/models y en %APPDATA%/whispr-open/models (también \
                 models-llm/). Si no hay modelo, el dictado sigue funcionando \
                 sin corrección.",
                c.faint,
                ty,
            ));
            main_rows.push(preference_row(
                "Modelo IA local",
                None,
                text_input(
                    "qwen2.5-0.5b-instruct-q4_k_m.gguf",
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
            ));
            main_rows.push(note_row(
                "Nombre del GGUF en models-llm/. Vacío = Qwen2.5-0.5B por defecto.",
                c.faint,
                ty,
            ));
        } else {
            main_rows.push(preference_row(
                "Endpoint LLM",
                None,
                text_input(
                    "https://api.openai.com/v1",
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
            ));
            main_rows.push(note_row(
                "Dirección del servidor de IA (API compatible con OpenAI).",
                c.faint,
                ty,
            ));
            main_rows.push(preference_row(
                "Modelo IA",
                None,
                text_input(
                    "gpt-4o-mini",
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
            ));
            main_rows.push(note_row(
                "El modelo que corrige en la nube.",
                c.faint,
                ty,
            ));
            main_rows.extend(self.api_key_rows(c, ty));
        }
        col = col.push(section_block(
            "Corrección IA",
            design_card(main_rows),
            c,
            ty,
        ));

        // (Los detalles técnicos —espera, sensibilidad, confirmación— están
        // ahora en Avanzado → Corrección IA, sprint final.)
        col.into()
    }

    /// Filas de la clave de API: solo con un proveedor que la pida (Custom —
    /// opcional si el servidor no la pide — y Cloud, obligatoria). Con Local el
    /// servidor no la usa, así que no se pinta nada. Se guarda en el Credential
    /// Manager de Windows, NUNCA en `config.json`.
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
    /// Sprint final: las mismas secciones que las pestañas principales, con las
    /// opciones técnicas movidas aquí (espera, sensibilidad y confirmación de la
    /// IA, servidor LLM, registro). Las pestañas principales quedan limpias.
    pub(crate) fn settings_section_advanced(
        &self,
        c: Chrome,
        ty: DesignType,
        _t: &UiPalette,
    ) -> Element<'_, Message> {
        let log_path = crate::log::log_path().to_string_lossy().into_owned();
        let debug_on = crate::log::debug_enabled();
        let debug_state = if debug_on {
            "WHISPR_DEBUG activado: el registro es detallado"
        } else {
            "WHISPR_DEBUG desactivado"
        };
        let mut col = Column::new()
            .spacing(theme::GRID_SECTION_GAP)
            .width(Length::Fill);
        col = col.push(section_block(
            "Corrección IA (avanzado)",
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
                    "Espera máxima de la IA antes de dar el turno por definitivo: \
                     si lo supera, se inserta sin corregir. Rango 1–120 s.",
                    c.faint,
                    ty,
                ),
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
                    "Con qué frecuencia se envía un turno a la IA. Rango 0–100.",
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
                preference_row(
                    "Server LLM local",
                    Some(
                        "Ruta explícita (opcional). Vacío = el server que viaja con la \
                         app (vendor/llama) y, si no, autodetección (D:/LlamaCPP → PATH).",
                    ),
                    text_input(
                        "(autodetección)",
                        self.config.llama_server_path().unwrap_or(""),
                    )
                    .on_input(Message::LlmServerPathChanged)
                    .padding(theme::FIELD_PADDING)
                    .size(ty.control)
                    .style(theme::text_input)
                    .width(Length::Fill)
                    .into(),
                    c,
                    ty,
                ),
            ]),
            c,
            ty,
        ));
        // (Sin bloque «Dictado (avanzado)»: duplicaba los sliders de la
        // pestaña Dictado — mismo ajuste, dos sitios donde tocarlo.)
        col = col.push(section_block(
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
                    "Rota al pasar de 1 MB (.1/.2). Trazas de arrastre solo con \
                     WHISPR_DEBUG=1.",
                    c.faint,
                    ty,
                ),
                preference_row("Registro detallado", Some(debug_state), no_control(), c, ty),
                preference_row("Assets propios", None, no_control(), c, ty),
                note_row(
                    "Fondo y GIF de la burbuja se copian a assets/: puedes vaciar \
                     Descargas sin perderlos.",
                    c.faint,
                    ty,
                ),
            ]),
            c,
            ty,
        ));
        col.into()
    }
}

/// Navegación de Ajustes del diseño (`Settings navigation`, 43 px): texto de
/// 13 px por pestaña (la activa en texto principal, el resto en secundario) y la
/// raya de 2 px del acento colgando del borde de abajo de la activa. Debajo va
/// la línea fina que cierra la fila, como en el diseño.
fn settings_nav(active: SettingsSection, c: Chrome, ty: DesignType) -> Element<'static, Message> {
    let mut tabs = row![]
        .spacing(theme::GRID_TAB_GAP)
        .align_y(Alignment::End)
        .height(theme::GRID_TAB_HEIGHT);
    for section in SettingsSection::ALL {
        let is_active = section == active;
        // La caja de la pestaña ES la etiqueta (más la raya de 2 px en su borde
        // de abajo, que pinta `theme::tab_underline`). Sin hijos Fill dentro: el
        // ancho sale del texto y «Corrección IA» no parte en dos líneas (con un
        // hijo Fill, iced vuelve a medir el texto a su ancho exacto y el salto de
        // línea queda al azar del redondeo).
        let tab = container(
            text(section.label())
                .size(ty.control)
                .font(if is_active {
                    fonts::medium()
                } else {
                    fonts::regular()
                })
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
