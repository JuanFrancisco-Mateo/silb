//! Transcripción con whisper-rs (whisper.cpp nativo, modelos GGML).
//!
//! Los modelos se descargan de HuggingFace (repo ggerganov/whisper.cpp,
//! archivos ggml-*.bin) a `%APPDATA%/whispr-open/models/` en Windows y
//! `~/.local/share/whispr-open/models/` en Linux (ver `models_dir`).
//!
//! GPU/CUDA (port Linux t_7ac526a3): la feature `cuda` de whisper-rs pasa a
//! ser una feature de Cargo del propio crate, OFF por defecto (ver
//! Cargo.toml `[features] cuda`). Sin ella, whisper.cpp compila CPU-only y
//! `TranscribeEngine` es el ÚNICO camino (no hay código de selección de
//! dispositivo GPU en este módulo: whisper.cpp decide internamente; la
//! feature solo activa compilar los kernels CUDA y el backend GPU).
//! En Linux el build default del port es CPU-only — viable en el portátil
//! de 12 hilos con modelo small (estimación honesta y referencias en
//! docs/PORT-LINUX/AUDIO-RUNTIME.md; la medición real la deja el usuario).
//! El código no cambia entre ambas: los mismos `FullParams` aplican con y
//! sin CUDA (whisper.cpp hace la reserva GPU en el contexto).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const BEAM_SIZE: i32 = 5;
pub const VAD_MODEL_FILE: &str = "ggml-vad-q8.bin";
const PROMPT_MAX_CHARS: usize = 256;

/// Modelo de voz whisper disponible (tiny/base/small): trade-off
/// velocidad ↔ calidad de transcripción. Tiny/Base cometen errores
/// fonéticos en español («buscar»→«boscar»); Small es el mínimo fiable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    Tiny,
    Base,
    Small,
}

impl Model {
    /// Identificador persistido en config.json (`"tiny"|"base"|"small"`).
    pub fn id(&self) -> &'static str {
        match self {
            Model::Tiny => "tiny",
            Model::Base => "base",
            Model::Small => "small",
        }
    }
    /// Etiqueta en español para el selector de la UI.
    pub fn label(&self) -> &'static str {
        match self {
            Model::Tiny => "Tiny (rápido)",
            Model::Base => "Base (equilibrado)",
            Model::Small => "Small (preciso)",
        }
    }
    /// Nombre del fichero GGML en disco (`ggml-*.bin`).
    pub fn file_name(&self) -> &'static str {
        match self {
            Model::Tiny => "ggml-tiny.bin",
            Model::Base => "ggml-base.bin",
            Model::Small => "ggml-small.bin",
        }
    }
    /// Tamaño aproximado de descarga, en MB (para los avisos de la UI).
    pub fn approx_mb(&self) -> u32 {
        match self {
            Model::Tiny => 75,
            Model::Base => 142,
            Model::Small => 466,
        }
    }
    /// Los tres modelos ofrecidos en el selector.
    pub fn all() -> [Model; 3] {
        [Model::Tiny, Model::Base, Model::Small]
    }
    /// URL de descarga (HuggingFace, repo oficial de whisper.cpp).
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

/// Carpeta de modelos bajo `%APPDATA%\whispr-open\models` (Windows) o
/// `~/.local/share/whispr-open/models` (Linux, respeta `$XDG_DATA_HOME`);
/// allí instalan los GGML descargados.
#[cfg(target_os = "windows")]
pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("whispr-open")
        .join("models")
}

/// Linux: modelos en XDG data (ficheros grandes, no editables a mano).
#[cfg(not(target_os = "windows"))]
pub fn models_dir() -> PathBuf {
    crate::paths::app_data_dir().join("models")
}
/// Ruta completa del GGML del modelo indicado.
pub fn model_path(model: Model) -> PathBuf {
    models_dir().join(model.file_name())
}
/// ¿El fichero del modelo existe ya en disco? (Existir ≠ ser válido: para
/// eso está `model_file_is_valid`.)
pub fn model_exists(model: Model) -> bool {
    model_path(model).is_file()
}

/// Magic de los ficheros GGML de whisper.cpp tal como queda EN DISCO: el
/// u32 `0x67676d6c` escrito en little-endian se lee como los bytes
/// `6c 6d 67 67` («lmgg»). Verificado contra un ggml-tiny.bin real
/// descargado de ggerganov/whisper.cpp.
const GGML_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

/// Tamaño mínimo plausible de un modelo whisper (todos ≥ 75 MB). Un fichero
/// más pequeño con magic válido solo puede ser un truncado a medias.
const MIN_MODEL_BYTES: u64 = 1024 * 1024;

/// Un GGML está sano si existe, supera el tamaño mínimo y empieza con el
/// magic de whisper.cpp. Es la línea de defensa contra ficheros truncados
/// (copias manuales a medias, corrupción de disco, descargas de versiones
/// antiguas de la app que no usaban `.part`): sin esto, un modelo corrupto
/// en el destino hace que «Reintentar» falle una y otra vez contra el mismo
/// fichero sin re-descargarlo jamás.
pub fn model_file_is_valid(path: &Path) -> bool {
    use std::io::Read;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() < MIN_MODEL_BYTES {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    match file.read_exact(&mut magic) {
        Ok(()) => magic == GGML_MAGIC,
        Err(_) => false,
    }
}

/// Qué hacer con un posible modelo ya en disco antes de descargar nada.
/// Separado como función pura para que la decisión (y su test) no dependa
/// de la red: `ensure_model` la aplica tal cual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingModelPlan {
    /// Hay fichero y es un GGML sano: ni tocarlo ni descargar.
    UseIt,
    /// Hay fichero pero está dañado/truncado: borrar y re-descargar.
    DeleteAndRedownload,
    /// No hay fichero: descarga normal.
    DownloadFresh,
}

/// Decisión pura (sin red ni disco) sobre un posible modelo ya presente:
/// reutilizarlo, borrarlo y re-descargar, o descargar de cero. `ensure_model`
/// la aplica tal cual; separarla permite testear la decisión sin red.
pub fn existing_model_plan(exists: bool, valid: bool) -> ExistingModelPlan {
    match (exists, valid) {
        (true, true) => ExistingModelPlan::UseIt,
        (true, false) => ExistingModelPlan::DeleteAndRedownload,
        (false, _) => ExistingModelPlan::DownloadFresh,
    }
}

/// Borra los `.part` de descargas abortadas (la nuestra si falló a medias,
/// o las de ejecuciones anteriores que murieron antes de limpiar). Un
/// fallo de limpieza no es error: se intenta y sigue.
pub fn sweep_part_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("part") {
            crate::log!(
                "transcribe",
                "borrando descarga a medias: {}",
                path.display()
            );
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Clasifica un error crudo de descarga (ureq / std::fs) en una causa corta
/// y accionable para la UI. Requisito de robustez: nada de cadenas técnicas
/// de la cadena de crates en la cara del usuario; el detalle queda en el log.
pub fn friendly_download_error(raw: &str) -> &'static str {
    let r = raw.to_ascii_lowercase();
    if r.contains("dns")
        || r.contains("resolve")
        || r.contains("network")
        || r.contains("timed out")
        || r.contains("timeout")
        || r.contains("connection")
        || r.contains("connect")
    {
        "sin conexión (o un cortafuegos bloquea la descarga)"
    } else if r.contains("no se pudo crear")
        || r.contains("no se pudo escribir")
        || r.contains("espacio")
        || r.contains("disk")
        || r.contains("os error 112")
    {
        "no se pudo escribir el fichero (¿permisos o disco lleno?)"
    } else {
        "error de red o de disco durante la descarga"
    }
}
/// Ruta del modelo de VAD (detección de actividad de voz) si se descargó.
pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_MODEL_FILE)
}
/// ¿El fichero de VAD indicado existe? (La VAD es opcional: si falta,
/// whisper transcribe sin ella.)
pub fn vad_available_for(model_file: &Path) -> bool {
    model_file.is_file()
}
/// ¿Hay VAD disponible en la carpeta de modelos?
pub fn vad_available() -> bool {
    vad_available_for(&vad_model_path())
}

static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Garantiza que el modelo está en disco y ES un GGML sano, descargándolo
/// si falta (o re-descargándolo si el fichero existente está truncado).
///
/// Recibe `on_progress(downloaded, total)` con los bytes descargados y el
/// total (0 si el servidor no lo declara) para la barra de progreso.
/// Devuelve la ruta del fichero listo para cargar.
///
/// Falla con un mensaje en español cuando: no hay red, no se puede escribir
/// en la carpeta (permisos/disco), o el servidor no devuelve un GGML
/// válido. Nunca deja un `.part` a medias tras un fallo.
pub fn ensure_model(model: Model, on_progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    let lock = DOWNLOAD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|e| format!("bloqueo de descarga envenenado: {e}"))?;
    let path = model_path(model);

    // Limpieza de descargas a medias de ejecuciones anteriores (requisito:
    // sin ficheros temporales huérfanos tras un fallo o cierre a medias).
    sweep_part_files(&models_dir());

    // Decisión sobre un posible fichero ya en disco (validado, no solo
    // "existe"): un GGML truncado debe re-descargarse, no bloquear el
    // arranque para siempre.
    let plan = existing_model_plan(path.is_file(), model_file_is_valid(&path));
    match plan {
        ExistingModelPlan::UseIt => return Ok(path),
        ExistingModelPlan::DeleteAndRedownload => {
            crate::log!(
                "transcribe",
                "modelo {} en disco no es un GGML válido (truncado o corrupto): \
                 se borra y se re-descarga",
                model.file_name()
            );
            std::fs::remove_file(&path)
                .map_err(|e| format!("no se pudo borrar el modelo dañado: {e}"))?;
        }
        ExistingModelPlan::DownloadFresh => {}
    }

    std::fs::create_dir_all(models_dir())
        .map_err(|e| format!("no se pudo crear models dir: {e}"))?;
    // Sin timeout GLOBAL: en ureq 2.x el `timeout()` de la petición abarca la
    // lectura del cuerpo, así que en una línea lenta (cientos de MB a pocos
    // MB/s) la descarga se cortaba a los 2 min, el `.part` se borraba y cada
    // reintento volvía a empezar de cero. Con timeout de conexión y de lectura
    // por bloque, una descarga lenta pero viva termina; una conexión colgada
    // sigue cortando.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build();
    let resp = agent
        .get(&model.download_url())
        .call()
        .map_err(|e| format!("no se pudo descargar el modelo: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let tmp = path.with_file_name(format!("{}.{}.part", model.file_name(), std::process::id()));
    // Toda la parte fallible va en una closure para que UN solo punto de
    // limpieza quite el `.part` en cualquier error (read, write, sync o
    // rename): nunca queda un temporal a medias tras un fallo.
    let outcome = (|| -> Result<(), String> {
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| format!("no se pudo crear {tmp:?}: {e}"))?;
        let mut downloaded = 0u64;
        let mut buf = [0u8; 64 * 1024];
        let mut reader = resp.into_reader();
        use std::io::{Read, Write};
        // El canal del arranque se drena a 10 Hz (un evento por tick), así que
        // emitir progreso por cada bloque de 64 KB encolaba ~7.400 eventos en
        // una descarga rápida: la UI se quedaba clavada en «5 / 465 MB · quedan
        // más de 1 h» minutos después de haber terminado. Se emite como mucho
        // un evento por 1 % (o por MB cuando el servidor no declara tamaño).
        let step = if total > 0 { (total / 100).max(1) } else { 1 << 20 };
        let mut last_emit = 0u64;
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
            if downloaded - last_emit >= step {
                last_emit = downloaded;
                on_progress(downloaded, total);
            }
        }
        on_progress(downloaded, total);
        file.sync_all()
            .map_err(|e| format!("error sincronizando modelo: {e}"))?;
        // Cerrar el handle ANTES del rename: en Windows mover un fichero
        // abierto puede fallar según el share mode del handle.
        drop(file);
        std::fs::rename(&tmp, &path).map_err(|e| format!("no se pudo finalizar la descarga: {e}"))?;
        Ok(())
    })();
    if let Err(e) = outcome {
        // Requisito de robustez: un fallo a mitad de descarga no deja
        // basura en disco. El `.part` se quita siempre (best-effort).
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    // El servidor respondió 200 pero con contenido que no es un GGML (p. ej.
    // una página HTML de error de un proxy): mejor detectarlo aquí y
    // borrarlo que dejar un modelo imposible de cargar.
    if !model_file_is_valid(&path) {
        crate::log!(
            "transcribe",
            "lo descargado para {} no es un GGML válido; se descarta",
            model.file_name()
        );
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "la descarga de {} no devolvió un modelo válido",
            model.file_name()
        ));
    }
    Ok(path)
}

/// `initial_prompt` de whisper a partir del diccionario personal (`entries`,
/// usando `from`: la forma en que whisper los transcribe mal).
///
/// `None` si no queda ningún término usable. Se deduplica y se corta en
/// `PROMPT_MAX_CHARS`: el prompt sesga la decodificación hacia esos términos
/// (nombres propios, jerga), pero uno largo ralentiza cada transcripción y
/// puede dominar la decodificación del turno.
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

/// Volcado de la decisión de parámetros de una transcripción (para el log:
/// saber QUÉ se le pidió a whisper cuando algo suena raro).
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

/// Parámetros de inferencia ajustables desde Ajustes avanzados: idioma
/// (`None` = autodetección), beam size (calidad ↔ velocidad) y nº de
/// hilos del decoder.
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

/// Documentación de hilos recomendados (port Linux, t_7ac526a3): el default
/// histórico es 4, razonable en un Ryzen 5 5650U (6C/12T) donde el resto de
/// la app (UI iced, cpal) también corre; la velocidad de transcripción con
/// small CPU-only se gana más subiendo n_threads a 6 que a 12 (el decoder
/// de whisper.cpp escala de forma sublinear y el sistema lo comparte con la
/// UI). El valor lo ajusta el usuario en Ajustes → Avanzado; este comentario
/// documenta la recomendación, NO un benchmark medido (ninguno se ha
/// ejecutado en el port: measurement pending, ver AUDIO-RUNTIME.md).

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

/// Motor de transcripción: un contexto whisper.cpp protegido con un mutex
/// (una transcripción a la vez; el hilo de UI nunca lo toca directamente).
pub struct WhisperEngine {
    inner: Mutex<WhisperContext>,
}
impl WhisperEngine {
    /// Carga el contexto del modelo desde disco. Falla si el fichero no
    /// existe o no es un GGML válido (el mensaje nombra el modelo).
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
    /// Transcribe `samples` (f32 mono 16 kHz) a texto plano. Recibe los
    /// parámetros de inferencia y un `initial_prompt` opcional (los
    /// términos del diccionario, para sesgar el reconocedor). Devuelve los
    /// segmentos unidos por espacios. Falla si whisper no puede procesar
    /// el audio (mensaje en español).
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
        // Sin expect() (requisito de robustez): el invariante «acabamos de
        // poner Some» se comprueba con un Result, no con un pánico. Si un
        // refactor futuro lo rompe, el turno falla con un error visible y
        // el dictado siguiente sigue funcionando.
        slot.as_ref()
            .map(|(_, engine)| engine.clone())
            .ok_or_else(|| "cache de contexto vacío tras inicializarlo".to_string())?
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
    fn ggml_sano_truncado_y_basura_se_distinguen() {
        // Requisito de robustez: un GGML a medias en el destino no puede
        // dejar a la app en un bucle de «Reintentar» que falla siempre.
        let dir = std::env::temp_dir().join(format!("whispr-valid-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        // Sano: magic correcto + tamaño por encima del mínimo.
        let good = dir.join("good.bin");
        let mut content = vec![0u8; MIN_MODEL_BYTES as usize];
        content[0..4].copy_from_slice(&GGML_MAGIC);
        std::fs::write(&good, &content).unwrap();
        assert!(model_file_is_valid(&good));

        // Truncado: magic correcto pero tamaño mínimo no alcanzado.
        let tiny = dir.join("tiny.bin");
        std::fs::write(&tiny, &GGML_MAGIC).unwrap();
        assert!(!model_file_is_valid(&tiny));

        // Basura: bytes cualesquiera (una página HTML de error de proxy).
        let junk = dir.join("junk.bin");
        let mut junk_bytes = vec![b'x'; MIN_MODEL_BYTES as usize];
        junk_bytes[0..4].copy_from_slice(b"<htm");
        std::fs::write(&junk, &junk_bytes).unwrap();
        assert!(!model_file_is_valid(&junk));

        // Inexistente.
        assert!(!model_file_is_valid(&dir.join("no-hay.bin")));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn plan_de_modelo_existente_cubre_los_tres_casos() {
        use ExistingModelPlan::*;
        assert_eq!(existing_model_plan(true, true), UseIt);
        assert_eq!(existing_model_plan(true, false), DeleteAndRedownload);
        assert_eq!(existing_model_plan(false, false), DownloadFresh);
        assert_eq!(
            existing_model_plan(false, true),
            DownloadFresh,
            "sin fichero no hay nada que reutilizar"
        );
    }
    #[test]
    fn sweep_part_solo_borra_los_part() {
        let dir = std::env::temp_dir().join(format!("whispr-sweep-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("ggml-tiny.bin.1234.part"), b"a medias").unwrap();
        std::fs::write(dir.join("ggml-small.bin.999.part"), b"otro resto").unwrap();
        std::fs::write(dir.join("ggml-base.bin"), b"modelo bueno").unwrap();
        sweep_part_files(&dir);
        assert!(!dir.join("ggml-tiny.bin.1234.part").exists());
        assert!(!dir.join("ggml-small.bin.999.part").exists());
        assert!(dir.join("ggml-base.bin").exists(), "el modelo no se toca");
        // Idempotente y sin error si el directorio no existe.
        sweep_part_files(&dir.join("no-existe"));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn errores_de_descarga_se_traducen_a_causas_accionables() {
        assert_eq!(
            friendly_download_error("https://huggingface.co/...: Dns Failed: resolve failure"),
            "sin conexión (o un cortafuegos bloquea la descarga)"
        );
        assert_eq!(
            friendly_download_error("error de descarga: Connection timed out"),
            "sin conexión (o un cortafuegos bloquea la descarga)"
        );
        assert_eq!(
            friendly_download_error(
                "no se pudo crear C:\\...\\ggml-x.bin.4242.part: ... (os error 112)"
            ),
            "no se pudo escribir el fichero (¿permisos o disco lleno?)"
        );
        assert_eq!(
            friendly_download_error("error sincronizando modelo: algo raro"),
            "error de red o de disco durante la descarga"
        );
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
