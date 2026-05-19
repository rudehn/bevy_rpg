//! Pure decision helpers for floor transitions.
//!
//! Everything here is a plain function over plain data — no Bevy
//! `World`, no `Commands`, no resources. The transition systems
//! ([`crate::map::dungeon::apply_map_transition`] and
//! [`crate::map::dungeon::spawn_dungeon`]) call into these helpers so
//! the decisions behind a floor transition can be unit-tested without
//! spinning up an `App`.

use crate::map::world::{FloorKind, FloorTheme, floor_kind};

// ---------------------------------------------------------------------------
// Welcome line + theme
// ---------------------------------------------------------------------------

/// One-line welcome message shown when the player arrives on a floor.
pub fn welcome_line_for_floor(floor: u32) -> String {
    match floor_kind(floor) {
        FloorKind::Town => "You arrive in the town square.".to_string(),
        FloorKind::Forest { depth } => {
            format!("You step deeper into the forest. (floor {depth})")
        }
        FloorKind::Temple => {
            "Cold stone closes around you. The cult's shrine.".to_string()
        }
    }
}

/// Renderer theme for a floor. Falls back to `FloorTheme::Dungeon` for
/// legacy floor indices (>= 12) which the overworld pipelines do not
/// cover.
pub fn theme_for_floor(floor: u32) -> FloorTheme {
    FloorTheme::for_floor_kind(floor_kind(floor))
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

    // ----- welcome_line --------------------------------------------------

    #[test]
    fn welcome_line_town_is_a_fixed_phrase() {
        assert_eq!(welcome_line_for_floor(0), "You arrive in the town square.");
    }

    #[test]
    fn welcome_line_forest_carries_depth() {
        assert_eq!(
            welcome_line_for_floor(1),
            "You step deeper into the forest. (floor 1)",
        );
        assert_eq!(
            welcome_line_for_floor(2),
            "You step deeper into the forest. (floor 2)",
        );
    }

    // ----- theme_for_floor -----------------------------------------------

    #[test]
    fn theme_matches_floor_kind() {
        assert_eq!(theme_for_floor(0), FloorTheme::Town);
        assert_eq!(theme_for_floor(1), FloorTheme::Forest);
        assert_eq!(theme_for_floor(2), FloorTheme::Forest);
        assert_eq!(theme_for_floor(3), FloorTheme::Forest);
        assert_eq!(theme_for_floor(4), FloorTheme::Forest);
        assert_eq!(theme_for_floor(crate::map::world::MAX_FLOOR), FloorTheme::Temple);
    }

    #[test]
    fn welcome_line_temple_is_distinct() {
        let line = welcome_line_for_floor(crate::map::world::MAX_FLOOR);
        assert!(line.contains("shrine") || line.contains("temple") || line.contains("cult"));
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
