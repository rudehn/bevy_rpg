//! Floor index scheme and rendering-theme helpers for the linear
//! roguelike layout.
//!
//! The game is a traditional descend-stairs roguelike: floor 0 is the
//! town hub, floors 1..=`MAX_FLOOR` are the dungeon (currently two
//! forest floors). Going down a `>` tile takes you to `floor + 1`;
//! going up `<` returns you to `floor - 1`. The town's Portal tile is
//! the win-condition return point once the amulet is recovered.
//!
//! This module previously hosted an overworld 3×3 grid topology
//! (GridDir / neighbor / cardinal stair helpers + `MapExitTile`
//! components). That was ripped out when the game pivoted back to
//! linear floors; see `docs/design/DUNGEON.md` for the writeup.

use bevy::prelude::{Color, Component, Resource};

use crate::components::Position;

use crate::assets::TileManifest;
use crate::map::tile::{Tile, TerrainType, resolve_tile_bg, resolve_tile_display};

/// Final floor of the descent. Player can descend 0 → 1 → 2; floor 2
/// is the deepest authored floor (holds the amulet). Raising this is
/// content work — add more `FloorKind::Forest` floors or new variants.
pub const MAX_FLOOR: u32 = 2;

/// What kind of map a `Floor(u32)` index represents. Drives the
/// builder pipeline + visual theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorKind {
    /// Floor 0 — the hub town with the return portal.
    Town,
    /// Floors 1..=MAX_FLOOR — forest descent. `depth` is the floor's
    /// distance from town (1 = first forest, 2 = deeper forest). Kept
    /// as a `u8` so future content can branch on it (boss floors,
    /// thematic variation per depth).
    Forest { depth: u8 },
}

/// Classify a `Floor(u32)` index into a `FloorKind`.
///
/// Panics on out-of-range indices — the floor scheme is closed.
pub fn floor_kind(floor: u32) -> FloorKind {
    match floor {
        0 => FloorKind::Town,
        f if f <= MAX_FLOOR => FloorKind::Forest { depth: f as u8 },
        other => panic!("floor_kind: floor {other} is beyond MAX_FLOOR ({MAX_FLOOR})"),
    }
}

/// Per-run state that survives across floor transitions. Kept as a
/// struct so future per-run content (faction influence, NPC state,
/// quest flags) has a home, even though it's empty today.
#[derive(Resource, Clone, Debug, Default)]
pub struct OverworldState {}

/// Optional component on a tile entity that overrides the default
/// `>` / `<` terrain-based transition with an **explicit** destination.
///
/// In the linear-floor scheme, terrain stairs are sufficient and this
/// component is rarely used — but the materializer still threads
/// `exit_tile_spawn_list` through the build pipeline so future content
/// (warps, fast-travel, scripted teleporters) can stamp explicit
/// transitions without re-introducing infrastructure.
#[derive(Component, Clone, Copy, Debug)]
pub struct MapExitTile {
    pub destination_floor: u32,
    /// `Some(pos)` means the player arrives at exactly `pos`. `None`
    /// means the destination floor decides (its `<` / `>` position).
    pub destination_pos: Option<Position>,
}

/// Reseed per-run state at the start of a new game. No-op today;
/// future seeders hook here.
pub fn seed_overworld_state(_state: &mut OverworldState) {
    // Intentionally empty.
}

// =====================================================================
// Floor-theme + path rendering — used by the ASCII renderer.
// =====================================================================

/// `Decoration::Custom { id: TOWN_PATH_DECO_ID }` marks a floor tile
/// as part of the town's path network — the renderer overrides the
/// glyph + colour to read as packed dirt.
pub const TOWN_PATH_DECO_ID: u32 = 1;

/// Visual theme for a floor. Set per-floor by `spawn_dungeon`; read by
/// the ASCII renderer to override Wall/Floor glyphs and colours.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FloorTheme {
    #[default]
    Dungeon,
    Town,
    Forest,
}

impl FloorTheme {
    pub fn for_floor_kind(kind: FloorKind) -> Self {
        match kind {
            FloorKind::Town => FloorTheme::Town,
            FloorKind::Forest { .. } => FloorTheme::Forest,
        }
    }

    fn override_glyph(self, terrain: TerrainType) -> Option<&'static str> {
        match (self, terrain) {
            (FloorTheme::Forest, TerrainType::Wall) => Some("\u{2663}"), // ♣
            (FloorTheme::Forest, TerrainType::Floor) => Some(","),
            (FloorTheme::Town, TerrainType::Wall) => Some("\u{2593}"),    // ▓
            (FloorTheme::Town, TerrainType::Floor) => Some("."),
            _ => None,
        }
    }

    fn override_fg(self, terrain: TerrainType) -> Option<Color> {
        match (self, terrain) {
            (FloorTheme::Forest, TerrainType::Wall) => Some(Color::srgb(0.20, 0.55, 0.18)),
            (FloorTheme::Forest, TerrainType::Floor) => Some(Color::srgb(0.20, 0.35, 0.12)),
            (FloorTheme::Town, TerrainType::Wall) => Some(Color::srgb(0.55, 0.40, 0.25)),
            (FloorTheme::Town, TerrainType::Floor) => Some(Color::srgb(0.65, 0.55, 0.40)),
            _ => None,
        }
    }

    fn override_bg(self, terrain: TerrainType) -> Option<Color> {
        match (self, terrain) {
            (FloorTheme::Forest, TerrainType::Floor) => Some(Color::srgb(0.06, 0.10, 0.05)),
            (FloorTheme::Forest, TerrainType::Wall) => Some(Color::srgb(0.04, 0.07, 0.03)),
            (FloorTheme::Town, TerrainType::Floor) => Some(Color::srgb(0.18, 0.15, 0.10)),
            _ => None,
        }
    }
}

fn is_path_tile(tile: Tile) -> bool {
    matches!(
        tile.decoration,
        crate::map::tile::Decoration::Custom { id } if id == TOWN_PATH_DECO_ID
    )
}

/// Theme-aware wrapper around [`resolve_tile_display`]. Returns the
/// glyph, foreground colour, and tile display name.
pub fn themed_tile_display<'a>(
    tile: Tile,
    manifest: &'a TileManifest,
    theme: FloorTheme,
) -> (String, Color, &'a str) {
    if is_path_tile(tile) {
        return (".".to_string(), Color::srgb(0.45, 0.32, 0.18), tile.terrain.name());
    }
    let (glyph, fg, name) = resolve_tile_display(tile, manifest);
    // Only theme bare Wall/Floor. Decorations / liquids / stairs keep
    // their manifest defaults.
    if name != tile.terrain.name() {
        return (glyph, fg, name);
    }
    let new_glyph = theme
        .override_glyph(tile.terrain)
        .map(|s| s.to_string())
        .unwrap_or(glyph);
    let new_fg = theme.override_fg(tile.terrain).unwrap_or(fg);
    (new_glyph, new_fg, name)
}

/// Theme-aware wrapper around [`resolve_tile_bg`].
pub fn themed_tile_bg(tile: Tile, manifest: &TileManifest, theme: FloorTheme) -> Color {
    if is_path_tile(tile) {
        return Color::srgb(0.30, 0.22, 0.13);
    }
    let base = resolve_tile_bg(tile, manifest);
    if tile.liquid != crate::map::tile::LiquidType::None {
        return base;
    }
    theme.override_bg(tile.terrain).unwrap_or(base)
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
    }

    #[test]
    #[should_panic]
    fn floor_kind_out_of_range_panics() {
        let _ = floor_kind(MAX_FLOOR + 1);
    }

    #[test]
    fn floor_theme_for_kind() {
        assert_eq!(FloorTheme::for_floor_kind(FloorKind::Town), FloorTheme::Town);
        assert_eq!(
            FloorTheme::for_floor_kind(FloorKind::Forest { depth: 1 }),
            FloorTheme::Forest,
        );
    }

    #[test]
    fn forest_theme_overrides_wall_and_floor_glyphs() {
        assert_eq!(FloorTheme::Forest.override_glyph(TerrainType::Wall), Some("\u{2663}"));
        assert_eq!(FloorTheme::Forest.override_glyph(TerrainType::Floor), Some(","));
    }

    #[test]
    fn dungeon_theme_overrides_nothing() {
        for terrain in [TerrainType::Wall, TerrainType::Floor, TerrainType::Portal] {
            assert_eq!(FloorTheme::Dungeon.override_glyph(terrain), None);
            assert_eq!(FloorTheme::Dungeon.override_fg(terrain), None);
            assert_eq!(FloorTheme::Dungeon.override_bg(terrain), None);
        }
    }

    #[test]
    fn theme_leaves_stairs_and_portals_alone() {
        for terrain in [
            TerrainType::DownStairs,
            TerrainType::UpStairs,
            TerrainType::Portal,
            TerrainType::Door,
        ] {
            for theme in [FloorTheme::Forest, FloorTheme::Town] {
                assert_eq!(theme.override_glyph(terrain), None);
                assert_eq!(theme.override_bg(terrain), None);
            }
        }
    }
}
