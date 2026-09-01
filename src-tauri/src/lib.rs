use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};

const DICTATION_SHORTCUT: &str = "CommandOrControl+Shift+D";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
