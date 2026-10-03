//! Rutas de la app por sistema operativo (port a Linux/Wayland).
//!
//! Windows: `%APPDATA%/whispr-open/` (vía `dirs::data_dir`).
//! Linux/Freedesktop spec:
//! - datos:     `$XDG_DATA_HOME` o `$HOME/.local/share` → `~/.local/share/whispr-open/`
//! - config:    `$XDG_CONFIG_HOME` o `$HOME/.config` → `~/.config/whispr-open/`
//! - cache:     `$XDG_CACHE_HOME` o `$HOME/.cache` → `~/.cache/whispr-open/`
//!
//! En Linux config.json (y el ajuste de datos permanecen separados,
//! como pide la spec: datos que el usuario no edita a mano en
//! data, config editable en config, ficheros regenerables en cache).

use std::path::PathBuf;

/// Carpeta de DATOS de la app (`whispr-open/` dentro del dir base por SO).
pub fn app_data_dir() -> PathBuf {
    base_dir("XDG_DATA_HOME", ".local/share")
}

/// Carpeta de CONFIG de la app.
pub fn app_config_dir() -> PathBuf {
    base_dir("XDG_CONFIG_HOME", ".config")
}

/// Carpeta de CACHE de la app (ficheros regenerables: descargas en curso,
/// logs rotados grandes, etc.). Solo Linux; en Windows es data.
pub fn app_cache_dir() -> PathBuf {
    base_dir("XDG_CACHE_HOME", ".cache")
}

/// Base por env var XDG o fallback bajo `$HOME` (spec XDG Base Directory).
///
/// Si ni siquiera hay `$HOME`, cae a `.` (el comportamiento de los
/// `dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))` originales:
/// la app sigue funcionando, guardando junto al ejecutable).
fn base_dir(xdg_env: &str, fallback: &str) -> PathBuf {
    if let Ok(xdg) = std::env::var(xdg_env) {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("whispr-open");
        }
    }
    match std::env::var("HOME") {
        Ok(home) if !home.trim().is_empty() => {
            PathBuf::from(home).join(fallback).join("whispr-open")
        }
        _ => PathBuf::from("."),
    }
}
