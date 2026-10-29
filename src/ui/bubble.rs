//! Manejadores de la ventana de la burbuja flotante.

use std::time::Duration;

use iced::Task;

use crate::app::{Message, WhisprApp};

impl WhisprApp {
    /// Lo que pasa en la ventana de la burbuja flotante: al abrirse, clic
    /// izquierdo (grabar), derecho (arrastrar), movimiento y salida.
    ///
    /// Movido tal cual desde `WhisprApp::update` (ver `app.rs`).
    pub(crate) fn handle_bubble_events(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::BubbleOpened(_id) => {
                // La ventana ya existe: se aplica lo que diga el Config —
                // WS_EX_NOACTIVATE (para que el Ctrl+V simulado caiga en la app
                // del usuario), la posición guardada o la esquina, y el tamaño.
                crate::bubble::apply_bubble_extras(&self.config);
                self.show_bubble()
            }
            Message::BubblePress => {
                // Clic izquierdo: siempre graba. Se captura el ratón para que
                // salir de la burbuja no corte el turno antes de tiempo.
                self.bubble_pressed = true;
                crate::bubble::capture_left_mouse();
                if self.audio.is_none() {
                    return self.update(Message::StartRecording);
                }
                // Ya se estaba grabando (desde la ventana principal): la burbuja
                // no vuelve a abrir el micro; al soltar se transcribe ese turno.
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
                // El botón derecho solo arrastra: nunca empieza a grabar ni abre
                // nada (un clic derecho sin mover no hace nada).
                self.bubble_right.press(self.bubble_last_pos);
                // Se captura YA al pulsar (no al pasar el umbral): con un
                // movimiento rápido el cursor puede salir de la ventana antes de
                // medir los 7 px del umbral; con la captura puesta los avisos
                // siguen llegando y el arrastre arranca igual. Un clic derecho sin
                // mover (pulsar y soltar) suelta la captura al soltar, sin dejar
                // nada raro.
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
                // Soltar el botón derecho siempre quita la captura, haya
                // arrastre o no (la captura está puesta desde que se pulsó).
                self.bubble_right.release();
                if self.bubble_drag.is_some() {
                    self.end_bubble_drag("botón derecho soltado");
                }
                Task::none()
            }
            Message::BubbleCursorMoved(pos) => {
                self.bubble_last_pos = pos;
                // El usuario está tocando la burbuja: se retrasa el esconderla
                // sola (no se oculta con el cursor encima).
                if self.bubble_hide_deadline.is_some() {
                    if let Some(secs) = self.config.bubble_hide_timeout().secs() {
                        self.bubble_hide_deadline =
                            Some(std::time::Instant::now() + Duration::from_secs(secs));
                    }
                }
                // Arrastre con el botón derecho: al pasar el umbral la burbuja
                // empieza a seguir al cursor (la captura ya está puesta desde que
                // se pulsó, así que llegan los movimientos aunque el cursor esté
                // fuera de la ventana).
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
                // Con la captura puesta, `CursorLeft` no significa que el usuario
                // haya soltado el botón: puede seguir dictando fuera de la
                // ventana. El vigilante del `Poll` corta solo cuando ve el botón
                // físicamente suelto.
                let task = Task::none();
                if self.bubble_right.pressed || self.bubble_right.dragging {
                    // El botón derecho sigue pulsado con la captura puesta: que
                    // el cursor salga de la ventana de 56 px es NORMAL al
                    // arrastrar (SetCapture sigue mandando los movimientos aunque
                    // esté fuera). Aquí NO se cancela — cancelarlo era lo que hacía
                    // que, con movimientos rápidos, se perdiera el arrastre y la
                    // burbuja se quedara atrás a saltos.
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