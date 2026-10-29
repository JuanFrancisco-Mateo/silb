//! Qué hace cada mensaje de Ajustes (dictado, corrección IA, nota, listas,
//! apariencia, paleta y motor de voz).
//!
//! Salió de `WhisprApp::update` al repartir el fichero (sin cambiar nada de
//! comportamiento): los cuerpos de los `match` se movieron tal cual.

use std::sync::atomic::Ordering;
use std::time::Duration;

use iced::{Task, window};

use crate::theme;
use crate::app::{
    LlmStatus, Message, SettingsSection, Status, WhisprApp, WindowSection,
    last_turn_from_transcript,
};
use crate::config::dictionary_path;
use crate::domain::dictionary::{DictionaryEntry, export_dictionary_to, import_dictionary_from};

impl WhisprApp {
    /// Ajustes de la corrección IA: proveedor, dirección, modelo, clave,
    /// espera, servidor local, umbral del clasificador, confirmación y el
    /// resultado del corrector (cambio o reescritura) con su deshacer.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_corrector_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::CorrectionProviderChanged(provider) => {
                self.config.correction_provider = Some(provider);
                self.save_config();
                // El indicador de estado tiene que enseñar el proveedor que se
                // está usando (pedido del 10/09/2026): se vuelve a lanzar la
                // comprobación con la dirección nueva — en local comprueba y
                // arranca el servidor; en remoto prueba con la clave y queda
                // «inactiva» si no contesta. Si hay una comprobación en marcha
                // con la config vieja, se cancela y se cambia (sus avisos se
                // pierden al soltar el canal).
                if !self.config.ai_correction_enabled() || self.is_starting() {
                    return Task::none();
                }
                if self.startup_rx.is_some() {
                    if let Some(cancel) = &self.llm_phase_cancel {
                        cancel.store(true, Ordering::SeqCst);
                    }
                }
                self.last_correction =
                    Some("Comprobando el nuevo proveedor de corrección…".to_string());
                return self.spawn_llm_startup();
            }
            Message::ToggleAiCorrection(on) => {
                self.config.ai_correction_enabled = Some(on);
                self.save_config();
                if !on {
                    // Apagar con la app abierta (10/09/2026): la corrección
                    // pendiente se queda en texto tal cual y el modelo se descarga
                    // de la memoria — solo el servidor que lanzó esta app; uno de
                    // otro sitio no se toca.
                    self.disable_ai_correction_now();
                    return Task::none();
                }
                // Encender la corrección con la app abierta tras un arranque sin
                // servidor: se vuelve a lanzar la fase (comprobar/arrancar
                // llama-server y esperar a que /v1/models conteste) para que el
                // interruptor prometa algo que funciona. Si el servidor no
                // levanta, LlmDisabled enseña el motivo.
                //
                // No se duplica una fase que ya va bien (startup_rx Some); si la
                // que hay está cancelada (se acaba de apagar), se cambia por una
                // nueva sin esperar a que muera el hilo viejo (su marca la corta
                // en el siguiente punto de control).
                let in_flight_cancelled = self
                    .llm_phase_cancel
                    .as_ref()
                    .map(|c| c.load(Ordering::SeqCst))
                    .unwrap_or(false);
                if on
                    && !matches!(self.llm_status, LlmStatus::Ready { .. })
                    && !self.is_starting()
                    && (self.startup_rx.is_none() || in_flight_cancelled)
                {
                    return self.spawn_llm_startup();
                }
                Task::none()
            }
            Message::LlmEndpointChanged(s) => {
                // Vacío en la interfaz → el getter usa el de por defecto. No se
                // guarda una cadena vacía, para distinguir «no escrito» de
                // «borrado a propósito»: para volver al de por defecto se borra
                // el campo o se edita config.json a mano.
                let trimmed = s.trim();
                self.config.llm_base_url = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                self.save_config();
                Task::none()
            }
            Message::LlmModelChanged(s) => {
                let trimmed = s.trim();
                self.config.llm_model = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                self.save_config();
                Task::none()
            }
            Message::LlmApiKeyChanged(s) => {
                let trimmed = s.trim();
                self.config.llm_api_key = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                self.save_config();
                Task::none()
            }
            Message::LlmTimeoutChanged(v) => {
                // El control ya emite 1..=120; el getter limita a 1 s mínimo de
                // todas formas. Se guarda `Some` para que el getter dé
                // exactamente lo que eligió el usuario.
                self.config.llm_timeout_secs = Some(v as u64);
                self.save_config();
                Task::none()
            }
            Message::LlmThresholdChanged(v) => {
                self.config.llm_classifier_threshold = Some(v as u8);
                self.save_config();
                Task::none()
            }
            Message::LlmServerPathChanged(s) => {
                let trimmed = s.trim();
                self.config.llama_server_path = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                let _ = self.config.heal_llama_server_path();
                self.save_config();
                Task::none()
            }
            Message::LlmConfirmToggled(on) => {
                self.config.ai_correction_confirm = Some(on);
                // Si se apaga el modo confirmar y hay un cambio pendiente, se
                // tira sin avisar: la interfaz «se olvida» y el usuario puede
                // reintentar dictando otra corrección (la instrucción no se
                // vuelve a meter sola al apagar el interruptor — sería una
                // sorpresa).
                if !on {
                    self.pending_diff = None;
                }
                self.save_config();
                Task::none()
            }
            Message::ApplyPendingDiff => {
                self.apply_pending_diff();
                Task::none()
            }
            Message::DiscardPendingDiff => {
                // Vuelve a meter la instrucción hablada como dictado normal
                // (punto 3.3.1: nunca perder texto) y limpia el cambio pendiente.
                self.discard_pending_diff();
                Task::none()
            }
            Message::CorrectionDone(result) => self.handle_llm_correction(result),
            Message::UndoLastCorrection => {
                if let Some((transcript, last_turn)) = self.undo_state.take() {
                    self.transcript = transcript;
                    self.last_turn = last_turn;
                    self.sync_editor_from_transcript();
                    self.last_correction = Some("Corrección IA deshecha ↩".to_string());
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// La nota se puede editar (R3): cambiar el texto a mano, borrar con
    /// confirmación y deshacer, copiar y escribir el último turno.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_note(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::TranscriptEdited(action) => {
                // Editar a mano: se aplica al editor y `transcript` (la fuente
                // de verdad) se actualiza en la misma acción. `Content::text()`
                // añade un '\n' al final que NO es del usuario: se recorta para no
                // acumular saltos.
                self.transcript_editor.perform(action);
                let edited = self.transcript_editor.text();
                let edited = edited.trim_end_matches('\n').to_string();
                if edited != self.transcript {
                    self.transcript = edited;
                    // El último turno sigue apuntando al final de la nota: la
                    // corrección de entre turnos y «Escribir en app» actúan sobre
                    // lo último que el usuario vio o tocó.
                    self.last_turn = last_turn_from_transcript(&self.transcript);
                    self.save_config();
                }
                Task::none()
            }
            Message::ClearPressed => {
                // Armar o desarmar la confirmación de borrado (borrar es
                // destructivo, así que se pide dos veces; caduca a los 5 s con
                // el `Poll`).
                self.clear_armed = !self.clear_armed;
                self.clear_armed_at = self.clear_armed.then(std::time::Instant::now);
                Task::none()
            }
            Message::ClearTranscript => {
                // Segundo clic («Sí, borrar»): borra CON copia para poder
                // deshacer (R3: todo se puede recuperar).
                self.clear_armed = false;
                self.clear_armed_at = None;
                if !self.transcript.is_empty() || !self.last_turn.is_empty() {
                    self.cleared_backup =
                        Some((self.transcript.clone(), self.last_turn.clone()));
                }
                self.transcript.clear();
                self.last_turn.clear();
                self.session.reset();
                self.sync_editor_from_transcript();
                self.last_correction = Some("Nota borrada".to_string());
                // Un deshacer de corrección que apunta a una nota ya borrada
                // resucitaría un fantasma: se descarta.
                self.undo_state = None;
                // Un cambio pendiente cuyo destino ya no existe, igual.
                self.pending_diff = None;
                Task::none()
            }
            Message::UndoClearTranscript => {
                // Recupera la nota borrada (la copia del borrado).
                if let Some((transcript, last_turn)) = self.cleared_backup.take() {
                    self.transcript = transcript;
                    self.last_turn = last_turn;
                    self.sync_editor_from_transcript();
                    self.last_correction = Some("Borrado deshecho ↩".to_string());
                }
                Task::none()
            }
            Message::CopyTranscript => {
                match crate::inject::copy_to_clipboard(&self.transcript) {
                    Ok(()) => {
                        // Aviso breve en el propio botón (R3): «✓ Copiado» ~2 s
                        // con el `Poll`.
                        self.copied_at = Some(std::time::Instant::now());
                        self.last_correction =
                            Some("Transcript copiado al portapapeles ✓".to_string());
                    }
                    Err(e) => {
                        self.status = Status::Error(format!("Copiar: {e}"));
                    }
                }
                Task::none()
            }
            Message::InjectLast => {
                if !self.last_turn.is_empty() {
                    let turn = self.last_turn.clone();
                    self.inject_into_active(&turn);
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Listas de apoyo: diccionario personal, muletillas (añadir, borrar,
    /// restablecer) e importar/exportar el diccionario.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_lists(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::AddEntry => {
                let from = self.entry_from.trim().to_string();
                let to = self.entry_to.trim().to_string();
                if !from.is_empty() && !to.is_empty() {
                    self.config.dictionary.push(DictionaryEntry { from, to });
                    self.entry_from.clear();
                    self.entry_to.clear();
                    self.save_config();
                }
                Task::none()
            }
            Message::EntryFromChanged(v) => {
                self.entry_from = v;
                Task::none()
            }
            Message::EntryToChanged(v) => {
                self.entry_to = v;
                Task::none()
            }
            Message::RemoveEntry(idx) => {
                if idx < self.config.dictionary.len() {
                    self.config.dictionary.remove(idx);
                    self.save_config();
                }
                Task::none()
            }
            // ── Muletillas (listas editables en Ajustes) ──────────────────
            Message::AddFiller => {
                let word = self.filler_input.trim().to_string();
                if !word.is_empty() {
                    let mut fillers = self.config.fillers();
                    let list = self.filler_category.list_mut(&mut fillers);
                    if !list.iter().any(|w| w == &word) {
                        list.push(word);
                        self.config.fillers = Some(fillers);
                        self.save_config();
                    }
                }
                self.filler_input.clear();
                Task::none()
            }
            Message::FillerInputChanged(v) => {
                self.filler_input = v;
                Task::none()
            }
            Message::FillerCategorySelected(category) => {
                self.filler_category = category;
                Task::none()
            }
            Message::RemoveFiller { category, index } => {
                let mut fillers = self.config.fillers();
                let list = category.list_mut(&mut fillers);
                if index < list.len() {
                    list.remove(index);
                    // Si al quitar quedan las listas de serie, se guarda `None`
                    // (el getter pone las de serie sin escribir nada en el JSON).
                    self.config.fillers = if fillers.is_default() {
                        None
                    } else {
                        Some(fillers)
                    };
                    self.save_config();
                }
                Task::none()
            }
            Message::ResetFillers => {
                self.config.fillers = None;
                self.save_config();
                Task::none()
            }
            // ── Diccionario: importar y exportar (copia y restauración) ────
            Message::ExportDictionary => {
                let path = dictionary_path();
                match export_dictionary_to(&path, &self.config.dictionary) {
                    Ok(()) => {
                        self.last_correction =
                            Some(format!("Diccionario exportado a {} ✓", path.display()));
                    }
                    Err(e) => self.status = Status::Error(format!("Exportación: {e}")),
                }
                Task::none()
            }
            Message::ImportDictionary => {
                let path = dictionary_path();
                match import_dictionary_from(&path) {
                    Ok(entries) => {
                        self.config.dictionary = entries;
                        self.save_config();
                        self.last_correction =
                            Some(format!("Diccionario importado desde {} ✓", path.display()));
                    }
                    Err(e) => self.status = Status::Error(format!("Importación: {e}")),
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Motor de voz: micrófono, idioma, beam size e hilos.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_dictation_engine(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::InputDeviceSelected(name) => {
                self.config.input_device = Some(name);
                // El micrófono se usa en la siguiente grabación.
                if self.audio.is_none() {
                    self.status = Status::Idle;
                }
                self.save_config();
                Task::none()
            }
            // ── Ajustes avanzados de transcripción ─────────────────────────
            // Idioma tal cual: "auto" → que lo detecte; "es"/"en"/… → código de
            // idioma. Se guarda como venga en el config (el getter decide cómo
            // pasarlo a whisper); aquí no se comprueba: whisper acepta códigos
            // raros y acabará en "en" sin avisar si hace falta.
            Message::LanguageSelected(value) => {
                let trimmed = value.trim().to_string();
                self.config.language = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
                self.save_config();
                Task::none()
            }
            // Beam size: se limita a 1..=8 en el getter; aquí solo se guarda lo
            // que escribió el usuario.
            Message::BeamSizeChanged(n) => {
                self.config.beam_size = Some(n);
                self.save_config();
                Task::none()
            }
            // Hilos: se limitan a 1..=32 (igual).
            Message::ThreadsChanged(n) => {
                self.config.n_threads = Some(n);
                self.save_config();
                Task::none()
            }
            // ── Atajo global (editable en General desde el sprint final) ───
            Message::ShortcutInputChanged(s) => {
                self.shortcut_input = Some(s);
                Task::none()
            }
            Message::ShortcutApply => {
                let raw = self
                    .shortcut_input
                    .clone()
                    .unwrap_or_else(|| self.config.shortcut());
                match crate::hotkey::HotkeyManager::register(raw.trim()) {
                    Ok(manager) => {
                        self.hotkey_label = manager.label();
                        self.hotkey_rx = manager.listen();
                        self.hotkey = manager;
                        self.config.shortcut =
                            if raw.trim().is_empty() { None } else { Some(raw.trim().to_string()) };
                        self.shortcut_error = None;
                        self.shortcut_input = None;
                        self.placeholder_text =
                            crate::app::transcript_placeholder(&self.hotkey_label);
                        self.save_config();
                    }
                    Err(e) => {
                        self.shortcut_error = Some(e);
                    }
                }
                Task::none()
            }
            Message::ShortcutReset => {
                let def = crate::hotkey::DEFAULT_SHORTCUT.to_string();
                match crate::hotkey::HotkeyManager::register(&def) {
                    Ok(manager) => {
                        self.hotkey_label = manager.label();
                        self.hotkey_rx = manager.listen();
                        self.hotkey = manager;
                        self.config.shortcut = None;
                        self.shortcut_error = None;
                        self.shortcut_input = None;
                        self.placeholder_text =
                            crate::app::transcript_placeholder(&self.hotkey_label);
                        self.save_config();
                    }
                    Err(e) => {
                        self.shortcut_error = Some(e);
                    }
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Apariencia: tema, tamaño de letra, forma/tamaño/opacidad/esquina y
    /// esconder sola de la burbuja, imágenes propias y el avance de la animación.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_appearance_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::ToggleTheme => {
                // El botón de la cabecera pasa Auto → Claro → Oscuro (desde el
                // modo que se esté viendo, incluido el que sale del viejo
                // dark_mode). `dark_mode` se queda como está, por si aparece una
                // config sin el campo nuevo.
                self.config.theme_mode = Some(self.config.theme_mode().next());
                self.save_config();
                Task::none()
            }
            Message::ThemeModeChanged(mode) => {
                self.config.theme_mode = Some(mode);
                self.save_config();
                Task::none()
            }
            Message::FontSizeChanged(size) => {
                self.config.font_size = Some(size);
                self.save_config();
                Task::none()
            }
            Message::BubbleSizeChanged(size) => {
                self.config.bubble_size = Some(size);
                self.save_config();
                // Cambio de tamaño CON la app abierta. La ventana se creó con
                // min/max en todo el rango 48–72 (bubble::bubble_settings), así
                // que el cambio no choca con el límite de WM_GETMINMAXINFO.
                match self.bubble_id {
                    Some(id) => window::resize(id, iced::Size::new(size.px(), size.px())),
                    None => Task::none(),
                }
            }
            Message::BubbleCornerChanged(corner) => {
                self.config.bubble_corner = Some(corner);
                // Elegir otra esquina invalida la posición guardada del último
                // arrastre: la esquina vuelve a mandar (también al arrancar).
                self.config.bubble_x = None;
                self.config.bubble_y = None;
                self.save_config();
                // Mueve la burbuja a la esquina nueva sin reiniciar (SetWindowPos).
                let px = self.config.bubble_size().px();
                crate::bubble::move_to_corner(corner, px);
                Task::none()
            }
            Message::BubbleStyleChanged(style) => {
                self.config.bubble_style = Some(style);
                self.save_config();
                Task::none()
            }
            Message::BubbleOpacityChanged(percent) => {
                // Control de 20–100; se limita por si acaso (el config.json
                // podría traer otro rango). La burbuja lee el getter al dibujarse.
                self.config.bubble_opacity = Some(percent.clamp(20, 100) as u8);
                self.save_config();
                Task::none()
            }
            Message::BubbleHideTimeoutChanged(timeout) => {
                self.config.bubble_hide_timeout = Some(timeout);
                self.save_config();
                // Si ya había un temporizador armado (turno terminado, burbuja a
                // la vista), se vuelve a armar con el plazo nuevo; si el usuario
                // eligió «Nunca», se cancela.
                if self.bubble_hide_deadline.is_some() {
                    self.bubble_hide_deadline = timeout
                        .secs()
                        .map(|s| std::time::Instant::now() + Duration::from_secs(s));
                }
                Task::none()
            }
            // ── Personalización III: imágenes del usuario ──────────────────
            Message::BgImageBrowse => Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Elegir imagen de fondo de la ventana")
                        .add_filter("Imagen", &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"])
                        .pick_file()
                        .map(|p| p.to_string_lossy().into_owned())
                },
                Message::BgImagePicked,
            ),
            Message::BgImagePicked(path) => {
                if let Some(p) = path {
                    // Comprobación mínima al elegir: la ruta existe y la
                    // extensión se acepta. Si falla, NO se guarda (la app sigue con
                    // el fondo de antes o el del tema) y se apunta en el log para
                    // poder diagnosticarlo.
                    let pb = std::path::PathBuf::from(&p);
                    if crate::custom_image::is_loadable(&pb) {
                        // H4 (QA 24/09/2026): el fichero se COPIA a
                        // `%APPDATA%/whispr-open/assets/` y en el config queda esa
                        // copia. Antes se guardaba la ruta de origen (por ejemplo
                        // Descargas) y al vaciarla la personalización desaparecía
                        // sin avisar.
                        if let Some(dest) = crate::custom_image::import_asset(&pb) {
                            crate::log!(
                                "ui",
                                "imagen de fondo aplicada: «{}» → copia persistente «{}»",
                                pb.display(),
                                dest.display()
                            );
                            self.config.bg_image_path =
                                Some(dest.to_string_lossy().into_owned());
                            self.save_config();
                        } else {
                            crate::log!(
                                "ui",
                                "no se pudo copiar la imagen de fondo «{}» a la \
                                 carpeta de datos; se conserva la configuración \
                                 anterior",
                                pb.display()
                            );
                        }
                    } else {
                        crate::log!(
                            "ui",
                            "imagen de fondo rechazada: «{}» no existe o \
                             no es una imagen soportada; se conserva la \
                             configuración anterior",
                            pb.display()
                        );
                    }
                }
                Task::none()
            }
            Message::BgImageClear => {
                self.config.bg_image_path = None;
                self.save_config();
                Task::none()
            }
            Message::BubbleImageBrowse => Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Elegir imagen de la burbuja (PNG o GIF animado)")
                        .add_filter("Imagen", &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"])
                        .pick_file()
                        .map(|p| p.to_string_lossy().into_owned())
                },
                Message::BubbleImagePicked,
            ),
            Message::BubbleImagePicked(path) => {
                if let Some(p) = path {
                    let pb = std::path::PathBuf::from(&p);
                    if crate::custom_image::is_loadable(&pb) {
                        // Si hay una imagen fija puesta, la animación no se usa:
                        // para animar hay que quitar la imagen fija antes de elegir
                        // una carpeta de imágenes. H4: la imagen fija se copia a la
                        // carpeta de datos y en el config queda la COPIA (Descargas
                        // se puede vaciar).
                        if let Some(dest) = crate::custom_image::import_asset(&pb) {
                            crate::log!(
                                "ui",
                                "sprite de burbuja aplicado: «{}» → copia persistente «{}»",
                                pb.display(),
                                dest.display()
                            );
                            self.config.bubble_image_path =
                                Some(dest.to_string_lossy().into_owned());
                            // El GIF se decodifica en el siguiente AnimationTick
                            // (ensure_gif_loaded va por ruta: al cambiarla se
                            // recarga sola). Se limpia la caché aquí para que el
                            // cambio se vea al momento.
                            self.gif_frames.clear();
                            self.gif_loaded_for = None;
                            self.bubble_frame_index = 0;
                            self.save_config();
                        } else {
                            crate::log!(
                                "ui",
                                "no se pudo copiar el sprite de burbuja «{}» a la \
                                 carpeta de datos; se conserva la configuración \
                                 anterior",
                                pb.display()
                            );
                        }
                    } else {
                        crate::log!(
                            "ui",
                            "sprite de burbuja rechazado: «{}»",
                            pb.display()
                        );
                    }
                }
                Task::none()
            }
            Message::BubbleImageClear => {
                self.config.bubble_image_path = None;
                self.gif_frames.clear();
                self.gif_loaded_for = None;
                self.bubble_frame_index = 0;
                self.save_config();
                Task::none()
            }
            Message::BubbleFramesBrowse => Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Elegir carpeta con los frames de la animación")
                        .pick_folder()
                        .map(|p| p.to_string_lossy().into_owned())
                },
                Message::BubbleFramesPicked,
            ),
            Message::BubbleFramesPicked(path) => {
                if let Some(p) = path {
                    // Comprobación previa: si la carpeta está vacía o no tiene
                    // imágenes, se rechaza (la app enseñaría el aspecto de
                    // siempre, pero el usuario merece saber que su elección no
                    // sirvió). H4 (QA 24/09/2026): la secuencia se COPIA a
                    // `%APPDATA%/whispr-open/assets/frames/` y en el config queda
                    // la copia (vaciando la carpeta original no se pierde la
                    // animación).
                    let pb = std::path::PathBuf::from(&p);
                    if let Some(dest) = crate::custom_image::import_frames_dir(&pb) {
                        // Se vuelve a empezar por la primera imagen, por si la
                        // lista cambió de tamaño entre una elección y otra.
                        self.bubble_frame_index = 0;
                        self.config.bubble_frames_path =
                            Some(dest.to_string_lossy().into_owned());
                        self.save_config();
                        crate::log!(
                            "ui",
                            "carpeta de frames aplicada: «{}» → copia persistente «{}»",
                            pb.display(),
                            dest.display()
                        );
                    } else {
                        crate::log!(
                            "ui",
                            "carpeta de frames rechazada: «{}» no contiene \
                             imágenes soportadas",
                            p
                        );
                    }
                }
                Task::none()
            }
            Message::BubbleFramesClear => {
                self.config.bubble_frames_path = None;
                self.bubble_frame_index = 0;
                self.save_config();
                Task::none()
            }
            Message::BubbleFpsChanged(fps) => {
                // El control emite 1..=30 (el rango de la interfaz); el getter
                // vuelve a limitarlo por si acaso.
                let fps = fps.clamp(
                    u32::from(crate::custom_image::MIN_BUBBLE_FPS),
                    u32::from(crate::custom_image::MAX_BUBBLE_FPS),
                ) as u8;
                self.config.bubble_animation_fps = Some(fps);
                self.save_config();
                Task::none()
            }
            Message::AnimationTick => {
                // Primero el GIF de un solo fichero (sprint final); después las
                // carpetas de siempre. Sin animación, el índice se queda en 0.
                self.ensure_gif_loaded();
                if !self.gif_frames.is_empty() {
                    self.bubble_frame_index = crate::custom_image::next_frame_index(
                        self.bubble_frame_index,
                        self.gif_frames.len(),
                    );
                    return Task::none();
                }
                if let Some(frames) = self.bubble_frames_resolved() {
                    if !frames.is_empty() {
                        self.bubble_frame_index =
                            crate::custom_image::next_frame_index(
                                self.bubble_frame_index,
                                frames.len(),
                            );
                    }
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Paleta editable (R7): cambiar campo a campo, restaurar, presets e
    /// importar/exportar, además de abrir el panel y sus pestañas.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_palette_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::PaletteFieldChanged(field, value) => {
                // Texto que se está escribiendo (todavía NO se guarda; el
                // usuario confirma con «Aplicar» o sale del campo).
                self.palette_inputs.insert(field, value);
                Task::none()
            }
            Message::PaletteFieldApply(field) => {
                // Coge lo que se está escribiendo (o el hex ya guardado si el
                // usuario no tocó nada), lo lee con cuidado y lo guarda.
                self.apply_palette_field(field);
                Task::none()
            }
            Message::PaletteFieldReset(field) => {
                let cfg = self.config.palette.get_or_insert_with(theme::PaletteConfig::default);
                cfg.clear_field(field);
                if cfg.is_empty() {
                    self.config.palette = None;
                }
                // Limpia también lo que se está escribiendo, para que el campo
                // salga vacío (la interfaz lo pinta como «por defecto»).
                self.palette_inputs.remove(&field);
                self.save_config();
                Task::none()
            }
            Message::PaletteResetAll => {
                self.config.palette = None;
                self.palette_inputs.clear();
                self.save_config();
                Task::none()
            }
            Message::PalettePresetApply(preset) => {
                let dark = self.config.resolved_dark(self.system_dark);
                let palette = preset.colors(dark);
                self.config.palette = Some(theme::palette_to_config(palette));
                // Refresca los campos con los hex nuevos (la interfaz los lee
                // del estado para repintar).
                self.palette_inputs.clear();
                self.save_config();
                Task::none()
            }
            Message::PaletteExport => Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Exportar paleta a JSON")
                        .add_filter("JSON", &["json"])
                        .set_file_name("whispr-palette.json")
                        .save_file()
                        .map(std::path::PathBuf::from)
                },
                Message::PaletteExportPicked,
            ),
            Message::PaletteExportPicked(path) => {
                if let Some(p) = path {
                    // Se exporta la paleta RESUELTA (no el PaletteConfig crudo)
                    // — así el JSON sirve tal cual como «una paleta completa» y
                    // quien lo importe puede mezclarlo con la suya.
                    let palette =
                        theme::resolve_palette(&self.config, self.config.resolved_dark(self.system_dark));
                    let cfg = theme::palette_to_config(palette);
                    match serde_json::to_string_pretty(&cfg) {
                        Ok(raw) => {
                            if let Err(e) = std::fs::write(&p, raw) {
                                crate::log!(
                                    "palette",
                                    "no se pudo escribir la paleta en {}: {}",
                                    p.display(),
                                    e
                                );
                            } else {
                                crate::log!("palette", "exportada a {}", p.display());
                            }
                        }
                        Err(e) => {
                            crate::log!("palette", "no se pudo serializar la paleta: {e}");
                        }
                    }
                }
                Task::none()
            }
            Message::PaletteImport => Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Importar paleta desde JSON")
                        .add_filter("JSON", &["json"])
                        .pick_file()
                        .map(std::path::PathBuf::from)
                },
                Message::PaletteImportPicked,
            ),
            Message::PaletteImportPicked(path) => {
                if let Some(p) = path {
                    match std::fs::read_to_string(&p) {
                        Ok(raw) => match serde_json::from_str::<theme::PaletteConfig>(&raw) {
                            Ok(imported) => {
                                // MEZCLA: la paleta importada COMPLETA la
                                // actual. Un campo que venga en el import manda;
                                // uno que no venga deja el valor actual. Así se
                                // puede compartir una paleta a medias (por ejemplo
                                // solo el acento).
                                let merged = match self.config.palette.as_mut() {
                                    Some(existing) => {
                                        Self::merge_palette_configs(existing, &imported);
                                        existing.clone()
                                    }
                                    None => imported.clone(),
                                };
                                self.config.palette = Some(merged);
                                self.palette_inputs.clear();
                                self.save_config();
                                crate::log!("palette", "importada desde {}", p.display());
                            }
                            Err(e) => {
                                crate::log!(
                                    "palette",
                                    "JSON inválido en {}: {e}",
                                    p.display()
                                );
                            }
                        },
                        Err(e) => {
                            crate::log!(
                                "palette",
                                "no se pudo leer {}: {e}",
                                p.display()
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::ToggleSettings => {
                self.settings_open = !self.settings_open;
                // Al cerrar el panel se vuelve a General: la regla del producto
                // es «recordar la última sección solo mientras la app está
                // abierta», así que la próxima vez se abre en lo más común. Al
                // abrir NO se reinicia: quien está saltando entre pestañas no
                // quiere perder su sitio.
                if !self.settings_open {
                    self.settings_section = SettingsSection::General;
                }
                Task::none()
            }
            Message::ToggleAdvancedSettings => {
                // Por compatibilidad (R7): el panel «Ajustes avanzados» se quitó
                // al repartir por pestañas. Si alguien lo dispara, se ignora sin
                // error.
                Task::none()
            }
            Message::SettingsSectionSelected(section) => {
                self.settings_section = section;
                Task::none()
            }
            Message::WindowSectionSelected(section) => {
                // La sección de la VENTANA (interfaz del 24/09/2026) se lee de
                // `settings_open` (ver `WhisprApp::window_section`): «Ajustes»
                // abre el panel y «Dictado» lo cierra. Al cerrar se vuelve a
                // General — la misma regla del botón Ajustes de siempre (la
                // sección se recuerda solo en la sesión).
                let abrir_ajustes = matches!(section, WindowSection::Ajustes);
                self.settings_open = abrir_ajustes;
                if !abrir_ajustes {
                    self.settings_section = SettingsSection::General;
                }
                Task::none()
            }
            Message::OpenSettingsSection(section) => {
                // Atajo del marco (ayuda → Avanzado, controles → Dictado): abre
                // Ajustes EN esa sección, igual que la navegación de la barra
                // lateral (`settings_open`).
                self.settings_open = true;
                self.settings_section = section;
                Task::none()
            }
            _ => Task::none(),
        }
    }

}