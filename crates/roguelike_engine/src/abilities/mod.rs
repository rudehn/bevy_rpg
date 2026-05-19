//! Ability / spell framework.
//!
//! This module provides the data structures, events, cooldown management,
//! and targeting helpers that every ability system needs:
//!
//! - [`TargetingRule`] — how an ability selects its targets (self, single,
//!   AoE, line, custom).
//! - [`AbilityTarget`] — the resolved target of a specific ability use.
//! - [`AbilityDef`] — a data-only descriptor loaded from content manifests.
//! - [`AbilitySlot`] / [`Abilities`] — per-entity runtime loadout with
//!   independent cooldown tracking.
//! - [`AbilityUseEvent`] — an event requesting ability activation.
//! - Pure helpers: [`ability_action_cost`], [`ability_aoe_tiles`],
//!   [`is_in_ability_range`].
//! - [`AbilityPlugin`] — registers the event and cooldown tick system.
//!
//! The full resolution system (reading `AbilityUseEvent`, looking up
//! `AbilityDef`, emitting `DamageEvent` / status effects) is intentionally
//! left to the game crate — it needs game-specific ability registries and
//! content. The engine provides the plumbing.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

use crate::combat::DamageType;
use crate::geometry::tiles_in_aoe;
use crate::status::StatusEffectKind;

// =====================================================================
// TargetingRule
// =====================================================================

/// How an ability selects its targets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetingRule {
    /// Affects the caster only (buffs, self-heals).
    SelfOnly,
    /// Single target within `range` tiles (Manhattan distance).
    SingleTarget { range: u32 },
    /// Area of effect centered on a point within `range`, affecting all
    /// tiles within `radius`.
    Aoe { range: u32, radius: u32 },
    /// Line from caster in a direction, up to `range` tiles.
    Line { range: u32 },
}

// =====================================================================
// AbilityTarget
// =====================================================================

/// The resolved target of an ability use.
#[derive(Clone, Debug)]
pub enum AbilityTarget {
    SelfCast,
    Point(Point),
    Entity(Entity),
}

// =====================================================================
// AbilityDef
// =====================================================================

/// Data-only descriptor for an ability. Games define these in content
/// manifests (RON files, spawn tables, etc.) and attach them to entities
/// via the [`Abilities`] component.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbilityDef {
    pub id: u32,
    pub name: String,
    /// Turns before this ability can be used again after activation.
    pub cooldown: u32,
    /// Multiplier applied to `BASE_ACTION_COST` when this ability is used.
    /// 1.0 = normal turn cost, 0.5 = half turn, 2.0 = double.
    pub action_cost_multiplier: f32,
    /// How the ability selects targets.
    pub targeting: TargetingRule,
    /// Optional damage dice notation (e.g. "2d6+1"). `None` = no damage.
    pub damage_dice: Option<String>,
    /// Damage type for the dice damage. Defaults to Physical if unset.
    pub damage_type: DamageType,
    /// Optional status effect to apply: (kind, duration_turns, magnitude).
    pub status_effect: Option<(StatusEffectKind, u32, i32)>,
    /// AoE radius for the ability (0 = single target).
    pub aoe_radius: u32,
}

impl Default for AbilityDef {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            cooldown: 0,
            action_cost_multiplier: 1.0,
            targeting: TargetingRule::SelfOnly,
            damage_dice: None,
            damage_type: DamageType::Physical,
            status_effect: None,
            aoe_radius: 0,
        }
    }
}

// =====================================================================
// AbilitySlot
// =====================================================================

/// Runtime state for one ability slot on an entity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbilitySlot {
    /// Which ability definition this slot holds.
    pub def_id: u32,
    /// Turns remaining before this ability can be used again. 0 = ready.
    pub current_cooldown: u32,
}

// =====================================================================
// Abilities component
// =====================================================================

/// Per-entity ability loadout. Each slot holds a reference to an
/// `AbilityDef` (by id) and tracks its cooldown independently.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Abilities {
    pub slots: Vec<AbilitySlot>,
}

impl Abilities {
    /// Tick all cooldowns down by 1 (call once per turn).
    pub fn tick_cooldowns(&mut self) {
        for slot in &mut self.slots {
            if slot.current_cooldown > 0 {
                slot.current_cooldown -= 1;
            }
        }
    }

    /// Check if the ability with `def_id` is off cooldown.
    pub fn can_use(&self, def_id: u32) -> bool {
        self.slots
            .iter()
            .find(|s| s.def_id == def_id)
            .is_some_and(|s| s.current_cooldown == 0)
    }

    /// Put the ability on cooldown. Returns false if not found.
    pub fn trigger_cooldown(&mut self, def_id: u32, cooldown: u32) -> bool {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.def_id == def_id) {
            slot.current_cooldown = cooldown;
            true
        } else {
            false
        }
    }

    /// Add a new ability slot.
    pub fn add(&mut self, def_id: u32) {
        self.slots.push(AbilitySlot {
            def_id,
            current_cooldown: 0,
        });
    }
}

// =====================================================================
// Pure helper functions
// =====================================================================

/// Compute the action cost for using an ability.
///
/// Multiplies `base_cost` by the ability's `action_cost_multiplier` and
/// rounds. Games feed this into `compute_reinsert_time`.
pub fn ability_action_cost(base_cost: u32, multiplier: f32) -> u32 {
    (base_cost as f32 * multiplier).round() as u32
}

/// Get all valid target points for an AoE ability.
///
/// Returns all points within `radius` of `center` using Chebyshev distance
/// (reuses `tiles_in_aoe` from the geometry module).
pub fn ability_aoe_tiles(center_x: i32, center_y: i32, radius: u32) -> Vec<(i32, i32)> {
    tiles_in_aoe(center_x, center_y, radius as i32)
}

/// Check if a target point is within range of the caster.
pub fn is_in_ability_range(
    caster_x: i32,
    caster_y: i32,
    target_x: i32,
    target_y: i32,
    range: u32,
) -> bool {
    let dx = (caster_x - target_x).unsigned_abs();
    let dy = (caster_y - target_y).unsigned_abs();
    // Manhattan distance
    (dx + dy) <= range
}

// =====================================================================
// AbilityUseEvent
// =====================================================================

/// Event requesting an ability activation. The resolution system
/// validates targeting and applies effects.
#[derive(Message, Debug, Clone)]
pub struct AbilityUseEvent {
    pub caster: Entity,
    pub ability_id: u32,
    pub target: AbilityTarget,
}

// =====================================================================
// System set & plugin
// =====================================================================

/// System set for ability systems. Games configure ordering and run
/// conditions via `configure_sets`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AbilitySet;

/// Bevy plugin that registers ability messages and systems.
///
/// Does NOT configure system ordering or `run_if` predicates -- that
/// is the game's responsibility via [`AbilitySet`].
pub struct AbilityPlugin;

impl Plugin for AbilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AbilityUseEvent>();
        app.add_systems(Update, ability_cooldown_tick_system.in_set(AbilitySet));
    }
}

/// Tick all ability cooldowns each turn.
pub fn ability_cooldown_tick_system(mut query: Query<&mut Abilities>) {
    for mut abilities in query.iter_mut() {
        abilities.tick_cooldowns();
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Abilities component tests
    // -----------------------------------------------------------------

    #[test]
    fn tick_cooldowns_decrements() {
        let mut abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 3,
            }],
        };
        abilities.tick_cooldowns();
        assert_eq!(abilities.slots[0].current_cooldown, 2);
    }

    #[test]
    fn tick_cooldowns_stops_at_zero() {
        let mut abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 0,
            }],
        };
        abilities.tick_cooldowns();
        assert_eq!(abilities.slots[0].current_cooldown, 0);
    }

    #[test]
    fn can_use_ready() {
        let abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 0,
            }],
        };
        assert!(abilities.can_use(1));
    }

    #[test]
    fn can_use_on_cooldown() {
        let abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 2,
            }],
        };
        assert!(!abilities.can_use(1));
    }

    #[test]
    fn can_use_missing_id() {
        let abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 0,
            }],
        };
        assert!(!abilities.can_use(99));
    }

    #[test]
    fn trigger_cooldown_sets_value() {
        let mut abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 0,
            }],
        };
        assert!(abilities.trigger_cooldown(1, 5));
        assert_eq!(abilities.slots[0].current_cooldown, 5);
    }

    #[test]
    fn trigger_cooldown_missing_returns_false() {
        let mut abilities = Abilities {
            slots: vec![AbilitySlot {
                def_id: 1,
                current_cooldown: 0,
            }],
        };
        assert!(!abilities.trigger_cooldown(99, 5));
    }

    #[test]
    fn add_ability() {
        let mut abilities = Abilities::default();
        abilities.add(42);
        assert_eq!(abilities.slots.len(), 1);
        assert_eq!(abilities.slots[0].def_id, 42);
        assert_eq!(abilities.slots[0].current_cooldown, 0);
    }

    // -----------------------------------------------------------------
    // Pure helper tests
    // -----------------------------------------------------------------

    #[test]
    fn ability_action_cost_normal() {
        assert_eq!(ability_action_cost(100, 1.0), 100);
    }

    #[test]
    fn ability_action_cost_half() {
        assert_eq!(ability_action_cost(100, 0.5), 50);
    }

    #[test]
    fn ability_action_cost_double() {
        assert_eq!(ability_action_cost(100, 2.0), 200);
    }

    #[test]
    fn is_in_ability_range_within() {
        // Distance = |3-5| + |3-4| = 3, range = 5 -> within
        assert!(is_in_ability_range(3, 3, 5, 4, 5));
    }

    #[test]
    fn is_in_ability_range_outside() {
        // Distance = |0-10| + |0-10| = 20, range = 5 -> outside
        assert!(!is_in_ability_range(0, 0, 10, 10, 5));
    }

    #[test]
    fn is_in_ability_range_exact() {
        // Distance = |0-3| + |0-2| = 5, range = 5 -> exactly at range
        assert!(is_in_ability_range(0, 0, 3, 2, 5));
    }

    #[test]
    fn ability_aoe_tiles_count() {
        // Chebyshev radius 1 around a center = 3x3 = 9 tiles
        let tiles = ability_aoe_tiles(5, 5, 1);
        assert_eq!(tiles.len(), 9);

        // Chebyshev radius 2 around a center = 5x5 = 25 tiles
        let tiles = ability_aoe_tiles(5, 5, 2);
        assert_eq!(tiles.len(), 25);
    }

    #[test]
    fn default_ability_def() {
        let def = AbilityDef::default();
        assert_eq!(def.id, 0);
        assert!(def.name.is_empty());
        assert_eq!(def.cooldown, 0);
        assert!((def.action_cost_multiplier - 1.0).abs() < f32::EPSILON);
        assert_eq!(def.targeting, TargetingRule::SelfOnly);
        assert!(def.damage_dice.is_none());
        assert_eq!(def.damage_type, DamageType::Physical);
        assert!(def.status_effect.is_none());
        assert_eq!(def.aoe_radius, 0);
    }
}
