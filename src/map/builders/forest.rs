//! Forest builder — cellular automata over a `Grid<u8>` to carve an
//! organic floor / "trees" (Wall terrain rendered as trees by the
//! Forest [`FloorTheme`]) layout, plus simple linear-roguelike stairs.
//!
//! Connectivity is enforced by keeping the largest connected region;
//! the remaining tiles are filled with Wall so the player can't wander
//! into isolated pockets. The UpStairs sits at the centre clearing
//! (where the player lands when descending from the floor above); the
//! DownStairs is placed at the farthest reachable tile from that
//! clearing (unless this is the final floor, in which case
//! `DistantExit` places the Amulet instead).

use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

use roguelike_engine::map::builders::algorithms::{
    Grid, cellular_automata_iteration, get_all_regions, randomize_grid,
};

const FLOOR_VAL: u8 = 1;
const WALL_VAL: u8 = 0;

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

// =====================================================================
// ForestTerrainBuilder — cellular automata trees + centre clearing.
// =====================================================================

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

impl InitialMapBuilder for ForestTerrainBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let min_region_size = ((w * h) as f32 * MIN_REGION_FRACTION) as usize;
        let cx = w / 2;
        let cy = h / 2;

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
                if let Some(region) = largest {
                    for cell in grid.data.iter_mut() { *cell = WALL_VAL; }
                    for idx in region.tiles { grid.data[idx] = FLOOR_VAL; }
                }
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

        // Always carve a small clearing at the map centre — UpStairs
        // lives here, and we never want it pinched off.
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
        ensure_centre_connected(&mut grid, cx, cy);
        keep_only_reachable_from(&mut grid, cx, cy);

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

        build.set_starting_position(Position { x: cx, y: cy });
    }
}

// =====================================================================
// ForestStairsBuilder — places UpStairs at start, DownStairs at farthest.
// =====================================================================

/// Places the forest's stair tiles for the linear floor scheme.
///
/// - **UpStairs** at `starting_position` (the centre clearing). The
///   player lands here when descending from the floor above.
/// - **DownStairs** at the farthest reachable floor tile from the
///   start. Omitted on the final floor (`crate::constants::MAX_FLOOR`)
///   — `AmuletPlacer` / `DistantExit` puts the Amulet there instead.
pub struct ForestStairsBuilder;

impl ForestStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for ForestStairsBuilder {
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Finalization) }
    fn build_map(&mut self, build: &mut BuilderMap) {
        let Some(start) = build.starting_position else {
            bevy::log::warn!("ForestStairsBuilder: starting_position not set; skipping");
            return;
        };

        // UpStairs at the start.
        let up_idx = build.map.xy_idx(start.x, start.y);
        build.map.tiles[up_idx].terrain = TerrainType::UpStairs;
        build.map.tiles[up_idx].liquid = LiquidType::None;

        let depth = build.map.depth;

        let start_idx = build
            .map
            .point2d_to_index(Point::new(start.x, start.y));

        // Walkable mask used by the Dijkstra map — any non-wall tile.
        let width = build.map.width;
        let height = build.map.height;
        let mut walkable = vec![false; (width * height) as usize];
        for (idx, tile) in build.map.tiles.iter().enumerate() {
            walkable[idx] = !matches!(tile.terrain, TerrainType::Wall);
        }

        let dijkstra = DijkstraMap::new(
            width as usize,
            height as usize,
            &[start_idx],
            &WalkableMaskMap { walkable: &walkable, width: width as usize, height: height as usize },
            1024.0,
        );

        let mut best: Option<(usize, f32)> = None;
        for (idx, &d) in dijkstra.map.iter().enumerate() {
            if d == std::f32::MAX || !walkable[idx] { continue; }
            if idx == start_idx { continue; }
            if best.map(|(_, bd)| d > bd).unwrap_or(true) {
                best = Some((idx, d));
            }
        }

        let Some((far_idx, _)) = best else {
            bevy::log::warn!("ForestStairsBuilder: no walkable target for stair / amulet");
            return;
        };
        let far_x = (far_idx % width as usize) as i32;
        let far_y = (far_idx / width as usize) as i32;
        if depth >= crate::constants::MAX_FLOOR {
            // Final floor — place the Amulet of Yendor at the farthest
            // tile, no DownStairs. Player must climb back up to the
            // town Portal.
            build.add_item_spawn(Point::new(far_x, far_y), "Amulet of Yendor".to_string(), 1);
            bevy::log::info!(
                "ForestStairsBuilder: Amulet of Yendor at ({}, {}) on final floor {}",
                far_x, far_y, depth,
            );
        } else {
            build.map.tiles[far_idx].terrain = TerrainType::DownStairs;
            build.map.tiles[far_idx].liquid = LiquidType::None;
            bevy::log::info!(
                "ForestStairsBuilder: DownStairs at ({}, {}) on floor {}",
                far_x, far_y, depth,
            );
        }
    }
}

/// Minimal `BaseMap` adapter for the Dijkstra map computation. A tile
/// is walkable iff its terrain is non-wall.
struct WalkableMaskMap<'a> {
    walkable: &'a [bool],
    width: usize,
    height: usize,
}

impl<'a> bracket_lib::prelude::BaseMap for WalkableMaskMap<'a> {
    fn is_opaque(&self, _idx: usize) -> bool { false }
    fn get_available_exits(&self, idx: usize) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
        let mut exits = bracket_lib::prelude::SmallVec::new();
        let x = (idx % self.width) as i32;
        let y = (idx / self.width) as i32;
        for (dx, dy) in [(0_i32, 1_i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx as usize >= self.width || ny as usize >= self.height {
                continue;
            }
            let nidx = (ny as usize) * self.width + (nx as usize);
            if self.walkable[nidx] {
                exits.push((nidx, 1.0));
            }
        }
        exits
    }
}

impl<'a> Algorithm2D for WalkableMaskMap<'a> {
    fn dimensions(&self) -> Point {
        Point::new(self.width as i32, self.height as i32)
    }
}

// =====================================================================
// Helpers (private to this module)
// =====================================================================

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
    let clearing_cells = ((CENTER_CLEARING * 2 + 1) * (CENTER_CLEARING * 2 + 1)) as usize;
    if reachable_count > clearing_cells {
        return;
    }
    let mut best: Option<(i32, i32, i32)> = None;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let idx = grid.xy_idx(x, y);
            if grid.data[idx] != FLOOR_VAL { continue; }
            if (x - cx).abs() <= CENTER_CLEARING && (y - cy).abs() <= CENTER_CLEARING {
                continue;
            }
            let dist = (x - cx).abs() + (y - cy).abs();
            if best.map(|b| dist < b.0).unwrap_or(true) {
                best = Some((dist, x, y));
            }
        }
    }
    if let Some((_, tx, ty)) = best {
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

    fn build_forest(depth: i32) -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(80, 60);
        bm.map.depth = depth;
        ForestTerrainBuilder::default().build_map(&mut bm);
        bm
    }

    #[test]
    fn forest_sets_starting_position() {
        let bm = build_forest(1);
        let start = bm.starting_position.expect("forest must set starting_position");
        let idx = bm.map.xy_idx(start.x, start.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
    }

    #[test]
    fn forest_largest_region_is_substantial() {
        let bm = build_forest(2);
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
        let largest = regions.iter().map(|r| r.size).max().unwrap_or(0);
        let min_size = ((w * h) as f32 * MIN_REGION_FRACTION * 0.5) as usize;
        assert!(largest >= min_size, "expected at least {min_size}, got {largest}");
    }

    #[test]
    fn forest_centre_clearing_is_walkable() {
        let bm = build_forest(1);
        let cx = bm.width / 2;
        let cy = bm.height / 2;
        let idx = bm.map.xy_idx(cx, cy);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
    }

    #[test]
    fn forest_stairs_stamps_up_stairs_at_start() {
        let mut bm = build_forest(1);
        ForestStairsBuilder.build_map(&mut bm);
        let start = bm.starting_position.expect("start set by terrain builder");
        let idx = bm.map.xy_idx(start.x, start.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::UpStairs);
    }

    #[test]
    fn forest_stairs_stamps_down_stairs_on_non_final_floor() {
        let mut bm = build_forest(1); // floor 1, MAX_FLOOR = 2
        ForestStairsBuilder.build_map(&mut bm);
        let count = bm
            .map
            .tiles
            .iter()
            .filter(|t| t.terrain == TerrainType::DownStairs)
            .count();
        assert_eq!(count, 1, "exactly one DownStairs on non-final floor");
    }

    #[test]
    fn forest_stairs_omits_down_stairs_on_final_floor() {
        let mut bm = build_forest(crate::constants::MAX_FLOOR); // floor 2
        ForestStairsBuilder.build_map(&mut bm);
        let count = bm
            .map
            .tiles
            .iter()
            .filter(|t| t.terrain == TerrainType::DownStairs)
            .count();
        assert_eq!(count, 0, "no DownStairs on final floor");
    }
}
