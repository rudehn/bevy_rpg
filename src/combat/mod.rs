//! Combat math primitives and core damage types.
//!
//! This module ships:
//!
//! - Pure arithmetic functions used by the combat pipeline: armor
//!   reduction, resistance percentages, and status-buff damage
//!   multipliers. No ECS state, no components, no Bevy `World` — so
//!   they can be unit-tested in isolation.
//! - The [`DamageType`] enum (physical, fire, lightning, poison) — the
//!   engine's "sensible default" damage type set, marked
//!   `#[non_exhaustive]` so additional types can be added later without
//!   breaking consumers.
//! - The [`DamageSource`] enum (melee, ranged, spell, environment) —
//!   where the damage came from.
//! - [`Resistances`], a per-entity `HashMap<DamageType, i32>` resistance
//!   component, and [`DamageTypeTag`], a marker component that tags an
//!   entity's native damage type.
//! - [`Health`], [`HealthRegen`], and [`RegenSuppression`] — the HP
//!   pool, regeneration tracker, and post-damage regen lockout. Pure
//!   data components; the systems that tick them live in the game crate.
//!
//! The full combat pipeline (intent → hit check → damage roll → armor
//! → resistance → apply) still lives in the game crate. Migration of
//! the pipeline is tracked in the extraction plan.

use bevy::prelude::{Component, Reflect, ReflectComponent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =====================================================================
// Damage types
// =====================================================================

/// The elemental / physical type of damage dealt.
///
/// The engine ships with this blessed set because it covers the most
/// common roguelike damage types. `#[non_exhaustive]` means new variants
/// can be added in patch releases without breaking downstream match
/// arms — callers must provide a fallback match arm.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Reflect)]
pub enum DamageType {
    #[default]
    Physical,
    Fire,
    Lightning,
    Poison,
}

impl DamageType {
    /// Parse a damage type from a case-insensitive string. Unknown
    /// strings fall back to [`DamageType::Physical`].
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fire" => DamageType::Fire,
            "lightning" => DamageType::Lightning,
            "poison" => DamageType::Poison,
            _ => DamageType::Physical,
        }
    }

    /// Lowercase name of the damage type for log messages and tooltips.
    pub fn name(&self) -> &'static str {
        match self {
            DamageType::Physical => "physical",
            DamageType::Fire => "fire",
            DamageType::Lightning => "lightning",
            DamageType::Poison => "poison",
        }
    }
}

/// Per-entity resistance map.
///
/// Values are percentages consumed by [`apply_resistance`]:
/// - `0` = normal damage
/// - `50` = 50% reduction
/// - `100` = immune
/// - `>100` = absorb/heal
/// - negative = vulnerability (takes extra damage)
///
/// Missing entries return `0` (normal damage) via [`Resistances::get`].
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Resistances(pub HashMap<DamageType, i32>);

impl Resistances {
    /// Look up the resistance value for `damage_type`. Returns `0`
    /// (normal damage) if the entity has no entry for that type.
    pub fn get(&self, damage_type: &DamageType) -> i32 {
        self.0.get(damage_type).copied().unwrap_or(0)
    }
}

/// Marker component: tags an entity's melee damage with a specific type.
///
/// Attach this to a weapon or a monster to declare that its basic
/// attacks deal a specific damage type.
#[derive(Component, Debug, Clone)]
pub struct DamageTypeTag(pub DamageType);

/// Where the damage originated from.
///
/// `#[non_exhaustive]` so games can add a case for their custom source
/// with a fallback arm in match expressions.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageSource {
    Melee,
    Ranged,
    Spell,
    Environment,
}

// =====================================================================
// Pure damage-math helpers
// =====================================================================

/// Apply armor reduction to raw damage.
///
/// Armor subtracts from raw damage and clamps to zero, so enough armor
/// can fully negate an attack. This function does not handle resistance —
/// pipe the result through [`apply_resistance`] for damage-type reduction.
pub fn compute_after_armor(raw_damage: i32, armor: i32) -> i32 {
    (raw_damage - armor).max(0)
}

/// Apply a resistance percentage to damage.
///
/// `resist_percent` is the reduction in whole-percent units:
/// - `0` = normal damage
/// - `50` = half damage
/// - `100` = immune (zero damage)
/// - `>100` = absorb/heal (returns a negative value)
/// - `<0` = vulnerability (returns more than `damage`)
pub fn apply_resistance(damage: i32, resist_percent: i32) -> i32 {
    let multiplier = 1.0 - (resist_percent as f32 / 100.0);
    (damage as f32 * multiplier).round() as i32
}

/// Apply status multipliers to base damage.
///
/// - `is_enraged`: +50% damage (multiplied, not added)
/// - `is_terrified`: -25% damage
/// - Final result is clamped to a minimum of 1, so a hit never rounds
///   to zero just from status effects.
///
/// Crits are handled upstream by doubling the damage dice, not here.
pub fn apply_damage_multipliers(base: i32, is_enraged: bool, is_terrified: bool) -> i32 {
    let mut damage = base;
    if is_enraged {
        damage = damage * 3 / 2;
    }
    if is_terrified {
        damage = damage * 3 / 4;
    }
    damage.max(1)
}

// =====================================================================
// Health components
// =====================================================================

/// Current and maximum hit points for an entity.
///
/// The engine ships `Health` as a plain pair of integers; clamping,
/// death detection, and regen are the game's responsibility (typically
/// via the game crate's damage pipeline systems). `current` may be
/// negative briefly while damage is being applied — the game's death
/// system is expected to observe the transition and despawn or flag
/// the entity accordingly.
#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

/// Component for passive health regeneration.
///
/// `regen_rate` is a fractional points-per-turn value scaled by 100:
/// a rate of 20 means "1 HP every 5 turns" (20 / 100 per turn). Each
/// turn the regen system adds `regen_rate` to `regen_accumulator` and
/// when the accumulator crosses 100 it consumes the 100 and restores
/// 1 HP. Games that want a different resolution can scale both fields.
#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct HealthRegen {
    pub regen_rate: i32,
    pub regen_accumulator: i32,
}

/// Suppresses HP regen for N turns after taking damage.
///
/// Games typically insert this (or increment it) when damage lands
/// on an entity, then tick it down each turn. Regen systems should
/// skip entities with a nonzero `RegenSuppression.0`.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect, Default)]
#[reflect(Component)]
pub struct RegenSuppression(pub u32);

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- DamageType ---

    #[test]
    fn damage_type_default_is_physical() {
        assert_eq!(DamageType::default(), DamageType::Physical);
    }

    #[test]
    fn damage_type_from_str_known_values() {
        assert_eq!(DamageType::from_str("fire"), DamageType::Fire);
        assert_eq!(DamageType::from_str("LIGHTNING"), DamageType::Lightning);
        assert_eq!(DamageType::from_str("poison"), DamageType::Poison);
    }

    #[test]
    fn damage_type_from_str_unknown_falls_back_to_physical() {
        assert_eq!(DamageType::from_str("unknown"), DamageType::Physical);
        assert_eq!(DamageType::from_str(""), DamageType::Physical);
        assert_eq!(DamageType::from_str("ice"), DamageType::Physical);
    }

    #[test]
    fn damage_type_names_are_lowercase() {
        assert_eq!(DamageType::Physical.name(), "physical");
        assert_eq!(DamageType::Fire.name(), "fire");
        assert_eq!(DamageType::Lightning.name(), "lightning");
        assert_eq!(DamageType::Poison.name(), "poison");
    }

    // --- Resistances ---

    #[test]
    fn resistances_default_returns_zero_for_all_types() {
        let r = Resistances::default();
        assert_eq!(r.get(&DamageType::Fire), 0);
        assert_eq!(r.get(&DamageType::Lightning), 0);
        assert_eq!(r.get(&DamageType::Poison), 0);
        assert_eq!(r.get(&DamageType::Physical), 0);
    }

    #[test]
    fn resistances_lookup_populated() {
        let mut map = HashMap::new();
        map.insert(DamageType::Fire, 100);
        map.insert(DamageType::Lightning, -50);
        let r = Resistances(map);
        assert_eq!(r.get(&DamageType::Fire), 100);
        assert_eq!(r.get(&DamageType::Lightning), -50);
        // Missing entry returns zero, not a panic.
        assert_eq!(r.get(&DamageType::Physical), 0);
    }

    // --- compute_after_armor ---

    #[test]
    fn armor_reduces_damage() {
        assert_eq!(compute_after_armor(10, 3), 7);
    }

    #[test]
    fn armor_can_reduce_to_zero() {
        assert_eq!(compute_after_armor(5, 100), 0);
    }

    #[test]
    fn zero_armor_passes_through() {
        assert_eq!(compute_after_armor(8, 0), 8);
    }

    // --- apply_resistance ---

    #[test]
    fn resistance_zero_is_normal() {
        assert_eq!(apply_resistance(10, 0), 10);
    }

    #[test]
    fn resistance_50_halves_damage() {
        assert_eq!(apply_resistance(10, 50), 5);
    }

    #[test]
    fn resistance_100_is_immune() {
        assert_eq!(apply_resistance(10, 100), 0);
    }

    #[test]
    fn resistance_150_heals() {
        assert_eq!(apply_resistance(10, 150), -5);
    }

    #[test]
    fn resistance_negative_50_is_vulnerable() {
        assert_eq!(apply_resistance(10, -50), 15);
    }

    // --- apply_damage_multipliers ---

    #[test]
    fn no_multipliers_passes_through() {
        assert_eq!(apply_damage_multipliers(10, false, false), 10);
    }

    #[test]
    fn enraged_adds_50_percent() {
        assert_eq!(apply_damage_multipliers(10, true, false), 15);
    }

    #[test]
    fn terrified_reduces_25_percent() {
        assert_eq!(apply_damage_multipliers(10, false, true), 7);
    }

    #[test]
    fn enraged_and_terrified_stack_multiplicatively() {
        // 10 * 1.5 (enrage) = 15, then 15 * 0.75 (terrified) = 11
        assert_eq!(apply_damage_multipliers(10, true, true), 11);
    }

    #[test]
    fn minimum_damage_is_one() {
        assert_eq!(apply_damage_multipliers(1, false, true), 1);
    }

    // --- Full pipeline integration: armor + resistance ---

    #[test]
    fn armor_then_resistance_vulnerable() {
        let after_armor = compute_after_armor(20, 5); // 15
        let final_damage = apply_resistance(after_armor, -50); // 15 * 1.5 = 22.5 -> 23
        assert_eq!(final_damage, 23);
    }

    #[test]
    fn armor_then_resistance_immune() {
        let after_armor = compute_after_armor(20, 5); // 15
        let final_damage = apply_resistance(after_armor, 100); // 0
        assert_eq!(final_damage, 0);
    }

    #[test]
    fn armor_then_resistance_absorb() {
        let after_armor = compute_after_armor(20, 5); // 15
        let final_damage = apply_resistance(after_armor, 150); // 15 * -0.5 = -7.5 -> -8
        assert_eq!(final_damage, -8);
    }
}
