//! Guard de instancia única (requisito de robustez): una segunda ejecución
//! de `whispr-open` NO debe robar ni duplicar el atajo global. Sin este
//! guard, el registro del atajo de la segunda instancia fallaría a ciegas
//! (Windows no permite dos `RegisterHotKey` del mismo atajo), la segunda
//! bandeja se acumularía en el tray y el usuario vería dos burbujas.
//!
//! Mecanismo por plataforma:
//! - **Windows**: mutex nombrado (`CreateMutexW`). El nombre vive en el
//!   namespace `Local\` (por sesión de usuario): dos sesiones de escritorio
//!   distintas pueden tener su instancia —que es lo correcto para una app de
//!   usuario— pero dentro de la misma sesión solo pasa la primera.
//! - **Linux (Wayland/X11)**: lock file con flock (`std::fs::File::try_lock`,
//!   estable desde Rust 1.88) en `$XDG_RUNTIME_DIR/whispr-open/instance.lock`
//!   (`$TMPDIR` como recurso). flock se libera solo al morir el proceso, así
//!   que un crash-kill no deja el candado pegado; además se guarda el pid
//!   para diagnóstico. El directorio XDG_RUNTIME es por usuario (uid), que es
//!   el equivalente Linux del namespace `Local\` de Windows.

use std::sync::OnceLock;

/// Namespace `Local\` = por sesión de usuario iniciada (Windows). Identidad
/// fija de la app: cualquier proceso que la conozca ve el mismo mutex.
pub const MUTEX_NAME: &str = "Local\\WhisprOpen-SingleInstance";

/// `GetLastError` de un `CreateMutexW` sobre un mutex que ya existía.
const ERROR_ALREADY_EXISTS: u32 = 183;

/// Nombre del fichero de lock en Linux. Vive bajo `whispr-open/` dentro del
/// runtime dir por usuario (mismo patrón que config.json en %APPDATA%).
#[cfg(unix)]
pub const LOCK_FILE_NAME: &str = "instance.lock";

#[cfg(target_os = "windows")]
static MUTEX: OnceLock<usize> = OnceLock::new();

/// Descriptor del lock file: vivo mientras viva el proceso (cerrarlo
/// liberaría el flock y dejaría colarse otra instancia).
#[cfg(unix)]
static LOCK_FILE: OnceLock<Option<std::fs::File>> = OnceLock::new();

#[cfg(unix)]
fn lock_file_path() -> std::path::PathBuf {
    // XDG_RUNTIME_DIR = /run/user/$UID en Arch/Omarchy: dentro de un login
    // session normal siempre está (pam_systemd); si faltara (cron, ssh sin
    // session), se cae al tmpdir del sistema.
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    base.join("whispr-open").join(LOCK_FILE_NAME)
}

/// ¿Está vivo el proceso `pid`? Solo para diagnóstico/log (Linux): el lock
/// real es el flock del kernel, no el pid.
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(unix)]
fn read_pid(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok()
}

/// Intenta adquirir el guard. `true` = esta ejecución es la primera (se
/// queda con el lock); `false` = ya hay otra instancia y esta debe salir
/// sin registrar atajo, bandeja ni burbuja.
///
/// Si el propio lock no pudiera ni crearse ni abrirse (disco lleno,
/// $XDG_RUNTIME_DIR sin permisos…) se devuelve `true`: mejor una app sin
/// guard que una app que no arranca por su mecanismo anti-duplicado (el
/// atajo ya degrada a stub por su cuenta si el registro falla).
#[cfg(target_os = "windows")]
pub fn acquire() -> bool {
    if *MUTEX.get_or_init(|| 0) != 0 {
        // Ya adquirido por este mismo proceso (llamada repetida).
        return true;
    }
    let mut name: Vec<u16> = MUTEX_NAME.encode_utf16().collect();
    name.push(0);
    unsafe {
        let handle = winapi::um::synchapi::CreateMutexW(
            std::ptr::null_mut(),
            0,
            name.as_ptr(),
        );
        if handle.is_null() {
            crate::log!(
                "single-instance",
                "no se pudo crear el mutex {MUTEX_NAME} (GetLastError={}); \
                 se continúa sin guard",
                winapi::um::errhandlingapi::GetLastError()
            );
            return true;
        }
        if winapi::um::errhandlingapi::GetLastError() == ERROR_ALREADY_EXISTS {
            // La otra instancia es la dueña: soltar nuestro handle duplicado.
            winapi::um::handleapi::CloseHandle(handle);
            false
        } else {
            let _ = MUTEX.set(handle as usize);
            true
        }
    }
}

/// Linux/UNIX: lock file con flock. Semántica fiel a la del mutex Windows:
/// - el fichero no puede ser la fuente de verdad (un crash anterior deja el
///   fichero ahí); la verdad es si se puede flock-ear;
/// - solo se recupera (se recorta y se reescribe el pid) cuando el flock se
///   consigue: garantiza que la instancia viva es la única en candarlo.
#[cfg(unix)]
pub fn acquire() -> bool {
    use std::io::Write;

    let path = lock_file_path();
    if let Some(parent) = path.parent() {
        // Fail-soft: si el directorio no se puede crear (mono-usuario roto,
        // sistema de sólo lectura), caemos al caso "sin guard" de abajo.
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
    {
        Ok(mut file) => match file.try_lock() {
            // Lock nuestro: escribir el pid para diagnóstico (no es la fuente
            // de verdad —el flock del kernel lo es—, solo una pista para
            // `kill` o QA). El descriptor se guarda vivo: cerrarlo liberaría
            // el lock.
            Ok(true) => {
                let _ = file.set_len(0);
                let _ = write!(file, "{}\n", std::process::id());
                let _ = file.flush();
                let _ = LOCK_FILE.set(Some(file));
                true
            }
            // flock ocupado: otra instancia viva lo tiene. Leemos su pid solo
            // para avisar; NO se recupera aquí (el owner es el dueño legítimo).
            Ok(false) => {
                let pid_hint = std::fs::read_to_string(&path).ok().and_then(|s| {
                    read_pid(&s).filter(|pid| pid_alive(*pid))
                });
                crate::log!(
                    "single-instance",
                    "lock {} ocupado por el pid {pid_hint:?}; esta instancia sale",
                    path.display()
                );
                false
            }
            Err(err) => {
                crate::log!(
                    "single-instance",
                    "flock sobre {} falló ({err}); se continúa sin guard",
                    path.display()
                );
                true
            }
        },
        Err(err) => {
            crate::log!(
                "single-instance",
                "no se pudo abrir el lock {} ({err}); se continúa sin guard",
                path.display()
            );
            true
        }
    }
}

/// Stub fuera de Windows/UNIX: sin mutex con nombre ni lock file no hay guard
/// de instancia única; `true` (esta instancia es la dueña) para no bloquear
/// el arranque.
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
pub fn acquire() -> bool {
    true
}

/// Aviso nativo para quien lanzó la segunda instancia con un doble clic y
/// no ve aparecer nada: la app que ya corría sigue en la bandeja.
#[cfg(target_os = "windows")]
pub fn notify_second_instance() {
    // MessageBoxW espera cadenas UTF-16 (wide): se terminan en NUL.
    let mut texto: Vec<u16> =
        "Ya hay otra instancia de Whispr Open abierta.\n\nMira la bandeja del sistema (junto al reloj) para usarla."
            .encode_utf16()
            .collect();
    texto.push(0);
    let mut titulo: Vec<u16> = "Whispr Open".encode_utf16().collect();
    titulo.push(0);
    unsafe {
        winapi::um::winuser::MessageBoxW(
            std::ptr::null_mut(),
            texto.as_ptr(),
            titulo.as_ptr(),
            winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONINFORMATION,
        );
    }
}

/// Linux: aviso ligero por el canal de notificaciones del escritorio
/// (`notify-send`, org.freedesktop.Notifications). Best-effort: sin daemon de
/// notificaciones o sin binary, solo queda el log (NO se dibuja UI propia).
#[cfg(unix)]
pub fn notify_second_instance() {
    let summary = "Whispr Open ya está en ejecución";
    let body =
        "Ya hay otra instancia de Whispr Open abierta.\n\nMira la bandeja del sistema para usarla.";
    match std::process::Command::new("notify-send")
        .args(["-a", "Whispr Open", summary, body])
        .output()
    {
        Ok(out) if out.status.success() => {
            crate::log!("single-instance", "aviso de segunda instancia vía notify-send")
        }
        Ok(_) => crate::log!(
            "single-instance",
            "notify-send devolvió error (¿sin daemon de notificaciones?); sin banner"
        ),
        Err(_) => crate::log!(
            "single-instance",
            "notify-send no está en el PATH; sin banner"
        ),
    }
}

/// Stub fuera de Windows/UNIX (el aviso real es un `MessageBoxW`).
#[cfg(not(any(target_os = "windows", target_family = "unix")))]
pub fn notify_second_instance() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn la_segunda_adquisicion_ve_el_mutex_ocupado() {
        // La primera llamada de este proceso de tests adquiere el mutex
        // (si la app real estuviera abierta en esta sesión mientras se
        // lanzan los tests, este test lo detectaría como falso positivo:
        // ciérrala antes de `cargo test`).
        let first = acquire();
        assert!(first, "la primera adquisición del proceso de tests pasa");
        // Una segunda creación con el MISMO nombre debe ver
        // ERROR_ALREADY_EXISTS: es exactamente lo que sentirá la segunda
        // instancia de la app (mismo mecanismo, mismo mutex).
        assert!(
            !acquire(),
            "una segunda instancia no debe poder adquirir el guard"
        );
    }

    #[test]
    fn el_nombre_del_mutex_es_local_a_la_sesion() {
        // Windows: namespace Local\ (dos sesiones de usuario distintas no se
        // pisan). Linux: el equivalente es el runtime dir por usuario.
        assert!(MUTEX_NAME.starts_with("Local\\"));
        assert!(MUTEX_NAME.contains("WhisprOpen"));
    }

    #[cfg(unix)]
    #[test]
    fn el_lock_vive_en_el_runtime_dir_bajo_whispr_open() {
        let p = lock_file_path();
        assert_eq!(p.file_name().and_then(|n| n.to_str()), Some(LOCK_FILE_NAME));
        assert!(
            p.to_string_lossy().contains("whispr-open"),
            "el lock vive junto a la identidad de la app"
        );
    }

    #[cfg(unix)]
    #[test]
    fn el_pid_se_parsea_de_la_pista() {
        assert_eq!(read_pid(" 1234 \n"), Some(1234));
        assert_eq!(read_pid("no-es-un-pid"), None);
        assert_eq!(read_pid(""), None);
    }
}
