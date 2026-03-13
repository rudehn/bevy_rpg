use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::game::combat::DamageType;

// --- Spell Effect Types ---

/// How a spell selects its target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpellTarget {
    /// Affects only the caster.
    Castor,
    /// Targets a visible enemy.
    Enemy,
    /// Targets the most-wounded visible ally (not self).
    Ally,
    /// Targets the most-wounded visible ally or self.
    AllyOrSelf,
}

/// A single effect applied when the spell resolves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpellEffect {
    /// Deal damage to the target. `dice` is "NdM" format; optionally scaled by INT bonus.
    Damage { dice: String, int_scaling: bool },
    /// Restore HP to the resolved target. `dice` is "NdM" format; optionally scaled by INT bonus.
    /// Target is determined by SpellTarget (Castor = self-heal, Ally = ally heal, etc.)
    Heal { dice: String, int_scaling: bool },
    /// Damage all entities within `radius` Manhattan distance of the target tile.
    AoeDamage { dice: String, radius: i32, int_scaling: bool },
    /// Hit primary target, then jump to nearby enemies.
    ChainDamage { dice: String, max_jumps: i32, jump_range: i32, int_scaling: bool },
    /// Temporarily boost a target's attribute.
    Buff { attribute: String, amount: i32, duration: u32 },
    /// Temporarily reduce a target's attribute.
    Debuff { attribute: String, amount: i32, duration: u32 },
    /// Apply damage-over-time poison status.
    ApplyPoison { damage_per_turn: i32, duration: u32 },
    /// Grant +50% speed for N turns.
    ApplyHaste { duration: u32 },
    /// Inflict -50% speed for N turns.
    ApplySlow { duration: u32 },
    /// Remove mana from target, add to caster.
    DrainMana { amount: i32, int_scaling: bool },
    /// Damage taken from mana instead of HP for N turns.
    SpiritShield { duration: u32 },
    /// Move caster to a tile. range=0 → random, range>0 → controlled.
    Teleport { range: i32 },
    /// Apply +50% damage multiplier for N turns.
    ApplyEnrage { duration: u32 },
}

// --- Spell Data ---

/// Full spell definition loaded from `spells.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellData {
    pub name: String,
    pub mana_cost: i32,
    /// Turns the caster must wait before using this spell again (0 = no cooldown).
    pub cooldown: u32,
    pub description: String,
    pub target: SpellTarget,
    /// Maximum tile range of the spell (0 means self/unlimited for Caster spells).
    pub range: u32,
    pub effects: Vec<SpellEffect>,
    /// The damage type for this spell's damage effects. Defaults to Physical.
    #[serde(default)]
    pub damage_type: DamageType,
}

// --- Registry Asset ---

/// Loaded from `assets/spells.ron`. Holds all spell definitions keyed by ID.
#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct SpellRegistry {
    pub spells: HashMap<String, SpellData>,
}

// --- Helpers ---

/// Parse a dice expression in "NdM" format and roll it, returning the total.
/// Ignores malformed input (returns 0).
pub fn roll_dice_expr(rng: &mut bracket_lib::prelude::RandomNumberGenerator, expr: &str) -> i32 {
    let parts: Vec<&str> = expr.split('d').collect();
    if parts.len() != 2 {
        return 0;
    }
    let n = parts[0].parse::<i32>().unwrap_or(1);
    let m = parts[1].parse::<i32>().unwrap_or(6);
    rng.roll_dice(n, m)
}
