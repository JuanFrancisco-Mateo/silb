//! Config persistente de la app (reemplaza localStorage de la versión web).
//! Archivo JSON en %APPDATA%/whispr-open/config.json: diccionario, modelo
//! seleccionado y preferencias. Correcciones best-effort: nunca rompe el arranque.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::dictionary::DictionaryEntry;
use crate::transcribe::Model;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub dictionary: Vec<DictionaryEntry>,
    pub model: Option<String>,
    pub cleanup_enabled: Option<bool>,
    pub auto_inject: Option<bool>,
}

pub fn config_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("config.json")
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, raw);
        }
    }

    pub fn selected_model(&self) -> Model {
        match self.model.as_deref() {
            Some("base") => Model::Base,
            Some("small") => Model::Small,
            _ => Model::Tiny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let cfg = Config::default();
        assert_eq!(cfg.selected_model(), Model::Tiny);
        assert!(cfg.dictionary.is_empty());
        assert_eq!(cfg.cleanup_enabled, None);
    }

    #[test]
    fn config_path_lives_under_appdata() {
        assert!(config_path().to_string_lossy().contains("whispr-open"));
    }

    #[test]
    fn invalid_model_falls_back_to_tiny() {
        let cfg = Config { model: Some("gigante".into()), ..Default::default() };
        assert_eq!(cfg.selected_model(), Model::Tiny);
    }
}