//! Whispr Open — dictado local de escritorio nativo (Iced + whisper.cpp).
//!
//! Módulos:
//! - `domain`: lógica pura portada de la versión web (corrección, muletillas, diccionario).
//! - `audio`: captura de micrófono (cpal).
//! - `transcribe`: transcripción con whisper-rs (GGML).
//! - `inject`: escritura en la app activa (arboard + enigo).
//! - `hotkey`: atajo global (global-hotkey).
//! - `theme`: tokens de diseño 2026 + estilos de widgets (única fuente de color).
//! - `fonts`: carga de la fuente emoji (Segoe UI Emoji) para que los símbolos
//!   de la UI (✓ ☀ ☾ ▾ ▸ ● → ✕) no se vean como cajas vacías.
//! - `tray`: bandeja del sistema (icono + menú Mostrar/Ocultar/Salir).
//! - `app`: UI Iced.

pub mod app;
pub mod audio;
pub mod bubble;
pub mod config;
pub mod domain;
pub mod fonts;
pub mod hotkey;
pub mod inject;
pub mod llm;
pub mod log;
pub mod single_instance;
pub mod theme;
pub mod transcribe;
pub mod tray;
