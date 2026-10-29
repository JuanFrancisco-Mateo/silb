//! Qué pasa al grabar, al transcribir y al cambiar ajustes del dictado.
//!
//! Salió de `WhisprApp::update` al repartir el fichero (sin cambiar nada de
//! comportamiento): los cuerpos de los `match` se movieron tal cual.

use std::sync::mpsc;

use iced::Task;

use crate::app::{
    download_failure_message, LlmMode, LlmPending, LlmResult, LlmStatus, Message, StartupPhase,
    Status, WhisprApp,
};
use crate::audio::AudioCapture;

impl WhisprApp {
    /// El turno de grabación: atajo o atajo global (`Toggle`), empezar, parar
    /// y micro perdido a mitad.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_recording_control(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Toggle => {
                if self.audio.is_some() {
                    return self.update(Message::StopRecording);
                }
                self.update(Message::StartRecording)
            }
            Message::StartRecording => {
                // Dictado bloqueado hasta que whisper esté listo (pantalla de
                // carga) y si el arranque falló sin un modelo que transcriba.
                if self.audio.is_some()
                    || self.transcribe_rx.is_some()
                    || self.llm_rx.is_some()
                    || self.llm_pending.is_some()
                    || self.pending_diff.is_some()
                    || matches!(self.status, Status::Downloading { .. })
                    || !matches!(self.startup, StartupPhase::Ready)
                {
                    return Task::none();
                }
                // Una grabación nueva descarta cualquier escritura pendiente.
                self.pending_inject = false;
                // Vuelve a enseñar la burbuja si el esconderla sola la ocultó
                // (dictar es lo principal: la burbuja tiene que verse).
                let show = self.show_bubble();
                let device = self.config.input_device.clone();
                match AudioCapture::start(device.as_deref()) {
                    Ok(mut capture) => {
                        // H6: si el micrófono configurado ya no estaba, se
                        // dictó igual con el de por defecto. El aviso va a la línea
                        // de avisos: el usuario ve qué micro se usa de verdad.
                        if let Some(notice) = capture.take_fallback_notice() {
                            self.last_correction = Some(notice);
                        }
                        self.audio = Some(capture);
                        self.status = Status::Recording;
                        // Marca de inicio para el tope de duración del turno (lo
                        // vigila el `Poll`).
                        self.recording_started = Some(std::time::Instant::now());
                        self.device_lost = false;
                        // La sesión y `last_turn` siguen entre turnos: las
                        // correcciones de varios turnos y la corrección IA necesitan
                        // ver lo dictado antes.
                        show
                    }
                    Err(e) => {
                        // Sin micro (u ocupado): error visible y estado coherente
                        // — el siguiente intento funciona sin reiniciar (nada se
                        // queda roto).
                        self.recording_started = None;
                        self.status = Status::Error(e);
                        Task::none()
                    }
                }
            }
            Message::StopRecording => {
                // Parada voluntaria (atajo, botón, burbuja o tope de duración):
                // el turno acaba como un dictado normal.
                self.device_lost = false;
                self.finish_recording()
            }
            Message::AudioDeviceLost(reason) => {
                // La entrada murió a mitad (micro desconectado): se corta el
                // turno YA con el audio guardado hasta ese momento — NUNCA se
                // pierde lo ya dicho — y la transcripción lleva un aviso.
                if self.audio.is_none() {
                    return Task::none();
                }
                crate::log!("audio", "turno cortado por fallo del stream: {reason}");
                self.device_lost = true;
                self.finish_recording()
            }
            _ => Task::none(),
        }
    }

    /// Llegó (o falló) la transcripción de un turno: pasa la limpieza, decide
    /// si escribir o guardar, y avisa del resultado.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_transcription_done(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::TranscriptionDone(result) => {
                self.transcribe_rx = None;
                // La burbuja marca `pending_inject` al soltar: ese turno se
                // escribe en la app activa aunque la escritura automática esté off.
                let force_inject = self.pending_inject;
                self.pending_inject = false;
                match result {
                    Ok(raw) => {
                        let (processed, rule_corrected) = self.apply_pipeline(&raw);
                        if processed.is_empty() && raw.trim().is_empty() {
                            self.status = Status::Idle;
                            // Turno vacío (hubo audio pero no se sacó texto):
                            // aviso útil en vez de un silencio que despista. Si
                            // además murió el micro a mitad, el aviso lo dice.
                            self.last_correction = Some(if self.device_lost {
                                "El micrófono se desconectó y no se llegó a oír nada; revisa el micro y vuelve a dictar.".to_string()
                            } else {
                                "No se oyó nada: comprueba el micrófono, acércate y repite el dictado.".to_string()
                            });
                            self.device_lost = false;
                            // Turno vacío: cuenta igual como «terminado» → armar
                            // el esconder la burbuja si está configurado.
                            self.arm_bubble_hide_timeout();
                            return Task::none();
                        }
                        // Aviso de micro perdido a mitad (si toca): se pone ya,
                        // antes de repartir — si hay corrección IA, su propio aviso
                        // puede sustituirlo después.
                        if self.device_lost {
                            self.last_correction = Some(
                                "El micrófono se desconectó a mitad: se transcribió lo que se oyó hasta ese momento.".to_string(),
                            );
                            self.device_lost = false;
                        }
                        // Nota pendiente del turno (por ejemplo el tope de
                        // duración): manda sobre el aviso del micro porque explica
                        // el corte.
                        if let Some(note) = self.pending_turn_note.take() {
                            self.last_correction = Some(note);
                        }

                        // Corrección IA (documento, puntos 3 y 3.5): el sistema
                        // decide en cascada qué hacer con el turno recién
                        // transcrito. De más a menos importante:
                        //
                        //   1) DENTRO DEL TURNO (lo normal desde el 10/09/2026):
                        //      el turno trae una corrección hablada
                        //      («digo X», «quiero decir», «no X sino Y»,
                        //      «mejor X», «en vez de»...) → el modelo
                        //      recibe el turno ENTERO y devuelve el
                        //      texto final ya corregido (sin la muletilla).
                        //      Caso típico:
                        //        «que calor en murcia digo cartagena»
                        //          → «que calor en cartagena»
                        //
                        //   2) ENTRE TURNOS (lo de antes, si no hay otra):
                        //      la frase apunta al turno ANTERIOR («no no lo
                        //      último está mal, es 2 en vez de 4») → el
                        //      modelo recibe (target, instruction) y
                        //      devuelve {find, replace} sobre `target`.
                        //
                        //   3) NORMAL: no se ve intención de corregir
                        //      → el turno se escribe tal cual.
                        //
                        // Los dos modos comparten los mismos filtros: «IA
                        // encendida», «servidor LLM listo» y «no hay otra
                        // corrección en marcha». El de dentro del turno NO
                        // pide un turno previo (el texto es el turno entero);
                        // el de entre turnos SÍ.
                        //
                        // LOG DE LA DECISIÓN (fallo de Juanfri 09/09/2026): el
                        // «no llama al modelo» antes no se veía porque el
                        // `eprintln!` se perdía en Windows sin consola. Ahora el
                        // resultado del clasificador, los filtros que cortan y la
                        // decisión de mandar van al log del archivo
                        // `%APPDATA%\whispr-open\whispr-open.log` (se ve siempre).
                        let in_turn = crate::domain::corrective::classify_in_turn(&processed);
                        let decision = crate::domain::corrective::classify_with(
                            &processed,
                            self.config.llm_classifier_threshold(),
                        );
                        let classifier_threshold = self.config.llm_classifier_threshold();
                        let enabled = self.config.ai_correction_enabled();
                        let llm_ready = matches!(self.llm_status, LlmStatus::Ready { .. });
                        let llm_busy = self.llm_rx.is_some() || self.llm_pending.is_some();
                        // (0.7.1) El modo manda: en Reescritura se llama al modelo
                        // en CADA turno (no hace falta marca de autocorrección) y
                        // con otro prompt; en Corrección, solo cuando el
                        // clasificador ve una, como siempre.
                        let modo = self.config.ai_mode();
                        let reescribir = modo == crate::config::AiMode::Reescritura;
                        let can_route_common = enabled && !llm_busy && llm_ready;
                        let can_route_in_turn = can_route_common && !rule_corrected;
                        let can_route_cross_turn = can_route_in_turn && !self.last_turn.is_empty();

                        // En Reescritura no hace falta ni marca ni que las reglas
                        // no hayan tocado nada: se reescribe el turno entero.
                        let route_in_turn = if reescribir {
                            can_route_common
                        } else {
                            can_route_in_turn && in_turn
                        };

                        if route_in_turn {
                            // ── Dentro del turno (t_78d9e1fa) ───────
                            // El modelo recibe `processed` entero y devuelve el
                            // texto final limpio. El resultado SUSTITUYE al turno
                            // que se iba a escribir (no se aplica un cambio).
                            //
                            // (0.7.1) El guard numérico determinista va DELANTE del
                            // modelo: medido con Qwen3-4B, «83 digo 84» hace que el
                            // modelo invente «83 o el 84», mientras que si el guard
                            // ya dejó «84» la reescritura sale limpia y más rápida.
                            // Si reconstruye, el turno que viaja (y al que se vuelve
                            // en los fallbacks) es el reconstruido, no el bruto.
                            let target = match crate::llm::precondition_numeric(&processed) {
                                Some(reconstruido) => {
                                    crate::log!(
                                        "llm",
                                        "guard numérico ANTES del modelo (modo={}): {:?} -> {:?}",
                                        modo.id(),
                                        processed,
                                        reconstruido
                                    );
                                    reconstruido
                                }
                                None => processed.clone(),
                            };
                            let llm_cfg = self.config.llm_settings();
                            let (tx, rx) = mpsc::channel();
                            self.llm_rx = Some(rx);
                            self.llm_pending = Some(LlmPending {
                                mode: LlmMode::InTurn,
                                instruction: target.clone(),
                                target: target.clone(),
                                force_inject,
                            });
                            self.status = Status::Correcting;
                            crate::log!(
                                "llm",
                                "enrutando al corrector (modo={}): input={:?} endpoint={} modelo={} timeout={}s",
                                modo.id(),
                                target,
                                llm_cfg.base_url,
                                llm_cfg.model,
                                llm_cfg.timeout_secs,
                            );
                            std::thread::spawn(move || {
                                let result = if reescribir {
                                    crate::llm::rewrite(&target, &llm_cfg)
                                } else {
                                    crate::llm::redact(&target, &llm_cfg)
                                };
                                let wrapped = match &result {
                                    Ok(s) => Ok(LlmResult::Redacted(s.clone())),
                                    Err(_) => Err(
                                        result.as_ref().err().cloned().unwrap_or_default(),
                                    ),
                                };
                                if let Err(ref e) = result {
                                    crate::log!(
                                        "llm",
                                        "fallo del corrector (modo={}): {e}. Se escribe el texto tal cual.",
                                        if reescribir { "reescritura" } else { "correccion" }
                                    );
                                }
                                let _ = tx.send(wrapped);
                            });
                            return Task::none();
                        }

                        if can_route_cross_turn
                            && decision == crate::domain::corrective::CorrectiveDecision::NeedsLlm
                        {
                            // ── Entre turnos (el camino de antes) ──────
                            // El modelo recibe (last_turn, instruction) y
                            // devuelve {find, replace}. Se aplica sobre `target`
                            // (= last_turn).
                            let target = self.last_turn.clone();
                            let instruction = processed.clone();
                            let llm_cfg = self.config.llm_settings();
                            let (tx, rx) = mpsc::channel();
                            self.llm_rx = Some(rx);
                            self.llm_pending = Some(LlmPending {
                                mode: LlmMode::CrossTurn,
                                instruction: instruction.clone(),
                                target: target.clone(),
                                force_inject,
                            });
                            self.status = Status::Correcting;
                            crate::log!(
                                "llm",
                                "enrutando al corrector (cross-turn): instruction={:?} target={:?} endpoint={} modelo={} timeout={}s threshold={}",
                                instruction,
                                target,
                                llm_cfg.base_url,
                                llm_cfg.model,
                                llm_cfg.timeout_secs,
                                classifier_threshold
                            );
                            std::thread::spawn(move || {
                                let result = crate::llm::correct(&target, &instruction, &llm_cfg);
                                let wrapped = match &result {
                                    Ok(diff) => Ok(LlmResult::Diff(diff.clone())),
                                    Err(_) => Err(
                                        result.as_ref().err().cloned().unwrap_or_default(),
                                    ),
                                };
                                if let Err(ref e) = result {
                                    crate::log!(
                                        "llm",
                                        "fallo del corrector cross-turn (recuperado como dictado normal): {e}"
                                    );
                                }
                                let _ = tx.send(wrapped);
                            });
                            return Task::none();
                        }
                        // Camino no tomado: apuntar POR QUÉ para que «no llama
                        // al modelo» deje de ser un misterio al mirar el log. Se
                        // distinguen los dos modos para que el diagnóstico sea
                        // inmediato cuando uno detecta y el otro no (caso real: el
                        // turno lleva «digo X» que SÍ es dentro del turno pero el
                        // de entre turnos lo descarta por umbral alto).
                        crate::log!(
                            "llm",
                            "no se enruta: modo={} in_turn={} classifier={:?} threshold={} enabled={} llm_ready={} rule_corrected={} llm_busy={} last_turn_empty={} processed={:?}",
                            modo.id(),
                            in_turn,
                            decision,
                            classifier_threshold,
                            enabled,
                            llm_ready,
                            rule_corrected,
                            llm_busy,
                            self.last_turn.is_empty(),
                            processed
                        );

                        if !processed.is_empty() {
                            if rule_corrected && !self.last_turn.is_empty() {
                                let target = self.last_turn.clone();
                                if let Some(updated) = crate::domain::corrective::replace_last_turn(
                                    &self.transcript,
                                    &target,
                                    &processed,
                                ) {
                                    self.transcript = updated;
                                }
                            } else {
                                self.transcript = crate::domain::correction::append_transcript(
                                    &self.transcript,
                                    &processed,
                                );
                            }
                            // La nota se puede editar (R3): cada cambio de la
                            // limpieza se ve en el editor al instante.
                            self.sync_editor_from_transcript();
                            self.last_turn = processed.clone();
                        }
                        self.status = Status::Idle;

                        if (force_inject || self.config.auto_inject.unwrap_or(true))
                            && !self.last_turn.is_empty()
                        {
                            let turn = self.last_turn.clone();
                            self.inject_into_active(&turn);
                        }
                    }
                    Err(e) => self.status = Status::Error(e),
                }
                // Turno terminado (o error): armar el esconder la burbuja. El
                // camino del modelo sale antes (return) y lo arma al resolver.
                self.arm_bubble_hide_timeout();
                self.save_config();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Ajustes del dictado que cambian lo que se hace (modelo de whisper,
    /// limpieza de muletillas, escritura automática).
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_dictation_settings(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::ModelSelected(model) => {
                if self.audio.is_some()
                    || self.transcribe_rx.is_some()
                    || self.llm_rx.is_some()
                    || self.llm_pending.is_some()
                    || self.pending_diff.is_some()
                    || matches!(self.status, Status::Downloading { .. })
                {
                    return Task::none();
                }
                if model != self.config.selected_model() {
                    self.config.model = Some(model.id().to_string());
                    self.status = Status::Idle;
                    self.save_config();
                    // Aviso de descarga la primera vez que se use el modelo.
                    if !crate::transcribe::model_exists(model) {
                        self.status = Status::Downloading {
                            model,
                            downloaded: 0,
                            total: 0,
                        };
                        let (tx, rx) = mpsc::channel();
                        self.transcribe_rx = Some(rx);
                        std::thread::spawn(move || {
                            let result = crate::transcribe::ensure_model(model, |d, t| {
                                let _ = tx.send(Ok(format!("__PROGRESS__{d}:{t}")));
                            })
                            .map(|_| "__DOWNLOAD_OK__".to_string())
                            // Error de descarga con una causa útil, no el texto
                            // crudo de ureq (nada de tecnicismos al usuario).
                            .map_err(|e| download_failure_message(model, &e));
                            let _ = tx.send(result);
                        });
                    }
                }
                Task::none()
            }
            Message::ToggleCleanup(on) => {
                self.config.cleanup_enabled = Some(on);
                self.save_config();
                Task::none()
            }
            Message::ToggleAutoInject(on) => {
                self.config.auto_inject = Some(on);
                self.save_config();
                Task::none()
            }
            _ => Task::none(),
        }
    }

}