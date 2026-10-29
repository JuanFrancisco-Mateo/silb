//! Silb — dictado local, una app de escritorio normal (Iced + whisper.cpp).
//!
//! Módulos:
//! - `domain`: lógica pura traída de la versión web (corrección, muletillas, diccionario).
//! - `audio`: captura del micrófono (cpal).
//! - `transcribe`: pasa el audio a texto con whisper-rs (GGML).
//! - `inject`: escribe en la app que tengas delante (arboard + enigo).
//! - `hotkey`: atajo global (global-hotkey).
//! - `theme`: colores y estilos de los controles del diseño 2026 (aquí sale todo el color).
//! - `fonts`: la fuente de marca Inter va dentro del binario (Regular/Medium/SemiBold)
//!   y se carga la fuente de emojis (Segoe UI Emoji) para que los símbolos de la UI
//!   (✓ ☀ ☾ ▾ ▸ ● → ✕ ⚠ ↩) no salgan como cajas vacías.
//! - `tray`: icono en la bandeja del sistema con menú Mostrar/Ocultar/Salir.
//! - `app`: la interfaz en Iced (estado + `update` + `view`; las funciones que
//!   atienden mensajes y las pantallas grandes viven en `ui`).
//! - `ui`: partes de la interfaz que salieron de `app` (pantallas de Ajustes,
//!   qué hace cada mensaje).

pub mod app;
pub mod audio;
pub mod bubble;
pub mod config;
pub mod custom_image;
pub mod domain;
pub mod fonts;
pub mod hotkey;
pub mod icon_header;
pub mod inject;
pub mod llm;
pub mod log;
pub mod paths;
pub mod single_instance;
pub mod theme;
pub mod transcribe;
pub mod tray;
pub mod ui;
