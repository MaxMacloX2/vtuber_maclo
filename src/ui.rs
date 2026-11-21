//! Renderizado de la interfaz flotante de controles y del sprite del avatar en `egui`.

use crate::animation::ModelTransform;
use crate::audio::AudioStatus;
use crate::avatar::{ActivationMode, AvatarProfile, AvatarStateId, SpriteSource};
use crate::config::{AnimationConfig, ConfigWarning};
use crate::input::{GamepadStatus, InputAction, InputCommand, InputRouter, TriggerHint};
use eframe::egui;
use std::borrow::Cow;
use std::ops::RangeInclusive;
use std::sync::Arc;

/// Datos necesarios para renderizar el panel flotante de controles sin acoplarlo a `VtuberApp`.
pub struct ControlsPanelProps<'a> {
    /// Umbral mutable de actividad de voz enlazado al slider.
    pub threshold: &'a mut f32,
    /// Rango permitido para el slider del umbral.
    pub threshold_range: RangeInclusive<f32>,
    /// Nivel actual de volumen RMS capturado.
    pub volume: f32,
    /// Estado actual del dispositivo de micrófono.
    pub audio_status: &'a AudioStatus,
    /// Estado actual de los mandos externos (`gamepad`).
    pub gamepad_status: &'a GamepadStatus,
    /// Perfil del avatar para listar expresiones en orden determinístico.
    pub avatar: &'a AvatarProfile,
    /// Estado visual efectivo que se está dibujando en este frame.
    pub active_state: &'a AvatarStateId,
    /// Enrutador de entrada para consultar las teclas configuradas de cada acción.
    pub input_router: &'a InputRouter,
    /// Advertencias acumuladas al cargar la configuración o registrar hotkeys.
    pub warnings: &'a [ConfigWarning],
}

/// Dibuja el panel flotante de controles sobre un `egui::Area` independiente del layout del avatar.
///
/// Usar `egui::Area` evita desplazar el sprite del avatar cuando se muestra u oculta el panel
/// con la tecla configurada (`H` por defecto). Devuelve comandos de UI (clic en expresiones,
/// botón neutro o recarga de configuración).
pub fn draw_controls_window(
    ctx: &egui::Context,
    props: ControlsPanelProps<'_>,
) -> Vec<InputCommand> {
    let mut commands = Vec::new();

    egui::Area::new(egui::Id::new("vtuber_controls_area"))
        .default_pos(egui::pos2(12.0, 12.0))
        .movable(true)
        .show(ctx, |ui| {
            egui::Frame::window(ctx.style().as_ref()).show(ui, |ui| {
                ui.set_max_width(340.0);

                ui.horizontal(|ui| {
                    ui.label("Umbral:");
                    ui.add(egui::Slider::new(props.threshold, props.threshold_range));
                });

                let hide_hint = props
                    .input_router
                    .action_hint(&InputAction::ToggleControls)
                    .map(|hint| format!("Atajo {hint}: ocultar UI"))
                    .unwrap_or_else(|| "Panel de controles".to_string());
                ui.label(format!("Volumen actual: {:.4} ({hide_hint})", props.volume));

                // Indicador claro del estado del micrófono sin bloquear ni ocultar fallos.
                let (mic_label, mic_color) = format_audio_status(props.audio_status);
                ui.colored_label(mic_color, mic_label);

                // Indicador claro del estado del control externo (gamepad).
                let (gp_label, gp_color) = format_gamepad_status(props.gamepad_status);
                ui.colored_label(gp_color, gp_label);

                let override_suffix = format_override_suffix(props.avatar);
                ui.label(format!(
                    "Estado avatar: {}{}",
                    props.active_state, override_suffix
                ));

                ui.separator();
                ui.label("Expresiones:");
                ui.horizontal_wrapped(|ui| {
                    for (state_id, sprites) in props.avatar.ordered_states() {
                        // Omitimos `idle` y `talk` de los chips porque responden automáticamente al micrófono
                        // y se recuperan con el botón "Neutro".
                        if *state_id == AvatarStateId::IDLE || *state_id == AvatarStateId::TALK {
                            continue;
                        }

                        let is_toggled = props.avatar.manual_toggle() == Some(state_id);
                        let is_momentary = props.avatar.momentary_active() == Some(state_id);
                        let label = format_expression_chip_label(
                            &sprites.display_name,
                            props.input_router.expression_hint(state_id),
                            is_momentary,
                        );
                        let chip_text = if is_momentary {
                            // Destacamos el estado sostenido momentáneamente para distinguirlo del toggle fijado.
                            egui::RichText::new(label)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 215, 100))
                        } else {
                            egui::RichText::new(label)
                        };

                        if ui.selectable_label(is_toggled, chip_text).clicked() {
                            commands.push(InputCommand::ToggleExpression(state_id.clone()));
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let clear_label = props
                        .input_router
                        .action_hint(&InputAction::ClearExpression)
                        .map(|hint| format!("Neutro ({hint})"))
                        .unwrap_or_else(|| "Neutro".to_string());

                    let is_neutral = props.avatar.active_override().is_none();
                    if ui.selectable_label(is_neutral, clear_label).clicked() {
                        commands.push(InputCommand::ClearExpression);
                    }

                    if ui.button("Recargar config").clicked() {
                        commands.push(InputCommand::ReloadConfig);
                    }
                });

                if !props.warnings.is_empty() {
                    ui.separator();
                    draw_warnings_summary(ui, props.warnings);
                }
            });
        });

    commands
}

/// Dibuja el sprite del avatar aplicando la transformación completa (`ModelTransform`).
///
/// Aplica `offset_x`, `offset_y`, `scale_x`, `scale_y` y `rotation_rad` sin recortar valores
/// negativos y sin realizar lectura síncrona de archivos en el hilo de renderizado.
pub fn draw_avatar(
    ui: &mut egui::Ui,
    source: &SpriteSource,
    transform: &ModelTransform,
    config: &AnimationConfig,
) {
    let avail = ui.available_rect_before_wrap();
    let image_source = resolve_image_source(source);

    // Calculamos el tamaño base preservando la relación de aspecto antes de aplicar squash & stretch.
    let sizing_image = egui::Image::new(image_source.clone())
        .max_width(config.avatar_max_width)
        .maintain_aspect_ratio(true);

    let base_size = sizing_image
        .load_and_calc_size(
            ui,
            egui::vec2(config.avatar_max_width, avail.height().max(1.0)),
        )
        .unwrap_or_else(|| egui::vec2(config.avatar_max_width, config.avatar_max_width));

    let safe_scale_x = if transform.scale_x.is_finite() {
        transform.scale_x.max(0.05)
    } else {
        1.0
    };
    let safe_scale_y = if transform.scale_y.is_finite() {
        transform.scale_y.max(0.05)
    } else {
        1.0
    };
    let final_size = egui::vec2(base_size.x * safe_scale_x, base_size.y * safe_scale_y);

    let offset_x = if transform.offset_x.is_finite() {
        transform.offset_x
    } else {
        0.0
    };
    let offset_y = if transform.offset_y.is_finite() {
        transform.offset_y
    } else {
        0.0
    };

    // Calculamos el centro base y sumamos los desplazamientos libremente para no recortar impulsos negativos.
    let base_center = egui::pos2(
        avail.center().x,
        avail.top() + config.avatar_base_top_space + base_size.y * 0.5,
    );
    let target_rect = egui::Rect::from_center_size(
        egui::pos2(base_center.x + offset_x, base_center.y + offset_y),
        final_size,
    );

    let mut paint_image = egui::Image::new(image_source);
    if transform.rotation_rad.is_finite() && transform.rotation_rad.abs() > f32::EPSILON {
        paint_image = paint_image.rotate(transform.rotation_rad, egui::Vec2::splat(0.5));
    }

    paint_image.paint_at(ui, target_rect);
}

/// Devuelve el sufijo descriptivo del override activo para la etiqueta de estado del avatar.
fn format_override_suffix(avatar: &AvatarProfile) -> &'static str {
    if avatar.momentary_active().is_some() {
        " (mant.)"
    } else if avatar.manual_toggle().is_some() {
        " (manual)"
    } else {
        ""
    }
}

/// Construye la etiqueta de un chip de expresión mostrando su atajo, modo y si está sostenido.
fn format_expression_chip_label(
    display_name: &str,
    hint: Option<TriggerHint>,
    is_momentary_active: bool,
) -> String {
    let base = match hint {
        Some(TriggerHint {
            label,
            mode: ActivationMode::Toggle,
        }) => {
            format!("{display_name} [{label} · toggle]")
        }
        Some(TriggerHint {
            label,
            mode: ActivationMode::Momentary,
        }) => {
            format!("{display_name} [{label} · mant.]")
        }
        None => display_name.to_string(),
    };
    if is_momentary_active {
        format!("{base} (mant.)")
    } else {
        base
    }
}

/// Muestra un resumen compacto de advertencias de configuración para no saturar la ventana.
fn draw_warnings_summary(ui: &mut egui::Ui, warnings: &[ConfigWarning]) {
    const MAX_VISIBLE_WARNINGS: usize = 4;

    ui.colored_label(
        egui::Color32::YELLOW,
        format!("Advertencias ({}):", warnings.len()),
    );
    for warning in warnings.iter().take(MAX_VISIBLE_WARNINGS) {
        ui.label(
            egui::RichText::new(format!("• {}", warning.message))
                .small()
                .color(egui::Color32::YELLOW),
        );
    }
    if warnings.len() > MAX_VISIBLE_WARNINGS {
        ui.label(
            egui::RichText::new(format!(
                "… y {} advertencia(s) más.",
                warnings.len() - MAX_VISIBLE_WARNINGS
            ))
            .small()
            .color(egui::Color32::YELLOW),
        );
    }
}

/// Formatea el estado del micrófono y elige un color visual acorde para la UI.
fn format_audio_status(status: &AudioStatus) -> (String, egui::Color32) {
    match status {
        AudioStatus::Active { device_name } => (
            format!("Micrófono: {device_name}"),
            egui::Color32::LIGHT_GREEN,
        ),
        AudioStatus::Unavailable(reason) => (
            format!("Micrófono no disponible: {reason}"),
            egui::Color32::YELLOW,
        ),
        AudioStatus::StreamError(err) => {
            (format!("Error de audio: {err}"), egui::Color32::LIGHT_RED)
        }
    }
}

/// Formatea el estado de conexión de mandos externos (`gamepad`) y elige un color acorde.
fn format_gamepad_status(status: &GamepadStatus) -> (String, egui::Color32) {
    match status {
        GamepadStatus::Connected(devices) => {
            if devices.len() == 1 {
                let first = &devices[0];
                (
                    format!("Mando P{}: {}", first.index + 1, first.name),
                    egui::Color32::LIGHT_GREEN,
                )
            } else {
                let summary = devices
                    .iter()
                    .map(|d| format!("P{}: {}", d.index + 1, d.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    format!("Mandos ({}): {summary}", devices.len()),
                    egui::Color32::LIGHT_GREEN,
                )
            }
        }
        GamepadStatus::NoControllers => {
            ("Mando: ninguno conectado".to_string(), egui::Color32::GRAY)
        }
        GamepadStatus::Unavailable(reason) => (
            format!("Mando no disponible: {reason}"),
            egui::Color32::YELLOW,
        ),
    }
}

/// Convierte un [`SpriteSource`] precargado en memoria en un `egui::ImageSource` sin E/S en disco.
fn resolve_image_source(source: &SpriteSource) -> egui::ImageSource<'static> {
    match source {
        SpriteSource::Embedded(image_source) => image_source.clone(),
        SpriteSource::Bytes { uri, bytes } => egui::ImageSource::Bytes {
            uri: Cow::Owned(uri.clone()),
            bytes: egui::load::Bytes::Shared(Arc::clone(bytes)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{ConnectedGamepadInfo, KeyCode, KeyTrigger};

    #[test]
    fn distingue_sufijo_manual_y_momentaneo_en_ui() {
        let mut avatar = AvatarProfile::builtin_default();
        assert_eq!(format_override_suffix(&avatar), "");

        assert_eq!(avatar.toggle_override(AvatarStateId::HAPPY), Ok(true));
        assert_eq!(format_override_suffix(&avatar), " (manual)");

        assert_eq!(avatar.set_momentary(AvatarStateId::SURPRISED, true), Ok(()));
        assert_eq!(format_override_suffix(&avatar), " (mant.)");

        let chip = format_expression_chip_label(
            "Sorprendido",
            Some(TriggerHint {
                label: KeyTrigger::key_only(KeyCode::Num2).format_short(),
                mode: ActivationMode::Momentary,
            }),
            true,
        );
        assert!(chip.ends_with(" (mant.)"));
    }

    #[test]
    fn formatea_estado_de_gamepad_y_chip_combo_gamepad() {
        let chip = format_expression_chip_label(
            "Sorprendido",
            Some(TriggerHint {
                label: "LB+Y".to_string(),
                mode: ActivationMode::Momentary,
            }),
            false,
        );
        assert_eq!(chip, "Sorprendido [LB+Y · mant.]");

        let (no_ctrl, color_none) = format_gamepad_status(&GamepadStatus::NoControllers);
        assert_eq!(no_ctrl, "Mando: ninguno conectado");
        assert_eq!(color_none, egui::Color32::GRAY);

        let (one_ctrl, color_one) =
            format_gamepad_status(&GamepadStatus::Connected(vec![ConnectedGamepadInfo {
                index: 0,
                name: "Xbox Controller".to_string(),
            }]));
        assert_eq!(one_ctrl, "Mando P1: Xbox Controller");
        assert_eq!(color_one, egui::Color32::LIGHT_GREEN);

        let (unavail, color_err) =
            format_gamepad_status(&GamepadStatus::Unavailable("sin permiso".to_string()));
        assert_eq!(unavail, "Mando no disponible: sin permiso");
        assert_eq!(color_err, egui::Color32::YELLOW);
    }
}
