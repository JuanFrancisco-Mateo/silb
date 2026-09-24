//! Manejadores de los mensajes de Ajustes (dictado, corrección IA, nota,
//! listas, apariencia, paleta y motor de voz).
//!
//! Extraído de `WhisprApp::update` en el refactor de organización (cero
//! cambio de conducta): los cuerpos de los `match` se movieron tal cual.

use std::sync::atomic::Ordering;
use std::time::Duration;

use iced::{Task, window};

use crate::theme;
use crate::app::{LlmStatus, Message, SettingsSection, Status, WhisprApp, last_turn_from_transcript};
use crate::config::dictionary_path;
use crate::domain::dictionary::{DictionaryEntry, export_dictionary_to, import_dictionary_from};

impl WhisprApp {
    /// Ajustes de la corrección IA: proveedor, endpoint, modelo, clave,
    /// timeout, server local, umbral del clasificador, confirmación y el
    /// resultado del corrector (diff/in-turn) con su undo.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_corrector_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::CorrectionProviderChanged(provider) => {
                self.config.correction_provider = Some(provider);
                self.save_config();
                // El indicador de estado debe reflejar el proveedor ACTIVO
                // (requisito 10/09/2026): se re-ejecuta la fase LLM contra
                // el nuevo endpoint — local: verifica/arranca el server;
                // remoto: probe con la API key, «inactiva» si no responde.
                // Si hay una fase en vuelo con la config antigua se cancela
                // y se reemplaza (sus eventos caen al dropear el rx).
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
                    // Apagar en caliente (ciclo de vida 10/09/2026): la
                    // corrección pendiente degrada a texto bruto y el modelo
                    // LLM se descarga de memoria — solo el server que lanzó
                    // esta app; un server ajeno no se toca.
                    self.disable_ai_correction_now();
                    return Task::none();
                }
                // Activar la corrección en caliente tras un arranque sin
                // server: relanzar la fase LLM (verificar/arrancar llama-
                // server y esperar a que /v1/models responda) para que el
                // toggle prometa algo que funciona. Si el server no levanta,
                // LlmDisabled muestra el motivo visible.
                //
                // Se evita duplicar una fase SANA en vuelo (startup_rx Some);
                // si la que hay está cancelada (off reciente en caliente),
                // se reemplaza por una nueva sin esperar a que el hilo viejo
                // muera (su token la aborta en el siguiente punto de control).
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
                // Vacío en la UI → cae al default por getter. No
                // sobreescribimos con cadena vacía para distinguir «no
                // escrito» de «borrado intencional»: si el usuario
                // quiere volver al default, borra el campo o edita
                // config.json directamente.
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
                // El slider ya emite 1..=120; el getter clampea a 1 s
                // mínimo igualmente. Guardamos Some para que el getter
                // emita exactamente lo que eligió el usuario.
                self.config.llm_timeout_secs = Some(v as u64);
                self.save_config();
                Task::none()
            }
            Message::LlmServerPathChanged(s) => {
                // Vacío → None (autodetección); con ruta → se trimea y
                // persiste tal cual (la validación de existencia la hace
                // `resolve_server_path` al lanzar, no al teclear).
                let trimmed = s.trim();
                self.config.llama_server_path = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                self.save_config();
                Task::none()
            }
            Message::LlmServerPathBrowse => {
                // Diálogo nativo de Windows (rfd) desde el hilo de UI. Se
                // filtra por llama-server.exe; el resultado (ruta o None
                // si se canceló) vuelve como mensaje.
                Task::perform(
                    async {
                        rfd::FileDialog::new()
                            .set_title("Elegir el ejecutable del server LLM")
                            .add_filter("llama-server", &["exe"])
                            .pick_file()
                            .map(|p| p.to_string_lossy().into_owned())
                    },
                    Message::LlmServerPathPicked,
                )
            }
            Message::LlmServerPathPicked(path) => {
                if let Some(p) = path {
                    // Solo `llama-server.exe` es un server válido
                    // (t_75732630). Si el usuario elige el wrapper
                    // `llama.exe` y junto a él vive el `llama-server.exe`
                    // real, se guarda el real; si no hay ninguno válido se
                    // rechaza y se conserva la config previa (la app sigue
                    // autodetectando en el arranque).
                    let elegido = std::path::PathBuf::from(&p);
                    match crate::llm::sanitize_server_exe(&elegido) {
                        Some(valid) => {
                            if valid != elegido {
                                crate::log!(
                                    "llm",
                                    "«{}» no es llama-server.exe: se guarda el server nativo {}",
                                    elegido.display(),
                                    valid.display()
                                );
                            }
                            self.config.llama_server_path =
                                Some(valid.to_string_lossy().into_owned());
                            self.save_config();
                            crate::log!(
                                "llm",
                                "server LLM configurado: {}",
                                self.config.llama_server_path.as_deref().unwrap_or("")
                            );
                        }
                        None => {
                            crate::log!(
                                "llm",
                                "ruta rechazada: «{}» no es llama-server.exe (el server \
                                 LLM es un proceso nativo de Windows, nunca un wrapper \
                                 ni wsl.exe); se conserva la ruta anterior",
                                elegido.display()
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::LlmServerPathReset => {
                self.config.llama_server_path = None;
                self.save_config();
                Task::none()
            }
            Message::LlmThresholdChanged(v) => {
                self.config.llm_classifier_threshold = Some(v as u8);
                self.save_config();
                Task::none()
            }
            Message::LlmConfirmToggled(on) => {
                self.config.ai_correction_confirm = Some(on);
                // Si se apaga el modo confirmar y hay un diff pendiente,
                // lo descartamos en silencio: el modelo de UI «se olvida»
                // y el usuario puede reintentar dictando otra corrección
                // (la instrucción no se reinserta automáticamente al
                // apagar el toggle — sería sorpresa).
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
                // Reinserta la instrucción hablada como dictado normal
                // (spec §3.3.1: nunca perder texto) y limpia el diff
                // pendiente.
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

    /// Nota editable (R3): edición a mano del transcript, borrado con
    /// confirmación y deshacer, copiado e inyección del último turno.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_note(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::TranscriptEdited(action) => {
                // Editar a mano: se aplica al editor y `transcript`
                // (fuente de verdad del pipeline) se actualiza en la misma
                // acción. `Content::text()` añade un '\n' final de cortesía
                // que NO es del usuario: se recorta para no acumular saltos.
                self.transcript_editor.perform(action);
                let edited = self.transcript_editor.text();
                let edited = edited.trim_end_matches('\n').to_string();
                if edited != self.transcript {
                    self.transcript = edited;
                    // El último turno sigue apuntando al final de la nota:
                    // la corrección cross-turn y «Escribir en app» operan
                    // sobre lo último que el usuario vio/tocó.
                    self.last_turn = last_turn_from_transcript(&self.transcript);
                    self.save_config();
                }
                Task::none()
            }
            Message::ClearPressed => {
                // Armar/desarmar la confirmación de borrado (acción
                // destructiva = explícita; caduca a los 5 s vía Poll).
                self.clear_armed = !self.clear_armed;
                self.clear_armed_at = self.clear_armed.then(std::time::Instant::now);
                Task::none()
            }
            Message::ClearTranscript => {
                // Segundo clic («Sí, borrar»): borra CON backup para
                // deshacer (requisito R3: reversible).
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
                // Un undo de corrección apuntando a un transcript ya
                // borrado sería un resucitar fantasma: se descarta.
                self.undo_state = None;
                // Un diff pendiente cuyo target ya no existe: idem.
                self.pending_diff = None;
                Task::none()
            }
            Message::UndoClearTranscript => {
                // Restaurar la nota borrada (backup del borrado).
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
                        // Confirmación visual breve en el propio botón
                        // (requisito R3): «✓ Copiado» ~2 s vía Poll.
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
            // ── Muletillas (Ajustes: listas editables) ────────────────────
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
                    // Si tras quitar quedan las listas de serie, se guarda
                    // None (el getter resuelve los defaults sin JSON extra).
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
            // ── Diccionario: importar/exportar (backup/restore) ───────────
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

    /// Motor de voz: dispositivo de entrada, idioma, beam size e hilos.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_dictation_engine(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::InputDeviceSelected(name) => {
                self.config.input_device = Some(name);
                // El micrófono se aplica en el siguiente turno de grabación.
                if self.audio.is_none() {
                    self.status = Status::Idle;
                }
                self.save_config();
                Task::none()
            }
            // ── Ajustes avanzados de transcripción ─────────────────────────
            // Idioma crudo: "auto" → autodetección; "es"/"en"/… → BCP-47.
            // Se guarda tal cual en config (el getter decide cómo pasarlo
            // a whisper); no se valida aquí: whisper acepta códigos
            // desconocidos y caerá a "en" silenciosamente si hace falta.
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
            // Beam size: clamp 1..=8 (lo hace el getter; aquí solo
            // persistimos lo que el usuario tecleó y el getter decide).
            Message::BeamSizeChanged(n) => {
                self.config.beam_size = Some(n);
                self.save_config();
                Task::none()
            }
            // Hilos: clamp 1..=32 (idem).
            Message::ThreadsChanged(n) => {
                self.config.n_threads = Some(n);
                self.save_config();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Apariencia: tema, tamaño de fuente, forma/tamaño/opacidad/anclaje y
    /// auto-ocultar de la burbuja, imágenes del usuario y tick de animación.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_appearance_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::ToggleTheme => {
                // El botón del header cicla Auto → Claro → Oscuro (partiendo
                // del modo EFECTIVO actual, incluido el legacy derivado de
                // dark_mode). `dark_mode` legacy queda intacto como fallback
                // para configs sin el campo nuevo.
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
                // Resize EN CALIENTE. La ventana se creó con min/max = rango
                // completo 48–72 (bubble::bubble_settings), así que el resize
                // no se topa con el clamp de WM_GETMINMAXINFO.
                match self.bubble_id {
                    Some(id) => window::resize(id, iced::Size::new(size.px(), size.px())),
                    None => Task::none(),
                }
            }
            Message::BubbleCornerChanged(corner) => {
                self.config.bubble_corner = Some(corner);
                // Re-elegir esquina invalida la posición persistida del último
                // arrastre: la esquina vuelve a mandar (también al arrancar).
                self.config.bubble_x = None;
                self.config.bubble_y = None;
                self.save_config();
                // Mover en caliente a la nueva esquina (winapi SetWindowPos).
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
                // Slider 20–100; clamp defensivo (config.json podría traer
                // otro rango). La burbuja lee el getter en bubble_view.
                self.config.bubble_opacity = Some(percent.clamp(20, 100) as u8);
                self.save_config();
                Task::none()
            }
            Message::BubbleHideTimeoutChanged(timeout) => {
                self.config.bubble_hide_timeout = Some(timeout);
                self.save_config();
                // Si ya había un timer armado (turno terminado, burbuja
                // visible), re-armarlo con el nuevo plazo elegido; si el
                // usuario eligió «Nunca», cancelarlo.
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
                    // Validación mínima al elegir: la ruta existe y la
                    // extensión está soportada. Si falla, NO se persiste
                    // (la app sigue con el fondo anterior o el del tema) y
                    // se avisa por log para diagnóstico.
                    let pb = std::path::PathBuf::from(&p);
                    if crate::custom_image::is_loadable(&pb) {
                        // H4 (QA 24/09/2026): el asset se COPIA a
                        // `%APPDATA%/whispr-open/assets/` y en config queda esa
                        // copia. Antes se guardaba la ruta de origen (p. ej.
                        // Descargas) y al vaciarla la personalización
                        // desaparecía sin avisar.
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
                        .set_title("Elegir sprite estático de la burbuja")
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
                        // Sprite estático siempre presente desactiva la
                        // animación por frames: si el usuario quiere
                        // animación debe limpiar el sprite antes de elegir
                        // una carpeta de frames.
                        // H4: el sprite se copia a la carpeta de datos y se
                        // guarda la COPIA en config (Descargas puede vaciarse).
                        if let Some(dest) = crate::custom_image::import_asset(&pb) {
                            crate::log!(
                                "ui",
                                "sprite de burbuja aplicado: «{}» → copia persistente «{}»",
                                pb.display(),
                                dest.display()
                            );
                            self.config.bubble_image_path =
                                Some(dest.to_string_lossy().into_owned());
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
                    // Validación temprana: si la carpeta está vacía o no
                    // tiene frames, se rechaza (la app mostraría
                    // igualmente el fallback, pero el usuario merece saber
                    // que su selección no sirvió).
                    // H4 (QA 24/09/2026): la secuencia se COPIA a
                    // `%APPDATA%/whispr-open/assets/frames/` y en config queda la
                    // copia (vaciando la carpeta original no se pierde la
                    // animación).
                    let pb = std::path::PathBuf::from(&p);
                    if let Some(dest) = crate::custom_image::import_frames_dir(&pb) {
                        // Reset del frame index por si la lista cambió de
                        // tamaño entre dos elecciones distintas.
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
                // El slider emite 1..=30 (rango definido en la UI); el
                // getter vuelve a clampear como red de seguridad.
                let fps = fps.clamp(
                    u32::from(crate::custom_image::MIN_BUBBLE_FPS),
                    u32::from(crate::custom_image::MAX_BUBBLE_FPS),
                ) as u8;
                self.config.bubble_animation_fps = Some(fps);
                self.save_config();
                Task::none()
            }
            Message::AnimationTick => {
                // Avanza el frame actual SOLO si la animación por frames
                // está activa. Si el usuario la desactivó mientras el
                // Poll seguía vivo, el índice se queda en 0 (la
                // suscripción se desactiva en `subscription()` cuando no
                // hay frames resueltos, pero por defensa también
                // guardamos aquí).
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

    /// Paleta editable (R7): edición por campo, reset, presets e
    /// importar/exportar, más la apertura del panel y sus pestañas.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_palette_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::PaletteFieldChanged(field, value) => {
                // Buffer de input en vuelo (NO se persiste todavía; el
                // usuario confirma con «Aplicar» o sale del campo).
                self.palette_inputs.insert(field, value);
                Task::none()
            }
            Message::PaletteFieldApply(field) => {
                // Toma el buffer en vuelo (o el hex ya persistido si el
                // usuario no tocó nada); parsea robusto y persiste.
                self.apply_palette_field(field);
                Task::none()
            }
            Message::PaletteFieldReset(field) => {
                let cfg = self.config.palette.get_or_insert_with(theme::PaletteConfig::default);
                cfg.clear_field(field);
                if cfg.is_empty() {
                    self.config.palette = None;
                }
                // Limpia también el buffer de input para que el input
                // muestre vacío (lo que la UI interpreta como «default»).
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
                // Refresca los buffers con los nuevos hex (la UI los lee
                // desde el estado para repintar el input).
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
                    // La paleta que exportamos es la RESUELTA (no la
                    // raw PaletteConfig) — así el JSON es directamente
                    // compartible como «una paleta completa» y el import
                    // puede fundirlo con la actual del receptor.
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
                                // FUSIÓN: la paleta importada COMPLEMENTA a
                                // la actual. Un campo presente en el import
                                // sobrescribe; uno ausente mantiene el
                                // valor actual. Esto permite «compartir»
                                // una paleta parcial (p. ej. solo el acento).
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
                // Al cerrar el panel reseteamos a General: la regla del
                // producto es «recordar la última sección solo en el estado
                // de sesión», así que la próxima vez que se abra empezará
                // en lo más común. Al abrir NO reseteamos: el usuario que
                // está alternando entre pestañas no quiere perder su sitio.
                if !self.settings_open {
                    self.settings_section = SettingsSection::General;
                }
                Task::none()
            }
            Message::ToggleAdvancedSettings => {
                // Compatibilidad hacia atrás (R7): el panel «Ajustes
                // avanzados» se eliminó al reorganizar por pestañas. Si
                // alguien lo dispara lo ignoramos sin error.
                Task::none()
            }
            Message::SettingsSectionSelected(section) => {
                self.settings_section = section;
                Task::none()
            }
            _ => Task::none(),
        }
    }

}