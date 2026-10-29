//! Dónde guarda la app sus cosas en cada sistema (portado a Linux/Wayland).
//!
//! Windows: `%APPDATA%/whispr-open/` (lo da `dirs::data_dir`).
//! Linux, siguiendo el estándar de escritorio:
//! - datos:     `$XDG_DATA_HOME` o `$HOME/.local/share` → `~/.local/share/whispr-open/`
//! - ajustes:  `$XDG_CONFIG_HOME` o `$HOME/.config` → `~/.config/whispr-open/`
//! - caché:    `$XDG_CACHE_HOME` o `$HOME/.cache` → `~/.cache/whispr-open/`
//!
//! En Linux el config.json y los datos van por separado, como pide el
//! estándar: lo que el usuario no toca a mano va en datos, lo que sí
//! edita va en ajustes y lo que se puede volver a generar va en caché.

use std::path::PathBuf;

/// Carpeta de DATOS de la app (`whispr-open/` dentro de la carpeta base del sistema).
pub fn app_data_dir() -> PathBuf {
    base_dir("XDG_DATA_HOME", ".local/share")
}

/// Carpeta de AJUSTES de la app.
pub fn app_config_dir() -> PathBuf {
    base_dir("XDG_CONFIG_HOME", ".config")
}

/// Carpeta de CACHÉ de la app (ficheros que se pueden volver a generar: descargas
/// a medias, logs viejos y grandes, etc.). Solo Linux; en Windows va todo a datos.
pub fn app_cache_dir() -> PathBuf {
    base_dir("XDG_CACHE_HOME", ".cache")
}

/// Carpeta base: la variable XDG si está puesta y, si no, `$HOME`.
///
/// Si tampoco hay `$HOME`, se queda en `.` (lo mismo que hacía antes
/// `dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))`:
/// la app sigue funcionando y guarda junto al ejecutable).
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
