//! Atajo global Ctrl+Shift+D (toggle de dictado).
//! Usa el crate `global-hotkey` (registro en el sistema), igual que hacía el
//! plugin de Tauri. Los eventos llegan por canal: `HotkeyManager::listen`.

use crossbeam_channel::Receiver;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

pub const DICTATION_SHORTCUT_LABEL: &str = "Ctrl+Shift+D";

pub struct HotkeyManager {
    // Se mantiene vivo mientras la app viva: si se cae, el atajo se desregistra.
    #[allow(dead_code)]
    manager: Option<GlobalHotKeyManager>,
}

impl HotkeyManager {
    pub fn register() -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("no se pudo iniciar el gestor de atajos: {e}"))?;

        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
        manager
            .register(hotkey)
            .map_err(|e| format!("no se pudo registrar Ctrl+Shift+D: {e}"))?;

        Ok(Self { manager: Some(manager) })
    }

    /// Stub para cuando el registro falla: siguen funcionando los botones de
    /// la UI, simplemente no hay atajo global.
    pub fn stub() -> Self {
        Self { manager: None }
    }

    /// Canal de eventos global. El primer `GlobalHotKeyEvent::received` de cada
    /// press es `Pressed`; la UI escucha y hace toggle.
    pub fn listen(&self) -> Receiver<GlobalHotKeyEvent> {
        GlobalHotKeyEvent::receiver().clone()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shortcut_label_matches() {
        assert_eq!(super::DICTATION_SHORTCUT_LABEL, "Ctrl+Shift+D");
    }
}