//! UI de Whispr Open con Iced: dictado por turnos, transcript acumulado,
//! limpieza de muletillas, diccionario personal, selector de modelo e
//! inyección en la app activa.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use iced::widget::{
    button, checkbox, column, container, horizontal_rule, pick_list, row, scrollable, text,
    text_input, Column,
};
use iced::{Color, Element, Font, Length, Subscription, Task, Theme};
use global_hotkey::GlobalHotKeyEvent;

use crate::audio::AudioCapture;
use crate::config::Config;
use crate::domain::cleanup::clean_transcript;
use crate::domain::correction::CorrectionSession;
use crate::domain::dictionary::{apply_dictionary, DictionaryEntry};
use crate::hotkey::HotkeyManager;
use crate::transcribe::{Model, WhisperEngine};
use crate::inject::inject_text_into_active_app;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Recording,
    Transcribing,
    Downloading { model: Model, downloaded: u64, total: u64 },
    Error(String),
}

impl Status {
    pub fn label(&self) -> String {
        match self {
            Status::Idle => "Listo".to_string(),
            Status::Recording => "● Grabando…".to_string(),
            Status::Transcribing => "Transcribiendo…".to_string(),
            Status::Downloading { model, downloaded, total } => {
                let pct = if *total > 0 { (*downloaded as f64 / *total as f64 * 100.0) as u32 } else { 0 };
                format!("Descargando {}… {}%", model.label(), pct)
            }
            Status::Error(e) => e.clone(),
        }
    }
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
    ClearTranscript,
    CopyTranscript,
    InjectLast,
    AddEntry,
    EntryFromChanged(String),
    EntryToChanged(String),
    RemoveEntry(usize),
    InputDeviceSelected(String),
    Poll,
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
                eprintln!("[hotkey] {e}");
                let stub = HotkeyManager::stub();
                let rx = stub.listen();
                (stub, rx)
            }
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
        };
        (app, Task::none())
    }

    fn save_config(&self) {
        let mut cfg = self.config.clone();
        cfg.dictionary = self.dictionary_snapshot();
        cfg.model = Some(self.config.selected_model().id().to_string());
        cfg.save();
    }

    fn dictionary_snapshot(&self) -> Vec<DictionaryEntry> {
        self.config.dictionary.clone()
    }

    /// Nivel del micrófono en % (0-100) mientras se graba.
    fn mic_level_percent(&self) -> u32 {
        self.audio.as_ref().map(|a| (a.level() * 100.0) as u32).unwrap_or(0)
    }

    fn apply_pipeline(&mut self, raw: &str) -> String {
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
        turn.text
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
                    self.status = Status::Idle;
                    return Task::none();
                }

                let model = self.config.selected_model();
                let (tx, rx) = mpsc::channel();
                self.transcribe_rx = Some(rx);
                self.status = Status::Transcribing;

                std::thread::spawn(move || {
                    let result = transcribe_samples(samples, model);
                    let _ = tx.send(result);
                });

                Task::none()
            }
            Message::TranscriptionDone(result) => {
                self.transcribe_rx = None;
                match result {
                    Ok(raw) => {
                        let processed = self.apply_pipeline(&raw);
                        if processed.is_empty() && raw.trim().is_empty() {
                            self.status = Status::Idle;
                            return Task::none();
                        }
                        self.last_turn = processed.clone();
                        if !self.last_turn.is_empty() {
                            self.transcript =
                                crate::domain::correction::append_transcript(&self.transcript, &processed);
                        }
                        self.status = Status::Idle;

                        if self.config.auto_inject.unwrap_or(true) && !self.last_turn.is_empty() {
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
            Message::ClearTranscript => {
                self.transcript.clear();
                self.last_turn.clear();
                self.session.reset();
                self.last_correction = None;
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

                // Resultado de transcripción/descarga.
                if let Some(rx) = &self.transcribe_rx {
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
                }
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        iced::time::every(Duration::from_millis(100)).map(|_| Message::Poll)
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = row![
            text("Whispr Open").size(22).font(Font::MONOSPACE),
            horizontal_rule(1),
            status_pill(&self.status),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        // Zona de transcript.
        let transcript_area = scrollable(
            container(text(if self.transcript.is_empty() {
                "Pulsa Ctrl+Shift+D o el botón Grabar y empieza a hablar."
            } else {
                &self.transcript
            }))
            .width(Length::Fill)
            .padding(12),
        )
        .height(Length::Fill);

        let recording_pill = match &self.status {
            Status::Recording => Some("● Grabando… (suelta el atajo para transcribir)"),
            _ => None,
        };

        let controls = column![
            row![
                match &self.status {
                    Status::Recording => button("■ Detener").on_press(Message::StopRecording),
                    _ => button("● Grabar").on_press(Message::StartRecording),
                },
                button("Copiar").on_press(Message::CopyTranscript),
                button("Limpiar").on_press(Message::ClearTranscript),
                if self.config.auto_inject.unwrap_or(true) {
                    button("Otra vez").on_press(Message::InjectLast)
                } else {
                    button("Escribir en app").on_press(Message::InjectLast)
                },
            ]
            .spacing(8),
            recording_row(
                recording_pill,
                self.mic_level_percent(),
            ),
            if let Some(corr) = &self.last_correction {
                text(corr).size(12).color(Color::from_rgb(0.6, 0.8, 1.0))
            } else {
                text("")
            },
            if !self.removed_fillers.is_empty() {
                text(format!("Muletillas eliminadas: {}", self.removed_fillers.join(", ")))
                    .size(12)
                    .color(Color::from_rgb(0.8, 0.7, 0.5))
            } else {
                text("")
            },
        ]
        .spacing(4);

        // Panel de configuración.
        let model_pick = pick_list(
            Model::all().to_vec(),
            Some(self.config.selected_model()),
            Message::ModelSelected,
        )
        .placeholder("Modelo");

        let settings = column![
            text("Configuración").size(16).font(Font::MONOSPACE),
            row![
                text("Micrófono:"),
                mic_pick_list(&self.input_devices, self.config.input_device.as_deref()),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![
                text("Modelo Whisper:"),
                model_pick,
                text(format!("(~{} MB)", self.config.selected_model().approx_mb()))
                    .size(12),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            checkbox(
                "Limpiar muletillas",
                self.config.cleanup_enabled.unwrap_or(true),
            )
            .on_toggle(Message::ToggleCleanup),
            checkbox(
                "Autoinyectar al terminar",
                self.config.auto_inject.unwrap_or(true),
            )
            .on_toggle(Message::ToggleAutoInject),
            text(format!("Atajo global: {}", crate::hotkey::DICTATION_SHORTCUT_LABEL)).size(12),
            horizontal_rule(1),
            text("Diccionario personal").size(16).font(Font::MONOSPACE),
            row![
                text_input("término", &self.entry_from).on_input(Message::EntryFromChanged),
                text_input("reemplazo", &self.entry_to).on_input(Message::EntryToChanged),
                button("Añadir").on_press(Message::AddEntry),
            ]
            .spacing(6),
            dictionary_list(&self.config.dictionary),
        ]
        .spacing(8)
        .width(Length::Fill);

        let content = column![header, transcript_area, controls, horizontal_rule(1), settings]
            .spacing(10)
            .padding(16);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn status_pill(status: &Status) -> Element<'static, Message> {
    let (label, color) = match status {
        Status::Idle => ("Listo", Color::from_rgb(0.5, 0.8, 0.5)),
        Status::Recording => ("Grabando", Color::from_rgb(1.0, 0.4, 0.4)),
        Status::Transcribing => ("Transcribiendo…", Color::from_rgb(0.9, 0.8, 0.4)),
        Status::Downloading { .. } => ("Descargando modelo…", Color::from_rgb(0.8, 0.7, 0.9)),
        Status::Error(_) => ("Error", Color::from_rgb(1.0, 0.5, 0.5)),
    };
    text(label).color(color).size(13).into()
}

/// Línea de estado durante la grabación: pill + medidor de nivel del micrófono.
fn recording_row(pill: Option<&'static str>, level_pct: u32) -> Element<'static, Message> {
    match pill {
        Some(pill) => row![
            text(pill).color(Color::from_rgb(1.0, 0.4, 0.4)),
            text(format!("Nivel: {level_pct}%"))
                .size(12)
                .color(Color::from_rgb(0.8, 0.9, 0.7)),
        ]
        .spacing(16)
        .into(),
        None => text("").into(),
    }
}

fn mic_pick_list(devices: &[String], selected: Option<&str>) -> Element<'static, Message> {
    if devices.is_empty() {
        return text("(sin micrófonos detectados)").size(12).into();
    }
    pick_list(devices.to_vec(), selected.map(|s| s.to_string()), Message::InputDeviceSelected)
        .placeholder("(por defecto)")
        .into()
}

fn dictionary_list(entries: &[DictionaryEntry]) -> Element<'static, Message> {
    if entries.is_empty() {
        return text("Sin entradas todavía. Añade jerga, nombres o siglas.").size(12).into();
    }
    let mut col = Column::new().spacing(4);
    for (idx, entry) in entries.iter().enumerate() {
        col = col.push(
            row![
                text(format!("{} → {}", entry.from, entry.to)).size(13),
                button("✕").on_press(Message::RemoveEntry(idx)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }
    col.into()
}

/// Transcribe en un hilo de trabajo: a) descarga el modelo si falta (la
/// primera vez, silencioso), b) carga el contexto whisper.cpp, c) transcribe.
fn transcribe_samples(samples: Vec<f32>, model: Model) -> Result<String, String> {
    if !crate::transcribe::model_exists(model) {
        eprintln!(
            "[transcribe] modelo {} no está descargado, descargando…",
            model.id()
        );
        crate::transcribe::ensure_model(model, |_, _| {})?;
    }
    let engine = WhisperEngine::load(model)?;
    engine.transcribe(&samples, "es")
}

// ponytail: el contexto whisper se carga en cada turno; si el modelo small
// se usa a diario, cachear el engine (Mutex<Option<WhisperEngine>> por modelo).
#[allow(dead_code)]
fn _engine_cache_hint() {}