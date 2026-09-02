//! Captura de micrófono con cpal. Ciclo por turnos: `start()` abre el stream
//! del dispositivo de entrada por defecto y acumula muestras f32; `stop()`
//! detiene la captura y devuelve las muestras para transcripción.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct AudioCapture {
    samples: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    sample_rate: u32,
}

impl AudioCapture {
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "no hay dispositivo de entrada (micrófono)".to_string())?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("no se pudo abrir la config de entrada: {e}"))?;

        let sample_rate = config.sample_rate().0;
        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let samples_cb = Arc::clone(&samples);
        let stop_cb = Arc::clone(&stop);
        let stop_loop = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            let stream = match build_stream(&device, &config, samples_cb, stop_cb) {
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
            drop(stream);
        });

        Ok(Self { samples, stop, handle: Some(handle), sample_rate })
    }

    /// Detiene la captura y devuelve las muestras f32 mono recogidas.
    pub fn stop(&mut self) -> Vec<f32> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        std::mem::take(&mut *self.samples.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn is_recording(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    let err_fn = |e| eprintln!("[audio] error en el stream: {e}");
    let channels = config.channels() as usize;

    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let stream = device
                .build_input_stream(
                    &config.config(),
                    move |data: &[f32], _| {
                        push_f32(data, channels, &samples);
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
                        push_f32(&buf, channels, &samples);
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
                        push_f32(&buf, channels, &samples);
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

/// Convierte a mono promediando canales y acumula en el buffer compartido.
fn push_f32(data: &[f32], channels: usize, samples: &Arc<Mutex<Vec<f32>>>) {
    if data.is_empty() {
        return;
    }
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