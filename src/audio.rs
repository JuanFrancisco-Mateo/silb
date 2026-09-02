//! Captura de micrófono con cpal. Ciclo por turnos: `start()` abre el stream
//! del dispositivo de entrada y acumula muestras f32; `stop()` detiene la
//! captura, normaliza la ganancia si el nivel fue bajo y devuelve las muestras
//! para transcripción.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Lista los dispositivos de entrada disponibles (para el selector de la UI).
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
        Err(_) => vec![],
    }
}

pub struct AudioCapture {
    samples: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>, // pico reciente en 0..=1000, para el medidor de la UI
    rate: u32,             // sample rate nativo del dispositivo (p. ej. 48 kHz)
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioCapture {
    /// Abre el dispositivo por nombre (si `device_name` es Some y existe);
    /// si no, usa el dispositivo de entrada por defecto.
    pub fn start(device_name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();

        let device: cpal::Device = match device_name {
            Some(name) => host
                .input_devices()
                .map_err(|e| format!("no se pudo listar los micrófonos: {e}"))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| format!("micrófono \"{name}\" no encontrado"))?,
            None => host
                .default_input_device()
                .ok_or_else(|| "no hay dispositivo de entrada (micrófono)".to_string())?,
        };

        let config = device
            .default_input_config()
            .map_err(|e| format!("no se pudo abrir la config de entrada: {e}"))?;
        let rate = config.sample_rate().0;

        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));

        let samples_cb = Arc::clone(&samples);
        let stop_cb = Arc::clone(&stop);
        let level_cb = Arc::clone(&level);
        let level_cleanup = Arc::clone(&level);
        let stop_loop = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            let stream = match build_stream(&device, &config, samples_cb, stop_cb, level_cb) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[audio] error creando stream: {e}");
                    return;
                }
            };
            match stream.play() {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("[audio] error en stream.play(): {e}");
                    return;
                }
            }
            // El stream se mantiene vivo mientras dure el turno; el callback
            // acumula muestras. El flag de stop lo suelta en el siguiente tick.
            loop {
                if stop_loop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let _ = level_cleanup.store(0, Ordering::Relaxed);
            drop(stream);
        });

        Ok(Self { samples, stop, level, rate, handle: Some(handle) })
    }

    /// Detiene la captura y devuelve las muestras f32 mono, con ganancia
    /// normalizada si el audio fue demasiado bajo (micrófono silencioso).
    pub fn stop(&mut self) -> Vec<f32> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let mut samples = std::mem::take(&mut *self.samples.lock().unwrap_or_else(|e| e.into_inner()));
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
}

/// Amplifica si el audio quedó casi mudo (pico < 5%): un micrófono a volumen
/// bajo hace que whisper "oiga" ruido y alucine (p. ej. "Música").
/// La ganancia se calcula sobre el pico absoluto y se satura a 1.0.
fn normalize_gain(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
    if peak > 0.0 && peak < 0.05 {
        let gain = 0.3 / peak;
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

/// Remuestrea a 16 kHz por interpolación lineal (whisper.cpp espera 16 kHz;
/// el micrófono suele dar 44.1/48 kHz). Suficiente para dictado; si la
/// calidad auditiva lo exige, cambiar a un sinc (rubato).
fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 16_000 {
        return samples.to_vec();
    }
    let ratio = 16_000.0 / from_rate as f64;
    let out_len = ((samples.len() as f64) * ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 / ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let next = (idx + 1).min(samples.len() - 1);
        out.push(samples[idx] * (1.0 - frac) + samples[next] * frac);
    }
    out
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream, String> {
    let err_fn = |e| eprintln!("[audio] error en el stream: {e}");
    let channels = config.channels() as usize;

    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let stream = device
                .build_input_stream(
                    &config.config(),
                    move |data: &[f32], _| {
                        push_f32(data, channels, &samples, &level);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?;
            let _ = &stop;
            Ok(stream)
        }
        cpal::SampleFormat::I16 => {
            let stream = device
                .build_input_stream(
                    &config.config(),
                    move |data: &[i16], _| {
                        let mut buf = Vec::with_capacity(data.len());
                        for s in data {
                            buf.push(*s as f32 / 32768.0);
                        }
                        push_f32(&buf, channels, &samples, &level);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?;
            Ok(stream)
        }
        cpal::SampleFormat::U16 => {
            let stream = device
                .build_input_stream(
                    &config.config(),
                    move |data: &[u16], _| {
                        let mut buf = Vec::with_capacity(data.len());
                        for s in data {
                            buf.push((*s as f32 / 32768.0) - 1.0);
                        }
                        push_f32(&buf, channels, &samples, &level);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?;
            Ok(stream)
        }
        other => Err(format!("formato de muestra no soportado: {other:?}")),
    }
}

/// Convierte a mono promediando canales, acumula en el buffer y actualiza el
/// pico reciente del medidor.
fn push_f32(data: &[f32], channels: usize, samples: &Arc<Mutex<Vec<f32>>>, level: &Arc<AtomicU32>) {
    if data.is_empty() {
        return;
    }
    let peak = data.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
    let peak_pct = (peak * 1000.0) as u32;
    level.store(peak_pct.max(level.load(Ordering::Relaxed)), Ordering::Relaxed);

    let mut guard = samples.lock().unwrap_or_else(|e| e.into_inner());
    if channels <= 1 {
        guard.extend_from_slice(data);
        return;
    }
    for chunk in data.chunks(channels) {
        let sum: f32 = chunk.iter().sum();
        guard.push(sum / (chunk.len() as f32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_silent_audio() {
        let mut samples = vec![0.001, -0.002, 0.0015, -0.001];
        normalize_gain(&mut samples);
        let peak = samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        // 0.3/0.002 = 150 -> pico final ~0.3 (saturado a 1.0)
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
        let out = resample_to_16k(&samples, 48_000);
        assert_eq!(out.len(), 16);
        assert!((out[0] - 0.0).abs() < 1e-6);
        assert!((out[15] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn resample_preserves_dc_and_passthrough() {
        let dc = vec![0.5f32; 30];
        let out = resample_to_16k(&dc, 44_100);
        assert_eq!(out.len(), 10);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));

        let already = resample_to_16k(&dc, 16_000);
        assert_eq!(already, dc);
    }
}