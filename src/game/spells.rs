use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Spell Effect Types ---

/// How a spell selects its target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpellTarget {
    /// Affects only the caster.
    Caster,
    /// Strikes the nearest visible enemy.
    NearestEnemy,
}

/// A single effect applied when the spell resolves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpellEffect {
    /// Deal damage to the target. `dice` is "NdM" format; optionally scaled by INT bonus.
    Damage { dice: String, int_scaling: bool },
    /// Restore HP to the caster. `dice` is "NdM" format; optionally scaled by INT bonus.
    HealCaster { dice: String, int_scaling: bool },
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
