//! Atajo global de dictado (toggle), configurable: `shortcut` en config.json
//! (p.ej. "Ctrl+Shift+D", "Alt+Space", "Ctrl+Alt+F12"); default Ctrl+Shift+D.
//! Usa el crate `global-hotkey` (registro en el sistema), igual que hacía el
//! plugin de Tauri. Los eventos llegan por canal: `HotkeyManager::listen`.
//!
//! Si el valor de config es inválido o el registro falla (atajo ya ocupado
//! por otra app), se vuelve al default / modo stub: los botones de la UI
//! siguen funcionando, simplemente no hay atajo global.

use crossbeam_channel::Receiver;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

/// Atajo por defecto (también es el valor de `shortcut` ausente en config).
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+D";

/// Atajo global registrado en el sistema. Debe permanecer vivo mientras viva
/// la app: si se cae, el atajo se desregistra. Si el registro falla se usa
/// `stub`, de modo que los botones de la UI siguen funcionando sin atajo.
pub struct HotkeyManager {
    // Se mantiene vivo mientras la app viva: si se cae, el atajo se desregistra.
    #[allow(dead_code)]
    manager: Option<GlobalHotKeyManager>,
    /// Atajo parseado y registrado: su id filtra los eventos del canal y su
    /// etiqueta canónica alimenta la UI ("Atajo global: …").
    hotkey: HotKey,
}

impl HotkeyManager {
    /// Registra `shortcut` en el sistema. `Err` con mensaje en español si el
    /// formato no es válido o el atajo ya lo tiene otra app (el llamador
    /// degrada a `stub` en vez de abortar el arranque).
    pub fn register(shortcut: &str) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("no se pudo iniciar el gestor de atajos: {e}"))?;

        let hotkey = parse_shortcut(shortcut);
        manager
            .register(hotkey)
            .map_err(|e| format!("no se pudo registrar {}: {e}", hotkey_label(hotkey)))?;

        Ok(Self {
            manager: Some(manager),
            hotkey,
        })
    }

    /// Stub para cuando el registro falla: siguen funcionando los botones de
    /// la UI, simplemente no hay atajo global.
    pub fn stub(shortcut: &str) -> Self {
        Self {
            manager: None,
            hotkey: parse_shortcut(shortcut),
        }
    }

    /// Canal de eventos global. El primer `GlobalHotKeyEvent::received` de cada
    /// press es `Pressed`; la UI escucha, filtra por [`Self::id`] y hace toggle.
    pub fn listen(&self) -> Receiver<GlobalHotKeyEvent> {
        GlobalHotKeyEvent::receiver().clone()
    }

    /// Id del atajo registrado: solo los eventos con este id son de nuestro
    /// atajo (cualquier otro id del canal se ignora).
    pub fn id(&self) -> u32 {
        self.hotkey.id()
    }

    /// Etiqueta canónica para la UI (p.ej. "Ctrl+Shift+D").
    pub fn label(&self) -> String {
        hotkey_label(self.hotkey)
    }
}

/// Parsea el atajo de config.json en un [`HotKey`] de global-hotkey. Valores
/// inválidos (o en blanco) caen al default: la config nunca rompe el arranque.
fn parse_shortcut(shortcut: &str) -> HotKey {
    let trimmed = shortcut.trim();
    match trimmed.parse::<HotKey>() {
        Ok(h) => h,
        Err(e) => {
            crate::log!(
                "hotkey",
                "atajo «{trimmed}» no válido ({e}); usando el default {DEFAULT_SHORTCUT}"
            );
            default_hotkey()
        }
    }
}

fn default_hotkey() -> HotKey {
    // Fuente de verdad: la constante de arriba (validada por tests). El
    // parse de una constante no puede fallar en la práctica, pero el
    // requisito de robustez veta pánicos en el arranque: si una
    // actualización del crate cambiara el formato aceptado, se construye
    // el MISMO atajo por API (infalible) en vez de tumbar el proceso.
    DEFAULT_SHORTCUT.parse::<HotKey>().unwrap_or_else(|_| {
        crate::log!(
            "hotkey",
            "el default «{DEFAULT_SHORTCUT}» dejó de parsear; se construye por API"
        );
        HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD)
    })
}

/// Etiqueta canónica Windows-style («Ctrl+Shift+D») a partir del atajo
/// parseado, independiente del orden/estilo que el usuario escribió en config.
fn hotkey_label(h: HotKey) -> String {
    let mut parts: Vec<String> = Vec::new();
    if h.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if h.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if h.mods.contains(Modifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if h.mods.contains(Modifiers::SUPER) {
        parts.push("Win".to_string());
    }
    parts.push(key_label(h.key));
    parts.join("+")
}

/// Nombre corto de tecla: `Code::Display` da "KeyD"/"Digit5"/"Space"/"F12".
fn key_label(code: Code) -> String {
    let s = code.to_string();
    if let Some(rest) = s.strip_prefix("Key") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("Digit") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("Numpad") {
        return format!("Num{rest}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_es_ctrl_shift_d() {
        let h = default_hotkey();
        assert_eq!(h.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(h.key, Code::KeyD);
        assert_eq!(hotkey_label(h), "Ctrl+Shift+D");
    }

    #[test]
    fn atajos_personalizados_se_parsean_y_etiquetan() {
        assert_eq!(hotkey_label(parse_shortcut("Alt+Space")), "Alt+Space");
        assert_eq!(hotkey_label(parse_shortcut("Ctrl+Alt+F12")), "Ctrl+Alt+F12");
        assert_eq!(hotkey_label(parse_shortcut("shift+KeyK")), "Shift+K");
        // Orden canónico fijo: Ctrl+Shift+Alt+Win, da igual el orden de entrada.
        assert_eq!(
            hotkey_label(parse_shortcut("Ctrl+Alt+Shift+Space")),
            "Ctrl+Shift+Alt+Space"
        );
        // Espacios alrededor no rompen el parseo.
        assert_eq!(
            hotkey_label(parse_shortcut("  Ctrl+Shift+D  ")),
            "Ctrl+Shift+D"
        );
    }

    #[test]
    fn invalido_cae_al_default() {
        // Sin tecla principal, token vacío, tecla inexistente.
        assert_eq!(hotkey_label(parse_shortcut("Ctrl+Shift+")), "Ctrl+Shift+D");
        assert_eq!(hotkey_label(parse_shortcut("")), "Ctrl+Shift+D");
        assert_eq!(hotkey_label(parse_shortcut("zzz")), "Ctrl+Shift+D");
    }

    #[test]
    fn id_determinista_por_atajo() {
        let a = parse_shortcut("Ctrl+Shift+D");
        let b = parse_shortcut("Ctrl+Shift+D");
        assert_eq!(a.id(), b.id());
        // Distinto atajo → distinto id: los eventos se filtran por él.
        assert_ne!(a.id(), parse_shortcut("Alt+Space").id());
    }

    #[test]
    fn fallback_por_api_es_el_mismo_atajo_que_el_default_parseado() {
        // Requisito de robustez: default_hotkey() ya no puede pánico. Si el
        // parse de la constante llegara a romperse (cambio de crate), la
        // construcción por API debe producir EXACTAMENTE el mismo atajo
        // (misma etiqueta y mismo id) para que la UI y el filtrado de
        // eventos no cambien de comportamiento.
        let por_api = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
        let por_parse = DEFAULT_SHORTCUT.parse::<HotKey>().unwrap();
        assert_eq!(por_api.id(), por_parse.id());
        assert_eq!(hotkey_label(por_api), hotkey_label(por_parse));
    }

    #[test]
    fn label_del_manager_refleja_lo_registrado() {
        let stub = HotkeyManager::stub("ctrl+alt+d");
        assert_eq!(stub.id(), parse_shortcut("ctrl+alt+d").id());
        assert_eq!(stub.label(), "Ctrl+Alt+D");
    }
}
