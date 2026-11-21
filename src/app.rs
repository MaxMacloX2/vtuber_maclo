//! Estado principal de la aplicación (`VtuberApp`) que coordina audio, avatar, entrada,
//! configuración externa, animaciones y renderizado de interfaz.

use crate::animation::{
    AnimationContext, AnimationEvent, AnimationPipeline, BounceAnimation, ModelTransform,
};
use crate::audio::{AudioCapture, VoiceActivityDetector, VoiceTransition};
use crate::avatar::{AvatarProfile, AvatarStateId};
use crate::config::{self, AppConfig, ConfigWarning, LoadedConfig};
use crate::input::{InputCommand, InputRouter};
use crate::ui::{self, ControlsPanelProps};
use eframe::egui;
use std::path::PathBuf;
use std::time::Instant;

/// Aplicación principal de `vtuber_maclo` integrada con `eframe::App`.
pub struct VtuberApp {
    config: AppConfig,
    config_path: PathBuf,
    config_warnings: Vec<ConfigWarning>,
    audio_capture: AudioCapture,
    vad: VoiceActivityDetector,
    avatar: AvatarProfile,
    input_router: InputRouter,
    animations: AnimationPipeline,
    show_controls: bool,
    last_state_id: AvatarStateId,
}

impl VtuberApp {
    /// Inicializa todos los subsistemas de la aplicación a partir de la configuración cargada.
    pub fn new(loaded: LoadedConfig) -> Self {
        let mut config_warnings = loaded.warnings;
        let audio_capture = AudioCapture::start_default();
        let vad = VoiceActivityDetector::new(
            loaded.app_config.audio.default_threshold,
            loaded.app_config.audio.hold_duration,
        );

        let (input_router, hotkey_warnings) = InputRouter::new(loaded.bindings);
        for msg in hotkey_warnings {
            config_warnings.push(ConfigWarning::new(msg));
        }

        let animations = build_animation_pipeline(&loaded.app_config);

        Self {
            config: loaded.app_config,
            config_path: loaded.config_path,
            config_warnings,
            audio_capture,
            vad,
            avatar: loaded.avatar_profile,
            input_router,
            animations,
            show_controls: true,
            last_state_id: AvatarStateId::IDLE,
        }
    }

    /// Recarga en caliente `vtuber_maclo.toml`, los sprites externos, animaciones y hotkeys.
    fn reload_config(&mut self, ctx: &egui::Context) {
        let loaded = config::load_or_create_config(&self.config_path);

        // Limpiamos la caché de imágenes en `egui` para reflejar cambios en los PNG del disco.
        ctx.forget_all_images();

        let mut warnings = loaded.warnings;
        let (release_commands, hotkey_warnings) =
            self.input_router.reload_bindings(loaded.bindings);
        for msg in hotkey_warnings {
            warnings.push(ConfigWarning::new(msg));
        }

        self.config = loaded.app_config;
        self.avatar = loaded.avatar_profile;
        self.execute_commands(release_commands, ctx);
        self.config_warnings = warnings;
        self.vad = VoiceActivityDetector::new(
            self.config.audio.default_threshold,
            self.config.audio.hold_duration,
        );
        self.animations = build_animation_pipeline(&self.config);
        self.last_state_id = AvatarStateId::IDLE;

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            self.config.window.title.clone(),
        ));
    }

    /// Ejecuta una lista de comandos provenientes de atajos o botones de la interfaz.
    fn execute_commands(&mut self, commands: Vec<InputCommand>, ctx: &egui::Context) {
        for command in commands {
            match command {
                InputCommand::ToggleControls => {
                    self.show_controls = !self.show_controls;
                }
                InputCommand::SetMomentary { state, active } => {
                    let _ = self.avatar.set_momentary(state, active);
                }
                InputCommand::ToggleExpression(state_id) => {
                    let _ = self.avatar.toggle_override(state_id);
                }
                InputCommand::ClearExpression => {
                    self.avatar.clear_overrides();
                }
                InputCommand::ReloadConfig => {
                    self.reload_config(ctx);
                    // Descartamos el resto del lote porque los comandos posteriores fueron
                    // calculados con los bindings anteriores y podrían dejar estados pegados.
                    return;
                }
            }
        }
    }

    /// Actualiza eventos de audio, detección de voz, notificaciones de cambio y animaciones.
    fn step_audio_and_animation(&mut self, raw_dt: f32) -> (f32, ModelTransform) {
        // Consumir eventos pendientes del hilo de audio para mantener actualizado `status`.
        let _audio_events = self.audio_capture.poll_events();

        let volume = self.audio_capture.current_volume();
        let transition = self.vad.update(volume, Instant::now());

        match transition {
            VoiceTransition::StartedTalking => {
                self.animations.notify(&AnimationEvent::SpeechStarted);
            }
            VoiceTransition::StoppedTalking => {
                self.animations.notify(&AnimationEvent::SpeechEnded);
            }
            VoiceTransition::Unchanged => {}
        }

        // Notificar cambios efectivos de estado visual del avatar (voz o expresión activa).
        let current_state_id = self.avatar.resolve_sprite(self.vad.is_talking()).0.clone();
        if current_state_id != self.last_state_id {
            self.last_state_id = current_state_id.clone();
            self.animations
                .notify(&AnimationEvent::StateChanged(current_state_id));
        }

        let anim_ctx = AnimationContext {
            is_talking: self.vad.is_talking(),
            volume,
        };
        let transform = self.animations.update(raw_dt, &anim_ctx);

        (volume, transform)
    }
}

/// Construye el pipeline de animaciones a partir de la configuración validada.
fn build_animation_pipeline(config: &AppConfig) -> AnimationPipeline {
    let mut animations = AnimationPipeline::new(config.animation.max_dt);
    animations.add_animator(Box::new(BounceAnimation::from_config(&config.animation)));
    animations
}

impl eframe::App for VtuberApp {
    /// Mantiene el fondo completamente transparente para captura directa en OBS.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Procesar atajos locales y globales antes de actualizar audio/UI.
        let input_commands = self.input_router.poll_commands(ctx);
        self.execute_commands(input_commands, ctx);

        // 2. Actualizar detección de voz y física independiente de FPS usando `stable_dt`.
        let raw_dt = ctx.input(|input| input.stable_dt);
        let (volume, transform) = self.step_audio_and_animation(raw_dt);

        // 3. Dibujar el avatar centrado en el panel transparente principal.
        let is_talking = self.vad.is_talking();
        let frame = egui::Frame::none().fill(egui::Color32::TRANSPARENT);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let (_, sprite_source) = self.avatar.resolve_sprite(is_talking);
            ui::draw_avatar(ui, sprite_source, &transform, &self.config.animation);
        });

        // 4. Dibujar controles como `egui::Area` flotante para no desplazar al avatar al alternar `H`.
        if self.show_controls {
            let active_state = self.avatar.resolve_sprite(is_talking).0.clone();
            let ui_commands = ui::draw_controls_window(
                ctx,
                ControlsPanelProps {
                    threshold: self.vad.threshold_mut(),
                    threshold_range: self.config.audio.threshold_range.clone(),
                    volume,
                    audio_status: self.audio_capture.status(),
                    gamepad_status: self.input_router.gamepad_status(),
                    avatar: &self.avatar,
                    active_state: &active_state,
                    input_router: &self.input_router,
                    warnings: &self.config_warnings,
                },
            );
            self.execute_commands(ui_commands, ctx);
        }

        // 5. Mantener refresco continuo para suavidad en animaciones, volumen y hotkeys globales.
        ctx.request_repaint();
    }
}
