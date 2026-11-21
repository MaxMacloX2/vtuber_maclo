//! Configuración centralizada y carga/generación de `vtuber_maclo.toml` con validación segura.

use crate::avatar::{AvatarProfile, AvatarStateId, ExpressionSprites, SpriteSource};
use crate::input::{
    default_bindings, GamepadTrigger, HotkeyScope, InputAction, InputBinding, KeyChord, KeyTrigger,
    TriggerSource,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Nombre predeterminado del archivo de configuración externa.
pub const DEFAULT_CONFIG_FILENAME: &str = "vtuber_maclo.toml";

/// Plantilla inicial bien comentada en español que se genera si `vtuber_maclo.toml` no existe.
const DEFAULT_CONFIG_TEMPLATE: &str = r#"# Configuración de vtuber_maclo
# Puedes editar este archivo en caliente y pulsar "Recargar config" en los controles.

[window]
title = "Vtuber Maclo"
inner_size = [450.0, 500.0]
transparent = true
decorations = true

[audio]
# Umbral inicial de volumen RMS para detectar voz.
default_threshold = 0.02
# Rango permitido en el control deslizante de umbral.
threshold_min = 0.001
threshold_max = 0.2
# Tiempo en milisegundos que se mantiene el estado de habla al bajar del umbral.
hold_ms = 120

[animation]
# Límite máximo de delta de tiempo (segundos) para estabilidad física.
max_dt = 0.05
# Impulso vertical al empezar a hablar (negativo = hacia arriba).
bounce_impulse_y = -18.0
# Velocidad de retorno exponencial hacia el reposo.
bounce_damping_speed = 13.5
# Margen superior base del avatar en puntos lógicos.
avatar_base_top_space = 30.0
# Ancho máximo base del sprite del avatar.
avatar_max_width = 350.0

# Estados del avatar.
# Si omites `png`, los estados integrados usan los sprites embebidos en el binario.
# Puedes indicar rutas relativas a este archivo (por ejemplo `png = "assets/happy.png"`).

[states.idle]
display_name = "Reposo"
# png = "assets/idle.png"

[states.talk]
display_name = "Hablando"
# png = "assets/talk.png"

[states.happy]
display_name = "Feliz"
# png = "assets/happy.png"
# talk_png = "assets/happy_talk.png"

[states.surprised]
display_name = "Sorprendido"
# png = "assets/surprised.png"
# talk_png = "assets/surprised_talk.png"

# Atajos de teclado y control externo configurables:
# - device: "keyboard" (predeterminado) o "gamepad" (mando Xbox / PlayStation / genérico)
# - scope (solo teclado): "global" (funciona sin foco) o "window" (solo con la ventana enfocada)
# - mode: "toggle" (encendido/apagado) o "momentary" (mientras se mantiene pulsada)
# - action: "toggle_controls", "expression", "clear_expression" o "reload_config"

[[bindings]]
key = "H"
scope = "window"
mode = "toggle"
action = "toggle_controls"

[[bindings]]
key = "1"
scope = "global"
mode = "toggle"
action = "expression"
state = "happy"

[[bindings]]
key = "2"
scope = "global"
mode = "momentary"
action = "expression"
state = "surprised"

[[bindings]]
key = "Escape"
scope = "window"
mode = "toggle"
action = "clear_expression"

# Ejemplo de combo multi-tecla (solo en ventana enfocada):
# [[bindings]]
# combo = ["J", "K"]
# scope = "window"
# mode = "momentary"
# action = "expression"
# state = "surprised"

# Ejemplos para mando externo (Xbox / gamepad):
# [[bindings]]
# device = "gamepad"
# button = "A"
# mode = "toggle"
# action = "expression"
# state = "happy"
#
# [[bindings]]
# device = "gamepad"
# combo = "LB+Y"
# mode = "momentary"
# action = "expression"
# state = "surprised"
#
# [[bindings]]
# device = "gamepad"
# button = "LT"
# threshold = 0.65
# gamepad_index = 0
# mode = "momentary"
# action = "expression"
# state = "happy"
"#;

/// Advertencia no fatal producida al cargar, validar o aplicar la configuración externa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigWarning {
    /// Mensaje descriptivo legible para mostrar en la interfaz o registros.
    pub message: String,
}

impl ConfigWarning {
    /// Crea una nueva advertencia con el texto indicado.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Resultado completo de cargar o generar `vtuber_maclo.toml`.
pub struct LoadedConfig {
    /// Configuración validada de ventana, audio y animaciones.
    pub app_config: AppConfig,
    /// Perfil de avatar precargado con los sprites validados y respaldos embebidos.
    pub avatar_profile: AvatarProfile,
    /// Enlaces de teclado validados.
    pub bindings: Vec<InputBinding>,
    /// Lista de advertencias no fatales detectadas durante la carga.
    pub warnings: Vec<ConfigWarning>,
    /// Ruta del archivo de configuración utilizado.
    pub config_path: PathBuf,
}

/// Configuración raíz validada de la aplicación que agrupa ventana, audio y animaciones.
#[derive(Clone, Debug, Default)]
pub struct AppConfig {
    /// Parámetros de la ventana nativa (`eframe`).
    pub window: WindowConfig,
    /// Parámetros de detección de voz y rango de sensibilidad del micrófono.
    pub audio: AudioConfig,
    /// Parámetros físicos y de diseño para las animaciones del avatar.
    pub animation: AnimationConfig,
}

/// Configuración de la ventana principal transparente para captura en OBS.
#[derive(Clone, Debug)]
pub struct WindowConfig {
    /// Título visible de la ventana nativa.
    pub title: String,
    /// Tamaño inicial interior `[ancho, alto]` en puntos lógicos.
    pub inner_size: [f32; 2],
    /// Habilita fondo completamente transparente para superposición en OBS.
    pub transparent: bool,
    /// Muestra u oculta la barra de título y bordes del sistema operativo.
    pub decorations: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Vtuber Maclo".to_string(),
            inner_size: [450.0, 500.0],
            transparent: true,
            decorations: true,
        }
    }
}

/// Configuración validada para el detector de actividad de voz (VAD).
#[derive(Clone, Debug)]
pub struct AudioConfig {
    /// Umbral inicial de volumen RMS a partir del cual se considera que hay voz.
    pub default_threshold: f32,
    /// Rango permitido en el control deslizante de umbral de la interfaz.
    pub threshold_range: RangeInclusive<f32>,
    /// Tiempo de retención tras caer bajo el umbral antes de pasar a estado inactivo.
    pub hold_duration: Duration,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            default_threshold: 0.02,
            threshold_range: 0.001..=0.2,
            hold_duration: Duration::from_millis(120),
        }
    }
}

/// Configuración validada de animaciones y geometría base del avatar en pantalla.
#[derive(Clone, Debug)]
pub struct AnimationConfig {
    /// Límite superior para `dt` (en segundos) para evitar saltos tras pausas del SO.
    pub max_dt: f32,
    /// Impulso vertical inicial (negativo = hacia arriba) al empezar a hablar.
    pub bounce_impulse_y: f32,
    /// Velocidad del amortiguamiento exponencial del rebote hacia el reposo.
    pub bounce_damping_speed: f32,
    /// Margen superior base en puntos lógicos sobre el sprite del avatar.
    pub avatar_base_top_space: f32,
    /// Ancho máximo base en puntos lógicos del sprite del avatar.
    pub avatar_max_width: f32,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            max_dt: 0.05,
            bounce_impulse_y: -18.0,
            bounce_damping_speed: 13.5,
            avatar_base_top_space: 30.0,
            avatar_max_width: 350.0,
        }
    }
}

/// DTO raíz deserializado desde `vtuber_maclo.toml` antes de sanitizar y validar.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ExternalConfigDto {
    /// Sección `[window]`.
    pub window: WindowConfigDto,
    /// Sección `[audio]`.
    pub audio: AudioConfigDto,
    /// Sección `[animation]`.
    pub animation: AnimationConfigDto,
    /// Tabla `[states.<id>]` de estados y expresiones del avatar.
    pub states: BTreeMap<String, StateConfigDto>,
    /// Arreglo `[[bindings]]` de atajos configurados; `None` indica usar los predeterminados.
    pub bindings: Option<Vec<BindingConfigDto>>,
}

/// DTO deserializado de `[window]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct WindowConfigDto {
    /// Título solicitado para la ventana.
    pub title: String,
    /// Dimensiones solicitadas `[ancho, alto]`.
    pub inner_size: [f32; 2],
    /// Fondo transparente solicitado.
    pub transparent: bool,
    /// Bordes nativos solicitados.
    pub decorations: bool,
}

impl Default for WindowConfigDto {
    fn default() -> Self {
        let def = WindowConfig::default();
        Self {
            title: def.title,
            inner_size: def.inner_size,
            transparent: def.transparent,
            decorations: def.decorations,
        }
    }
}

/// DTO deserializado de `[audio]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AudioConfigDto {
    /// Umbral inicial solicitado.
    pub default_threshold: f32,
    /// Mínimo del slider de umbral.
    pub threshold_min: f32,
    /// Máximo del slider de umbral.
    pub threshold_max: f32,
    /// Tiempo de retención en milisegundos.
    pub hold_ms: u64,
}

impl Default for AudioConfigDto {
    fn default() -> Self {
        let def = AudioConfig::default();
        Self {
            default_threshold: def.default_threshold,
            threshold_min: *def.threshold_range.start(),
            threshold_max: *def.threshold_range.end(),
            hold_ms: def.hold_duration.as_millis() as u64,
        }
    }
}

/// DTO deserializado de `[animation]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AnimationConfigDto {
    /// Límite máximo de `dt`.
    pub max_dt: f32,
    /// Impulso vertical del rebote.
    pub bounce_impulse_y: f32,
    /// Velocidad de amortiguamiento del rebote.
    pub bounce_damping_speed: f32,
    /// Margen superior base del avatar.
    pub avatar_base_top_space: f32,
    /// Ancho máximo base del avatar.
    pub avatar_max_width: f32,
}

impl Default for AnimationConfigDto {
    fn default() -> Self {
        let def = AnimationConfig::default();
        Self {
            max_dt: def.max_dt,
            bounce_impulse_y: def.bounce_impulse_y,
            bounce_damping_speed: def.bounce_damping_speed,
            avatar_base_top_space: def.avatar_base_top_space,
            avatar_max_width: def.avatar_max_width,
        }
    }
}

/// DTO deserializado para cada sección `[states.<id>]`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct StateConfigDto {
    /// Nombre visible opcional para la interfaz de controles.
    pub display_name: Option<String>,
    /// Ruta opcional al PNG de reposo/base de esta expresión.
    pub png: Option<String>,
    /// Ruta opcional al PNG usado cuando se habla con esta expresión activa.
    pub talk_png: Option<String>,
}

/// Acción declarada en una entrada `[[bindings]]` del TOML.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingActionDto {
    /// Mostrar u ocultar controles flotantes.
    ToggleControls,
    /// Activar una expresión indicada en el campo `state`.
    #[serde(alias = "activate_expression")]
    Expression,
    /// Volver a estado neutro limpiando overrides.
    ClearExpression,
    /// Recargar el archivo de configuración desde disco.
    ReloadConfig,
}

/// Dispositivo de entrada asociado a una entrada `[[bindings]]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingDeviceDto {
    /// Teclado local o global (valor predeterminado para retrocompatibilidad).
    #[default]
    Keyboard,
    /// Mando externo o controlador Xbox (`gilrs`).
    Gamepad,
}

/// Representa un combo multi-tecla o multi-botón ya sea como lista `["A", "B"]` o cadena `"A+B"`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ComboInputDto {
    /// Arreglo explícito en TOML (por ejemplo `combo = ["LB", "Y"]`).
    List(Vec<String>),
    /// Cadena separada por `+` (por ejemplo `combo = "LB+Y"`).
    Single(String),
}

/// DTO deserializado para cada entrada `[[bindings]]`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct BindingConfigDto {
    /// Dispositivo de entrada (`keyboard` o `gamepad`).
    pub device: BindingDeviceDto,
    /// Combinación de teclado con una tecla principal (por ejemplo `"1"`, `"H"`, `"Ctrl+Shift+1"`).
    pub key: Option<String>,
    /// Botón o eje de gamepad individual (por ejemplo `"A"`, `"LB"`, `"LT"`).
    pub button: Option<String>,
    /// Combo multi-tecla o multi-botón (`["A", "B"]` o `"LB+Y"`).
    pub combo: Option<ComboInputDto>,
    /// Índice opcional de mando (`0`, `1`, ...); `None` escucha cualquier mando conectado.
    pub gamepad_index: Option<usize>,
    /// Umbral analógico en `[0.1, 0.95]` para gatillos/sticks de gamepad (por defecto `0.5`).
    pub threshold: Option<f32>,
    /// Ámbito (`global` o `window`, aplica a teclado).
    pub scope: HotkeyScope,
    /// Modo de activación (`toggle` o `momentary`).
    pub mode: crate::avatar::ActivationMode,
    /// Acción a ejecutar.
    pub action: Option<BindingActionDto>,
    /// Estado objetivo cuando `action = "expression"`.
    pub state: Option<String>,
}

/// Resuelve la ruta predeterminada de `vtuber_maclo.toml`.
///
/// Soporta `--config <ruta>` o `--config=<ruta>` en los argumentos del proceso. Si no se pasa,
/// busca `vtuber_maclo.toml` en el directorio actual (CWD) o junto al ejecutable.
pub fn resolve_default_config_path() -> PathBuf {
    let cwd = std::env::current_dir().ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));

    resolve_config_path_from_args(std::env::args_os(), cwd.as_deref(), exe_dir.as_deref())
}

/// Lógica pura para resolver la ruta de configuración dada una lista de argumentos, CWD y exe_dir.
pub fn resolve_config_path_from_args<I>(
    args: I,
    cwd: Option<&Path>,
    exe_dir: Option<&Path>,
) -> PathBuf
where
    I: IntoIterator<Item = OsString>,
{
    let mut iter = args.into_iter();
    // Saltamos el argumento 0 (nombre del binario).
    let _ = iter.next();

    while let Some(arg) = iter.next() {
        let lossy = arg.to_string_lossy();
        if lossy == "--config" {
            if let Some(next) = iter.next() {
                if !next.is_empty() {
                    return PathBuf::from(next);
                }
            }
        } else if let Some(rest) = lossy.strip_prefix("--config=") {
            if !rest.is_empty() {
                return PathBuf::from(rest);
            }
        }
    }

    if let Some(dir) = cwd {
        let candidate = dir.join(DEFAULT_CONFIG_FILENAME);
        if candidate.exists() {
            return candidate;
        }
    }

    if let Some(dir) = exe_dir {
        let candidate = dir.join(DEFAULT_CONFIG_FILENAME);
        if candidate.exists() {
            return candidate;
        }
    }

    // Si aún no existe, preferimos el CWD cuando no es raíz del sistema (como `/` al abrir un `.app`).
    if let Some(dir) = cwd {
        if dir != Path::new("/") {
            return dir.join(DEFAULT_CONFIG_FILENAME);
        }
    }

    if let Some(dir) = exe_dir {
        return dir.join(DEFAULT_CONFIG_FILENAME);
    }

    PathBuf::from(DEFAULT_CONFIG_FILENAME)
}

/// Carga `vtuber_maclo.toml` desde `path` o lo genera con valores por defecto si no existe.
///
/// Nunca entra en pánico ante errores de E/S, TOML corrupto, valores numéricos inválidos o
/// imágenes faltantes: degrada de forma segura hacia valores integrados y registra `ConfigWarning`.
pub fn load_or_create_config(path: &Path) -> LoadedConfig {
    let mut warnings = Vec::new();

    if !path.exists() {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    warnings.push(ConfigWarning::new(format!(
                        "No se pudo crear la carpeta de configuración '{}': {err}",
                        parent.display()
                    )));
                }
            }
        }

        if let Err(err) = std::fs::write(path, DEFAULT_CONFIG_TEMPLATE) {
            warnings.push(ConfigWarning::new(format!(
                "No se pudo escribir la configuración inicial en '{}': {err}",
                path.display()
            )));
        }

        return LoadedConfig {
            app_config: AppConfig::default(),
            avatar_profile: AvatarProfile::builtin_default(),
            bindings: default_bindings(),
            warnings,
            config_path: path.to_path_buf(),
        };
    }

    let raw_toml = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            warnings.push(ConfigWarning::new(format!(
                "No se pudo leer '{}': {err}. Se usarán valores predeterminados.",
                path.display()
            )));
            return LoadedConfig {
                app_config: AppConfig::default(),
                avatar_profile: AvatarProfile::builtin_default(),
                bindings: default_bindings(),
                warnings,
                config_path: path.to_path_buf(),
            };
        }
    };

    let dto = match toml::from_str::<ExternalConfigDto>(&raw_toml) {
        Ok(parsed) => parsed,
        Err(err) => {
            warnings.push(ConfigWarning::new(format!(
                "Error de sintaxis en '{}': {err}. Se usarán valores predeterminados.",
                path.display()
            )));
            return LoadedConfig {
                app_config: AppConfig::default(),
                avatar_profile: AvatarProfile::builtin_default(),
                bindings: default_bindings(),
                warnings,
                config_path: path.to_path_buf(),
            };
        }
    };

    let app_config = AppConfig {
        window: sanitize_window_config(dto.window, &mut warnings),
        audio: sanitize_audio_config(dto.audio, &mut warnings),
        animation: sanitize_animation_config(dto.animation, &mut warnings),
    };

    let base_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let avatar_profile = build_avatar_profile(dto.states, base_dir, &mut warnings);
    let bindings = build_bindings(dto.bindings, &avatar_profile, &mut warnings);

    LoadedConfig {
        app_config,
        avatar_profile,
        bindings,
        warnings,
        config_path: path.to_path_buf(),
    }
}

/// Sanitiza los campos de `[window]` evitando tamaños no finitos/negativos o título vacío.
fn sanitize_window_config(dto: WindowConfigDto, warnings: &mut Vec<ConfigWarning>) -> WindowConfig {
    let def = WindowConfig::default();

    let title = if dto.title.trim().is_empty() {
        warnings.push(ConfigWarning::new(
            "window.title está vacío; se usará el título predeterminado.",
        ));
        def.title
    } else {
        dto.title
    };

    let width = sanitize_dimension(
        dto.inner_size[0],
        def.inner_size[0],
        "window.inner_size[0]",
        warnings,
    );
    let height = sanitize_dimension(
        dto.inner_size[1],
        def.inner_size[1],
        "window.inner_size[1]",
        warnings,
    );

    WindowConfig {
        title,
        inner_size: [width, height],
        transparent: dto.transparent,
        decorations: dto.decorations,
    }
}

/// Valida una dimensión de ventana positiva y finita dentro de un rango razonable.
fn sanitize_dimension(
    value: f32,
    fallback: f32,
    field: &str,
    warnings: &mut Vec<ConfigWarning>,
) -> f32 {
    if !value.is_finite() || value < 100.0 || value > 8192.0 {
        warnings.push(ConfigWarning::new(format!(
            "{field} inválido ({value}); se usará {fallback}."
        )));
        fallback
    } else {
        value
    }
}

/// Sanitiza un parámetro flotante asegurando finitud y acotándolo al rango `min..=max`.
fn sanitize_bounded_f32(
    value: f32,
    min: f32,
    max: f32,
    fallback: f32,
    field: &str,
    warnings: &mut Vec<ConfigWarning>,
) -> f32 {
    if !value.is_finite() {
        warnings.push(ConfigWarning::new(format!(
            "{field} inválido ({value}); se usará {fallback}."
        )));
        return fallback;
    }
    if value < min || value > max {
        let clamped = value.clamp(min, max);
        warnings.push(ConfigWarning::new(format!(
            "{field} ({value}) fuera de {min}..={max}; ajustado a {clamped}."
        )));
        return clamped;
    }
    value
}

/// Sanitiza `[audio]` asegurando límites finitos, rango no invertido y umbral acotado en `[0.0, 1.0]`.
fn sanitize_audio_config(dto: AudioConfigDto, warnings: &mut Vec<ConfigWarning>) -> AudioConfig {
    let def = AudioConfig::default();
    let def_min = *def.threshold_range.start();
    let def_max = *def.threshold_range.end();

    // El RMS normalizado vive en [0.0, 1.0], por lo que los umbrales no deben superar 1.0.
    let mut min_t = if !dto.threshold_min.is_finite() || dto.threshold_min < 0.0 {
        warnings.push(ConfigWarning::new(format!(
            "audio.threshold_min inválido ({}); se usará {def_min}.",
            dto.threshold_min
        )));
        def_min
    } else if dto.threshold_min > 1.0 {
        warnings.push(ConfigWarning::new(format!(
            "audio.threshold_min ({}) excede 1.0; acotado a 1.0.",
            dto.threshold_min
        )));
        1.0
    } else {
        dto.threshold_min
    };

    let mut max_t = if !dto.threshold_max.is_finite() || dto.threshold_max <= 0.0 {
        warnings.push(ConfigWarning::new(format!(
            "audio.threshold_max inválido ({}); se usará {def_max}.",
            dto.threshold_max
        )));
        def_max
    } else if dto.threshold_max > 1.0 {
        warnings.push(ConfigWarning::new(format!(
            "audio.threshold_max ({}) excede 1.0; acotado a 1.0.",
            dto.threshold_max
        )));
        1.0
    } else {
        dto.threshold_max
    };

    if min_t >= max_t {
        warnings.push(ConfigWarning::new(format!(
            "Rango de umbral invertido ({min_t}..={max_t}); se usará {def_min}..={def_max}."
        )));
        min_t = def_min;
        max_t = def_max;
    }

    let default_threshold = if !dto.default_threshold.is_finite() || dto.default_threshold < 0.0 {
        warnings.push(ConfigWarning::new(format!(
            "audio.default_threshold inválido ({}); se acotará al rango válido.",
            dto.default_threshold
        )));
        def.default_threshold.clamp(min_t, max_t)
    } else if dto.default_threshold < min_t || dto.default_threshold > max_t {
        let clamped = dto.default_threshold.clamp(min_t, max_t);
        warnings.push(ConfigWarning::new(format!(
            "audio.default_threshold ({}) fuera de {min_t}..={max_t}; ajustado a {clamped}.",
            dto.default_threshold
        )));
        clamped
    } else {
        dto.default_threshold
    };

    let hold_ms = if dto.hold_ms > 60_000 {
        warnings.push(ConfigWarning::new(format!(
            "audio.hold_ms ({}) es excesivo; limitado a 60000 ms.",
            dto.hold_ms
        )));
        60_000
    } else {
        dto.hold_ms
    };

    AudioConfig {
        default_threshold,
        threshold_range: min_t..=max_t,
        hold_duration: Duration::from_millis(hold_ms),
    }
}

/// Sanitiza `[animation]` rechazando `NaN`, infinitos y valores no positivos donde no tienen sentido.
fn sanitize_animation_config(
    dto: AnimationConfigDto,
    warnings: &mut Vec<ConfigWarning>,
) -> AnimationConfig {
    let def = AnimationConfig::default();

    let max_dt = if dto.max_dt.is_finite() && dto.max_dt > 0.0 && dto.max_dt <= 1.0 {
        dto.max_dt
    } else {
        warnings.push(ConfigWarning::new(format!(
            "animation.max_dt inválido ({}); se usará {}.",
            dto.max_dt, def.max_dt
        )));
        def.max_dt
    };

    let bounce_impulse_y = sanitize_bounded_f32(
        dto.bounce_impulse_y,
        -2000.0,
        2000.0,
        def.bounce_impulse_y,
        "animation.bounce_impulse_y",
        warnings,
    );

    let bounce_damping_speed = sanitize_bounded_f32(
        dto.bounce_damping_speed,
        0.1,
        200.0,
        def.bounce_damping_speed,
        "animation.bounce_damping_speed",
        warnings,
    );

    let avatar_base_top_space = sanitize_bounded_f32(
        dto.avatar_base_top_space,
        0.0,
        4096.0,
        def.avatar_base_top_space,
        "animation.avatar_base_top_space",
        warnings,
    );

    let avatar_max_width = sanitize_bounded_f32(
        dto.avatar_max_width,
        16.0,
        8192.0,
        def.avatar_max_width,
        "animation.avatar_max_width",
        warnings,
    );

    AnimationConfig {
        max_dt,
        bounce_impulse_y,
        bounce_damping_speed,
        avatar_base_top_space,
        avatar_max_width,
    }
}

/// Construye el [`AvatarProfile`] aplicando las personalizaciones de `[states.*]` sobre los
/// respaldos embebidos y precargando/validando cualquier PNG externo.
fn build_avatar_profile(
    states_dto: BTreeMap<String, StateConfigDto>,
    base_dir: &Path,
    warnings: &mut Vec<ConfigWarning>,
) -> AvatarProfile {
    let mut profile = AvatarProfile::builtin_default();

    for (raw_key, state_dto) in states_dto {
        let trimmed_key = raw_key.trim();
        if trimmed_key.is_empty() {
            warnings.push(ConfigWarning::new(
                "Se encontró una sección `[states]` con identificador vacío y se omitió.",
            ));
            continue;
        }

        let state_id = canonical_state_id(trimmed_key);
        let embedded = profile.embedded_fallback(&state_id).cloned();

        let display_name = state_dto
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| embedded.as_ref().map(|e| e.display_name.clone()))
            .unwrap_or_else(|| trimmed_key.to_string());

        let idle_sprite = match state_dto
            .png
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(rel_or_abs) => {
                let full_path = resolve_asset_path(base_dir, rel_or_abs);
                match SpriteSource::load_validated_png(&full_path) {
                    Ok(source) => source,
                    Err(err) => {
                        warnings.push(ConfigWarning::new(format!(
                            "Estado '{state_id}': {err}. Se usará sprite embebido de respaldo."
                        )));
                        embedded
                            .as_ref()
                            .map(|e| e.idle.clone())
                            .unwrap_or_else(|| profile.default_embedded_sprite().clone())
                    }
                }
            }
            None => {
                if let Some(ref builtin) = embedded {
                    builtin.idle.clone()
                } else {
                    warnings.push(ConfigWarning::new(format!(
                        "El estado personalizado '{state_id}' no especificó `png`; se usará el respaldo 'idle'."
                    )));
                    profile.default_embedded_sprite().clone()
                }
            }
        };

        let talk_sprite = state_dto
            .talk_png
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|rel_or_abs| {
                let full_path = resolve_asset_path(base_dir, rel_or_abs);
                match SpriteSource::load_validated_png(&full_path) {
                    Ok(source) => Some(source),
                    Err(err) => {
                        warnings.push(ConfigWarning::new(format!(
                            "Estado '{state_id}' (`talk_png`): {err}. Se mantendrá el sprite base al hablar."
                        )));
                        None
                    }
                }
            });

        let _ = profile.register_state(
            state_id,
            ExpressionSprites {
                display_name,
                idle: idle_sprite,
                talk: talk_sprite,
            },
        );
    }

    profile
}

/// Convierte los DTOs `[[bindings]]` en [`InputBinding`] validados respecto al perfil cargado.
fn build_bindings(
    bindings_dto: Option<Vec<BindingConfigDto>>,
    profile: &AvatarProfile,
    warnings: &mut Vec<ConfigWarning>,
) -> Vec<InputBinding> {
    let Some(items) = bindings_dto else {
        return default_bindings();
    };

    let mut validated = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let ordinal = index + 1;

        let Some(source) = parse_binding_source(&item, ordinal, warnings) else {
            continue;
        };
        let label = source.format_short();

        let Some(action_dto) = item.action else {
            warnings.push(ConfigWarning::new(format!(
                "Binding #{ordinal} ('{label}') ignorado: falta el campo `action`."
            )));
            continue;
        };

        let action = match action_dto {
            BindingActionDto::ToggleControls => InputAction::ToggleControls,
            BindingActionDto::ClearExpression => InputAction::ClearExpression,
            BindingActionDto::ReloadConfig => InputAction::ReloadConfig,
            BindingActionDto::Expression => {
                let Some(raw_state) = item
                    .state
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    warnings.push(ConfigWarning::new(format!(
                        "Binding #{ordinal} ('{label}') ignorado: `action = \"expression\"` requiere el campo `state`."
                    )));
                    continue;
                };

                let state_id = canonical_state_id(raw_state);
                if !profile.contains_state(&state_id) {
                    warnings.push(ConfigWarning::new(format!(
                        "Binding #{ordinal} ('{label}') ignorado: el estado '{state_id}' no existe."
                    )));
                    continue;
                }
                InputAction::ActivateExpression(state_id)
            }
        };

        validated.push(InputBinding {
            source,
            action,
            mode: item.mode,
        });
    }

    validated
}

/// Valida y traduce la especificación de dispositivo/disparador de un [`BindingConfigDto`].
fn parse_binding_source(
    item: &BindingConfigDto,
    ordinal: usize,
    warnings: &mut Vec<ConfigWarning>,
) -> Option<TriggerSource> {
    match item.device {
        BindingDeviceDto::Keyboard => {
            if item.button.as_deref().is_some_and(|b| !b.trim().is_empty()) {
                warnings.push(ConfigWarning::new(format!(
                    "Binding #{ordinal}: el campo `button` solo aplica a `device = \"gamepad\"` y será ignorado."
                )));
            }

            if let Some(combo) = &item.combo {
                if item.key.as_deref().is_some_and(|k| !k.trim().is_empty()) {
                    warnings.push(ConfigWarning::new(format!(
                        "Binding #{ordinal}: se especificaron `combo` y `key`; se usará `combo`."
                    )));
                }
                let parsed = match combo {
                    ComboInputDto::List(parts) => KeyChord::parse_parts(parts),
                    ComboInputDto::Single(raw) => KeyChord::parse_str(raw),
                };
                match parsed {
                    Ok(chord) => Some(TriggerSource::KeyboardChord {
                        chord,
                        scope: item.scope,
                    }),
                    Err(err) => {
                        warnings.push(ConfigWarning::new(format!(
                            "Binding #{ordinal} ignorado: combo de teclado inválido ({err})."
                        )));
                        None
                    }
                }
            } else if let Some(raw_key) =
                item.key.as_deref().map(str::trim).filter(|s| !s.is_empty())
            {
                match KeyTrigger::parse(raw_key) {
                    Ok(trigger) => Some(TriggerSource::Keyboard {
                        trigger,
                        scope: item.scope,
                    }),
                    Err(err) => {
                        warnings.push(ConfigWarning::new(format!(
                            "Binding #{ordinal} ignorado: tecla '{raw_key}' inválida ({err})."
                        )));
                        None
                    }
                }
            } else {
                warnings.push(ConfigWarning::new(format!(
                    "Binding #{ordinal} ignorado: falta el campo `key` o `combo`."
                )));
                None
            }
        }
        BindingDeviceDto::Gamepad => {
            if item.key.as_deref().is_some_and(|k| !k.trim().is_empty()) {
                warnings.push(ConfigWarning::new(format!(
                    "Binding #{ordinal}: el campo `key` se ignora cuando `device = \"gamepad\"`."
                )));
            }

            let threshold_milli = match item.threshold {
                Some(raw) => {
                    let sanitized = sanitize_bounded_f32(
                        raw,
                        0.1,
                        0.95,
                        0.5,
                        &format!("bindings[{ordinal}].threshold"),
                        warnings,
                    );
                    (sanitized * 1000.0).round() as u16
                }
                None => GamepadTrigger::DEFAULT_THRESHOLD_MILLI,
            };

            let parsed = if let Some(combo) = &item.combo {
                if item.button.as_deref().is_some_and(|b| !b.trim().is_empty()) {
                    warnings.push(ConfigWarning::new(format!(
                        "Binding #{ordinal}: se especificaron `combo` y `button`; se usará `combo`."
                    )));
                }
                match combo {
                    ComboInputDto::List(parts) => {
                        GamepadTrigger::parse_parts(parts, item.gamepad_index, threshold_milli)
                    }
                    ComboInputDto::Single(raw) => {
                        GamepadTrigger::parse_str(raw, item.gamepad_index, threshold_milli)
                    }
                }
            } else if let Some(raw_button) = &item.button {
                GamepadTrigger::parse_str(raw_button, item.gamepad_index, threshold_milli)
            } else {
                warnings.push(ConfigWarning::new(format!(
                    "Binding #{ordinal} ignorado: `device = \"gamepad\"` requiere `button` o `combo`."
                )));
                return None;
            };

            match parsed {
                Ok(trigger) => Some(TriggerSource::Gamepad(trigger)),
                Err(err) => {
                    warnings.push(ConfigWarning::new(format!(
                        "Binding #{ordinal} ignorado: disparador de mando inválido ({err})."
                    )));
                    None
                }
            }
        }
    }
}

/// Mapea nombres estándar a las constantes estáticas de [`AvatarStateId`] o crea un id custom
/// normalizado en minúsculas ASCII para que `[states.*]` y `[[bindings]]` coincidan sin importar
/// mayúsculas/minúsculas.
fn canonical_state_id(raw: &str) -> AvatarStateId {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "idle" => AvatarStateId::IDLE,
        "talk" => AvatarStateId::TALK,
        "happy" => AvatarStateId::HAPPY,
        "surprised" => AvatarStateId::SURPRISED,
        _ => AvatarStateId::custom(normalized),
    }
}

/// Resuelve rutas relativas respecto al directorio del archivo `vtuber_maclo.toml`.
fn resolve_asset_path(base_dir: &Path, raw_path: &str) -> PathBuf {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::ActivationMode;
    use crate::input::KeyCode;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!(
                "vtuber_maclo_test_{label}_{}_{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("crear temp_dir de prueba");
            Self { path: dir }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn resuelve_ruta_por_args_o_directorios() {
        let temp = TempDirGuard::new("resolve_args");
        let custom_path = temp.path.join("custom.toml");

        let from_flag = resolve_config_path_from_args(
            [
                OsString::from("vtuber_maclo"),
                OsString::from("--config"),
                custom_path.as_os_str().to_os_string(),
            ],
            Some(&temp.path),
            None,
        );
        assert_eq!(from_flag, custom_path);

        let from_eq = resolve_config_path_from_args(
            [
                OsString::from("vtuber_maclo"),
                OsString::from(format!("--config={}", custom_path.display())),
            ],
            None,
            None,
        );
        assert_eq!(from_eq, custom_path);

        let fallback_cwd =
            resolve_config_path_from_args([OsString::from("vtuber_maclo")], Some(&temp.path), None);
        assert_eq!(fallback_cwd, temp.path.join(DEFAULT_CONFIG_FILENAME));
    }

    #[test]
    fn genera_config_por_defecto_si_no_existe_y_luego_la_carga_sin_advertencias() {
        let temp = TempDirGuard::new("autocreate");
        let cfg_path = temp.path.join("vtuber_maclo.toml");
        assert!(!cfg_path.exists());

        let created = load_or_create_config(&cfg_path);
        assert!(cfg_path.exists());
        assert!(created.warnings.is_empty());
        assert_eq!(created.bindings.len(), 4);

        // Volver a cargarlo desde disco debe parsear el TOML generado sin advertencias.
        let reloaded = load_or_create_config(&cfg_path);
        assert!(reloaded.warnings.is_empty());
        assert_eq!(reloaded.app_config.window.title, "Vtuber Maclo");
        assert_eq!(reloaded.bindings, default_bindings());
    }

    #[test]
    fn carga_config_valida_con_png_externo_y_estado_personalizado() {
        let temp = TempDirGuard::new("valid_custom");
        let png_path = temp.path.join("wink.png");
        let valid_png_bytes = include_bytes!("../assets/happy.png");
        std::fs::write(&png_path, valid_png_bytes).expect("escribir png de prueba");

        let toml_content = r#"
[window]
title = "Mi Avatar"
inner_size = [600.0, 700.0]
transparent = false
decorations = true

[audio]
default_threshold = 0.05
threshold_min = 0.01
threshold_max = 0.5
hold_ms = 200

[animation]
max_dt = 0.04
bounce_impulse_y = -24.0
bounce_damping_speed = 10.0
avatar_base_top_space = 20.0
avatar_max_width = 400.0

[states.wink]
display_name = "Guiño"
png = "wink.png"

[[bindings]]
key = "Ctrl+3"
scope = "window"
mode = "momentary"
action = "expression"
state = "wink"
"#;

        let cfg_path = temp.path.join("vtuber_maclo.toml");
        std::fs::write(&cfg_path, toml_content).expect("escribir toml válido");

        let loaded = load_or_create_config(&cfg_path);
        assert!(
            loaded.warnings.is_empty(),
            "no se esperaban advertencias: {:?}",
            loaded.warnings
        );
        assert_eq!(loaded.app_config.window.title, "Mi Avatar");
        assert_eq!(loaded.app_config.window.inner_size, [600.0, 700.0]);
        assert!(!loaded.app_config.window.transparent);
        assert_eq!(loaded.app_config.audio.default_threshold, 0.05);
        assert_eq!(
            loaded.app_config.audio.hold_duration,
            Duration::from_millis(200)
        );
        assert_eq!(loaded.app_config.animation.bounce_impulse_y, -24.0);

        let wink_id = AvatarStateId::custom("wink");
        assert!(loaded.avatar_profile.contains_state(&wink_id));
        assert_eq!(loaded.bindings.len(), 1);
        assert_eq!(loaded.bindings[0].mode, ActivationMode::Momentary);
        assert_eq!(
            loaded.bindings[0].action,
            InputAction::ActivateExpression(wink_id)
        );
        assert!(matches!(
            loaded.bindings[0].source,
            TriggerSource::Keyboard {
                trigger,
                scope: HotkeyScope::Window
            } if trigger.key == KeyCode::Num3 && trigger.modifiers.ctrl
        ));
    }

    #[test]
    fn toml_corrupto_degrada_a_defaults_con_advertencia() {
        let temp = TempDirGuard::new("corrupt_toml");
        let cfg_path = temp.path.join("vtuber_maclo.toml");
        std::fs::write(&cfg_path, "[window\ntitle = sin_comillas").expect("escribir toml roto");

        let loaded = load_or_create_config(&cfg_path);
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(loaded.app_config.window.title, "Vtuber Maclo");
        assert_eq!(loaded.bindings, default_bindings());
        assert!(loaded
            .avatar_profile
            .contains_state(&AvatarStateId::SURPRISED));
    }

    #[test]
    fn valores_invalidos_y_png_faltante_hacen_fallback_y_reportan_warnings() {
        let temp = TempDirGuard::new("invalid_values");
        let cfg_path = temp.path.join("vtuber_maclo.toml");
        let toml_content = r#"
[window]
title = "   "
inner_size = [-50.0, nan]

[audio]
default_threshold = -1.0
threshold_min = 0.5
threshold_max = 0.1

[animation]
max_dt = 0.0
bounce_impulse_y = inf
bounce_damping_speed = -5.0
avatar_base_top_space = -10.0
avatar_max_width = 2.0

[states.happy]
display_name = "Feliz Custom"
png = "no_existe.png"

[[bindings]]
key = "TeclaMala"
action = "toggle_controls"

[[bindings]]
key = "F5"
action = "expression"
state = "fantasma"
"#;
        std::fs::write(&cfg_path, toml_content).expect("escribir toml con valores inválidos");

        let loaded = load_or_create_config(&cfg_path);
        // Debe haber advertencias pero ningún valor NaN/negativo/inválido ni bindings rotos.
        assert!(!loaded.warnings.is_empty());
        assert_eq!(loaded.app_config.window.title, "Vtuber Maclo");
        assert_eq!(loaded.app_config.window.inner_size, [450.0, 500.0]);
        assert_eq!(loaded.app_config.audio.threshold_range, 0.001..=0.2);
        assert!(loaded.app_config.audio.default_threshold.is_finite());
        assert!(loaded.app_config.animation.max_dt > 0.0);
        assert!(loaded.app_config.animation.bounce_impulse_y.is_finite());
        assert!(loaded.app_config.animation.bounce_damping_speed > 0.0);
        assert!(loaded.app_config.animation.avatar_base_top_space >= 0.0);
        assert!(loaded.app_config.animation.avatar_max_width >= 16.0);

        // `happy` debe seguir existiendo con fallback embebido funcional.
        assert!(loaded.avatar_profile.contains_state(&AvatarStateId::HAPPY));
        // Los 2 bindings inválidos fueron ignorados.
        assert!(loaded.bindings.is_empty());
    }

    #[test]
    fn valores_extremos_en_audio_y_animacion_se_acotan_con_warnings() {
        let temp = TempDirGuard::new("extreme_bounds");
        let cfg_path = temp.path.join("vtuber_maclo.toml");
        let toml_content = r#"
[audio]
default_threshold = 1e20
threshold_min = 0.02
threshold_max = 1e20

[animation]
bounce_impulse_y = 1e20
bounce_damping_speed = 1e20
avatar_base_top_space = 1e20
avatar_max_width = 1e20
"#;
        std::fs::write(&cfg_path, toml_content).expect("escribir toml con extremos positivos");

        let high = load_or_create_config(&cfg_path);
        assert!(
            high.warnings.len() >= 5,
            "se esperaban advertencias al recortar valores 1e20: {:?}",
            high.warnings
        );
        assert!(*high.app_config.audio.threshold_range.start() >= 0.0);
        assert!(*high.app_config.audio.threshold_range.end() <= 1.0);
        assert!((0.02..=1.0).contains(&high.app_config.audio.default_threshold));
        assert_eq!(high.app_config.animation.bounce_impulse_y, 2000.0);
        assert_eq!(high.app_config.animation.bounce_damping_speed, 200.0);
        assert_eq!(high.app_config.animation.avatar_base_top_space, 4096.0);
        assert_eq!(high.app_config.animation.avatar_max_width, 8192.0);

        let low_toml = r#"
[audio]
default_threshold = -1e20
threshold_min = -1e20
threshold_max = -1e20

[animation]
bounce_impulse_y = -1e20
bounce_damping_speed = -1e20
avatar_base_top_space = -1e20
avatar_max_width = -1e20
"#;
        std::fs::write(&cfg_path, low_toml).expect("escribir toml con extremos negativos");

        let low = load_or_create_config(&cfg_path);
        assert!(
            low.warnings.len() >= 6,
            "se esperaban advertencias al recortar/revertir valores -1e20: {:?}",
            low.warnings
        );
        assert!(*low.app_config.audio.threshold_range.start() >= 0.0);
        assert!(*low.app_config.audio.threshold_range.end() <= 1.0);
        assert!(
            low.app_config.audio.threshold_range.start()
                < low.app_config.audio.threshold_range.end()
        );
        assert_eq!(low.app_config.animation.bounce_impulse_y, -2000.0);
        assert_eq!(low.app_config.animation.bounce_damping_speed, 0.1);
        assert_eq!(low.app_config.animation.avatar_base_top_space, 0.0);
        assert_eq!(low.app_config.animation.avatar_max_width, 16.0);
    }

    #[test]
    fn normaliza_mayusculas_en_estados_personalizados_y_bindings() {
        let temp = TempDirGuard::new("case_insensitive_custom_state");
        let png_path = temp.path.join("wink.png");
        let valid_png_bytes = include_bytes!("../assets/happy.png");
        std::fs::write(&png_path, valid_png_bytes).expect("escribir png de prueba");

        let toml_content = r#"
[states.Wink]
png = "wink.png"

[[bindings]]
key = "Ctrl+3"
scope = "window"
mode = "toggle"
action = "expression"
state = "wink"
"#;
        let cfg_path = temp.path.join("vtuber_maclo.toml");
        std::fs::write(&cfg_path, toml_content).expect("escribir toml válido");

        let loaded = load_or_create_config(&cfg_path);
        assert!(
            loaded.warnings.is_empty(),
            "no se esperaban advertencias: {:?}",
            loaded.warnings
        );

        let wink_id = AvatarStateId::custom("wink");
        assert!(loaded.avatar_profile.contains_state(&wink_id));

        // Conserva `Wink` como display_name legible para la UI aunque el ID se normalizó.
        let (_, sprites) = loaded
            .avatar_profile
            .ordered_states()
            .find(|(id, _)| *id == &wink_id)
            .expect("el estado wink debe existir");
        assert_eq!(sprites.display_name, "Wink");

        assert_eq!(loaded.bindings.len(), 1);
        assert_eq!(
            loaded.bindings[0].action,
            InputAction::ActivateExpression(wink_id)
        );
    }

    #[test]
    fn carga_combos_multitecla_y_gamepad_con_validacion_de_threshold_y_vacios() {
        let temp = TempDirGuard::new("combos_and_gamepad");
        let cfg_path = temp.path.join("vtuber_maclo.toml");
        let toml_content = r#"
[[bindings]]
combo = ["B", "A"]
scope = "window"
mode = "momentary"
action = "expression"
state = "happy"

[[bindings]]
combo = "Ctrl+J+K"
scope = "window"
mode = "toggle"
action = "toggle_controls"

[[bindings]]
device = "gamepad"
button = "A"
mode = "toggle"
action = "expression"
state = "happy"

[[bindings]]
device = "gamepad"
combo = ["LB", "Y"]
gamepad_index = 1
threshold = 0.75
mode = "momentary"
action = "expression"
state = "surprised"

[[bindings]]
device = "gamepad"
combo = "LT+X"
threshold = nan
mode = "momentary"
action = "clear_expression"

[[bindings]]
device = "gamepad"
button = "   "
action = "toggle_controls"

[[bindings]]
device = "gamepad"
combo = []
action = "toggle_controls"
"#;
        std::fs::write(&cfg_path, toml_content).expect("escribir toml de combos y gamepad");

        let loaded = load_or_create_config(&cfg_path);
        // Debe tener 5 bindings válidos y 3 warnings (threshold = nan, button vacío, combo vacío).
        assert_eq!(loaded.bindings.len(), 5);
        assert_eq!(
            loaded.warnings.len(),
            3,
            "advertencias obtenidas: {:?}",
            loaded.warnings
        );

        assert_eq!(loaded.bindings[0].source.format_short(), "A+B");
        assert_eq!(loaded.bindings[1].source.format_short(), "Ctrl+J+K");
        assert_eq!(loaded.bindings[2].source.format_short(), "A");
        assert_eq!(loaded.bindings[3].source.format_short(), "P2:LB+Y");

        match &loaded.bindings[4].source {
            TriggerSource::Gamepad(gp) => {
                // `threshold = nan` debe haber hecho fallback seguro a 0.5 (500 milli).
                assert_eq!(gp.threshold_milli, 500);
            }
            other => panic!("se esperaba TriggerSource::Gamepad, llegó {other:?}"),
        }
    }
}
