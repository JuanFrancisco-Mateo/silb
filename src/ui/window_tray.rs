//! Qué pasa con la ventana principal y con la bandeja del sistema.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use iced::Task;
use iced::window;

use crate::app::{LlmStatus, Message, StartupPhase, WhisprApp, run_startup_thread};

impl WhisprApp {
    /// Ventana principal y bandeja: se registra la ventana, se muestra u
    /// oculta desde el menú, se carga la fuente de emojis y se atienden los
    /// avisos del arranque (incluido «Reintentar»).
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_window_and_tray(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::WindowOpened(id) => {
                // La ventana principal abre antes que la burbuja; aun así, para no
                // depender del orden, se descarta la del id conocido de la burbuja:
                // la primera que no sea esa es la principal.
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
            // Botones propios de la barra de ventana (sin barra del sistema).
            // Sin id conocido (todavía no llegó `WindowOpened`) no hay nada que
            // mover: en ese hueco la barra no responde.
            Message::DragMainWindow => match self.main_window_id {
                Some(id) => window::drag(id),
                None => Task::none(),
            },
            Message::MinimizeMainWindow => match self.main_window_id {
                Some(id) => window::minimize(id, true),
                None => Task::none(),
            },
            Message::MaximizeMainWindow => match self.main_window_id {
                Some(id) => window::toggle_maximize(id),
                None => Task::none(),
            },
            Message::FontLoaded(result) => {
                // `iced::font::Error` no se puede construir en iced 0.13; la rama
                // Err queda por si eso cambia algún día.
                match result {
                    Ok(()) => crate::log!("fonts", "fuente emoji registrada en el FontSystem"),
                    Err(e) => crate::log!("fonts", "error cargando la fuente emoji: {e:?}"),
                }
                Task::none()
            }
            Message::Startup(event) => self.handle_startup_event(event),
            Message::RetryStartup => {
                crate::log!("startup", "«Reintentar»: relanzando arranque completo");
                self.startup = StartupPhase::Loading;
                self.startup_label = "Preparando la app…".to_string();
                self.llm_status = LlmStatus::Unknown;
                self.startup_tick = 0;
                let (startup_tx, startup_rx) = mpsc::channel();
                let startup_config = self.config.clone();
                let startup_cancel = Arc::new(AtomicBool::new(false));
                let startup_cancel_thread = startup_cancel.clone();
                std::thread::spawn(move || {
                    run_startup_thread(startup_config, startup_tx, startup_cancel_thread)
                });
                self.startup_rx = Some(startup_rx);
                self.llm_phase_cancel = Some(startup_cancel);
                Task::none()
            }
            Message::OpenLogFolder => {
                // H2 (QA 24/09/2026): la gente no encontraba el log.
                // Abrimos en el Explorador la carpeta que lo tiene (ahí están
                // también config.json y assets/).
                let dir = crate::log::log_dir();
                let _ = std::fs::create_dir_all(&dir);
                match std::process::Command::new("explorer.exe").arg(&dir).spawn() {
                    Ok(_) => crate::log!("ui", "carpeta del registro abierta: {}", dir.display()),
                    Err(e) => crate::log!("ui", "no se pudo abrir «{}»: {e}", dir.display()),
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

}