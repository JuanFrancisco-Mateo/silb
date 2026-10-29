//! Transcripción con whisper-rs (whisper.cpp dentro de la app, modelos GGML).
//!
//! Los modelos se descargan de HuggingFace (repo ggerganov/whisper.cpp,
//! ficheros ggml-*.bin) a `%APPDATA%/whispr-open/models/` en Windows y
//! `~/.local/share/whispr-open/models/` en Linux (ver `models_dir`).
//!
//! GPU/CUDA (port a Linux t_7ac526a3): la opción `cuda` de whisper-rs pasa a
//! ser una opción de Cargo del crate, apagada por defecto (ver Cargo.toml
//! `[features] cuda`). Sin ella, whisper.cpp se compila solo para CPU y
//! `TranscribeEngine` es el ÚNICO camino (aquí no hay código que elija GPU:
//! decide whisper.cpp por dentro; la opción solo hace que se compilen las
//! piezas de CUDA y el soporte de GPU).
//! En Linux lo que se compila por defecto es solo CPU — sirve en el portátil
//! de 12 hilos con el modelo small (una estimación honesta, con referencias
//! en docs/PORT-LINUX/AUDIO-RUNTIME.md; la medición real la hará el usuario).
//! El código es el mismo con y sin CUDA: los mismos `FullParams` valen en los
//! dos casos (whisper.cpp reserva la memoria de la GPU al crear el contexto).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const BEAM_SIZE: i32 = 5;
pub const VAD_MODEL_FILE: &str = "ggml-vad-q8.bin";
const PROMPT_MAX_CHARS: usize = 256;

/// Modelo de voz de whisper (tiny/base/small): se cambia velocidad por
/// calidad. Tiny y Base se equivocan con sonidos del español («buscar» →
/// «boscar»); Small es el mínimo que va bien.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    Tiny,
    Base,
    Small,
}

impl Model {
    /// Nombre que se guarda en config.json (`"tiny"|"base"|"small"`).
    pub fn id(&self) -> &'static str {
        match self {
            Model::Tiny => "tiny",
            Model::Base => "base",
            Model::Small => "small",
        }
    }
    /// Texto en español para el selector de la interfaz.
    pub fn label(&self) -> &'static str {
        match self {
            Model::Tiny => "Tiny (rápido)",
            Model::Base => "Base (equilibrado)",
            Model::Small => "Small (preciso)",
        }
    }
    /// Nombre del fichero GGML en el disco (`ggml-*.bin`).
    pub fn file_name(&self) -> &'static str {
        match self {
            Model::Tiny => "ggml-tiny.bin",
            Model::Base => "ggml-base.bin",
            Model::Small => "ggml-small.bin",
        }
    }
    /// Peso aproximado de la descarga, en MB (para los avisos de la interfaz).
    pub fn approx_mb(&self) -> u32 {
        match self {
            Model::Tiny => 75,
            Model::Base => 142,
            Model::Small => 466,
        }
    }
    /// Los tres modelos que ofrece el selector.
    pub fn all() -> [Model; 3] {
        [Model::Tiny, Model::Base, Model::Small]
    }
    /// De dónde se descarga (HuggingFace, el repo oficial de whisper.cpp).
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

/// Carpeta de modelos dentro de `%APPDATA%\whispr-open\models` (Windows) o
/// `~/.local/share/whispr-open/models` (Linux, respeta `$XDG_DATA_HOME`); ahí
/// se dejan los GGML descargados.
#[cfg(target_os = "windows")]
pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("whispr-open")
        .join("models")
}

/// Linux: los modelos van a la carpeta de datos XDG (son grandes y no se editan a mano).
#[cfg(not(target_os = "windows"))]
pub fn models_dir() -> PathBuf {
    crate::paths::app_data_dir().join("models")
}
/// Ruta completa del GGML del modelo indicado.
pub fn model_path(model: Model) -> PathBuf {
    models_dir().join(model.file_name())
}
/// ¿El fichero del modelo ya está en el disco? (Que exista no es que valga:
/// para eso está `model_file_is_valid`.)
pub fn model_exists(model: Model) -> bool {
    model_path(model).is_file()
}

/// Los primeros bytes de un GGML de whisper.cpp tal como quedan EN DISCO: el
/// número `0x67676d6c` escrito en little-endian se lee como los bytes
/// `6c 6d 67 67` («lmgg»). Comprobado con un ggml-tiny.bin de verdad
/// descargado de ggerganov/whisper.cpp.
const GGML_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

/// Tamaño mínimo que puede tener un modelo de whisper (todos pasan de 75 MB).
/// Un fichero más pequeño con los bytes buenos solo puede estar a medias.
const MIN_MODEL_BYTES: u64 = 1024 * 1024;

/// Un GGML está bien si existe, pasa del tamaño mínimo y empieza con los
/// bytes de whisper.cpp. Es la defensa contra ficheros a medias (copias
/// manuales cortadas, disco que falla, descargas de versiones viejas de la app
/// que no usaban `.part`): sin esto, un modelo roto en su sitio hace que
/// «Reintentar» falle una y otra vez contra el mismo fichero sin volver a
/// descargarlo nunca.
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

/// Qué hacer con un posible modelo que ya está en el disco antes de descargar
/// nada. Va aparte, sin red, para poder probar la decisión: `ensure_model` la
/// aplica tal cual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingModelPlan {
    /// Hay fichero y está bien: ni tocarlo ni descargar.
    UseIt,
    /// Hay fichero pero está roto o a medias: borrarlo y descargar otra vez.
    DeleteAndRedownload,
    /// No hay fichero: descarga normal.
    DownloadFresh,
}

/// Decisión (sin red ni disco) sobre un posible modelo que ya está: usarlo,
/// borrarlo y volver a descargar, o descargar desde cero. `ensure_model` la
/// aplica tal cual; tenerla aparte permite probarla sin red.
pub fn existing_model_plan(exists: bool, valid: bool) -> ExistingModelPlan {
    match (exists, valid) {
        (true, true) => ExistingModelPlan::UseIt,
        (true, false) => ExistingModelPlan::DeleteAndRedownload,
        (false, _) => ExistingModelPlan::DownloadFresh,
    }
}

/// Borra los `.part` de descargas cortadas (la nuestra si falló a medias, o
/// las de ejecuciones anteriores que murieron sin limpiar). Si la limpieza
/// falla no es un error: se intenta y se sigue.
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

/// Clasifica un error de descarga (ureq o std::fs) en una causa corta y útil
/// para la interfaz. Nada de cadenas técnicas de las librerías en la cara del
/// usuario; el detalle se queda en el log.
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
/// Ruta del modelo de VAD (detectar cuándo hay voz) si se descargó.
pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_MODEL_FILE)
}
/// ¿Existe el fichero de VAD indicado? (La VAD es opcional: si falta,
/// whisper transcribe sin ella.)
pub fn vad_available_for(model_file: &Path) -> bool {
    model_file.is_file()
}
/// ¿Hay VAD en la carpeta de modelos?
pub fn vad_available() -> bool {
    vad_available_for(&vad_model_path())
}

static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Se asegura de que el modelo está en el disco y de que ES un GGML bueno,
/// descargándolo si falta (o bajándolo otra vez si el que hay está a medias).
///
/// Llama a `on_progress(downloaded, total)` con los bytes bajados y el total
/// (0 si el servidor no lo dice) para la barra de progreso. Devuelve la ruta
/// del fichero listo para cargar.
///
/// Falla con un mensaje en español cuando no hay red, no se puede escribir en
/// la carpeta (permisos o disco) o el servidor no devuelve un GGML válido.
/// Nunca deja un `.part` a medias después de un fallo.
pub fn ensure_model(model: Model, on_progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    let lock = DOWNLOAD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|e| format!("bloqueo de descarga envenenado: {e}"))?;
    let path = model_path(model);

    // Limpia descargas a medias de ejecuciones anteriores (no pueden quedar
    // ficheros temporales sueltos tras un fallo o un cierre brusco).
    sweep_part_files(&models_dir());

    // Decisión sobre un posible fichero que ya está (comprobado, no solo
    // «existe»): un GGML a medias hay que volver a descargarlo, no quedarse
    // bloqueado en el arranque para siempre.
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
    // Sin límite GLOBAL de tiempo: en ureq 2.x el `timeout()` de la petición
    // también cuenta la lectura del cuerpo, así que en una línea lenta
    // (cientos de MB a pocos MB/s) la descarga se cortaba a los 2 min, se
    // borraba el `.part` y cada intento volvía a empezar. Con límite al
    // conectar y al leer cada trozo, una descarga lenta pero viva termina; una
    // conexión colgada se sigue cortando.
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
    // Toda la parte que puede fallar va dentro de un bloque para que UN solo
    // sitio de limpieza quite el `.part` con cualquier error (leer, escribir,
    // sincronizar o renombrar): nunca queda un temporal a medias tras un fallo.
    let outcome = (|| -> Result<(), String> {
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| format!("no se pudo crear {tmp:?}: {e}"))?;
        let mut downloaded = 0u64;
        let mut buf = [0u8; 64 * 1024];
        let mut reader = resp.into_reader();
        use std::io::{Read, Write};
        // El canal del arranque se lee 10 veces por segundo (un evento por
        // vuelta), así que avisar del progreso por cada trozo de 64 KB metía
        // ~7.400 eventos en una descarga rápida: la interfaz se quedaba clavada
        // en «5 / 465 MB · quedan más de 1 h» minutos después de haber
        // terminado. Se avisa como mucho una vez por 1 % (o por MB cuando el
        // servidor no dice el tamaño).
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
        // Cerrar el fichero ANTES de renombrarlo: en Windows mover un fichero
        // abierto puede fallar según cómo se abrió.
        drop(file);
        std::fs::rename(&tmp, &path).map_err(|e| format!("no se pudo finalizar la descarga: {e}"))?;
        Ok(())
    })();
    if let Err(e) = outcome {
        // Un fallo a mitad de descarga no deja basura en el disco: el `.part`
        // se quita siempre (si se puede).
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    // El servidor respondió 200 pero lo que llegó no es un GGML (por ejemplo
    // una página HTML de error de un proxy): mejor verlo aquí y borrarlo que
    // dejar un modelo que no se va a poder cargar.
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
/// `None` si no queda ninguna palabra que sirva. Se quitan repetidos y se
/// corta en `PROMPT_MAX_CHARS`: el prompt empuja la transcripción hacia esas
/// palabras (nombres propios, jerga), pero uno largo hace más lenta cada
/// transcripción y puede mandar demasiado en la del turno.
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

/// Vuelca los parámetros con los que se transcribió (para el log: saber QUÉ
/// se le pidió a whisper cuando algo suena raro).
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

/// Ajustes de la transcripción que se cambian en Ajustes avanzados: idioma
/// (`None` = que lo detecte solo), beam size (calidad ↔ velocidad) y número de
/// hilos del decodificador.
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

/// Cuántos hilos conviene usar (port a Linux, t_7ac526a3): el valor de
/// siempre es 4, razonable en un Ryzen 5 5650U (6 núcleos/12 hilos) donde
/// también corren la interfaz y cpal; con el modelo small solo en CPU se gana
/// más subiendo a 6 hilos que a 12 (el decodificador de whisper.cpp no escala
/// en proporción y el sistema lo comparte con la interfaz). El valor lo pone
/// el usuario en Ajustes → Avanzado; este comentario deja la recomendación,
/// NO una medición (no se ha hecho ninguna en el port: measurement pending,
/// ver AUDIO-RUNTIME.md).

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

/// Motor de transcripción: un contexto de whisper.cpp protegido con un mutex
/// (una transcripción cada vez; el hilo de la interfaz nunca lo toca).
pub struct WhisperEngine {
    inner: Mutex<WhisperContext>,
}
impl WhisperEngine {
    /// Carga el modelo del disco. Falla si el fichero no existe o no es un
    /// GGML válido (el mensaje dice qué modelo).
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
    /// Pasa `samples` (mono a 16 kHz) a texto plano. Recibe los ajustes y un
    /// `initial_prompt` opcional (las palabras del diccionario, para empujar al
    /// reconocedor hacia ellas). Devuelve los trozos unidos con espacios. Falla
    /// si whisper no puede con el audio (mensaje en español).
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

/// Carga el modelo en la caché SIN transcribir (adelanto al arrancar,
/// petición de UX del 09/09/2026). Así la primera pulsación del atajo ya no
/// paga la carga (esos 1-2 s de «Transcribiendo…» con el modelo todavía en
/// disco): la carga ocurre mientras el usuario ve la pantalla de arranque.
/// Se puede llamar varias veces sin problema y usa la misma `ENGINE_CACHE`
/// que `transcribe_cached` (cambiar de modelo aquí o al transcribir sustituye
/// el contexto sin riesgo).
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

/// Transcribe reutilizando el contexto ya cargado del modelo activo. Solo se
/// guarda un modelo para no gastar memoria; cambiar de modelo sustituye el
/// contexto anterior sin riesgo cuando no hay otro turno en marcha.
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
        // Sin expect(): el «acabamos de poner Some» se comprueba con un
        // Result, no con un pánico. Si un cambio futuro lo rompe, el turno
        // falla con un error visible y el dictado siguiente sigue funcionando.
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
        // Un GGML a medias en el destino no puede dejar la app en un bucle de
        // «Reintentar» que falla siempre.
        let dir = std::env::temp_dir().join(format!("whispr-valid-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        // Bien: los bytes buenos y más del tamaño mínimo.
        let good = dir.join("good.bin");
        let mut content = vec![0u8; MIN_MODEL_BYTES as usize];
        content[0..4].copy_from_slice(&GGML_MAGIC);
        std::fs::write(&good, &content).unwrap();
        assert!(model_file_is_valid(&good));

        // A medias: los bytes buenos pero no llega al tamaño mínimo.
        let tiny = dir.join("tiny.bin");
        std::fs::write(&tiny, &GGML_MAGIC).unwrap();
        assert!(!model_file_is_valid(&tiny));

        // Basura: bytes cualesquiera (una página HTML de error de un proxy).
        let junk = dir.join("junk.bin");
        let mut junk_bytes = vec![b'x'; MIN_MODEL_BYTES as usize];
        junk_bytes[0..4].copy_from_slice(b"<htm");
        std::fs::write(&junk, &junk_bytes).unwrap();
        assert!(!model_file_is_valid(&junk));

        // No existe.
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
        // Si la carpeta no existe, no pasa nada: se llama otra vez y ya está.
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
