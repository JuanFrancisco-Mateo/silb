//! Manejadores de la ventana de la burbuja flotante.

use std::time::Duration;

use iced::Task;

use crate::app::{Message, WhisprApp};

impl WhisprApp {
    /// Eventos de la ventana de la burbuja flotante: apertura, pulsación
    /// izquierda (grabar), derecha (arrastrar), movimiento y salida.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_bubble_events(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::BubbleOpened(_id) => {
                // La ventana ya existe: aplicar lo que dicta el Config —
                // WS_EX_NOACTIVATE (para que el Ctrl+V simulado caiga en la
                // app del usuario), posición persistida o esquina, y tamaño.
                crate::bubble::apply_bubble_extras(&self.config);
                self.show_bubble()
            }
            Message::BubblePress => {
                // Clic izquierdo: siempre graba. Capturamos el ratón para que
                // salir de la burbuja no finalice prematuramente el turno.
                self.bubble_pressed = true;
                crate::bubble::capture_left_mouse();
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
                crate::bubble::release_mouse_capture();
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
                            crate::log!(
                                "bubble",
                                "botón derecho pulsado: captura del ratón activa"
                            );
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
                // Actividad del usuario sobre la burbuja: pospone el
                // auto-ocultar (no esconderla mientras el cursor está encima).
                if self.bubble_hide_deadline.is_some() {
                    if let Some(secs) = self.config.bubble_hide_timeout().secs() {
                        self.bubble_hide_deadline =
                            Some(std::time::Instant::now() + Duration::from_secs(secs));
                    }
                }
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
                // Con captura activa, CursorLeft no significa que el usuario
                // haya soltado el botón: puede seguir dictando fuera de la
                // ventana. El watchdog del Poll finaliza solo al detectar
                // físicamente liberado el botón.
                let task = Task::none();
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
            _ => Task::none(),
        }
    }

}