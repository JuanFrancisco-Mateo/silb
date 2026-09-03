//! Transcripción con whisper-rs (whisper.cpp nativo, modelos GGML).
//!
//! Los modelos se descargan de HuggingFace (repo ggerganov/whisper.cpp,
//! archivos ggml-*.bin) a `%APPDATA%/whispr-open/models/`. Cargar el contexto
//! es caro, así que se reutiliza un contexto por modelo mientras la app vive.
//!
//! ## Parámetros de inferencia (verificados contra whisper-rs 0.16.0 y el
//! whisper.cpp 1.8.3 vendido en whisper-rs-sys 0.15.0, no de memoria)
//!
//! | Parámetro   | Antes              | Ahora        | Por qué |
//! |-------------|--------------------|--------------|---------|
//! | estrategia  | Greedy best_of=1   | BeamSearch 5 | beam search explora 5 hipótesis en paralelo; gana precisión en dictado. CUDA + small lo mantiene en tiempo real (mide el bench `bench_greedy_vs_beam`). |
//! | temperature | 0.0 (default)      | 0.0 explícito| determinista: en dictado no interesa muestreo aleatorio |
//! | language    | "es" desde app.rs  | "es" (igual) | forzado a español |
//! | timestamps  | calculados         | off          | la app junta segmentos en texto plano y nunca usa marcas de tiempo: `no_timestamps=true` evita tokens de timestamp contaminando la transcripción. (En el bench NO se midió ganancia de latencia: el decode está dominado por la expansión del beam en GPU.) |
//! | fp16        | —                  | — (ya on)    | NO es un parámetro expuesto por whisper-rs/whisper.cpp: `whisper_context_params` solo tiene use_gpu/flash_attn/gpu_device/dtw_*. Con el feature `cuda` (use_gpu por defecto) el backend ggml-cuda convierte los pesos a fp16 en la GPU automáticamente (ggml-cuda.cu, `ggml_mul_mat`). |
//! | VAD         | off                | auto         | whisper.cpp 1.8.3 soporta VAD en `whisper_full` con un modelo aparte (`ggml-vad-q8.bin`). Se activa solo si el modelo está en el dir de modelos; si falta, off (push-to-talk ya acota el audio). |
//! | initial_prompt | sin sesgo      | diccionario  | el diccionario del usuario (jerga, nombres, siglas) se inyecta como prompt inicial: sesga el reconocedor hacia esos términos ANTES de transcribir. |
//!
//! Nota VAD (API real): `FullParams::enable_vad(true)` hace PANIC si el path
//! del modelo no está puesto antes (`set_vad_model_path` primero), y
//! `set_vad_model_path(None)` desactiva VAD. Por eso el orden aquí es
//! path → enable, y solo cuando `vad_available()` confirma el archivo.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Beam size preferido por Juanfri: máxima calidad sobre velocidad.
/// Coincide con el default de whisper.cpp para beam search (5).
pub const BEAM_SIZE: i32 = 5;

/// Modelo VAD de whisper.cpp (Silero convertido a GGML). La app lo usa solo
/// si el archivo existe en el dir de modelos (descarga manual desde
/// https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-vad-q8.bin).
pub const VAD_MODEL_FILE: &str = "ggml-vad-q8.bin";

/// Tope de caracteres del initial_prompt (suficiente para sesgar léxico;
/// whisper lo tokeniza como contexto inicial, no como instrucción).
const PROMPT_MAX_CHARS: usize = 256;

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

/// Ruta del modelo VAD (silero GGML). No se descarga automáticamente: el VAD
/// interno de whisper_full es opcional y se activa solo si el archivo está.
pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_MODEL_FILE)
}

pub fn vad_available_for(model_file: &Path) -> bool {
    model_file.is_file()
}

/// ¿Está el modelo VAD en el dir de modelos? Si sí, `build_params` activa
/// `params.vad` en whisper_full (recorta silencio/ruido de los bordes).
pub fn vad_available() -> bool {
    vad_available_for(&vad_model_path())
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

/// Builda el initial_prompt desde el diccionario del usuario: los términos
/// hablados (`from`) como sesgo léxico para el reconocedor. Sin duplicados,
/// sin nulos, acotado a `PROMPT_MAX_CHARS` caracteres.
pub fn build_initial_prompt(entries: &[crate::domain::dictionary::DictionaryEntry]) -> Option<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = String::new();
    for entry in entries {
        let term = entry.from.trim();
        if term.is_empty() || term.contains('\0') {
            continue;
        }
        if term.chars().count() > PROMPT_MAX_CHARS.saturating_sub(out.chars().count()) {
            break; // sin espacio para este término entero
        }
        if !seen.insert(term.to_string()) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(term);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Decisión de parámetros que se aplicó, para logs y tests. Los campos del
/// `FullParams` real son `pub(crate)` en whisper-rs, así que la decisión se
/// refleja aquí (misma función que los construye ⇒ no puede divergir).
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

/// Parámetros de inferencia configurables por el usuario. Los defaults
/// coinciden con los valores fijos que la app usaba antes de exponerlos
/// en Ajustes (idioma es, beam 5, threads 4, remuestreo 16 k on).
/// `vad` lo decide el llamador (normalmente `vad_available()`); no es un
/// parámetro de usuario.
#[derive(Debug, Clone)]
pub struct InferenceParams {
    /// Idioma BCP-47 (`Some("es")`, `Some("en")`, …) o `None` para
    /// autodetección (whisper decide).
    pub language: Option<String>,
    /// Beam size para `SamplingStrategy::BeamSearch`. 1..=8; 1 = greedy.
    pub beam_size: i32,
    /// Hilos del decoder.
    pub n_threads: i32,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            language: Some("es".to_string()),
            beam_size: BEAM_SIZE,
            n_threads: 4,
        }
    }
}

/// Parámetros de inferencia de la app (ver tabla en el header del módulo).
/// `vad` lo decide el llamador (normalmente `vad_available()`).
fn build_params<'a>(
    inference: &'a InferenceParams,
    initial_prompt: Option<&str>,
    vad: bool,
) -> (FullParams<'a, 'static>, ParamsDecision) {
    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: inference.beam_size,
        // -1.0 = default de whisper.cpp 1.8.3; el campo es legacy (nunca se
        // lee en el decodificador de esta versión, verificado en la fuente).
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

    // VAD: path primero, enable después (enable_vad panica si el path es
    // null, API verificada en whisper-rs 0.16.0).
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
        language: inference.language.clone().unwrap_or_else(|| "auto".to_string()),
        initial_prompt: prompt,
        vad,
    };
    (params, decision)
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
    ///
    /// `inference` agrupa los parámetros ajustables por el usuario
    /// (idioma, beam size, threads); `initial_prompt` es el sesgo léxico
    /// desde el diccionario personal.
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
    use crate::domain::dictionary::DictionaryEntry;

    fn entry(from: &str) -> DictionaryEntry {
        DictionaryEntry { from: from.into(), to: from.into() }
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
        let dir = models_dir();
        assert!(dir.to_string_lossy().contains("whispr-open"));
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
        let entries = vec![entry("whatsapp"), entry("openai"), entry("sape")];
        let p = build_initial_prompt(&entries).unwrap();
        assert_eq!(p, "whatsapp openai sape");
    }

    #[test]
    fn prompt_dedupes_terms_and_skips_nulls() {
        let entries = vec![entry("whatsapp"), entry("whatsapp"), entry("a\0b")];
        let p = build_initial_prompt(&entries).unwrap();
        assert_eq!(p, "whatsapp");
    }

    #[test]
    fn prompt_is_capped_by_chars() {
        let entries: Vec<DictionaryEntry> = (0..200)
            .map(|i| entry(&format!("término-{i}")))
            .collect();
        let p = build_initial_prompt(&entries).unwrap();
        assert!(p.chars().count() <= PROMPT_MAX_CHARS);
        assert!(p.starts_with("término-0"));
    }

    #[test]
    fn prompt_never_splits_a_multibyte_char() {
        // "é" ocupa 2 bytes: recortar por bytes partiría el carácter. El
        // builder solo admite términos enteros y nunca trunca a mitad.
        let entries: Vec<DictionaryEntry> =
            (0..100).map(|i| entry(&format!("é-{i}"))).collect();
        let p = build_initial_prompt(&entries).unwrap();
        assert!(p.chars().count() <= PROMPT_MAX_CHARS);
        for word in p.split(' ') {
            if let Some(num) = word.strip_prefix("é-") {
                assert!(num.parse::<u32>().is_ok(), "término partido: {word:?}");
            } else {
                panic!("término extraño en el prompt: {word:?}");
            }
        }
    }

    #[test]
    fn decision_uses_beam_five_and_no_timestamps() {
        let inference = InferenceParams::default();
        let (_, d) = build_params(&inference, None, false);
        assert_eq!(d.strategy, "beam_search");
        assert_eq!(d.beam_size, 5);
        assert_eq!(d.temperature, 0.0);
        assert!(d.no_timestamps);
        assert_eq!(d.language, "es");
        assert_eq!(d.initial_prompt, None);
        assert!(!d.vad);
    }

    #[test]
    fn decision_carries_prompt_and_vad() {
        let inference = InferenceParams::default();
        let (_, d) = build_params(&inference, Some("  openai sape  "), true);
        assert_eq!(d.language, "es");
        assert_eq!(d.initial_prompt.as_deref(), Some("openai sape"));
        assert!(d.vad);
        // el prompt con nulo se descarta para no llegar al panic de
        // set_initial_prompt (CString::new)
        let (_, d2) = build_params(&inference, Some("a\0b"), false);
        assert_eq!(d2.initial_prompt, None);
    }

    #[test]
    fn decision_vad_path_set_before_enable() {
        // el orden path→enable es lo que evita el panic de whisper-rs;
        // build_params(vad=true) no debe paniquear aunque el archivo no exista
        let inference = InferenceParams::default();
        let (_, d) = build_params(&inference, None, true);
        assert!(d.vad);
    }

    #[test]
    fn decision_refleja_beam_y_threads_de_inference() {
        // Configuración alternativa del usuario: beam 3, threads 8, en inglés.
        let inference = InferenceParams {
            language: Some("en".into()),
            beam_size: 3,
            n_threads: 8,
        };
        let (_, d) = build_params(&inference, None, false);
        assert_eq!(d.beam_size, 3);
        assert_eq!(d.n_threads, 8);
        assert_eq!(d.language, "en");
    }

    #[test]
    fn decision_con_language_none_reporta_auto() {
        // language: None → autodetección de whisper; el ParamsDecision
        // registra "auto" para diagnóstico legible en logs (whisper
        // internamente no recibe set_language).
        let inference = InferenceParams {
            language: None,
            beam_size: 5,
            n_threads: 4,
        };
        let (_, d) = build_params(&inference, None, false);
        assert_eq!(d.language, "auto");
    }

    #[test]
    fn inference_params_default_es_es_beam_5_threads_4() {
        // Defaults alineados con la conducta previa (idioma es, beam 5,
        // threads 4). Si cambian, revisar compat hacia atrás en Config.
        let p = InferenceParams::default();
        assert_eq!(p.language.as_deref(), Some("es"));
        assert_eq!(p.beam_size, 5);
        assert_eq!(p.n_threads, 4);
    }

    // ------------------------------------------------------------------
    // Bench de latencia: compara el parámetro antiguo (greedy best_of=1 +
    // timestamps) contra el nuevo (beam 5 sin timestamps) sobre el modelo
    // small real y el clip canónico jfk.wav de whisper.cpp.
    //
    // Correr a mano (requiere modelo small descargado + red la 1ª vez):
    //   cargo.exe test --release transcribe::tests::bench -- --ignored --nocapture
    // ------------------------------------------------------------------
    #[test]
    #[ignore = "medición de latencia real: requiere modelo small + red"]
    fn bench_greedy_vs_beam() {
        use std::time::{Duration, Instant};

        let model = Model::Small;
        let path = model_path(model);
        if !path.is_file() {
            println!("[SKIP] modelo small no encontrado en {path:?}");
            return;
        }
        let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
            .expect("cargar contexto small");
        println!("{}", whisper_rs::print_system_info());

        // jfk.wav: clip de referencia de whisper.cpp (PCM16 16kHz mono).
        let wav_path = Path::new("target/bench-audio/jfk.wav");
        let wav_bytes = if wav_path.is_file() {
            std::fs::read(wav_path).unwrap()
        } else {
            let url = "https://github.com/ggerganov/whisper.cpp/raw/master/samples/jfk.wav";
            println!("descargando {url}");
            match ureq::get(url).call().map(|r| r.into_reader()) {
                Ok(mut reader) => {
                    use std::io::Read;
                    let mut buf = Vec::new();
                    if reader.read_to_end(&mut buf).is_err() {
                        println!("[SKIP] falló la descarga de la muestra");
                        return;
                    }
                    if let Some(parent) = wav_path.parent() {
                        std::fs::create_dir_all(parent).unwrap();
                    }
                    std::fs::write(wav_path, &buf).unwrap();
                    buf
                }
                Err(e) => {
                    println!("[SKIP] sin red para la muestra ({e})");
                    return;
                }
            }
        };
        let Some(samples) = parse_wav_f32(&wav_bytes) else {
            println!("[SKIP] jfk.wav no es PCM16 mono 16kHz");
            return;
        };
        let secs = samples.len() as f64 / 16_000.0;
        println!("muestra: {secs:.1}s de audio, {} muestras", samples.len());

        fn run_once(
            ctx: &WhisperContext,
            make: &mut dyn FnMut() -> FullParams<'static, 'static>,
            samples: &[f32],
        ) -> (Duration, String) {
            let t = Instant::now();
            let mut state = ctx.create_state().expect("create_state");
            state.full(make(), samples).expect("full");
            let elapsed = t.elapsed();
            let mut parts: Vec<String> = vec![];
            for i in 0..state.full_n_segments() {
                if let Some(seg) = state.get_segment(i) {
                    if let Ok(s) = seg.to_str() {
                        let s = s.trim();
                        if !s.is_empty() {
                            parts.push(s.to_string());
                        }
                    }
                }
            }
            (elapsed, parts.join(" "))
        }

        fn measure(
            ctx: &WhisperContext,
            label: &str,
            lang: &'static str,
            make: &mut dyn FnMut() -> FullParams<'static, 'static>,
            samples: &[f32],
        ) -> (f32, String) {
            let _ = run_once(ctx, &mut *make, samples); // warmup
            let runs = 3;
            let mut total = Duration::ZERO;
            let mut text = String::new();
            for _ in 0..runs {
                let (d, t) = run_once(ctx, &mut *make, samples);
                total += d;
                text = t;
            }
            let avg_ms = total.as_secs_f32() * 1000.0 / runs as f32;
            println!("  {label:>34} lang={lang}: {avg_ms:7.0} ms prom  -> {text}");
            (avg_ms, text)
        }

        for lang in ["es", "en"] {
            println!("== lenguaje '{lang}' ==");
            let mut old = move || {
                let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
                p.set_language(Some(lang));
                p.set_n_threads(4);
                p
            };
            let (old_ms, old_text) =
                measure(&ctx, "antiguo: greedy best_of=1 + ts", lang, &mut old, &samples);

            let mut novo = move || {
                let mut p = FullParams::new(SamplingStrategy::BeamSearch {
                    beam_size: 5,
                    patience: -1.0,
                });
                p.set_language(Some(lang));
                p.set_temperature(0.0);
                p.set_no_timestamps(true);
                p.set_n_threads(4);
                p
            };
            let (new_ms, new_text) =
                measure(&ctx, "nuevo: beam 5 sin timestamps", lang, &mut novo, &samples);

            let mut beam_ts = move || {
                let mut p = FullParams::new(SamplingStrategy::BeamSearch {
                    beam_size: 5,
                    patience: -1.0,
                });
                p.set_language(Some(lang));
                p.set_n_threads(4);
                p
            };
            let (beam_ts_ms, _) =
                measure(&ctx, "beam 5 CON timestamps", lang, &mut beam_ts, &samples);

            println!(
                "  resumen {lang}: antiguo {old_ms:.0} ms | nuevo {new_ms:.0} ms | beam+ts {beam_ts_ms:.0} ms"
            );
            if lang == "en" {
                println!("  texto antiguo: {old_text}");
                println!("  texto nuevo:   {new_text}");
            }
        }
        println!("[OK] bench completo");
    }

    /// Parser minimalista de WAV PCM16 mono: extrae fmt + data sin depender
    /// de crates de audio (la muestra de referencia es PCM16 16kHz mono).
    fn parse_wav_f32(bytes: &[u8]) -> Option<Vec<f32>> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let mut pos = 12usize;
        let mut sample_rate = 0u32;
        let mut channels = 0u16;
        let mut bits = 0u16;
        let mut data: &[u8] = &[];
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
            let body = pos + 8;
            if body + size > bytes.len() {
                break;
            }
            match id {
                b"fmt " => {
                    if size >= 16 {
                        let fmt = u16::from_le_bytes(bytes[body..body + 2].try_into().ok()?);
                        if fmt != 1 {
                            return None; // solo PCM
                        }
                        channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().ok()?);
                        sample_rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?);
                        bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().ok()?);
                    }
                }
                b"data" => {
                    data = &bytes[body..body + size];
                }
                _ => {}
            }
            pos = body + size + (size & 1); // chunks alineados a palabra par
        }
        if sample_rate != 16_000 || channels != 1 || bits != 16 {
            return None;
        }
        let mut out = Vec::with_capacity(data.len() / 2);
        for c in data.chunks_exact(2) {
            out.push(i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0);
        }
        Some(out)
    }

    #[test]
    fn wav_parser_accepts_pcm16_mono() {
        // header mínimo: RIFF/WAVE + fmt PCM16 16kHz mono + 4 muestras
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&36u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8u32.to_le_bytes());
        wav.extend_from_slice(&0i16.to_le_bytes());
        wav.extend_from_slice(&32767i16.to_le_bytes());
        wav.extend_from_slice(&(-32768i16).to_le_bytes());
        wav.extend_from_slice(&16384i16.to_le_bytes());
        let samples = parse_wav_f32(&wav).expect("wav válido");
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[1], 32767.0 / 32768.0);
        assert_eq!(samples[2], -1.0);
    }

    #[test]
    fn wav_parser_rejects_stereo_and_non_pcm() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&36u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes()); // estéreo
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&64_000u32.to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 8]);
        assert_eq!(parse_wav_f32(&wav), None);
    }
}