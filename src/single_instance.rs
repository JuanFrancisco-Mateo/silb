//! Solo puede haber una copia abierta: una segunda ejecución de `silb.exe`
//! no debe quitar ni duplicar el atajo global. Sin esto, el `RegisterHotKey`
//! de la segunda fallaría sin avisar (Windows no deja dos registros del mismo
//! atajo), la segunda bandeja se acumularía y el usuario vería dos burbujas.
//!
//! Cómo: un mutex con nombre de Windows (`CreateMutexW`). El nombre vive en
//! `Local\` (por sesión de usuario): dos sesiones de escritorio distintas
//! pueden tener su copia —que es lo correcto para una app de usuario— pero
//! dentro de la misma sesión solo pasa la primera.

use std::sync::OnceLock;

/// `Local\` = por sesión de usuario (no para toda la máquina). Nombre fijo de
/// la app: cualquier proceso que lo sepa ve el mismo mutex.
pub const MUTEX_NAME: &str = "Local\\WhisprOpen-SingleInstance";

/// `GetLastError` de un `CreateMutexW` sobre un mutex que ya existía.
const ERROR_ALREADY_EXISTS: u32 = 183;

/// Handle del mutex, vivo mientras viva el proceso (si se cerrara, otra
/// copia podría colarse). Se guarda como `usize` porque un HANDLE (puntero
/// crudo) no se puede compartir entre hilos y los statics lo exigen.
static MUTEX: OnceLock<usize> = OnceLock::new();

/// Intenta coger el turno. `true` = esta ejecución es la primera (se queda con
/// el mutex); `false` = ya hay otra copia y esta debe salir sin registrar
/// atajo, bandeja ni burbuja.
///
/// Si el mutex no se pudiera crear (memoria agotada…), se devuelve `true`:
/// mejor una app sin esta protección que una app que no arranca por su propio
/// mecanismo anti-duplicados (el atajo ya se queda en modo sin atajo solo si
/// el registro falla).
#[cfg(target_os = "windows")]
pub fn acquire() -> bool {
    if *MUTEX.get_or_init(|| 0) != 0 {
        // Ya lo tiene este mismo proceso (llamada repetida).
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
            // La otra copia es la dueña: se suelta nuestro handle de más.
            winapi::um::handleapi::CloseHandle(handle);
            false
        } else {
            let _ = MUTEX.set(handle as usize);
            true
        }
    }
}

/// [Linux/macOS] Protección con un fichero bloqueado (`flock`) en
/// `$XDG_RUNTIME_DIR/whispr-open/instance.lock` (`$TMPDIR` si no está).
/// flock lo suelta el sistema al morir el proceso, así que un cierre brusco
/// no deja el candado pegado. `true` = esta copia es la dueña.
///
/// ponytail: el bloqueo se mantiene con `mem::forget` (dura lo que dura el
/// proceso, que es justo lo que queremos); si algún día hacen falta
/// «segundas copias que avisan a la primera», guardar el File en un OnceLock
/// y usar el socket de aviso.
#[cfg(not(target_os = "windows"))]
pub fn acquire() -> bool {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("whispr-open");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("instance.lock");
    match std::fs::File::create(&path) {
        Ok(file) => match file.try_lock() {
            Ok(()) => {
                let _ = std::fs::write(dir.join("instance.pid"), std::process::id().to_string());
                std::mem::forget(file);
                true
            }
            Err(_) => false,
        },
        // Si no se puede crear el fichero (disco raro), no se bloquea el arranque.
        Err(_) => true,
    }
}

/// Aviso del sistema para quien abrió la segunda copia con doble clic y no ve
/// nada: la app que ya corría sigue en la bandeja.
#[cfg(target_os = "windows")]
pub fn notify_second_instance() {
    // MessageBoxW pide texto UTF-16: se cierra con NUL.
    let mut texto: Vec<u16> =
        "Ya hay otra instancia de Silb abierta.\n\nMira la bandeja del sistema (junto al reloj) para usarla."
            .encode_utf16()
            .collect();
    texto.push(0);
    let mut titulo: Vec<u16> = "Silb".encode_utf16().collect();
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

/// Versión para otros sistemas (ahí el aviso de verdad es un `MessageBoxW`).
#[cfg(not(target_os = "windows"))]
pub fn notify_second_instance() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn la_segunda_adquisicion_ve_el_mutex_ocupado() {
        // La primera llamada de este proceso de tests coge el mutex (si la
        // app real estuviera abierta en esta sesión mientras se lanzan los
        // tests, este test lo vería como falso positivo: ciérrala antes de
        // `cargo test`).
        let first = acquire();
        assert!(first, "la primera adquisición del proceso de tests pasa");
        // Una segunda creación con el MISMO nombre tiene que dar
        // ERROR_ALREADY_EXISTS: es justo lo que verá la segunda copia de la app
        // (mismo mecanismo, mismo mutex).
        assert!(
            !acquire(),
            "una segunda instancia no debe poder adquirir el guard"
        );
    }

    #[test]
    fn el_nombre_del_mutex_es_local_a_la_sesion() {
        // `Local\`: dos sesiones de usuario distintas no se pisan, pero dentro
        // de la misma sesión solo pasa una copia.
        assert!(MUTEX_NAME.starts_with("Local\\"));
        assert!(MUTEX_NAME.contains("WhisprOpen"));
    }
}
