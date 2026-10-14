//! Submódulos de UI extraídos de `app.rs` (refactor de organización, cero
//! cambio de conducta). `app.rs` se queda con el struct, `new`, el
//! despachador `update`, `subscription`, `theme`, `view` y los helpers
//! pequeños de estado; aquí vive el resto:
//!
//! - `settings`: vistas del panel de Ajustes y sus widgets.
//! - `settings_handlers`: manejadores de los mensajes de Ajustes (dictado,
//!   corrección IA, nota, listas, apariencia, paleta y motor de voz).
//! - `recording`: grabación, transcripción y ajustes del dictado.
//! - `bubble`: eventos de la ventana de la burbuja flotante.
//! - `window_tray`: ventana principal, bandeja y arranque.
//! - `poll`: tick del `Poll` (canales de los hilos, plazos y animación).
//! - `icons`: iconos vectoriales del diseño (SVG incrustados).
//! - `shell`: marco del rediseño (barra de ventana, barra lateral y los
//!   espacios de trabajo de los frames Dictado y Ajustes).
pub mod bubble;
pub mod icons;
pub mod poll;
pub mod recording;
pub mod settings;
pub mod settings_handlers;
pub mod shell;
pub mod window_tray;
