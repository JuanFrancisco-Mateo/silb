//! Marco del rediseño: barra de ventana propia, barra lateral y el espacio de
//! trabajo de «Dictado» (frame `3:484` de Figma).
//!
//! Fuente de verdad de medidas y colores: `design/openwhispr-figma/raw/
//! dictado-design-context.txt` (capa a capa) y `metadata-arbol-capas.txt`
//! (coordenadas). Aquí no hay ni un literal de color ni de tamaño de fuente:
//! los tonos vienen de [`crate::theme::chrome`] y la escala tipográfica de
//! [`crate::theme::design_type`] (ambos derivados del diseño).
//!
//! Estructura del frame (1200×820):
//!
//! ```text
//! Window bar 40 ──────────────────────────────────────────────
//! Sidebar 204 │ Dictation workspace (pt 32 · px 40 · pb 24 · gap 24)
//!             │   Page heading · Recording console · Transcript editor · Footer
//! ```
//!
//! El espacio de trabajo de «Ajustes» sigue siendo el panel de siempre
//! (`settings_view`): el frame de Ajustes es otra tarjeta del tablero.

use iced::widget::text::Shaping;
use iced::widget::{
    button, column, container, horizontal_rule, mouse_area, pick_list, row, text, text_editor,
    toggler, Space,
};
use iced::{Alignment, Element, Length, Padding};

use crate::app::{
    feedback_is_warning, language_label, secondary_button, ui_text, Message, SettingsSection,
    Status, WhisprApp, WindowSection, LANGUAGE_OPTIONS,
};
use crate::theme::{self, Chrome, DesignType};
use crate::ui::icons;

/// Margen izquierdo único de la barra lateral: marca, items de navegación y
/// pie comparten la misma x para su contenido (el fondo del item activo sigue
/// empezando en el borde del sidebar; el icono cae donde el logo).
const SIDEBAR_CONTENT_PAD: f32 = 12.0;

/// Nº de barras del medidor de nivel del micrófono (18 en el diseño, de 3 px
/// de ancho y 3 px de separación: 18*3 + 17*3 = 105 px de ancho).
const LEVEL_BARS: usize = 18;

/// Alturas (px) de las barras del medidor en el diseño. Son la FORMA del
/// medidor; la altura real de cada barra se escala con el nivel del micrófono
/// (con el micro en silencio quedan en 3 px, el mínimo).
const LEVEL_SHAPE: [u16; LEVEL_BARS] = [3, 5, 7, 4, 10, 6, 4, 8, 5, 3, 6, 4, 8, 5, 3, 6, 4, 3];

impl WhisprApp {
    /// `Window bar` (40 px): título, control de tema y controles de ventana
    /// propios (minimizar / maximizar / cerrar, 12 px). La zona del título
    /// arrastra la ventana (la decoración nativa está desactivada).
    pub(crate) fn frame_window_bar(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let theme_toggle = match self.config.theme_mode() {
            crate::config::ThemeMode::Auto => "◐ Auto",
            crate::config::ThemeMode::Claro => "☀ Claro",
            crate::config::ThemeMode::Oscuro => "☾ Oscuro",
        };
        let drag_zone = mouse_area(
            container(text("Silb").size(ty.chrome).color(c.faint))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .on_press(Message::DragMainWindow);

        let mut controls = row![].spacing(12.0).align_y(Alignment::Center);
        for (bytes, message) in [
            (icons::MINUS, Some(Message::MinimizeMainWindow)),
            (icons::SQUARE, Some(Message::MaximizeMainWindow)),
            (
                icons::X,
                self.main_window_id.map(Message::WindowCloseRequested),
            ),
        ] {
            let control = button(icons::icon(bytes, 12.0))
                .padding(Padding {
                    top: 4.0,
                    right: 6.0,
                    bottom: 4.0,
                    left: 6.0,
                })
                .style(theme::icon_button);
            let item: Element<'_, Message> = match message {
                Some(message) => control.on_press(message).into(),
                None => control.into(),
            };
            controls = controls.push(item);
        }

        container(
            row![
                drag_zone,
                secondary_button(theme_toggle).on_press(Message::ToggleTheme),
                controls,
            ]
            .spacing(16.0)
            .align_y(Alignment::Center)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(theme::WINDOW_BAR_HEIGHT)
        .padding(Padding {
            top: 0.0,
            right: 16.0,
            bottom: 0.0,
            left: 20.0,
        })
        .style(theme::window_bar)
        .into()
    }

    /// `Sidebar` (204 px): marca, navegación, hueco elástico y versión + ayuda.
    pub(crate) fn frame_sidebar(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        // Un único margen izquierdo para marca, nav y pie: el contenido arranca
        // en la misma x que el icono de los items de navegación.
        let brand = row![
            icons::icon(icons::WHISPR_SYMBOL, 32.0),
            // Marca: solo el texto, el logo (el cuadrado con la onda) se queda.
            text("Silb").size(ty.brand).color(c.text),
        ]
        .spacing(10.0)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: SIDEBAR_CONTENT_PAD,
        });

        // Los dos destinos de la navegación salen de `WindowSection::ALL` (el
        // orden y las etiquetas viven en el enum, no duplicados aquí).
        let mut nav = column![].spacing(6.0).width(Length::Fill);
        for section in WindowSection::ALL {
            nav = nav.push(nav_item(
                match section {
                    WindowSection::Dictado => icons::AUDIO_LINES,
                    WindowSection::Ajustes => icons::SLIDERS,
                },
                section.label(),
                section == self.window_section(),
                Message::WindowSectionSelected(section),
                c,
                ty,
            ));
        }

        // Ayuda: abre Ajustes → Avanzado, que es donde vive lo técnico
        // (ruta del registro, carpeta de la app) que un usuario busca aquí.
        let version = row![
            text(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(ty.chrome)
                .color(c.faint),
            Space::with_width(Length::Fill),
            button(icons::icon(icons::CIRCLE_HELP, 15.0))
                .padding(2.0)
                .on_press(Message::OpenSettingsSection(SettingsSection::Avanzado))
                .style(theme::icon_button),
        ]
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: SIDEBAR_CONTENT_PAD,
            bottom: 0.0,
            left: SIDEBAR_CONTENT_PAD,
        });

        container(
            column![brand, nav, Space::with_height(Length::Fill), version,]
                .spacing(32.0)
                .padding(Padding {
                    top: 28.0,
                    right: 16.0,
                    bottom: 20.0,
                    left: 16.0,
                }),
        )
        .width(theme::SIDEBAR_WIDTH)
        .height(Length::Fill)
        .style(theme::sidebar)
        .into()
    }

    /// Espacio de trabajo del frame Dictado: cabecera de página, tarjeta de
    /// grabación, tarjeta de transcripción y pie.
    pub(crate) fn dictado_workspace(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &theme::UiPalette,
    ) -> Element<'_, Message> {
        let heading = column![
            text("Dictado").size(ty.title).color(c.text),
            text("Tu voz, convertida en texto.")
                .size(ty.control)
                .color(c.secondary),
        ]
        .spacing(6.0)
        .width(Length::Fill);

        let mut workspace = column![
            heading,
            self.recording_card(c, ty, t),
            self.transcript_card(c, ty),
        ]
        .spacing(24.0)
        .height(Length::Fill);

        // El feedback del turno (corrección pendiente, undo, muletillas) solo
        // ocupa sitio cuando hay algo que decir.
        if let Some(feedback) = self.dictado_feedback(c, ty) {
            workspace = workspace.push(feedback);
        }
        workspace = workspace.push(self.dictation_footer(c, ty));

        container(workspace)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 32.0,
                right: 40.0,
                bottom: 24.0,
                left: 40.0,
            })
            .into()
    }

    /// Espacio de trabajo de Ajustes (frame `3:604`): cabecera de página,
    /// navegación de pestañas, contenido con scroll y pie, con el mismo margen
    /// (pt 32 · px 40 · pb 24) que el espacio de trabajo de Dictado.
    pub(crate) fn settings_workspace(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &theme::UiPalette,
    ) -> Element<'_, Message> {
        container(self.settings_view(c, ty, t))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 32.0,
                right: 40.0,
                bottom: 24.0,
                left: 40.0,
            })
            .into()
    }

    /// `Recording console` (190 px): botón circular de 80 px, estado, atajo en
    /// keycaps, medidor de nivel y fila de preferencias (micrófono, idioma,
    /// modelo).
    fn recording_card(
        &self,
        c: Chrome,
        ty: DesignType,
        t: &theme::UiPalette,
    ) -> Element<'_, Message> {
        let mic = self.mic_ui(t);
        let dot_color = mic.color;
        let recording = matches!(self.status, Status::Recording);
        let level = if recording {
            self.mic_level_percent()
        } else {
            0
        };

        let record_button = button(
            container(icons::icon(icons::MIC, 30.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(theme::RECORD_BUTTON_SIZE)
        .height(theme::RECORD_BUTTON_SIZE)
        .padding(0.0)
        .on_press(Message::Toggle)
        .style(theme::record);

        let mut status = column![
            row![
                container(Space::new(6.0, 6.0)).style(move |_| theme::dot(dot_color)),
                text(mic.label).size(ty.body).color(c.secondary),
            ]
            .spacing(7.0)
            .align_y(Alignment::Center),
            text(record_headline(&self.status))
                .size(ty.hero)
                .color(c.text),
            text(mic.hint)
                .size(ty.body)
                .color(if matches!(self.status, Status::Error(_)) {
                    // El motivo de un error va en rojo (el resto de pistas,
                    // tenues como en el diseño): un fallo nunca se esconde.
                    t.err
                } else {
                    c.faint
                }),
        ]
        .spacing(8.0)
        .width(Length::Fill);

        // Progreso de descarga (R4: tamaño + ETA, no solo un porcentaje) y
        // aviso de pico del micrófono: se conservan bajo la pista.
        match &self.status {
            Status::Downloading {
                downloaded, total, ..
            } if *total > 0 => {
                let pct = (*downloaded as f32 / *total as f32 * 100.0).min(100.0);
                status = status.push(
                    iced::widget::progress_bar(0.0..=100.0, pct)
                        .width(Length::Fill)
                        .style(move |th| theme::progress(th, c.accent)),
                );
            }
            Status::Recording => {
                let mut pista = row![
                    text(format!("pico {level}%"))
                        .size(ty.chrome)
                        .color(c.faint),
                    Space::with_width(Length::Fill),
                ]
                .align_y(Alignment::Center);
                if let Some(hint) = self.mic_peak_hint() {
                    pista = pista.push(ui_text(hint.label()).size(ty.chrome).color(t.warn));
                }
                status = status.push(pista);
            }
            _ => {}
        }

        let shortcut = column![self.keycaps(c, ty), self.mic_level(level)]
            .spacing(12.0)
            .align_x(Alignment::End);

        let control = row![record_button, status, shortcut]
            .spacing(24.0)
            .align_y(Alignment::Center)
            .padding(24.0)
            .width(Length::Fill);

        container(column![
            container(control).width(Length::Fill),
            horizontal_rule(1).style(theme::divider_rule),
            self.recording_preferences(c, ty),
        ])
        .width(Length::Fill)
        .style(theme::card)
        .into()
    }

    /// Fila de preferencias de grabación (62 px): micrófono, idioma, modelo en
    /// uso y acceso a Ajustes → Dictado.
    fn recording_preferences(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let mic_pick = pick_list(
            self.input_devices.clone(),
            self.config.input_device.clone(),
            Message::InputDeviceSelected,
        )
        .placeholder("(por defecto)")
        .width(244.0)
        .text_size(ty.control)
        .padding(Padding {
            top: 10.0,
            right: 12.0,
            bottom: 10.0,
            left: 12.0,
        })
        .style(theme::pick_list);

        let lang_pick = pick_list(
            LANGUAGE_OPTIONS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            Some(language_label(&self.config.language)),
            Message::LanguageSelected,
        )
        .placeholder("Idioma")
        .width(160.0)
        .text_size(ty.control)
        .padding(Padding {
            top: 10.0,
            right: 12.0,
            bottom: 10.0,
            left: 12.0,
        })
        .style(theme::pick_list);

        let model = row![
            icons::icon(icons::CPU, 15.0),
            text(self.config.selected_model().label())
                .size(ty.body)
                .color(c.secondary),
        ]
        .spacing(7.0)
        .align_y(Alignment::Center);

        container(
            row![
                mic_pick,
                lang_pick,
                container(model).width(Length::Fill),
                button(icons::icon(icons::SLIDERS_SM, 16.0))
                    .padding(2.0)
                    .on_press(Message::OpenSettingsSection(SettingsSection::Dictado))
                    .style(theme::icon_button),
            ]
            .spacing(12.0)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(62.0)
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 24.0,
        })
        .into()
    }

    /// Atajo global en keycaps (un keycap por tecla del atajo registrado).
    fn keycaps(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let mut keys = row![].spacing(4.0).align_y(Alignment::Center);
        for key in keycap_labels(&self.hotkey_label) {
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
        keys.into()
    }

    /// Medidor de nivel del micrófono (105×17 px del diseño).
    fn mic_level(&self, level: u32) -> Element<'_, Message> {
        let mut bars = row![].spacing(3.0).height(17.0).align_y(Alignment::Center);
        for height in level_bar_heights(level) {
            bars = bars.push(
                container(Space::with_width(3.0))
                    .width(3.0)
                    .height(height as f32)
                    .style(theme::level_bar),
            );
        }
        bars.into()
    }

    /// `Transcript editor`: cabecera con acciones, texto editable y pie con el
    /// contador de palabras.
    fn transcript_card(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let toolbar = container(
            row![
                text("Transcripción").size(ty.nav).color(c.text),
                Space::with_width(Length::Fill),
                self.clear_control(ty),
                self.inject_control(ty),
                self.copy_control(c, ty),
            ]
            .spacing(10.0)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(64.0)
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 24.0,
        });

        let body = container(
            text_editor(&self.transcript_editor)
                .placeholder(self.placeholder_text.as_str())
                .on_action(Message::TranscriptEdited)
                .size(ty.transcript)
                .line_height(1.7)
                .padding(0.0)
                .height(Length::Fill)
                .style(theme::transcript_editor),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 26.0,
            right: 28.0,
            bottom: 24.0,
            left: 28.0,
        });

        let status = container(
            row![
                row![
                    icons::icon(icons::PENCIL, 13.0),
                    text("Texto editable").size(ty.chrome).color(c.faint),
                ]
                .spacing(7.0)
                .align_y(Alignment::Center),
                Space::with_width(Length::Fill),
                text(format!("{} palabras", word_count(&self.transcript)))
                    .size(ty.chrome)
                    .color(c.faint),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(42.0)
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 24.0,
        });

        container(column![
            toolbar,
            horizontal_rule(1).style(theme::divider_rule),
            body,
            horizontal_rule(1).style(theme::divider_rule),
            status,
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::card)
        .into()
    }

    /// «Limpiar» con la confirmación armada de siempre (borrar la nota es
    /// destructivo y explícito: primer clic arma, segundo borra). Con la nota
    /// vacía no hay nada que borrar: el botón sale DESHABILITADO (iced pinta
    /// `Status::Disabled`, que ahora se ve apagado) en vez de activo.
    fn clear_control(&self, _ty: DesignType) -> Element<'_, Message> {
        if self.clear_armed {
            return row![
                text("¿Borrar la nota?")
                    .size(theme::FONT_XS)
                    .color(theme::WARN),
                button(ui_text("Sí, borrar"))
                    .on_press(Message::ClearTranscript)
                    .padding(theme::SECONDARY_BUTTON_PADDING)
                    .style(theme::primary),
                secondary_button("No").on_press(Message::ClearPressed),
            ]
            .spacing(8.0)
            .align_y(Alignment::Center)
            .into();
        }
        let limpiar = button(
            row![
                icons::icon(icons::TRASH, 15.0),
                ui_text("Limpiar").size(theme::FONT_SM),
            ]
            .spacing(7.0)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 8.0,
            right: 12.0,
            bottom: 8.0,
            left: 12.0,
        })
        .style(theme::secondary);
        if word_count(&self.transcript) > 0 {
            limpiar.on_press(Message::ClearPressed).into()
        } else {
            limpiar.into()
        }
    }

    /// «Otra vez» / «Escribir en app»: reinserción del último turno.
    fn inject_control(&self, _ty: DesignType) -> Element<'_, Message> {
        let label = if self.config.auto_inject.unwrap_or(true) {
            "Otra vez"
        } else {
            "Escribir en app"
        };
        if self.last_turn.is_empty() {
            secondary_button(label).into()
        } else {
            secondary_button(label).on_press(Message::InjectLast).into()
        }
    }

    /// «Copiar texto» (acción principal de la tarjeta). Sin texto no hay nada
    /// que copiar: sale DESHABILITADO (no activo).
    fn copy_control(&self, _c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let label = if self.copied_at.is_some() {
            "✓ Copiado"
        } else {
            "Copiar texto"
        };
        let copiar = button(
            row![
                icons::icon(icons::COPY, 16.0),
                ui_text(label).size(ty.control),
            ]
            .spacing(8.0)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: theme::ACTION_BUTTON_PADDING[0],
            right: theme::ACTION_BUTTON_PADDING[1],
            bottom: theme::ACTION_BUTTON_PADDING[0],
            left: theme::ACTION_BUTTON_PADDING[1],
        })
        .style(theme::primary);
        if word_count(&self.transcript) > 0 {
            copiar.on_press(Message::CopyTranscript).into()
        } else {
            copiar.into()
        }
    }

    /// Feedback del último turno (diff pendiente, corrección aplicada con
    /// undo, muletillas eliminadas). `None` si no hay nada que decir: el
    /// diseño no reserva espacio para esta línea.
    fn dictado_feedback(&self, c: Chrome, ty: DesignType) -> Option<Element<'_, Message>> {
        if let Some(_diff) = self.pending_diff.as_ref() {
            let label = self
                .last_correction
                .as_deref()
                .unwrap_or("Corrección IA pendiente de confirmación");
            return Some(
                row![
                    text(label)
                        .size(ty.body)
                        .color(c.accent_active)
                        .shaping(Shaping::Advanced),
                    button(ui_text("Aplicar"))
                        .on_press(Message::ApplyPendingDiff)
                        .padding(theme::SECONDARY_BUTTON_PADDING)
                        .style(theme::primary),
                    secondary_button("Descartar").on_press(Message::DiscardPendingDiff),
                ]
                .spacing(8.0)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        if let Some(corr) = &self.last_correction {
            let color = if feedback_is_warning(corr) {
                theme::WARN
            } else {
                theme::OK
            };
            let mut fila = row![text(corr.as_str())
                .size(ty.body)
                .color(color)
                .shaping(Shaping::Advanced)]
            .spacing(8.0)
            .align_y(Alignment::Center);
            if self.undo_state.is_some() {
                fila =
                    fila.push(secondary_button("↩ Deshacer").on_press(Message::UndoLastCorrection));
            }
            if self.cleared_backup.is_some() {
                fila = fila.push(
                    secondary_button("↩ Deshacer borrado").on_press(Message::UndoClearTranscript),
                );
            }
            return Some(fila.into());
        }
        if !self.removed_fillers.is_empty() {
            return Some(
                text(format!(
                    "Muletillas eliminadas: {}",
                    self.removed_fillers.join(", ")
                ))
                .size(ty.body)
                .color(c.faint)
                .into(),
            );
        }
        None
    }

    /// `Dictation footer`: interruptor de corrección IA + promesa de
    /// privacidad (honesta: si el corrector sale a la nube, lo dice).
    fn dictation_footer(&self, c: Chrome, ty: DesignType) -> Element<'_, Message> {
        let enabled = self.config.ai_correction_enabled();
        let ai_state = if !enabled {
            "Desactivada".to_string()
        } else {
            match &self.llm_status {
                crate::app::LlmStatus::Ready { .. } => {
                    format!("Activa · {}", self.config.llm_settings().model)
                }
                crate::app::LlmStatus::Disabled(reason) if !reason.is_empty() => {
                    format!("Inactiva — {reason}")
                }
                _ => "Comprobando…".to_string(),
            }
        };

        // Aviso honesto SOLO cuando el corrector sale a la nube: con la
        // corrección local (o apagada) el texto «nada sale de tu PC» era
        // redundante con el README y se retira.
        let cloud_correction = enabled
            && matches!(
                self.config.correction_provider(),
                crate::config::CorrectionProvider::Cloud
            );
        let mut footer = row![
            row![
                icons::icon(icons::SPARKLES, 15.0),
                text("Corrección IA").size(ty.body).color(c.secondary),
                toggler(enabled)
                    .on_toggle(Message::ToggleAiCorrection)
                    .size(22.0)
                    .style(theme::toggler),
                text(ai_state).size(ty.chrome).color(c.faint),
            ]
            .spacing(10.0)
            .align_y(Alignment::Center),
            Space::with_width(Length::Fill),
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center);
        if cloud_correction {
            footer = footer.push(
                row![
                    icons::icon(icons::HARD_DRIVE, 14.0),
                    text(crate::app::CLOUD_CORRECTION_NOTICE)
                        .size(ty.chrome)
                        .color(c.faint),
                ]
                .spacing(7.0)
                .align_y(Alignment::Center),
            );
        }
        footer.into()
    }
}

/// Un elemento de navegación de la barra lateral (40 px de alto).
fn nav_item<'a>(
    bytes: &'static [u8],
    label: &'static str,
    active: bool,
    message: Message,
    c: Chrome,
    ty: DesignType,
) -> Element<'a, Message> {
    button(
        row![
            icons::icon(bytes, 18.0),
            text(label)
                .size(ty.nav)
                .color(if active { c.text } else { c.secondary }),
        ]
        .spacing(11.0)
        .align_y(Alignment::Center),
    )
    .height(40.0)
    .width(Length::Fill)
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: SIDEBAR_CONTENT_PAD,
    })
    .on_press(message)
    .style(move |th, status| theme::nav_item(th, status, active))
    .into()
}

/// Acción principal de la tarjeta de grabación (texto grande del diseño:
/// «Iniciar dictado» cuando está listo).
fn record_headline(status: &Status) -> &'static str {
    match status {
        Status::Idle => "Iniciar dictado",
        Status::Recording => "Detener y transcribir",
        Status::Transcribing => "Transcribiendo…",
        Status::Correcting => "Corrigiendo con IA…",
        Status::Downloading { .. } => "Descargando el modelo…",
        Status::Error(_) => "Error",
    }
}

/// Teclas del atajo registrado (`Ctrl+Shift+D` → `["Ctrl", "Shift", "D"]`).
fn keycap_labels(hotkey_label: &str) -> Vec<&str> {
    hotkey_label.split('+').map(str::trim).collect()
}

/// Palabras de la nota (el contador del pie del editor del diseño).
fn word_count(transcript: &str) -> usize {
    transcript.split_whitespace().count()
}

/// Altura de cada barra del medidor para un nivel 0..=100. Con el micro en
/// silencio todas quedan en el mínimo (3 px): el medidor nunca miente.
fn level_bar_heights(level: u32) -> [u16; LEVEL_BARS] {
    let level = level.min(100) as f32;
    let mut heights = [3u16; LEVEL_BARS];
    for (slot, shape) in heights.iter_mut().zip(LEVEL_SHAPE) {
        *slot = ((shape as f32 * level / 100.0).round() as u16).max(3);
    }
    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_accion_principal_cambia_con_el_estado_sin_perder_la_pista() {
        assert_eq!(record_headline(&Status::Idle), "Iniciar dictado");
        assert_eq!(record_headline(&Status::Recording), "Detener y transcribir");
        assert_eq!(record_headline(&Status::Transcribing), "Transcribiendo…");
        assert_eq!(record_headline(&Status::Correcting), "Corrigiendo con IA…");
        assert_eq!(
            record_headline(&Status::Downloading {
                model: crate::transcribe::Model::Base,
                downloaded: 1,
                total: 2
            }),
            "Descargando el modelo…"
        );
        assert_eq!(record_headline(&Status::Error("x".into())), "Error");
    }

    #[test]
    fn las_teclas_del_atajo_salen_del_atajo_registrado() {
        assert_eq!(keycap_labels("Ctrl+Shift+D"), vec!["Ctrl", "Shift", "D"]);
        assert_eq!(keycap_labels("Alt+Space"), vec!["Alt", "Space"]);
        assert_eq!(
            keycap_labels("Ctrl+Alt+Shift+Space"),
            vec!["Ctrl", "Alt", "Shift", "Space"]
        );
    }

    #[test]
    fn el_contador_de_palabras_cuenta_palabras_de_verdad() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("  \n "), 0);
        assert_eq!(word_count("El jueves revisamos la propuesta"), 5);
        assert_eq!(word_count("una\ndos\ttres"), 3);
    }

    #[test]
    fn el_medidor_nunca_miente_en_silencio() {
        let silencio = level_bar_heights(0);
        assert_eq!(silencio, [3u16; LEVEL_BARS]);
        // A tope, cada barra llega a su forma máxima del diseño.
        assert_eq!(level_bar_heights(100), LEVEL_SHAPE);
        // Monótono y nunca por debajo del mínimo.
        let medio = level_bar_heights(50);
        assert!(medio.iter().all(|h| *h >= 3));
        assert!(medio.iter().zip(silencio).all(|(a, b)| *a >= b));
        assert!(medio.iter().zip(LEVEL_SHAPE).all(|(a, b)| *a <= b));
        assert!(level_bar_heights(999).iter().all(|h| *h <= 10));
    }
}
