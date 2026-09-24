//! Captura de micrófono con cpal. Ciclo por turnos: `start()` abre el stream
//! del dispositivo de entrada y acumula muestras f32; `stop()` detiene la
//! captura, normaliza la ganancia si el nivel fue bajo y devuelve las muestras
//! para transcripción.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Tope de duración de un turno de dictado. Motivo (requisito de robustez,
/// «dictado muy largo sin agotar memoria»): el buffer crece linealmente con
/// el tiempo (a 48 kHz estéreo→mono son ~230 MB de f32 por hora), así que se
/// corta por arriba en 2 h — margen sobrado para una nota y memoria acotada
/// (~460 MB peor caso antes de remuestrear). La app además auto-detiene el
/// turno al llegar al tope (el Poll lo vigila), de modo que el audio capturado
/// hasta ese momento NO se pierde: se transcribe igual.
pub const MAX_RECORDING_SECS: u64 = 2 * 60 * 60;

/// Lista los dispositivos de entrada disponibles (para el selector de la UI).
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(_) => vec![],
    }
}

/// Decisión (pura, sin cpal) sobre qué dispositivo de entrada abrir según la
/// config y los dispositivos que el sistema ofrece. Separarla del hardware
/// permite fijar la regla «el configurado desapareció → se cae al por
/// defecto» con un test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelection {
    /// El dispositivo configurado está entre los disponibles: se usa ese.
    Configured,
    /// El configurado NO está (USB desenchufado, driver recargado, nombre
    /// cambiado…): se usa el dispositivo por defecto del sistema.
    Fallback { configured: String },
    /// No hay dispositivo configurado: se usa el por defecto, sin aviso.
    Default,
}

/// Decide qué dispositivo abrir. `available` son los nombres que devuelve
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

/// Aviso en español para la UI cuando hubo que dictar con el dispositivo por
/// defecto porque el configurado no estaba (H6). Nombra ambos para que el
/// usuario sepa qué se usó de verdad y cómo arreglarlo; nunca un error seco.
pub fn fallback_notice(configured: &str, used: &str) -> String {
    format!(
        "El micrófono configurado («{configured}») no está disponible; se usa «{used}». \
         Revísalo en Ajustes si no es el que quieres."
    )
}

/// Abre el dispositivo pedido por nombre. Si ya no está entre los
/// disponibles, cae al dispositivo por defecto del sistema y devuelve el
/// aviso para la UI: sin este fallback el dictado quedaba muerto hasta que
/// el usuario entrara a Ajustes (H6). Si tampoco hay default, error en
/// español.
///
/// Una sola enumeración: se resuelve el dispositivo y, a la vez, se recogen
/// los nombres para la decisión pura (`select_device`).
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

/// Captura de micrófono por turnos: el callback de CPAL (en sus propios
/// hilos) acumula muestras f32 mono; aquí se expone el nivel reciente y los
/// errores del stream a la UI.
pub struct AudioCapture {
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>, // pico reciente en 0..=1000, para el medidor de la UI
    rate: u32,             // sample rate nativo del dispositivo (p. ej. 48 kHz)
    // CPAL ejecuta el callback en sus propios hilos. El Stream debe permanecer
    // en el hilo que lo creó en este target; por eso no se mueve a std::thread.
    stream: Option<cpal::Stream>,
    /// Errores del stream avisados por cpal (dispositivo desconectado a
    /// mitad de grabación, driver caído…). La UI lo drena en su Poll: sin
    /// esto, un micro que muere dejaba la app «Grabando» para siempre con
    /// un stream muerto que no acumulaba nada.
    errors: mpsc::Receiver<String>,
    /// Aviso para la UI cuando el micrófono configurado ya no estaba y se
    /// dictó con el dispositivo por defecto (H6). `None` = se usó el
    /// configurado (o el por defecto sin incidencias).
    fallback_notice: Option<String>,
}

impl AudioCapture {
    /// Abre el dispositivo por nombre (si `device_name` es Some y existe);
    /// si no, usa el dispositivo de entrada por defecto. Un nombre
    /// configurado que ya no existe NO mata el dictado: se cae al
    /// dispositivo por defecto avisando en la UI (H6).
    ///
    /// El stream se crea y se reproduce antes de devolver `Ok`, de modo que
    /// cualquier fallo real llega al estado de error de la UI. CPAL mantiene
    /// el callback activo mientras `stream` siga vivo.
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
        let samples = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        // Canal de errores: el callback de cpal corre en su propio hilo y no
        // puede tocar la UI; el mensaje viaja hasta el Poll de la app.
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

    /// Se lleva el aviso de fallback (una sola vez) para que la UI lo muestre
    /// en su línea de feedback: el usuario ve que el micrófono configurado no
    /// estaba y cuál se está usando de verdad (H6).
    pub fn take_fallback_notice(&mut self) -> Option<String> {
        self.fallback_notice.take()
    }

    /// Detiene la captura y devuelve muestras f32 mono a 16 kHz.
    pub fn stop(&mut self) -> Vec<f32> {
        // Drop explícito antes de leer el buffer: no pueden entrar callbacks
        // nuevos mientras se prepara el turno para Whisper.
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

    /// Nivel del micrófono en 0..=1 (pico reciente), para el medidor.
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

/// Amplifica si el audio quedó casi mudo (pico < 5%): un micrófono a volumen
/// bajo hace que whisper «oiga» ruido y alucine (p. ej. «Música»).
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

/// Remuestrea a 16 kHz por interpolación lineal. Es suficiente para dictado;
/// si la calidad auditiva lo exige, cambiar a un remuestreador sinc.
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

/// Mensaje accionable (español, sin jerga de driver) para un error del
/// stream a mitad de grabación. El detalle técnico queda en el log; el
/// usuario solo necesita saber QUÉ pasó y que lo grabado no se pierde.
pub fn friendly_stream_error(_raw: &str) -> String {
    "el micrófono se desconectó o dejó de responder a mitad de la grabación".to_string()
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    err_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream, String> {
    // Tope duro de muestras acumuladas (memoria acotada pase lo que pase,
    // incluso si el hilo de la UI se atasca y no llega a auto-detener el
    // turno por tiempo). Calculado con el sample rate NATIVO: el buffer
    // guarda muestras a ese rate y solo se remuestrea al parar.
    let max_samples = (config.sample_rate().0 as usize) * MAX_RECORDING_SECS as usize;
    // El callback de error corre en un hilo de cpal: log + aviso por canal
    // (quien decida qué hacer con el turno es la UI en su Poll).
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

/// Convierte a mono promediando canales, acumula en el buffer y actualiza el
/// pico reciente del medidor. El buffer nunca supera `max_samples`: llegado
/// el tope se deja de acumular (la app corta el turno por tiempo y transcribe
/// lo que haya).
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
        // Tope EXACTO: se añade solo lo que cabe (el chunk de cpal puede
        // sobrepasar el tope por menos de un chunk — aquí se recorta).
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
        // Requisito: un dictado larguísimo no puede agotar la memoria. El
        // buffer se corta en `max_samples` aunque sigan llegando callbacks.
        let samples = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        let chunk = vec![0.3f32; 1000];
        // Tope simulado de 2500 muestras: dos pushes llenan, el tercero no
        // puede crecer más allá del tope.
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
        // El medidor sigue vivo tras el tope (el usuario ve que el micro
        // sigue sonando aunque el turno ya esté cortado).
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
        // 2 h: margen de sobra para una nota hablada y memoria acotada
        // (~460 MB peor caso a 16 kHz f32; a 48 kHz nativos se remuestrea al
        // parar, así que el tope nativo equivale a 2 h de audio real).
        assert_eq!(MAX_RECORDING_SECS, 2 * 60 * 60);
    }

    /// H6 (QA 24/09/2026): si el micrófono configurado desaparece (el USB de
    /// `input_device: "Línea (4- AudioBox USB)"` desenchufado), el dictado NO
    /// puede quedarse muerto con un error seco: se cae al dispositivo por
    /// defecto. El audio real lo prueba el usuario; aquí se fija la decisión.
    #[test]
    fn h6_dispositivo_configurado_ausente_cae_al_por_defecto() {
        let disponibles = vec![
            "Micrófono (Realtek(R) Audio)".to_string(),
            "Línea (4- AudioBox USB)".to_string(),
        ];
        // Está → se usa el configurado, sin aviso de fallback.
        assert_eq!(
            select_device(Some("Línea (4- AudioBox USB)"), &disponibles),
            DeviceSelection::Configured
        );
        // Sin configurar → se usa el por defecto, sin aviso.
        assert_eq!(select_device(None, &disponibles), DeviceSelection::Default);
        // Ausente → marca de fallback al dispositivo por defecto.
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

    /// H6: el fallback no es silencioso. El aviso nombra el dispositivo
    /// configurado, el que se usa de verdad y cómo arreglarlo (el usuario lo
    /// ve en la UI, que lo pinta como aviso ámbar).
    #[test]
    fn h6_el_aviso_de_fallback_nombra_los_dos_dispositivos() {
        let aviso = fallback_notice("Línea (4- AudioBox USB)", "Micrófono (Realtek(R) Audio)");
        assert!(aviso.contains("Línea (4- AudioBox USB)"), "{aviso}");
        assert!(aviso.contains("Micrófono (Realtek(R) Audio)"), "{aviso}");
        assert!(aviso.contains("no está disponible"), "{aviso}");
        assert!(aviso.contains("Ajustes"), "dice cómo arreglarlo: {aviso}");
    }
}
