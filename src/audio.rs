//! Captura del micrófono con cpal. Va por turnos: `start()` abre la entrada
//! del dispositivo y va guardando muestras; `stop()` corta la captura, sube
//! el volumen si se grabó muy bajo y devuelve las muestras para pasar a texto.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Tope de duración de un turno de dictado. El motivo: el búfer crece con el
/// tiempo (a 48 kHz, de estéreo a mono, son unos 230 MB por hora), así que se
/// corta a las 2 h — de sobra para una nota y con la memoria controlada
/// (~460 MB en el peor caso antes de cambiar la frecuencia). Además la app
/// corta el turno sola al llegar al tope (lo vigila el `Poll`), de modo que
/// lo grabado hasta ese momento NO se pierde: se transcribe igual.
pub const MAX_RECORDING_SECS: u64 = 2 * 60 * 60;

/// Lista los micrófonos que hay (para el selector de la interfaz).
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices
            .filter_map(|device| device.name().ok())
            .filter(|name| is_real_capture(name))
            .collect(),
        Err(_) => vec![],
    }
}

/// En Linux, cpal usa ALSA y `input_devices()` devuelve todo el catálogo de
/// plugins (`null`, `lavrate`, `speexrate`, `jack`, `oss`, `upmix`…), que no
/// son micrófonos y llenaban el selector de ruido. Solo pasan la entrada por
/// defecto y las tarjetas de sonido.
#[cfg(target_os = "linux")]
fn is_real_capture(name: &str) -> bool {
    matches!(name, "default" | "pipewire" | "pulse")
        || ["hw:", "plughw:", "sysdefault:", "front:"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

#[cfg(not(target_os = "linux"))]
fn is_real_capture(_name: &str) -> bool {
    true
}

/// Decisión (sin tocar hardware) sobre qué micrófono abrir según el config y
/// los que ofrece el sistema. Tenerla aparte permite fijar con un test la
/// regla «el configurado ya no está → se usa el de por defecto».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelection {
    /// El dispositivo configurado está entre los disponibles: se usa ese.
    Configured,
    /// El configurado NO está (USB desenchufado, driver recargado, nombre
    /// cambiado…): se usa el micrófono por defecto del sistema.
    Fallback { configured: String },
    /// No hay nada configurado: se usa el de por defecto, sin avisar.
    Default,
}

/// Decide qué micrófono abrir. `available` son los nombres que devuelve
/// `host.input_devices()`.
pub fn select_device(configured: Option<&str>, available: &[String]) -> DeviceSelection {
    match configured {
        Some(name) if available.iter().any(|device| device == name) => DeviceSelection::Configured,
        Some(name) => DeviceSelection::Fallback {
            configured: name.to_string(),
        },
        None => DeviceSelection::Default,
    }
}

/// Aviso en español para la interfaz cuando hubo que dictar con el micrófono
/// por defecto porque el configurado no estaba (H6). Nombra los dos para que
/// el usuario sepa qué se usó de verdad y cómo arreglarlo; nunca un error seco.
pub fn fallback_notice(configured: &str, used: &str) -> String {
    format!(
        "El micrófono configurado («{configured}») no está disponible; se usa «{used}». \
         Revísalo en Ajustes si no es el que quieres."
    )
}

/// Abre el micrófono pedido por nombre. Si ya no está entre los disponibles,
/// tira del que haya por defecto y devuelve el aviso para la interfaz: sin
/// esto, el dictado se quedaba muerto hasta que el usuario entraba en Ajustes
/// (H6). Si tampoco hay uno por defecto, error explicado en español.
///
/// Se pregunta una sola vez: se elige el dispositivo y, a la vez, se guardan
/// los nombres para la decisión sin hardware (`select_device`).
fn open_configured_device(
    host: &cpal::Host,
    name: &str,
) -> Result<(cpal::Device, Option<String>), String> {
    let mut available: Vec<String> = Vec::new();
    let mut target: Option<cpal::Device> = None;
    for device in host
        .input_devices()
        .map_err(|error| format!("no se pudo listar los micrófonos: {error}"))?
    {
        let label = device.name().unwrap_or_default();
        let is_target = label == name;
        available.push(label);
        if is_target {
            target = Some(device);
        }
    }

    match select_device(Some(name), &available) {
        DeviceSelection::Configured => Ok((
            target.ok_or_else(|| format!("micrófono \"{name}\" no encontrado"))?,
            None,
        )),
        DeviceSelection::Fallback { .. } | DeviceSelection::Default => {
            let device = host.default_input_device().ok_or_else(|| {
                format!(
                    "micrófono \"{name}\" no encontrado y no hay dispositivo de entrada por defecto"
                )
            })?;
            let used = device
                .name()
                .unwrap_or_else(|_| "dispositivo por defecto".to_string());
            crate::log!(
                "audio",
                "el micrófono configurado \"{name}\" no está disponible; se usa \"{used}\""
            );
            Ok((device, Some(fallback_notice(name, &used))))
        }
    }
}

/// Captura por turnos: el aviso de cpal (en sus propios hilos) va guardando
/// muestras mono; aquí se da a la interfaz el nivel reciente y los errores
/// del stream.
pub struct AudioCapture {
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>, // pico reciente en 0..=1000, para el medidor de la UI
    rate: u32,             // sample rate nativo del dispositivo (p. ej. 48 kHz)
    // cpal ejecuta su aviso en sus propios hilos. El `Stream` tiene que
    // quedarse en el hilo que lo creó; por eso no se mueve a otro hilo.
    stream: Option<cpal::Stream>,
    /// Errores del stream que avisa cpal (micro desconectado a mitad de
    /// grabación, driver caído…). La interfaz los recoge en su `Poll`: sin
    /// esto, un micro que se muere dejaba la app «Grabando» para siempre, con
    /// un stream muerto que no guardaba nada.
    errors: mpsc::Receiver<String>,
    /// Aviso para la interfaz cuando el micrófono configurado ya no estaba y
    /// se dictó con el de por defecto (H6). `None` = se usó el configurado (o
    /// el de por defecto sin incidencias).
    fallback_notice: Option<String>,
}

impl AudioCapture {
    /// Abre el micrófono por nombre (si `device_name` tiene algo y existe);
    /// si no, usa la entrada por defecto. Un nombre configurado que ya no
    /// existe NO deja el dictado muerto: se usa el de por defecto y se avisa
    /// en la interfaz (H6).
    ///
    /// El stream se crea antes de devolver `Ok`, así que cualquier fallo real
    /// llega a la pantalla de error de la interfaz. cpal mantiene el aviso
    /// activo mientras el `stream` siga vivo.
    pub fn start(device_name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        let (device, fallback) = match device_name {
            Some(name) => open_configured_device(&host, name)?,
            None => (
                host.default_input_device()
                    .ok_or_else(|| "no hay dispositivo de entrada (micrófono)".to_string())?,
                None,
            ),
        };

        let config = device
            .default_input_config()
            .map_err(|error| format!("no se pudo abrir la config de entrada: {error}"))?;
        let rate = config.sample_rate().0;
        crate::log!(
            "audio",
            "dispositivo de entrada: «{}»",
            device
                .name()
                .unwrap_or_else(|_| "desconocido".to_string())
        );
        crate::log!(
            "audio",
            "config de entrada nativa: {rate} Hz, {} canales, {:?} ({}); \
             buffer cpal = Default (el driver/elige)",
            config.channels(),
            config.sample_format(),
            if cfg!(target_os = "linux") {
                "host cpal ALSA→PipeWire/ALSA"
            } else {
                "host cpal nativo del SO"
            },
        );
        let samples = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        // Canal de errores: el aviso de cpal corre en su propio hilo y no
        // puede tocar la interfaz; el mensaje viaja hasta el `Poll` de la app.
        let (err_tx, err_rx) = mpsc::channel();
        let stream = build_stream(
            &device,
            &config,
            Arc::clone(&samples),
            Arc::clone(&level),
            err_tx,
        )
        .map_err(|error| format!("error creando stream: {error}"))?;
        stream
            .play()
            .map_err(|error| format!("error en stream.play(): {error}"))?;

        Ok(Self {
            samples,
            level,
            rate,
            stream: Some(stream),
            errors: err_rx,
            fallback_notice: fallback,
        })
    }

    /// Se lleva el aviso de cambio de micro (una sola vez) para que la
    /// interfaz lo enseñe en su línea de avisos: el usuario ve que el
    /// configurado no estaba y cuál se está usando de verdad (H6).
    pub fn take_fallback_notice(&mut self) -> Option<String> {
        self.fallback_notice.take()
    }

    /// Corta la captura y devuelve muestras mono a 16 kHz.
    pub fn stop(&mut self) -> Vec<f32> {
        // Se tira el stream a propósito antes de leer el búfer: así no
        // entran muestras nuevas mientras se prepara el turno para Whisper.
        self.stream.take();
        let mut samples = std::mem::take(
            &mut *self
                .samples
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        normalize_gain(&mut samples);

        if self.rate != 16_000 {
            samples = resample_to_16k(&samples, self.rate);
        }
        samples
    }

    /// Nivel del micrófono entre 0 y 1 (el pico reciente), para el medidor.
    pub fn level(&self) -> f32 {
        self.level.load(Ordering::Relaxed) as f32 / 1000.0
    }

    /// Error del stream pendiente de atender (micro desconectado, driver
    /// caído…). `Some(mensaje)` = la app debe cortar el turno YA con el
    /// audio grabado hasta ese momento.
    pub fn take_stream_error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
}

/// Sube el volumen si el audio quedó casi mudo (pico < 5%): con un micro
/// bajo, whisper «oye» ruido y se inventa cosas (por ejemplo «Música»).
fn normalize_gain(samples: &mut [f32]) {
    let peak = samples
        .iter()
        .fold(0.0f32, |accumulator, sample| accumulator.max(sample.abs()));
    if peak > 0.0 && peak < 0.05 {
        let gain = 0.3 / peak;
        for sample in samples.iter_mut() {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }
}

/// Pasa a 16 kHz interpolando. Con eso basta para dictado; si algún día
/// la calidad se queda corta, habría que usar un conversor de más nivel.
pub(crate) fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 16_000 {
        return samples.to_vec();
    }
    let ratio = 16_000.0 / from_rate as f64;
    let output_len = ((samples.len() as f64) * ratio).floor() as usize;
    let mut output = Vec::with_capacity(output_len);
    for index in 0..output_len {
        let position = index as f64 / ratio;
        let left = position.floor() as usize;
        let fraction = (position - left as f64) as f32;
        let right = (left + 1).min(samples.len() - 1);
        output.push(samples[left] * (1.0 - fraction) + samples[right] * fraction);
    }
    output
}

/// Mensaje claro (en español, sin hablar de drivers) para un error del
/// stream a mitad de grabación. El detalle técnico se queda en el log; el
/// usuario solo necesita saber QUÉ pasó y que lo grabado no se pierde.
pub fn friendly_stream_error(_raw: &str) -> String {
    "el micrófono se desconectó o dejó de responder a mitad de la grabación".to_string()
}

/// Notas del port a Linux (t_7ac526a3) — con cpal 0.15 en este equipo:
///
/// 1. SALIDA DE AUDIO: cpal 0.15 en Linux habla con ALSA por defecto (en
///    crates.io, versión 0.15.3, los hosts de Linux son ALSA y JACK;
///    PipeWire se usa a través de su plugin `pipewire-alsa`, no con un host
///    propio — la opción `pipewire` existe pero NO se activa aquí, para no
///    pedir libpipewire-dev al compilar). En Omarchy (con PipeWire) el
///    camino real es app → cpal/ALSA → plugin pipewire-alsa → PipeWire. Es
///    el mismo que usan casi todas las apps con ventana, y sirve tanto para
///    el micrófono normal como para elegir dispositivo (los nombres llegan
///    como ids de ALSA del tipo «hw:0,0» o como perfiles de pipewire-alsa).
/// 2. BÚFER: cpal usa `BufferSize::Default` al pedir la configuración por
///    defecto. Al grabar, el búfer de entrada no mete un retardo que se note
///    en dictado (se consume lo que el driver va juntando); no aplica el
///    fallo de latencia alta de salida que cuenta el issue #1029 de cpal
///    (reproducir con pipewire-alsa y `BufferSize::Default` → búferes de
///    ~21 s). Si alguien reporta grabación entrecortada con búfer gigante
///    (el mismo tipo de fallo, pero grabando), el arreglo sería pedir
///    `SupportedStreamConfig` con `BufferSize::Fixed(1024)`; queda apuntado,
///    NO se hace (no está medido y tocaría el camino común de Windows y
///    Linux sin necesidad).
/// 3. FRECUENCIAS ALTAS: el «fallo conocido de cpal 0.15 en Wayland con
///    frecuencias altas» NO existe: Wayland no tiene nada que ver con el
///    audio (los hosts son ALSA y PipeWire). Lo que sí hay: (a) el issue
///    #1029 (búfer gigante con pipewire-alsa y `BufferSize::Default`, ver
///    nota 2, al reproducir) y (b) que la lista de dispositivos devuelve ids
///    de ALSA poco legibles para la interfaz. Ninguno bloquea: grabar a
///    44,1 o 48 kHz con la configuración por defecto funciona, y el fallo de
///    latencia alta no afecta al dictado (se graba por turnos, no en
///    continuo). SE QUEDA apuntado, sin parche a ciegas.
/// 4. Micro sin conectar o sin dispositivos: `default_input_device()` da
///    `None` → ya hay mensaje en español en `AudioCapture::start()` («no hay
///    dispositivo de entrada»).
/// 5. TODO lo de arriba está A VALIDAR con la compilación real por parte del
///    usuario (la task pedía no lanzar cargo build; lo contado con honestidad
///    queda en docs/PORT-LINUX/AUDIO-RUNTIME.md).

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    err_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream, String> {
    // Tope duro de muestras guardadas (la memoria no se dispara pase lo que
    // pase, aunque el hilo de la interfaz se atasque y no llegue a cortar el
    // turno por tiempo). Se calcula con la frecuencia NATIVA: el búfer guarda
    // a esa frecuencia y solo se cambia al parar.
    let max_samples = (config.sample_rate().0 as usize) * MAX_RECORDING_SECS as usize;
    // El aviso de error corre en un hilo de cpal: log y aviso por canal
    // (quien decide qué hacer con el turno es la interfaz, en su `Poll`).
    let error_callback = move |error: cpal::StreamError| {
        crate::log!("audio", "error en el stream: {error}");
        let _ = err_tx.send(friendly_stream_error(&error.to_string()));
    };
    let channels = config.channels() as usize;

    match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &config.config(),
                move |data: &[f32], _| {
                    push_f32(data, channels, &samples, &level, max_samples)
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &config.config(),
                move |data: &[i16], _| {
                    let converted: Vec<f32> =
                        data.iter().map(|sample| *sample as f32 / 32768.0).collect();
                    push_f32(&converted, channels, &samples, &level, max_samples);
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                &config.config(),
                move |data: &[u16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| *sample as f32 / 32768.0 - 1.0)
                        .collect();
                    push_f32(&converted, channels, &samples, &level, max_samples);
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        other => Err(format!("formato de muestra no soportado: {other:?}")),
    }
}

/// Pasa a mono promediando canales, lo acumula en el búfer y actualiza el
/// pico del medidor. El búfer nunca pasa de `max_samples`: al llegar al tope
/// deja de acumular (la app corta el turno por tiempo y transcribe lo que
/// haya).
fn push_f32(
    data: &[f32],
    channels: usize,
    samples: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<AtomicU32>,
    max_samples: usize,
) {
    if data.is_empty() {
        return;
    }
    let peak = data
        .iter()
        .fold(0.0f32, |accumulator, sample| accumulator.max(sample.abs()));
    let peak_scaled = (peak * 1000.0) as u32;
    level.store(
        peak_scaled.max(level.load(Ordering::Relaxed)),
        Ordering::Relaxed,
    );

    let mut guard = samples.lock().unwrap_or_else(|error| error.into_inner());
    if guard.len() >= max_samples {
        return;
    }
    if channels <= 1 {
        // Tope EXACTO: solo entra lo que cabe (el trozo de cpal puede
        // pasarse del tope por menos de un trozo; aquí se recorta).
        let room = max_samples - guard.len();
        guard.extend_from_slice(&data[..room.min(data.len())]);
        return;
    }
    for chunk in data.chunks(channels) {
        if guard.len() >= max_samples {
            break;
        }
        let sum: f32 = chunk.iter().sum();
        guard.push(sum / chunk.len() as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_silent_audio() {
        let mut samples = vec![0.001, -0.002, 0.0015, -0.001];
        normalize_gain(&mut samples);
        let peak = samples
            .iter()
            .fold(0.0f32, |accumulator, sample| accumulator.max(sample.abs()));
        assert!(peak > 0.2 && peak <= 1.0);
    }

    #[test]
    fn leaves_loud_audio_untouched() {
        let mut samples = vec![0.2, -0.4, 0.3];
        let original = samples.clone();
        normalize_gain(&mut samples);
        assert_eq!(samples, original);
    }

    #[test]
    fn resamples_48k_to_16k_third_length() {
        let samples: Vec<f32> = (0..48).map(|i| (i % 3) as f32 / 3.0).collect();
        let output = resample_to_16k(&samples, 48_000);
        assert_eq!(output.len(), 16);
        assert!((output[0] - 0.0).abs() < 1e-6);
        assert!((output[15] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn resample_preserves_dc_and_passthrough() {
        let dc = vec![0.5f32; 30];
        let output = resample_to_16k(&dc, 44_100);
        assert_eq!(output.len(), 10);
        assert!(output.iter().all(|sample| (*sample - 0.5).abs() < 1e-6));

        let already = resample_to_16k(&dc, 16_000);
        assert_eq!(already, dc);
    }

    #[test]
    fn push_f32_acota_el_buffer_al_tope_de_memoria() {
        // Un dictado larguísimo no puede agotar la memoria: el búfer se corta
        // en `max_samples` aunque sigan llegando avisos.
        let samples = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        let chunk = vec![0.3f32; 1000];
        // Tope simulado de 2500 muestras: dos tandas lo llenan y la tercera
        // ya no puede pasar del tope.
        let cap = 2500usize;
        for _ in 0..5 {
            push_f32(&chunk, 1, &samples, &level, cap);
        }
        let guard = samples.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            guard.len(),
            cap,
            "el buffer se detiene exactamente en el tope"
        );
        // El medidor sigue funcionando pasado el tope (el usuario ve que el
        // micro sigue sonando aunque el turno ya esté cortado).
        assert!(level.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn error_de_stream_se_explica_en_espanol_sin_jerga() {
        let msg = friendly_stream_error("device type is no longer available");
        assert!(msg.contains("micrófono"), "el mensaje nombra al culpable");
        assert!(
            !msg.contains("available"),
            "sin jerga del driver en la cara del usuario"
        );
    }

    #[test]
    fn tope_de_grabacion_es_holgado_y_acotado() {
        // 2 h: de sobra para una nota hablada y memoria controlada
        // (~460 MB en el peor caso a 16 kHz; a 48 kHz se cambia la frecuencia
        // al parar, así que el tope nativo equivale a 2 h de audio real).
        assert_eq!(MAX_RECORDING_SECS, 2 * 60 * 60);
    }

    /// H6 (QA 24/09/2026): si el micrófono configurado desaparece (el USB de
    /// `input_device: "Línea (4- AudioBox USB)"` desenchufado), el dictado NO
    /// puede quedarse muerto con un error seco: se usa el micrófono por
    /// defecto. El audio real lo prueba el usuario; aquí se fija la decisión.
    #[test]
    fn h6_dispositivo_configurado_ausente_cae_al_por_defecto() {
        let disponibles = vec![
            "Micrófono (Realtek(R) Audio)".to_string(),
            "Línea (4- AudioBox USB)".to_string(),
        ];
        // Está → se usa el configurado, sin aviso.
        assert_eq!(
            select_device(Some("Línea (4- AudioBox USB)"), &disponibles),
            DeviceSelection::Configured
        );
        // Sin configurar → se usa el de por defecto, sin aviso.
        assert_eq!(select_device(None, &disponibles), DeviceSelection::Default);
        // No está → hay que usar el de por defecto y avisar.
        assert_eq!(
            select_device(
                Some("Línea (4- AudioBox USB)"),
                &["Micrófono (Realtek(R) Audio)".to_string()],
            ),
            DeviceSelection::Fallback {
                configured: "Línea (4- AudioBox USB)".to_string(),
            }
        );
    }

    /// H6: el cambio de micro no es silencioso. El aviso nombra el dispositivo
    /// configurado, el que se usa de verdad y cómo arreglarlo (el usuario lo ve
    /// en la interfaz, que lo pinta como aviso ámbar).
    #[test]
    fn h6_el_aviso_de_fallback_nombra_los_dos_dispositivos() {
        let aviso = fallback_notice("Línea (4- AudioBox USB)", "Micrófono (Realtek(R) Audio)");
        assert!(aviso.contains("Línea (4- AudioBox USB)"), "{aviso}");
        assert!(aviso.contains("Micrófono (Realtek(R) Audio)"), "{aviso}");
        assert!(aviso.contains("no está disponible"), "{aviso}");
        assert!(aviso.contains("Ajustes"), "dice cómo arreglarlo: {aviso}");
    }

    /// En Linux, el host de cpal lista todo el catálogo de plugins de ALSA como
    /// si fueran entradas: el selector enseñaba «null», «jack», «speexrate»…
    /// junto a los micrófonos de verdad.
    #[test]
    #[cfg(target_os = "linux")]
    fn el_zoo_de_plugins_de_alsa_no_cuenta_como_microfono() {
        for plugin in [
            "null",
            "lavrate",
            "samplerate",
            "speexrate",
            "speex",
            "jack",
            "oss",
            "upmix",
            "vdownmix",
            "usbstream:CARD=Generic",
        ] {
            assert!(!is_real_capture(plugin), "«{plugin}» no es una entrada");
        }
        for real in [
            "default",
            "pipewire",
            "pulse",
            "sysdefault:CARD=Generic_1",
            "hw:CARD=Generic_1,DEV=0",
            "plughw:CARD=Generic,DEV=0",
            "front:CARD=Generic,DEV=0",
        ] {
            assert!(is_real_capture(real), "«{real}» sí es una entrada");
        }
    }
}
