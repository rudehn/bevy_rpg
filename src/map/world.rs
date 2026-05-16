//! Overworld topology — floor index scheme and direction helpers.
//!
//! The overworld is a 3×3 grid: town at center (floor 0), 8 forest
//! tiles surrounding it (floors 1..=8), then a 3-floor temple beneath
//! one of the forest tiles (floors 9..=11). See
//! `docs/design/DUNGEON.md` for the full layout.

use bevy::prelude::{Color, Component, Resource};

use crate::assets::TileManifest;
use crate::components::Position;
use crate::map::map::MAP_SIZE;
use crate::map::tile::{Tile, TerrainType, resolve_tile_bg, resolve_tile_display};

/// Per-run overworld state. Picked at the start of a new game and
/// persisted across saves so the temple stays in the same forest tile
/// for the duration of a run.
#[derive(Resource, Clone, Copy, Debug)]
pub struct OverworldState {
    /// Which of the 8 forest tiles contains the temple entrance (1..=8).
    pub temple_entrance_floor: u32,
    /// Where the DownStairs sit on that forest tile. Written by the
    /// forest builder when it stamps the entrance; read by the temple
    /// builder so temple floor 1's UpStairs can return to that exact
    /// tile.
    pub temple_entrance_pos: Option<Position>,
}

impl Default for OverworldState {
    fn default() -> Self {
        Self {
            // Sentinel — `seed_overworld_state` rerolls this when a new
            // run starts. We avoid randomising in `Default` so unit
            // tests that init the resource get a deterministic value.
            temple_entrance_floor: 1,
            temple_entrance_pos: None,
        }
    }
}

/// Pick a random forest tile (1..=8) as the temple entrance. Called
/// when a new run begins (via `OnEnter(AppState::InGame)` in
/// `DungeonPlugin`). The save-load path overwrites this from
/// `GameSaveData` so the entrance stays put across reloads.
pub fn seed_overworld_state(state: &mut OverworldState) {
    let mut rng = bracket_lib::random::RandomNumberGenerator::new();
    state.temple_entrance_floor = rng.range(1, 9) as u32;
    state.temple_entrance_pos = None;
}

/// Tagged onto a tile entity to mark it as a one-way transition to
/// another floor.
///
/// Used for overworld edge tiles, the temple entrance, and the temple
/// exit. The legacy `TerrainType::DownStairs` / `UpStairs` still work
/// without this component (they imply `floor ± 1` with stair-relative
/// arrival); this component is the explicit-destination version.
#[derive(Component, Clone, Copy, Debug)]
pub struct MapExitTile {
    pub destination_floor: u32,
    /// `Some(pos)` means the player arrives at exactly `pos`.
    /// `None` means the destination floor decides (its UpStairs /
    /// DownStairs position, depending on which way we travelled).
    pub destination_pos: Option<Position>,
}

/// Eight-way grid direction.
///
/// Used for both overworld navigation (which neighbor a floor is) and
/// edge-tile placement (which wall an exit sits on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridDir {
    NW, N, NE,
    W,      E,
    SW, S, SE,
}

impl GridDir {
    pub const ALL: [GridDir; 8] = [
        GridDir::NW, GridDir::N, GridDir::NE,
        GridDir::W,              GridDir::E,
        GridDir::SW, GridDir::S, GridDir::SE,
    ];

    /// (dx, dy) on the 3×3 grid. dy follows screen convention (south = +1).
    pub fn delta(self) -> (i32, i32) {
        match self {
            GridDir::NW => (-1, -1),
            GridDir::N  => ( 0, -1),
            GridDir::NE => ( 1, -1),
            GridDir::W  => (-1,  0),
            GridDir::E  => ( 1,  0),
            GridDir::SW => (-1,  1),
            GridDir::S  => ( 0,  1),
            GridDir::SE => ( 1,  1),
        }
    }
}

/// What kind of map a `Floor(u32)` index represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorKind {
    /// Floor 0 — the hub town.
    Town,
    /// Floors 1..=8 — one of the 8 forest tiles.
    Forest(GridDir),
    /// Floors 9..=11 — temple level 1, 2, or 3.
    Temple(u8),
}

/// Forest floor index for a given grid direction (1..=8).
pub fn forest_index(dir: GridDir) -> u32 {
    match dir {
        GridDir::NW => 1,
        GridDir::N  => 2,
        GridDir::NE => 3,
        GridDir::W  => 4,
        GridDir::E  => 5,
        GridDir::SW => 6,
        GridDir::S  => 7,
        GridDir::SE => 8,
    }
}

fn forest_dir(index: u32) -> Option<GridDir> {
    match index {
        1 => Some(GridDir::NW),
        2 => Some(GridDir::N),
        3 => Some(GridDir::NE),
        4 => Some(GridDir::W),
        5 => Some(GridDir::E),
        6 => Some(GridDir::SW),
        7 => Some(GridDir::S),
        8 => Some(GridDir::SE),
        _ => None,
    }
}

/// Classify a `Floor(u32)` index into a `FloorKind`.
///
/// Panics on out-of-range indices (>= 12) — the floor scheme is closed
/// and any out-of-range floor is a programmer error.
pub fn floor_kind(floor: u32) -> FloorKind {
    match floor {
        0 => FloorKind::Town,
        1..=8 => FloorKind::Forest(forest_dir(floor).unwrap()),
        9 => FloorKind::Temple(1),
        10 => FloorKind::Temple(2),
        11 => FloorKind::Temple(3),
        other => panic!("floor_kind: invalid floor index {other}"),
    }
}

/// The reverse of a direction (180° turn).
pub fn mirror_dir(d: GridDir) -> GridDir {
    match d {
        GridDir::NW => GridDir::SE,
        GridDir::N  => GridDir::S,
        GridDir::NE => GridDir::SW,
        GridDir::W  => GridDir::E,
        GridDir::E  => GridDir::W,
        GridDir::SW => GridDir::NE,
        GridDir::S  => GridDir::N,
        GridDir::SE => GridDir::NW,
    }
}

/// The neighbor on the 3×3 overworld grid in direction `dir`, or
/// `None` if there is no neighbor (e.g., walking N off the NW forest
/// tile would leave the world).
///
/// Temple floors have no overworld neighbors — they connect by stairs.
pub fn neighbor(from: u32, dir: GridDir) -> Option<u32> {
    // Grid coords with town at (0, 0).
    let (fx, fy) = match floor_kind(from) {
        FloorKind::Town => (0, 0),
        FloorKind::Forest(d) => d.delta(),
        FloorKind::Temple(_) => return None,
    };
    let (dx, dy) = dir.delta();
    let (nx, ny) = (fx + dx, fy + dy);
    if !(-1..=1).contains(&nx) || !(-1..=1).contains(&ny) {
        return None;
    }
    if (nx, ny) == (0, 0) {
        return Some(0);
    }
    // Find the GridDir whose delta is (nx, ny).
    let neighbor_dir = GridDir::ALL.into_iter().find(|d| d.delta() == (nx, ny))?;
    Some(forest_index(neighbor_dir))
}

/// The valid overworld exit directions for a given floor.
///
/// - Town: all 8 directions.
/// - Forest tile: only the directions that lead to another in-bounds
///   overworld tile (so corner forests have 3 inward exits, edge
///   forests have 5).
/// - Temple: none.
pub fn valid_exits(floor: u32) -> Vec<GridDir> {
    GridDir::ALL
        .into_iter()
        .filter(|d| neighbor(floor, *d).is_some())
        .collect()
}

/// The 4 cardinal compass directions. Used for map-to-map
/// transitions: each border of a map maps to the mirror border of
/// the destination map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardinalDir { N, S, E, W }

impl CardinalDir {
    pub const ALL: [CardinalDir; 4] = [
        CardinalDir::N, CardinalDir::S, CardinalDir::E, CardinalDir::W,
    ];

    pub fn mirror(self) -> Self {
        match self {
            CardinalDir::N => CardinalDir::S,
            CardinalDir::S => CardinalDir::N,
            CardinalDir::E => CardinalDir::W,
            CardinalDir::W => CardinalDir::E,
        }
    }

    pub fn delta(self) -> (i32, i32) {
        match self {
            CardinalDir::N => (0, -1),
            CardinalDir::S => (0, 1),
            CardinalDir::E => (1, 0),
            CardinalDir::W => (-1, 0),
        }
    }

    /// Promote to a GridDir so we can use the existing `forest_index`
    /// table for floor numbering.
    fn as_grid(self) -> GridDir {
        match self {
            CardinalDir::N => GridDir::N,
            CardinalDir::S => GridDir::S,
            CardinalDir::E => GridDir::E,
            CardinalDir::W => GridDir::W,
        }
    }
}

/// How many stair tiles per border. Spread evenly along the border so
/// the K-th stair pairs with the K-th stair on the destination's
/// mirror border ("walk off the east → arrive on the west, same row").
pub const STAIRS_PER_BORDER: usize = 4;

/// Positions of the 4 stair tiles along the border in direction `dir`,
/// in K-order from low to high coordinate. **Clustered side-by-side
/// at the centre of the border** — 4 consecutive tiles whose midpoint
/// is the map's centre x (for N/S) or centre y (for E/W). Always one
/// tile inside the map border so the stair is reachable from the
/// interior.
pub fn border_stair_positions(dir: CardinalDir) -> [Position; STAIRS_PER_BORDER] {
    let w = MAP_SIZE.x as i32;
    let h = MAP_SIZE.y as i32;
    let xs = cluster(w);
    let ys = cluster(h);
    match dir {
        CardinalDir::N => [
            Position { x: xs[0], y: 1 },
            Position { x: xs[1], y: 1 },
            Position { x: xs[2], y: 1 },
            Position { x: xs[3], y: 1 },
        ],
        CardinalDir::S => [
            Position { x: xs[0], y: h - 2 },
            Position { x: xs[1], y: h - 2 },
            Position { x: xs[2], y: h - 2 },
            Position { x: xs[3], y: h - 2 },
        ],
        CardinalDir::E => [
            Position { x: w - 2, y: ys[0] },
            Position { x: w - 2, y: ys[1] },
            Position { x: w - 2, y: ys[2] },
            Position { x: w - 2, y: ys[3] },
        ],
        CardinalDir::W => [
            Position { x: 1, y: ys[0] },
            Position { x: 1, y: ys[1] },
            Position { x: 1, y: ys[2] },
            Position { x: 1, y: ys[3] },
        ],
    }
}

/// Where the player arrives when stepping off the K-th stair on the
/// `exit_dir` border. Lands on the K-th stair on the destination's
/// mirror border so the player ends up at the same column (N↔S) or
/// row (E↔W) — the StairCooldown prevents an immediate re-trigger.
pub fn arrival_at_mirror(exit_dir: CardinalDir, k: usize) -> Position {
    border_stair_positions(exit_dir.mirror())[k]
}

/// 4 consecutive coordinates centred on the midpoint of `length`. For
/// length=80 returns [38, 39, 40, 41]; for length=60 returns [28, 29,
/// 30, 31]. The cluster matches on the mirror border, so the K-th
/// stair lines up with the K-th destination stair.
fn cluster(length: i32) -> [i32; 4] {
    let mid = length / 2;
    [mid - 2, mid - 1, mid, mid + 1]
}

/// The neighbour floor in cardinal direction `dir`, or `None` if the
/// step would leave the 3×3 overworld grid (or this floor is a
/// temple). Always uses the cardinal-only restriction — no diagonals.
pub fn cardinal_neighbor(floor: u32, dir: CardinalDir) -> Option<u32> {
    let (col, row) = match floor_kind(floor) {
        FloorKind::Town => (1, 1),
        FloorKind::Forest(d) => {
            let (dx, dy) = d.delta();
            (1 + dx, 1 + dy)
        }
        FloorKind::Temple(_) => return None,
    };
    let (dx, dy) = dir.delta();
    let (nc, nr) = (col + dx, row + dy);
    if !(0..=2).contains(&nc) || !(0..=2).contains(&nr) {
        return None;
    }
    if (nc, nr) == (1, 1) {
        return Some(0); // town
    }
    let neighbor_dir = GridDir::ALL.into_iter().find(|d| d.delta() == (nc - 1, nr - 1))?;
    Some(forest_index(neighbor_dir))
}

/// Which cardinal exits are valid for this floor (only the directions
/// whose neighbour is in-bounds).
pub fn valid_cardinal_exits(floor: u32) -> Vec<CardinalDir> {
    CardinalDir::ALL.into_iter()
        .filter(|d| cardinal_neighbor(floor, *d).is_some())
        .collect()
}

// Silence unused-warning lint when as_grid isn't used elsewhere.
#[allow(dead_code)]
fn _force_use_as_grid(d: CardinalDir) -> GridDir { d.as_grid() }

/// `Decoration::Custom { id: TOWN_PATH_DECO_ID }` marks a floor tile
/// as part of the town's path network — the renderer overrides the
/// glyph + colour to read as packed dirt. Defined here (not in the
/// engine) because path tiles are a town-specific concept.
pub const TOWN_PATH_DECO_ID: u32 = 1;

/// Visual theme for a floor, applied in the ASCII renderer to override
/// the default Wall/Floor glyph + color so different overworld biomes
/// read distinctly without requiring new `TerrainType` variants.
///
/// `FloorTheme::Dungeon` is the legacy/default appearance.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FloorTheme {
    #[default]
    Dungeon,
    Town,
    Forest,
    Temple,
}

impl FloorTheme {
    pub fn for_floor_kind(kind: FloorKind) -> Self {
        match kind {
            FloorKind::Town       => FloorTheme::Town,
            FloorKind::Forest(_)  => FloorTheme::Forest,
            FloorKind::Temple(_)  => FloorTheme::Temple,
        }
    }

    /// Theme override for the **glyph** of a base terrain type, or
    /// `None` to use the manifest default. Only Wall + Floor are themed —
    /// stairs, doors, portal, etc. stay manifest-driven.
    fn override_glyph(self, terrain: TerrainType) -> Option<&'static str> {
        match (self, terrain) {
            (FloorTheme::Forest, TerrainType::Wall)  => Some("♣"),
            (FloorTheme::Forest, TerrainType::Floor) => Some(","),
            (FloorTheme::Town,   TerrainType::Wall)  => Some("▓"),
            (FloorTheme::Town,   TerrainType::Floor) => Some("."),
            _ => None,
        }
    }

    /// Theme override for the **foreground colour** of a base terrain type.
    fn override_fg(self, terrain: TerrainType) -> Option<Color> {
        match (self, terrain) {
            (FloorTheme::Forest, TerrainType::Wall)  => Some(Color::srgb(0.20, 0.55, 0.18)),
            (FloorTheme::Forest, TerrainType::Floor) => Some(Color::srgb(0.20, 0.35, 0.12)),
            (FloorTheme::Town,   TerrainType::Wall)  => Some(Color::srgb(0.55, 0.40, 0.25)),
            (FloorTheme::Town,   TerrainType::Floor) => Some(Color::srgb(0.65, 0.55, 0.40)),
            (FloorTheme::Temple, TerrainType::Wall)  => Some(Color::srgb(0.40, 0.50, 0.40)),
            _ => None,
        }
    }

    /// Theme override for the **background colour** of a base terrain type.
    fn override_bg(self, terrain: TerrainType) -> Option<Color> {
        match (self, terrain) {
            (FloorTheme::Forest, TerrainType::Floor) => Some(Color::srgb(0.06, 0.10, 0.05)),
            (FloorTheme::Forest, TerrainType::Wall)  => Some(Color::srgb(0.04, 0.07, 0.03)),
            (FloorTheme::Town,   TerrainType::Floor) => Some(Color::srgb(0.18, 0.15, 0.10)),
            _ => None,
        }
    }
}

/// Is this tile the town's path decoration?
fn is_path_tile(tile: Tile) -> bool {
    matches!(
        tile.decoration,
        crate::map::tile::Decoration::Custom { id } if id == TOWN_PATH_DECO_ID
    )
}

/// Theme-aware wrapper around [`resolve_tile_display`].
///
/// Returns the manifest default unless the active `FloorTheme`
/// overrides this terrain's glyph/colour. Decorations, liquids, and
/// priority terrain (stairs, doors, portal) are unaffected — the
/// override only fires when the un-themed code path would have used
/// the base Wall/Floor glyph. Town path tiles get a packed-dirt look
/// regardless of the surrounding theme so the path stays readable.
pub fn themed_tile_display<'a>(
    tile: Tile,
    manifest: &'a TileManifest,
    theme: FloorTheme,
) -> (String, Color, &'a str) {
    // Town paths render as a darker `.` regardless of terrain theme.
    if is_path_tile(tile) {
        return (
            ".".to_string(),
            Color::srgb(0.45, 0.32, 0.18),
            tile.terrain.name(),
        );
    }

    let (glyph, fg, name) = resolve_tile_display(tile, manifest);

    // Only theme bare Wall/Floor tiles. If a decoration or liquid took
    // priority, `name` won't be the terrain name — leave it alone.
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
    // Liquids own their own bg unconditionally.
    if tile.liquid != crate::map::tile::LiquidType::None {
        return base;
    }
    theme.override_bg(tile.terrain).unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_kind_classification() {
        assert_eq!(floor_kind(0), FloorKind::Town);
        assert_eq!(floor_kind(1), FloorKind::Forest(GridDir::NW));
        assert_eq!(floor_kind(2), FloorKind::Forest(GridDir::N));
        assert_eq!(floor_kind(5), FloorKind::Forest(GridDir::E));
        assert_eq!(floor_kind(8), FloorKind::Forest(GridDir::SE));
        assert_eq!(floor_kind(9), FloorKind::Temple(1));
        assert_eq!(floor_kind(10), FloorKind::Temple(2));
        assert_eq!(floor_kind(11), FloorKind::Temple(3));
    }

    #[test]
    #[should_panic]
    fn floor_kind_out_of_range() {
        let _ = floor_kind(12);
    }

    #[test]
    fn forest_index_roundtrip() {
        for d in GridDir::ALL {
            assert_eq!(floor_kind(forest_index(d)), FloorKind::Forest(d));
        }
    }

    #[test]
    fn mirror_dir_is_involution() {
        for d in GridDir::ALL {
            assert_eq!(mirror_dir(mirror_dir(d)), d);
        }
    }

    #[test]
    fn mirror_dir_pairs() {
        assert_eq!(mirror_dir(GridDir::N), GridDir::S);
        assert_eq!(mirror_dir(GridDir::NE), GridDir::SW);
        assert_eq!(mirror_dir(GridDir::E), GridDir::W);
    }

    #[test]
    fn neighbor_from_town() {
        // Town has all 8 forest neighbors.
        for d in GridDir::ALL {
            assert_eq!(neighbor(0, d), Some(forest_index(d)));
        }
    }

    #[test]
    fn neighbor_back_to_town() {
        // Walking back toward town from any forest tile by going the
        // mirror of the tile's own direction lands at floor 0.
        for d in GridDir::ALL {
            assert_eq!(neighbor(forest_index(d), mirror_dir(d)), Some(0));
        }
    }

    #[test]
    fn neighbor_corner_forest_world_edge() {
        // NW forest tile (floor 1) is at grid (-1, -1).
        // It can only reach the world by going E (to N forest), S (to W forest),
        // or SE (back to town). N, W, NW, NE, SW all leave the world.
        let floor = forest_index(GridDir::NW);
        assert_eq!(neighbor(floor, GridDir::N), None);
        assert_eq!(neighbor(floor, GridDir::W), None);
        assert_eq!(neighbor(floor, GridDir::NW), None);
        assert_eq!(neighbor(floor, GridDir::NE), None);
        assert_eq!(neighbor(floor, GridDir::SW), None);
        assert_eq!(neighbor(floor, GridDir::E), Some(forest_index(GridDir::N)));
        assert_eq!(neighbor(floor, GridDir::S), Some(forest_index(GridDir::W)));
        assert_eq!(neighbor(floor, GridDir::SE), Some(0));
    }

    #[test]
    fn neighbor_edge_forest_has_five_exits() {
        // N forest tile (floor 2) is at grid (0, -1). World-edge to the
        // north; valid exits: W, E (other forests), SW, S, SE (toward town row).
        let n = forest_index(GridDir::N);
        assert_eq!(neighbor(n, GridDir::N), None);
        assert_eq!(neighbor(n, GridDir::NW), None);
        assert_eq!(neighbor(n, GridDir::NE), None);
        assert_eq!(neighbor(n, GridDir::W), Some(forest_index(GridDir::NW)));
        assert_eq!(neighbor(n, GridDir::E), Some(forest_index(GridDir::NE)));
        assert_eq!(neighbor(n, GridDir::SW), Some(forest_index(GridDir::W)));
        assert_eq!(neighbor(n, GridDir::S), Some(0));
        assert_eq!(neighbor(n, GridDir::SE), Some(forest_index(GridDir::E)));
    }

    #[test]
    fn neighbor_temple_has_none() {
        for d in GridDir::ALL {
            assert_eq!(neighbor(9, d), None);
            assert_eq!(neighbor(10, d), None);
            assert_eq!(neighbor(11, d), None);
        }
    }

    #[test]
    fn valid_exits_count() {
        assert_eq!(valid_exits(0).len(), 8); // town
        assert_eq!(valid_exits(forest_index(GridDir::NW)).len(), 3); // corner
        assert_eq!(valid_exits(forest_index(GridDir::N)).len(), 5); // edge
        assert_eq!(valid_exits(9).len(), 0); // temple
    }

    #[test]
    fn cardinal_directions_mirror() {
        assert_eq!(CardinalDir::N.mirror(), CardinalDir::S);
        assert_eq!(CardinalDir::S.mirror(), CardinalDir::N);
        assert_eq!(CardinalDir::E.mirror(), CardinalDir::W);
        assert_eq!(CardinalDir::W.mirror(), CardinalDir::E);
        for d in CardinalDir::ALL {
            assert_eq!(d.mirror().mirror(), d);
        }
    }

    #[test]
    fn border_stair_positions_lie_inside_map() {
        let w = MAP_SIZE.x as i32;
        let h = MAP_SIZE.y as i32;
        for d in CardinalDir::ALL {
            for p in border_stair_positions(d) {
                assert!(p.x > 0 && p.x < w - 1, "{:?} stair x out of range: {:?}", d, p);
                assert!(p.y > 0 && p.y < h - 1, "{:?} stair y out of range: {:?}", d, p);
            }
        }
    }

    #[test]
    fn border_stair_positions_are_on_the_right_border() {
        let w = MAP_SIZE.x as i32;
        let h = MAP_SIZE.y as i32;
        for p in border_stair_positions(CardinalDir::N) { assert_eq!(p.y, 1); }
        for p in border_stair_positions(CardinalDir::S) { assert_eq!(p.y, h - 2); }
        for p in border_stair_positions(CardinalDir::E) { assert_eq!(p.x, w - 2); }
        for p in border_stair_positions(CardinalDir::W) { assert_eq!(p.x, 1); }
    }

    #[test]
    fn k_th_stair_pairs_with_k_th_mirror_stair_same_axis() {
        // K-th N stair has the same x as K-th S stair (entering S
        // border of destination keeps the player's column).
        for k in 0..STAIRS_PER_BORDER {
            let north = border_stair_positions(CardinalDir::N)[k];
            let south = border_stair_positions(CardinalDir::S)[k];
            assert_eq!(north.x, south.x, "K={}", k);
        }
        // K-th E stair has the same y as K-th W stair.
        for k in 0..STAIRS_PER_BORDER {
            let east = border_stair_positions(CardinalDir::E)[k];
            let west = border_stair_positions(CardinalDir::W)[k];
            assert_eq!(east.y, west.y, "K={}", k);
        }
    }

    #[test]
    fn arrival_at_mirror_lands_on_destinations_k_th_stair() {
        for k in 0..STAIRS_PER_BORDER {
            assert_eq!(
                arrival_at_mirror(CardinalDir::N, k),
                border_stair_positions(CardinalDir::S)[k],
            );
            assert_eq!(
                arrival_at_mirror(CardinalDir::W, k),
                border_stair_positions(CardinalDir::E)[k],
            );
        }
    }

    #[test]
    fn town_has_four_cardinal_exits() {
        let exits = valid_cardinal_exits(0);
        assert_eq!(exits.len(), 4);
        assert!(exits.contains(&CardinalDir::N));
        assert!(exits.contains(&CardinalDir::S));
        assert!(exits.contains(&CardinalDir::E));
        assert!(exits.contains(&CardinalDir::W));
    }

    #[test]
    fn cardinal_forest_has_three_exits() {
        // N forest (floor 2) reaches town (S), NW (W), NE (E).
        let exits = valid_cardinal_exits(forest_index(GridDir::N));
        assert_eq!(exits.len(), 3);
        assert!(exits.contains(&CardinalDir::S));
        assert!(exits.contains(&CardinalDir::W));
        assert!(exits.contains(&CardinalDir::E));
        assert!(!exits.contains(&CardinalDir::N));
    }

    #[test]
    fn corner_forest_has_two_exits() {
        // NW forest reaches N (E) and W (S). No N or W exit.
        let exits = valid_cardinal_exits(forest_index(GridDir::NW));
        assert_eq!(exits.len(), 2);
        assert!(exits.contains(&CardinalDir::E));
        assert!(exits.contains(&CardinalDir::S));
    }

    #[test]
    fn cardinal_neighbor_topology() {
        // From town: cardinal moves go to the 4 cardinal forests.
        assert_eq!(cardinal_neighbor(0, CardinalDir::N), Some(forest_index(GridDir::N)));
        assert_eq!(cardinal_neighbor(0, CardinalDir::S), Some(forest_index(GridDir::S)));
        assert_eq!(cardinal_neighbor(0, CardinalDir::E), Some(forest_index(GridDir::E)));
        assert_eq!(cardinal_neighbor(0, CardinalDir::W), Some(forest_index(GridDir::W)));
        // From N forest: S goes back to town, W to NW, E to NE.
        let n = forest_index(GridDir::N);
        assert_eq!(cardinal_neighbor(n, CardinalDir::S), Some(0));
        assert_eq!(cardinal_neighbor(n, CardinalDir::W), Some(forest_index(GridDir::NW)));
        assert_eq!(cardinal_neighbor(n, CardinalDir::E), Some(forest_index(GridDir::NE)));
        assert_eq!(cardinal_neighbor(n, CardinalDir::N), None);
        // Temple has no cardinal neighbours.
        for d in CardinalDir::ALL {
            assert_eq!(cardinal_neighbor(9, d), None);
        }
    }

    #[test]
    fn floor_theme_for_kind() {
        assert_eq!(FloorTheme::for_floor_kind(FloorKind::Town), FloorTheme::Town);
        assert_eq!(
            FloorTheme::for_floor_kind(FloorKind::Forest(GridDir::N)),
            FloorTheme::Forest,
        );
        assert_eq!(FloorTheme::for_floor_kind(FloorKind::Temple(2)), FloorTheme::Temple);
    }

    #[test]
    fn forest_theme_overrides_wall_and_floor_glyphs() {
        assert_eq!(FloorTheme::Forest.override_glyph(TerrainType::Wall), Some("♣"));
        assert_eq!(FloorTheme::Forest.override_glyph(TerrainType::Floor), Some(","));
    }

    #[test]
    fn dungeon_theme_overrides_nothing() {
        // Default theme must be the legacy look — no overrides at all.
        for terrain in [
            TerrainType::Wall, TerrainType::Floor, TerrainType::DownStairs,
            TerrainType::Portal,
        ] {
            assert_eq!(FloorTheme::Dungeon.override_glyph(terrain), None);
            assert_eq!(FloorTheme::Dungeon.override_fg(terrain), None);
            assert_eq!(FloorTheme::Dungeon.override_bg(terrain), None);
        }
    }

    #[test]
    fn theme_leaves_stairs_alone() {
        // Even in Forest, DownStairs / UpStairs / Portal stay manifest-driven.
        for terrain in [
            TerrainType::DownStairs,
            TerrainType::UpStairs,
            TerrainType::Portal,
            TerrainType::Door,
        ] {
            for theme in [FloorTheme::Forest, FloorTheme::Town, FloorTheme::Temple] {
                assert_eq!(theme.override_glyph(terrain), None);
                assert_eq!(theme.override_bg(terrain), None);
            }
        }
    }
}
