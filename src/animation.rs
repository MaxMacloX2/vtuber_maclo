//! Sistema de animaciones independientes de FPS (`dt`) y transformaciones del modelo.

use crate::avatar::AvatarStateId;
use crate::config::AnimationConfig;

/// Limpia y acota el delta de tiempo (`dt`) entre frames.
///
/// Rechaza `NaN`, infinitos y valores negativos devolviendo `0.0`, y limita picos altos
/// (por ejemplo tras minimizar o mover la ventana) a `max_dt` para mantener estabilidad física.
pub fn sanitize_dt(raw_dt: f32, max_dt: f32) -> f32 {
    if !raw_dt.is_finite() || raw_dt <= 0.0 {
        return 0.0;
    }
    if !max_dt.is_finite() || max_dt <= 0.0 {
        return 0.0;
    }
    raw_dt.min(max_dt)
}

/// Amortiguamiento exponencial independiente de la tasa de cuadros (FPS).
///
/// A diferencia de `current + (target - current) * k`, usar `1.0 - (-speed * dt).exp()`
/// garantiza que la trayectoria de convergencia tras un tiempo `T` sea idéntica a 30, 60 o 144 FPS.
pub fn exp_damp(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    let safe_target = if target.is_finite() { target } else { 0.0 };
    let safe_current = if current.is_finite() {
        current
    } else {
        safe_target
    };

    if !dt.is_finite() || dt <= 0.0 || !speed.is_finite() || speed <= 0.0 {
        return safe_current;
    }

    let factor = 1.0 - (-speed * dt).exp();
    let result = safe_current + (safe_target - safe_current) * factor;
    if result.is_finite() {
        result
    } else {
        safe_target
    }
}

/// Transformación espacial acumulada que las animaciones aplican sobre el sprite del avatar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelTransform {
    /// Desplazamiento horizontal en puntos lógicos.
    pub offset_x: f32,
    /// Desplazamiento vertical en puntos lógicos (negativo = hacia arriba).
    pub offset_y: f32,
    /// Factor de escala horizontal (`1.0` = tamaño base).
    pub scale_x: f32,
    /// Factor de escala vertical (`1.0` = tamaño base).
    pub scale_y: f32,
    /// Rotación en radianes alrededor del centro visual.
    pub rotation_rad: f32,
}

impl Default for ModelTransform {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_rad: 0.0,
        }
    }
}

impl ModelTransform {
    /// Combina aditivamente desplazamientos/rotación y multiplicativamente escalas.
    pub fn combine(&self, other: &Self) -> Self {
        Self {
            offset_x: self.offset_x + other.offset_x,
            offset_y: self.offset_y + other.offset_y,
            scale_x: self.scale_x * other.scale_x,
            scale_y: self.scale_y * other.scale_y,
            rotation_rad: self.rotation_rad + other.rotation_rad,
        }
    }
}

/// Eventos discretos que pueden disparar reacciones dentro de los animadores del modelo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnimationEvent {
    /// El detector de voz pasó de silencio a habla activa.
    SpeechStarted,
    /// El detector de voz volvió a silencio tras expirar el *hold time*.
    SpeechEnded,
    /// Cambió el estado visual activo del avatar (por voz o por override manual).
    StateChanged(AvatarStateId),
}

/// Contexto continuo por frame entregado a cada animador durante su actualización.
#[derive(Clone, Copy, Debug)]
pub struct AnimationContext {
    /// Indica si el avatar está actualmente hablando.
    // Campo reservado para animadores reactivos a voz continua (p. ej. sacudida o respiración).
    #[allow(dead_code)]
    pub is_talking: bool,
    /// Nivel de volumen RMS actual del micrófono.
    #[allow(dead_code)]
    pub volume: f32,
}

/// Interfaz extensible para animaciones modulares del avatar (rebote, parpadeo, sacudida, etc.).
pub trait ModelAnimator: Send {
    /// Recibe eventos discretos del ciclo de voz o cambios de expresión.
    fn on_event(&mut self, _event: &AnimationEvent) {}

    /// Avanza la animación `dt` segundos (ya sanitizados) y retorna su transformación aportada.
    fn update(&mut self, dt: f32, ctx: &AnimationContext) -> ModelTransform;
}

/// Animación de rebote vertical al iniciar el habla con retorno suave por amortiguamiento exponencial.
#[derive(Clone, Debug)]
pub struct BounceAnimation {
    offset_y: f32,
    impulse_y: f32,
    damping_speed: f32,
}

impl BounceAnimation {
    /// Crea una animación de rebote con el impulso vertical y velocidad de retorno indicados.
    pub fn new(impulse_y: f32, damping_speed: f32) -> Self {
        Self {
            offset_y: 0.0,
            impulse_y,
            damping_speed,
        }
    }

    /// Crea la animación usando los parámetros definidos en [`AnimationConfig`].
    pub fn from_config(config: &AnimationConfig) -> Self {
        Self::new(config.bounce_impulse_y, config.bounce_damping_speed)
    }
}

impl ModelAnimator for BounceAnimation {
    fn on_event(&mut self, event: &AnimationEvent) {
        if matches!(event, AnimationEvent::SpeechStarted) {
            // Aplicamos un impulso instantáneo hacia arriba (eje Y negativo en egui).
            self.offset_y = self.impulse_y;
        }
    }

    fn update(&mut self, dt: f32, _ctx: &AnimationContext) -> ModelTransform {
        self.offset_y = exp_damp(self.offset_y, 0.0, self.damping_speed, dt);
        ModelTransform {
            offset_y: self.offset_y,
            ..ModelTransform::default()
        }
    }
}

/// Orquestador que ejecuta en cadena todos los animadores activos y combina sus transformaciones.
pub struct AnimationPipeline {
    animators: Vec<Box<dyn ModelAnimator>>,
    max_dt: f32,
}

impl AnimationPipeline {
    /// Crea un pipeline vacío con el límite máximo de `dt` especificado.
    pub fn new(max_dt: f32) -> Self {
        Self {
            animators: Vec::new(),
            max_dt,
        }
    }

    /// Registra un nuevo animador en el pipeline.
    pub fn add_animator(&mut self, animator: Box<dyn ModelAnimator>) {
        self.animators.push(animator);
    }

    /// Propaga un evento de animación a todos los animadores registrados.
    pub fn notify(&mut self, event: &AnimationEvent) {
        for animator in &mut self.animators {
            animator.on_event(event);
        }
    }

    /// Avanza todas las animaciones con `dt` sanitizado y devuelve la transformación combinada.
    pub fn update(&mut self, raw_dt: f32, ctx: &AnimationContext) -> ModelTransform {
        let dt = sanitize_dt(raw_dt, self.max_dt);
        let mut combined = ModelTransform::default();
        for animator in &mut self.animators {
            combined = combined.combine(&animator.update(dt, ctx));
        }
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_dt_limita_picos_y_nan() {
        assert_eq!(sanitize_dt(f32::NAN, 0.05), 0.0);
        assert_eq!(sanitize_dt(f32::INFINITY, 0.05), 0.0);
        assert_eq!(sanitize_dt(-0.016, 0.05), 0.0);
        assert_eq!(sanitize_dt(0.0, 0.05), 0.0);
        assert_eq!(sanitize_dt(0.016, 0.05), 0.016);
        assert_eq!(sanitize_dt(1.5, 0.05), 0.05);
        assert_eq!(sanitize_dt(0.016, f32::NAN), 0.0);
    }

    #[test]
    fn exp_damp_es_consistente_a_distintos_fps() {
        // Simulamos exactamente 0.5 segundos a 30 FPS (15 pasos) y a 120 FPS (60 pasos).
        let speed = 13.5;
        let total_time = 0.5_f32;

        let mut val_30fps = -18.0_f32;
        let dt_30 = total_time / 15.0;
        for _ in 0..15 {
            val_30fps = exp_damp(val_30fps, 0.0, speed, dt_30);
        }

        let mut val_120fps = -18.0_f32;
        let dt_120 = total_time / 60.0;
        for _ in 0..60 {
            val_120fps = exp_damp(val_120fps, 0.0, speed, dt_120);
        }

        assert!((val_30fps - val_120fps).abs() < 1e-4);
    }

    #[test]
    fn bounce_aplica_impulso_y_vuelve_a_cero() {
        let mut pipeline = AnimationPipeline::new(0.05);
        pipeline.add_animator(Box::new(BounceAnimation::new(-18.0, 13.5)));

        let ctx = AnimationContext {
            is_talking: true,
            volume: 0.1,
        };

        // En reposo inicial, el desplazamiento es 0.
        let initial = pipeline.update(0.016, &ctx);
        assert_eq!(initial.offset_y, 0.0);

        // Al iniciar el habla, se aplica el impulso negativo y comienza a regresar hacia 0.
        pipeline.notify(&AnimationEvent::SpeechStarted);
        let first_frame = pipeline.update(0.016, &ctx);
        assert!(first_frame.offset_y < -10.0);
        assert!(first_frame.offset_y > -18.0);

        // Simulando ~1 segundo adicional de frames, el rebote converge casi por completo a 0.
        let mut current = first_frame;
        for _ in 0..60 {
            current = pipeline.update(1.0 / 60.0, &ctx);
        }
        assert!(current.offset_y.abs() < 0.01);

        // SpeechEnded y StateChanged no rompen ni alteran el estado de reposo.
        pipeline.notify(&AnimationEvent::SpeechEnded);
        pipeline.notify(&AnimationEvent::StateChanged(AvatarStateId::IDLE));
    }

    #[test]
    fn exp_damp_no_propaga_nan_ni_infinitos() {
        assert_eq!(exp_damp(f32::NAN, f32::NAN, 10.0, 0.016), 0.0);
        assert_eq!(exp_damp(f32::INFINITY, 5.0, 10.0, 0.016), 5.0);
        assert_eq!(
            exp_damp(2.0, f32::NEG_INFINITY, 10.0, 0.016),
            2.0 * (-0.16_f32).exp()
        );
        assert_eq!(exp_damp(f32::NAN, 3.0, f32::NAN, 0.016), 3.0);
    }
}
