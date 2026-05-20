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

/// The deepest floor in the dungeon. Player can descend 0 → 1 → … → 5:
/// floor 0 is the town hub (return Portal lives here), floors 1..=4
/// are forest, floor 5 is the cult temple. The Amulet of Yendor sits
/// inside the temple; return to the town Portal to win.
///
/// Typed `u32` to match `Floor(u32)` and every save-side floor index.
/// `map.depth` is `i32` (engine convention from bracket-lib); the few
/// places that compare against it cast explicitly.
pub const MAX_FLOOR: u32 = 5;

/// Damage dice used when no weapon is equipped (bare fists).
pub const UNARMED_DAMAGE: &str = "1d4";
