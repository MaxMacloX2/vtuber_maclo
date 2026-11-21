//! Disparadores de teclado simples (`KeyTrigger`) y multi-tecla (`KeyChord`), junto con los
//! backends de ventana local (`EguiKeyboardBackend`) y global del sistema (`GlobalHotkeyBackend`).

use super::chord::{parse_combo_tokens, Chord, ChordTracker, ComboSplitError, TriggerPhase};
use super::{TriggerBackend, TriggerEvent, TriggerSource};
use eframe::egui;
use global_hotkey::hotkey::{Code as GlobalCode, HotKey, Modifiers as GlobalModifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

/// Teclas lógicas soportadas de forma portable entre `egui` y `global-hotkey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Escape,
    Space,
    Tab,
    Enter,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

impl KeyCode {
    /// Intenta convertir una tecla `egui::Key` a [`KeyCode`].
    pub const fn from_egui_key(key: egui::Key) -> Option<Self> {
        match key {
            egui::Key::A => Some(Self::A),
            egui::Key::B => Some(Self::B),
            egui::Key::C => Some(Self::C),
            egui::Key::D => Some(Self::D),
            egui::Key::E => Some(Self::E),
            egui::Key::F => Some(Self::F),
            egui::Key::G => Some(Self::G),
            egui::Key::H => Some(Self::H),
            egui::Key::I => Some(Self::I),
            egui::Key::J => Some(Self::J),
            egui::Key::K => Some(Self::K),
            egui::Key::L => Some(Self::L),
            egui::Key::M => Some(Self::M),
            egui::Key::N => Some(Self::N),
            egui::Key::O => Some(Self::O),
            egui::Key::P => Some(Self::P),
            egui::Key::Q => Some(Self::Q),
            egui::Key::R => Some(Self::R),
            egui::Key::S => Some(Self::S),
            egui::Key::T => Some(Self::T),
            egui::Key::U => Some(Self::U),
            egui::Key::V => Some(Self::V),
            egui::Key::W => Some(Self::W),
            egui::Key::X => Some(Self::X),
            egui::Key::Y => Some(Self::Y),
            egui::Key::Z => Some(Self::Z),
            egui::Key::Num0 => Some(Self::Num0),
            egui::Key::Num1 => Some(Self::Num1),
            egui::Key::Num2 => Some(Self::Num2),
            egui::Key::Num3 => Some(Self::Num3),
            egui::Key::Num4 => Some(Self::Num4),
            egui::Key::Num5 => Some(Self::Num5),
            egui::Key::Num6 => Some(Self::Num6),
            egui::Key::Num7 => Some(Self::Num7),
            egui::Key::Num8 => Some(Self::Num8),
            egui::Key::Num9 => Some(Self::Num9),
            egui::Key::F1 => Some(Self::F1),
            egui::Key::F2 => Some(Self::F2),
            egui::Key::F3 => Some(Self::F3),
            egui::Key::F4 => Some(Self::F4),
            egui::Key::F5 => Some(Self::F5),
            egui::Key::F6 => Some(Self::F6),
            egui::Key::F7 => Some(Self::F7),
            egui::Key::F8 => Some(Self::F8),
            egui::Key::F9 => Some(Self::F9),
            egui::Key::F10 => Some(Self::F10),
            egui::Key::F11 => Some(Self::F11),
            egui::Key::F12 => Some(Self::F12),
            egui::Key::Escape => Some(Self::Escape),
            egui::Key::Space => Some(Self::Space),
            egui::Key::Tab => Some(Self::Tab),
            egui::Key::Enter => Some(Self::Enter),
            egui::Key::Backspace => Some(Self::Backspace),
            egui::Key::ArrowUp => Some(Self::ArrowUp),
            egui::Key::ArrowDown => Some(Self::ArrowDown),
            egui::Key::ArrowLeft => Some(Self::ArrowLeft),
            egui::Key::ArrowRight => Some(Self::ArrowRight),
            egui::Key::Insert => Some(Self::Insert),
            egui::Key::Delete => Some(Self::Delete),
            egui::Key::Home => Some(Self::Home),
            egui::Key::End => Some(Self::End),
            egui::Key::PageUp => Some(Self::PageUp),
            egui::Key::PageDown => Some(Self::PageDown),
            _ => None,
        }
    }

    /// Convierte esta tecla al código físico/lógico esperado por `global-hotkey`.
    pub const fn to_global_code(self) -> GlobalCode {
        match self {
            Self::A => GlobalCode::KeyA,
            Self::B => GlobalCode::KeyB,
            Self::C => GlobalCode::KeyC,
            Self::D => GlobalCode::KeyD,
            Self::E => GlobalCode::KeyE,
            Self::F => GlobalCode::KeyF,
            Self::G => GlobalCode::KeyG,
            Self::H => GlobalCode::KeyH,
            Self::I => GlobalCode::KeyI,
            Self::J => GlobalCode::KeyJ,
            Self::K => GlobalCode::KeyK,
            Self::L => GlobalCode::KeyL,
            Self::M => GlobalCode::KeyM,
            Self::N => GlobalCode::KeyN,
            Self::O => GlobalCode::KeyO,
            Self::P => GlobalCode::KeyP,
            Self::Q => GlobalCode::KeyQ,
            Self::R => GlobalCode::KeyR,
            Self::S => GlobalCode::KeyS,
            Self::T => GlobalCode::KeyT,
            Self::U => GlobalCode::KeyU,
            Self::V => GlobalCode::KeyV,
            Self::W => GlobalCode::KeyW,
            Self::X => GlobalCode::KeyX,
            Self::Y => GlobalCode::KeyY,
            Self::Z => GlobalCode::KeyZ,
            Self::Num0 => GlobalCode::Digit0,
            Self::Num1 => GlobalCode::Digit1,
            Self::Num2 => GlobalCode::Digit2,
            Self::Num3 => GlobalCode::Digit3,
            Self::Num4 => GlobalCode::Digit4,
            Self::Num5 => GlobalCode::Digit5,
            Self::Num6 => GlobalCode::Digit6,
            Self::Num7 => GlobalCode::Digit7,
            Self::Num8 => GlobalCode::Digit8,
            Self::Num9 => GlobalCode::Digit9,
            Self::F1 => GlobalCode::F1,
            Self::F2 => GlobalCode::F2,
            Self::F3 => GlobalCode::F3,
            Self::F4 => GlobalCode::F4,
            Self::F5 => GlobalCode::F5,
            Self::F6 => GlobalCode::F6,
            Self::F7 => GlobalCode::F7,
            Self::F8 => GlobalCode::F8,
            Self::F9 => GlobalCode::F9,
            Self::F10 => GlobalCode::F10,
            Self::F11 => GlobalCode::F11,
            Self::F12 => GlobalCode::F12,
            Self::Escape => GlobalCode::Escape,
            Self::Space => GlobalCode::Space,
            Self::Tab => GlobalCode::Tab,
            Self::Enter => GlobalCode::Enter,
            Self::Backspace => GlobalCode::Backspace,
            Self::ArrowUp => GlobalCode::ArrowUp,
            Self::ArrowDown => GlobalCode::ArrowDown,
            Self::ArrowLeft => GlobalCode::ArrowLeft,
            Self::ArrowRight => GlobalCode::ArrowRight,
            Self::Insert => GlobalCode::Insert,
            Self::Delete => GlobalCode::Delete,
            Self::Home => GlobalCode::Home,
            Self::End => GlobalCode::End,
            Self::PageUp => GlobalCode::PageUp,
            Self::PageDown => GlobalCode::PageDown,
        }
    }

    /// Devuelve el nombre canónico en texto de la tecla.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::H => "H",
            Self::I => "I",
            Self::J => "J",
            Self::K => "K",
            Self::L => "L",
            Self::M => "M",
            Self::N => "N",
            Self::O => "O",
            Self::P => "P",
            Self::Q => "Q",
            Self::R => "R",
            Self::S => "S",
            Self::T => "T",
            Self::U => "U",
            Self::V => "V",
            Self::W => "W",
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::Num0 => "0",
            Self::Num1 => "1",
            Self::Num2 => "2",
            Self::Num3 => "3",
            Self::Num4 => "4",
            Self::Num5 => "5",
            Self::Num6 => "6",
            Self::Num7 => "7",
            Self::Num8 => "8",
            Self::Num9 => "9",
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            Self::Escape => "Escape",
            Self::Space => "Space",
            Self::Tab => "Tab",
            Self::Enter => "Enter",
            Self::Backspace => "Backspace",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::Insert => "Insert",
            Self::Delete => "Delete",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
        }
    }

    /// Devuelve una etiqueta compacta para mostrar en botones o chips de la interfaz.
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Escape => "Esc",
            Self::ArrowUp => "↑",
            Self::ArrowDown => "↓",
            Self::ArrowLeft => "←",
            Self::ArrowRight => "→",
            _ => self.as_str(),
        }
    }

    /// Parsea un token de tecla individual sin modificadores (insensible a mayúsculas).
    fn parse_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "a" => Some(Self::A),
            "b" => Some(Self::B),
            "c" => Some(Self::C),
            "d" => Some(Self::D),
            "e" => Some(Self::E),
            "f" => Some(Self::F),
            "g" => Some(Self::G),
            "h" => Some(Self::H),
            "i" => Some(Self::I),
            "j" => Some(Self::J),
            "k" => Some(Self::K),
            "l" => Some(Self::L),
            "m" => Some(Self::M),
            "n" => Some(Self::N),
            "o" => Some(Self::O),
            "p" => Some(Self::P),
            "q" => Some(Self::Q),
            "r" => Some(Self::R),
            "s" => Some(Self::S),
            "t" => Some(Self::T),
            "u" => Some(Self::U),
            "v" => Some(Self::V),
            "w" => Some(Self::W),
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            "0" | "num0" | "digit0" => Some(Self::Num0),
            "1" | "num1" | "digit1" => Some(Self::Num1),
            "2" | "num2" | "digit2" => Some(Self::Num2),
            "3" | "num3" | "digit3" => Some(Self::Num3),
            "4" | "num4" | "digit4" => Some(Self::Num4),
            "5" | "num5" | "digit5" => Some(Self::Num5),
            "6" | "num6" | "digit6" => Some(Self::Num6),
            "7" | "num7" | "digit7" => Some(Self::Num7),
            "8" | "num8" | "digit8" => Some(Self::Num8),
            "9" | "num9" | "digit9" => Some(Self::Num9),
            "f1" => Some(Self::F1),
            "f2" => Some(Self::F2),
            "f3" => Some(Self::F3),
            "f4" => Some(Self::F4),
            "f5" => Some(Self::F5),
            "f6" => Some(Self::F6),
            "f7" => Some(Self::F7),
            "f8" => Some(Self::F8),
            "f9" => Some(Self::F9),
            "f10" => Some(Self::F10),
            "f11" => Some(Self::F11),
            "f12" => Some(Self::F12),
            "esc" | "escape" => Some(Self::Escape),
            "space" | "espacio" => Some(Self::Space),
            "tab" => Some(Self::Tab),
            "enter" | "return" => Some(Self::Enter),
            "backspace" => Some(Self::Backspace),
            "up" | "arrowup" | "arrow_up" => Some(Self::ArrowUp),
            "down" | "arrowdown" | "arrow_down" => Some(Self::ArrowDown),
            "left" | "arrowleft" | "arrow_left" => Some(Self::ArrowLeft),
            "right" | "arrowright" | "arrow_right" => Some(Self::ArrowRight),
            "insert" | "ins" => Some(Self::Insert),
            "delete" | "del" => Some(Self::Delete),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "pageup" | "page_up" | "pgup" => Some(Self::PageUp),
            "pagedown" | "page_down" | "pgdn" => Some(Self::PageDown),
            _ => None,
        }
    }
}

/// Conjunto de modificadores exactos exigidos para activar un atajo de teclado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyModifiers {
    /// Tecla Control (`Ctrl`).
    pub ctrl: bool,
    /// Tecla Mayúsculas (`Shift`).
    pub shift: bool,
    /// Tecla Alt / Option.
    pub alt: bool,
    /// Tecla Super / Command (macOS) / Windows.
    pub super_key: bool,
}

impl KeyModifiers {
    /// Sin modificadores activos.
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
        super_key: false,
    };

    /// Verifica coincidencia exacta con los modificadores de `egui` del frame actual.
    pub fn matches_egui(self, active: egui::Modifiers) -> bool {
        // Comparamos `mac_cmd` para distinguir Cmd de Ctrl en macOS sin falsos positivos.
        active.ctrl == self.ctrl
            && active.shift == self.shift
            && active.alt == self.alt
            && active.mac_cmd == self.super_key
    }

    /// Convierte los modificadores al formato bitflags de `global-hotkey`.
    pub fn to_global_modifiers(self) -> Option<GlobalModifiers> {
        let mut mods = GlobalModifiers::empty();
        if self.ctrl {
            mods |= GlobalModifiers::CONTROL;
        }
        if self.shift {
            mods |= GlobalModifiers::SHIFT;
        }
        if self.alt {
            mods |= GlobalModifiers::ALT;
        }
        if self.super_key {
            mods |= GlobalModifiers::SUPER;
        }
        if mods.is_empty() {
            None
        } else {
            Some(mods)
        }
    }

    /// Intenta aplicar un token de modificador; devuelve `true` si el token fue reconocido.
    fn apply_token(&mut self, token: &str) -> bool {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => {
                self.ctrl = true;
                true
            }
            "shift" => {
                self.shift = true;
                true
            }
            "alt" | "option" => {
                self.alt = true;
                true
            }
            "super" | "cmd" | "command" | "meta" | "win" => {
                self.super_key = true;
                true
            }
            _ => false,
        }
    }

    /// Agrega las etiquetas canónicas de modificadores activos a `parts`.
    fn push_labels(self, parts: &mut Vec<&str>) {
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.super_key {
            parts.push("Super");
        }
    }
}

/// Errores posibles al parsear un atajo de teclado desde texto (por ejemplo `"Ctrl+Shift+1"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyParseError {
    /// La cadena está vacía o solo contiene espacios.
    Empty,
    /// Existe un segmento vacío alrededor de `+` (por ejemplo `"Ctrl+"` o `"+H"`).
    EmptySegment(String),
    /// Se especificó una combinación sin tecla principal (solo modificadores).
    MissingMainKey(String),
    /// Se incluyeron dos o más teclas principales en la misma combinación.
    MultipleMainKeys(String),
    /// Un token no corresponde a ningún modificador ni tecla soportada.
    UnknownToken(String),
}

impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "el atajo de teclado está vacío"),
            Self::EmptySegment(raw) => {
                write!(
                    f,
                    "atajo '{raw}' contiene un segmento vacío alrededor de '+'"
                )
            }
            Self::MissingMainKey(raw) => {
                write!(
                    f,
                    "atajo '{raw}' solo tiene modificadores y falta la tecla principal"
                )
            }
            Self::MultipleMainKeys(raw) => {
                write!(f, "atajo '{raw}' contiene múltiples teclas principales")
            }
            Self::UnknownToken(token) => {
                write!(f, "tecla o modificador no reconocido: '{token}'")
            }
        }
    }
}

impl std::error::Error for KeyParseError {}

/// Clasifica un token individual de teclado como modificador (actualizando `modifiers`) o tecla principal.
fn parse_key_or_modifier(
    token: &str,
    modifiers: &mut KeyModifiers,
) -> Result<Option<KeyCode>, KeyParseError> {
    if modifiers.apply_token(token) {
        Ok(None)
    } else {
        KeyCode::parse_token(token)
            .map(Some)
            .ok_or_else(|| KeyParseError::UnknownToken(token.to_string()))
    }
}

/// Convierte un error genérico de división de combos en [`KeyParseError`].
fn map_combo_key_error(err: ComboSplitError<KeyParseError>) -> KeyParseError {
    match err {
        ComboSplitError::Empty => KeyParseError::Empty,
        ComboSplitError::EmptySegment(raw) => KeyParseError::EmptySegment(raw),
        ComboSplitError::Token(inner) => inner,
    }
}

/// Combinación de tecla principal y modificadores requerida para activar una acción.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyTrigger {
    /// Tecla principal del atajo.
    pub key: KeyCode,
    /// Modificadores exactos exigidos.
    pub modifiers: KeyModifiers,
}

impl KeyTrigger {
    /// Crea un disparador con tecla y modificadores explícitos.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn new(key: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { key, modifiers }
    }

    /// Crea un disparador para una tecla sin modificadores activos.
    pub const fn key_only(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Parsea un string como `"H"`, `"1"`, `"Ctrl+Shift+1"`, `"F5"` o `"Escape"`.
    pub fn parse(raw: &str) -> Result<Self, KeyParseError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(KeyParseError::Empty);
        }

        let mut modifiers = KeyModifiers::NONE;
        let (raw_joined, tokens) = parse_combo_tokens(&[trimmed], |token| {
            parse_key_or_modifier(token, &mut modifiers)
        })
        .map_err(map_combo_key_error)?;

        let mut main_key: Option<KeyCode> = None;
        for key in tokens.into_iter().flatten() {
            if main_key.is_some() {
                return Err(KeyParseError::MultipleMainKeys(raw_joined));
            }
            main_key = Some(key);
        }

        let key = main_key.ok_or(KeyParseError::MissingMainKey(raw_joined))?;
        Ok(Self { key, modifiers })
    }

    /// Formatea el atajo de forma compacta para etiquetas de UI (por ejemplo `"Esc"` o `"Ctrl+1"`).
    pub fn format_short(&self) -> String {
        self.format_with_key(self.key.short_label())
    }

    /// Formatea el atajo uniendo los modificadores activos en orden canónico y la etiqueta de tecla.
    fn format_with_key(&self, key_label: &str) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(5);
        self.modifiers.push_labels(&mut parts);
        parts.push(key_label);
        parts.join("+")
    }
}

impl fmt::Display for KeyTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_with_key(self.key.as_str()))
    }
}

impl FromStr for KeyTrigger {
    type Err = KeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Combinación multi-tecla compuesta por modificadores opcionales y un conjunto canónico de teclas.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    /// Modificadores exactos exigidos al activar el combo.
    pub modifiers: KeyModifiers,
    /// Conjunto canónico ordenado y deduplicado de teclas principales simultáneas.
    pub keys: Chord<KeyCode>,
}

impl KeyChord {
    /// Crea un combo multi-tecla a partir de modificadores y un [`Chord`] ya normalizado.
    pub const fn new(modifiers: KeyModifiers, keys: Chord<KeyCode>) -> Self {
        Self { modifiers, keys }
    }

    /// Parsea una lista de partes (`["A", "B"]`, `["Ctrl", "A", "B"]` o `["Ctrl+A", "B"]`).
    pub fn parse_parts<S: AsRef<str>>(parts: &[S]) -> Result<Self, KeyParseError> {
        let mut modifiers = KeyModifiers::NONE;
        let (raw_joined, tokens) =
            parse_combo_tokens(parts, |token| parse_key_or_modifier(token, &mut modifiers))
                .map_err(map_combo_key_error)?;

        let chord = Chord::from_atoms(tokens.into_iter().flatten())
            .ok_or(KeyParseError::MissingMainKey(raw_joined))?;
        Ok(Self::new(modifiers, chord))
    }

    /// Parsea un combo multi-tecla desde una cadena separada por `+` (por ejemplo `"Ctrl+A+B"`).
    pub fn parse_str(raw: &str) -> Result<Self, KeyParseError> {
        if raw.trim().is_empty() {
            return Err(KeyParseError::Empty);
        }
        Self::parse_parts(&[raw])
    }

    /// Formatea el combo de manera compacta para mostrar en la interfaz (por ejemplo `"Ctrl+A+B"`).
    pub fn format_short(&self) -> String {
        self.format_with(|k| k.short_label())
    }

    fn format_with(&self, label_fn: fn(KeyCode) -> &'static str) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(4 + self.keys.len());
        self.modifiers.push_labels(&mut parts);
        for &key in self.keys.atoms() {
            parts.push(label_fn(key));
        }
        parts.join("+")
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_with(KeyCode::as_str))
    }
}

/// Ámbito en el que escucha un atajo de teclado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyScope {
    /// Atajo global del sistema operativo (funciona incluso sin foco en la ventana).
    Global,
    /// Atajo local a la ventana de `egui` (solo cuando la aplicación tiene foco).
    #[default]
    Window,
}

/// Disparador local de teclado aceptado por [`EguiKeyboardBackend`] (tecla simple o combo).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindowKeyboardTrigger {
    /// Atajo tradicional de una tecla principal con modificadores opcionales.
    Single(KeyTrigger),
    /// Combo multi-tecla simultáneo.
    Chord(KeyChord),
}

impl WindowKeyboardTrigger {
    fn modifiers(&self) -> KeyModifiers {
        match self {
            Self::Single(t) => t.modifiers,
            Self::Chord(c) => c.modifiers,
        }
    }

    fn key_chord(&self) -> Chord<KeyCode> {
        match self {
            Self::Single(t) => Chord::single(t.key),
            Self::Chord(c) => c.keys.clone(),
        }
    }

    fn into_trigger_source(self) -> TriggerSource {
        match self {
            Self::Single(trigger) => TriggerSource::Keyboard {
                trigger,
                scope: HotkeyScope::Window,
            },
            Self::Chord(chord) => TriggerSource::KeyboardChord {
                chord,
                scope: HotkeyScope::Window,
            },
        }
    }
}

impl From<KeyTrigger> for WindowKeyboardTrigger {
    fn from(trigger: KeyTrigger) -> Self {
        Self::Single(trigger)
    }
}

impl From<KeyChord> for WindowKeyboardTrigger {
    fn from(chord: KeyChord) -> Self {
        Self::Chord(chord)
    }
}

/// Agrupa todos los disparadores locales que comparten exactamente los mismos [`KeyModifiers`].
#[derive(Debug, Default)]
struct ModifierChordGroup {
    tracker: ChordTracker<KeyCode>,
    chord_sources: BTreeMap<Chord<KeyCode>, Vec<TriggerSource>>,
}

/// Backend de teclado local a la ventana usando la cola cruda `input.events` de `egui`.
///
/// Utiliza [`ChordTracker`] para soportar tanto teclas individuales como combinaciones multi-tecla
/// con prioridad por aridad máxima y liberación automática al perder el foco.
#[derive(Debug, Default)]
pub struct EguiKeyboardBackend {
    /// Grupos ordenados por modificadores para emitir eventos determinísticos.
    groups: BTreeMap<KeyModifiers, ModifierChordGroup>,
}

impl EguiKeyboardBackend {
    /// Crea un backend de ventana con la lista de disparadores locales a vigilar.
    pub fn new<I, T>(triggers: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<WindowKeyboardTrigger>,
    {
        let mut backend = Self::default();
        backend.set_triggers(triggers);
        backend
    }

    /// Reemplaza la lista de disparadores locales tras una recarga de configuración.
    pub fn set_triggers<I, T>(&mut self, triggers: I)
    where
        I: IntoIterator<Item = T>,
        T: Into<WindowKeyboardTrigger>,
    {
        self.groups.clear();

        for item in triggers {
            let wt: WindowKeyboardTrigger = item.into();
            let mods = wt.modifiers();
            let chord = wt.key_chord();
            let source = wt.into_trigger_source();

            let group = self.groups.entry(mods).or_default();
            let targets = group.chord_sources.entry(chord).or_default();
            if !targets.contains(&source) {
                targets.push(source);
            }
        }

        for group in self.groups.values_mut() {
            let chords: Vec<Chord<KeyCode>> = group.chord_sources.keys().cloned().collect();
            let _ = group.tracker.set_chords(chords);
        }
    }

    /// Procesa eventos de teclado puros sin requerir un contexto gráfico (`egui::Context`).
    ///
    /// - Ignora eventos con `repeat: true` para no alternar toggles al mantener pulsada una tecla.
    /// - Respeta `wants_keyboard_input` en pulsaciones para no interferir con campos de texto.
    /// - Si la ventana pierde el foco (`!focused`), libera todos los disparadores activos.
    pub fn process_input(
        &mut self,
        wants_keyboard_input: bool,
        focused: bool,
        events: &[egui::Event],
    ) -> Vec<TriggerEvent> {
        let mut out = Vec::new();

        // Si la ventana perdió el foco, liberamos cualquier tecla o combo sostenido.
        if !focused {
            for group in self.groups.values_mut() {
                for transition in group.tracker.release_all() {
                    push_group_transition(&group.chord_sources, transition, &mut out);
                }
            }
            return out;
        }

        for event in events {
            let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
            else {
                continue;
            };

            let Some(key_code) = KeyCode::from_egui_key(*key) else {
                continue;
            };

            if *pressed {
                // Filtramos repeticiones automáticas del sistema y respetamos widgets de texto activos.
                if *repeat || wants_keyboard_input {
                    continue;
                }

                for (&req_mods, group) in &mut self.groups {
                    if req_mods.matches_egui(*modifiers) {
                        for transition in group.tracker.press(key_code) {
                            push_group_transition(&group.chord_sources, transition, &mut out);
                        }
                    }
                }
            } else {
                // Al soltar una tecla física, liberamos cualquier chord activo que la contenga
                // en todos los grupos, incluso si el usuario soltó el modificador antes que la tecla.
                for group in self.groups.values_mut() {
                    for transition in group.tracker.release(key_code) {
                        push_group_transition(&group.chord_sources, transition, &mut out);
                    }
                }
            }
        }

        out
    }
}

/// Expande una transición de [`ChordTracker`] en los [`TriggerEvent`] asociados.
fn push_group_transition(
    chord_sources: &BTreeMap<Chord<KeyCode>, Vec<TriggerSource>>,
    transition: super::chord::ChordTransition<KeyCode>,
    out: &mut Vec<TriggerEvent>,
) {
    if let Some(sources) = chord_sources.get(&transition.chord) {
        for source in sources {
            out.push(TriggerEvent {
                source: source.clone(),
                phase: transition.phase,
            });
        }
    }
}

impl TriggerBackend for EguiKeyboardBackend {
    fn poll_events(&mut self, ctx: &egui::Context) -> Vec<TriggerEvent> {
        let wants_keyboard_input = ctx.wants_keyboard_input();
        ctx.input(|input| self.process_input(wants_keyboard_input, input.focused, &input.events))
    }
}

/// Backend de atajos globales del sistema operativo respaldado por `global-hotkey`.
pub struct GlobalHotkeyBackend {
    manager: Option<GlobalHotKeyManager>,
    registered_hotkeys: Vec<HotKey>,
    id_to_trigger: HashMap<u32, KeyTrigger>,
    pressed_ids: HashSet<u32>,
}

impl GlobalHotkeyBackend {
    /// Crea un backend deshabilitado (útil para pruebas unitarias puras sin tocar el SO).
    #[cfg(test)]
    pub fn disabled_for_tests() -> Self {
        Self {
            manager: None,
            registered_hotkeys: Vec::new(),
            id_to_trigger: HashMap::new(),
            pressed_ids: HashSet::new(),
        }
    }

    /// Inicializa el administrador global en el hilo principal y registra los atajos solicitados.
    ///
    /// Devuelve los triggers que sí quedaron registrados como globales y una lista de avisos
    /// legibles para cualquier atajo que deba degradarse a ámbito `Window`.
    pub fn new(global_triggers: &[KeyTrigger]) -> (Self, HashSet<KeyTrigger>, Vec<String>) {
        let mut backend = match GlobalHotKeyManager::new() {
            Ok(manager) => Self {
                manager: Some(manager),
                registered_hotkeys: Vec::new(),
                id_to_trigger: HashMap::new(),
                pressed_ids: HashSet::new(),
            },
            Err(err) => {
                let mut warnings = Vec::new();
                if !global_triggers.is_empty() {
                    warnings.push(format!(
                        "No se pudo inicializar el gestor de atajos globales ({err}); los atajos globales funcionarán solo con la ventana enfocada."
                    ));
                }
                return (
                    Self {
                        manager: None,
                        registered_hotkeys: Vec::new(),
                        id_to_trigger: HashMap::new(),
                        pressed_ids: HashSet::new(),
                    },
                    HashSet::new(),
                    warnings,
                );
            }
        };

        let (registered, warnings) = backend.sync_triggers(global_triggers);
        (backend, registered, warnings)
    }

    /// Reemplaza los atajos globales registrados en el sistema y reporta fallos individuales.
    pub fn sync_triggers(
        &mut self,
        global_triggers: &[KeyTrigger],
    ) -> (HashSet<KeyTrigger>, Vec<String>) {
        self.unregister_all();
        self.pressed_ids.clear();

        // Vaciamos eventos globales rezagados para evitar disparos fantasma tras recargar.
        while GlobalHotKeyEvent::receiver().try_recv().is_ok() {}

        let mut registered_set = HashSet::new();
        let mut warnings = Vec::new();

        let Some(manager) = self.manager.as_ref() else {
            if !global_triggers.is_empty() {
                warnings.push(
                    "El gestor de atajos globales no está disponible; se usará escucha local en ventana."
                        .to_string(),
                );
            }
            return (registered_set, warnings);
        };

        // Deduplicamos combinaciones antes de registrar para no chocar con nosotros mismos.
        let mut seen = HashSet::new();
        for &trigger in global_triggers {
            if !seen.insert(trigger) {
                continue;
            }

            let hotkey = HotKey::new(
                trigger.modifiers.to_global_modifiers(),
                trigger.key.to_global_code(),
            );
            match manager.register(hotkey) {
                Ok(()) => {
                    self.id_to_trigger.insert(hotkey.id(), trigger);
                    self.registered_hotkeys.push(hotkey);
                    registered_set.insert(trigger);
                }
                Err(err) => {
                    warnings.push(format!(
                        "No se pudo registrar el atajo global '{trigger}' ({err}); se degradó a ámbito de ventana."
                    ));
                }
            }
        }

        (registered_set, warnings)
    }

    /// Libera del sistema operativo todos los atajos actualmente registrados.
    fn unregister_all(&mut self) {
        if let Some(manager) = self.manager.as_ref() {
            if !self.registered_hotkeys.is_empty() {
                let _ = manager.unregister_all(&self.registered_hotkeys);
            }
        }
        self.registered_hotkeys.clear();
        self.id_to_trigger.clear();
    }
}

impl Drop for GlobalHotkeyBackend {
    fn drop(&mut self) {
        self.unregister_all();
    }
}

impl TriggerBackend for GlobalHotkeyBackend {
    fn poll_events(&mut self, _ctx: &egui::Context) -> Vec<TriggerEvent> {
        if self.id_to_trigger.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::new();
        while let Ok(global_event) = GlobalHotKeyEvent::receiver().try_recv() {
            let Some(&trigger) = self.id_to_trigger.get(&global_event.id) else {
                continue;
            };

            match global_event.state {
                HotKeyState::Pressed => {
                    // Evitamos múltiples `Pressed` si el sistema envía repetición de teclado global.
                    if self.pressed_ids.insert(global_event.id) {
                        events.push(TriggerEvent {
                            source: TriggerSource::Keyboard {
                                trigger,
                                scope: HotkeyScope::Global,
                            },
                            phase: TriggerPhase::Pressed,
                        });
                    }
                }
                HotKeyState::Released => {
                    self.pressed_ids.remove(&global_event.id);
                    events.push(TriggerEvent {
                        source: TriggerSource::Keyboard {
                            trigger,
                            scope: HotkeyScope::Global,
                        },
                        phase: TriggerPhase::Released,
                    });
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(
        key: egui::Key,
        pressed: bool,
        repeat: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn parser_y_formateador_de_teclas() {
        assert_eq!(KeyTrigger::parse("h"), Ok(KeyTrigger::key_only(KeyCode::H)));
        assert_eq!(
            KeyTrigger::parse("1"),
            Ok(KeyTrigger::key_only(KeyCode::Num1))
        );
        assert_eq!(
            KeyTrigger::parse("f5"),
            Ok(KeyTrigger::key_only(KeyCode::F5))
        );
        assert_eq!(
            KeyTrigger::parse("esc"),
            Ok(KeyTrigger::key_only(KeyCode::Escape))
        );

        let combo = KeyTrigger::parse("shift + ctrl + 1").expect("combinación válida");
        assert_eq!(
            combo,
            KeyTrigger::new(
                KeyCode::Num1,
                KeyModifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                }
            )
        );
        assert_eq!(combo.to_string(), "Ctrl+Shift+1");
        assert_eq!(combo.format_short(), "Ctrl+Shift+1");
        assert_eq!(KeyTrigger::key_only(KeyCode::Escape).format_short(), "Esc");

        // Casos inválidos deben retornar error claro sin pánico.
        assert_eq!(KeyTrigger::parse("  "), Err(KeyParseError::Empty));
        assert!(matches!(
            KeyTrigger::parse("Ctrl+"),
            Err(KeyParseError::EmptySegment(_))
        ));
        assert!(matches!(
            KeyTrigger::parse("Ctrl+Shift"),
            Err(KeyParseError::MissingMainKey(_))
        ));
        assert!(matches!(
            KeyTrigger::parse("A+B"),
            Err(KeyParseError::MultipleMainKeys(_))
        ));
        assert!(matches!(
            KeyTrigger::parse("TeclaInventada"),
            Err(KeyParseError::UnknownToken(_))
        ));
    }

    #[test]
    fn key_chord_parsea_partes_y_cadenas_y_normaliza_orden() {
        let ab = KeyChord::parse_str("B+A").expect("combo A+B");
        let ab_parts = KeyChord::parse_parts(&["A", "B"]).expect("partes A+B");
        assert_eq!(ab, ab_parts);
        assert_eq!(ab.format_short(), "A+B");

        let ctrl_ab = KeyChord::parse_parts(&["Ctrl", "B", "A"]).expect("Ctrl+A+B");
        assert!(ctrl_ab.modifiers.ctrl);
        assert_eq!(ctrl_ab.format_short(), "Ctrl+A+B");

        assert_eq!(KeyChord::parse_str(" "), Err(KeyParseError::Empty));
        assert_eq!(
            KeyChord::parse_parts::<&str>(&[]),
            Err(KeyParseError::Empty)
        );
        assert!(matches!(
            KeyChord::parse_parts(&["Ctrl", "Shift"]),
            Err(KeyParseError::MissingMainKey(_))
        ));
    }

    #[test]
    fn egui_backend_ignora_repeticion_y_libera_al_perder_foco() {
        let t_toggle = KeyTrigger::key_only(KeyCode::H);
        let t_momentary = KeyTrigger::new(
            KeyCode::Num2,
            KeyModifiers {
                ctrl: true,
                ..KeyModifiers::NONE
            },
        );
        let mut backend = EguiKeyboardBackend::new(vec![t_toggle, t_momentary]);

        let key_h_press = key_event(egui::Key::H, true, false, egui::Modifiers::NONE);
        let key_h_repeat = key_event(egui::Key::H, true, true, egui::Modifiers::NONE);
        let key_h_release = key_event(egui::Key::H, false, false, egui::Modifiers::NONE);

        // Si un campo de texto quiere teclado, se ignoran las pulsaciones.
        assert!(backend
            .process_input(true, true, std::slice::from_ref(&key_h_press))
            .is_empty());

        // Con foco y sin repeat, emite Pressed; la repetición se descarta.
        let events =
            backend.process_input(false, true, &[key_h_press, key_h_repeat, key_h_release]);
        assert_eq!(
            events,
            vec![
                TriggerEvent {
                    source: TriggerSource::Keyboard {
                        trigger: t_toggle,
                        scope: HotkeyScope::Window,
                    },
                    phase: TriggerPhase::Pressed,
                },
                TriggerEvent {
                    source: TriggerSource::Keyboard {
                        trigger: t_toggle,
                        scope: HotkeyScope::Window,
                    },
                    phase: TriggerPhase::Released,
                },
            ]
        );

        // Si se presiona Ctrl+2 y luego se pierde foco sin evento Released, se libera automáticamente.
        let ctrl_2_press = key_event(egui::Key::Num2, true, false, egui::Modifiers::CTRL);
        let pressed_events = backend.process_input(false, true, &[ctrl_2_press]);
        assert_eq!(pressed_events.len(), 1);
        assert_eq!(pressed_events[0].phase, TriggerPhase::Pressed);

        // Un segundo Pressed consecutivo sin Released intermedio (incluso con `repeat = false`) no se duplica.
        let dup_press = key_event(egui::Key::Num2, true, false, egui::Modifiers::CTRL);
        assert!(backend.process_input(false, true, &[dup_press]).is_empty());

        let blur_events = backend.process_input(false, false, &[]);
        assert_eq!(
            blur_events,
            vec![TriggerEvent {
                source: TriggerSource::Keyboard {
                    trigger: t_momentary,
                    scope: HotkeyScope::Window,
                },
                phase: TriggerPhase::Released,
            }]
        );
    }

    #[test]
    fn egui_backend_soporta_combos_multitecla_y_suprime_subconjuntos() {
        let single_a = KeyTrigger::key_only(KeyCode::A);
        let chord_ab = KeyChord::parse_str("A+B").expect("A+B");
        let mut backend = EguiKeyboardBackend::new([
            WindowKeyboardTrigger::Single(single_a),
            WindowKeyboardTrigger::Chord(chord_ab.clone()),
        ]);

        let press_a = key_event(egui::Key::A, true, false, egui::Modifiers::NONE);
        let press_b = key_event(egui::Key::B, true, false, egui::Modifiers::NONE);
        let release_b = key_event(egui::Key::B, false, false, egui::Modifiers::NONE);
        let release_a = key_event(egui::Key::A, false, false, egui::Modifiers::NONE);

        // Al pulsar A se activa A.
        assert_eq!(
            backend.process_input(false, true, &[press_a]),
            vec![TriggerEvent {
                source: TriggerSource::Keyboard {
                    trigger: single_a,
                    scope: HotkeyScope::Window,
                },
                phase: TriggerPhase::Pressed,
            }]
        );

        // Al pulsar B sin soltar A: libera A y activa A+B.
        assert_eq!(
            backend.process_input(false, true, &[press_b]),
            vec![
                TriggerEvent {
                    source: TriggerSource::Keyboard {
                        trigger: single_a,
                        scope: HotkeyScope::Window,
                    },
                    phase: TriggerPhase::Released,
                },
                TriggerEvent {
                    source: TriggerSource::KeyboardChord {
                        chord: chord_ab.clone(),
                        scope: HotkeyScope::Window,
                    },
                    phase: TriggerPhase::Pressed,
                },
            ]
        );

        // Al soltar B: libera A+B sin re-disparar A.
        assert_eq!(
            backend.process_input(false, true, &[release_b]),
            vec![TriggerEvent {
                source: TriggerSource::KeyboardChord {
                    chord: chord_ab,
                    scope: HotkeyScope::Window,
                },
                phase: TriggerPhase::Released,
            }]
        );

        // Al soltar A no hay eventos pendientes.
        assert!(backend.process_input(false, true, &[release_a]).is_empty());
    }
}
