//! Registro a archivo: `%APPDATA%/whispr-open/whispr-open.log`.
//!
//! Con `#![windows_subsystem = "windows"]` la app no abre consola, así que
//! los `eprintln!` quedan mudos. Este módulo centraliza el log: siempre
//! escribe a archivo (append, con timestamp) y, cuando la app corre desde
//! una consola (debug / `cargo run`), también repite a stderr.
//!
//! Dos niveles (H2 del QA 24/09/2026):
//!
//! - **Normal** ([`write_log`] / `crate::log!`): lo que el usuario o el
//!   soporte necesitan para entender una sesión.
//! - **Depuración** ([`write_debug`] / `crate::log_debug!`): trazas de alta
//!   frecuencia (p. ej. cada evento de ratón del arrastre de la burbuja).
//!   Están apagadas por defecto y se encienden con `WHISPR_DEBUG=1`; antes
//!   ensuciaban el 79 % del fichero y hacían ilegible el diagnóstico.
//!
//! Rotación por tamaño: al pasar de [`MAX_LOG_BYTES`] el fichero se
//! renombra a `.1`, el `.1` anterior pasa a `.2` (tope: [`MAX_LOG_BACKUPS`]
//! históricos) y se sigue escribiendo en el fichero actual. Todo con `std`,
//! sin dependencias nuevas.

use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Serializa los writes: los logs llegan desde hilos distintos
/// (audio, hotkey, transcribe, UI).
static LOCK: Mutex<()> = Mutex::new(());

/// Tamaño máximo del fichero de log antes de rotar (1 MiB, H2).
pub const MAX_LOG_BYTES: u64 = 1_048_576;

/// Ficheros históricos que se conservan: `.1` (el más reciente) y `.2`.
/// Con el tope de 1 MiB por fichero, el registro nunca ocupa más de ~3 MiB.
pub const MAX_LOG_BACKUPS: u8 = 2;

/// Variable de entorno que enciende el log de depuración.
pub const DEBUG_ENV: &str = "WHISPR_DEBUG";

/// Valor de `WHISPR_DEBUG` memoizado (leer el entorno en cada línea de
/// arrastre sería un desperdicio: el arrastre emite ~1.000 líneas/s).
static DEBUG: OnceLock<bool> = OnceLock::new();

/// Ruta del log: `%APPDATA%/whispr-open/whispr-open.log`.
pub fn log_path() -> PathBuf {
    log_dir().join("whispr-open.log")
}

/// Carpeta donde vive el log (y los assets de personalización):
/// `%APPDATA%/whispr-open/` (Windows) o `~/.local/share/whispr-open/`
/// (Linux, respeta `$XDG_DATA_HOME`). La UI la usa para «Abrir carpeta».
#[cfg(target_os = "windows")]
pub fn log_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("whispr-open")
}

/// Linux: log y assets en XDG data.
#[cfg(not(target_os = "windows"))]
pub fn log_dir() -> PathBuf {
    crate::paths::app_data_dir()
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

/// Macro de log de **depuración** (H2): solo escribe si `WHISPR_DEBUG`
/// está activada. Para trazas de alta frecuencia (cada evento de ratón,
/// cada tick del poll…):
///
/// ```ignore
/// crate::log_debug!("bubble", "drag target=({x},{y})");
/// ```
#[macro_export]
macro_rules! log_debug {
    ($tag:expr, $($arg:tt)*) => {
        $crate::log::write_debug($tag, &format!($($arg)*))
    };
}

/// ¿Está activado el log de depuración? (`WHISPR_DEBUG=1`, `true`, `on`,
/// `yes`, `sí`). Cacheado tras la primera consulta.
pub fn debug_enabled() -> bool {
    *DEBUG.get_or_init(|| debug_flag_from_value(std::env::var(DEBUG_ENV).ok().as_deref()))
}

/// Parser del valor de `WHISPR_DEBUG` (función pura, testeable).
fn debug_flag_from_value(value: Option<&str>) -> bool {
    match value {
        Some(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes" | "si" | "sí"
        ),
        None => false,
    }
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
    append_line(&log_path(), &line);
}

/// Igual que [`write_log`] pero solo si la depuración está activa. El tag
/// se marca con `[debug]` en el cuerpo para que sea evidente que esa
/// línea no está en el modo normal.
pub fn write_debug(tag: &str, msg: &str) {
    if debug_enabled() {
        write_log(tag, &format!("[debug] {msg}"));
    }
}

/// Añade una línea al fichero `path`, rotándolo antes si ya supera
/// [`MAX_LOG_BYTES`]. Crea el directorio padre si falta.
fn append_line(path: &Path, line: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    rotate_if_needed(path, MAX_LOG_BYTES, MAX_LOG_BACKUPS);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
}

/// Ruta del histórico `n` (1 = el más reciente): `whispr-open.log.1`.
fn backup_path(path: &Path, n: u8) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

/// Rota `path` si su tamaño alcanza `max_bytes`:
///
/// 1. se borra el histórico más viejo (`.{backups}`),
/// 2. cada histórico sube un puesto (`.1` → `.2`, …),
/// 3. el fichero actual pasa a `.1` y se deja libre para seguir escribiendo.
///
/// Devuelve `true` si rotó. Best-effort: si el FS falla, se sigue
/// escribiendo en el fichero actual (nunca se pierde el log del turno).
fn rotate_if_needed(path: &Path, max_bytes: u64, backups: u8) -> bool {
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        // No existe todavía: no hay nada que rotar.
        Err(_) => return false,
    };
    if size < max_bytes {
        return false;
    }
    if backups == 0 {
        let _ = std::fs::remove_file(path);
        return true;
    }
    // El histórico más viejo se descarta.
    let _ = std::fs::remove_file(backup_path(path, backups));
    // Los demás suben un puesto, del más viejo al más nuevo.
    for n in (1..backups).rev() {
        let from = backup_path(path, n);
        if from.is_file() {
            let _ = std::fs::rename(&from, backup_path(path, n + 1));
        }
    }
    let _ = std::fs::rename(path, backup_path(path, 1));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    /// Contador para que cada test use su propio directorio temporal.
    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let id = TEMP_ID.fetch_add(1, AtomicOrdering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "whispr_log_{tag}_{}_{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("crear temp dir");
        TempDir(dir)
    }

    fn size(path: &Path) -> u64 {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }

    #[test]
    fn log_path_lives_under_appdata() {
        let p = log_path();
        assert!(p.to_string_lossy().contains("whispr-open"));
        assert!(p.to_string_lossy().contains("whispr-open.log"));
        // El log vive junto al config, en la carpeta que ofrece «Abrir carpeta».
        assert!(p.starts_with(log_dir()));
    }

    #[test]
    fn debug_flag_accepts_si_no_y_si() {
        assert!(debug_flag_from_value(Some("1")));
        assert!(debug_flag_from_value(Some(" TRUE ")));
        assert!(debug_flag_from_value(Some("on")));
        assert!(debug_flag_from_value(Some("yes")));
        assert!(debug_flag_from_value(Some("sí")));
        // Ausente o cualquier otro valor (incluido «0») → apagado.
        assert!(!debug_flag_from_value(None));
        assert!(!debug_flag_from_value(Some("")));
        assert!(!debug_flag_from_value(Some("0")));
        assert!(!debug_flag_from_value(Some("false")));
        assert!(!debug_flag_from_value(Some("off")));
    }

    #[test]
    fn no_rota_por_debajo_del_umbral() {
        let dir = temp_dir("norota");
        let path = dir.0.join("whispr-open.log");
        append_line(&path, "linea 1");
        append_line(&path, "linea 2");
        assert!(!rotate_if_needed(&path, 10_000, MAX_LOG_BACKUPS));
        assert!(!backup_path(&path, 1).exists());
        assert!(size(&path) > 0);
    }

    #[test]
    fn rota_al_superar_el_umbral_y_conserva_dos_historicos() {
        let dir = temp_dir("rota");
        let path = dir.0.join("whispr-open.log");
        // Umbral diminuto: cada línea de ~30 bytes dispara una rotación.
        let max = 16;
        for n in 1..=3 {
            append_line(&path, &format!("linea {n} con relleno suficiente"));
            rotate_if_needed(&path, max, MAX_LOG_BACKUPS);
        }
        let b1 = backup_path(&path, 1);
        let b2 = backup_path(&path, 2);
        assert!(b1.is_file(), "el histórico .1 debe existir");
        assert!(b2.is_file(), "el histórico .2 debe existir");
        // El histórico .1 es MÁS RECIENTE que el .2.
        let t1 = std::fs::read_to_string(&b1).unwrap();
        let t2 = std::fs::read_to_string(&b2).unwrap();
        assert!(t1.contains("linea 3"), "el .1 lleva la línea más reciente: {t1}");
        assert!(t2.contains("linea 2"), "el .2 lleva la anterior: {t2}");
        // Se sigue escribiendo en el fichero actual (no se pierde el log).
        append_line(&path, "linea final");
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("linea final"));
        // Nunca hay más de MAX_LOG_BACKUPS históricos.
        assert!(!backup_path(&path, 3).exists());
    }

    #[test]
    fn sin_historicos_permitidos_el_fichero_se_vacia() {
        let dir = temp_dir("sinhistoricos");
        let path = dir.0.join("whispr-open.log");
        std::fs::write(&path, vec![b'x'; 64]).unwrap();
        assert!(rotate_if_needed(&path, 16, 0));
        assert!(!path.exists(), "sin históricos, la rotación borra el fichero");
    }
}
