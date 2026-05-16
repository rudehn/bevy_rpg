//! Pure decision helpers for floor transitions.
//!
//! Everything here is a plain function over plain data — no Bevy
//! `World`, no `Commands`, no resources. The transition systems
//! ([`crate::map::dungeon::apply_map_transition`] and
//! [`crate::map::dungeon::spawn_dungeon`]) call into these helpers so
//! the decisions behind a floor transition can be unit-tested without
//! spinning up an `App`.

use crate::components::Position;
use crate::map::Map;
use crate::map::tile::TerrainType;
use crate::map::world::{FloorKind, FloorTheme, OverworldState, floor_kind};

// ---------------------------------------------------------------------------
// Overworld writeback
// ---------------------------------------------------------------------------

/// If `floor` is the chosen temple-entrance forest tile, scan `map` for
/// the `DownStairs` the forest builder stamped and return its position
/// so the caller can latch it onto [`OverworldState::temple_entrance_pos`].
///
/// Returns `None` when this floor is not the entrance forest, or when
/// the map has no `DownStairs` (the legacy / non-overworld pipeline).
///
/// Replaces the post-hoc scan that used to live inline in
/// `spawn_dungeon` so the writeback can be tested without a Bevy World.
pub fn overworld_edit_for_floor(
    floor: u32,
    map: &Map,
    overworld: &OverworldState,
) -> Option<Position> {
    if floor != overworld.temple_entrance_floor {
        return None;
    }
    map.tiles.iter().enumerate().find_map(|(idx, tile)| {
        if tile.terrain == TerrainType::DownStairs {
            let (x, y) = map.idx_xy(idx);
            Some(Position { x, y })
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// Welcome line + theme
// ---------------------------------------------------------------------------

/// One-line welcome message shown when the player arrives on a floor.
/// Falls back to a generic "descend into the dungeon" line for legacy
/// floor indices (>= 12) that the overworld pipelines do not cover.
pub fn welcome_line_for_floor(floor: u32) -> String {
    if floor <= 11 {
        match floor_kind(floor) {
            FloorKind::Town => "You arrive in the town square.".to_string(),
            FloorKind::Forest(_) => format!("You step into the forest. (floor {floor})"),
            FloorKind::Temple(n) => format!("You descend into the temple. (level {n})"),
        }
    } else {
        format!("You descend into the dungeon. (floor {floor})")
    }
}

/// Renderer theme for a floor. Falls back to `FloorTheme::Dungeon` for
/// legacy floor indices (>= 12) which the overworld pipelines do not
/// cover.
pub fn theme_for_floor(floor: u32) -> FloorTheme {
    if floor <= 11 {
        FloorTheme::for_floor_kind(floor_kind(floor))
    } else {
        FloorTheme::Dungeon
    }
}

// ---------------------------------------------------------------------------
// Source dispatch
// ---------------------------------------------------------------------------

/// Classifier for which source `spawn_dungeon` should materialise from.
/// Mirrors the three arms of [`crate::map::floor_materializer::FloorSource`]
/// but is decoupled from the source data so the *priority* is testable
/// on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorSourceKind {
    Load,
    Restore,
    Generate,
}

/// Priority: a pending game-load wins, else a pending floor-restore,
/// else fresh generation.
pub fn pick_source_kind(
    has_pending_load: bool,
    has_pending_restore: bool,
) -> FloorSourceKind {
    if has_pending_load {
        FloorSourceKind::Load
    } else if has_pending_restore {
        FloorSourceKind::Restore
    } else {
        FloorSourceKind::Generate
    }
}

// ---------------------------------------------------------------------------
// Transition decision (snapshot / restore / arrival direction)
// ---------------------------------------------------------------------------

/// Outcome of deciding what `apply_map_transition` should do given a
/// transition request. Encodes the three pieces of state the system
/// has to set up before firing `SpawnDungeonMessage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionDecision {
    /// Always true when source != target; kept as a field so tests can
    /// assert the no-op branch.
    pub snapshot_source: bool,
    /// True when the destination has been visited before (a
    /// `CachedFloor` exists for it).
    pub restore_target: bool,
    /// "Ascending" in stair-relative arrival terms — the player is
    /// returning to a numerically smaller floor, so the destination's
    /// `DownStairs` is the natural landing spot.
    pub ascending: bool,
}

/// Pure decision: given source/target floors and whether the floor
/// cache has the target, compute the three flags `apply_map_transition`
/// needs. Returns `None` for the no-op case (`source == target`).
pub fn decide_transition(
    source_floor: u32,
    target_floor: u32,
    target_in_cache: bool,
) -> Option<TransitionDecision> {
    if source_floor == target_floor {
        return None;
    }
    Some(TransitionDecision {
        snapshot_source: true,
        restore_target: target_in_cache,
        ascending: target_floor < source_floor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, LiquidType, Tile};
    use crate::map::world::{GridDir, forest_index};

    fn empty_map(depth: u32, w: i32, h: i32) -> Map {
        let mut map = Map::new(depth as i32, w, h, "test");
        for t in map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        map
    }

    // ----- overworld_edit_for_floor --------------------------------------

    #[test]
    fn overworld_edit_returns_none_for_non_entrance_forest() {
        let mut map = empty_map(2, 10, 10);
        // Stamp a DownStairs at (5, 5).
        let idx = map.xy_idx(5, 5);
        map.tiles[idx].terrain = TerrainType::DownStairs;
        let overworld = OverworldState {
            temple_entrance_floor: 3, // entrance is floor 3, not 2
            temple_entrance_pos: None,
        };
        assert_eq!(overworld_edit_for_floor(2, &map, &overworld), None);
    }

    #[test]
    fn overworld_edit_returns_pos_for_entrance_forest_with_downstairs() {
        let mut map = empty_map(3, 10, 10);
        let idx = map.xy_idx(7, 4);
        map.tiles[idx].terrain = TerrainType::DownStairs;
        let overworld = OverworldState {
            temple_entrance_floor: 3,
            temple_entrance_pos: None,
        };
        assert_eq!(
            overworld_edit_for_floor(3, &map, &overworld),
            Some(Position { x: 7, y: 4 }),
        );
    }

    #[test]
    fn overworld_edit_returns_none_when_entrance_floor_has_no_downstairs() {
        let map = empty_map(3, 10, 10);
        let overworld = OverworldState {
            temple_entrance_floor: 3,
            temple_entrance_pos: None,
        };
        assert_eq!(overworld_edit_for_floor(3, &map, &overworld), None);
    }

    #[test]
    fn overworld_edit_picks_first_downstairs_when_multiple_exist() {
        // Forests shouldn't have two DownStairs, but the helper should
        // be deterministic — first walkable index wins.
        let mut map = empty_map(forest_index(GridDir::N), 10, 10);
        let idx_a = map.xy_idx(3, 3);
        let idx_b = map.xy_idx(7, 7);
        map.tiles[idx_a].terrain = TerrainType::DownStairs;
        map.tiles[idx_b].terrain = TerrainType::DownStairs;
        let overworld = OverworldState {
            temple_entrance_floor: forest_index(GridDir::N),
            temple_entrance_pos: None,
        };
        // (3, 3) has the lower index in row-major iteration.
        assert_eq!(
            overworld_edit_for_floor(forest_index(GridDir::N), &map, &overworld),
            Some(Position { x: 3, y: 3 }),
        );
    }

    // ----- welcome_line --------------------------------------------------

    #[test]
    fn welcome_line_town_is_a_fixed_phrase() {
        assert_eq!(welcome_line_for_floor(0), "You arrive in the town square.");
    }

    #[test]
    fn welcome_line_forest_carries_floor_index() {
        assert_eq!(
            welcome_line_for_floor(4),
            "You step into the forest. (floor 4)",
        );
    }

    #[test]
    fn welcome_line_temple_uses_level_number_not_floor_index() {
        assert_eq!(welcome_line_for_floor(9), "You descend into the temple. (level 1)");
        assert_eq!(welcome_line_for_floor(10), "You descend into the temple. (level 2)");
        assert_eq!(welcome_line_for_floor(11), "You descend into the temple. (level 3)");
    }

    #[test]
    fn welcome_line_legacy_dungeon_falls_through() {
        assert_eq!(
            welcome_line_for_floor(15),
            "You descend into the dungeon. (floor 15)",
        );
    }

    // ----- theme_for_floor -----------------------------------------------

    #[test]
    fn theme_matches_floor_kind() {
        assert_eq!(theme_for_floor(0), FloorTheme::Town);
        assert_eq!(theme_for_floor(3), FloorTheme::Forest);
        assert_eq!(theme_for_floor(10), FloorTheme::Temple);
    }

    #[test]
    fn theme_falls_back_to_dungeon_for_legacy_floors() {
        assert_eq!(theme_for_floor(12), FloorTheme::Dungeon);
        assert_eq!(theme_for_floor(20), FloorTheme::Dungeon);
    }

    // ----- pick_source_kind ----------------------------------------------

    #[test]
    fn pick_source_kind_load_wins() {
        assert_eq!(pick_source_kind(true, true), FloorSourceKind::Load);
        assert_eq!(pick_source_kind(true, false), FloorSourceKind::Load);
    }

    #[test]
    fn pick_source_kind_restore_when_no_load() {
        assert_eq!(pick_source_kind(false, true), FloorSourceKind::Restore);
    }

    #[test]
    fn pick_source_kind_generate_when_nothing_pending() {
        assert_eq!(pick_source_kind(false, false), FloorSourceKind::Generate);
    }

    // ----- decide_transition ---------------------------------------------

    #[test]
    fn decide_transition_returns_none_when_source_equals_target() {
        assert_eq!(decide_transition(3, 3, false), None);
        assert_eq!(decide_transition(3, 3, true), None);
    }

    #[test]
    fn decide_transition_ascending_means_target_below_source() {
        // Walking from floor 5 → floor 3 returns to a numerically
        // smaller floor → ascending = true (the destination's
        // DownStairs is where the player came from originally).
        let d = decide_transition(5, 3, false).unwrap();
        assert!(d.ascending);
    }

    #[test]
    fn decide_transition_descending_when_target_above_source() {
        let d = decide_transition(3, 5, false).unwrap();
        assert!(!d.ascending);
    }

    #[test]
    fn decide_transition_restore_target_flag_propagates_cache_lookup() {
        let with_cache = decide_transition(5, 3, true).unwrap();
        let no_cache = decide_transition(5, 3, false).unwrap();
        assert!(with_cache.restore_target);
        assert!(!no_cache.restore_target);
    }

    #[test]
    fn decide_transition_always_snapshots_source_when_not_a_no_op() {
        // The source floor must always be snapshotted on a real
        // transition — even when the destination is also new (so the
        // player can return and find the source preserved).
        for (s, t) in [(0u32, 1), (1, 2), (5, 4), (9, 11)] {
            let d = decide_transition(s, t, false).unwrap();
            assert!(d.snapshot_source);
        }
    }
}
