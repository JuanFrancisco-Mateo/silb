//! Aquí van las partes de la interfaz que salieron de `app.rs` (solo se
//! repartió el fichero, nada cambió de comportamiento). `app.rs` se queda
//! con la estructura, `new`, la función que reparte los mensajes `update`,
//! `subscription`, `theme`, `view` y los ayudantes pequeños de estado;
//! lo demás vive aquí:
//! - `settings`: las pantallas de Ajustes y sus controles.
//! - `settings_handlers`: qué hace cada mensaje de Ajustes (dictado,
//!   corrección IA, nota, listas, apariencia, paleta y motor de voz).
//! - `recording`: grabación, transcripción y ajustes del dictado.
//! - `bubble`: lo que pasa en la ventana de la burbuja flotante.
//! - `window_tray`: ventana principal, bandeja y arranque.
//! - `poll`: cada vuelta de `Poll` (canales de los hilos, plazos y animación).
//! - `icons`: iconos del diseño, metidos dentro del binario.
//! - `shell`: el marco del rediseño (barra de ventana, barra lateral y las
//!   zonas de trabajo de Dictado y Ajustes).
pub mod bubble;
pub mod icons;
pub mod poll;
pub mod recording;
pub mod settings;
pub mod settings_handlers;
pub mod shell;
pub mod window_tray;
