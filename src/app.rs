//! UI de Whispr Open con Iced: dictado por turnos, transcript acumulado,
//! limpieza de muletillas, diccionario personal, selector de modelo e
//! inyección en la app activa.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use iced::widget::{
    button, checkbox, column, container, horizontal_rule, pick_list, progress_bar, row,
    scrollable, text, text_input, Column, Space,
};
use iced::{window, Background, Color, Element, Font, Length, Point, Subscription, Task, Theme};
use global_hotkey::GlobalHotKeyEvent;

use crate::audio::AudioCapture;
use crate::config::Config;
use crate::domain::cleanup::clean_transcript;
use crate::domain::correction::CorrectionSession;
use crate::domain::dictionary::{apply_dictionary, DictionaryEntry};
use crate::hotkey::HotkeyManager;
use crate::theme::{self, UiPalette};
use crate::transcribe::{Model, WhisperEngine};
use crate::inject::inject_text_into_active_app;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Recording,
    Transcribing,
    /// El LLM corrector está trabajando sobre el último turno dictado
    /// (docs/CORRECCION-VOZ.md §4.4: estado ámbar como Transcribing).
    Correcting,
    Downloading { model: Model, downloaded: u64, total: u64 },
    Error(String),
}

/// Lectura visual del estado del micrófono (feedback doble: color + texto).
struct MicUi {
    color: Color,
    hint_color: Color,
    label: String,
    hint: String,
}

/// La instrucción hablada que se está enviando al LLM corrector. Si la
/// llamada falla o el diff no aplica, se reinserta como dictado normal
/// (spec §3.3.1: nunca perder texto). `target` es el turno sobre el que se
/// pidió la corrección (capturado al despachar: un turno nuevo dictado
/// mientras el LLM trabaja no debe desviar el diff a otro texto).
struct LlmPending {
    instruction: String,
    /// Último turno dictado que la instrucción pretende corregir.
    target: String,
    /// El turno venía de la burbuja (inyección forzada al soltar).
    force_inject: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    StartRecording,
    StopRecording,
    TranscriptionDone(Result<String, String>),
    ModelSelected(Model),
    ToggleCleanup(bool),
    ToggleAutoInject(bool),
    /// Corrección IA (otro modelo) activada/desactivada — Ajustes.
    ToggleAiCorrection(bool),
    /// Resultado del LLM corrector: diff a aplicar sobre el último turno.
    CorrectionDone(Result<crate::domain::corrective::Diff, String>),
    /// Deshacer la última corrección IA aplicada (un nivel).
    UndoLastCorrection,
    ClearTranscript,
    CopyTranscript,
    InjectLast,
    AddEntry,
    EntryFromChanged(String),
    EntryToChanged(String),
    RemoveEntry(usize),
    InputDeviceSelected(String),
    ToggleTheme,
    ToggleSettings,
    Poll,
    // ── Burbuja flotante (overlay) ────────────────────────────────────────
    /// La ventana de la burbuja ya fue creada (aplicar WS_EX_NOACTIVATE).
    BubbleOpened(window::Id),
    /// Botón izquierdo pulsado dentro de la burbuja (empieza a grabar).
    BubblePress,
    /// Botón izquierdo soltado (transcribir + escribir en la app activa).
    BubbleRelease,
    /// Botón derecho pulsado dentro de la burbuja (arma el arrastre).
    BubbleRightPress,
    /// Botón derecho soltado (fin del arrastre; clic simple = no-op).
    BubbleRightRelease,
    /// Cursor movido dentro de la burbuja (detección de arrastre derecho).
    BubbleCursorMoved(Point),
    /// Cursor salió de la burbuja sin soltar (cerrar botones pendientes).
    BubbleCursorLeft,
    /// El usuario pidió cerrar una ventana (X de la principal → a la bandeja).
    WindowCloseRequested(window::Id),
    // ── Bandeja del sistema (tray) ────────────────────────────────────────
    /// Se abrió una ventana; con su id se identifica la principal.
    WindowOpened(window::Id),
    /// «Mostrar/Ocultar ventana» pulsado en el menú de la bandeja.
    ToggleMainWindow,
    /// La fuente emoji terminó de cargarse en el FontSystem de Iced
    /// (resultado de `iced::font::load` al arrancar; `Error` es ininhabitable
    /// en iced 0.13, pero se loguea por si en el futuro se propaga algo).
    FontLoaded(Result<(), iced::font::Error>),
}

pub struct WhisprApp {
    config: Config,
    status: Status,
    transcript: String,
    last_turn: String,
    removed_fillers: Vec<String>,
    last_correction: Option<String>,
    session: CorrectionSession,
    audio: Option<AudioCapture>,
    input_devices: Vec<String>,
    transcribe_rx: Option<Receiver<Result<String, String>>>,
    hotkey_rx: crossbeam_channel::Receiver<GlobalHotKeyEvent>,
    _hotkey: HotkeyManager,
    entry_from: String,
    entry_to: String,
    settings_open: bool,
    // ── Estado de la burbuja flotante ─────────────────────────────────────
    bubble_id: Option<window::Id>,
    bubble_pressed: bool, // botón izquierdo pulsado (graba)
    bubble_last_pos: Point,
    /// Botón derecho: solo arrastra la burbuja (nunca graba ni abre nada).
    bubble_right: crate::bubble::RightDragState,
    /// Captura del ratón + seguimiento de la posición durante el arrastre.
    bubble_drag: Option<crate::bubble::BubbleDrag>,
    pending_inject: bool, // la próxima transcripción debe inyectarse sí o sí
    // ── Corrección IA (LLM) ────────────────────────────────────────────────
    llm_rx: Option<Receiver<Result<crate::domain::corrective::Diff, String>>>,
    /// Instrucción en ruta al LLM (con el target capturado): si la llamada
    /// falla o el diff no aplica, se reinserta como dictado normal.
    llm_pending: Option<LlmPending>,
    /// Undo de 1 nivel: (transcript, last_turn) antes de la última corrección.
    undo_state: Option<(String, String)>,
    // ── Bandeja del sistema ───────────────────────────────────────────────
    tray: Option<crate::tray::SystemTray>,
    /// Id de la ventana principal (se descubre con `window::open_events()`:
    /// iced no lo expone al crear la ventana desde `program::run`).
    main_window_id: Option<window::Id>,
    /// Si la ventana principal está visible (controla el texto del menú).
    main_visible: bool,
}

impl WhisprApp {
    pub fn new() -> (Self, Task<Message>) {
        let config = Config::load();
        let (_hotkey, hotkey_rx) = match HotkeyManager::register() {
            Ok(m) => {
                let rx = m.listen();
                (m, rx)
            }
            Err(e) => {
                crate::log!("hotkey", "{e}");
                let stub = HotkeyManager::stub();
                let rx = stub.listen();
                (stub, rx)
            }
        };

        // Segunda ventana: la burbuja flotante. Se abre al arrancar; cuando la
        // ventana exista llegará `Message::BubbleOpened` y se le aplicará
        // WS_EX_NOACTIVATE (no robar el foco de la app donde se dicta).
        let (bubble_id, bubble_task) = {
            let (id, task) = window::open(crate::bubble::bubble_settings());
            (Some(id), task.map(Message::BubbleOpened))
        };

        // Bandeja del sistema. Se crea aquí (hilo principal, el mismo donde
        // corre el bucle de winit): en Windows, tray-icon crea su ventana
        // oculta en el hilo llamante y los eventos llegan por canal global.
        // Si falla, la app sigue funcionando sin bandeja (se avisa en el log).
        let tray = match crate::tray::SystemTray::new() {
            Ok(tray) => Some(tray),
            Err(e) => {
                crate::log!("tray", "{e}");
                None
            }
        };

        // Fuente emoji (Segoe UI Emoji en Windows): que los símbolos de la UI
        // (✓ ☀ ☾ ▾ ▸ ● → ✕) no aparezcan como cajas vacías. La carga añade
        // los bytes al FontSystem de Iced; los textos se re-shapean solos al
        // cambiar la versión del FontSystem (ver src/fonts.rs).
        let font_task = match crate::fonts::load_emoji_font() {
            Some(task) => task.map(Message::FontLoaded),
            None => Task::none(),
        };

        let app = Self {
            config,
            status: Status::Idle,
            transcript: String::new(),
            last_turn: String::new(),
            removed_fillers: vec![],
            last_correction: None,
            session: CorrectionSession::new(),
            audio: None,
            input_devices: crate::audio::input_devices(),
            transcribe_rx: None,
            hotkey_rx,
            _hotkey,
            entry_from: String::new(),
            entry_to: String::new(),
            settings_open: false,
            bubble_id,
            bubble_pressed: false,
            bubble_last_pos: Point::new(0.0, 0.0),
            bubble_right: crate::bubble::RightDragState::default(),
            bubble_drag: None,
            pending_inject: false,
            llm_rx: None,
            llm_pending: None,
            undo_state: None,
            tray,
            main_window_id: None,
            main_visible: true,
        };
        (app, Task::batch([bubble_task, font_task]))
    }

    fn save_config(&self) {
        let mut cfg = self.config.clone();
        cfg.dictionary = self.dictionary_snapshot();
        cfg.model = Some(self.config.selected_model().id().to_string());
        cfg.save();
    }

    /// Aplica el diff del LLM al último turno (o recupera la instrucción si
    /// la llamada falló: dictado normal, nunca perder texto del usuario).
    fn handle_llm_correction(
        &mut self,
        result: Result<crate::domain::corrective::Diff, String>,
    ) -> Task<Message> {
        self.llm_rx = None;
        let pending = self.llm_pending.take();

        match result {
            Ok(diff) => {
                // El target se capturó al despachar; un turno nuevo dictado
                // mientras el LLM trabajaba no debe desviar el diff.
                let Some(p) = pending.as_ref() else {
                    self.status = Status::Idle;
                    return Task::none();
                };
                let target = p.target.clone();
                match crate::domain::corrective::apply_diff(&target, &diff) {
                    Some(after) => {
                        // Undo de 1 nivel ANTES de mutar (estado completo).
                        self.undo_state = Some((self.transcript.clone(), self.last_turn.clone()));
                        if let Some(t) = crate::domain::corrective::replace_last_turn(
                            &self.transcript,
                            &target,
                            &after,
                        ) {
                            self.transcript = t;
                        }
                        // Solo si no intervino un turno nuevo: last_turn sigue
                        // siendo el turno corregido.
                        if self.last_turn == target {
                            self.last_turn = after.clone();
                        }
                        self.last_correction = Some(format!(
                            "Rectificado IA: “{}” → “{}”",
                            diff.find, diff.replace
                        ));
                        crate::log!("llm", "rectificado: {} → {}", diff.find, diff.replace);

                        let force_inject = p.force_inject;
                        if (self.config.auto_inject.unwrap_or(true) || force_inject)
                            && !after.is_empty()
                        {
                            if let Err(e) = inject_text_into_active_app(&after) {
                                self.status = Status::Error(format!("Inyección: {e}"));
                            }
                        }
                    }
                    None => {
                        self.last_correction =
                            Some("De momento no sé qué corregir en el último turno".to_string());
                        self.restore_instruction_as_dictation(pending);
                    }
                }
            }
            Err(e) => {
                self.last_correction = Some(format!("Corrección IA no disponible: {e}"));
                self.restore_instruction_as_dictation(pending);
            }
        }
        self.status = Status::Idle;
        self.save_config();
        Task::none()
    }

    /// Spec §3.3.1: si el LLM no llega a corregir nada (caído, timeout o
    /// diff inválido), la instrucción se trata como dictado normal. El texto
    /// nunca se pierde; si un turno nuevo intervino mientras corregía, se
    /// conserva igualmente sin pisar el turno actual.
    fn restore_instruction_as_dictation(&mut self, pending: Option<LlmPending>) {
        let Some(p) = pending else { return };
        if p.instruction.is_empty() {
            return;
        }
        let should_update_turn = self.last_turn == p.target;
        self.transcript =
            crate::domain::correction::append_transcript(&self.transcript, &p.instruction);
        if should_update_turn {
            self.last_turn = p.instruction.clone();
        } else {
            crate::log!("llm", "instrucción recuperada tras un turno intermedio");
        }
        if (self.config.auto_inject.unwrap_or(true) || p.force_inject) && !self.last_turn.is_empty()
        {
            let _ = inject_text_into_active_app(&self.last_turn);
        }
    }

    // ── Accesores para la burbuja flotante (bubble.rs) ─────────────────────
    pub(crate) fn is_bubble_window(&self, id: window::Id) -> bool {
        Some(id) == self.bubble_id
    }

    pub(crate) fn bubble_status(&self) -> &Status {
        &self.status
    }

    pub(crate) fn config_is_dark(&self) -> bool {
        self.config.is_dark()
    }

    fn dictionary_snapshot(&self) -> Vec<DictionaryEntry> {
        self.config.dictionary.clone()
    }

    /// Nivel del micrófono en % (0-100) mientras se graba.
    fn mic_level_percent(&self) -> u32 {
        self.audio.as_ref().map(|a| (a.level() * 100.0) as u32).unwrap_or(0)
    }

    /// Lectura del estado para el hero: color de estado + label + hint
    /// (feedback doble, nunca solo color).
    fn mic_ui(&self, t: &UiPalette) -> MicUi {
        match &self.status {
            Status::Idle => MicUi {
                color: t.dim,
                hint_color: t.dim,
                label: "Listo".to_string(),
                hint: "Pulsa el círculo o Ctrl+Shift+D y empieza a hablar".to_string(),
            },
            Status::Recording => MicUi {
                color: t.accent,
                hint_color: t.dim,
                label: "Grabando…".to_string(),
                hint: "Suelta el atajo o pulsa para transcribir".to_string(),
            },
            Status::Transcribing => MicUi {
                color: t.warn,
                hint_color: t.dim,
                label: "Transcribiendo…".to_string(),
                hint: "Procesando el audio con Whisper".to_string(),
            },
            Status::Correcting => MicUi {
                color: t.warn,
                hint_color: t.dim,
                label: "Corrigiendo…".to_string(),
                hint: "El modelo de corrección ajusta lo que dictaste".to_string(),
            },
            Status::Downloading { model, downloaded, total } => {
                let pct = if *total > 0 { (*downloaded as f64 / *total as f64 * 100.0) as u32 } else { 0 };
                MicUi {
                    color: t.warn,
                    hint_color: t.dim,
                    label: if *total > 0 {
                        format!("Descargando {}… {}%", model.label(), pct)
                    } else {
                        format!("Descargando {}…", model.label())
                    },
                    hint: "Primera vez con este modelo: se descarga una sola vez".to_string(),
                }
            }
            Status::Error(e) => MicUi {
                color: t.err,
                hint_color: t.err,
                label: "Error".to_string(),
                hint: e.clone(),
            },
        }
    }

    /// Aplica cleanup + diccionario + reglas de corrección hablada.
    /// Devuelve el texto del turno y si las REGLAS resolvieron una corrección
    /// (en ese caso el LLM no debe intervenir — nivel A de la spec §2).
    fn apply_pipeline(&mut self, raw: &str) -> (String, bool) {
        // Orden de procesado (igual que la web): cleanup -> diccionario.
        let mut text = raw.to_string();
        self.removed_fillers.clear();

        if self.config.cleanup_enabled.unwrap_or(true) {
            let cleaned = clean_transcript(raw);
            text = cleaned.text;
            self.removed_fillers = cleaned.removed;
        }

        if !self.config.dictionary.is_empty() {
            let dict = apply_dictionary(&text, &self.config.dictionary);
            text = dict.text;
        }

        // Corrección hablada multi-turno.
        let turn = self.session.accept(&text);
        if let Some(c) = &turn.correction {
            self.last_correction = Some(format!(
                "Rectificado: “{}” → “{}”",
                c.original, c.replacement
            ));
        } else if self.session.has_pending_correction() {
            self.last_correction = Some("Esperando el resto de la corrección…".to_string());
        } else if !self.last_turn.is_empty() {
            self.last_correction = None;
        }
        (turn.text, turn.correction.is_some())
    }
}

impl WhisprApp {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                if self.audio.is_some() {
                    return self.update(Message::StopRecording);
                }
                self.update(Message::StartRecording)
            }
            Message::StartRecording => {
                if self.audio.is_some() {
                    return Task::none();
                }
                // Una grabación nueva descarta cualquier inyección pendiente.
                self.pending_inject = false;
                let device = self.config.input_device.clone();
                match AudioCapture::start(device.as_deref()) {
                    Ok(capture) => {
                        self.audio = Some(capture);
                        self.status = Status::Recording;
                        self.session.reset();
                        self.last_turn.clear();
                    }
                    Err(e) => {
                        self.status = Status::Error(e);
                    }
                }
                Task::none()
            }
            Message::StopRecording => {
                let Some(mut capture) = self.audio.take() else {
                    return Task::none();
                };
                let samples = capture.stop();
                if samples.is_empty() {
                    self.pending_inject = false;
                    self.status = Status::Idle;
                    return Task::none();
                }

                let model = self.config.selected_model();
                // El diccionario viaja como initial_prompt: sesga el
                // reconocedor hacia jerga/nombres/siglas ANTES de transcribir.
                let dictionary = self.config.dictionary.clone();
                let (tx, rx) = mpsc::channel();
                self.transcribe_rx = Some(rx);
                self.status = Status::Transcribing;

                std::thread::spawn(move || {
                    let result = transcribe_samples(samples, model, dictionary);
                    let _ = tx.send(result);
                });

                Task::none()
            }
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
                            return Task::none();
                        }

                        // Corrección IA (v2, docs/CORRECCION-VOZ.md §3): si las
                        // reglas no resolvieron el turno, la función está
                        // activa y el turno huele a corrección («no no lo
                        // último está mal, es 2 en vez de 4»), se enruta al
                        // LLM: corrige el ÚLTIMO turno dictado usando esta
                        // frase como instrucción. Corre en un hilo (patrón
                        // transcribe_rx); el resultado se drena en Poll.
                        if !rule_corrected
                            && self.config.ai_correction_enabled()
                            && self.llm_rx.is_none()
                            && !self.last_turn.is_empty()
                            && crate::domain::corrective::classify(&processed)
                                == crate::domain::corrective::CorrectiveDecision::NeedsLlm
                        {
                            let target = self.last_turn.clone();
                            let instruction = processed.clone();
                            let llm_cfg = self.config.llm_settings();
                            let (tx, rx) = mpsc::channel();
                            self.llm_rx = Some(rx);
                            self.llm_pending = Some(LlmPending {
                                instruction: instruction.clone(),
                                target: target.clone(),
                                force_inject,
                            });
                            self.status = Status::Correcting;
                            crate::log!("llm", "corrección IA en camino…");
                            std::thread::spawn(move || {
                                let result =
                                    crate::llm::correct(&target, &instruction, &llm_cfg);
                                let _ = tx.send(result);
                            });
                            return Task::none();
                        }

                        self.last_turn = processed.clone();
                        if !self.last_turn.is_empty() {
                            self.transcript =
                                crate::domain::correction::append_transcript(&self.transcript, &processed);
                        }
                        self.status = Status::Idle;

                        if (force_inject || self.config.auto_inject.unwrap_or(true))
                            && !self.last_turn.is_empty()
                        {
                            match inject_text_into_active_app(&self.last_turn) {
                                Ok(()) => self.last_correction =
                                    Some("Texto escrito en la app activa ✓".to_string()),
                                Err(e) => self.status = Status::Error(format!("Inyección: {e}")),
                            }
                        }
                    }
                    Err(e) => self.status = Status::Error(e),
                }
                self.save_config();
                Task::none()
            }
            Message::ModelSelected(model) => {
                if self.status == Status::Recording {
                    return Task::none();
                }
                if model != self.config.selected_model() {
                    self.config.model = Some(model.id().to_string());
                    self.status = Status::Idle;
                    self.save_config();
                    // Aviso de descarga la primera vez que se use el modelo.
                    if !crate::transcribe::model_exists(model) {
                        self.status = Status::Downloading { model, downloaded: 0, total: 0 };
                        let (tx, rx) = mpsc::channel();
                        self.transcribe_rx = Some(rx);
                        std::thread::spawn(move || {
                            let result = crate::transcribe::ensure_model(
                                model,
                                |d, t| {
                                    let _ = tx.send(Ok(format!("__PROGRESS__{d}:{t}")));
                                },
                            )
                            .map(|_| "__DOWNLOAD_OK__".to_string())
                            .map_err(|e| e);
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
            Message::ToggleAiCorrection(on) => {
                self.config.ai_correction_enabled = Some(on);
                self.save_config();
                Task::none()
            }
            Message::CorrectionDone(result) => self.handle_llm_correction(result),
            Message::UndoLastCorrection => {
                if let Some((transcript, last_turn)) = self.undo_state.take() {
                    self.transcript = transcript;
                    self.last_turn = last_turn;
                    self.last_correction = Some("Corrección IA deshecha ↩".to_string());
                }
                Task::none()
            }
            Message::ClearTranscript => {
                self.transcript.clear();
                self.last_turn.clear();
                self.session.reset();
                self.last_correction = None;
                // Un undo apuntando a un transcript ya borrado sería un
                // resucitar fantasma: se descarta junto con el borrado.
                self.undo_state = None;
                Task::none()
            }
            Message::CopyTranscript => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(self.transcript.clone());
                    self.last_correction = Some("Transcript copiado al portapapeles ✓".to_string());
                }
                Task::none()
            }
            Message::InjectLast => {
                if !self.last_turn.is_empty() {
                    match inject_text_into_active_app(&self.last_turn) {
                        Ok(()) => self.last_correction =
                            Some("Texto escrito en la app activa ✓".to_string()),
                        Err(e) => self.status = Status::Error(format!("Inyección: {e}")),
                    }
                }
                Task::none()
            }
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
            Message::InputDeviceSelected(name) => {
                self.config.input_device = Some(name);
                // El micrófono se aplica en el siguiente turno de grabación.
                if self.audio.is_none() {
                    self.status = Status::Idle;
                }
                self.save_config();
                Task::none()
            }
            Message::ToggleTheme => {
                self.config.dark_mode = Some(!self.config.is_dark());
                self.save_config();
                Task::none()
            }
            Message::ToggleSettings => {
                self.settings_open = !self.settings_open;
                Task::none()
            }
            Message::Poll => {
                // Atajo global: toggle al detectar press.
                let mut toggled = false;
                for _ in 0..4 {
                    match self.hotkey_rx.try_recv() {
                        Ok(ev) => {
                            if ev.state() == global_hotkey::HotKeyState::Pressed && !toggled {
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
                                crate::log!("tray", "«Salir» desde la bandeja");
                                return iced::exit();
                            }
                            Ok(_) => continue,
                            Err(_) => break,
                        }
                    }
                }

                // Auto-recuperación del arrastre con botón derecho: si el
                // sistema roba la captura (alt-tab, menú del sistema, doble
                // clic en otra ventana) o el evento de soltado se pierde, se
                // cierra el arrastre limpio para no quedarse «atascado» ni
                // acumular saltos. `GetCapture` es el detector fiable de robo
                // de captura (equivale a WM_CAPTURECHANGED); el botón ya
                // liberado es la red de seguridad del evento perdido.
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
                                        if let Status::Downloading { model, .. } = &self.status {
                                            self.status = Status::Downloading {
                                                model: *model,
                                                downloaded: d,
                                                total: t,
                                            };
                                        }
                                    }
                                }
                            }
                            Task::none()
                        }
                        Ok(Ok(msg)) if msg == "__DOWNLOAD_OK__" => {
                            self.transcribe_rx = None;
                            self.status = Status::Idle;
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
                            Task::none()
                        }
                    };
                }
                Task::none()
            }
            Message::BubbleOpened(_id) => {
                // La ventana ya existe: quitarle la activación para que el
                // Ctrl+V simulado caiga en la app del usuario, no en la burbuja.
                crate::bubble::apply_noactivate_async();
                Task::none()
            }
            Message::BubblePress => {
                // Clic izquierdo: siempre graba (comportamiento original,
                // no se toca). El arrastre ya NO se dispara con el izquierdo.
                self.bubble_pressed = true;
                if self.audio.is_none() {
                    return self.update(Message::StartRecording);
                }
                // Ya se estaba grabando (desde la ventana principal): la burbuja
                // no vuelve a abrir el micrófono; el soltado transcribirá ese turno.
                Task::none()
            }
            Message::BubbleRelease => {
                let was_pressed = self.bubble_pressed;
                self.bubble_pressed = false;
                if !was_pressed {
                    return Task::none();
                }
                // Este turno va sí o sí a la app activa.
                self.pending_inject = true;
                self.update(Message::StopRecording)
            }
            Message::BubbleRightPress => {
                // El botón derecho solo arrastra: nunca inicia grabación ni
                // abre nada (el clic derecho simple queda en no-op).
                self.bubble_right.press(self.bubble_last_pos);
                // Captura YA en el pulsado (no al cruzar el umbral): con un
                // movimiento rápido el cursor puede salir de la ventana antes
                // de medir los 7 px del umbral; con la captura activa los
                // eventos siguen llegando y el arrastre arranca igualmente.
                // El clic derecho simple (pulsar+soltar sin mover) libera la
                // captura en la suelta, sin dejar nada raro.
                if self.bubble_drag.is_none() {
                    match crate::bubble::BubbleDrag::start() {
                        Some(drag) => {
                            crate::log!("bubble", "botón derecho pulsado: captura del ratón activa");
                            self.bubble_drag = Some(drag);
                        }
                        None => crate::log!("bubble", "no se pudo iniciar la captura del ratón"),
                    }
                }
                Task::none()
            }
            Message::BubbleRightRelease => {
                // Soltar el botón derecho siempre cierra la captura, hubiera
                // arrastre o no (la captura está activa desde el pulsado).
                self.bubble_right.release();
                if self.bubble_drag.is_some() {
                    self.end_bubble_drag("botón derecho soltado");
                }
                Task::none()
            }
            Message::BubbleCursorMoved(pos) => {
                self.bubble_last_pos = pos;
                // Arrastre con botón derecho: al superar el umbral la burbuja
                // empieza a seguir al cursor (la captura ya está activa desde
                // el pulsado, así que los movimientos llegan aunque el cursor
                // esté fuera de la ventana).
                if self.bubble_right.move_cursor(pos) {
                    crate::log!("bubble", "arrastre con botón derecho iniciado");
                }
                if self.bubble_right.dragging {
                    if let Some(drag) = &mut self.bubble_drag {
                        drag.follow();
                    }
                }
                Task::none()
            }
            Message::BubbleCursorLeft => {
                // El izquierdo dicta igualmente aunque el cursor se haya ido
                // (mismo comportamiento que antes, no se toca).
                let task = self.update(Message::BubbleRelease);
                if self.bubble_right.pressed || self.bubble_right.dragging {
                    // El botón derecho sigue pulsado con captura activa: el
                    // cursor salir de la ventana de 56 px es NORMAL durante
                    // un arrastre (SetCapture sigue entregando los movimientos
                    // aunque esté fuera). Aquí NO se cancela — cancelarlo era
                    // la causa de que con movimientos rápidos el drag se
                    // perdiera y la burbuja se quedara atrás saltando.
                    crate::log!(
                        "bubble",
                        "cursor fuera de la burbuja con botón derecho pulsado (el arrastre se mantiene)"
                    );
                }
                task
            }
            // ── Bandeja del sistema ─────────────────────────────────────────
            Message::WindowOpened(id) => {
                // La ventana principal abre antes que la burbuja; pero para no
                // depender de ordenes, se descarta la del id conocido de la
                // burbuja: la primera distinta es la principal.
                if self.main_window_id.is_none() && Some(id) != self.bubble_id {
                    self.main_window_id = Some(id);
                    crate::log!("tray", "ventana principal registrada (id={id})");
                }
                Task::none()
            }
            Message::ToggleMainWindow => {
                if self.main_visible {
                    self.hide_main_window()
                } else {
                    self.show_main_window()
                }
            }
            Message::FontLoaded(result) => {
                // `iced::font::Error` es ininhabitable en iced 0.13; el arm
                // Err queda como red de seguridad si cambia en el futuro.
                match result {
                    Ok(()) => crate::log!("fonts", "fuente emoji registrada en el FontSystem"),
                    Err(e) => crate::log!("fonts", "error cargando la fuente emoji: {e:?}"),
                }
                Task::none()
            }
            // Cierre de la ventana principal → se oculta a la bandeja (la app
            // sigue viva: burbuja, atajo global y bandeja). El cierre lo
            // gestionamos nosotros (`exit_on_close_request = false` en ambas
            // ventanas, ver `bubble::bubble_settings`).
            Message::WindowCloseRequested(id) if Some(id) == self.main_window_id => {
                self.hide_main_window()
            }
            // La burbuja no tiene botón de cierre (sin decoración): un
            // WM_CLOSE ahí solo llega desde fuera (taskkill, cierre de sesión)
            // y sí termina la app. También cubre el caso de que la principal
            // aún no se haya registrado (cierre antes de WindowOpened).
            Message::WindowCloseRequested(id) => {
                crate::log!("tray", "cierre de ventana {id} → salir");
                iced::exit()
            }
        }
    }

    /// Libera la captura del ratón y cierra el arrastre con botón derecho
    /// (la máquina de estados ya quedó limpia en el llamador). Log con el
    /// motivo: clave para diagnosticar arrastres que terminen raro.
    fn end_bubble_drag(&mut self, reason: &str) {
        if let Some(drag) = self.bubble_drag.take() {
            drag.end();
        }
        crate::log!("bubble", "fin de arrastre con botón derecho ({reason})");
    }

    /// Oculta la ventana principal (se queda en la bandeja) y actualiza el
    /// texto del ítem del menú.
    fn hide_main_window(&mut self) -> Task<Message> {
        let Some(id) = self.main_window_id else {
            return Task::none();
        };
        self.main_visible = false;
        crate::log!("tray", "ocultando ventana principal (queda en la bandeja)");
        if let Some(tray) = &self.tray {
            tray.toggle_item.set_text(crate::tray::MENU_LABEL_HIDDEN);
        }
        window::change_mode(id, window::Mode::Hidden)
    }

    /// Vuelve a mostrar (y enfocar) la ventana principal desde la bandeja.
    fn show_main_window(&mut self) -> Task<Message> {
        let Some(id) = self.main_window_id else {
            return Task::none();
        };
        self.main_visible = true;
        crate::log!("tray", "mostrando ventana principal");
        if let Some(tray) = &self.tray {
            tray.toggle_item.set_text(crate::tray::MENU_LABEL_VISIBLE);
        }
        Task::batch([
            window::change_mode(id, window::Mode::Windowed),
            window::gain_focus(id),
        ])
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let poll = iced::time::every(Duration::from_millis(100)).map(|_| Message::Poll);
        let closes = iced::window::close_requests().map(Message::WindowCloseRequested);
        // Descubre el id de la ventana principal (iced no lo devuelve al
        // crearla desde `program::run`; la burbuja se identifica por el suyo).
        let opened = iced::window::open_events().map(Message::WindowOpened);
        let bubble = crate::bubble::bubble_events_subscription(self.bubble_id);
        Subscription::batch([poll, closes, opened, bubble])
    }

    pub fn theme(&self) -> Theme {
        theme::custom(self.config.is_dark())
    }

    pub fn view(&self) -> Element<'_, Message> {
        let t = theme::tokens(&self.theme());

        // ── Cabecera: wordmark + controles globales ──────────────────────────
        let theme_toggle = if self.config.is_dark() { "☀ Claro" } else { "☾ Oscuro" };
        let settings_label = if self.settings_open { "Ajustes ▾" } else { "Ajustes ▸" };
        let header = row![
            text("Whispr Open").size(theme::FONT_XL).font(Font::MONOSPACE).color(t.text),
            Space::with_width(Length::Fill),
            button(theme_toggle).on_press(Message::ToggleTheme).style(theme::secondary),
            button(settings_label).on_press(Message::ToggleSettings).style(theme::secondary),
        ]
        .spacing(theme::SPACE_2)
        .align_y(iced::Alignment::Center);

        // ── Hero: estado del micrófono (tira horizontal compacta) ────────────
        // En portrait el hero deja de ser una columna centrada y pasa a una
        // fila: punto de estado + label + hint a la derecha; la barra de
        // nivel/progreso va debajo, a todo el ancho de la tarjeta.
        // Feedback doble intacto: color de estado (punto + borde) y label.
        let mic = self.mic_ui(t);
        let mut hero_content = column![
            row![
                text("●").size(theme::FONT_XL).color(mic.color),
                column![
                    text(mic.label).size(theme::FONT_HERO).color(t.text),
                    text(mic.hint).size(theme::FONT_SM).color(mic.hint_color),
                ]
                .width(Length::Fill)
                .spacing(theme::SPACE_1),
            ]
            .spacing(theme::SPACE_3)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(theme::SPACE_2);

        // Micro-feedback (>= 200 ms) no decorativo: nivel del mic / progreso.
        match &self.status {
            Status::Recording => {
                hero_content = hero_content.push(
                    progress_bar(0.0..=100.0, self.mic_level_percent() as f32)
                        .width(Length::Fill)
                        .style(move |th| theme::progress(th, t.accent)),
                );
            }
            Status::Downloading { downloaded, total, .. } if *total > 0 => {
                let pct = (*downloaded as f32 / *total as f32 * 100.0).min(100.0);
                hero_content = hero_content.push(
                    progress_bar(0.0..=100.0, pct)
                        .width(Length::Fill)
                        .style(move |th| theme::progress(th, t.warn)),
                );
            }
            _ => {}
        }

        let hero = button(
            container(hero_content)
                .width(Length::Fill)
                .padding(theme::SPACE_3)
                .style(move |th| theme::mic_card(th, mic.color)),
        )
        .on_press(Message::Toggle)
        .style(theme::mic);

        // ── Transcript: la zona central, scroll propio (tipo chat) ───────────
        let transcript_empty = self.transcript.is_empty();
        let transcript_card = container(
            scrollable(
                container(
                    text(if transcript_empty {
                        "Pulsa Ctrl+Shift+D o el botón del micrófono y empieza a hablar."
                    } else {
                        self.transcript.as_str()
                    })
                    .size(theme::FONT_MD)
                    .color(if transcript_empty { t.dim } else { t.text }),
                )
                .width(Length::Fill)
                .padding(theme::SPACE_3),
            )
            .style(theme::scrollable)
            // Sigue al final del texto: cada turno nuevo se ve al instante
            // (como una conversación de chat).
            .anchor_bottom()
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::flat);

        // ── Acciones discretas ───────────────────────────────────────────────
        let inject_label = if self.config.auto_inject.unwrap_or(true) {
            "Otra vez"
        } else {
            "Escribir en app"
        };
        let actions = row![
            button("Copiar").on_press(Message::CopyTranscript).style(theme::secondary),
            button("Limpiar").on_press(Message::ClearTranscript).style(theme::secondary),
            if self.last_turn.is_empty() {
                button(inject_label).style(theme::secondary)
            } else {
                button(inject_label).on_press(Message::InjectLast).style(theme::secondary)
            },
        ]
        .spacing(theme::SPACE_2);

        // ── Feedback textual (éxitos / espera / muletillas) ──────────────────
        // Con undo disponible, el feedback lleva el botón «Deshacer» (spec
        // §4.3: AI transparente y overrideable — un nivel de undo basta).
        let feedback: Element<'_, Message> = if let Some(corr) = &self.last_correction {
            let color = if corr.starts_with("Esperando")
                || corr.contains("no disponible")
                || corr.contains("no sé qué corregir")
            {
                t.warn
            } else {
                t.ok
            };
            let mut row_el = row![text(corr.as_str()).size(theme::FONT_SM).color(color)]
                .spacing(theme::SPACE_2)
                .align_y(iced::Alignment::Center);
            if self.undo_state.is_some() {
                row_el = row_el.push(
                    button("↩ Deshacer")
                        .on_press(Message::UndoLastCorrection)
                        .style(theme::secondary),
                );
            }
            row_el.into()
        } else if !self.removed_fillers.is_empty() {
            text(format!("Muletillas eliminadas: {}", self.removed_fillers.join(", ")))
                .size(theme::FONT_SM)
                .color(t.dim)
                .into()
        } else {
            text("").into()
        };

        // ── Zona central: transcript o ajustes (progressive disclosure) ──────
        // En portrait el ancho es pequeño: los ajustes se apoderan de la zona
        // central con su propio scroll en vez de empujar el transcript hacia
        // abajo (el transcript conserva su scroll independiente).
        let middle: Element<'_, Message> = if self.settings_open {
            scrollable(self.settings_view(t))
                .style(theme::scrollable)
                .height(Length::Fill)
                .into()
        } else {
            transcript_card.into()
        };

        let mut content = column![header, hero, middle]
            .spacing(theme::SPACE_3)
            .padding(theme::SPACE_3);
        // Las acciones y el feedback operan sobre el transcript: solo se
        // muestran cuando este está visible, no dentro de los ajustes.
        if !self.settings_open {
            content = content.push(actions).push(feedback);
        }

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|th| container::Style {
                background: Some(Background::Color(theme::tokens(th).bg)),
                ..Default::default()
            })
            .into()
    }

    /// Panel de configuración (oculto tras el botón Ajustes).
    fn settings_view(&self, t: &UiPalette) -> Element<'_, Message> {
        let model_pick = pick_list(
            Model::all().to_vec(),
            Some(self.config.selected_model()),
            Message::ModelSelected,
        )
        .placeholder("Modelo")
        // En portrait el ancho es limitado: el selector absorbe el espacio
        // sobrante de la fila en vez de desbordar la ventana.
        .width(Length::Fill)
        .style(theme::pick_list);
        let mic_pick = mic_pick_list(&self.input_devices, self.config.input_device.as_deref());

        let panel = column![
            row![
                text("Modelo").size(theme::FONT_SM).color(t.dim).width(Length::Fixed(96.0)),
                model_pick,
                text(format!("(~{} MB)", self.config.selected_model().approx_mb()))
                    .size(theme::FONT_SM)
                    .color(t.dim),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
            row![
                text("Micrófono").size(theme::FONT_SM).color(t.dim).width(Length::Fixed(96.0)),
                mic_pick,
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
            checkbox(
                "Limpiar muletillas",
                self.config.cleanup_enabled.unwrap_or(true),
            )
            .on_toggle(Message::ToggleCleanup)
            .style(theme::checkbox),
            checkbox(
                "Autoinyectar al terminar",
                self.config.auto_inject.unwrap_or(true),
            )
            .on_toggle(Message::ToggleAutoInject)
            .style(theme::checkbox),
            checkbox(
                "Corrección IA: otro modelo corrige lo dictado",
                self.config.ai_correction_enabled(),
            )
            .on_toggle(Message::ToggleAiCorrection)
            .style(theme::checkbox),
            text(format!("Atajo global: {}", crate::hotkey::DICTATION_SHORTCUT_LABEL))
                .size(theme::FONT_XS)
                .color(t.dim),
            horizontal_rule(1).style(theme::rule),
            text("Diccionario personal")
                .size(theme::FONT_LG)
                .font(Font::MONOSPACE)
                .color(t.text),
            row![
                text_input("término", &self.entry_from)
                    .on_input(Message::EntryFromChanged)
                    .style(theme::text_input),
                text_input("reemplazo", &self.entry_to)
                    .on_input(Message::EntryToChanged)
                    .style(theme::text_input),
                button("Añadir").on_press(Message::AddEntry).style(theme::primary),
            ]
            .spacing(theme::SPACE_2),
            dictionary_list(&self.config.dictionary),
        ]
        .spacing(theme::SPACE_2)
        .width(Length::Fill);

        container(panel)
            .width(Length::Fill)
            .padding(theme::SPACE_3)
            .style(|th| theme::card(th))
            .into()
    }
}

fn mic_pick_list(devices: &[String], selected: Option<&str>) -> Element<'static, Message> {
    if devices.is_empty() {
        return text("(sin micrófonos detectados)").size(theme::FONT_SM).into();
    }
    pick_list(devices.to_vec(), selected.map(|s| s.to_string()), Message::InputDeviceSelected)
        .placeholder("(por defecto)")
        // Los nombres de dispositivos son largos: el selector se ajusta al
        // ancho disponible (critical en portrait).
        .width(Length::Fill)
        .style(theme::pick_list)
        .into()
}

fn dictionary_list(entries: &[DictionaryEntry]) -> Element<'static, Message> {
    if entries.is_empty() {
        return text("Sin entradas todavía. Añade jerga, nombres o siglas.")
            .size(theme::FONT_SM)
            .into();
    }
    let mut col = Column::new().spacing(theme::SPACE_1);
    for (idx, entry) in entries.iter().enumerate() {
        col = col.push(
            row![
                text(format!("{} → {}", entry.from, entry.to)).size(theme::FONT_MD),
                button("✕").on_press(Message::RemoveEntry(idx)).style(theme::secondary),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
    }
    col.into()
}

/// Transcribe en un hilo de trabajo: a) descarga el modelo si falta (la
/// primera vez, silencioso), b) carga el contexto whisper.cpp, c) transcribe
/// con el diccionario del usuario como initial_prompt (sesgo léxico).
fn transcribe_samples(
    samples: Vec<f32>,
    model: Model,
    dictionary: Vec<DictionaryEntry>,
) -> Result<String, String> {
    if !crate::transcribe::model_exists(model) {
        crate::log!(
            "transcribe",
            "modelo {} no está descargado, descargando…",
            model.id()
        );
        crate::transcribe::ensure_model(model, |_, _| {})?;
    }
    let engine = WhisperEngine::load(model)?;
    let prompt = crate::transcribe::build_initial_prompt(&dictionary);
    engine.transcribe(&samples, "es", prompt.as_deref())
}

// ponytail: el contexto whisper se carga en cada turno; si el modelo small
// se usa a diario, cachear el engine (Mutex<Option<WhisperEngine>> por modelo).
#[allow(dead_code)]
fn _engine_cache_hint() {}