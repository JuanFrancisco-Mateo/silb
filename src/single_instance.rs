//! Guard de instancia única (requisito de robustez): una segunda ejecución
//! de `silb.exe` NO debe robar ni duplicar el atajo global. Sin este
//! guard, el `RegisterHotKey` de la segunda instancia fallaría a ciegas
//! (Windows no permite dos registros del mismo atajo), la segunda bandeja
//! se acumularía en el tray y el usuario vería dos burbujas.
//!
//! Mecanismo: mutex nombrado de Windows (`CreateMutexW`). El nombre vive en
//! el namespace `Local\` (por sesión de usuario): dos sesiones de escritorio
//! distintas pueden tener su instancia —que es lo correcto para una app de
//! usuario— pero dentro de la misma sesión solo pasa la primera.

use std::sync::OnceLock;

/// Namespace `Local\` = por sesión de usuario iniciada (no global a la
/// máquina). Identidad fija de la app: cualquier proceso que la conozca ve
/// el mismo mutex.
pub const MUTEX_NAME: &str = "Local\\WhisprOpen-SingleInstance";

/// `GetLastError` de un `CreateMutexW` sobre un mutex que ya existía.
const ERROR_ALREADY_EXISTS: u32 = 183;

/// Handle del mutex, vivo mientras viva el proceso (si se cerrara, otra
/// instancia podría colarse). Se guarda como `usize` porque un HANDLE
/// (puntero crudo) no es `Send`/`Sync` y los statics lo exigen.
static MUTEX: OnceLock<usize> = OnceLock::new();

/// Intenta adquirir el guard. `true` = esta ejecución es la primera (se
/// queda con el mutex); `false` = ya hay otra instancia y esta debe salir
/// sin registrar atajo, bandeja ni burbuja.
///
/// Si el propio mutex no pudiera crearse (memoria agotada, …) se devuelve
/// `true`: mejor una app sin guard que una app que no arranca por su
/// mecanismo anti-duplicado (el atajo ya degrada a stub por su cuenta si
/// el registro falla).
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

/// Stub fuera de Windows: sin mutex con nombre no hay guard de instancia
/// única; `true` (esta instancia es la dueña) para no bloquear el arranque.
#[cfg(not(target_os = "windows"))]
pub fn acquire() -> bool {
    true
}

/// Aviso nativo para quien lanzó la segunda instancia con un doble clic y
/// no ve aparecer nada: la app que ya corría sigue en la bandeja.
#[cfg(target_os = "windows")]
pub fn notify_second_instance() {
    // MessageBoxW espera cadenas UTF-16 (wide): se terminan en NUL.
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

/// Stub fuera de Windows (el aviso real es un `MessageBoxW`).
#[cfg(not(target_os = "windows"))]
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
        // Namespace Local\: dos sesiones de usuario distintas no se pisan,
        // pero dentro de la misma sesión solo pasa una instancia.
        assert!(MUTEX_NAME.starts_with("Local\\"));
        assert!(MUTEX_NAME.contains("WhisprOpen"));
    }
}
