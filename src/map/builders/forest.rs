//! Forest builder — cellular automata over a `Grid<u8>` to carve an
//! organic floor / "trees" (Wall terrain rendered as trees by the
//! Forest [`FloorTheme`]) layout, plus a return UpStairs to town.
//!
//! Connectivity is enforced by keeping the largest connected region;
//! the remaining tiles are filled with Wall so the player can't wander
//! into isolated pockets via diagonal bouncing. The UpStairs is placed
//! on the floor's `starting_position`, which is also where the player
//! lands when descending from town.

use bracket_lib::prelude::Point;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile, is_walkable};
use crate::map::world::{
    CardinalDir, arrival_at_mirror, border_stair_positions, cardinal_neighbor,
    valid_cardinal_exits,
};

use roguelike_engine::map::builders::algorithms::{
    Grid, cellular_automata_iteration, get_all_regions, randomize_grid,
};

const FLOOR_VAL: u8 = 1;
const WALL_VAL: u8 = 0;

/// Cellular-automata forest. Trees are `TerrainType::Wall`; the renderer
/// applies the Forest theme to draw them as `♣`.
pub struct ForestTerrainBuilder {
    pub initial_alive_percent: i32,
    pub round_count: usize,
    pub birth_threshold: i32,
    pub survival_threshold: i32,
}

impl Default for ForestTerrainBuilder {
    fn default() -> Self {
        Self {
            initial_alive_percent: 55,
            round_count: 5,
            birth_threshold: 5,
            survival_threshold: 4,
        }
    }
}

impl ForestTerrainBuilder {
    pub fn new() -> Box<Self> { Box::new(Self::default()) }
}

/// Stamps the forest's border stair tiles — 4 per valid cardinal
/// direction. Stairs going back to town render as `<`, stairs going
/// to lateral forest neighbours render as `>`. Each stair is paired
/// with the K-th stair on the destination's mirror border for
/// continuous positioning.
///
/// Tunnels a 1-tile-wide path from each stair into the forest centre
/// so the player can actually walk between the border and the
/// interior (the CA leaves the border solid wall).
pub struct ForestBorderStairsBuilder;

impl ForestBorderStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for ForestBorderStairsBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::StructurePlacement)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let floor = build.map.depth as u32;
        let Some(start) = build.starting_position else {
            bevy::log::warn!("ForestBorderStairsBuilder: no starting_position; skipping");
            return;
        };
        for dir in valid_cardinal_exits(floor) {
            let Some(dest_floor) = cardinal_neighbor(floor, dir) else { continue };
            // Town-bound stairs render as `<`; lateral forest stairs as `>`.
            let terrain = if dest_floor == 0 {
                TerrainType::UpStairs
            } else {
                TerrainType::DownStairs
            };
            for (k, pos) in border_stair_positions(dir).into_iter().enumerate() {
                let idx = build.map.xy_idx(pos.x, pos.y);
                build.map.tiles[idx] = Tile {
                    terrain,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
                build.add_exit_tile(
                    Point::new(pos.x, pos.y),
                    dest_floor,
                    Some(arrival_at_mirror(dir, k)),
                );
                carve_tunnel(build, pos, start);
            }
        }
    }
}

/// Carve a 1-tile-wide L-corridor of `Floor` from `from` toward `to`,
/// stopping when we hit an already-walkable tile (so the tunnel
/// merges into the forest body without leaving long straight scars).
/// Stops at existing `MapExitTile` positions too — never tunnels
/// through another stair.
fn carve_tunnel(build: &mut BuilderMap, from: Position, to: Position) {
    let stamp = |build: &mut BuilderMap, x: i32, y: i32| -> bool {
        if x <= 0 || y <= 0 || x >= build.width - 1 || y >= build.height - 1 {
            return false;
        }
        let idx = build.map.xy_idx(x, y);
        let tile = build.map.tiles[idx];
        // Already a walkable interior tile — tunnel has merged with the forest.
        if tile.terrain == TerrainType::Floor {
            return true;
        }
        // Don't overwrite stair tiles.
        if matches!(tile.terrain, TerrainType::DownStairs | TerrainType::UpStairs) {
            return false;
        }
        build.map.tiles[idx] = Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
        false
    };

    // Horizontal leg first.
    let (xa, xb) = if from.x < to.x { (from.x, to.x) } else { (to.x, from.x) };
    if from.x < to.x {
        for x in xa..=xb {
            if stamp(build, x, from.y) { return; }
        }
    } else {
        for x in (xa..=xb).rev() {
            if stamp(build, x, from.y) { return; }
        }
    }
    // Vertical leg.
    let (ya, yb) = if from.y < to.y { (from.y, to.y) } else { (to.y, from.y) };
    if from.y < to.y {
        for y in ya..=yb {
            if stamp(build, to.x, y) { return; }
        }
    } else {
        for y in (ya..=yb).rev() {
            if stamp(build, to.x, y) { return; }
        }
    }
}

/// Adds the temple entrance to a forest floor: places `DownStairs` on
/// a walkable cell far from the forest's UpStairs (so the player has
/// to explore to find it), and stamps a `MapExitTile` pointing to
/// temple floor 1.
pub struct TempleEntranceBuilder;

impl TempleEntranceBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TempleEntranceBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        // Runs after ForestUpStairsBuilder so we can pick a tile away
        // from the UpStairs.
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let Some(start) = build.starting_position else {
            bevy::log::warn!("TempleEntranceBuilder: no starting_position; skipping");
            return;
        };
        let target = farthest_interior_walkable(build, start.x, start.y);
        if target.x == start.x && target.y == start.y {
            bevy::log::warn!("TempleEntranceBuilder: no interior tile available; skipping");
            return;
        }
        let idx = build.map.xy_idx(target.x, target.y);
        build.map.tiles[idx] = Tile {
            terrain: TerrainType::DownStairs,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
        // Temple floor 1 = floor index 9.
        build.add_exit_tile(target, 9, None);
    }
}

/// Manhattan-distance scan for the walkable tile farthest from
/// `(sx, sy)`, restricted to the interior so we don't pick a border
/// stair tile. Mirrors the helper in `amulet_placer.rs` — kept local
/// to avoid leaking a module-private helper across files.
fn farthest_interior_walkable(build: &BuilderMap, sx: i32, sy: i32) -> Point {
    let mut best = Point::new(sx, sy);
    let mut best_dist: i32 = -1;
    let margin = 4; // stay clear of border stairs
    for y in margin..(build.height - margin) {
        for x in margin..(build.width - margin) {
            let idx = build.map.xy_idx(x, y);
            let tile = build.map.tiles[idx];
            if !is_walkable(tile) { continue; }
            // Skip any tile that already has a stair on it.
            if matches!(tile.terrain, TerrainType::DownStairs | TerrainType::UpStairs) {
                continue;
            }
            let dist = (x - sx).abs() + (y - sy).abs();
            if dist > best_dist {
                best_dist = dist;
                best = Point::new(x, y);
            }
        }
    }
    best
}

/// If the largest connected region after CA covers less than this
/// fraction of the playable area, the builder retries with a fresh
/// random seed. Stops the "1×1 forest" failure mode from shipping.
const MIN_REGION_FRACTION: f32 = 0.25;
/// Maximum CA attempts before falling back to a fully-open interior
/// (guarantees the player can always move even on pathological seeds).
const MAX_CA_RETRIES: usize = 8;
/// Half-width of the always-clear patch at the map centre, where the
/// forest UpStairs gets placed.
const CENTER_CLEARING: i32 = 2;

impl InitialMapBuilder for ForestTerrainBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let min_region_size = ((w * h) as f32 * MIN_REGION_FRACTION) as usize;
        let cx = w / 2;
        let cy = h / 2;

        // Retry CA until the largest connected region is big enough.
        let mut grid: Grid<u8> = Grid::new(w, h, WALL_VAL);
        let mut attempts = 0;
        loop {
            attempts += 1;
            randomize_grid(&mut grid, self.initial_alive_percent, FLOOR_VAL, WALL_VAL);
            force_border_wall(&mut grid);
            for _ in 0..self.round_count {
                cellular_automata_iteration(
                    &mut grid,
                    self.birth_threshold,
                    self.survival_threshold,
                    FLOOR_VAL,
                    WALL_VAL,
                );
            }

            let regions = get_all_regions(&grid, FLOOR_VAL, WALL_VAL);
            let largest = regions.into_iter().max_by_key(|r| r.size);
            let biggest_size = largest.as_ref().map(|r| r.size).unwrap_or(0);

            if biggest_size >= min_region_size || attempts >= MAX_CA_RETRIES {
                // Commit: every cell becomes wall, then the largest
                // region is restored as floor.
                if let Some(region) = largest {
                    for cell in grid.data.iter_mut() { *cell = WALL_VAL; }
                    for idx in region.tiles { grid.data[idx] = FLOOR_VAL; }
                }
                // Pathological seed safety net — if even the largest
                // region is too small, open the interior so the
                // player can still play.
                if biggest_size < min_region_size {
                    for y in 1..h - 1 {
                        for x in 1..w - 1 {
                            let idx = grid.xy_idx(x, y);
                            grid.data[idx] = FLOOR_VAL;
                        }
                    }
                }
                break;
            }
        }

        // Always carve a small clearing at the map centre — the forest
        // UpStairs lives here, and we never want it pinched off.
        for dy in -CENTER_CLEARING..=CENTER_CLEARING {
            for dx in -CENTER_CLEARING..=CENTER_CLEARING {
                let x = cx + dx;
                let y = cy + dy;
                if x <= 0 || y <= 0 || x >= w - 1 || y >= h - 1 { continue; }
                let idx = grid.xy_idx(x, y);
                grid.data[idx] = FLOOR_VAL;
            }
        }
        force_border_wall(&mut grid);

        // If the centre clearing turned out to be its own pocket
        // (disconnected from the largest region), tunnel a 1-tile
        // path from centre to the nearest floor cell in the rest of
        // the map. Guarantees the player at spawn can reach the
        // forest body.
        ensure_centre_connected(&mut grid, cx, cy);
        // Final cull: anything not reachable from the centre becomes
        // wall. The player can never walk into a dead pocket.
        keep_only_reachable_from(&mut grid, cx, cy);

        // Stamp into the BuilderMap.
        for y in 0..h {
            for x in 0..w {
                let gidx = grid.xy_idx(x, y);
                let midx = build.map.xy_idx(x, y);
                let terrain = if grid.data[gidx] == FLOOR_VAL {
                    TerrainType::Floor
                } else {
                    TerrainType::Wall
                };
                build.map.tiles[midx] = Tile {
                    terrain,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }

        // Starting position is the centre clearing — guaranteed walkable.
        build.set_starting_position(Position { x: cx, y: cy });
    }
}

fn force_border_wall(grid: &mut Grid<u8>) {
    let w = grid.width;
    let h = grid.height;
    for x in 0..w {
        let top = grid.xy_idx(x, 0);
        grid.data[top] = WALL_VAL;
        let bot = grid.xy_idx(x, h - 1);
        grid.data[bot] = WALL_VAL;
    }
    for y in 0..h {
        let left = grid.xy_idx(0, y);
        grid.data[left] = WALL_VAL;
        let right = grid.xy_idx(w - 1, y);
        grid.data[right] = WALL_VAL;
    }
}

/// BFS from `(cx, cy)` over walkable cells; if no other floor cell is
/// already reachable, tunnel a 1-tile-wide L-shaped path to the
/// nearest floor cell elsewhere on the grid. Keeps the centre
/// clearing from becoming an isolated pocket on degenerate CA outputs.
fn ensure_centre_connected(grid: &mut Grid<u8>, cx: i32, cy: i32) {
    let w = grid.width;
    let h = grid.height;
    let total = (w * h) as usize;
    let mut visited = vec![false; total];
    let mut queue = std::collections::VecDeque::new();
    let start_idx = grid.xy_idx(cx, cy);
    visited[start_idx] = true;
    queue.push_back((cx, cy));
    let mut reachable_count = 0;
    while let Some((x, y)) = queue.pop_front() {
        let idx = grid.xy_idx(x, y);
        if grid.data[idx] != FLOOR_VAL { continue; }
        reachable_count += 1;
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
            let nidx = grid.xy_idx(nx, ny);
            if visited[nidx] { continue; }
            visited[nidx] = true;
            if grid.data[nidx] == FLOOR_VAL {
                queue.push_back((nx, ny));
            }
        }
    }
    // If the centre's connected floor region already covers more than
    // just the clearing, we're fine — there's a real forest to walk in.
    let clearing_cells = ((CENTER_CLEARING * 2 + 1) * (CENTER_CLEARING * 2 + 1)) as usize;
    if reachable_count > clearing_cells {
        return;
    }

    // Otherwise: scan for the nearest floor cell outside the clearing
    // and tunnel toward it. Manhattan distance is fine — we just want
    // to connect somewhere.
    let mut best: Option<(i32, i32, i32)> = None; // (dist, x, y)
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let idx = grid.xy_idx(x, y);
            if grid.data[idx] != FLOOR_VAL { continue; }
            if (x - cx).abs() <= CENTER_CLEARING && (y - cy).abs() <= CENTER_CLEARING {
                continue; // skip the clearing itself
            }
            let dist = (x - cx).abs() + (y - cy).abs();
            if best.map(|b| dist < b.0).unwrap_or(true) {
                best = Some((dist, x, y));
            }
        }
    }
    if let Some((_, tx, ty)) = best {
        // Horizontal leg then vertical (matches the L-corridor pattern
        // used throughout the codebase).
        let (xa, xb) = if cx < tx { (cx, tx) } else { (tx, cx) };
        for x in xa..=xb {
            if x <= 0 || x >= w - 1 { continue; }
            let idx = grid.xy_idx(x, cy);
            grid.data[idx] = FLOOR_VAL;
        }
        let (ya, yb) = if cy < ty { (cy, ty) } else { (ty, cy) };
        for y in ya..=yb {
            if y <= 0 || y >= h - 1 { continue; }
            let idx = grid.xy_idx(tx, y);
            grid.data[idx] = FLOOR_VAL;
        }
    }
}

/// Final BFS from `(cx, cy)` over walkable cells — anything not
/// reachable becomes wall so the player can never walk into a dead
/// pocket disconnected from spawn.
fn keep_only_reachable_from(grid: &mut Grid<u8>, cx: i32, cy: i32) {
    let w = grid.width;
    let h = grid.height;
    let total = (w * h) as usize;
    let mut reachable = vec![false; total];
    let mut queue = std::collections::VecDeque::new();
    let start_idx = grid.xy_idx(cx, cy);
    if grid.data[start_idx] != FLOOR_VAL { return; }
    reachable[start_idx] = true;
    queue.push_back((cx, cy));
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
            let nidx = grid.xy_idx(nx, ny);
            if reachable[nidx] { continue; }
            if grid.data[nidx] == FLOOR_VAL {
                reachable[nidx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    for (idx, cell) in grid.data.iter_mut().enumerate() {
        if *cell == FLOOR_VAL && !reachable[idx] {
            *cell = WALL_VAL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builders::MetaMapBuilder;
    use crate::map::world::{GridDir, forest_index};

    fn build_forest(floor: u32) -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(80, 60);
        bm.map.depth = floor as i32;
        ForestTerrainBuilder::default().build_map(&mut bm);
        bm
    }

    #[test]
    fn forest_sets_starting_position() {
        let bm = build_forest(forest_index(GridDir::N));
        let start = bm.starting_position.expect("forest must set starting_position");
        let idx = bm.map.xy_idx(start.x, start.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
    }

    #[test]
    fn forest_produces_a_connected_floor_region() {
        let bm = build_forest(forest_index(GridDir::E));
        // Build a Grid<u8> from the resulting map and assert single
        // connected floor region.
        let w = bm.width;
        let h = bm.height;
        let mut grid: Grid<u8> = Grid::new(w, h, WALL_VAL);
        for y in 0..h {
            for x in 0..w {
                let midx = bm.map.xy_idx(x, y);
                let gidx = grid.xy_idx(x, y);
                grid.data[gidx] = if bm.map.tiles[midx].terrain == TerrainType::Floor {
                    FLOOR_VAL
                } else {
                    WALL_VAL
                };
            }
        }
        let regions = get_all_regions(&grid, FLOOR_VAL, WALL_VAL);
        assert_eq!(regions.len(), 1, "forest must end up with one connected floor region; got {}", regions.len());
    }

    #[test]
    fn forest_border_stairs_uses_upstairs_back_to_town() {
        // N forest (floor 2) has a south border that leads back to town.
        let mut bm = build_forest(forest_index(GridDir::N));
        ForestBorderStairsBuilder.build_map(&mut bm);
        // Inspect the south-border stair cluster (4 tiles).
        let south_positions = crate::map::world::border_stair_positions(
            crate::map::world::CardinalDir::S,
        );
        for stair in south_positions {
            let idx = bm.map.xy_idx(stair.x, stair.y);
            assert_eq!(
                bm.map.tiles[idx].terrain,
                TerrainType::UpStairs,
                "south-border stair at ({}, {}) should be UpStairs (back to town)",
                stair.x, stair.y,
            );
        }
        // West border on N forest goes to NW forest (a lateral move),
        // so those stairs are DownStairs (`>`).
        let west_positions = crate::map::world::border_stair_positions(
            crate::map::world::CardinalDir::W,
        );
        for stair in west_positions {
            let idx = bm.map.xy_idx(stair.x, stair.y);
            assert_eq!(
                bm.map.tiles[idx].terrain,
                TerrainType::DownStairs,
                "west-border stair at ({}, {}) should be DownStairs (to NW forest)",
                stair.x, stair.y,
            );
        }
    }

    #[test]
    fn forest_border_stairs_pair_with_mirror_k_th_position() {
        let mut bm = build_forest(forest_index(GridDir::N));
        ForestBorderStairsBuilder.build_map(&mut bm);
        // South border: K-th stair points to town with arrival at K-th
        // position on town's N border (same x, y=1).
        let south = crate::map::world::border_stair_positions(crate::map::world::CardinalDir::S);
        let town_n = crate::map::world::border_stair_positions(crate::map::world::CardinalDir::N);
        for (k, stair) in south.iter().enumerate() {
            let entry = bm.exit_tile_spawn_list.iter()
                .find(|(pt, _)| pt.x == stair.x && pt.y == stair.y)
                .expect("south stair has an exit-tile entry");
            assert_eq!(entry.1.destination_floor, 0);
            assert_eq!(entry.1.destination_pos, Some(town_n[k]));
        }
    }

    #[test]
    fn temple_entrance_lands_on_interior_walkable_tile() {
        let mut bm = build_forest(forest_index(GridDir::SW));
        ForestBorderStairsBuilder.build_map(&mut bm);
        let before_count = bm.exit_tile_spawn_list.len();
        TempleEntranceBuilder.build_map(&mut bm);
        // Exactly one new exit tile added.
        assert_eq!(bm.exit_tile_spawn_list.len(), before_count + 1);
        let (pt, exit) = *bm.exit_tile_spawn_list.last().unwrap();
        // Not on the border (4-tile margin).
        assert!(pt.x >= 4 && pt.x < bm.width - 4);
        assert!(pt.y >= 4 && pt.y < bm.height - 4);
        let idx = bm.map.xy_idx(pt.x, pt.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::DownStairs);
        assert_eq!(exit.destination_floor, 9);
        assert!(exit.destination_pos.is_none());
    }

    #[test]
    fn forest_centre_clearing_is_walkable() {
        // Regression guard for the "1×1 forest" bug: even on degenerate
        // CA seeds, the centre 5×5 around the start must be open floor.
        for _ in 0..10 {
            let bm = build_forest(forest_index(GridDir::N));
            let start = bm.starting_position.unwrap();
            for dy in -CENTER_CLEARING..=CENTER_CLEARING {
                for dx in -CENTER_CLEARING..=CENTER_CLEARING {
                    let x = start.x + dx;
                    let y = start.y + dy;
                    if x <= 0 || y <= 0 || x >= bm.width - 1 || y >= bm.height - 1 {
                        continue;
                    }
                    let idx = bm.map.xy_idx(x, y);
                    assert_eq!(
                        bm.map.tiles[idx].terrain,
                        TerrainType::Floor,
                        "centre clearing tile ({}, {}) must be Floor", x, y,
                    );
                }
            }
        }
    }

    #[test]
    fn forest_largest_region_is_substantial() {
        // The forest the player walks in must cover more than a token
        // patch — at minimum the centre clearing plus the rest of the
        // CA growth (or the fallback fully-open interior).
        for _ in 0..10 {
            let bm = build_forest(forest_index(GridDir::SE));
            let floor_count: usize = bm.map.tiles.iter()
                .filter(|t| t.terrain == TerrainType::Floor)
                .count();
            // Centre clearing alone is (2*2+1)^2 = 25 cells. Anything
            // around that means the connectivity guarantees fired but
            // produced a tiny map — fail loudly.
            assert!(
                floor_count >= 200,
                "forest had only {} floor tiles — CA collapsed without recovery",
                floor_count,
            );
        }
    }

    #[test]
    fn forest_outer_ring_is_wall() {
        // Edge tiles get carved by MapEdgeBuilder later — but the raw
        // forest must enforce a wall border so the player can't fall
        // off the map before the edge builder runs.
        let bm = build_forest(forest_index(GridDir::W));
        let w = bm.width;
        let h = bm.height;
        for x in 0..w {
            let top = bm.map.xy_idx(x, 0);
            let bot = bm.map.xy_idx(x, h - 1);
            assert_eq!(bm.map.tiles[top].terrain, TerrainType::Wall);
            assert_eq!(bm.map.tiles[bot].terrain, TerrainType::Wall);
        }
        for y in 0..h {
            let left = bm.map.xy_idx(0, y);
            let right = bm.map.xy_idx(w - 1, y);
            assert_eq!(bm.map.tiles[left].terrain, TerrainType::Wall);
            assert_eq!(bm.map.tiles[right].terrain, TerrainType::Wall);
        }
    }
}
