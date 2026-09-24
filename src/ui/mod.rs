//! Submódulos de UI extraídos de `app.rs` (refactor de organización, cero
//! cambio de conducta). `app.rs` se queda con el struct, `new`, el
//! despachador `update`, `subscription`, `theme`, `view` y los helpers
//! pequeños de estado; aquí vive el resto:
//!
//! - `settings`: vistas del panel de Ajustes y sus widgets.
//! - `recording`: grabación, transcripción y ajustes del dictado.
pub mod recording;
pub mod settings;
