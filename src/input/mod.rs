//! Sistema extensible de disparadores (*triggers*), backends de entrada (teclado local/global,
//! combos multi-tecla y mando externo) y enrutamiento de comandos hacia la aplicación.

pub mod chord;
pub mod gamepad;
pub mod keyboard;

#[allow(unused_imports)]
pub use chord::{AnalogHysteresis, Chord, ChordTracker, ChordTransition, TriggerPhase};
#[allow(unused_imports)]
pub use gamepad::{
    ConnectedGamepadInfo, GamepadBackend, GamepadControl, GamepadParseError, GamepadStatus,
    GamepadTrigger,
};
#[allow(unused_imports)]
pub use keyboard::{
    EguiKeyboardBackend, GlobalHotkeyBackend, HotkeyScope, KeyChord, KeyCode, KeyModifiers,
    KeyParseError, KeyTrigger, WindowKeyboardTrigger,
};

use crate::avatar::{ActivationMode, AvatarStateId};
use eframe::egui;
use std::collections::{HashMap, HashSet};

/// Fuente extensible de un evento de disparo (teclado simple, combo multi-tecla o mando externo).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TriggerSource {
    /// Combinación de teclado con una tecla principal y modificadores en ámbito `Window` o `Global`.
    Keyboard {
        /// Tecla y modificadores del disparador.
        trigger: KeyTrigger,
        /// Ámbito de escucha (`Global` o `Window`).
        scope: HotkeyScope,
    },
    /// Combinación multi-tecla de teclado (`KeyChord`) asociada a ámbito de ventana.
    KeyboardChord {
        /// Modificadores y teclas principales simultáneas.
        chord: KeyChord,
        /// Ámbito solicitado (los combos multi-tecla se degradan a `Window` si piden `Global`).
        scope: HotkeyScope,
    },
    /// Botón, gatillo, dirección o combo de un mando externo (`Gamepad`).
    Gamepad(GamepadTrigger),
}

impl TriggerSource {
    /// Formatea la fuente de entrada de forma compacta para mostrar en etiquetas y chips de UI.
    pub fn format_short(&self) -> String {
        match self {
            Self::Keyboard { trigger, .. } => trigger.format_short(),
            Self::KeyboardChord { chord, .. } => chord.format_short(),
            Self::Gamepad(trigger) => trigger.format_short(),
        }
    }
}

/// Sugerencia de atajo y modo de activación para mostrar en los chips de expresión de la interfaz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerHint {
    /// Etiqueta corta del atajo o botón (por ejemplo `"1"`, `"A+B"` o `"LB+Y"`).
    pub label: String,
    /// Modo de activación configurado (`Toggle` o `Momentary`).
    pub mode: ActivationMode,
}

/// Evento normalizado emitido por un [`TriggerBackend`] hacia el [`InputRouter`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerEvent {
    /// Origen que produjo el disparo.
    pub source: TriggerSource,
    /// Fase de pulsación o liberación.
    pub phase: TriggerPhase,
}

/// Acción declarativa configurada para un enlace de entrada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    /// Muestra u oculta el panel flotante de controles.
    ToggleControls,
    /// Activa o mantiene una expresión del avatar.
    ActivateExpression(AvatarStateId),
    /// Limpia cualquier expresión activa para volver al estado reactivo por voz.
    ClearExpression,
    /// Recarga la configuración externa desde disco.
    ReloadConfig,
}

/// Enlace completo entre una fuente de disparo, un modo de activación y una acción.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputBinding {
    /// Disparador físico/lógico y su ámbito.
    pub source: TriggerSource,
    /// Acción a ejecutar cuando el disparador se active.
    pub action: InputAction,
    /// Modo de activación (`toggle` o `momentary`).
    pub mode: ActivationMode,
}

/// Comando imperativo listo para ejecutarse sobre `VtuberApp` y `AvatarProfile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputCommand {
    /// Alterna la visibilidad de los controles flotantes.
    ToggleControls,
    /// Activa o desactiva una expresión momentánea en la pila del avatar.
    SetMomentary {
        /// Estado de expresión objetivo.
        state: AvatarStateId,
        /// `true` al pulsar, `false` al soltar.
        active: bool,
    },
    /// Alterna una expresión fija mediante `toggle_override`.
    ToggleExpression(AvatarStateId),
    /// Limpia todas las expresiones activas.
    ClearExpression,
    /// Solicita recargar el archivo `vtuber_maclo.toml` y sus sprites.
    ReloadConfig,
}

/// Devuelve los enlaces de teclado predeterminados de `vtuber_maclo`.
///
/// - `H` (`window`, `toggle`) -> alternar controles
/// - `1` (`global`, `toggle`) -> expresión `happy`
/// - `2` (`global`, `momentary`) -> expresión `surprised`
/// - `Escape` (`window`, `toggle`) -> limpiar expresión
pub fn default_bindings() -> Vec<InputBinding> {
    vec![
        InputBinding {
            source: TriggerSource::Keyboard {
                trigger: KeyTrigger::key_only(KeyCode::H),
                scope: HotkeyScope::Window,
            },
            action: InputAction::ToggleControls,
            mode: ActivationMode::Toggle,
        },
        InputBinding {
            source: TriggerSource::Keyboard {
                trigger: KeyTrigger::key_only(KeyCode::Num1),
                scope: HotkeyScope::Global,
            },
            action: InputAction::ActivateExpression(AvatarStateId::HAPPY),
            mode: ActivationMode::Toggle,
        },
        InputBinding {
            source: TriggerSource::Keyboard {
                trigger: KeyTrigger::key_only(KeyCode::Num2),
                scope: HotkeyScope::Global,
            },
            action: InputAction::ActivateExpression(AvatarStateId::SURPRISED),
            mode: ActivationMode::Momentary,
        },
        InputBinding {
            source: TriggerSource::Keyboard {
                trigger: KeyTrigger::key_only(KeyCode::Escape),
                scope: HotkeyScope::Window,
            },
            action: InputAction::ClearExpression,
            mode: ActivationMode::Toggle,
        },
    ]
}

/// Contrato común para cualquier fuente de eventos de disparo no bloqueante.
pub trait TriggerBackend {
    /// Recoge los eventos producidos en el frame actual sin bloquear el hilo de UI.
    fn poll_events(&mut self, ctx: &egui::Context) -> Vec<TriggerEvent>;
}

/// Orquestador que garantiza propiedad exclusiva de cada binding entre backends y traduce
/// [`TriggerEvent`] en [`InputCommand`] según el [`ActivationMode`] configurado.
pub struct InputRouter {
    bindings: Vec<InputBinding>,
    egui_backend: EguiKeyboardBackend,
    global_backend: GlobalHotkeyBackend,
    gamepad_backend: GamepadBackend,
    /// Fuentes momentáneas activas por expresión para evitar apagados prematuros si hay múltiples disparadores.
    active_momentary_sources: HashMap<AvatarStateId, HashSet<TriggerSource>>,
}

impl InputRouter {
    /// Construye el enrutador asignando cada binding a su backend (`Global`, `Window` o `Gamepad`).
    ///
    /// Si algún atajo global falla al registrarse en el sistema operativo o si un combo multi-tecla
    /// solicita `HotkeyScope::Global`, degrada el binding a `HotkeyScope::Window` y devuelve avisos.
    pub fn new(raw_bindings: Vec<InputBinding>) -> (Self, Vec<String>) {
        let (normalized_bindings, mut warnings) = degrade_global_keyboard_chords(raw_bindings);

        let requested_globals = collect_global_triggers(&normalized_bindings);
        let (global_backend, registered_globals, global_warnings) =
            GlobalHotkeyBackend::new(&requested_globals);
        warnings.extend(global_warnings);

        let effective_bindings =
            apply_global_registration(normalized_bindings, &registered_globals);
        let window_triggers = collect_window_triggers(&effective_bindings);
        let egui_backend = EguiKeyboardBackend::new(window_triggers);

        let gamepad_triggers = collect_gamepad_triggers(&effective_bindings);
        let (gamepad_backend, gamepad_warnings) = GamepadBackend::new(gamepad_triggers);
        warnings.extend(gamepad_warnings);

        (
            Self {
                bindings: effective_bindings,
                egui_backend,
                global_backend,
                gamepad_backend,
                active_momentary_sources: HashMap::new(),
            },
            warnings,
        )
    }

    /// Reemplaza todos los enlaces tras recargar la configuración externa sin recrear los managers.
    ///
    /// Devuelve `(comandos_de_liberacion, advertencias)` para apagar cualquier expresión momentánea
    /// que estuviera activa antes de la recarga.
    pub fn reload_bindings(
        &mut self,
        raw_bindings: Vec<InputBinding>,
    ) -> (Vec<InputCommand>, Vec<String>) {
        // Emitimos `SetMomentary { active: false }` en orden determinístico para los estados activos previos.
        let mut active_states: Vec<AvatarStateId> =
            self.active_momentary_sources.keys().cloned().collect();
        active_states.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let release_commands = active_states
            .into_iter()
            .map(|state| InputCommand::SetMomentary {
                state,
                active: false,
            })
            .collect();
        self.active_momentary_sources.clear();

        let (normalized_bindings, mut warnings) = degrade_global_keyboard_chords(raw_bindings);
        let requested_globals = collect_global_triggers(&normalized_bindings);
        let (registered_globals, global_warnings) =
            self.global_backend.sync_triggers(&requested_globals);
        warnings.extend(global_warnings);

        self.bindings = apply_global_registration(normalized_bindings, &registered_globals);
        self.egui_backend
            .set_triggers(collect_window_triggers(&self.bindings));
        self.gamepad_backend
            .set_triggers(collect_gamepad_triggers(&self.bindings));

        (release_commands, warnings)
    }

    /// Devuelve el estado actual de conexión del subsistema de mandos externos.
    pub fn gamepad_status(&self) -> &GamepadStatus {
        self.gamepad_backend.status()
    }

    /// Busca el primer disparador configurado para activar la expresión `state`, útil para mostrar en UI.
    pub fn expression_hint(&self, state: &AvatarStateId) -> Option<TriggerHint> {
        self.bindings.iter().find_map(|binding| {
            let InputAction::ActivateExpression(target) = &binding.action else {
                return None;
            };
            if target != state {
                return None;
            }
            Some(TriggerHint {
                label: binding.source.format_short(),
                mode: binding.mode,
            })
        })
    }

    /// Devuelve la etiqueta corta del primer disparador configurado para una acción simple.
    pub fn action_hint(&self, target_action: &InputAction) -> Option<String> {
        self.bindings.iter().find_map(|binding| {
            if &binding.action != target_action {
                return None;
            }
            Some(binding.source.format_short())
        })
    }

    /// Consulta todos los backends sin bloquear y traduce los eventos del frame a comandos.
    pub fn poll_commands(&mut self, ctx: &egui::Context) -> Vec<InputCommand> {
        let mut events = self.global_backend.poll_events(ctx);
        events.extend(self.egui_backend.poll_events(ctx));
        events.extend(self.gamepad_backend.poll_events(ctx));
        self.resolve_events(&events)
    }

    /// Traduce un lote de [`TriggerEvent`] en [`InputCommand`] según el modo de cada binding.
    ///
    /// Para expresiones en modo `Momentary`, mantiene un conteo de fuentes activas por estado
    /// de modo que solo emite `active: true` al pasar de 0 -> 1 y `active: false` de 1 -> 0.
    pub fn resolve_events(&mut self, events: &[TriggerEvent]) -> Vec<InputCommand> {
        let mut commands = Vec::new();
        for event in events {
            for binding in &self.bindings {
                if binding.source != event.source {
                    continue;
                }
                match (&binding.action, binding.mode, event.phase) {
                    (
                        InputAction::ActivateExpression(state),
                        ActivationMode::Momentary,
                        TriggerPhase::Pressed,
                    ) => {
                        let sources = self
                            .active_momentary_sources
                            .entry(state.clone())
                            .or_default();
                        let was_empty = sources.is_empty();
                        sources.insert(event.source.clone());
                        if was_empty {
                            commands.push(InputCommand::SetMomentary {
                                state: state.clone(),
                                active: true,
                            });
                        }
                    }
                    (
                        InputAction::ActivateExpression(state),
                        ActivationMode::Momentary,
                        TriggerPhase::Released,
                    ) => {
                        if let Some(sources) = self.active_momentary_sources.get_mut(state) {
                            sources.remove(&event.source);
                            if sources.is_empty() {
                                self.active_momentary_sources.remove(state);
                                commands.push(InputCommand::SetMomentary {
                                    state: state.clone(),
                                    active: false,
                                });
                            }
                        }
                    }
                    _ => {
                        if let Some(cmd) = map_non_momentary_command(binding, event.phase) {
                            commands.push(cmd);
                        }
                    }
                }
            }
        }
        commands
    }
}

/// Degrada a `HotkeyScope::Window` cualquier `KeyboardChord` que haya solicitado ámbito `Global`,
/// ya que `global-hotkey` solo registra atajos de una tecla principal con modificadores.
fn degrade_global_keyboard_chords(bindings: Vec<InputBinding>) -> (Vec<InputBinding>, Vec<String>) {
    let mut warnings = Vec::new();
    let normalized = bindings
        .into_iter()
        .map(|mut binding| {
            if let TriggerSource::KeyboardChord {
                ref chord,
                ref mut scope,
            } = binding.source
            {
                if *scope == HotkeyScope::Global {
                    *scope = HotkeyScope::Window;
                    warnings.push(format!(
                        "El combo multi-tecla '{}' no soporta ámbito global; se degradó a ámbito de ventana.",
                        chord.format_short()
                    ));
                }
            }
            binding
        })
        .collect();
    (normalized, warnings)
}

/// Recopila los disparadores simples únicos que solicitan ámbito `Global`.
fn collect_global_triggers(bindings: &[InputBinding]) -> Vec<KeyTrigger> {
    let mut triggers = Vec::new();
    for binding in bindings {
        let TriggerSource::Keyboard {
            trigger,
            scope: HotkeyScope::Global,
        } = binding.source
        else {
            continue;
        };
        if !triggers.contains(&trigger) {
            triggers.push(trigger);
        }
    }
    triggers
}

/// Recopila los disparadores de teclado (simples o combos) cuyo ámbito efectivo es `Window`.
fn collect_window_triggers(bindings: &[InputBinding]) -> Vec<WindowKeyboardTrigger> {
    let mut triggers = Vec::new();
    for binding in bindings {
        let wt = match &binding.source {
            TriggerSource::Keyboard {
                trigger,
                scope: HotkeyScope::Window,
            } => WindowKeyboardTrigger::Single(*trigger),
            TriggerSource::KeyboardChord {
                chord,
                scope: HotkeyScope::Window,
            } => WindowKeyboardTrigger::Chord(chord.clone()),
            _ => continue,
        };
        if !triggers.contains(&wt) {
            triggers.push(wt);
        }
    }
    triggers
}

/// Recopila los disparadores únicos configurados para mandos externos (`Gamepad`).
fn collect_gamepad_triggers(bindings: &[InputBinding]) -> Vec<GamepadTrigger> {
    let mut triggers = Vec::new();
    for binding in bindings {
        let TriggerSource::Gamepad(ref trigger) = binding.source else {
            continue;
        };
        if !triggers.contains(trigger) {
            triggers.push(trigger.clone());
        }
    }
    triggers
}

/// Degrada a `HotkeyScope::Window` cualquier binding global que no haya podido registrarse en el SO.
fn apply_global_registration(
    bindings: Vec<InputBinding>,
    registered_globals: &HashSet<KeyTrigger>,
) -> Vec<InputBinding> {
    bindings
        .into_iter()
        .map(|mut binding| {
            if let TriggerSource::Keyboard {
                trigger,
                scope: ref mut scope @ HotkeyScope::Global,
            } = binding.source
            {
                if !registered_globals.contains(&trigger) {
                    *scope = HotkeyScope::Window;
                }
            }
            binding
        })
        .collect()
}

/// Convierte una coincidencia que no es expresión momentánea en su [`InputCommand`].
fn map_non_momentary_command(binding: &InputBinding, phase: TriggerPhase) -> Option<InputCommand> {
    match (&binding.action, binding.mode, phase) {
        (InputAction::ActivateExpression(state), ActivationMode::Toggle, TriggerPhase::Pressed) => {
            Some(InputCommand::ToggleExpression(state.clone()))
        }
        (InputAction::ToggleControls, _, TriggerPhase::Pressed) => {
            Some(InputCommand::ToggleControls)
        }
        (InputAction::ClearExpression, _, TriggerPhase::Pressed) => {
            Some(InputCommand::ClearExpression)
        }
        (InputAction::ReloadConfig, _, TriggerPhase::Pressed) => Some(InputCommand::ReloadConfig),
        (_, _, TriggerPhase::Released) => None,
        (InputAction::ActivateExpression(_), ActivationMode::Momentary, TriggerPhase::Pressed) => {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router_for_pure_tests(bindings: Vec<InputBinding>) -> InputRouter {
        let window_triggers = collect_window_triggers(&bindings);
        let gamepad_triggers = collect_gamepad_triggers(&bindings);
        // Creamos un router de prueba con managers deshabilitados (sin registrar hotkeys reales ni abrir hardware).
        InputRouter {
            bindings,
            egui_backend: EguiKeyboardBackend::new(window_triggers),
            global_backend: GlobalHotkeyBackend::disabled_for_tests(),
            gamepad_backend: GamepadBackend::headless_for_tests(gamepad_triggers),
            active_momentary_sources: HashMap::new(),
        }
    }

    #[test]
    fn router_resuelve_toggle_vs_momentary_sin_doble_disparo() {
        let bindings = vec![
            InputBinding {
                source: TriggerSource::Keyboard {
                    trigger: KeyTrigger::key_only(KeyCode::Num1),
                    scope: HotkeyScope::Global,
                },
                action: InputAction::ActivateExpression(AvatarStateId::HAPPY),
                mode: ActivationMode::Toggle,
            },
            InputBinding {
                source: TriggerSource::Keyboard {
                    trigger: KeyTrigger::key_only(KeyCode::Num2),
                    scope: HotkeyScope::Window,
                },
                action: InputAction::ActivateExpression(AvatarStateId::SURPRISED),
                mode: ActivationMode::Momentary,
            },
        ];
        let mut router = router_for_pure_tests(bindings);

        // Toggle solo dispara comando en Pressed, ignora Released.
        let toggle_cmds = router.resolve_events(&[
            TriggerEvent {
                source: TriggerSource::Keyboard {
                    trigger: KeyTrigger::key_only(KeyCode::Num1),
                    scope: HotkeyScope::Global,
                },
                phase: TriggerPhase::Pressed,
            },
            TriggerEvent {
                source: TriggerSource::Keyboard {
                    trigger: KeyTrigger::key_only(KeyCode::Num1),
                    scope: HotkeyScope::Global,
                },
                phase: TriggerPhase::Released,
            },
        ]);
        assert_eq!(
            toggle_cmds,
            vec![InputCommand::ToggleExpression(AvatarStateId::HAPPY)]
        );

        // Un evento de scope distinto (Window para el binding Global de Num1) NO debe dispararse.
        let wrong_scope = router.resolve_events(&[TriggerEvent {
            source: TriggerSource::Keyboard {
                trigger: KeyTrigger::key_only(KeyCode::Num1),
                scope: HotkeyScope::Window,
            },
            phase: TriggerPhase::Pressed,
        }]);
        assert!(wrong_scope.is_empty());

        // Momentary emite active: true en Pressed y active: false en Released.
        let momentary_cmds = router.resolve_events(&[
            TriggerEvent {
                source: TriggerSource::Keyboard {
                    trigger: KeyTrigger::key_only(KeyCode::Num2),
                    scope: HotkeyScope::Window,
                },
                phase: TriggerPhase::Pressed,
            },
            TriggerEvent {
                source: TriggerSource::Keyboard {
                    trigger: KeyTrigger::key_only(KeyCode::Num2),
                    scope: HotkeyScope::Window,
                },
                phase: TriggerPhase::Released,
            },
        ]);
        assert_eq!(
            momentary_cmds,
            vec![
                InputCommand::SetMomentary {
                    state: AvatarStateId::SURPRISED,
                    active: true,
                },
                InputCommand::SetMomentary {
                    state: AvatarStateId::SURPRISED,
                    active: false,
                },
            ]
        );
    }

    #[test]
    fn degradacion_de_atajos_globales_mantiene_propiedad_exclusiva() {
        let global_ok = KeyTrigger::key_only(KeyCode::Num1);
        let global_failed = KeyTrigger::key_only(KeyCode::Num2);
        let bindings = vec![
            InputBinding {
                source: TriggerSource::Keyboard {
                    trigger: global_ok,
                    scope: HotkeyScope::Global,
                },
                action: InputAction::ActivateExpression(AvatarStateId::HAPPY),
                mode: ActivationMode::Toggle,
            },
            InputBinding {
                source: TriggerSource::Keyboard {
                    trigger: global_failed,
                    scope: HotkeyScope::Global,
                },
                action: InputAction::ActivateExpression(AvatarStateId::SURPRISED),
                mode: ActivationMode::Momentary,
            },
        ];

        let mut registered = HashSet::new();
        registered.insert(global_ok);

        let effective = apply_global_registration(bindings, &registered);
        assert_eq!(
            effective[0].source,
            TriggerSource::Keyboard {
                trigger: global_ok,
                scope: HotkeyScope::Global,
            }
        );
        assert_eq!(
            effective[1].source,
            TriggerSource::Keyboard {
                trigger: global_failed,
                scope: HotkeyScope::Window,
            }
        );
    }

    #[test]
    fn keyboard_chord_global_se_degrada_a_window_con_advertencia() {
        let chord = KeyChord::parse_str("A+B").expect("A+B");
        let bindings = vec![InputBinding {
            source: TriggerSource::KeyboardChord {
                chord: chord.clone(),
                scope: HotkeyScope::Global,
            },
            action: InputAction::ToggleControls,
            mode: ActivationMode::Toggle,
        }];

        let (degraded, warnings) = degrade_global_keyboard_chords(bindings);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("A+B"));
        assert_eq!(
            degraded[0].source,
            TriggerSource::KeyboardChord {
                chord,
                scope: HotkeyScope::Window,
            }
        );
    }

    #[test]
    fn router_mantiene_momentary_con_multiples_fuentes_y_libera_al_recargar() {
        let kb_source = TriggerSource::Keyboard {
            trigger: KeyTrigger::key_only(KeyCode::Num2),
            scope: HotkeyScope::Window,
        };
        let gp_source =
            TriggerSource::Gamepad(GamepadTrigger::parse_str("LB+Y", None, 500).expect("LB+Y"));

        let bindings = vec![
            InputBinding {
                source: kb_source.clone(),
                action: InputAction::ActivateExpression(AvatarStateId::SURPRISED),
                mode: ActivationMode::Momentary,
            },
            InputBinding {
                source: gp_source.clone(),
                action: InputAction::ActivateExpression(AvatarStateId::SURPRISED),
                mode: ActivationMode::Momentary,
            },
        ];
        let mut router = router_for_pure_tests(bindings);

        // 0 -> 1 fuente activa: emite active: true.
        assert_eq!(
            router.resolve_events(&[TriggerEvent {
                source: kb_source.clone(),
                phase: TriggerPhase::Pressed,
            }]),
            vec![InputCommand::SetMomentary {
                state: AvatarStateId::SURPRISED,
                active: true,
            }]
        );

        // 1 -> 2 fuentes activas: no duplica SetMomentary(true).
        assert!(router
            .resolve_events(&[TriggerEvent {
                source: gp_source.clone(),
                phase: TriggerPhase::Pressed,
            }])
            .is_empty());

        // 2 -> 1 fuente activa: sigue sostenido, no apaga todavía.
        assert!(router
            .resolve_events(&[TriggerEvent {
                source: kb_source,
                phase: TriggerPhase::Released,
            }])
            .is_empty());

        // Recargar bindings mientras `gp_source` sigue activo debe emitir SetMomentary(false).
        let (release_cmds, _) = router.reload_bindings(Vec::new());
        assert_eq!(
            release_cmds,
            vec![InputCommand::SetMomentary {
                state: AvatarStateId::SURPRISED,
                active: false,
            }]
        );

        // Luego si vuelve a llegar Pressed+Released en una única fuente, 0 -> 1 y 1 -> 0 funcionan normal.
        let mut router2 = router_for_pure_tests(vec![InputBinding {
            source: gp_source.clone(),
            action: InputAction::ActivateExpression(AvatarStateId::HAPPY),
            mode: ActivationMode::Momentary,
        }]);
        assert_eq!(
            router2.resolve_events(&[
                TriggerEvent {
                    source: gp_source.clone(),
                    phase: TriggerPhase::Pressed,
                },
                TriggerEvent {
                    source: gp_source,
                    phase: TriggerPhase::Released,
                },
            ]),
            vec![
                InputCommand::SetMomentary {
                    state: AvatarStateId::HAPPY,
                    active: true,
                },
                InputCommand::SetMomentary {
                    state: AvatarStateId::HAPPY,
                    active: false,
                },
            ]
        );
    }

    #[test]
    fn hints_muestran_combos_de_teclado_y_gamepad() {
        let chord = KeyChord::parse_str("A+B").expect("A+B");
        let gp = GamepadTrigger::parse_str("LB+Y", None, 500).expect("LB+Y");
        let router = router_for_pure_tests(vec![
            InputBinding {
                source: TriggerSource::KeyboardChord {
                    chord,
                    scope: HotkeyScope::Window,
                },
                action: InputAction::ToggleControls,
                mode: ActivationMode::Toggle,
            },
            InputBinding {
                source: TriggerSource::Gamepad(gp),
                action: InputAction::ActivateExpression(AvatarStateId::SURPRISED),
                mode: ActivationMode::Momentary,
            },
        ]);

        assert_eq!(
            router.action_hint(&InputAction::ToggleControls),
            Some("A+B".to_string())
        );
        assert_eq!(
            router.expression_hint(&AvatarStateId::SURPRISED),
            Some(TriggerHint {
                label: "LB+Y".to_string(),
                mode: ActivationMode::Momentary,
            })
        );
    }
}
