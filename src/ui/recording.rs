//! Manejadores de grabación, transcripción y ajustes del dictado.
//!
//! Extraído de `WhisprApp::update` en el refactor de organización (cero
//! cambio de conducta): los cuerpos de los `match` se movieron tal cual.

use std::sync::mpsc;

use iced::Task;

use crate::app::{
    download_failure_message, LlmMode, LlmPending, LlmResult, LlmStatus, Message, StartupPhase,
    Status, WhisprApp,
};
use crate::audio::AudioCapture;

impl WhisprApp {
    /// Control del turno de grabación: atajo/atajo-global (`Toggle`),
    /// arranque, parada y micrófono perdido a mitad de turno.
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
                // carga) y si el arranque falló sin modelo transcribible.
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
                // Una grabación nueva descarta cualquier inyección pendiente.
                self.pending_inject = false;
                // Re-mostrar la burbuja si el auto-ocultar la escondió (dictar
                // es la actividad principal: la burbuja debe estar visible).
                let show = self.show_bubble();
                let device = self.config.input_device.clone();
                match AudioCapture::start(device.as_deref()) {
                    Ok(mut capture) => {
                        // H6: si el micrófono configurado ya no estaba, se
                        // dictó igual con el dispositivo por defecto. El
                        // aviso va a la línea de feedback: el usuario ve qué
                        // micro se está usando de verdad.
                        if let Some(notice) = capture.take_fallback_notice() {
                            self.last_correction = Some(notice);
                        }
                        self.audio = Some(capture);
                        self.status = Status::Recording;
                        // Marcador de inicio para el tope de duración del
                        // turno (el Poll lo vigila).
                        self.recording_started = Some(std::time::Instant::now());
                        self.device_lost = false;
                        // La sesión y `last_turn` sobreviven entre turnos:
                        // las correcciones multi-turno y la corrección IA
                        // necesitan ver el dictado anterior.
                        show
                    }
                    Err(e) => {
                        // Sin micro (o ocupado): error visible y estado
                        // consistente — el siguiente intento funciona sin
                        // reiniciar (requisito de robustez).
                        self.recording_started = None;
                        self.status = Status::Error(e);
                        Task::none()
                    }
                }
            }
            Message::StopRecording => {
                // Parada voluntaria (atajo, botón, burbuja o tope de
                // duración): el turno termina como dictado normal.
                self.device_lost = false;
                self.finish_recording()
            }
            Message::AudioDeviceLost(reason) => {
                // El stream murió a mitad (micro desconectado): se corta el
                // turno YA con el audio acumulado hasta ese momento — NUNCA
                // se pierde lo ya dicho — y la transcripción lleva un aviso.
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

    /// Llegó (o falló) la transcripción de un turno: aplica el pipeline de
    /// limpieza, decide inyectar/guardar y avisa del resultado.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_transcription_done(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::TranscriptionDone(result) => {
                self.transcribe_rx = None;
                // La burbuja marca `pending_inject` en el soltado: ese turno
                // se escribe en la app activa aunque autoinyectar esté off.
                let force_inject = self.pending_inject;
                self.pending_inject = false;
                match result {
                    Ok(raw) => {
                        let (processed, rule_corrected) = self.apply_pipeline(&raw);
                        if processed.is_empty() && raw.trim().is_empty() {
                            self.status = Status::Idle;
                            // Turno vacío (hubo audio pero no se transcribió
                            // nada): aviso útil en vez de silencio confuso.
                            // Si además el micro murió a mitad, el aviso lo
                            // dice (requisito de robustez).
                            self.last_correction = Some(if self.device_lost {
                                "El micrófono se desconectó y no se llegó a oír nada; revisa el micro y vuelve a dictar.".to_string()
                            } else {
                                "No se oyó nada: comprueba el micrófono, acércate y repite el dictado.".to_string()
                            });
                            self.device_lost = false;
                            // Turno vacío: igualmente «terminado» → armar el
                            // auto-ocultar si está configurado.
                            self.arm_bubble_hide_timeout();
                            return Task::none();
                        }
                        // Aviso de micro perdido a mitad (si procede): se
                        // pone ya, antes del enrutado — si hay corrección IA,
                        // su propio feedback puede reemplazarlo después.
                        if self.device_lost {
                            self.last_correction = Some(
                                "El micrófono se desconectó a mitad: se transcribió lo que se oyó hasta ese momento.".to_string(),
                            );
                            self.device_lost = false;
                        }
                        // Nota pendiente del turno (p. ej. tope de duración
                        // maratoniano): tiene prioridad sobre el aviso de
                        // micro porque explica el corte.
                        if let Some(note) = self.pending_turn_note.take() {
                            self.last_correction = Some(note);
                        }

                        // Corrección IA (spec §3 + §3.5, t_78d9e1fa): el sistema decide en
                        // cascada qué modo aplicar al turno recién
                        // transcrito. De más prioritario a menos:
                        //
                        //   1) IN-TURN (modo principal desde 10/09/2026):
                        //      el turno contiene auto-corrección hablada
                        //      («digo X», «quiero decir», «no X sino Y»,
                        //      «mejor X», «en vez de»...) → el LLM
                        //      recibe el turno COMPLETO y devuelve el
                        //      texto final corregido (sin meta-lengua).
                        //      Caso canónico:
                        //        «que calor en murcia digo cartagena»
                        //          → «que calor en cartagena»
                        //
                        //   2) CROSS-TURN (modo histórico, fallback):
                        //      la frase apunta al turno ANTERIOR («no no
                        //      lo último está mal, es 2 en vez de 4») →
                        //      el LLM recibe (target, instruction) y
                        //      devuelve {find, replace} sobre `target`.
                        //
                        //   3) PLAIN: no se detecta intención correctiva
                        //      → el turno se inyecta como está.
                        //
                        // Ambos modos comparten los mismos gates de
                        // «AI activada», «server LLM listo» y «no hay
                        // otra corrección volando». El modo in-turn NO
                        // exige que haya un turno previo (el input es
                        // el turno actual entero); el cross-turn SÍ.
                        //
                        // LOG DE DECISIÓN (bug Juanfri 09/09/2026): el
                        // síntoma «no llama al LLM» antes era invisible
                        // porque el `eprintln!` se perdía en Windows GUI.
                        // Ahora el resultado del clasificador, los gates
                        // que cortan y la decisión de enrutar van al log
                        // del archivo `%APPDATA%\whispr-open\whispr-open.log`
                        // (visible siempre, incluso sin consola).
                        let in_turn = crate::domain::corrective::classify_in_turn(&processed);
                        let decision = crate::domain::corrective::classify_with(
                            &processed,
                            self.config.llm_classifier_threshold(),
                        );
                        let classifier_threshold = self.config.llm_classifier_threshold();
                        let enabled = self.config.ai_correction_enabled();
                        let llm_ready = matches!(self.llm_status, LlmStatus::Ready { .. });
                        let llm_busy = self.llm_rx.is_some() || self.llm_pending.is_some();
                        let can_route_common = !rule_corrected && enabled && !llm_busy && llm_ready;
                        let can_route_in_turn = can_route_common;
                        let can_route_cross_turn = can_route_common && !self.last_turn.is_empty();

                        if can_route_in_turn && in_turn {
                            // ── Modo in-turn (t_78d9e1fa) ───────────
                            // El LLM recibe `processed` entero y
                            // devuelve el texto final limpio. El
                            // resultado REEMPLAZA el turno que
                            // íbamos a inyectar (no se aplica diff).
                            let target = processed.clone();
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
                                "enrutando al corrector (in-turn): input={:?} endpoint={} modelo={} timeout={}s",
                                target,
                                llm_cfg.base_url,
                                llm_cfg.model,
                                llm_cfg.timeout_secs,
                            );
                            std::thread::spawn(move || {
                                let result = crate::llm::redact(&target, &llm_cfg);
                                let wrapped = match &result {
                                    Ok(s) => Ok(LlmResult::Redacted(s.clone())),
                                    Err(_) => Err(
                                        result.as_ref().err().cloned().unwrap_or_default(),
                                    ),
                                };
                                if let Err(ref e) = result {
                                    crate::log!(
                                        "llm",
                                        "fallo del corrector in-turn (recuperado como dictado normal): {e}"
                                    );
                                }
                                let _ = tx.send(wrapped);
                            });
                            return Task::none();
                        }

                        if can_route_cross_turn
                            && decision == crate::domain::corrective::CorrectiveDecision::NeedsLlm
                        {
                            // ── Modo cross-turn (flujo histórico) ─────
                            // El LLM recibe (last_turn, instruction)
                            // y devuelve {find, replace}. Se aplica
                            // sobre `target` (= last_turn).
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
                        // Ruta no tomada: registrar POR QUÉ para que
                        // «no llama al LLM» deje de ser un misterio cuando
                        // el usuario mira el log. Distinguimos
                        // in_turn / cross_turn por separado para que el
                        // diagnóstico de Juanfri sea inmediato cuando un
                        // modo detecta y el otro no (caso real: el turno
                        // tiene «digo X» que SÍ es in-turn pero el
                        // cross-turn lo descarta por umbral alto).
                        crate::log!(
                            "llm",
                            "no se enruta: in_turn={} classifier={:?} threshold={} enabled={} llm_ready={} rule_corrected={} llm_busy={} last_turn_empty={} processed={:?}",
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
                            // La nota es editable (R3): todo cambio del
                            // pipeline se refleja en el editor al instante.
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
                // Turno terminado (o error): armar el auto-ocultar. El camino
                // del LLM sale antes (return) y arma al resolver la corrección.
                self.arm_bubble_hide_timeout();
                self.save_config();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Ajustes del dictado que cambian el pipeline (modelo whisper,
    /// limpieza de muletillas, auto-inyección).
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
                            // Error de descarga con causa accionable, no la
                            // cadena cruda de ureq (requisito de robustez).
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