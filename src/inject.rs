//! Inyección de texto en la aplicación activa (la que tiene el foco).
//! Port del command `type_text` de la versión Tauri: copia al portapapeles
//! con arboard y simula Ctrl+V con enigo. Solo Windows.

use std::time::Duration;

#[cfg(target_os = "windows")]
pub fn inject_text_into_active_app(text: &str) -> Result<(), String> {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut clipboard = Clipboard::new()
        .map_err(|err| format!("no se pudo abrir el portapapeles: {err}"))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|err| format!("no se pudo copiar el texto al portapapeles: {err}"))?;

    // Pequeña pausa para que el portapapeles esté listo antes del pegado.
    std::thread::sleep(Duration::from_millis(40));

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|err| format!("no se pudo inicializar el simulador de teclado: {err}"))?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|err| format!("no se pudo simular la pulsación de Ctrl: {err}"))?;
    enigo
        .key(Key::V, Direction::Click)
        .map_err(|err| format!("no se pudo simular Ctrl+V: {err}"))?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|err| format!("no se pudo soltar Ctrl: {err}"))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn inject_text_into_active_app(_text: &str) -> Result<(), String> {
    Err("la inyección de texto solo está disponible en Windows".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_exists() {
        // Solo verifica que la firma pública está disponible en todas las plataformas.
        let _ = super::inject_text_into_active_app;
    }
}