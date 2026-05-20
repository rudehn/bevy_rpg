//! Floor topology — index scheme, rendering theme, and pure decision
//! helpers for floor transitions. Everything here is plain data + plain
//! functions: no Bevy `World`, no `Commands`, no resources. The Bevy
//! adapter that drives floors at runtime lives in
//! [`crate::map::dungeon`] and calls into the decision helpers here.
//!
//! The game is a traditional descend-stairs roguelike: floor 0 is the
//! town hub, floors 1..=`MAX_FLOOR - 1` are the forest, and `MAX_FLOOR`
//! itself is the cult temple holding the Amulet of Yendor. Going down
//! a `>` tile takes you to `floor + 1`; going up `<` returns you to
//! `floor - 1`. The town's Portal tile is the win-condition return
//! point once the amulet is recovered.
//!
//! This module previously hosted an overworld 3×3 grid topology
//! (GridDir / neighbor / cardinal stair helpers + `MapExitTile`
//! components). That was ripped out when the game pivoted back to
//! linear floors; see `docs/design/OVERWORLD.md` for the writeup.

use bevy::prelude::{Color, Resource};

use crate::assets::TileManifest;
use crate::constants::MAX_FLOOR;
use crate::map::tile::{TerrainType, Tile, resolve_tile_bg, resolve_tile_display};

/// What kind of map a `Floor(u32)` index represents. Drives the
/// builder pipeline and the ASCII renderer's per-floor theming.
/// Stored as a resource by `spawn_dungeon` and read by tile rendering;
/// the game starts on the town hub, so `Town` is the default.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FloorKind {
    /// Floor 0 — the hub town with the return portal.
    #[default]
    Town,
    /// Floors 1..=MAX_FLOOR-1 — forest descent. `depth` is the floor's
    /// distance from town (1 = first forest, 4 = deepest forest, where
    /// the temple entrance hides). Kept as a `u8` so future content
    /// can branch on it (boss floors, thematic variation per depth).
    Forest { depth: u8 },
    /// Floor `MAX_FLOOR` — the cult temple. Linear corridor today;
    /// designed to descend into additional sub-floors in the future.
    Temple,
}

/// Classify a `Floor(u32)` index into a `FloorKind`.
///
/// Panics on out-of-range indices — the floor scheme is closed.
pub fn floor_kind(floor: u32) -> FloorKind {
    match floor {
        0 => FloorKind::Town,
        f if f < MAX_FLOOR => FloorKind::Forest { depth: f as u8 },
        f if f == MAX_FLOOR => FloorKind::Temple,
        other => panic!("floor_kind: floor {other} is beyond MAX_FLOOR ({MAX_FLOOR})"),
    }
}

// =====================================================================
// Per-floor theming — used by the ASCII renderer.
//
// All three lookups (glyph, foreground, background) match directly on
// `FloorKind`. The `Forest { depth }` payload is ignored today; if you
// want depth-tuned theming (e.g. Forest 4 walls darker than Forest 1)
// it slots in here without touching any other module.
// =====================================================================

impl FloorKind {
    fn override_glyph(self, terrain: TerrainType) -> Option<&'static str> {
        match (self, terrain) {
            (FloorKind::Forest { .. }, TerrainType::Wall) => Some("\u{2663}"), // ♣
            (FloorKind::Forest { .. }, TerrainType::Floor) => Some(" "),
            // Town walls fall through to the manifest default `#`.
            (FloorKind::Town, TerrainType::Floor) => Some("."),
            (FloorKind::Temple, TerrainType::Wall) => Some("\u{2592}"), // ▒
            (FloorKind::Temple, TerrainType::Floor) => Some("."),
            _ => None,
        }
    }

    fn override_fg(self, terrain: TerrainType) -> Option<Color> {
        match (self, terrain) {
            (FloorKind::Forest { .. }, TerrainType::Wall) => Some(Color::srgb(0.20, 0.55, 0.18)),
            (FloorKind::Forest { .. }, TerrainType::Floor) => Some(Color::srgb(0.40, 0.30, 0.18)),
            (FloorKind::Town, TerrainType::Wall) => Some(Color::srgb(0.55, 0.40, 0.25)),
            (FloorKind::Town, TerrainType::Floor) => Some(Color::srgb(0.65, 0.55, 0.40)),
            (FloorKind::Temple, TerrainType::Wall) => Some(Color::srgb(0.42, 0.42, 0.48)),
            (FloorKind::Temple, TerrainType::Floor) => Some(Color::srgb(0.30, 0.35, 0.32)),
            _ => None,
        }
    }

    fn override_bg(self, terrain: TerrainType) -> Option<Color> {
        match (self, terrain) {
            // Forest floor: dark loam — warm but low-saturation so the
            // floor recedes and lets decorations / walls / overlays
            // (poison gas, fire, blood) carry the visual signal. Per-tile
            // noise tint is applied at render time (see `themed_floor_bg`
            // in `ascii_renderer.rs`), so this is just the base palette.
            (FloorKind::Forest { .. }, TerrainType::Floor) => Some(Color::srgb(0.13, 0.11, 0.08)),
            (FloorKind::Forest { .. }, TerrainType::Wall) => Some(Color::srgb(0.04, 0.07, 0.03)),
            (FloorKind::Town, TerrainType::Floor) => Some(Color::srgb(0.18, 0.15, 0.10)),
            (FloorKind::Temple, TerrainType::Floor) => Some(Color::srgb(0.08, 0.09, 0.10)),
            (FloorKind::Temple, TerrainType::Wall) => Some(Color::srgb(0.05, 0.06, 0.07)),
            _ => None,
        }
    }
}

fn is_path_tile(tile: Tile) -> bool {
    matches!(tile.decoration, crate::map::tile::Decoration::TownPath)
}

/// Theme-aware wrapper around [`resolve_tile_display`]. Returns the
/// glyph, foreground colour, and tile display name.
pub fn themed_tile_display<'a>(
    tile: Tile,
    manifest: &'a TileManifest,
    kind: FloorKind,
) -> (String, Color, &'a str) {
    if is_path_tile(tile) {
        return (
            ".".to_string(),
            Color::srgb(0.45, 0.32, 0.18),
            tile.terrain.name(),
        );
    }
    let (glyph, fg, name) = resolve_tile_display(tile, manifest);
    // Only theme bare Wall/Floor. Decorations / liquids / stairs keep
    // their manifest defaults.
    if name != tile.terrain.name() {
        return (glyph, fg, name);
    }
    let new_glyph = kind
        .override_glyph(tile.terrain)
        .map(|s| s.to_string())
        .unwrap_or(glyph);
    let new_fg = kind.override_fg(tile.terrain).unwrap_or(fg);
    (new_glyph, new_fg, name)
}

/// Theme-aware wrapper around [`resolve_tile_bg`].
pub fn themed_tile_bg(tile: Tile, manifest: &TileManifest, kind: FloorKind) -> Color {
    if is_path_tile(tile) {
        return Color::srgb(0.30, 0.22, 0.13);
    }
    let base = resolve_tile_bg(tile, manifest);
    if tile.liquid != crate::map::tile::LiquidType::None {
        return base;
    }
    kind.override_bg(tile.terrain).unwrap_or(base)
}

// =====================================================================
// Floor-transition decision helpers
//
// Pure functions consumed by `dungeon::apply_map_transition` and
// `dungeon::spawn_dungeon`. Keeping them here (no Bevy types in
// signatures) so the priority + decision logic is unit-testable
// without spinning up an `App`.
// =====================================================================

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

    #[test]
    fn floor_kind_town_at_zero() {
        assert_eq!(floor_kind(0), FloorKind::Town);
    }

    #[test]
    fn floor_kind_forest_at_depth() {
        assert_eq!(floor_kind(1), FloorKind::Forest { depth: 1 });
        assert_eq!(floor_kind(2), FloorKind::Forest { depth: 2 });
        assert_eq!(floor_kind(3), FloorKind::Forest { depth: 3 });
        assert_eq!(floor_kind(4), FloorKind::Forest { depth: 4 });
    }

    #[test]
    fn floor_kind_temple_at_max_floor() {
        assert_eq!(floor_kind(MAX_FLOOR), FloorKind::Temple);
    }

    #[test]
    #[should_panic]
    fn floor_kind_out_of_range_panics() {
        let _ = floor_kind(MAX_FLOOR + 1);
    }

    #[test]
    fn forest_theme_overrides_wall_and_floor_glyphs() {
        let forest = FloorKind::Forest { depth: 1 };
        assert_eq!(forest.override_glyph(TerrainType::Wall), Some("\u{2663}"));
        assert_eq!(forest.override_glyph(TerrainType::Floor), Some(" "));
    }

    #[test]
    fn theme_leaves_stairs_and_portals_alone() {
        for terrain in [
            TerrainType::DownStairs,
            TerrainType::UpStairs,
            TerrainType::Portal,
            TerrainType::Door,
        ] {
            for kind in [
                FloorKind::Town,
                FloorKind::Forest { depth: 1 },
                FloorKind::Temple,
            ] {
                assert_eq!(kind.override_glyph(terrain), None);
                assert_eq!(kind.override_bg(terrain), None);
            }
        }
    }

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

    #[test]
    fn welcome_line_temple_is_distinct() {
        let line = welcome_line_for_floor(MAX_FLOOR);
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
