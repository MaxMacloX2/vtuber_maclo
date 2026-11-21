//! Captura de audio en tiempo real (`cpal`), cálculo de volumen RMS y detección de voz (VAD).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Errores posibles al inicializar o ejecutar la captura de micrófono.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    /// No se detectó ningún dispositivo de entrada predeterminado en el sistema.
    NoInputDevice,
    /// No fue posible consultar la configuración de entrada del dispositivo.
    InputConfigUnavailable(String),
    /// El formato de muestras del micrófono no está soportado.
    UnsupportedSampleFormat(String),
    /// Falló la creación del stream de entrada en `cpal`.
    StreamBuildFailed(String),
    /// Falló el inicio de reproducción/captura del stream de audio.
    StreamPlayFailed(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputDevice => write!(f, "No hay ningún micrófono predeterminado disponible"),
            Self::InputConfigUnavailable(err) => {
                write!(f, "No se pudo obtener la configuración de entrada: {err}")
            }
            Self::UnsupportedSampleFormat(fmt_name) => {
                write!(f, "Formato de audio no soportado: {fmt_name}")
            }
            Self::StreamBuildFailed(err) => {
                write!(f, "No se pudo construir el stream de audio: {err}")
            }
            Self::StreamPlayFailed(err) => {
                write!(f, "No se pudo iniciar la captura de audio: {err}")
            }
        }
    }
}

impl std::error::Error for AudioError {}

/// Estado observable del subsistema de captura de micrófono para mostrar en la UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioStatus {
    /// Captura activa sobre el dispositivo indicado.
    Active {
        /// Nombre legible del dispositivo de entrada en uso.
        device_name: String,
    },
    /// No se pudo inicializar el micrófono; la aplicación sigue funcionando en silencio.
    Unavailable(String),
    /// Ocurrió un error en tiempo de ejecución dentro del callback de audio.
    StreamError(String),
}

/// Eventos asíncronos enviados desde el hilo de audio hacia la interfaz sin bloquear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioEvent {
    /// El stream reportó un error en tiempo de ejecución.
    StreamError(String),
}

/// Calcula el valor RMS (*Root Mean Square*) a partir de un iterador de muestras `f32`.
///
/// Ignora valores no finitos (`NaN`, `Inf`) para evitar contaminar el cálculo y evita
/// divisiones por cero devolviendo `0.0` cuando no hay muestras válidas.
fn compute_rms_iter<I>(samples: I) -> f32
where
    I: Iterator<Item = f32>,
{
    let mut sum_sq = 0.0_f64;
    let mut valid_count = 0_usize;

    for sample in samples {
        // Descartamos muestras corruptas o no finitas antes de acumular.
        if !sample.is_finite() {
            continue;
        }
        let s = f64::from(sample);
        sum_sq += s * s;
        valid_count += 1;
    }

    if valid_count == 0 {
        return 0.0;
    }

    let rms = (sum_sq / valid_count as f64).sqrt() as f32;
    if rms.is_finite() && rms >= 0.0 {
        rms
    } else {
        0.0
    }
}

/// Calcula el volumen RMS de un bloque de muestras `f32`.
///
/// Ignora muestras `NaN` o infinitas y devuelve `0.0` si el slice está vacío o no contiene
/// muestras finitas válidas.
// Helper público probado en tests unitarios y disponible para buffers f32 directos.
#[allow(dead_code)]
pub fn compute_rms_f32(samples: &[f32]) -> f32 {
    compute_rms_iter(samples.iter().copied())
}

/// Construye un stream de entrada `cpal` genérico sobre el formato de muestra `T`.
///
/// Convierte cada muestra a `f32` sin asignar memoria en el hilo de tiempo real, calcula el RMS
/// y lo publica en `volume_bits` mediante operación atómica relajada. Los errores del stream
/// se envían de forma no bloqueante por `error_tx` usando `try_send`.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    volume_bits: Arc<AtomicU32>,
    error_tx: SyncSender<AudioEvent>,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Convertimos cada muestra al vuelo a f32 para reutilizar compute_rms_iter sin heap.
                let rms =
                    compute_rms_iter(data.iter().map(|&sample| cpal::Sample::to_sample(sample)));
                // Relaxed basta porque solo publicamos la última lectura escalar independiente.
                volume_bits.store(rms.to_bits(), Ordering::Relaxed);
            },
            move |err| {
                // Nunca bloqueamos ni hacemos I/O pesado dentro del callback de audio.
                let _ = error_tx.try_send(AudioEvent::StreamError(err.to_string()));
            },
            None,
        )
        .map_err(|err| AudioError::StreamBuildFailed(err.to_string()))
}

/// Intenta abrir, configurar e iniciar el stream de entrada del micrófono predeterminado.
fn try_start_stream(
    volume_bits: Arc<AtomicU32>,
    event_tx: SyncSender<AudioEvent>,
) -> Result<(cpal::Stream, String), AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(AudioError::NoInputDevice)?;

    // Leemos el nombre del dispositivo con un fallback legible si el driver falla al consultarlo.
    let device_name = device
        .name()
        .unwrap_or_else(|_| "Micrófono predeterminado".to_string());

    let supported_config = device
        .default_input_config()
        .map_err(|err| AudioError::InputConfigUnavailable(err.to_string()))?;

    let sample_format = supported_config.sample_format();
    let stream_config: cpal::StreamConfig = supported_config.into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &stream_config, volume_bits, event_tx)
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &stream_config, volume_bits, event_tx)
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &stream_config, volume_bits, event_tx)
        }
        cpal::SampleFormat::I32 => {
            build_stream::<i32>(&device, &stream_config, volume_bits, event_tx)
        }
        other => {
            return Err(AudioError::UnsupportedSampleFormat(format!("{other:?}")));
        }
    }?;

    stream
        .play()
        .map_err(|err| AudioError::StreamPlayFailed(err.to_string()))?;

    Ok((stream, device_name))
}

/// Gestor de captura de micrófono no bloqueante para la interfaz de usuario.
///
/// Mantiene vivo el stream de `cpal` (cuando está disponible) y expone el último volumen RMS
/// mediante una variable atómica compartida y una cola acotada de eventos.
pub struct AudioCapture {
    _stream: Option<cpal::Stream>,
    volume_bits: Arc<AtomicU32>,
    status: AudioStatus,
    event_rx: Receiver<AudioEvent>,
}

impl AudioCapture {
    /// Inicia la captura sobre el micrófono predeterminado del sistema.
    ///
    /// Nunca entra en pánico si no hay micrófono o si el driver falla; en ese caso devuelve
    /// una instancia válida con estado [`AudioStatus::Unavailable`] y volumen `0.0`.
    pub fn start_default() -> Self {
        let volume_bits = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
        // Canal acotado para evitar acumulación ilimitada si el hilo de UI se detiene.
        let (event_tx, event_rx) = mpsc::sync_channel(8);

        match try_start_stream(Arc::clone(&volume_bits), event_tx) {
            Ok((stream, device_name)) => Self {
                _stream: Some(stream),
                volume_bits,
                status: AudioStatus::Active { device_name },
                event_rx,
            },
            Err(err) => Self {
                _stream: None,
                volume_bits,
                status: AudioStatus::Unavailable(err.to_string()),
                event_rx,
            },
        }
    }

    /// Devuelve el último volumen RMS capturado, sanitizando `NaN`, infinitos y negativos.
    pub fn current_volume(&self) -> f32 {
        let raw = f32::from_bits(self.volume_bits.load(Ordering::Relaxed));
        if raw.is_finite() && raw >= 0.0 {
            raw
        } else {
            0.0
        }
    }

    /// Devuelve el estado actual del micrófono para mostrarlo en los controles.
    pub fn status(&self) -> &AudioStatus {
        &self.status
    }

    /// Lee de forma no bloqueante los eventos pendientes del hilo de audio y actualiza `status`.
    pub fn poll_events(&mut self) -> Vec<AudioEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                AudioEvent::StreamError(msg) => {
                    self.status = AudioStatus::StreamError(msg.clone());
                    // Limpiamos el último volumen para no quedar atascados en "hablando" tras un fallo.
                    self.volume_bits.store(0.0_f32.to_bits(), Ordering::Relaxed);
                }
            }
            events.push(event);
        }
        events
    }
}

/// Resultado de transición emitido por [`VoiceActivityDetector::update`] en cada frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceTransition {
    /// El usuario pasó de silencio a estar hablando.
    StartedTalking,
    /// El usuario dejó de hablar tras superar el tiempo de retención (*hold time*).
    StoppedTalking,
    /// El estado de voz se mantuvo igual que en la actualización anterior.
    Unchanged,
}

/// Detector determinístico de actividad de voz basado en umbral y tiempo de retención.
#[derive(Clone, Debug)]
pub struct VoiceActivityDetector {
    /// Umbral mínimo de volumen RMS para activar el estado de habla.
    threshold: f32,
    /// Tiempo que se mantiene activo el estado de habla después de bajar del umbral.
    hold_duration: Duration,
    /// Estado actual de voz detectada.
    is_talking: bool,
    /// Último instante en el que el volumen superó el umbral.
    last_above_threshold: Option<Instant>,
}

impl VoiceActivityDetector {
    /// Crea un nuevo detector con el umbral y tiempo de retención especificados.
    pub fn new(threshold: f32, hold_duration: Duration) -> Self {
        Self {
            threshold: sanitize_threshold(threshold),
            hold_duration,
            is_talking: false,
            last_above_threshold: None,
        }
    }

    /// Devuelve una referencia mutable al umbral para enlazarlo con el slider de UI.
    pub fn threshold_mut(&mut self) -> &mut f32 {
        &mut self.threshold
    }

    /// Indica si el avatar se encuentra actualmente en estado de habla.
    pub fn is_talking(&self) -> bool {
        self.is_talking
    }

    /// Evalúa un nuevo nivel de volumen en el instante `now` y devuelve si hubo transición.
    ///
    /// Aceptar `now` como parámetro permite evaluar el detector de forma determinística
    /// en pruebas unitarias sin depender de `sleep` reales.
    pub fn update(&mut self, volume: f32, now: Instant) -> VoiceTransition {
        let clean_volume = if volume.is_finite() && volume >= 0.0 {
            volume
        } else {
            0.0
        };
        let threshold = sanitize_threshold(self.threshold);

        if clean_volume > threshold {
            self.last_above_threshold = Some(now);
            if !self.is_talking {
                self.is_talking = true;
                return VoiceTransition::StartedTalking;
            }
            return VoiceTransition::Unchanged;
        }

        if self.is_talking {
            // Mantenemos el estado activo hasta que transcurra hold_duration sin superar umbral.
            let elapsed = self
                .last_above_threshold
                .map(|last| now.saturating_duration_since(last))
                .unwrap_or(self.hold_duration);

            if elapsed >= self.hold_duration {
                self.is_talking = false;
                return VoiceTransition::StoppedTalking;
            }
        }

        VoiceTransition::Unchanged
    }
}

/// Normaliza un umbral de audio para garantizar que sea finito y no negativo.
fn sanitize_threshold(threshold: f32) -> f32 {
    if threshold.is_finite() && threshold >= 0.0 {
        threshold
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_maneja_vacio_y_valores_no_finitos() {
        assert_eq!(compute_rms_f32(&[]), 0.0);
        assert_eq!(
            compute_rms_f32(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]),
            0.0
        );

        // Solo las muestras finitas (0.5 y -0.5) deben entrar en el cálculo => RMS = 0.5.
        let rms = compute_rms_f32(&[f32::NAN, 0.5, f32::INFINITY, -0.5]);
        assert!((rms - 0.5).abs() < 1e-6);
    }

    #[test]
    fn vad_respeta_umbral_y_hold_time() {
        let hold = Duration::from_millis(120);
        let mut vad = VoiceActivityDetector::new(0.05, hold);
        let t0 = Instant::now();

        // Por debajo o igual al umbral no activa el habla.
        assert_eq!(vad.update(0.05, t0), VoiceTransition::Unchanged);
        assert!(!vad.is_talking());

        // Por encima del umbral dispara StartedTalking.
        assert_eq!(
            vad.update(0.10, t0 + Duration::from_millis(10)),
            VoiceTransition::StartedTalking
        );
        assert!(vad.is_talking());

        // Cae por debajo del umbral pero dentro de los 120ms de retención: sigue hablando.
        assert_eq!(
            vad.update(0.01, t0 + Duration::from_millis(100)),
            VoiceTransition::Unchanged
        );
        assert!(vad.is_talking());

        // A los 129ms desde t0+10ms (119ms de silencio) aún mantiene el estado activo.
        assert_eq!(
            vad.update(0.01, t0 + Duration::from_millis(129)),
            VoiceTransition::Unchanged
        );
        assert!(vad.is_talking());

        // Al cumplirse los 120ms de silencio (t0 + 130ms) transiciona a StoppedTalking.
        assert_eq!(
            vad.update(0.01, t0 + Duration::from_millis(130)),
            VoiceTransition::StoppedTalking
        );
        assert!(!vad.is_talking());

        // Valores no finitos nunca deben disparar habla.
        assert_eq!(
            vad.update(f32::NAN, t0 + Duration::from_millis(140)),
            VoiceTransition::Unchanged
        );
        assert!(!vad.is_talking());
    }

    #[test]
    fn stream_error_resetea_volumen_a_cero() {
        let (tx, rx) = mpsc::sync_channel(4);
        let volume_bits = Arc::new(AtomicU32::new(0.25_f32.to_bits()));
        let mut capture = AudioCapture {
            _stream: None,
            volume_bits,
            status: AudioStatus::Active {
                device_name: "Mic prueba".to_string(),
            },
            event_rx: rx,
        };

        assert!((capture.current_volume() - 0.25).abs() < 1e-6);
        tx.send(AudioEvent::StreamError("Desconectado".to_string()))
            .expect("el canal debe aceptar el evento");

        let events = capture.poll_events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            capture.status(),
            &AudioStatus::StreamError("Desconectado".to_string())
        );
        assert_eq!(capture.current_volume(), 0.0);
    }
}
