//! Status effect / buff-debuff framework.
//!
//! This module provides a general-purpose status effect system for
//! turn-based roguelikes:
//!
//! - [`StatusEffectKind`] — an extensible enum of built-in effect types
//!   (burning, poisoned, stunned, etc.) with a `Custom { id }` variant.
//! - [`StatusEffects`] — a per-entity component holding a stack of active
//!   [`StatusEffectInstance`]s with duration, magnitude, and source tracking.
//! - Pure helper functions ([`compute_speed_modifier`],
//!   [`compute_damage_modifier`]) that derive gameplay multipliers from
//!   the active effects on an entity.
//! - [`StatusAppliedEvent`] and [`StatusExpiredEvent`] messages for game
//!   code to react to status changes.
//! - [`status_effect_tick_system`] — decrements durations, emits DoT
//!   damage via [`DamageEvent`], and fires expiration events.
//!
//! Register the system with [`StatusEffectPlugin`]; configure ordering
//! and run conditions via [`StatusEffectSet`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::events::DamageEvent;
use crate::combat::{DamageSource, DamageType};

// =====================================================================
// StatusEffectKind
// =====================================================================

/// The kind of status effect active on an entity.
///
/// `#[non_exhaustive]` so games can match with a fallback arm and new
/// variants can be added in patch releases. Use `Custom { id }` for
/// game-specific effects without forking the engine.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusEffectKind {
    Burning,
    Poisoned,
    Stunned,
    Hasted,
    Slowed,
    Strengthened,
    Weakened,
    Custom { id: u32 },
}

impl StatusEffectKind {
    /// Human-readable lowercase name for log messages and tooltips.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Burning => "burning",
            Self::Poisoned => "poisoned",
            Self::Stunned => "stunned",
            Self::Hasted => "hasted",
            Self::Slowed => "slowed",
            Self::Strengthened => "strengthened",
            Self::Weakened => "weakened",
            _ => "unknown",
        }
    }
}

// =====================================================================
// StatusEffectInstance
// =====================================================================

/// A single active status effect with duration, magnitude, and source.
///
/// `source` tracks which entity applied the effect (for kill credit on
/// DoT kills, dispel targeting, etc.). It is skipped during
/// serialization because [`Entity`] identifiers are not stable across
/// save/load cycles.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusEffectInstance {
    pub kind: StatusEffectKind,
    pub remaining_turns: u32,
    pub magnitude: i32,
    #[serde(skip)]
    pub source: Option<Entity>,
}

// =====================================================================
// StatusEffects component
// =====================================================================

/// Per-entity collection of active status effects.
///
/// Attach this component to any entity that can receive buffs or
/// debuffs. The [`StatusEffects::add`] method handles refresh semantics
/// (same-kind effects merge by taking the longer duration and higher
/// magnitude).
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatusEffects {
    pub effects: Vec<StatusEffectInstance>,
}

impl StatusEffects {
    /// Add a status effect, refreshing an existing one of the same kind.
    ///
    /// If the entity already has an effect of the same [`StatusEffectKind`],
    /// the duration is extended to `max(existing, new)` and the magnitude
    /// is raised to `max(existing, new)`. The source is updated to the
    /// newest applicator.
    pub fn add(&mut self, instance: StatusEffectInstance) {
        if let Some(existing) = self.effects.iter_mut().find(|e| e.kind == instance.kind) {
            existing.remaining_turns = existing.remaining_turns.max(instance.remaining_turns);
            existing.magnitude = existing.magnitude.max(instance.magnitude);
            existing.source = instance.source;
        } else {
            self.effects.push(instance);
        }
    }

    /// Returns `true` if the entity currently has an effect of `kind`.
    pub fn has(&self, kind: StatusEffectKind) -> bool {
        self.effects.iter().any(|e| e.kind == kind)
    }

    /// Returns the magnitude of the effect of `kind`, or `0` if absent.
    pub fn magnitude_of(&self, kind: StatusEffectKind) -> i32 {
        self.effects
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.magnitude)
            .unwrap_or(0)
    }

    /// Remove all effects of `kind`.
    pub fn remove(&mut self, kind: StatusEffectKind) {
        self.effects.retain(|e| e.kind != kind);
    }

    /// Remove effects whose `remaining_turns` has reached zero.
    ///
    /// Returns the kinds of effects that were removed, so callers can
    /// emit expiration events or trigger on-expire logic.
    pub fn remove_expired(&mut self) -> Vec<StatusEffectKind> {
        let expired: Vec<StatusEffectKind> = self
            .effects
            .iter()
            .filter(|e| e.remaining_turns == 0)
            .map(|e| e.kind)
            .collect();
        self.effects.retain(|e| e.remaining_turns > 0);
        expired
    }
}

// =====================================================================
// Pure helper functions
// =====================================================================

/// Compute speed modifier from active status effects.
///
/// - **Hasted**: 0.5x delay (acts twice as fast)
/// - **Slowed**: 1.5x delay (acts 50% slower)
/// - **Stunned**: 100x delay (effectively skip turn)
///
/// Modifiers stack multiplicatively.
pub fn compute_speed_modifier(effects: &StatusEffects) -> f32 {
    let mut modifier = 1.0;
    if effects.has(StatusEffectKind::Hasted) {
        modifier *= 0.5;
    }
    if effects.has(StatusEffectKind::Slowed) {
        modifier *= 1.5;
    }
    if effects.has(StatusEffectKind::Stunned) {
        modifier *= 100.0;
    }
    modifier
}

/// Compute damage modifier from active status effects.
///
/// - **Strengthened**: +50% damage
/// - **Weakened**: -25% damage
///
/// Modifiers stack multiplicatively.
pub fn compute_damage_modifier(effects: &StatusEffects) -> f32 {
    let mut modifier = 1.0;
    if effects.has(StatusEffectKind::Strengthened) {
        modifier *= 1.5;
    }
    if effects.has(StatusEffectKind::Weakened) {
        modifier *= 0.75;
    }
    modifier
}

// =====================================================================
// Messages
// =====================================================================

/// Emitted when a status effect's duration reaches zero and is removed.
#[derive(Message, Debug, Clone)]
pub struct StatusExpiredEvent {
    pub entity: Entity,
    pub kind: StatusEffectKind,
}

/// Emitted when a status effect is applied to an entity (for UI
/// feedback, logging, or reactive game logic).
#[derive(Message, Debug, Clone)]
pub struct StatusAppliedEvent {
    pub entity: Entity,
    pub kind: StatusEffectKind,
    pub remaining_turns: u32,
    pub magnitude: i32,
}

// =====================================================================
// System set & plugin
// =====================================================================

/// System set for the status effect tick pipeline. Games configure
/// ordering and run conditions via `configure_sets`:
///
/// ```ignore
/// app.configure_sets(
///     Update,
///     StatusEffectSet
///         .after(MyTurnResolutionSet)
///         .run_if(in_state(MyGameState::Running)),
/// );
/// ```
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatusEffectSet;

/// Bevy plugin that registers status effect messages and systems.
///
/// Does NOT configure system ordering or `run_if` predicates -- that
/// is the game's responsibility via [`StatusEffectSet`].
pub struct StatusEffectPlugin;

impl Plugin for StatusEffectPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StatusExpiredEvent>();
        app.add_message::<StatusAppliedEvent>();
        app.add_systems(
            Update,
            status_effect_tick_system.in_set(StatusEffectSet),
        );
    }
}

// =====================================================================
// Systems
// =====================================================================

/// Tick all active status effects: apply DoT damage, decrement
/// durations, remove expired effects, and emit expiration events.
///
/// **Order within each entity**:
/// 1. Fire DoT damage (Burning, Poisoned) *before* decrementing.
/// 2. Decrement `remaining_turns` by 1.
/// 3. Remove effects that hit zero and emit [`StatusExpiredEvent`].
pub fn status_effect_tick_system(
    mut query: Query<(Entity, &mut StatusEffects)>,
    mut expired_writer: MessageWriter<StatusExpiredEvent>,
    mut damage_writer: MessageWriter<DamageEvent>,
) {
    for (entity, mut effects) in query.iter_mut() {
        // 1. Apply DoT effects BEFORE decrementing
        for effect in effects.effects.iter() {
            match effect.kind {
                StatusEffectKind::Burning => {
                    damage_writer.write(DamageEvent {
                        target: entity,
                        amount: effect.magnitude,
                        damage_type: DamageType::Fire,
                        source: DamageSource::Environment,
                        attacker: effect.source,
                        armor: 0,
                    });
                }
                StatusEffectKind::Poisoned => {
                    damage_writer.write(DamageEvent {
                        target: entity,
                        amount: effect.magnitude,
                        damage_type: DamageType::Poison,
                        source: DamageSource::Environment,
                        attacker: effect.source,
                        armor: 0,
                    });
                }
                _ => {}
            }
        }

        // 2. Decrement durations
        for effect in effects.effects.iter_mut() {
            if effect.remaining_turns > 0 {
                effect.remaining_turns -= 1;
            }
        }

        // 3. Remove expired and emit events
        let expired = effects.remove_expired();
        for kind in expired {
            expired_writer.write(StatusExpiredEvent { entity, kind });
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a default StatusEffects with no active effects.
    fn empty_effects() -> StatusEffects {
        StatusEffects::default()
    }

    /// Helper: build a StatusEffectInstance with no source.
    fn make_effect(kind: StatusEffectKind, turns: u32, magnitude: i32) -> StatusEffectInstance {
        StatusEffectInstance {
            kind,
            remaining_turns: turns,
            magnitude,
            source: None,
        }
    }

    // -----------------------------------------------------------------
    // add / refresh
    // -----------------------------------------------------------------

    #[test]
    fn add_new_effect() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Burning, 3, 5));
        assert!(effects.has(StatusEffectKind::Burning));
        assert_eq!(effects.effects.len(), 1);
        assert_eq!(effects.effects[0].remaining_turns, 3);
        assert_eq!(effects.effects[0].magnitude, 5);
    }

    #[test]
    fn add_refreshes_existing() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Poisoned, 3, 2));
        effects.add(make_effect(StatusEffectKind::Poisoned, 5, 1));
        // Duration takes max(3, 5) = 5
        assert_eq!(effects.effects.len(), 1);
        assert_eq!(effects.effects[0].remaining_turns, 5);
        // Magnitude takes max(2, 1) = 2
        assert_eq!(effects.effects[0].magnitude, 2);
    }

    // -----------------------------------------------------------------
    // has
    // -----------------------------------------------------------------

    #[test]
    fn has_returns_true_for_present() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Stunned, 1, 0));
        assert!(effects.has(StatusEffectKind::Stunned));
    }

    #[test]
    fn has_returns_false_for_absent() {
        let effects = empty_effects();
        assert!(!effects.has(StatusEffectKind::Hasted));
    }

    // -----------------------------------------------------------------
    // magnitude_of
    // -----------------------------------------------------------------

    #[test]
    fn magnitude_of_returns_value() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Strengthened, 5, 10));
        assert_eq!(effects.magnitude_of(StatusEffectKind::Strengthened), 10);
    }

    #[test]
    fn magnitude_of_returns_zero_for_absent() {
        let effects = empty_effects();
        assert_eq!(effects.magnitude_of(StatusEffectKind::Weakened), 0);
    }

    // -----------------------------------------------------------------
    // remove
    // -----------------------------------------------------------------

    #[test]
    fn remove_clears_effect() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Burning, 3, 5));
        effects.add(make_effect(StatusEffectKind::Poisoned, 2, 3));
        effects.remove(StatusEffectKind::Burning);
        assert!(!effects.has(StatusEffectKind::Burning));
        assert!(effects.has(StatusEffectKind::Poisoned));
    }

    // -----------------------------------------------------------------
    // remove_expired
    // -----------------------------------------------------------------

    #[test]
    fn remove_expired_returns_kinds() {
        let mut effects = empty_effects();
        effects.effects.push(StatusEffectInstance {
            kind: StatusEffectKind::Stunned,
            remaining_turns: 0,
            magnitude: 0,
            source: None,
        });
        effects.effects.push(StatusEffectInstance {
            kind: StatusEffectKind::Hasted,
            remaining_turns: 2,
            magnitude: 0,
            source: None,
        });

        let expired = effects.remove_expired();
        assert_eq!(expired, vec![StatusEffectKind::Stunned]);
        assert_eq!(effects.effects.len(), 1);
        assert!(effects.has(StatusEffectKind::Hasted));
    }

    // -----------------------------------------------------------------
    // compute_speed_modifier
    // -----------------------------------------------------------------

    #[test]
    fn compute_speed_modifier_hasted() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Hasted, 3, 0));
        assert!((compute_speed_modifier(&effects) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_speed_modifier_slowed() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Slowed, 3, 0));
        assert!((compute_speed_modifier(&effects) - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_speed_modifier_both() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Hasted, 3, 0));
        effects.add(make_effect(StatusEffectKind::Slowed, 3, 0));
        // 1.0 * 0.5 * 1.5 = 0.75
        assert!((compute_speed_modifier(&effects) - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_speed_modifier_none() {
        let effects = empty_effects();
        assert!((compute_speed_modifier(&effects) - 1.0).abs() < f32::EPSILON);
    }

    // -----------------------------------------------------------------
    // compute_damage_modifier
    // -----------------------------------------------------------------

    #[test]
    fn compute_damage_modifier_strengthened() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Strengthened, 3, 0));
        assert!((compute_damage_modifier(&effects) - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_damage_modifier_weakened() {
        let mut effects = empty_effects();
        effects.add(make_effect(StatusEffectKind::Weakened, 3, 0));
        assert!((compute_damage_modifier(&effects) - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_damage_modifier_none() {
        let effects = empty_effects();
        assert!((compute_damage_modifier(&effects) - 1.0).abs() < f32::EPSILON);
    }

    // -----------------------------------------------------------------
    // Custom kind
    // -----------------------------------------------------------------

    #[test]
    fn custom_kind_works() {
        let mut effects = empty_effects();
        let custom = StatusEffectKind::Custom { id: 42 };
        effects.add(make_effect(custom, 5, 7));
        assert!(effects.has(custom));
        assert_eq!(effects.magnitude_of(custom), 7);
        assert_eq!(custom.name(), "unknown");
    }
}
