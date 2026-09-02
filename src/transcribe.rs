//! Transcripción con whisper-rs (whisper.cpp nativo, modelos GGML).
//!
//! Los modelos se descargan de HuggingFace (repo ggerganov/whisper.cpp,
//! archivos ggml-*.bin) a `%APPDATA%/whispr-open/models/`. Cargar el contexto
//! es caro, así que se reutiliza un contexto por modelo mientras la app vive.

use std::path::PathBuf;
use std::sync::Mutex;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Tiny,
    Base,
    Small,
}

impl Model {
    pub fn id(&self) -> &'static str {
        match self {
            Model::Tiny => "tiny",
            Model::Base => "base",
            Model::Small => "small",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Model::Tiny => "Tiny (rápido)",
            Model::Base => "Base (equilibrado)",
            Model::Small => "Small (preciso)",
        }
    }

    pub fn file_name(&self) -> &'static str {
        match self {
            Model::Tiny => "ggml-tiny.bin",
            Model::Base => "ggml-base.bin",
            Model::Small => "ggml-small.bin",
        }
    }

    /// Tamaño aproximado en MB de la descarga.
    pub fn approx_mb(&self) -> u32 {
        match self {
            Model::Tiny => 75,
            Model::Base => 142,
            Model::Small => 466,
        }
    }

    pub fn all() -> [Model; 3] {
        [Model::Tiny, Model::Base, Model::Small]
    }

    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.file_name()
        )
    }
}

impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Directorio de modelos: %APPDATA%/whispr-open/models
pub fn models_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("whispr-open").join("models")
}

pub fn model_path(model: Model) -> PathBuf {
    models_dir().join(model.file_name())
}

pub fn model_exists(model: Model) -> bool {
    model_path(model).is_file()
}

/// Descarga el modelo si no existe. Devuelve la ruta lista para cargar.
pub fn ensure_model(model: Model, on_progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    let path = model_path(model);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(models_dir()).map_err(|e| format!("no se pudo crear models dir: {e}"))?;

    let url = model.download_url();
    crate::log!("transcribe", "descargando {url}");
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("no se pudo descargar el modelo: {e}"))?;

    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    // Descarga a archivo temporal y renombra al terminar (no deja modelos a medias).
    let tmp = path.with_extension("bin.part");
    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("no se pudo crear {tmp:?}: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    let mut reader = resp.into_reader();
    use std::io::Read;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("error de descarga: {e}"))?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buf[..n]).map_err(|e| format!("error escribiendo modelo: {e}"))?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    std::fs::rename(&tmp, &path).map_err(|e| format!("no se pudo finalizar la descarga: {e}"))?;
    Ok(path)
}

/// Contexto whisper reutilizable (un hilo a la vez).
pub struct WhisperEngine {
    inner: Mutex<WhisperContext>,
}

impl WhisperEngine {
    pub fn load(model: Model) -> Result<Self, String> {
        crate::log!("transcribe", "{}", whisper_rs::print_system_info());
        let path = model_path(model);
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(&path, params)
            .map_err(|e| format!("no se pudo cargar el modelo {}: {e}", model.id()))?;
        Ok(Self { inner: Mutex::new(ctx) })
    }

    /// Transcribe muestras f32 mono a texto. Bloquea mientras transcribe
    /// (llamado desde un hilo de trabajo, nunca desde la UI).
    pub fn transcribe(&self, samples: &[f32], language: &str) -> Result<String, String> {
        let ctx = self.inner.lock().map_err(|e| e.to_string())?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("no se pudo crear estado de transcripción: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_n_threads(4);

        state
            .full(params, samples)
            .map_err(|e| format!("error transcribiendo: {e}"))?;

        let num_segments = state.full_n_segments();
        let mut parts = Vec::with_capacity(num_segments as usize);
        for i in 0..num_segments {
            if let Some(seg) = state.get_segment(i) {
                if let Ok(t) = seg.to_str() {
                    let t = t.trim();
                    if !t.is_empty() {
                        parts.push(t.to_string());
                    }
                }
            }
        }
        Ok(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_metadata_is_consistent() {
        for m in Model::all() {
            assert_eq!(m.file_name(), &format!("ggml-{}.bin", m.id()));
            assert!(m.approx_mb() > 0);
            assert!(m.download_url().contains(m.file_name()));
        }
    }

    #[test]
    fn models_dir_lives_under_appdata() {
        let dir = models_dir();
        assert!(dir.to_string_lossy().contains("whispr-open"));
    }
}