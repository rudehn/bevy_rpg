pub const TILE_SIZE_X: u32 = 16;
pub const TILE_SIZE_Y: u32 = 16;
pub const Z_PLAYER: f32 = 3.0;
pub const Z_MONSTER: f32 = 2.0;
pub const Z_ITEM: f32 = 1.0;

pub const BASE_ACTION_COST: u32 = 100;

/// The deepest floor in the dungeon. The final boss spawns here instead of stairs.
pub const MAX_FLOOR: i32 = 10;

/// Damage dice used when no weapon is equipped (bare fists).
pub const UNARMED_DAMAGE: &str = "1d4";
