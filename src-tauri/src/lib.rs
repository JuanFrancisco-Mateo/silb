use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};

const DICTATION_SHORTCUT: &str = "CommandOrControl+Shift+D";

/// Escribe `text` en la aplicación activa (la que tenga el foco) copiándolo al
/// portapapeles y simulando Ctrl+V. Solo disponible en Windows; en el resto de
/// plataformas devuelve un error controlado.
#[tauri::command]
fn type_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        paste_text_on_windows(&text)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        Err("type_text solo está disponible en Windows".to_string())
    }
}

#[cfg(target_os = "windows")]
fn paste_text_on_windows(text: &str) -> Result<(), String> {
    use std::time::Duration;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![type_text])
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let is_dictation_shortcut = if cfg!(target_os = "macos") {
                        shortcut.matches(Modifiers::SUPER | Modifiers::SHIFT, Code::KeyD)
                    } else {
                        shortcut.matches(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyD)
                    };

                    if is_dictation_shortcut && event.state() == ShortcutState::Pressed
                    {
                        let _ = app.emit("global-dictation-toggle", ());
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            if let Err(error) = app.global_shortcut().register(DICTATION_SHORTCUT) {
                eprintln!("No se pudo registrar el atajo global {DICTATION_SHORTCUT}: {error}");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Whispr Open");
}
