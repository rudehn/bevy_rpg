//! Lake generation matching BrogueCE's Architect.c: designLakes → fillLakes → createWreath.
//!
//! Phase 1 (designLakes): Generate blob shapes on a full-size grid, try random placements,
//! validate connectivity hypothetically, stamp as Floor on the DUNGEON layer, mark in lakeMap.
//!
//! Phase 2 (fillLakes): Iterate lakeMap. For each unfilled lake tile, pick a liquid type,
//! flood-fill within scanWidth=4 to merge nearby blobs, then create a circular wreath.

use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, LiquidType, Tile};
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::prelude::{Point, Algorithm2D};
use rand::prelude::*;
use std::collections::{HashSet, VecDeque};

/// Brogue's fillLake scanWidth — lake tiles within this radius merge into the same liquid.
const LAKE_SCAN_WIDTH: i32 = 4;

/// Maximum random placement attempts per lake size.
const MAX_PLACEMENT_ATTEMPTS: i32 = 20;

pub struct LakeBuilder {
    depth: i32,
}

impl LakeBuilder {
    pub fn new(depth: i32) -> Box<Self> {
        Box::new(Self { depth })
    }

    /// Pick liquid type based on depth (Brogue's liquidType function).
    /// Brogue: below minimumLavaLevel → no lava; below minimumBrimstoneLevel → no brimstone.
    /// We simplify: early = water, mid = water/lava, deep = water/lava/chasm.
    fn pick_liquid_type(&self, rng: &mut impl Rng) -> LiquidType {
        let roll: f32 = rng.random();
        match self.depth {
            1..=5 => LiquidType::Water,
            6..=10 => {
                if roll < 0.60 { LiquidType::Water }
                else { LiquidType::Lava }
            }
            _ => {
                if roll < 0.20 { LiquidType::Water }
                else if roll < 0.70 { LiquidType::Lava }
                else { LiquidType::Chasm }
            }
        }
    }
}

// ─── designLakes ────────────────────────────────────────────────────────────

/// Generate a lake blob on a full-size grid matching Brogue's createBlobOnGrid call.
/// Returns the blob grid and bounding box (blob_x, blob_y, blob_w, blob_h).
fn generate_blob_on_full_grid(
    map_width: i32,
    map_height: i32,
    max_blob_width: i32,
    max_blob_height: i32,
) -> (Grid<BlobType>, i32, i32, i32, i32) {
    // Brogue: createBlobOnGrid(grid, &lakeX, &lakeY, &lakeWidth, &lakeHeight,
    //                          5, 4, 4, lakeMaxWidth, lakeMaxHeight, 55,
    //                          "ffffftttt", "ffffttttt")
    let config = BlobGenConfig {
        round_count: 5,
        min_blob_width: 4,
        min_blob_height: 4,
        max_blob_width: max_blob_width,
        max_blob_height: max_blob_height,
        initial_alive_percent: 55,
        birth_threshold: 5,
        survival_threshold: 4,
    };

    let initial_grid = Grid::new(map_width, map_height, BlobType::Wall);
    create_blob(&initial_grid, &config, BlobType::Floor, BlobType::Wall)
}

/// Brogue's lakeDisruptsPassability: checks whether placing the proposed lake
/// (grid at dungeon offset) would disconnect any passable non-lake tile.
///
/// - `map`: the current dungeon (UNMODIFIED — this is a hypothetical check)
/// - `grid`: the blob grid (blob tiles = BlobType::Floor)
/// - `lake_map`: already-committed lake positions from previous iterations
/// - `grid_to_dungeon_x/y`: offset to convert grid coords to dungeon coords
///   (Brogue passes dungeonToGridX/Y which is the NEGATIVE of this)
/// - `blob_x/y/w/h`: bounding box of the blob within the grid
fn lake_disrupts_passability(
    map: &crate::map::map::Map,
    grid: &Grid<BlobType>,
    lake_map: &[bool],
    grid_to_dungeon_x: i32,
    grid_to_dungeon_y: i32,
    blob_x: i32,
    blob_y: i32,
    blob_w: i32,
    blob_h: i32,
) -> bool {
    use crate::map::tile::is_passable;

    let map_w = map.width;
    let map_h = map.height;

    // Helper: would this dungeon tile be covered by the proposed lake?
    let is_proposed_lake = |dx: i32, dy: i32| -> bool {
        let gx = dx - grid_to_dungeon_x;
        let gy = dy - grid_to_dungeon_y;
        if gx >= 0 && gx < grid.width && gy >= 0 && gy < grid.height {
            grid.data[grid.xy_idx(gx, gy)] == BlobType::Floor
        } else {
            false
        }
    };

    // Find first passable tile that is NOT a lake tile and NOT the proposed blob
    let mut start = None;
    for j in 0..map_h {
        for i in 0..map_w {
            let idx = map.xy_idx(i, j);
            if is_passable(map.tiles[idx]) && !lake_map[idx] && !is_proposed_lake(i, j) {
                start = Some((i, j));
                break;
            }
        }
        if start.is_some() { break; }
    }

    let Some((sx, sy)) = start else {
        return false; // no passable tiles at all — trivially connected
    };

    // Flood-fill from start through passable non-lake non-blob tiles (4-directional)
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let start_idx = map.xy_idx(sx, sy);
    visited.insert(start_idx);
    queue.push_back((sx, sy));

    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= map_w || ny >= map_h { continue; }
            let n_idx = map.xy_idx(nx, ny);
            if visited.contains(&n_idx) { continue; }
            if !is_passable(map.tiles[n_idx]) { continue; }
            if lake_map[n_idx] { continue; }
            if is_proposed_lake(nx, ny) { continue; }
            visited.insert(n_idx);
            queue.push_back((nx, ny));
        }
    }

    // Check: any passable non-lake non-blob tile that wasn't reached?
    for j in 0..map_h {
        for i in 0..map_w {
            let idx = map.xy_idx(i, j);
            if is_passable(map.tiles[idx]) && !lake_map[idx] && !is_proposed_lake(i, j) && !visited.contains(&idx) {
                return true; // disconnected
            }
        }
    }
    false
}

/// Brogue's designLakes: generate blob shapes, try placements, validate connectivity,
/// stamp as Floor terrain, mark in lakeMap.
fn design_lakes(build_data: &mut BuilderMap, lake_map: &mut Vec<bool>) {
    let map_w = build_data.map.width;
    let map_h = build_data.map.height;

    let start_pos = match &build_data.starting_position {
        Some(p) => *p,
        None => return,
    };
    let _start_idx = build_data.map.xy_idx(start_pos.x, start_pos.y);

    // Brogue: for (lakeMaxHeight = 15, lakeMaxWidth = 30; lakeMaxHeight >= 10; lakeMaxHeight--, lakeMaxWidth -= 2)
    let mut lake_max_height = 15i32;
    let mut lake_max_width = 30i32;

    while lake_max_height >= 10 {
        // Generate ONE blob on a full-size grid
        let (grid, blob_x, blob_y, blob_w, blob_h) =
            generate_blob_on_full_grid(map_w, map_h, lake_max_width, lake_max_height);

        // Try up to 20 random placements
        let mut rng = rand::rng();
        for _ in 0..MAX_PLACEMENT_ATTEMPTS {
            // Brogue: x = rand_range(1 - lakeX, DCOLS - lakeWidth - lakeX - 2)
            // This is the offset to convert grid coords to dungeon coords.
            let x_min = 1 - blob_x;
            let x_max = map_w - blob_w - blob_x - 2;
            let y_min = 1 - blob_y;
            let y_max = map_h - blob_h - blob_y - 2;

            if x_min > x_max || y_min > y_max { continue; }

            let offset_x = rng.random_range(x_min..=x_max);
            let offset_y = rng.random_range(y_min..=y_max);

            // Check connectivity hypothetically (don't modify the map)
            if lake_disrupts_passability(
                &build_data.map, &grid, lake_map,
                offset_x, offset_y,
                blob_x, blob_y, blob_w, blob_h,
            ) {
                continue;
            }

            // Passed! Copy lake into lakeMap and stamp DUNGEON = FLOOR.
            // Brogue iterates (i in 0..lakeWidth, j in 0..lakeHeight)
            for j in 0..blob_h {
                for i in 0..blob_w {
                    let gx = i + blob_x;
                    let gy = j + blob_y;
                    if gx < 0 || gy < 0 || gx >= grid.width || gy >= grid.height { continue; }
                    if grid.data[grid.xy_idx(gx, gy)] != BlobType::Floor { continue; }

                    let dx = gx + offset_x;
                    let dy = gy + offset_y;
                    if dx < 1 || dy < 1 || dx >= map_w - 1 || dy >= map_h - 1 { continue; }

                    let idx = build_data.map.xy_idx(dx, dy);
                    // Brogue: pmap[...].layers[DUNGEON] = FLOOR (overwrites everything)
                    // We protect stairs only.
                    let terrain = build_data.map.tiles[idx].terrain;
                    if terrain == TerrainType::DownStairs || terrain == TerrainType::UpStairs {
                        continue;
                    }
                    lake_map[idx] = true;
                    build_data.map.tiles[idx].terrain = TerrainType::Floor;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                }
            }
            break; // lake placed successfully
        }

        lake_max_height -= 1;
        lake_max_width -= 2;
    }
}

// ─── fillLakes ──────────────────────────────────────────────────────────────

/// Brogue's fillLake: iterative flood-fill from a lake tile, scanning within
/// LAKE_SCAN_WIDTH to merge nearby lake tiles into the same liquid.
/// Returns the set of filled tile indices (for wreath generation).
fn fill_lake(
    build_data: &mut BuilderMap,
    lake_map: &mut Vec<bool>,
    start_x: i32,
    start_y: i32,
    liquid: LiquidType,
) -> HashSet<usize> {
    let width = build_data.map.width;
    let height = build_data.map.height;
    let mut wreath_set = HashSet::new();
    let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
    queue.push_back((start_x, start_y));

    while let Some((x, y)) = queue.pop_front() {
        for dy in -LAKE_SCAN_WIDTH..=LAKE_SCAN_WIDTH {
            for dx in -LAKE_SCAN_WIDTH..=LAKE_SCAN_WIDTH {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                let idx = build_data.map.xy_idx(nx, ny);
                if lake_map[idx] {
                    lake_map[idx] = false;
                    build_data.map.tiles[idx].liquid = liquid;
                    wreath_set.insert(idx);
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    wreath_set
}

/// Brogue's createWreath: place shallow liquid in a circular radius around each
/// deep lake tile. Uses Euclidean distance. Converts doors to floor.
fn create_wreath(
    build_data: &mut BuilderMap,
    wreath_set: &HashSet<usize>,
    shallow_liquid: LiquidType,
    wreath_width: i32,
) {
    if wreath_width == 0 { return; }

    let width = build_data.map.width;
    let height = build_data.map.height;
    let mut wreath_tiles = Vec::new();

    for &lake_idx in wreath_set {
        let (lx, ly) = build_data.map.idx_xy(lake_idx);
        for dy in -wreath_width..=wreath_width {
            for dx in -wreath_width..=wreath_width {
                let nx = lx + dx;
                let ny = ly + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                // Euclidean distance check (circular wreath)
                if dx * dx + dy * dy > wreath_width * wreath_width { continue; }

                let n_idx = build_data.map.xy_idx(nx, ny);
                // Brogue: only place wreath if LIQUID layer is NOTHING
                if build_data.map.tiles[n_idx].liquid != LiquidType::None { continue; }

                wreath_tiles.push(n_idx);
            }
        }
    }

    let unique_wreath: HashSet<usize> = wreath_tiles.into_iter().collect();
    for idx in unique_wreath {
        build_data.map.tiles[idx].liquid = shallow_liquid;
        // Brogue: if pmap[k][l].layers[DUNGEON] == DOOR → FLOOR
        if build_data.map.tiles[idx].terrain == TerrainType::Door
            || build_data.map.tiles[idx].terrain == TerrainType::OpenDoor
        {
            build_data.map.tiles[idx].terrain = TerrainType::Floor;
        }
    }
}

/// Brogue's fillLakes: iterate lakeMap, for each unfilled lake tile pick a liquid
/// type, flood-fill connected lake tiles, create wreath.
fn fill_lakes(build_data: &mut BuilderMap, lake_map: &mut Vec<bool>, depth: i32) {
    let width = build_data.map.width;
    let height = build_data.map.height;
    let mut rng = rand::rng();

    for y in 0..height {
        for x in 0..width {
            let idx = build_data.map.xy_idx(x, y);
            if lake_map[idx] {
                // Pick liquid type for this connected lake group
                let builder = LakeBuilder { depth };
                let liquid = builder.pick_liquid_type(&mut rng);

                // Brogue's wreath parameters per liquid type:
                // Water: shallow=SHALLOW_WATER, width=2
                // Lava: shallow=NOTHING, width=0
                // Chasm: shallow=CHASM_EDGE, width=1
                // Brimstone: shallow=OBSIDIAN, width=2
                let (shallow, wreath_width) = match liquid {
                    LiquidType::Water => (LiquidType::ShallowWater, 2),
                    LiquidType::Lava => (LiquidType::None, 0),
                    LiquidType::Chasm => (LiquidType::None, 0), // TODO: chasm edge
                    _ => (LiquidType::None, 0),
                };

                let wreath_set = fill_lake(build_data, lake_map, x, y, liquid);
                create_wreath(build_data, &wreath_set, shallow, wreath_width);
            }
        }
    }
}

// ─── cleanUpLakeBoundaries ──────────────────────────────────────────────────

/// Brogue's cleanUpLakeBoundaries: merge thin walls sandwiched between same-type
/// lake tiles. If a wall/blocking tile has the same lake type on opposite sides
/// (horizontal or vertical), it gets replaced with that lake type.
fn clean_up_lake_boundaries(build_data: &mut BuilderMap) {
    let width = build_data.map.width;
    let height = build_data.map.height;

    let mut changed = true;
    let mut reverse = true;
    let mut failsafe = 100;

    while changed && failsafe > 0 {
        changed = false;
        reverse = !reverse;
        failsafe -= 1;

        let x_range: Vec<i32> = if reverse {
            (1..width - 1).rev().collect()
        } else {
            (1..width - 1).collect()
        };
        let y_range: Vec<i32> = if reverse {
            (1..height - 1).rev().collect()
        } else {
            (1..height - 1).collect()
        };

        for &x in &x_range {
            for &y in &y_range {
                let idx = build_data.map.xy_idx(x, y);
                let tile = build_data.map.tiles[idx];

                // Only process blocking tiles (walls or impassable liquid)
                if tile.terrain != TerrainType::Wall { continue; }

                // Check horizontal: same liquid on left and right
                let left_idx = build_data.map.xy_idx(x - 1, y);
                let right_idx = build_data.map.xy_idx(x + 1, y);
                let left_liq = build_data.map.tiles[left_idx].liquid;
                let right_liq = build_data.map.tiles[right_idx].liquid;

                if left_liq != LiquidType::None && left_liq == right_liq {
                    // Replace this wall with floor + same liquid
                    build_data.map.tiles[idx].terrain = TerrainType::Floor;
                    build_data.map.tiles[idx].liquid = left_liq;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                    changed = true;
                    continue;
                }

                // Check vertical: same liquid above and below
                let up_idx = build_data.map.xy_idx(x, y - 1);
                let down_idx = build_data.map.xy_idx(x, y + 1);
                let up_liq = build_data.map.tiles[up_idx].liquid;
                let down_liq = build_data.map.tiles[down_idx].liquid;

                if up_liq != LiquidType::None && up_liq == down_liq {
                    build_data.map.tiles[idx].terrain = TerrainType::Floor;
                    build_data.map.tiles[idx].liquid = up_liq;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                    changed = true;
                }
            }
        }
    }
}

// ─── MetaMapBuilder ─────────────────────────────────────────────────────────

/// Remove lake/liquid tiles that are completely isolated from any dry passable tile.
/// These are artifacts from blobs placed in solid wall areas. Reverts them to Wall.
fn remove_isolated_lake_tiles(build_data: &mut BuilderMap) {
    use crate::map::tile::is_passable;

    let width = build_data.map.width;
    let height = build_data.map.height;

    // Find all tiles reachable from any dry passable tile
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();

    // Seed from all dry passable tiles (floor/door without liquid)
    for y in 0..height {
        for x in 0..width {
            let idx = build_data.map.xy_idx(x, y);
            if is_passable(build_data.map.tiles[idx]) && build_data.map.tiles[idx].liquid == LiquidType::None {
                if !reachable.contains(&idx) {
                    reachable.insert(idx);
                    queue.push_back((x, y));
                }
            }
        }
    }

    // Flood-fill through all passable tiles (including liquid ones)
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
            let n_idx = build_data.map.xy_idx(nx, ny);
            if !reachable.contains(&n_idx) {
                // Can reach through any passable terrain (including liquid tiles)
                if is_passable(build_data.map.tiles[n_idx]) {
                    reachable.insert(n_idx);
                    queue.push_back((nx, ny));
                }
            }
        }
    }

    // Remove liquid from any tile not reachable from dry land → revert to Wall
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let idx = build_data.map.xy_idx(x, y);
            if build_data.map.tiles[idx].liquid != LiquidType::None && !reachable.contains(&idx) {
                build_data.map.tiles[idx].terrain = TerrainType::Wall;
                build_data.map.tiles[idx].liquid = LiquidType::None;
                build_data.map.tiles[idx].decoration = Decoration::None;
            }
        }
    }
}

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let tile_count = (build_data.map.width * build_data.map.height) as usize;
        let mut lake_map = vec![false; tile_count];

        // Phase 1: designLakes — generate blobs, place as Floor, mark in lakeMap
        design_lakes(build_data, &mut lake_map);

        // Phase 2: fillLakes — assign liquid types, merge via scanWidth, create wreaths
        fill_lakes(build_data, &mut lake_map, self.depth);

        // Phase 3: cleanUpLakeBoundaries — merge thin walls between same-type lakes
        clean_up_lake_boundaries(build_data);

        // Phase 4: Remove isolated lake tiles unreachable from dry land
        remove_isolated_lake_tiles(build_data);
    }
}
