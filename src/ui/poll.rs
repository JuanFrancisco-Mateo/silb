//! Tick del `Poll`: drena los canales de los hilos y vigila los plazos.

use std::sync::mpsc;
use std::time::Duration;

use iced::{Task, window};

use crate::app::{Message, StartupPhase, Status, WhisprApp};
use crate::config::ThemeMode;
use crate::theme;

impl WhisprApp {
    /// Tick del `Poll` (10 Hz): drena los canales de los hilos (atajo
    /// global, bandeja, errores de audio, descargas, arranque, LLM),
    /// vigila el tope de grabación, refresca el tema del sistema y avanza
    /// la animación de la burbuja.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_poll(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Poll => {
                // ── Arranque: eventos del hilo background + spinner ────────
                // El prerroll de whisper y la verificación del server LLM
                // corren en su hilo; aquí solo se drena el canal (nunca
                // bloquea) y se anima el spinner mientras la pantalla de
                // carga esté visible.
                if let Some(rx) = &self.startup_rx {
                    match rx.try_recv() {
                        Ok(event) => return self.update(Message::Startup(event)),
                        Err(mpsc::TryRecvError::Empty) => {
                            if matches!(self.startup, StartupPhase::Loading) {
                                self.startup_tick = self.startup_tick.wrapping_add(1);
                            }
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            // El hilo murió sin evento terminal: no dejar la
                            // app colgada en «cargando».
                            self.startup_rx = None;
                            if matches!(self.startup, StartupPhase::Loading) {
                                self.startup = StartupPhase::Failed {
                                    error: "El arranque terminó inesperadamente".to_string(),
                                };
                            }
                        }
                    }
                }

                // ── Tema auto: refresco periódico del estado del sistema ──
                // Cada 5 s se relee AppsUseLightTheme (registro). El view se
                // redibuja con cada tick del Poll (100 ms), así que el cambio
                // de tema se propaga solo, sin tareas extra.
                if self.last_theme_check.elapsed() >= Duration::from_secs(5) {
                    self.last_theme_check = std::time::Instant::now();
                    if self.config.theme_mode() == ThemeMode::Auto {
                        self.system_dark = theme::system_dark_mode();
                    }
                }

                // ── Auto-ocultar la burbuja ────────────────────────────────
                // Timer armado al terminar un turno (arm_bubble_hide_timeout):
                // al expirar, la burbuja se oculta. Se re-muestra al pulsar el
                // micrófono o al abrirla.
                if let Some(deadline) = self.bubble_hide_deadline {
                    if std::time::Instant::now() >= deadline {
                        self.bubble_hide_deadline = None;
                        if let Some(id) = self.bubble_id {
                            self.bubble_hidden = true;
                            return window::change_mode(id, window::Mode::Hidden);
                        }
                    }
                }

                // ── Robustez de grabación ────────────────────────────────
                // 1) El stream puede morir a mitad de turno (micro
                //    desconectado, driver caído): el canal de errores lo
                //    cuenta aquí y el turno se corta con lo grabado.
                // 2) Un turno maratoniano se detiene solo al tope
                //    (`audio::MAX_RECORDING_SECS`): memoria acotada y el
                //    audio grabado hasta ahí se transcribe con normalidad.
                if let Some(audio) = &self.audio {
                    if let Some(reason) = audio.take_stream_error() {
                        return self.update(Message::AudioDeviceLost(reason));
                    }
                }
                if let Some(started) = self.recording_started {
                    if started.elapsed()
                        >= Duration::from_secs(crate::audio::MAX_RECORDING_SECS)
                    {
                        crate::log!(
                            "audio",
                            "tope de {} s de grabación: el turno se detiene solo",
                            crate::audio::MAX_RECORDING_SECS
                        );
                        // El aviso viaja como nota pendiente: se muestra al
                        // llegar la transcripción (apply_pipeline limpia
                        // last_correction en el camino y se comería un aviso
                        // puesto ya).
                        self.pending_turn_note = Some(format!(
                            "Dictado larguísimo: parado a las {} h para no agotar la memoria; \
                             lo grabado hasta ahí se transcribe normal.",
                            crate::audio::MAX_RECORDING_SECS / 3600
                        ));
                        return self.update(Message::StopRecording);
                    }
                }

                // ── Confirmaciones efímeras ─────────────────────────────
                // «✓ Copiado» vive ~2 s en el botón; la confirmación de
                // borrado armada caduca a los 5 s (un «¿Borrar?» olvidado
                // no puede esperar para siempre).
                if self
                    .copied_at
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
                {
                    self.copied_at = None;
                }
                if self
                    .clear_armed_at
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(5))
                {
                    self.clear_armed = false;
                    self.clear_armed_at = None;
                }

                // Atajo global: toggle al detectar press de NUESTRO atajo (se filtra
                // por id: el canal es global y podría traer eventos ajenos).
                let mut toggled = false;
                let hotkey_id = self.hotkey.id();
                for _ in 0..4 {
                    match self.hotkey_rx.try_recv() {
                        Ok(ev) => {
                            if ev.id() == hotkey_id
                                && ev.state() == global_hotkey::HotKeyState::Pressed
                                && !toggled
                            {
                                toggled = true;
                            }
                        }
                        Err(_) => break,
                    }
                }
                if toggled {
                    return self.update(Message::Toggle);
                }

                // Bandeja: clics del menú contextual (mostrar/ocultar, salir).
                // Mismo patrón que el atajo global: canal global drenado aquí.
                if let Some(tray) = &self.tray {
                    for _ in 0..8 {
                        match tray.events.try_recv() {
                            Ok(ev) if ev.id == crate::tray::MENU_TOGGLE_ID => {
                                return self.update(Message::ToggleMainWindow);
                            }
                            Ok(ev) if ev.id == crate::tray::MENU_QUIT_ID => {
                                crate::log!(
                                    "tray",
                                    "«Salir» desde la bandeja → terminando server LLM propio"
                                );
                                crate::llm::kill_launched_server();
                                self.persist_config();
                                self.config_save_deadline = None;
                                return iced::exit();
                            }
                            Ok(_) => continue,
                            Err(_) => break,
                        }
                    }
                }

                self.flush_config();

                // Auto-recuperación del arrastre con botón derecho: si el
                // sistema roba la captura (alt-tab, menú del sistema, doble
                // clic en otra ventana) o el evento de soltado se pierde, se
                // cierra el arrastre limpio para no quedarse «atascado» ni
                // acumular saltos. `GetCapture` es el detector fiable de robo
                // de captura (equivale a WM_CAPTURECHANGED); el botón ya
                // liberado es la red de seguridad del evento perdido.
                if self.bubble_pressed && !crate::bubble::left_button_down() {
                    let _ = self.update(Message::BubbleRelease);
                }
                if self.bubble_right.pressed || self.bubble_right.dragging {
                    let stolen = match &self.bubble_drag {
                        Some(drag) => !drag.is_captured(),
                        None => false,
                    };
                    if stolen {
                        self.bubble_right.cancel();
                        self.end_bubble_drag("captura robada por el sistema (GetCapture)");
                    } else if !crate::bubble::right_button_down() {
                        self.bubble_right.cancel();
                        self.end_bubble_drag("captura perdida (botón liberado)");
                    } else if self.bubble_right.dragging {
                        // Seguimiento redundante (anclaje absoluto, idempotente):
                        // si el bucle de eventos no entrega CursorMoved con el
                        // cursor fuera de la ventana, el poll sigue al cursor
                        // igualmente a 10 Hz.
                        if let Some(drag) = &mut self.bubble_drag {
                            drag.follow();
                        }
                    }
                }

                // Resultado de transcripción/descarga.
                let _tx_drained: Task<Message> = if let Some(rx) = &self.transcribe_rx {
                    match rx.try_recv() {
                        Ok(Ok(msg)) if msg.starts_with("__PROGRESS__") => {
                            if let Some(rest) = msg.strip_prefix("__PROGRESS__") {
                                let mut it = rest.split(':');
                                if let (Some(d), Some(t)) = (it.next(), it.next()) {
                                    if let (Ok(d), Ok(t)) = (d.parse::<u64>(), t.parse::<u64>()) {
                                        // Mismo tracker de velocidad que la
                                        // pantalla de carga (MB + ETA, R4).
                                        self.note_download_progress(d, t);
                                    }
                                }
                            }
                            Task::none()
                        }
                        Ok(Ok(msg)) if msg == "__DOWNLOAD_OK__" => {
                            self.transcribe_rx = None;
                            self.status = Status::Idle;
                            // Onboarding (sprint final): si la descarga terminó
                            // mientras el arranque estaba en Failed (primera
                            // configuración inicial), relanzar el arranque solo
                            // en vez de dejar la pantalla de error colgada.
                            if matches!(self.startup, StartupPhase::Failed { .. }) {
                                return self.update(Message::RetryStartup);
                            }
                            Task::none()
                        }
                        Ok(Err(e)) => {
                            self.transcribe_rx = None;
                            self.status = Status::Error(e);
                            Task::none()
                        }
                        Ok(Ok(text)) => self.update(Message::TranscriptionDone(Ok(text))),
                        Err(mpsc::TryRecvError::Empty) => Task::none(),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            self.transcribe_rx = None;
                            self.pending_inject = false;
                            self.status = Status::Error(
                                "El hilo de transcripción terminó inesperadamente".to_string(),
                            );
                            self.arm_bubble_hide_timeout();
                            Task::none()
                        }
                    }
                } else {
                    Task::none()
                };

                // Resultado de la corrección IA (mismo patrón que la
                // transcripción: canal mpsc drenado sin bloquear el bucle).
                if let Some(rx) = &self.llm_rx {
                    return match rx.try_recv() {
                        Ok(result) => self.update(Message::CorrectionDone(result)),
                        Err(mpsc::TryRecvError::Empty) => Task::none(),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            self.llm_rx = None;
                            let pending = self.llm_pending.take();
                            self.last_correction = Some(
                                "Corrección IA no disponible: el hilo terminó inesperadamente"
                                    .to_string(),
                            );
                            self.restore_pending_as_dictation(pending);
                            self.status = Status::Idle;
                            self.arm_bubble_hide_timeout();
                            self.save_config();
                            Task::none()
                        }
                    };
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

}