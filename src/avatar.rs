//! Gestión modular de estados, expresiones, modos de activación y fuentes de sprites del avatar.

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// Identificador tipado para un estado o expresión del avatar (por ejemplo `idle`, `talk`, `happy`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AvatarStateId(Cow<'static, str>);

impl AvatarStateId {
    /// Estado de reposo predeterminado cuando el usuario no está hablando.
    pub const IDLE: Self = Self(Cow::Borrowed("idle"));
    /// Estado activo predeterminado cuando se detecta voz en el micrófono.
    pub const TALK: Self = Self(Cow::Borrowed("talk"));
    /// Expresión feliz integrada por defecto en el binario.
    pub const HAPPY: Self = Self(Cow::Borrowed("happy"));
    /// Expresión sorprendida integrada por defecto en el binario.
    pub const SURPRISED: Self = Self(Cow::Borrowed("surprised"));

    /// Crea un identificador personalizado de expresión o estado alterno.
    pub fn custom(id: impl Into<String>) -> Self {
        Self(Cow::Owned(id.into()))
    }

    /// Devuelve la representación textual del identificador.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AvatarStateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Origen de imagen asociado a un estado del avatar sin realizar E/S en el hilo de render.
#[derive(Clone, Debug)]
pub enum SpriteSource {
    /// Imagen incrustada en el binario mediante `egui::include_image!`.
    Embedded(egui::ImageSource<'static>),
    /// Bytes PNG ya leídos y validados en memoria durante la carga de configuración.
    Bytes {
        /// Identificador URI único usado como clave de caché en `egui`.
        uri: String,
        /// Contenido binario validado de la imagen.
        bytes: Arc<[u8]>,
    },
}

impl SpriteSource {
    /// Lee un archivo PNG desde disco y valida que pueda decodificarse antes de usarlo en UI.
    ///
    /// Al precargar y validar los bytes durante la carga/recarga de configuración evitamos
    /// bloquear el hilo de renderizado con operaciones síncronas de disco.
    pub fn load_validated_png(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|err| format!("no se pudo leer '{}': {err}", path.display()))?;

        // Validamos la cabecera y estructura de la imagen con `image` antes de entregarla a egui.
        image::load_from_memory(&bytes)
            .map_err(|err| format!("imagen inválida en '{}': {err}", path.display()))?;

        Ok(Self::Bytes {
            uri: format!("bytes://{}", path.display()),
            bytes: Arc::from(bytes),
        })
    }
}

/// Sprites y metadatos visuales asociados a una expresión o estado del avatar.
#[derive(Clone, Debug)]
pub struct ExpressionSprites {
    /// Nombre legible para mostrar al usuario en la interfaz de controles.
    pub display_name: String,
    /// Sprite principal usado en reposo o cuando no existe variante específica de habla.
    pub idle: SpriteSource,
    /// Sprite opcional usado mientras esta expresión está activa y el usuario está hablando.
    pub talk: Option<SpriteSource>,
}

/// Modo de activación de un disparador de entrada sobre una expresión.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    /// Alterna la expresión con cada pulsación (encendido/apagado).
    #[default]
    Toggle,
    /// Mantiene la expresión activa únicamente mientras se sostiene la tecla o disparador.
    Momentary,
}

/// Errores de validación o búsqueda al manipular los estados de un [`AvatarProfile`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvatarError {
    /// Se intentó registrar un estado con identificador vacío o compuesto solo por espacios.
    EmptyStateId,
    /// Se solicitó activar una expresión que no está registrada en el perfil.
    UnknownState(AvatarStateId),
}

impl fmt::Display for AvatarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyStateId => write!(f, "El identificador de estado del avatar está vacío"),
            Self::UnknownState(id) => {
                write!(f, "El estado de avatar '{id}' no existe en el perfil")
            }
        }
    }
}

impl std::error::Error for AvatarError {}

/// Perfil extensible que mapea estados del avatar hacia sus sprites y gestiona prioridades.
pub struct AvatarProfile {
    states: HashMap<AvatarStateId, ExpressionSprites>,
    /// Orden determinístico de registro para presentar los estados en la UI.
    state_order: Vec<AvatarStateId>,
    /// Copias embebidas de respaldo para los estados integrados (`idle`, `talk`, `happy`, `surprised`).
    embedded_fallbacks: HashMap<AvatarStateId, ExpressionSprites>,
    default_state: AvatarStateId,
    talk_state: AvatarStateId,
    /// Expresión fijada mediante activación `toggle`.
    manual_toggle: Option<AvatarStateId>,
    /// Pila LIFO de expresiones activadas en modo `momentary` (la última pulsada tiene prioridad).
    momentary_stack: Vec<AvatarStateId>,
    /// Respaldo inmutable garantizado para resolver sprites sin usar `.unwrap()`.
    fallback_sprite: SpriteSource,
}

impl AvatarProfile {
    /// Construye el perfil predeterminado con los 4 PNG incrustados (`idle`, `talk`, `happy`, `surprised`).
    pub fn builtin_default() -> Self {
        let idle_source = SpriteSource::Embedded(egui::include_image!("../assets/idle.png"));
        let talk_source = SpriteSource::Embedded(egui::include_image!("../assets/talk.png"));
        let happy_source = SpriteSource::Embedded(egui::include_image!("../assets/happy.png"));
        let surprised_source =
            SpriteSource::Embedded(egui::include_image!("../assets/surprised.png"));

        let builtin_entries = [
            (
                AvatarStateId::IDLE,
                ExpressionSprites {
                    display_name: "Reposo".to_string(),
                    idle: idle_source.clone(),
                    talk: None,
                },
            ),
            (
                AvatarStateId::TALK,
                ExpressionSprites {
                    display_name: "Hablando".to_string(),
                    idle: talk_source,
                    talk: None,
                },
            ),
            (
                AvatarStateId::HAPPY,
                ExpressionSprites {
                    display_name: "Feliz".to_string(),
                    idle: happy_source,
                    talk: None,
                },
            ),
            (
                AvatarStateId::SURPRISED,
                ExpressionSprites {
                    display_name: "Sorprendido".to_string(),
                    idle: surprised_source,
                    talk: None,
                },
            ),
        ];

        let mut states = HashMap::new();
        let mut state_order = Vec::with_capacity(builtin_entries.len());
        let mut embedded_fallbacks = HashMap::new();

        for (id, sprites) in builtin_entries {
            state_order.push(id.clone());
            embedded_fallbacks.insert(id.clone(), sprites.clone());
            states.insert(id, sprites);
        }

        Self {
            states,
            state_order,
            embedded_fallbacks,
            default_state: AvatarStateId::IDLE,
            talk_state: AvatarStateId::TALK,
            manual_toggle: None,
            momentary_stack: Vec::new(),
            fallback_sprite: idle_source,
        }
    }

    /// Registra o reemplaza los sprites asociados a un estado conservando orden determinístico.
    pub fn register_state(
        &mut self,
        id: AvatarStateId,
        sprites: ExpressionSprites,
    ) -> Result<(), AvatarError> {
        if id.as_str().trim().is_empty() {
            return Err(AvatarError::EmptyStateId);
        }
        if !self.states.contains_key(&id) {
            self.state_order.push(id.clone());
        }
        self.states.insert(id, sprites);
        Ok(())
    }

    /// Indica si un estado existe actualmente en el perfil.
    pub fn contains_state(&self, id: &AvatarStateId) -> bool {
        self.states.contains_key(id)
    }

    /// Devuelve el respaldo embebido equivalente para un estado integrado, si existe.
    pub fn embedded_fallback(&self, id: &AvatarStateId) -> Option<&ExpressionSprites> {
        self.embedded_fallbacks.get(id)
    }

    /// Devuelve el sprite embebido `idle` garantizado para cualquier respaldo de último recurso.
    pub fn default_embedded_sprite(&self) -> &SpriteSource {
        &self.fallback_sprite
    }

    /// Itera sobre todos los estados registrados siguiendo el orden determinístico de `state_order`.
    pub fn ordered_states(&self) -> impl Iterator<Item = (&AvatarStateId, &ExpressionSprites)> {
        self.state_order
            .iter()
            .filter_map(|id| self.states.get_key_value(id))
    }

    /// Activa o desactiva un estado dentro de la pila momentánea (`momentary_stack`).
    ///
    /// Al activar (`active = true`), el estado se coloca en la cima de la pila para tomar
    /// prioridad inmediata sobre cualquier otro estado previo. Al desactivar (`active = false`),
    /// se retira de la pila sin afectar al resto de teclas sostenidas ni al `manual_toggle`.
    pub fn set_momentary(&mut self, id: AvatarStateId, active: bool) -> Result<(), AvatarError> {
        if active {
            if !self.states.contains_key(&id) {
                return Err(AvatarError::UnknownState(id));
            }
            // Evitamos duplicados si llega otro evento Pressed y promovemos a la cima de la pila.
            self.momentary_stack.retain(|existing| existing != &id);
            self.momentary_stack.push(id);
        } else {
            self.momentary_stack.retain(|existing| existing != &id);
        }
        Ok(())
    }

    /// Alterna un override manual (`toggle`); devuelve `true` si quedó activo o `false` si se desactivó.
    pub fn toggle_override(&mut self, id: AvatarStateId) -> Result<bool, AvatarError> {
        if self.manual_toggle.as_ref() == Some(&id) {
            self.manual_toggle = None;
            return Ok(false);
        }

        if !self.states.contains_key(&id) {
            return Err(AvatarError::UnknownState(id));
        }

        self.manual_toggle = Some(id);
        Ok(true)
    }

    /// Limpia cualquier override activo (`manual_toggle` y `momentary_stack`) para volver a neutro.
    pub fn clear_overrides(&mut self) {
        self.manual_toggle = None;
        self.momentary_stack.clear();
    }

    /// Devuelve el estado fijado manualmente (`manual_toggle`) si existe.
    pub fn manual_toggle(&self) -> Option<&AvatarStateId> {
        self.manual_toggle.as_ref()
    }

    /// Devuelve el estado momentáneo actualmente activo en la cima de la pila, si existe.
    pub fn momentary_active(&self) -> Option<&AvatarStateId> {
        self.momentary_stack.last()
    }

    /// Devuelve el identificador del override con mayor prioridad actualmente activo, si existe.
    pub fn active_override(&self) -> Option<&AvatarStateId> {
        self.momentary_stack.last().or(self.manual_toggle.as_ref())
    }

    /// Resuelve el estado y sprite a renderizar según si el usuario está hablando y los overrides.
    ///
    /// Prioridad: `momentary_stack.last()` -> `manual_toggle` -> `talk_state` (si `is_talking`) ->
    /// `default_state` (`IDLE`). Si una expresión activa no define sprite `talk`, mantiene su
    /// sprite `idle` para no perder la expresión mientras el rebote sigue reaccionando a la voz.
    pub fn resolve_sprite(&self, is_talking: bool) -> (&AvatarStateId, &SpriteSource) {
        // 1. Revisamos la pila momentánea desde la cima hacia abajo por si hubiera algún huérfano.
        for momentary_id in self.momentary_stack.iter().rev() {
            if let Some((state_id, entry)) = self.states.get_key_value(momentary_id) {
                let sprite = select_entry_sprite(entry, is_talking);
                return (state_id, sprite);
            }
        }

        // 2. Si no hay momentáneo válido, revisamos el override manual por toggle.
        if let Some(toggle_id) = &self.manual_toggle {
            if let Some((state_id, entry)) = self.states.get_key_value(toggle_id) {
                let sprite = select_entry_sprite(entry, is_talking);
                return (state_id, sprite);
            }
        }

        // 3. Sin expresiones activas, respondemos al estado de voz si el usuario habla.
        if is_talking {
            if let Some((state_id, entry)) = self.states.get_key_value(&self.talk_state) {
                let sprite = select_entry_sprite(entry, is_talking);
                return (state_id, sprite);
            }
        }

        // 4. Estado base de reposo con respaldo garantizado sin `.unwrap()`.
        if let Some((state_id, entry)) = self.states.get_key_value(&self.default_state) {
            return (state_id, &entry.idle);
        }

        (&self.default_state, &self.fallback_sprite)
    }
}

/// Elige la variante `talk` de una expresión cuando `is_talking` es verdadero, o `idle` como respaldo.
fn select_entry_sprite(entry: &ExpressionSprites, is_talking: bool) -> &SpriteSource {
    if is_talking {
        entry.talk.as_ref().unwrap_or(&entry.idle)
    } else {
        &entry.idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_bytes_sprite(name: &str) -> SpriteSource {
        SpriteSource::Bytes {
            uri: format!("bytes://test/{name}"),
            bytes: Arc::from(name.as_bytes()),
        }
    }

    #[test]
    fn builtin_default_incluye_cuatro_estados_en_orden() {
        let profile = AvatarProfile::builtin_default();
        let ids: Vec<&str> = profile
            .ordered_states()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(ids, vec!["idle", "talk", "happy", "surprised"]);
        assert!(profile.embedded_fallback(&AvatarStateId::HAPPY).is_some());
        assert!(profile
            .embedded_fallback(&AvatarStateId::SURPRISED)
            .is_some());
    }

    #[test]
    fn toggle_override_y_estados_desconocidos() {
        let mut profile = AvatarProfile::builtin_default();

        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::IDLE);
        assert_eq!(profile.resolve_sprite(true).0, &AvatarStateId::TALK);

        let missing = AvatarStateId::custom("inexistente");
        assert_eq!(
            profile.toggle_override(missing.clone()),
            Err(AvatarError::UnknownState(missing))
        );

        assert_eq!(profile.toggle_override(AvatarStateId::HAPPY), Ok(true));
        assert_eq!(profile.active_override(), Some(&AvatarStateId::HAPPY));
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::HAPPY);
        assert_eq!(profile.resolve_sprite(true).0, &AvatarStateId::HAPPY);

        assert_eq!(profile.toggle_override(AvatarStateId::HAPPY), Ok(false));
        assert_eq!(profile.active_override(), None);
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::IDLE);
    }

    #[test]
    fn pila_momentanea_con_dos_teclas_y_liberacion_inversa() {
        let mut profile = AvatarProfile::builtin_default();

        // Dejamos un toggle activo de fondo para verificar que la pila momentánea lo supera y luego regresa a él.
        assert_eq!(profile.toggle_override(AvatarStateId::HAPPY), Ok(true));
        assert_eq!(profile.manual_toggle(), Some(&AvatarStateId::HAPPY));
        assert_eq!(profile.momentary_active(), None);
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::HAPPY);

        // Sostener tecla 1 -> surprised.
        assert_eq!(
            profile.set_momentary(AvatarStateId::SURPRISED, true),
            Ok(())
        );
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::SURPRISED);
        assert_eq!(profile.active_override(), Some(&AvatarStateId::SURPRISED));
        // `manual_toggle` permanece intacto mientras `momentary_active` refleja la tecla sostenida.
        assert_eq!(profile.manual_toggle(), Some(&AvatarStateId::HAPPY));
        assert_eq!(profile.momentary_active(), Some(&AvatarStateId::SURPRISED));

        // Registrar y sostener tecla 2 -> custom("guiño") mientras tecla 1 sigue sostenida.
        let wink = AvatarStateId::custom("wink");
        assert!(profile
            .register_state(
                wink.clone(),
                ExpressionSprites {
                    display_name: "Guiño".to_string(),
                    idle: dummy_bytes_sprite("wink_idle"),
                    talk: None,
                },
            )
            .is_ok());

        assert_eq!(profile.set_momentary(wink.clone(), true), Ok(()));
        assert_eq!(profile.resolve_sprite(false).0, &wink);

        // Soltar en orden inverso: primero tecla 2 (wink) -> vuelve a tecla 1 (surprised).
        assert_eq!(profile.set_momentary(wink, false), Ok(()));
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::SURPRISED);

        // Soltar tecla 1 (surprised) -> vuelve al toggle previo (happy).
        assert_eq!(
            profile.set_momentary(AvatarStateId::SURPRISED, false),
            Ok(())
        );
        assert_eq!(profile.momentary_active(), None);
        assert_eq!(profile.manual_toggle(), Some(&AvatarStateId::HAPPY));
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::HAPPY);

        // Limpiar todos los overrides vuelve a reactivo por voz.
        profile.clear_overrides();
        assert!(profile.manual_toggle().is_none());
        assert!(profile.momentary_active().is_none());
        assert!(profile.active_override().is_none());
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::IDLE);
    }

    #[test]
    fn expresion_con_y_sin_sprite_talk_hace_fallback_limpio() {
        let mut profile = AvatarProfile::builtin_default();
        let with_talk = AvatarStateId::custom("excited");
        let without_talk = AvatarStateId::custom("calm");

        profile
            .register_state(
                with_talk.clone(),
                ExpressionSprites {
                    display_name: "Emocionado".to_string(),
                    idle: dummy_bytes_sprite("excited_idle"),
                    talk: Some(dummy_bytes_sprite("excited_talk")),
                },
            )
            .expect("estado válido");

        profile
            .register_state(
                without_talk.clone(),
                ExpressionSprites {
                    display_name: "Calmado".to_string(),
                    idle: dummy_bytes_sprite("calm_idle"),
                    talk: None,
                },
            )
            .expect("estado válido");

        // Con variante `talk` definida, alterna entre idle y talk de la expresión.
        assert_eq!(profile.toggle_override(with_talk), Ok(true));
        match profile.resolve_sprite(false).1 {
            SpriteSource::Bytes { uri, .. } => assert_eq!(uri, "bytes://test/excited_idle"),
            _ => panic!("se esperaba SpriteSource::Bytes"),
        }
        match profile.resolve_sprite(true).1 {
            SpriteSource::Bytes { uri, .. } => assert_eq!(uri, "bytes://test/excited_talk"),
            _ => panic!("se esperaba SpriteSource::Bytes"),
        }

        // Sin variante `talk`, mantiene el sprite `idle` de la expresión aunque `is_talking` sea true.
        assert_eq!(profile.toggle_override(without_talk), Ok(true));
        match profile.resolve_sprite(true).1 {
            SpriteSource::Bytes { uri, .. } => assert_eq!(uri, "bytes://test/calm_idle"),
            _ => panic!("se esperaba SpriteSource::Bytes"),
        }

        // Si por error hubiera un identificador huérfano, degrada de forma segura a IDLE.
        profile.manual_toggle = Some(AvatarStateId::custom("huerfano"));
        assert_eq!(profile.resolve_sprite(false).0, &AvatarStateId::IDLE);
    }
}
