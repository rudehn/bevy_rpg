//! Engine-wide tuning constants.
//!
//! These are the numeric defaults the engine uses for rendering, layer
//! ordering, and the turn scheduler. They are NOT game-balance values —
//! things like max floor count, starting HP, or unarmed damage stay in
//! the game crate. The split is:
//!
//! - **Engine** (this module): cell pixel dimensions, Z-layer ordering,
//!   default action cost, and other values that every game built on
//!   top of the engine needs regardless of its theme or balance.
//! - **Game**: balance values, content counts, damage dice defaults.
//!
//! Games are free to ignore these and define their own constants; these
//! are the "sensible defaults" the engine's built-in systems use.

/// Pixel width of a single map tile.
pub const TILE_SIZE_X: u32 = 16;

/// Pixel height of a single map tile.
pub const TILE_SIZE_Y: u32 = 16;

/// Z-layer for the player sprite (drawn on top of everything else).
pub const Z_PLAYER: f32 = 3.0;

/// Z-layer for monster sprites.
pub const Z_MONSTER: f32 = 2.0;

/// Z-layer for items on the ground.
pub const Z_ITEM: f32 = 1.0;

/// Base cost of a single action in the turn scheduler.
///
/// Actual re-insertion time is computed as
/// `round(BASE_ACTION_COST * speed_delay)` via
/// [`crate::turn::compute_reinsert_time`], so a base cost of 100 with a
/// speed delay of 0.5 produces an action cost of 50. Games can multiply
/// this for heavier actions (e.g., `BASE_ACTION_COST * 2` for a charged
/// attack).
pub const BASE_ACTION_COST: u32 = 100;

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_sizes_are_square() {
        assert_eq!(TILE_SIZE_X, TILE_SIZE_Y);
    }

    #[test]
    fn z_layers_ordered_player_above_monster_above_item() {
        assert!(Z_PLAYER > Z_MONSTER);
        assert!(Z_MONSTER > Z_ITEM);
        assert!(Z_ITEM > 0.0);
    }

    #[test]
    fn base_action_cost_is_nonzero() {
        // Zero cost would make `compute_reinsert_time` produce
        // zero-duration actions, which would block turn advancement.
        assert!(BASE_ACTION_COST > 0);
    }
}
