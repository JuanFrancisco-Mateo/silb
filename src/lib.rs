//! Whispr Open — dictado local de escritorio nativo (Iced + whisper.cpp).
//!
//! Módulos:
//! - `domain`: lógica pura portada de la versión web (corrección, muletillas, diccionario).
//! - `audio`: captura de micrófono (cpal).
//! - `transcribe`: transcripción con whisper-rs (GGML).
//! - `inject`: escritura en la app activa (arboard + enigo).
//! - `hotkey`: atajo global (global-hotkey).
//! - `theme`: tokens de diseño 2026 + estilos de widgets (única fuente de color).
//! - `app`: UI Iced.

pub mod log;
pub mod app;
pub mod audio;
pub mod config;
pub mod domain;
pub mod hotkey;
pub mod inject;
pub mod theme;
pub mod transcribe;