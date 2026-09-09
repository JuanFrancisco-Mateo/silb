//! Registro a archivo: `%APPDATA%/whispr-open/whispr-open.log`.
//!
//! Con `#![windows_subsystem = "windows"]` la app no abre consola, así que
//! los `eprintln!` quedan mudos. Este módulo centraliza el log: siempre
//! escribe a archivo (append, con timestamp) y, cuando la app corre desde
//! una consola (debug / `cargo run`), también repite a stderr.

use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// Serializa los writes: los logs llegan desde hilos distintos
/// (audio, hotkey, transcribe, UI).
static LOCK: Mutex<()> = Mutex::new(());

/// Ruta del log: `%APPDATA%/whispr-open/whispr-open.log`.
pub fn log_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("whispr-open")
        .join("whispr-open.log")
}

/// Macro de log con tag, mismo estilo que `eprintln!`:
///
/// ```ignore
/// crate::log!("audio", "error creando stream: {e}");
/// crate::log!("transcribe", "{}", whisper_rs::print_system_info());
/// ```
#[macro_export]
macro_rules! log {
    ($tag:expr, $($arg:tt)*) => {
        $crate::log::write_log($tag, &format!($($arg)*))
    };
}

/// Escribe una línea con timestamp en el archivo de log y, si stderr apunta
/// a una consola (padre con terminal → modo debug), también a stderr.
/// Best-effort: un fallo de log nunca debe romper la app.
pub fn write_log(tag: &str, msg: &str) {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    // El mensaje puede ser multi-línea (p. ej. print_system_info de CUDA):
    // se indenta el cuerpo para que el log se lea bien.
    let body = msg.replace('\n', "\n    ");
    let line = format!("[{ts}] [{tag}] {body}");

    if std::io::stderr().is_terminal() {
        eprintln!("{line}");
    }

    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = log_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_lives_under_appdata() {
        let p = log_path();
        assert!(p.to_string_lossy().contains("whispr-open"));
        assert!(p.to_string_lossy().contains("whispr-open.log"));
    }
}
