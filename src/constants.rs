//! Shared constants for The Veiled Tyrant.
//!
//! Engine-wide tuning defaults (tile size, Z-layers, base action cost)
//! now live in `roguelike_engine::constants` and are re-exported below
//! so existing `use crate::constants::*;` sites continue to work.
//!
//! Game-specific balance values (max floor count, unarmed damage dice)
//! stay in this module — they're part of The Veiled Tyrant's content
//! definition, not the engine.

pub use roguelike_engine::constants::{
    BASE_ACTION_COST, TILE_SIZE_X, TILE_SIZE_Y, Z_ITEM, Z_MONSTER, Z_PLAYER,
};

// --- Game-specific balance constants ---

/// The deepest floor in the dungeon. The final boss spawns here instead of stairs.
pub const MAX_FLOOR: i32 = 26;

/// Damage dice used when no weapon is equipped (bare fists).
pub const UNARMED_DAMAGE: &str = "1d4";
