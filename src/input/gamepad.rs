//! Soporte para mandos externos / gamepads (Xbox, PlayStation, genéricos) mediante `gilrs`,
//! con normalización de botones y ejes, histéresis analógica y detección de combos por mando.

#[cfg(test)]
use super::chord::TriggerPhase;
use super::chord::{
    parse_combo_tokens, AnalogHysteresis, Chord, ChordTracker, ChordTransition, ComboSplitError,
};
use super::{TriggerBackend, TriggerEvent, TriggerSource};
use eframe::egui;
use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use std::collections::{hash_map::Entry, BTreeMap, BTreeSet, HashMap};
use std::fmt;

/// Controles soportados de un mando externo (botones digitales, gatillos y direcciones de sticks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GamepadControl {
    /// Gatillo superior izquierdo (`LB` / `L1`).
    LeftBumper,
    /// Gatillo superior derecho (`RB` / `R1`).
    RightBumper,
    /// Gatillo inferior izquierdo analógico (`LT` / `L2`).
    LeftTrigger,
    /// Gatillo inferior derecho analógico (`RT` / `R2`).
    RightTrigger,
    /// Botón inferior de acción (`A` en Xbox / Cruz en PlayStation).
    South,
    /// Botón derecho de acción (`B` en Xbox / Círculo en PlayStation).
    East,
    /// Botón izquierdo de acción (`X` en Xbox / Cuadrado en PlayStation).
    West,
    /// Botón superior de acción (`Y` en Xbox / Triángulo en PlayStation).
    North,
    /// Cruceta arriba.
    DPadUp,
    /// Cruceta abajo.
    DPadDown,
    /// Cruceta izquierda.
    DPadLeft,
    /// Cruceta derecha.
    DPadRight,
    /// Botón Start / Menú / Options.
    Start,
    /// Botón Select / Back / View.
    Select,
    /// Botón Guía / Xbox / Home.
    Mode,
    /// Pulsación del stick izquierdo (`L3`).
    LeftThumb,
    /// Pulsación del stick derecho (`R3`).
    RightThumb,
    /// Stick izquierdo inclinado hacia arriba.
    LeftStickUp,
    /// Stick izquierdo inclinado hacia abajo.
    LeftStickDown,
    /// Stick izquierdo inclinado hacia la izquierda.
    LeftStickLeft,
    /// Stick izquierdo inclinado hacia la derecha.
    LeftStickRight,
    /// Stick derecho inclinado hacia arriba.
    RightStickUp,
    /// Stick derecho inclinado hacia abajo.
    RightStickDown,
    /// Stick derecho inclinado hacia la izquierda.
    RightStickLeft,
    /// Stick derecho inclinado hacia la derecha.
    RightStickRight,
}

impl GamepadControl {
    /// Parsea un nombre o alias de control de mando (por ejemplo `"A"`, `"LB"`, `"LT"`, `"L3"`).
    pub fn parse(raw: &str) -> Result<Self, GamepadParseError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(GamepadParseError::Empty);
        }

        // Normalizamos quitando espacios, guiones bajos y guiones para aceptar múltiples estilos.
        let normalized: String = trimmed
            .chars()
            .filter(|c| !matches!(c, ' ' | '_' | '-'))
            .flat_map(char::to_lowercase)
            .collect();

        match normalized.as_str() {
            "a" | "south" | "cross" => Ok(Self::South),
            "b" | "east" | "circle" => Ok(Self::East),
            "x" | "west" | "square" => Ok(Self::West),
            "y" | "north" | "triangle" => Ok(Self::North),
            "lb" | "l1" | "leftbumper" => Ok(Self::LeftBumper),
            "rb" | "r1" | "rightbumper" => Ok(Self::RightBumper),
            "lt" | "l2" | "lefttrigger" => Ok(Self::LeftTrigger),
            "rt" | "r2" | "righttrigger" => Ok(Self::RightTrigger),
            "up" | "dpadup" => Ok(Self::DPadUp),
            "down" | "dpaddown" => Ok(Self::DPadDown),
            "left" | "dpadleft" => Ok(Self::DPadLeft),
            "right" | "dpadright" => Ok(Self::DPadRight),
            "start" | "menu" | "options" => Ok(Self::Start),
            "select" | "back" | "view" | "share" => Ok(Self::Select),
            "mode" | "guide" | "home" | "xbox" | "ps" => Ok(Self::Mode),
            "l3" | "ls" | "leftthumb" | "leftstickpress" => Ok(Self::LeftThumb),
            "r3" | "rs" | "rightthumb" | "rightstickpress" => Ok(Self::RightThumb),
            "lstickup" | "leftstickup" | "lsup" => Ok(Self::LeftStickUp),
            "lstickdown" | "leftstickdown" | "lsdown" => Ok(Self::LeftStickDown),
            "lstickleft" | "leftstickleft" | "lsleft" => Ok(Self::LeftStickLeft),
            "lstickright" | "leftstickright" | "lsright" => Ok(Self::LeftStickRight),
            "rstickup" | "rightstickup" | "rsup" => Ok(Self::RightStickUp),
            "rstickdown" | "rightstickdown" | "rsdown" => Ok(Self::RightStickDown),
            "rstickleft" | "rightstickleft" | "rsleft" => Ok(Self::RightStickLeft),
            "rstickright" | "rightstickright" | "rsright" => Ok(Self::RightStickRight),
            _ => Err(GamepadParseError::UnknownControl(trimmed.to_string())),
        }
    }

    /// Devuelve una etiqueta corta estilo Xbox para chips y botones de la interfaz.
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::LeftBumper => "LB",
            Self::RightBumper => "RB",
            Self::LeftTrigger => "LT",
            Self::RightTrigger => "RT",
            Self::South => "A",
            Self::East => "B",
            Self::West => "X",
            Self::North => "Y",
            Self::DPadUp => "D↑",
            Self::DPadDown => "D↓",
            Self::DPadLeft => "D←",
            Self::DPadRight => "D→",
            Self::Start => "Start",
            Self::Select => "Back",
            Self::Mode => "Guide",
            Self::LeftThumb => "L3",
            Self::RightThumb => "R3",
            Self::LeftStickUp => "LS↑",
            Self::LeftStickDown => "LS↓",
            Self::LeftStickLeft => "LS←",
            Self::LeftStickRight => "LS→",
            Self::RightStickUp => "RS↑",
            Self::RightStickDown => "RS↓",
            Self::RightStickLeft => "RS←",
            Self::RightStickRight => "RS→",
        }
    }

    /// Indica si el control responde a lecturas analógicas continuas (gatillos o sticks).
    pub const fn is_analog(self) -> bool {
        matches!(
            self,
            Self::LeftTrigger
                | Self::RightTrigger
                | Self::LeftStickUp
                | Self::LeftStickDown
                | Self::LeftStickLeft
                | Self::LeftStickRight
                | Self::RightStickUp
                | Self::RightStickDown
                | Self::RightStickLeft
                | Self::RightStickRight
        )
    }
}

/// Errores posibles al parsear un botón o combo de mando externo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GamepadParseError {
    /// El campo o lista está vacío.
    Empty,
    /// Existe un segmento vacío alrededor de `+`.
    EmptySegment(String),
    /// El nombre del botón o eje no se reconoce.
    UnknownControl(String),
}

impl fmt::Display for GamepadParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "el disparador de mando está vacío"),
            Self::EmptySegment(raw) => {
                write!(
                    f,
                    "combo de mando '{raw}' tiene un segmento vacío junto a '+'"
                )
            }
            Self::UnknownControl(token) => {
                write!(f, "botón o eje de mando no reconocido: '{token}'")
            }
        }
    }
}

impl std::error::Error for GamepadParseError {}

/// Disparador completo de mando externo (botón individual o combo multi-botón).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GamepadTrigger {
    /// Conjunto canónico ordenado de controles requeridos simultáneamente.
    pub controls: Chord<GamepadControl>,
    /// Índice opcional del mando (`Some(0)` = primer mando; `None` = cualquier mando conectado).
    pub gamepad_index: Option<usize>,
    /// Umbral analógico en milésimas (`100..=950`, equivalente a `0.10..=0.95`).
    pub threshold_milli: u16,
}

impl GamepadTrigger {
    /// Umbral analógico predeterminado en milésimas (`0.5`).
    pub const DEFAULT_THRESHOLD_MILLI: u16 = 500;

    /// Parsea una lista de partes (`["LB", "Y"]` o `["LB+Y"]`) en un [`GamepadTrigger`].
    pub fn parse_parts<S: AsRef<str>>(
        parts: &[S],
        gamepad_index: Option<usize>,
        threshold_milli: u16,
    ) -> Result<Self, GamepadParseError> {
        let (_, parsed_controls) =
            parse_combo_tokens(parts, GamepadControl::parse).map_err(|err| match err {
                ComboSplitError::Empty => GamepadParseError::Empty,
                ComboSplitError::EmptySegment(raw) => GamepadParseError::EmptySegment(raw),
                ComboSplitError::Token(inner) => inner,
            })?;

        let controls = Chord::from_atoms(parsed_controls).ok_or(GamepadParseError::Empty)?;
        Ok(Self {
            controls,
            gamepad_index,
            threshold_milli: threshold_milli.clamp(100, 950),
        })
    }

    /// Parsea una cadena como `"A"`, `"LB+Y"` o `"LT"` en un [`GamepadTrigger`].
    pub fn parse_str(
        raw: &str,
        gamepad_index: Option<usize>,
        threshold_milli: u16,
    ) -> Result<Self, GamepadParseError> {
        if raw.trim().is_empty() {
            return Err(GamepadParseError::Empty);
        }
        Self::parse_parts(&[raw], gamepad_index, threshold_milli)
    }

    /// Formatea el disparador de manera compacta para mostrar en la interfaz (ej. `"LB+Y"`).
    pub fn format_short(&self) -> String {
        let joined = self
            .controls
            .atoms()
            .iter()
            .map(|c| c.short_label())
            .collect::<Vec<_>>()
            .join("+");
        match self.gamepad_index {
            Some(idx) => format!("P{}:{joined}", idx + 1),
            None => joined,
        }
    }
}

/// Información resumida de un mando conectado para la UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectedGamepadInfo {
    /// Índice de ranura asignado (`0`, `1`, ...).
    pub index: usize,
    /// Nombre reportado por el dispositivo.
    pub name: String,
}

/// Estado actual del subsistema de mandos externos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GamepadStatus {
    /// Uno o más mandos conectados.
    Connected(Vec<ConnectedGamepadInfo>),
    /// Subsistema listo, sin mandos conectados.
    NoControllers,
    /// No se pudo inicializar `gilrs` (la aplicación sigue operando con degradación elegante).
    Unavailable(String),
}

/// Átomo interno usado por el [`ChordTracker`] de cada mando para distinguir umbrales analógicos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum GamepadAtom {
    Digital(GamepadControl),
    Analog {
        control: GamepadControl,
        threshold_milli: u16,
    },
}

impl GamepadTrigger {
    /// Convierte el disparador en un [`Chord<GamepadAtom>`] separando controles digitales y analógicos.
    fn to_atom_chord(&self) -> Option<Chord<GamepadAtom>> {
        let atoms = self.controls.atoms().iter().map(|&control| {
            if control.is_analog() {
                GamepadAtom::Analog {
                    control,
                    threshold_milli: self.threshold_milli,
                }
            } else {
                GamepadAtom::Digital(control)
            }
        });
        Chord::from_atoms(atoms)
    }
}

/// Identifica uno de los dos gatillos traseros (`LT`/`RT`) para arbitrar entre eventos analógicos y digitales.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerSide {
    Left,
    Right,
}

impl TriggerSide {
    const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }

    const fn control(self) -> GamepadControl {
        match self {
            Self::Left => GamepadControl::LeftTrigger,
            Self::Right => GamepadControl::RightTrigger,
        }
    }

    const fn from_button(button: Button) -> Option<Self> {
        match button {
            Button::LeftTrigger2 => Some(Self::Left),
            Button::RightTrigger2 => Some(Self::Right),
            _ => None,
        }
    }

    const fn from_axis(axis: Axis) -> Option<Self> {
        match axis {
            Axis::LeftZ => Some(Self::Left),
            Axis::RightZ => Some(Self::Right),
            _ => None,
        }
    }
}

/// Registra qué fuentes analógicas se han observado para un gatillo en un dispositivo o lote.
#[derive(Clone, Copy, Debug, Default)]
struct TriggerAnalogCaps {
    saw_button_changed: bool,
    saw_axis_changed: bool,
}

impl TriggerAnalogCaps {
    const fn has_any(self) -> bool {
        self.saw_button_changed || self.saw_axis_changed
    }
}

/// Estado aislado por dispositivo conectado para evitar interferencias entre múltiples mandos.
struct GamepadDeviceState {
    slot_index: usize,
    name: String,
    tracker: ChordTracker<GamepadAtom>,
    chord_sources: BTreeMap<Chord<GamepadAtom>, Vec<TriggerSource>>,
    analog_filters: BTreeMap<(GamepadControl, u16), AnalogHysteresis>,
    dpad_axis_filters: HashMap<GamepadControl, AnalogHysteresis>,
    /// Capacidades analógicas observadas por gatillo (`[LeftTrigger, RightTrigger]`).
    trigger_caps: [TriggerAnalogCaps; 2],
}

impl GamepadDeviceState {
    fn new(slot_index: usize, name: String, triggers: &[GamepadTrigger]) -> Self {
        let mut state = Self {
            slot_index,
            name,
            tracker: ChordTracker::new(),
            chord_sources: BTreeMap::new(),
            analog_filters: BTreeMap::new(),
            dpad_axis_filters: HashMap::new(),
            trigger_caps: [TriggerAnalogCaps::default(); 2],
        };
        state.configure_triggers(triggers);
        state
    }

    /// Configura los combos aplicables a este mando y reinicia su estado de pulsación/histéresis.
    fn configure_triggers(&mut self, triggers: &[GamepadTrigger]) {
        self.chord_sources.clear();
        self.analog_filters.clear();
        self.dpad_axis_filters.clear();

        let mut registered_chords = Vec::new();
        for trigger in triggers {
            if let Some(target_idx) = trigger.gamepad_index {
                if target_idx != self.slot_index {
                    continue;
                }
            }

            let Some(atom_chord) = trigger.to_atom_chord() else {
                continue;
            };

            // Creamos un filtro de histéresis por cada par (control analógico, umbral) requerido.
            for &atom in atom_chord.atoms() {
                if let GamepadAtom::Analog {
                    control,
                    threshold_milli,
                } = atom
                {
                    self.analog_filters
                        .entry((control, threshold_milli))
                        .or_insert_with(|| {
                            AnalogHysteresis::new(f32::from(threshold_milli) / 1000.0)
                        });
                }
            }

            let source = TriggerSource::Gamepad(trigger.clone());
            let list = self.chord_sources.entry(atom_chord.clone()).or_default();
            if !list.contains(&source) {
                list.push(source);
            }
            registered_chords.push(atom_chord);
        }

        let _ = self.tracker.set_chords(registered_chords);
    }

    fn push_transitions(
        &self,
        transitions: Vec<ChordTransition<GamepadAtom>>,
        out: &mut Vec<TriggerEvent>,
    ) {
        for transition in transitions {
            if let Some(sources) = self.chord_sources.get(&transition.chord) {
                for source in sources {
                    out.push(TriggerEvent {
                        source: source.clone(),
                        phase: transition.phase,
                    });
                }
            }
        }
    }

    fn press_digital(&mut self, control: GamepadControl, out: &mut Vec<TriggerEvent>) {
        let transitions = self.tracker.press(GamepadAtom::Digital(control));
        self.push_transitions(transitions, out);
    }

    fn release_digital(&mut self, control: GamepadControl, out: &mut Vec<TriggerEvent>) {
        let transitions = self.tracker.release(GamepadAtom::Digital(control));
        self.push_transitions(transitions, out);
    }

    /// Actualiza todos los filtros analógicos asociados a `control` y propaga sus transiciones al tracker.
    fn update_analog(&mut self, control: GamepadControl, value: f32, out: &mut Vec<TriggerEvent>) {
        if !value.is_finite() {
            return;
        }

        let matching_keys: Vec<(GamepadControl, u16)> = self
            .analog_filters
            .keys()
            .copied()
            .filter(|(c, _)| *c == control)
            .collect();

        for key in matching_keys {
            let change = self
                .analog_filters
                .get_mut(&key)
                .and_then(|filter| filter.update(value));
            match change {
                Some(true) => {
                    let transitions = self.tracker.press(GamepadAtom::Analog {
                        control: key.0,
                        threshold_milli: key.1,
                    });
                    self.push_transitions(transitions, out);
                }
                Some(false) => {
                    let transitions = self.tracker.release(GamepadAtom::Analog {
                        control: key.0,
                        threshold_milli: key.1,
                    });
                    self.push_transitions(transitions, out);
                }
                None => {}
            }
        }
    }

    /// Convierte ejes de cruceta (`DPadX`/`DPadY`) en pulsaciones digitales mediante histéresis fija.
    fn update_dpad_axis(
        &mut self,
        control: GamepadControl,
        value: f32,
        out: &mut Vec<TriggerEvent>,
    ) {
        if !value.is_finite() {
            return;
        }
        let filter = self
            .dpad_axis_filters
            .entry(control)
            .or_insert_with(|| AnalogHysteresis::with_thresholds(0.5, 0.25));
        match filter.update(value) {
            Some(true) => self.press_digital(control, out),
            Some(false) => self.release_digital(control, out),
            None => {}
        }
    }

    /// Actualiza un par de direcciones opuestas en un eje bipolar (sticks o cruceta analógica).
    ///
    /// Libera primero el sentido opuesto (`0.0`) antes de aplicar la nueva magnitud para evitar
    /// estados contradictorios si el eje salta de negativo a positivo en un solo evento.
    fn update_bipolar_axis(
        &mut self,
        positive_ctrl: GamepadControl,
        negative_ctrl: GamepadControl,
        value: f32,
        out: &mut Vec<TriggerEvent>,
    ) {
        if !value.is_finite() {
            return;
        }

        let apply =
            |this: &mut Self, ctrl: GamepadControl, mag: f32, out: &mut Vec<TriggerEvent>| {
                let clamped = mag.clamp(0.0, 1.0);
                if ctrl.is_analog() {
                    this.update_analog(ctrl, clamped, out);
                } else {
                    this.update_dpad_axis(ctrl, clamped, out);
                }
            };

        if value >= 0.0 {
            apply(self, negative_ctrl, 0.0, out);
            apply(self, positive_ctrl, value, out);
        } else {
            apply(self, positive_ctrl, 0.0, out);
            apply(self, negative_ctrl, -value, out);
        }
    }

    /// Libera todos los combos activos y reinicia filtros analógicos al desconectarse el mando.
    fn release_all(&mut self) -> Vec<TriggerEvent> {
        for filter in self.analog_filters.values_mut() {
            let _ = filter.reset();
        }
        for filter in self.dpad_axis_filters.values_mut() {
            let _ = filter.reset();
        }
        let transitions = self.tracker.release_all();
        let mut out = Vec::new();
        self.push_transitions(transitions, &mut out);
        out
    }
}

/// Estado puro del enrutamiento de mandos, separado de `gilrs::Gilrs` para facilitar pruebas sin hardware.
struct GamepadRouterState {
    status: GamepadStatus,
    registered_triggers: Vec<GamepadTrigger>,
    devices: HashMap<usize, GamepadDeviceState>,
}

impl GamepadRouterState {
    fn new(triggers: Vec<GamepadTrigger>) -> Self {
        Self {
            status: GamepadStatus::NoControllers,
            registered_triggers: triggers,
            devices: HashMap::new(),
        }
    }

    /// Asigna el menor índice de ranura libre (`0, 1, ...`) para mantener índices predecibles.
    fn next_free_slot(&self) -> usize {
        let used: BTreeSet<usize> = self.devices.values().map(|d| d.slot_index).collect();
        (0..).find(|idx| !used.contains(idx)).unwrap_or(0)
    }

    fn refresh_status(&mut self) {
        if matches!(self.status, GamepadStatus::Unavailable(_)) {
            return;
        }
        if self.devices.is_empty() {
            self.status = GamepadStatus::NoControllers;
            return;
        }
        let mut list: Vec<ConnectedGamepadInfo> = self
            .devices
            .values()
            .map(|d| ConnectedGamepadInfo {
                index: d.slot_index,
                name: d.name.clone(),
            })
            .collect();
        list.sort_by_key(|info| info.index);
        self.status = GamepadStatus::Connected(list);
    }

    fn ensure_connected(
        &mut self,
        device_key: usize,
        name: Option<&str>,
    ) -> &mut GamepadDeviceState {
        let trimmed_name = name.map(str::trim).filter(|s| !s.is_empty());
        let next_slot = self.next_free_slot();

        match self.devices.entry(device_key) {
            Entry::Vacant(vacant) => {
                let resolved_name = trimmed_name
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("Mando {}", next_slot + 1));
                upsert_connected_status(&mut self.status, next_slot, &resolved_name);
                let device =
                    GamepadDeviceState::new(next_slot, resolved_name, &self.registered_triggers);
                vacant.insert(device)
            }
            Entry::Occupied(mut occupied) => {
                if let Some(raw_name) = trimmed_name {
                    let dev = occupied.get_mut();
                    if dev.name != raw_name {
                        dev.name = raw_name.to_string();
                        upsert_connected_status(&mut self.status, dev.slot_index, raw_name);
                    }
                }
                occupied.into_mut()
            }
        }
    }

    fn set_triggers(&mut self, triggers: Vec<GamepadTrigger>) {
        self.registered_triggers = triggers;
        for device in self.devices.values_mut() {
            device.configure_triggers(&self.registered_triggers);
        }
    }

    /// Procesa un lote de entradas crudas detectando previamente qué gatillos tienen fuente
    /// analógica (`ButtonChanged` o `AxisChanged`) dentro del mismo lote o en el dispositivo.
    fn handle_batch(
        &mut self,
        inputs: &[(usize, RawGamepadInput, Option<&str>)],
    ) -> Vec<TriggerEvent> {
        let mut gen_by_device: HashMap<usize, usize> = HashMap::new();
        let mut batch_caps: HashMap<(usize, usize), [TriggerAnalogCaps; 2]> = HashMap::new();

        // Primera pasada: detectamos capacidades analógicas de gatillos por segmento de conexión.
        for &(device_key, input, _) in inputs {
            let generation = *gen_by_device.get(&device_key).unwrap_or(&0);
            match input {
                RawGamepadInput::Disconnected => {
                    *gen_by_device.entry(device_key).or_insert(0) = generation + 1;
                }
                RawGamepadInput::ButtonChanged(button, value) if value.is_finite() => {
                    if let Some(side) = TriggerSide::from_button(button) {
                        batch_caps.entry((device_key, generation)).or_default()[side.index()]
                            .saw_button_changed = true;
                    }
                }
                RawGamepadInput::AxisChanged(axis, value) if value.is_finite() => {
                    if let Some(side) = TriggerSide::from_axis(axis) {
                        batch_caps.entry((device_key, generation)).or_default()[side.index()]
                            .saw_axis_changed = true;
                    }
                }
                _ => {}
            }
        }

        let mut out = Vec::new();
        let mut exec_gen: HashMap<usize, usize> = HashMap::new();
        for &(device_key, input, name) in inputs {
            let generation = *exec_gen.get(&device_key).unwrap_or(&0);
            let caps = batch_caps
                .get(&(device_key, generation))
                .copied()
                .unwrap_or_default();
            if matches!(input, RawGamepadInput::Disconnected) {
                *exec_gen.entry(device_key).or_insert(0) = generation + 1;
            }
            self.handle_single_input(device_key, input, name, caps, &mut out);
        }
        out
    }

    #[cfg(test)]
    fn handle_input(
        &mut self,
        device_key: usize,
        input: RawGamepadInput,
        name: Option<&str>,
    ) -> Vec<TriggerEvent> {
        self.handle_batch(&[(device_key, input, name)])
    }

    fn handle_button_phase(
        &mut self,
        device_key: usize,
        button: Button,
        pressed: bool,
        name: Option<&str>,
        batch_caps: [TriggerAnalogCaps; 2],
        out: &mut Vec<TriggerEvent>,
    ) {
        let Some(control) = map_gilrs_button(button) else {
            return;
        };

        let device = self.ensure_connected(device_key, name);
        if let Some(side) = TriggerSide::from_button(button) {
            let idx = side.index();
            let has_analog = device.trigger_caps[idx].has_any() || batch_caps[idx].has_any();
            // Solo simulamos pulsación digital en gatillos si el mando no reporta lecturas analógicas.
            if !has_analog {
                device.update_analog(control, if pressed { 1.0 } else { 0.0 }, out);
            }
            return;
        }

        if !control.is_analog() {
            if pressed {
                device.press_digital(control, out);
            } else {
                device.release_digital(control, out);
            }
        }
    }

    fn handle_single_input(
        &mut self,
        device_key: usize,
        input: RawGamepadInput,
        name: Option<&str>,
        batch_caps: [TriggerAnalogCaps; 2],
        out: &mut Vec<TriggerEvent>,
    ) {
        match input {
            RawGamepadInput::Connected => {
                let _ = self.ensure_connected(device_key, name);
            }
            RawGamepadInput::Disconnected => {
                if let Some(mut device) = self.devices.remove(&device_key) {
                    out.extend(device.release_all());
                    self.refresh_status();
                }
            }
            RawGamepadInput::ButtonPressed(button) => {
                self.handle_button_phase(device_key, button, true, name, batch_caps, out);
            }
            RawGamepadInput::ButtonReleased(button) => {
                self.handle_button_phase(device_key, button, false, name, batch_caps, out);
            }
            RawGamepadInput::ButtonChanged(button, value) => {
                if value.is_finite() {
                    if let Some(control) = map_gilrs_button(button) {
                        if control.is_analog() {
                            let device = self.ensure_connected(device_key, name);
                            if let Some(side) = TriggerSide::from_button(button) {
                                device.trigger_caps[side.index()].saw_button_changed = true;
                            }
                            device.update_analog(control, value.clamp(0.0, 1.0), out);
                        }
                    }
                }
            }
            RawGamepadInput::AxisChanged(axis, value) => {
                if value.is_finite() {
                    let device = self.ensure_connected(device_key, name);
                    match axis {
                        Axis::LeftZ | Axis::RightZ => {
                            if let Some(side) = TriggerSide::from_axis(axis) {
                                let idx = side.index();
                                // Si el driver ya envía `ButtonChanged` para este gatillo, ignoramos `AxisChanged`
                                // para evitar conflictos de escala o doble lectura.
                                let has_button_changed = device.trigger_caps[idx]
                                    .saw_button_changed
                                    || batch_caps[idx].saw_button_changed;
                                if !has_button_changed {
                                    device.trigger_caps[idx].saw_axis_changed = true;
                                    device.update_analog(
                                        side.control(),
                                        value.clamp(0.0, 1.0),
                                        out,
                                    );
                                }
                            }
                        }
                        Axis::LeftStickX => device.update_bipolar_axis(
                            GamepadControl::LeftStickRight,
                            GamepadControl::LeftStickLeft,
                            value,
                            out,
                        ),
                        Axis::LeftStickY => device.update_bipolar_axis(
                            GamepadControl::LeftStickUp,
                            GamepadControl::LeftStickDown,
                            value,
                            out,
                        ),
                        Axis::RightStickX => device.update_bipolar_axis(
                            GamepadControl::RightStickRight,
                            GamepadControl::RightStickLeft,
                            value,
                            out,
                        ),
                        Axis::RightStickY => device.update_bipolar_axis(
                            GamepadControl::RightStickUp,
                            GamepadControl::RightStickDown,
                            value,
                            out,
                        ),
                        Axis::DPadX => device.update_bipolar_axis(
                            GamepadControl::DPadRight,
                            GamepadControl::DPadLeft,
                            value,
                            out,
                        ),
                        Axis::DPadY => device.update_bipolar_axis(
                            GamepadControl::DPadUp,
                            GamepadControl::DPadDown,
                            value,
                            out,
                        ),
                        Axis::Unknown => {}
                    }
                }
            }
            RawGamepadInput::Other => {}
        }
    }
}

/// Actualiza incrementalmente `status` al conectar o renombrar un mando sin re-prestar `devices`.
fn upsert_connected_status(status: &mut GamepadStatus, index: usize, name: &str) {
    match status {
        GamepadStatus::Unavailable(_) => {}
        GamepadStatus::NoControllers => {
            *status = GamepadStatus::Connected(vec![ConnectedGamepadInfo {
                index,
                name: name.to_string(),
            }]);
        }
        GamepadStatus::Connected(list) => {
            if let Some(existing) = list.iter_mut().find(|info| info.index == index) {
                existing.name = name.to_string();
            } else {
                list.push(ConnectedGamepadInfo {
                    index,
                    name: name.to_string(),
                });
                list.sort_by_key(|info| info.index);
            }
        }
    }
}

/// Representación interna desacoplada del código de evento opaco de `gilrs`.
#[derive(Clone, Copy, Debug)]
enum RawGamepadInput {
    Connected,
    Disconnected,
    ButtonPressed(Button),
    ButtonReleased(Button),
    ButtonChanged(Button, f32),
    AxisChanged(Axis, f32),
    Other,
}

impl RawGamepadInput {
    /// Convierte un [`EventType`] de `gilrs` a la representación interna desacoplada.
    fn from_gilrs(event: EventType) -> Self {
        match event {
            EventType::Connected => Self::Connected,
            EventType::Disconnected => Self::Disconnected,
            EventType::ButtonPressed(button, _) => Self::ButtonPressed(button),
            EventType::ButtonReleased(button, _) => Self::ButtonReleased(button),
            EventType::ButtonChanged(button, value, _) => Self::ButtonChanged(button, value),
            EventType::AxisChanged(axis, value, _) => Self::AxisChanged(axis, value),
            _ => Self::Other,
        }
    }
}

/// Convierte un botón estándar de `gilrs` a [`GamepadControl`].
fn map_gilrs_button(button: Button) -> Option<GamepadControl> {
    match button {
        Button::South => Some(GamepadControl::South),
        Button::East => Some(GamepadControl::East),
        Button::West => Some(GamepadControl::West),
        Button::North => Some(GamepadControl::North),
        // En `gilrs`, `LeftTrigger`/`RightTrigger` son los bumpers (LB/RB) y `*Trigger2` los gatillos (LT/RT).
        Button::LeftTrigger => Some(GamepadControl::LeftBumper),
        Button::RightTrigger => Some(GamepadControl::RightBumper),
        Button::LeftTrigger2 => Some(GamepadControl::LeftTrigger),
        Button::RightTrigger2 => Some(GamepadControl::RightTrigger),
        Button::DPadUp => Some(GamepadControl::DPadUp),
        Button::DPadDown => Some(GamepadControl::DPadDown),
        Button::DPadLeft => Some(GamepadControl::DPadLeft),
        Button::DPadRight => Some(GamepadControl::DPadRight),
        Button::Start => Some(GamepadControl::Start),
        Button::Select => Some(GamepadControl::Select),
        Button::Mode => Some(GamepadControl::Mode),
        Button::LeftThumb => Some(GamepadControl::LeftThumb),
        Button::RightThumb => Some(GamepadControl::RightThumb),
        Button::C | Button::Z | Button::Unknown => None,
    }
}

/// Backend no bloqueante para lectura de mandos externos sobre `gilrs`.
pub struct GamepadBackend {
    gilrs: Option<Gilrs>,
    state: GamepadRouterState,
}

impl GamepadBackend {
    /// Crea un backend sin inicializar `gilrs` (para pruebas unitarias sin hardware).
    #[cfg(test)]
    pub fn headless_for_tests(triggers: Vec<GamepadTrigger>) -> Self {
        Self {
            gilrs: None,
            state: GamepadRouterState::new(triggers),
        }
    }

    /// Inicializa `gilrs` en el hilo principal sin entrar en pánico y registra mandos conectados.
    pub fn new(triggers: Vec<GamepadTrigger>) -> (Self, Vec<String>) {
        let mut state = GamepadRouterState::new(triggers);
        match Gilrs::new() {
            Ok(gilrs) => {
                for (id, gamepad) in gilrs.gamepads() {
                    if gamepad.is_connected() {
                        let _ = state.ensure_connected(usize::from(id), Some(gamepad.name()));
                    }
                }
                (
                    Self {
                        gilrs: Some(gilrs),
                        state,
                    },
                    Vec::new(),
                )
            }
            Err(err) => {
                let reason = err.to_string();
                state.status = GamepadStatus::Unavailable(reason.clone());
                let warning = format!(
                    "Control externo (gamepad) no disponible ({reason}); se continuará solo con teclado."
                );
                (Self { gilrs: None, state }, vec![warning])
            }
        }
    }

    /// Devuelve el estado actual de conexión de mandos externos.
    pub fn status(&self) -> &GamepadStatus {
        &self.state.status
    }

    /// Reemplaza los disparadores de mando vigilados tras recargar la configuración.
    pub fn set_triggers(&mut self, triggers: Vec<GamepadTrigger>) {
        self.state.set_triggers(triggers);
    }

    /// Procesa un lote de eventos crudos de `gilrs` de manera pura y determinística.
    pub fn handle_raw_events(
        &mut self,
        events: &[(GamepadId, EventType, Option<&str>)],
    ) -> Vec<TriggerEvent> {
        let inputs: Vec<(usize, RawGamepadInput, Option<&str>)> = events
            .iter()
            .map(|&(id, event, name)| (usize::from(id), RawGamepadInput::from_gilrs(event), name))
            .collect();
        self.state.handle_batch(&inputs)
    }

    /// Procesa un evento crudo individual de `gilrs` de manera pura y determinística (útil en pruebas).
    #[cfg(test)]
    pub fn handle_raw_event(
        &mut self,
        id: GamepadId,
        event: EventType,
        name: Option<&str>,
    ) -> Vec<TriggerEvent> {
        self.handle_raw_events(&[(id, event, name)])
    }
}

impl TriggerBackend for GamepadBackend {
    fn poll_events(&mut self, _ctx: &egui::Context) -> Vec<TriggerEvent> {
        let Some(ref mut gilrs) = self.gilrs else {
            return Vec::new();
        };

        // Recolectamos todos los eventos pendientes en lote para detectar fuentes analógicas por frame.
        let mut owned_batch = Vec::new();
        while let Some(ev) = gilrs.next_event() {
            let name = gilrs.gamepad(ev.id).name().to_string();
            owned_batch.push((ev.id, ev.event, name));
        }
        if owned_batch.is_empty() {
            return Vec::new();
        }
        let borrowed_batch: Vec<(GamepadId, EventType, Option<&str>)> = owned_batch
            .iter()
            .map(|(id, event, name)| (*id, *event, Some(name.as_str())))
            .collect();
        self.handle_raw_events(&borrowed_batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_aliases_xbox_y_formatea_combos() {
        assert_eq!(GamepadControl::parse("A"), Ok(GamepadControl::South));
        assert_eq!(GamepadControl::parse("south"), Ok(GamepadControl::South));
        assert_eq!(GamepadControl::parse("b"), Ok(GamepadControl::East));
        assert_eq!(GamepadControl::parse("x"), Ok(GamepadControl::West));
        assert_eq!(GamepadControl::parse("Y"), Ok(GamepadControl::North));
        assert_eq!(GamepadControl::parse("LB"), Ok(GamepadControl::LeftBumper));
        assert_eq!(
            GamepadControl::parse("right_bumper"),
            Ok(GamepadControl::RightBumper)
        );
        assert_eq!(GamepadControl::parse("LT"), Ok(GamepadControl::LeftTrigger));
        assert_eq!(
            GamepadControl::parse("RT"),
            Ok(GamepadControl::RightTrigger)
        );
        assert_eq!(GamepadControl::parse("L3"), Ok(GamepadControl::LeftThumb));
        assert_eq!(GamepadControl::parse("r3"), Ok(GamepadControl::RightThumb));
        assert_eq!(
            GamepadControl::parse("lstick_up"),
            Ok(GamepadControl::LeftStickUp)
        );

        let lb_y = GamepadTrigger::parse_str("Y + LB", None, 500).expect("LB+Y válido");
        assert_eq!(lb_y.format_short(), "LB+Y");

        let p2 = GamepadTrigger::parse_parts(&["LB", "A"], Some(1), 500).expect("P2 válido");
        assert_eq!(p2.format_short(), "P2:LB+A");

        assert_eq!(GamepadControl::parse("   "), Err(GamepadParseError::Empty));
        assert!(matches!(
            GamepadControl::parse("BotonDesconocido"),
            Err(GamepadParseError::UnknownControl(_))
        ));
    }

    #[test]
    fn combo_lb_y_y_desconexion_libera_limpiamente() {
        let trigger_lb = GamepadTrigger::parse_str("LB", None, 500).expect("LB");
        let trigger_lb_y = GamepadTrigger::parse_str("LB+Y", None, 500).expect("LB+Y");
        let mut backend =
            GamepadBackend::headless_for_tests(vec![trigger_lb.clone(), trigger_lb_y.clone()]);

        // Conecta mando 0.
        let _ = backend.state.handle_input(
            0,
            RawGamepadInput::Connected,
            Some("Xbox Wireless Controller"),
        );
        assert_eq!(
            backend.status(),
            &GamepadStatus::Connected(vec![ConnectedGamepadInfo {
                index: 0,
                name: "Xbox Wireless Controller".to_string(),
            }])
        );

        // Pulsa LB -> Pressed(LB).
        assert_eq!(
            backend.state.handle_input(
                0,
                RawGamepadInput::ButtonPressed(Button::LeftTrigger),
                None
            ),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(trigger_lb.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );

        // Pulsa Y -> Released(LB), Pressed(LB+Y).
        assert_eq!(
            backend
                .state
                .handle_input(0, RawGamepadInput::ButtonPressed(Button::North), None),
            vec![
                TriggerEvent {
                    source: TriggerSource::Gamepad(trigger_lb),
                    phase: TriggerPhase::Released,
                },
                TriggerEvent {
                    source: TriggerSource::Gamepad(trigger_lb_y.clone()),
                    phase: TriggerPhase::Pressed,
                },
            ]
        );

        // Desconexión repentina mientras LB+Y está sostenido -> emite Released(LB+Y) y actualiza estado.
        assert_eq!(
            backend
                .state
                .handle_input(0, RawGamepadInput::Disconnected, None),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(trigger_lb_y),
                phase: TriggerPhase::Released,
            }]
        );
        assert_eq!(backend.status(), &GamepadStatus::NoControllers);
    }

    #[test]
    fn aislamiento_entre_dos_mandos_y_histeresis_en_gatillos_y_ejes() {
        let t_p1_a = GamepadTrigger::parse_str("A", Some(0), 500).expect("P1 A");
        let t_p2_a = GamepadTrigger::parse_str("A", Some(1), 500).expect("P2 A");
        let t_lt = GamepadTrigger::parse_str("LT", None, 600).expect("LT 0.6");
        let t_ls_up = GamepadTrigger::parse_str("lstick_up", None, 500).expect("LS up 0.5");

        let mut backend = GamepadBackend::headless_for_tests(vec![
            t_p1_a.clone(),
            t_p2_a.clone(),
            t_lt.clone(),
            t_ls_up.clone(),
        ]);

        let _ = backend
            .state
            .handle_input(10, RawGamepadInput::Connected, Some("Mando 1"));
        let _ = backend
            .state
            .handle_input(20, RawGamepadInput::Connected, Some("Mando 2"));

        // Mando 1 (slot 0) pulsa A -> solo dispara t_p1_a.
        assert_eq!(
            backend
                .state
                .handle_input(10, RawGamepadInput::ButtonPressed(Button::South), None),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_p1_a.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );

        // Mando 2 (slot 1) pulsa A -> dispara t_p2_a sin interferir con Mando 1.
        assert_eq!(
            backend
                .state
                .handle_input(20, RawGamepadInput::ButtonPressed(Button::South), None),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_p2_a),
                phase: TriggerPhase::Pressed,
            }]
        );

        // Desconectar Mando 2 solo libera t_p2_a, Mando 1 sigue presionando A.
        let disc_p2 = backend
            .state
            .handle_input(20, RawGamepadInput::Disconnected, None);
        assert_eq!(disc_p2.len(), 1);

        // Soltar A en Mando 1 ahora libera t_p1_a.
        assert_eq!(
            backend
                .state
                .handle_input(10, RawGamepadInput::ButtonReleased(Button::South), None),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_p1_a),
                phase: TriggerPhase::Released,
            }]
        );

        // Gatillo LT: valores NaN o por debajo del umbral (0.6) no disparan; ButtonPressed(LeftTrigger2) tampoco duplica.
        assert!(backend
            .state
            .handle_input(
                10,
                RawGamepadInput::ButtonChanged(Button::LeftTrigger2, f32::NAN),
                None
            )
            .is_empty());
        assert!(backend
            .state
            .handle_input(
                10,
                RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.3),
                None
            )
            .is_empty());
        assert!(backend
            .state
            .handle_input(
                10,
                RawGamepadInput::ButtonPressed(Button::LeftTrigger2),
                None
            )
            .is_empty());

        // Al llegar a 0.65 se activa LT una sola vez; luego ButtonPressed(LeftTrigger2) no duplica.
        assert_eq!(
            backend.state.handle_input(
                10,
                RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.65),
                None
            ),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_lt.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert!(backend
            .state
            .handle_input(
                10,
                RawGamepadInput::ButtonPressed(Button::LeftTrigger2),
                None
            )
            .is_empty());

        // Histéresis: en 0.50 (entre 0.42 y 0.60) sigue activo; al bajar a 0.40 se libera.
        assert!(backend
            .state
            .handle_input(
                10,
                RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.50),
                None
            )
            .is_empty());
        assert_eq!(
            backend.state.handle_input(
                10,
                RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.40),
                None
            ),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_lt),
                phase: TriggerPhase::Released,
            }]
        );

        // Eje LeftStickY: NaN ignorado, +0.70 activa LeftStickUp, +0.10 libera.
        assert!(backend
            .state
            .handle_input(
                10,
                RawGamepadInput::AxisChanged(Axis::LeftStickY, f32::NAN),
                None
            )
            .is_empty());
        assert_eq!(
            backend.state.handle_input(
                10,
                RawGamepadInput::AxisChanged(Axis::LeftStickY, 0.70),
                None
            ),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_ls_up.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert_eq!(
            backend.state.handle_input(
                10,
                RawGamepadInput::AxisChanged(Axis::LeftStickY, 0.10),
                None
            ),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_ls_up),
                phase: TriggerPhase::Released,
            }]
        );
    }

    #[test]
    fn salto_bipolar_libera_lado_opuesto_antes_de_activar_el_nuevo() {
        let t_ls_left = GamepadTrigger::parse_str("lstick_left", None, 500).expect("LS left");
        let t_ls_right = GamepadTrigger::parse_str("lstick_right", None, 500).expect("LS right");
        let t_dpad_left = GamepadTrigger::parse_str("dpad_left", None, 500).expect("DPad left");
        let t_dpad_right = GamepadTrigger::parse_str("dpad_right", None, 500).expect("DPad right");

        let mut backend = GamepadBackend::headless_for_tests(vec![
            t_ls_left.clone(),
            t_ls_right.clone(),
            t_dpad_left.clone(),
            t_dpad_right.clone(),
        ]);

        // Stick izquierdo: primero -0.9 activa izquierda, luego salto directo a +0.9
        // debe emitir Released(LeftStickLeft) antes que Pressed(LeftStickRight).
        assert_eq!(
            backend.state.handle_input(
                0,
                RawGamepadInput::AxisChanged(Axis::LeftStickX, -0.9),
                None
            ),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_ls_left.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert_eq!(
            backend.state.handle_input(
                0,
                RawGamepadInput::AxisChanged(Axis::LeftStickX, 0.9),
                None
            ),
            vec![
                TriggerEvent {
                    source: TriggerSource::Gamepad(t_ls_left),
                    phase: TriggerPhase::Released,
                },
                TriggerEvent {
                    source: TriggerSource::Gamepad(t_ls_right),
                    phase: TriggerPhase::Pressed,
                },
            ]
        );

        // Cruceta analógica: mismo orden determinístico al saltar de -0.9 a +0.9.
        assert_eq!(
            backend
                .state
                .handle_input(0, RawGamepadInput::AxisChanged(Axis::DPadX, -0.9), None),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_dpad_left.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert_eq!(
            backend
                .state
                .handle_input(0, RawGamepadInput::AxisChanged(Axis::DPadX, 0.9), None),
            vec![
                TriggerEvent {
                    source: TriggerSource::Gamepad(t_dpad_left),
                    phase: TriggerPhase::Released,
                },
                TriggerEvent {
                    source: TriggerSource::Gamepad(t_dpad_right),
                    phase: TriggerPhase::Pressed,
                },
            ]
        );
    }

    #[test]
    fn gatillos_digitales_y_prioridad_de_button_changed_sobre_axis_z() {
        let t_lt = GamepadTrigger::parse_str("LT", None, 600).expect("LT");
        let t_rt = GamepadTrigger::parse_str("RT", None, 600).expect("RT");
        let mut backend = GamepadBackend::headless_for_tests(vec![t_lt.clone(), t_rt.clone()]);

        // Mando 0 con gatillo puramente digital: ButtonPressed/ButtonReleased sin ButtonChanged ni AxisChanged.
        assert_eq!(
            backend.state.handle_batch(&[(
                0,
                RawGamepadInput::ButtonPressed(Button::LeftTrigger2),
                Some("Mando Digital"),
            )]),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_lt.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert_eq!(
            backend.state.handle_batch(&[(
                0,
                RawGamepadInput::ButtonReleased(Button::LeftTrigger2),
                None,
            )]),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_lt.clone()),
                phase: TriggerPhase::Released,
            }]
        );

        // Mando 1: si en el mismo lote llega ButtonPressed seguido de ButtonChanged(0.3 < 0.6),
        // se detecta la fuente analógica del lote y NO se dispara falsamente LT.
        assert!(backend
            .state
            .handle_batch(&[
                (
                    1,
                    RawGamepadInput::ButtonPressed(Button::LeftTrigger2),
                    Some("Mando Mixto")
                ),
                (
                    1,
                    RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.3),
                    None
                ),
            ])
            .is_empty());

        // Si el driver envía ambos AxisChanged(LeftZ) y ButtonChanged(LeftTrigger2),
        // ButtonChanged tiene prioridad e impide que AxisChanged(LeftZ, 0.0) apague el gatillo.
        assert_eq!(
            backend.state.handle_batch(&[
                (1, RawGamepadInput::AxisChanged(Axis::LeftZ, 0.0), None),
                (
                    1,
                    RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.8),
                    None
                ),
            ]),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_lt.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert!(backend
            .state
            .handle_batch(&[(1, RawGamepadInput::AxisChanged(Axis::LeftZ, 0.0), None)])
            .is_empty());
        assert_eq!(
            backend.state.handle_batch(&[(
                1,
                RawGamepadInput::ButtonChanged(Button::LeftTrigger2, 0.2),
                None
            )]),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_lt),
                phase: TriggerPhase::Released,
            }]
        );

        // Para RT sin ButtonChanged, AxisChanged(RightZ) funciona y acota valores fuera de [0, 1].
        assert_eq!(
            backend.state.handle_batch(&[(
                1,
                RawGamepadInput::AxisChanged(Axis::RightZ, 1.5),
                None
            )]),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_rt.clone()),
                phase: TriggerPhase::Pressed,
            }]
        );
        assert_eq!(
            backend.state.handle_batch(&[(
                1,
                RawGamepadInput::AxisChanged(Axis::RightZ, -0.4),
                None
            )]),
            vec![TriggerEvent {
                source: TriggerSource::Gamepad(t_rt),
                phase: TriggerPhase::Released,
            }]
        );

        // Verificamos que el adaptador unitario `handle_raw_event` conserva su firma en pruebas.
        let _raw_event_fn: fn(
            &mut GamepadBackend,
            GamepadId,
            EventType,
            Option<&str>,
        ) -> Vec<TriggerEvent> = GamepadBackend::handle_raw_event;
    }
}
