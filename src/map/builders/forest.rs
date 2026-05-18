//! Forest builder — cellular automata over a `Grid<u8>` to carve an
//! organic forest layout, plus an east-west spine for the linear
//! roguelike stair flow.
//!
//! Two clearings are always carved: a western clearing (where the
//! `UpStairs` sits, mirroring the player's east-into-the-forest entry
//! from town) and an eastern clearing (where the `DownStairs` or the
//! `Amulet of Yendor` sits, depending on whether this is the final
//! floor). A 1-tile-wide corridor connects them through the map
//! centre — the player's natural east-bound path through the woods.
//! Cellular-automata trees fill the regions on either side, giving
//! the corridor an organic feel.
//!
//! CA parameters vary by depth so each forest floor has its own
//! character:
//!
//! - Forest 1 (outer woods): sparser trees, larger clearings, brighter.
//! - Forest 2 (deep woods): denser trees, gnarlier, more decay decorations.
//!
//! Connectivity is enforced by keeping only tiles reachable from the
//! west clearing, so the player can never wander into an isolated
//! pocket.

use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

use roguelike_engine::map::builders::algorithms::{
    Grid, cellular_automata_iteration, get_all_regions, randomize_grid,
};

const FLOOR_VAL: u8 = 1;
const WALL_VAL: u8 = 0;

const MIN_REGION_FRACTION: f32 = 0.25;
const MAX_CA_RETRIES: usize = 8;
/// Half-width of each end-clearing (3×3 carve when this is 1).
const END_CLEARING_HALF: i32 = 1;
/// Distance from the map's east/west border to the centre of the
/// end-clearings. 3 keeps the stair one tile inside the border-wall.
const CLEARING_INSET: i32 = 3;

// =====================================================================
// CA profile per forest depth
// =====================================================================

/// Returns `(initial_alive_percent, round_count)` tuned to depth.
/// Forest 1 is sparser (smaller %), Forest 2 is denser.
fn profile_for_depth(depth: i32) -> (i32, usize) {
    match depth {
        // Forest 1 (outer woods) — looser trees, larger clearings.
        1 => (50, 4),
        // Forest 2 (deep woods) — gnarlier, denser canopy.
        2 => (62, 5),
        // Beyond authored content — fall back to mid values.
        _ => (55, 5),
    }
}

// =====================================================================
// ForestTerrainBuilder — cellular automata + east-west spine.
// =====================================================================

pub struct ForestTerrainBuilder {
    pub birth_threshold: i32,
    pub survival_threshold: i32,
}

impl Default for ForestTerrainBuilder {
    fn default() -> Self {
        Self {
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
        let depth = build.map.depth;
        let (initial_alive_percent, round_count) = profile_for_depth(depth);
        let min_region_size = ((w * h) as f32 * MIN_REGION_FRACTION) as usize;

        // East-west spine: west clearing → corridor → east clearing.
        let mid_y = h / 2;
        let west_cx = CLEARING_INSET;
        let east_cx = w - 1 - CLEARING_INSET;

        // 1. Run CA until we get a healthy connected region OR exhaust retries.
        let mut grid: Grid<u8> = Grid::new(w, h, WALL_VAL);
        let mut attempts = 0;
        loop {
            attempts += 1;
            randomize_grid(&mut grid, initial_alive_percent, FLOOR_VAL, WALL_VAL);
            force_border_wall(&mut grid);
            for _ in 0..round_count {
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
                    // Pathological CA fallback: just open the interior.
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

        // 2. Force-carve the two end-clearings and the corridor between
        //    them. This guarantees the spine exists no matter what CA
        //    produced.
        carve_clearing(&mut grid, west_cx, mid_y, END_CLEARING_HALF);
        carve_clearing(&mut grid, east_cx, mid_y, END_CLEARING_HALF);
        carve_corridor(&mut grid, west_cx, east_cx, mid_y);
        force_border_wall(&mut grid);

        // 3. Final connectivity cull from the west clearing centre —
        //    everything not reachable from spawn becomes wall.
        keep_only_reachable_from(&mut grid, west_cx, mid_y);

        // 4. Stamp the grid onto the BuilderMap.
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

        // Starting position = west clearing centre (the UpStairs spot).
        // The player lands here when arriving from the east stair on
        // the previous floor (town or Forest n-1).
        build.set_starting_position(Position { x: west_cx, y: mid_y });
    }
}

fn carve_clearing(grid: &mut Grid<u8>, cx: i32, cy: i32, half: i32) {
    let w = grid.width;
    let h = grid.height;
    for dy in -half..=half {
        for dx in -half..=half {
            let x = cx + dx;
            let y = cy + dy;
            if x <= 0 || y <= 0 || x >= w - 1 || y >= h - 1 { continue; }
            let idx = grid.xy_idx(x, y);
            grid.data[idx] = FLOOR_VAL;
        }
    }
}

fn carve_corridor(grid: &mut Grid<u8>, x_start: i32, x_end: i32, y: i32) {
    let w = grid.width;
    let h = grid.height;
    if y <= 0 || y >= h - 1 { return; }
    let (xa, xb) = if x_start < x_end { (x_start, x_end) } else { (x_end, x_start) };
    for x in xa..=xb {
        if x <= 0 || x >= w - 1 { continue; }
        let idx = grid.xy_idx(x, y);
        grid.data[idx] = FLOOR_VAL;
    }
}

// =====================================================================
// ForestStairsBuilder — stairs at the east + west ends of the spine.
// =====================================================================

pub struct ForestStairsBuilder;

impl ForestStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for ForestStairsBuilder {
    // StructurePlacement (not Finalization): stairs *are* terrain placement,
    // and Spawning must run after stair tiles exist so the spawner can
    // skip them. See docs/design/SPAWNING.md §pipeline ordering.
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::StructurePlacement) }
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let mid_y = h / 2;
        let west_cx = CLEARING_INSET;
        let east_cx = w - 1 - CLEARING_INSET;
        let depth = build.map.depth;

        // West clearing → UpStairs (where the player arrives).
        let up_idx = build.map.xy_idx(west_cx, mid_y);
        build.map.tiles[up_idx].terrain = TerrainType::UpStairs;
        build.map.tiles[up_idx].liquid = LiquidType::None;

        // East clearing → DownStairs (non-final floor) or Amulet (final floor).
        if depth >= crate::constants::MAX_FLOOR {
            build.add_item_spawn(
                Point::new(east_cx, mid_y),
                "Amulet of Yendor".to_string(),
                1,
            );
            bevy::log::info!(
                "ForestStairsBuilder: Amulet at east clearing ({}, {}) on final floor {}",
                east_cx, mid_y, depth,
            );
        } else {
            let down_idx = build.map.xy_idx(east_cx, mid_y);
            build.map.tiles[down_idx].terrain = TerrainType::DownStairs;
            build.map.tiles[down_idx].liquid = LiquidType::None;
            bevy::log::info!(
                "ForestStairsBuilder: DownStairs at east clearing ({}, {}) on floor {}",
                east_cx, mid_y, depth,
            );
        }

        // Belt-and-suspenders: ensure the east clearing is reachable
        // from the west by walking east along the spine. The terrain
        // builder already carved this corridor, but rerun a Dijkstra
        // sanity check; if unreachable, force-carve the corridor again.
        let start_idx = build
            .map
            .point2d_to_index(Point::new(west_cx, mid_y));
        let mut walkable = vec![false; (w * h) as usize];
        for (idx, tile) in build.map.tiles.iter().enumerate() {
            walkable[idx] = !matches!(tile.terrain, TerrainType::Wall);
        }
        let dijkstra = DijkstraMap::new(
            w as usize,
            h as usize,
            &[start_idx],
            &WalkableMaskMap { walkable: &walkable, width: w as usize, height: h as usize },
            1024.0,
        );
        let east_idx = build.map.xy_idx(east_cx, mid_y);
        if dijkstra.map[east_idx] == f32::MAX {
            // Re-carve the spine on the live map. Rare path; just defends
            // against a CA + cull combo that severed the corridor.
            for x in west_cx..=east_cx {
                let idx = build.map.xy_idx(x, mid_y);
                if build.map.tiles[idx].terrain == TerrainType::Wall {
                    build.map.tiles[idx].terrain = TerrainType::Floor;
                }
            }
        }
    }
}

/// Minimal `BaseMap` adapter for the spine-reachability check.
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
// Connectivity helpers
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
    fn forest_starting_position_at_west_clearing() {
        let bm = build_forest(1);
        let start = bm.starting_position.expect("forest must set starting_position");
        assert_eq!(start.x, CLEARING_INSET, "start x must be at west clearing");
        assert_eq!(start.y, bm.height / 2);
        let idx = bm.map.xy_idx(start.x, start.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
    }

    #[test]
    fn forest_stairs_at_west_and_east() {
        let mut bm = build_forest(1); // floor 1, MAX_FLOOR = 2
        ForestStairsBuilder.build_map(&mut bm);
        let west_idx = bm.map.xy_idx(CLEARING_INSET, bm.height / 2);
        let east_idx = bm.map.xy_idx(bm.width - 1 - CLEARING_INSET, bm.height / 2);
        assert_eq!(bm.map.tiles[west_idx].terrain, TerrainType::UpStairs);
        assert_eq!(bm.map.tiles[east_idx].terrain, TerrainType::DownStairs);
    }

    #[test]
    fn forest_final_floor_places_amulet_not_downstairs() {
        let mut bm = build_forest(crate::constants::MAX_FLOOR);
        ForestStairsBuilder.build_map(&mut bm);
        let east_idx = bm.map.xy_idx(bm.width - 1 - CLEARING_INSET, bm.height / 2);
        assert_eq!(bm.map.tiles[east_idx].terrain, TerrainType::Floor,
                   "east clearing must NOT be DownStairs on the final floor");
        let amulet_count = bm
            .item_spawn_list
            .iter()
            .filter(|(_, name, _)| name == "Amulet of Yendor")
            .count();
        assert_eq!(amulet_count, 1, "final floor must spawn the Amulet");
    }

    #[test]
    fn forest_west_to_east_is_walkable() {
        // Sanity: walking east-west along the spine y=h/2 should stay
        // on Floor (or Stairs) tiles all the way across.
        let mut bm = build_forest(1);
        ForestStairsBuilder.build_map(&mut bm);
        let mid_y = bm.height / 2;
        for x in CLEARING_INSET..=(bm.width - 1 - CLEARING_INSET) {
            let idx = bm.map.xy_idx(x, mid_y);
            let terrain = bm.map.tiles[idx].terrain;
            assert!(
                matches!(terrain, TerrainType::Floor | TerrainType::UpStairs | TerrainType::DownStairs),
                "spine tile ({}, {}) must be walkable, got {:?}", x, mid_y, terrain,
            );
        }
    }

    /// Forest 1 and Forest 2 use different CA profiles. We can't
    /// assert *which* is denser on any single seed, but we can at
    /// least check that both depths produce a valid spine.
    #[test]
    fn forest_2_carving_produces_walkable_spine() {
        let mut bm = build_forest(2);
        ForestStairsBuilder.build_map(&mut bm);
        let mid_y = bm.height / 2;
        for x in CLEARING_INSET..=(bm.width - 1 - CLEARING_INSET) {
            let idx = bm.map.xy_idx(x, mid_y);
            let terrain = bm.map.tiles[idx].terrain;
            assert!(
                matches!(terrain, TerrainType::Floor | TerrainType::UpStairs | TerrainType::DownStairs),
                "deep-forest spine tile ({}, {}) must be walkable, got {:?}", x, mid_y, terrain,
            );
        }
    }
}
