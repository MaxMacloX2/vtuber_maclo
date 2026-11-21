//! Primitivas genéricas para combinaciones multi-átomo (`Chord`), seguimiento de estado con
//! prioridad por aridad máxima (`ChordTracker`) y filtro de histéresis analógica (`AnalogHysteresis`).

use std::collections::{BTreeSet, HashSet};
use std::hash::Hash;

/// Fase física de un disparador de entrada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerPhase {
    /// El usuario acaba de presionar o activar el disparador.
    Pressed,
    /// El usuario acaba de soltar o desactivar el disparador.
    Released,
}

/// Combinación canónica ordenada y deduplicada de uno o más átomos de entrada (`KeyCode`, botones, etc.).
///
/// Garantiza que `[B, A]` y `[A, B]` produzcan exactamente el mismo `Chord`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chord<A> {
    atoms: Vec<A>,
}

impl<A: Ord + Clone> Chord<A> {
    /// Construye un [`Chord`] ordenando y deduplicando los átomos recibidos.
    ///
    /// Retorna `None` si el iterador no contiene ningún átomo.
    pub fn from_atoms(atoms: impl IntoIterator<Item = A>) -> Option<Self> {
        let mut sorted: Vec<A> = atoms.into_iter().collect();
        if sorted.is_empty() {
            return None;
        }
        // Ordenamos y deduplicamos para garantizar representación canónica independiente del orden.
        sorted.sort();
        sorted.dedup();
        Some(Self { atoms: sorted })
    }

    /// Construye un [`Chord`] compuesto por un único átomo.
    pub fn single(atom: A) -> Self {
        Self { atoms: vec![atom] }
    }

    /// Devuelve el slice canónico de átomos del combo.
    pub fn atoms(&self) -> &[A] {
        &self.atoms
    }

    /// Devuelve la cantidad de átomos distintos que conforman el combo.
    pub fn len(&self) -> usize {
        self.atoms.len()
    }

    /// Indica si el combo no contiene átomos (invariante: siempre es `false` tras `from_atoms`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    /// Verifica si el combo contiene al átomo indicado mediante búsqueda binaria.
    pub fn contains(&self, atom: &A) -> bool {
        self.atoms.binary_search(atom).is_ok()
    }

    /// Indica si todos los átomos de `self` pertenecen también a `other`.
    pub fn is_subset_of(&self, other: &Self) -> bool {
        if self.atoms.len() > other.atoms.len() {
            return false;
        }
        self.atoms
            .iter()
            .all(|atom| other.atoms.binary_search(atom).is_ok())
    }
}

/// Transición producida por [`ChordTracker`] al presionar o soltar átomos físicos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChordTransition<A> {
    /// Combo que cambió de estado.
    pub chord: Chord<A>,
    /// Nueva fase (`Pressed` o `Released`).
    pub phase: TriggerPhase,
}

/// Máquina de estados pura para detectar combinaciones multi-tecla o multi-botón sin duplicados.
///
/// Implementa prioridad por aridad máxima (supresión de subconjuntos): si un combo mayor como
/// `A+B` se completa mientras un subconjunto menor `A` estaba activo, emite primero `Released`
/// para `A` y luego `Pressed` para `A+B`. Al soltar cualquiera de los átomos del combo mayor,
/// emite `Released` para `A+B` sin re-disparar el átomo restante.
#[derive(Clone, Debug)]
pub struct ChordTracker<A: Ord + Eq + Hash + Copy> {
    /// Lista ordenada y deduplicada de combos vigilados.
    registered: Vec<Chord<A>>,
    /// Conjunto de átomos que están físicamente presionados en este momento.
    pressed_atoms: HashSet<A>,
    /// Conjunto de combos actualmente considerados activos.
    active_chords: HashSet<Chord<A>>,
}

impl<A: Ord + Eq + Hash + Copy> Default for ChordTracker<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Ord + Eq + Hash + Copy> ChordTracker<A> {
    /// Crea un rastreador vacío sin combos registrados.
    pub fn new() -> Self {
        Self {
            registered: Vec::new(),
            pressed_atoms: HashSet::new(),
            active_chords: HashSet::new(),
        }
    }

    /// Reemplaza los combos registrados liberando previamente cualquier combo activo.
    pub fn set_chords(
        &mut self,
        chords: impl IntoIterator<Item = Chord<A>>,
    ) -> Vec<ChordTransition<A>> {
        let released = self.release_all();
        // Usamos BTreeSet para deduplicar y mantener un orden determinístico.
        let unique: BTreeSet<Chord<A>> = chords.into_iter().collect();
        self.registered = unique.into_iter().collect();
        released
    }

    /// Registra la pulsación física de `atom` y devuelve las transiciones generadas.
    ///
    /// Si `atom` ya estaba presionado, la llamada se ignora para filtrar repeticiones automáticas.
    pub fn press(&mut self, atom: A) -> Vec<ChordTransition<A>> {
        if !self.pressed_atoms.insert(atom) {
            return Vec::new();
        }

        // Buscamos todos los combos registrados que contengan el átomo recién pulsado y tengan
        // todos sus átomos actualmente presionados.
        let candidates: Vec<Chord<A>> = self
            .registered
            .iter()
            .filter(|chord| {
                chord.contains(&atom)
                    && chord
                        .atoms()
                        .iter()
                        .all(|member| self.pressed_atoms.contains(member))
            })
            .filter(|chord| {
                // No activamos un combo si ya está activo o si un superconjunto suyo ya está activo.
                !self.active_chords.contains(*chord)
                    && !self
                        .active_chords
                        .iter()
                        .any(|active| chord.is_subset_of(active))
            })
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Vec::new();
        }

        // Aplicamos prioridad por aridad máxima: solo ganan los candidatos con mayor número de átomos.
        let max_arity = candidates.iter().map(Chord::len).max().unwrap_or(0);
        let winners: Vec<Chord<A>> = candidates
            .into_iter()
            .filter(|chord| chord.len() == max_arity)
            .collect();

        let mut transitions = Vec::new();

        // Si algún combo menor actualmente activo es subconjunto de un combo ganador, se libera primero.
        let mut suppressed: Vec<Chord<A>> = self
            .active_chords
            .iter()
            .filter(|active| {
                winners
                    .iter()
                    .any(|winner| active.len() < winner.len() && active.is_subset_of(winner))
            })
            .cloned()
            .collect();
        suppressed.sort();

        for chord in suppressed {
            self.active_chords.remove(&chord);
            transitions.push(ChordTransition {
                chord,
                phase: TriggerPhase::Released,
            });
        }

        for winner in winners {
            if self.active_chords.insert(winner.clone()) {
                transitions.push(ChordTransition {
                    chord: winner,
                    phase: TriggerPhase::Pressed,
                });
            }
        }

        transitions
    }

    /// Registra la liberación física de `atom` y emite `Released` para todo combo activo que lo incluya.
    pub fn release(&mut self, atom: A) -> Vec<ChordTransition<A>> {
        if !self.pressed_atoms.remove(&atom) {
            return Vec::new();
        }

        let mut to_release: Vec<Chord<A>> = self
            .active_chords
            .iter()
            .filter(|chord| chord.contains(&atom))
            .cloned()
            .collect();
        to_release.sort();

        let mut transitions = Vec::with_capacity(to_release.len());
        for chord in to_release {
            self.active_chords.remove(&chord);
            transitions.push(ChordTransition {
                chord,
                phase: TriggerPhase::Released,
            });
        }
        transitions
    }

    /// Libera todos los átomos y combos activos en orden determinístico (por ejemplo al perder foco o desconectar).
    pub fn release_all(&mut self) -> Vec<ChordTransition<A>> {
        self.pressed_atoms.clear();
        if self.active_chords.is_empty() {
            return Vec::new();
        }

        let mut active: Vec<Chord<A>> = self.active_chords.drain().collect();
        active.sort();

        active
            .into_iter()
            .map(|chord| ChordTransition {
                chord,
                phase: TriggerPhase::Released,
            })
            .collect()
    }
}

/// Filtro tipo Schmitt trigger para convertir valores analógicos (`0.0..=1.0`) en transiciones
/// digitales estables sin rebotes alrededor del umbral.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnalogHysteresis {
    /// Valor mínimo para pasar de inactivo a activo.
    pub press_threshold: f32,
    /// Valor máximo para volver de activo a inactivo (siempre menor que `press_threshold`).
    pub release_threshold: f32,
    /// Estado booleano actual del filtro.
    pub active: bool,
}

impl AnalogHysteresis {
    /// Crea un filtro con umbral de activación acotado a `[0.1, 0.95]` y un umbral de liberación al 70%.
    pub fn new(press_threshold: f32) -> Self {
        let press = if press_threshold.is_finite() {
            press_threshold.clamp(0.1, 0.95)
        } else {
            0.5
        };
        let release = (press * 0.7).max(0.02);
        Self {
            press_threshold: press,
            release_threshold: release,
            active: false,
        }
    }

    /// Crea un filtro especificando explícitamente ambos umbrales (útil para ejes o pruebas).
    pub fn with_thresholds(press_threshold: f32, release_threshold: f32) -> Self {
        let press = if press_threshold.is_finite() {
            press_threshold.clamp(0.05, 0.99)
        } else {
            0.5
        };
        let release = if release_threshold.is_finite() {
            release_threshold.clamp(0.01, (press - 0.01).max(0.01))
        } else {
            (press * 0.7).max(0.01)
        };
        Self {
            press_threshold: press,
            release_threshold: release,
            active: false,
        }
    }

    /// Actualiza el filtro con una nueva lectura analógica.
    ///
    /// Descarta valores no finitos (`NaN` o infinito). Devuelve `Some(true)` al activarse,
    /// `Some(false)` al desactivarse o `None` si el estado no cambió.
    pub fn update(&mut self, value: f32) -> Option<bool> {
        if !value.is_finite() {
            return None;
        }

        if !self.active && value >= self.press_threshold {
            self.active = true;
            Some(true)
        } else if self.active && value <= self.release_threshold {
            self.active = false;
            Some(false)
        } else {
            None
        }
    }

    /// Reinicia el filtro a inactivo; retorna `true` si estaba activo antes de reiniciar.
    pub fn reset(&mut self) -> bool {
        if self.active {
            self.active = false;
            true
        } else {
            false
        }
    }
}

/// Error al dividir y normalizar partes de un combo separado por `+`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboSplitError<E> {
    /// La lista de partes está vacía.
    Empty,
    /// Existe una parte o segmento vacío junto a `+` (acompañado del texto unido).
    EmptySegment(String),
    /// Error específico devuelto por el parser de tokens.
    Token(E),
}

/// Divide y normaliza partes de una combinación separada por `+`, validando segmentos vacíos
/// y aplicando `parse_one` sobre cada token individual recortado.
///
/// Devuelve `(texto_unido, tokens_parseados)` cuando todas las partes son válidas.
pub fn parse_combo_tokens<T, E, S, F>(
    parts: &[S],
    mut parse_one: F,
) -> Result<(String, Vec<T>), ComboSplitError<E>>
where
    S: AsRef<str>,
    F: FnMut(&str) -> Result<T, E>,
{
    if parts.is_empty() {
        return Err(ComboSplitError::Empty);
    }

    // Construimos la representación unida antes de validar para preservar contexto en errores.
    let raw_joined = parts
        .iter()
        .map(|part| part.as_ref().trim())
        .collect::<Vec<_>>()
        .join("+");

    let mut parsed = Vec::new();
    for part in parts {
        let trimmed_part = part.as_ref().trim();
        if trimmed_part.is_empty() {
            return Err(ComboSplitError::EmptySegment(raw_joined));
        }

        for segment in trimmed_part.split('+') {
            let token = segment.trim();
            if token.is_empty() {
                return Err(ComboSplitError::EmptySegment(raw_joined));
            }
            parsed.push(parse_one(token).map_err(ComboSplitError::Token)?);
        }
    }

    Ok((raw_joined, parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_normaliza_orden_y_deduplica_y_verifica_subconjuntos() {
        assert!(Chord::<u8>::from_atoms([]).is_none());

        let ab = Chord::from_atoms([2u8, 1u8, 2u8]).expect("chord no vacío");
        let ba = Chord::from_atoms([1u8, 2u8]).expect("chord no vacío");
        assert_eq!(ab, ba);
        assert_eq!(ab.atoms(), &[1, 2]);
        assert_eq!(ab.len(), 2);
        assert!(!ab.is_empty());

        let a = Chord::single(1u8);
        let c = Chord::single(3u8);
        assert!(a.is_subset_of(&ab));
        assert!(!ab.is_subset_of(&a));
        assert!(!c.is_subset_of(&ab));
    }

    #[test]
    fn tracker_suprime_subconjunto_al_completar_combo_mayor_y_no_redispara_al_soltar() {
        let mut tracker = ChordTracker::new();
        let chord_a = Chord::single('A');
        let chord_ab = Chord::from_atoms(['A', 'B']).expect("A+B");
        tracker.set_chords([chord_a.clone(), chord_ab.clone()]);

        // Pulsar A activa el combo simple A.
        assert_eq!(
            tracker.press('A'),
            vec![ChordTransition {
                chord: chord_a.clone(),
                phase: TriggerPhase::Pressed,
            }]
        );

        // Repetición de A sin soltar no emite nada.
        assert!(tracker.press('A').is_empty());

        // Pulsar B mientras A sigue pulsado suprime A (Released) y activa A+B (Pressed).
        assert_eq!(
            tracker.press('B'),
            vec![
                ChordTransition {
                    chord: chord_a,
                    phase: TriggerPhase::Released,
                },
                ChordTransition {
                    chord: chord_ab.clone(),
                    phase: TriggerPhase::Pressed,
                },
            ]
        );

        // Soltar B libera A+B y NO re-dispara A aunque A siga presionada.
        assert_eq!(
            tracker.release('B'),
            vec![ChordTransition {
                chord: chord_ab,
                phase: TriggerPhase::Released,
            }]
        );

        // Soltar A tampoco emite nada adicional.
        assert!(tracker.release('A').is_empty());
    }

    #[test]
    fn tracker_no_activa_subconjunto_si_superconjunto_esta_activo_y_libera_todo() {
        let mut tracker = ChordTracker::new();
        let chord_a = Chord::single('A');
        let chord_b = Chord::single('B');
        let chord_ab = Chord::from_atoms(['A', 'B']).expect("A+B");
        tracker.set_chords([chord_a, chord_b, chord_ab.clone()]);

        // Si A+B está activo (pulsando A y luego B), release_all lo libera ordenadamente.
        let _ = tracker.press('A');
        let _ = tracker.press('B');
        assert_eq!(
            tracker.release_all(),
            vec![ChordTransition {
                chord: chord_ab,
                phase: TriggerPhase::Released,
            }]
        );
        assert!(tracker.release_all().is_empty());
    }

    #[test]
    fn histeresis_analogica_evita_rebotes_y_descarta_no_finitos() {
        let mut filter = AnalogHysteresis::new(0.5);
        assert_eq!(filter.update(f32::NAN), None);
        assert_eq!(filter.update(f32::INFINITY), None);
        assert_eq!(filter.update(0.49), None);
        assert_eq!(filter.update(0.50), Some(true));
        // En la banda muerta entre release_threshold (0.35) y 0.50 mantiene estado activo.
        assert_eq!(filter.update(0.42), None);
        assert_eq!(filter.update(0.80), None);
        assert_eq!(filter.update(0.36), None);
        assert_eq!(filter.update(0.35), Some(false));
        assert_eq!(filter.update(0.10), None);

        assert!(!filter.reset());
        assert_eq!(filter.update(0.9), Some(true));
        assert!(filter.reset());
        assert!(!filter.reset());
    }

    #[test]
    fn parse_combo_tokens_divide_y_valida_segmentos_vacios() {
        let ok = parse_combo_tokens(&[" Ctrl + A ", " B "], |tok| Ok::<_, ()>(tok.to_string()))
            .expect("combo válido");
        assert_eq!(ok.0, "Ctrl + A+B");
        assert_eq!(ok.1, vec!["Ctrl", "A", "B"]);

        assert_eq!(
            parse_combo_tokens::<String, (), &str, _>(&[], |t| Ok(t.to_string())),
            Err(ComboSplitError::Empty)
        );
        assert!(matches!(
            parse_combo_tokens(&["A+"], |t| Ok::<_, ()>(t.to_string())),
            Err(ComboSplitError::EmptySegment(_))
        ));
        assert!(matches!(
            parse_combo_tokens(&["A", "   "], |t| Ok::<_, ()>(t.to_string())),
            Err(ComboSplitError::EmptySegment(_))
        ));
        assert_eq!(
            parse_combo_tokens(&["A+bad"], |t| {
                if t == "bad" {
                    Err("desconocido")
                } else {
                    Ok(t.to_string())
                }
            }),
            Err(ComboSplitError::Token("desconocido"))
        );
    }
}
