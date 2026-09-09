//! Transcripción con whisper-rs (whisper.cpp nativo, modelos GGML).
//!
//! Los modelos se descargan de HuggingFace (repo ggerganov/whisper.cpp,
//! archivos ggml-*.bin) a `%APPDATA%/whispr-open/models/`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const BEAM_SIZE: i32 = 5;
pub const VAD_MODEL_FILE: &str = "ggml-vad-q8.bin";
const PROMPT_MAX_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        f.write_str(self.label())
    }
}

pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("whispr-open")
        .join("models")
}
pub fn model_path(model: Model) -> PathBuf {
    models_dir().join(model.file_name())
}
pub fn model_exists(model: Model) -> bool {
    model_path(model).is_file()
}
pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_MODEL_FILE)
}
pub fn vad_available_for(model_file: &Path) -> bool {
    model_file.is_file()
}
pub fn vad_available() -> bool {
    vad_available_for(&vad_model_path())
}

static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn ensure_model(model: Model, on_progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    let lock = DOWNLOAD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|e| format!("bloqueo de descarga envenenado: {e}"))?;
    let path = model_path(model);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(models_dir())
        .map_err(|e| format!("no se pudo crear models dir: {e}"))?;
    let resp = ureq::get(&model.download_url())
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|e| format!("no se pudo descargar el modelo: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let tmp = path.with_file_name(format!("{}.{}.part", model.file_name(), std::process::id()));
    let mut file =
        std::fs::File::create(&tmp).map_err(|e| format!("no se pudo crear {tmp:?}: {e}"))?;
    let mut downloaded = 0u64;
    let mut buf = [0u8; 64 * 1024];
    let mut reader = resp.into_reader();
    use std::io::{Read, Write};
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("error de descarga: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("error escribiendo modelo: {e}"))?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.sync_all()
        .map_err(|e| format!("error sincronizando modelo: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("no se pudo finalizar la descarga: {e}"))?;
    Ok(path)
}

pub fn build_initial_prompt(
    entries: &[crate::domain::dictionary::DictionaryEntry],
) -> Option<String> {
    let mut seen = HashSet::new();
    let mut out = String::new();
    for entry in entries {
        let term = entry.from.trim();
        if term.is_empty() || term.contains('\0') || !seen.insert(term.to_string()) {
            continue;
        }
        let separator = usize::from(!out.is_empty());
        if out.chars().count() + separator + term.chars().count() > PROMPT_MAX_CHARS {
            break;
        }
        if separator != 0 {
            out.push(' ');
        }
        out.push_str(term);
    }
    (!out.is_empty()).then_some(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamsDecision {
    pub strategy: &'static str,
    pub beam_size: i32,
    pub temperature: f32,
    pub n_threads: i32,
    pub no_timestamps: bool,
    pub language: String,
    pub initial_prompt: Option<String>,
    pub vad: bool,
}

#[derive(Debug, Clone)]
pub struct InferenceParams {
    pub language: Option<String>,
    pub beam_size: i32,
    pub n_threads: i32,
}
impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            language: Some("es".into()),
            beam_size: BEAM_SIZE,
            n_threads: 4,
        }
    }
}

fn build_params<'a>(
    inference: &'a InferenceParams,
    initial_prompt: Option<&str>,
    vad: bool,
) -> (FullParams<'a, 'static>, ParamsDecision) {
    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: inference.beam_size,
        patience: -1.0,
    });
    params.set_language(inference.language.as_deref());
    params.set_temperature(0.0);
    params.set_no_timestamps(true);
    params.set_n_threads(inference.n_threads);
    let prompt = initial_prompt
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.contains('\0'))
        .map(str::to_string);
    if let Some(p) = &prompt {
        params.set_initial_prompt(p);
    }
    if vad {
        let path = vad_model_path();
        params.set_vad_model_path(Some(&path.to_string_lossy()));
        params.enable_vad(true);
    }
    let decision = ParamsDecision {
        strategy: "beam_search",
        beam_size: inference.beam_size,
        temperature: 0.0,
        n_threads: inference.n_threads,
        no_timestamps: true,
        language: inference.language.clone().unwrap_or_else(|| "auto".into()),
        initial_prompt: prompt,
        vad,
    };
    (params, decision)
}

pub struct WhisperEngine {
    inner: Mutex<WhisperContext>,
}
impl WhisperEngine {
    pub fn load(model: Model) -> Result<Self, String> {
        crate::log!("transcribe", "{}", whisper_rs::print_system_info());
        let ctx = WhisperContext::new_with_params(
            &model_path(model),
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("no se pudo cargar el modelo {}: {e}", model.id()))?;
        Ok(Self {
            inner: Mutex::new(ctx),
        })
    }
    pub fn transcribe(
        &self,
        samples: &[f32],
        inference: &InferenceParams,
        initial_prompt: Option<&str>,
    ) -> Result<String, String> {
        let ctx = self.inner.lock().map_err(|e| e.to_string())?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("no se pudo crear estado de transcripción: {e}"))?;
        let (params, decision) = build_params(inference, initial_prompt, vad_available());
        crate::log!("transcribe", "{decision:?}");
        state
            .full(params, samples)
            .map_err(|e| format!("error transcribiendo: {e}"))?;
        let count = state.full_n_segments();
        let mut parts = Vec::with_capacity(count as usize);
        for i in 0..count {
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

/// Carga el contexto del modelo en el cache SIN transcribir (prerroll de
/// arranque, requisito UX 09/09/2026). La primera pulsación del atajo deja
/// de pagar la carga del contexto (visible como 1-2 s de "Transcribiendo…"
/// con el modelo aún en disco): con esto la carga ocurre en background
/// mientras el usuario ve la pantalla de carga. Es idempotente y comparte
/// el mismo `ENGINE_CACHE` que `transcribe_cached` (cambiar de modelo aquí
/// o en transcripción reemplaza el contexto de forma segura).
pub fn preload(model: Model) -> Result<(), String> {
    let cache = ENGINE_CACHE.get_or_init(|| Mutex::new(None));
    let mut slot = cache.lock().map_err(|e| e.to_string())?;
    if slot
        .as_ref()
        .map(|(cached, _)| *cached != model)
        .unwrap_or(true)
    {
        crate::log!("transcribe", "prerroll: cargando contexto {}…", model.id());
        *slot = Some((model, Arc::new(WhisperEngine::load(model)?)));
    }
    crate::log!(
        "transcribe",
        "prerroll: modelo {} listo en cache",
        model.id()
    );
    Ok(())
}

static ENGINE_CACHE: OnceLock<Mutex<Option<(Model, Arc<WhisperEngine>)>>> = OnceLock::new();

/// Transcribe reutilizando el contexto cargado para el modelo activo. Solo se
/// conserva un modelo para limitar memoria; cambiar de modelo reemplaza el
/// contexto anterior de forma segura cuando no hay otro turno en curso.
pub fn transcribe_cached(
    samples: &[f32],
    model: Model,
    inference: &InferenceParams,
    prompt: Option<&str>,
) -> Result<String, String> {
    let cache = ENGINE_CACHE.get_or_init(|| Mutex::new(None));
    let engine = {
        let mut slot = cache.lock().map_err(|e| e.to_string())?;
        if slot
            .as_ref()
            .map(|(cached, _)| *cached != model)
            .unwrap_or(true)
        {
            *slot = Some((model, Arc::new(WhisperEngine::load(model)?)));
        }
        slot.as_ref().expect("cache recién inicializada").1.clone()
    };
    engine.transcribe(samples, inference, prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dictionary::DictionaryEntry;
    fn entry(from: &str) -> DictionaryEntry {
        DictionaryEntry {
            from: from.into(),
            to: from.into(),
        }
    }
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
        assert!(models_dir().to_string_lossy().contains("whispr-open"));
    }
    #[test]
    fn vad_model_lives_in_models_dir() {
        assert!(vad_model_path().to_string_lossy().contains("whispr-open"));
        assert!(vad_model_path().ends_with(VAD_MODEL_FILE));
    }
    #[test]
    fn vad_available_for_checks_file_existence() {
        let dir = std::env::temp_dir().join(format!("whispr-vad-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let missing = dir.join("ggml-vad-q8.bin");
        assert!(!vad_available_for(&missing));
        std::fs::write(&missing, b"fake").unwrap();
        assert!(vad_available_for(&missing));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn prompt_none_without_entries() {
        assert_eq!(build_initial_prompt(&[]), None);
        assert_eq!(build_initial_prompt(&[entry("   ")]), None);
    }
    #[test]
    fn prompt_joins_spoken_terms() {
        let p = build_initial_prompt(&[entry("whatsapp"), entry("openai"), entry("sape")]).unwrap();
        assert_eq!(p, "whatsapp openai sape");
    }
    #[test]
    fn prompt_dedupes_terms_and_skips_nulls() {
        let p =
            build_initial_prompt(&[entry("whatsapp"), entry("whatsapp"), entry("a\0b")]).unwrap();
        assert_eq!(p, "whatsapp");
    }
    #[test]
    fn prompt_is_capped_by_chars() {
        let p = build_initial_prompt(
            &(0..200)
                .map(|i| entry(&format!("término-{i}")))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(p.chars().count() <= PROMPT_MAX_CHARS);
        assert!(p.starts_with("término-0"));
    }
    #[test]
    fn prompt_never_splits_a_multibyte_char() {
        let p = build_initial_prompt(
            &(0..100)
                .map(|i| entry(&format!("é-{i}")))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(p.chars().count() <= PROMPT_MAX_CHARS);
        for word in p.split(' ') {
            assert!(word
                .strip_prefix("é-")
                .and_then(|n| n.parse::<u32>().ok())
                .is_some());
        }
    }
}
