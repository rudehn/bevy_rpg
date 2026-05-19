//! Lake generation matching BrogueCE's Architect.c: designLakes → fillLakes → createWreath.
//!
//! Phase 1 (designLakes): Generate blob shapes on a full-size grid, try random placements,
//! validate connectivity hypothetically, stamp as Floor on the DUNGEON layer, mark in lakeMap.
//!
//! Phase 2 (fillLakes): Iterate lakeMap. For each unfilled lake tile, pick a liquid type,
//! flood-fill within scanWidth=4 to merge nearby blobs, then create a circular wreath.

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::map::Map;
use crate::map::tile::{Decoration, TerrainType, LiquidType};
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::random::RandomNumberGenerator;
use std::collections::VecDeque;

/// Brogue's fillLake scanWidth — lake tiles within this radius merge into the same liquid.
const LAKE_SCAN_WIDTH: i32 = 4;

/// Maximum random placement attempts per lake size.
const MAX_PLACEMENT_ATTEMPTS: i32 = 20;

pub struct LakeBuilder {
    depth: i32,
}

impl LakeBuilder {
    pub fn new(depth: i32) -> Self {
        Self { depth }
    }

    /// Pick liquid type based on depth (Brogue's liquidType function).
    /// Brogue: below minimumLavaLevel → no lava; below minimumBrimstoneLevel → no brimstone.
    /// Tuning:
    /// - Floors 1-9: water + chasm only (no lava — early descent is "natural" hazards).
    /// - Floors 10-17: water/lava/chasm at moderate lava share.
    /// - Floors 18+: deep-dungeon mix, lava-dominant.
    fn pick_liquid_type(&self, rng: &mut RandomNumberGenerator) -> LiquidType {
        let roll: f32 = rng.range(0.0f32, 1.0f32);
        match self.depth {
            1..=9 => {
                if roll < 0.70 { LiquidType::Water }
                else { LiquidType::Chasm }
            }
            10..=17 => {
                if roll < 0.40 { LiquidType::Water }
                else if roll < 0.75 { LiquidType::Lava }
                else { LiquidType::Chasm }
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
        max_blob_width,
        max_blob_height,
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
    map: &Map,
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
    let total = (map_w * map_h) as usize;

    // Pre-compute proposed lake mask as Vec<bool> to avoid per-tile closure overhead
    let mut proposed_lake = vec![false; total];
    for j in 0..blob_h {
        for i in 0..blob_w {
            let gx = i + blob_x;
            let gy = j + blob_y;
            if gx < 0 || gy < 0 || gx >= grid.width || gy >= grid.height { continue; }
            if grid.data[grid.xy_idx(gx, gy)] != BlobType::Floor { continue; }
            let dx = gx + grid_to_dungeon_x;
            let dy = gy + grid_to_dungeon_y;
            if dx >= 0 && dy >= 0 && dx < map_w && dy < map_h {
                proposed_lake[map.xy_idx(dx, dy)] = true;
            }
        }
    }

    // Find first passable tile that is NOT a lake tile and NOT the proposed blob
    let mut start = None;
    for j in 0..map_h {
        for i in 0..map_w {
            let idx = map.xy_idx(i, j);
            if is_passable(map.tiles[idx]) && !lake_map[idx] && !proposed_lake[idx] {
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
    let mut visited = vec![false; total];
    let mut queue = VecDeque::new();
    let start_idx = map.xy_idx(sx, sy);
    visited[start_idx] = true;
    queue.push_back((sx, sy));
    let mut visited_count = 1usize;

    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= map_w || ny >= map_h { continue; }
            let n_idx = map.xy_idx(nx, ny);
            if visited[n_idx] { continue; }
            if !is_passable(map.tiles[n_idx]) { continue; }
            if lake_map[n_idx] { continue; }
            if proposed_lake[n_idx] { continue; }
            visited[n_idx] = true;
            visited_count += 1;
            queue.push_back((nx, ny));
        }
    }

    // Count total eligible tiles and compare with visited count
    let mut total_eligible = 0usize;
    for idx in 0..total {
        if is_passable(map.tiles[idx]) && !lake_map[idx] && !proposed_lake[idx] {
            total_eligible += 1;
        }
    }

    visited_count < total_eligible
}

/// Brogue's designLakes: generate blob shapes, try placements, validate connectivity,
/// stamp as Floor terrain, mark in lakeMap.
fn design_lakes(ctx: &mut impl BuildContext, lake_map: &mut [bool]) {
    let map_w = ctx.width();
    let map_h = ctx.height();

    let start_pos = match ctx.starting_position() {
        Some(p) => p,
        None => return,
    };
    let _start_idx = ctx.map().xy_idx(start_pos.x, start_pos.y);

    // Brogue: for (lakeMaxHeight = 15, lakeMaxWidth = 30; lakeMaxHeight >= 10; lakeMaxHeight--, lakeMaxWidth -= 2)
    let mut lake_max_height = 15i32;
    let mut lake_max_width = 30i32;

    while lake_max_height >= 10 {
        // Generate ONE blob on a full-size grid
        let (grid, blob_x, blob_y, blob_w, blob_h) =
            generate_blob_on_full_grid(map_w, map_h, lake_max_width, lake_max_height);

        // Try up to 20 random placements
        for _ in 0..MAX_PLACEMENT_ATTEMPTS {
            // Brogue: x = rand_range(1 - lakeX, DCOLS - lakeWidth - lakeX - 2)
            // This is the offset to convert grid coords to dungeon coords.
            let x_min = 1 - blob_x;
            let x_max = map_w - blob_w - blob_x - 2;
            let y_min = 1 - blob_y;
            let y_max = map_h - blob_h - blob_y - 2;

            if x_min > x_max || y_min > y_max { continue; }

            let offset_x = ctx.rng().range(x_min, x_max + 1);
            let offset_y = ctx.rng().range(y_min, y_max + 1);

            // Check connectivity hypothetically (don't modify the map)
            if lake_disrupts_passability(
                ctx.map(), &grid, lake_map,
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

                    let idx = ctx.map().xy_idx(dx, dy);
                    // Brogue: pmap[...].layers[DUNGEON] = FLOOR (overwrites everything)
                    // We protect stairs only.
                    let terrain = ctx.map().tiles[idx].terrain;
                    if terrain == TerrainType::DownStairs || terrain == TerrainType::UpStairs {
                        continue;
                    }
                    lake_map[idx] = true;
                    ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
                    ctx.map_mut().tiles[idx].decoration = Decoration::None;
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
    map: &mut Map,
    lake_map: &mut [bool],
    start_x: i32,
    start_y: i32,
    liquid: LiquidType,
) -> Vec<usize> {
    let width = map.width;
    let height = map.height;
    let mut wreath_tiles = Vec::new();
    let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
    queue.push_back((start_x, start_y));

    while let Some((x, y)) = queue.pop_front() {
        for dy in -LAKE_SCAN_WIDTH..=LAKE_SCAN_WIDTH {
            for dx in -LAKE_SCAN_WIDTH..=LAKE_SCAN_WIDTH {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                let idx = map.xy_idx(nx, ny);
                if lake_map[idx] {
                    lake_map[idx] = false;
                    map.tiles[idx].liquid = liquid;
                    wreath_tiles.push(idx);
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    wreath_tiles
}

/// Brogue's createWreath: place shallow liquid in a circular radius around each
/// deep lake tile. Uses Euclidean distance. Converts doors to floor.
fn create_wreath(
    map: &mut Map,
    wreath_tiles_src: &[usize],
    shallow_liquid: LiquidType,
    wreath_width: i32,
) {
    if wreath_width == 0 { return; }

    let width = map.width;
    let height = map.height;
    let total = (width * height) as usize;
    let mut wreath_mask = vec![false; total];

    for &lake_idx in wreath_tiles_src {
        let (lx, ly) = map.idx_xy(lake_idx);
        for dy in -wreath_width..=wreath_width {
            for dx in -wreath_width..=wreath_width {
                let nx = lx + dx;
                let ny = ly + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                // Euclidean distance check (circular wreath)
                if dx * dx + dy * dy > wreath_width * wreath_width { continue; }

                let n_idx = map.xy_idx(nx, ny);
                if map.tiles[n_idx].liquid != LiquidType::None { continue; }
                let terrain = map.tiles[n_idx].terrain;
                if terrain == TerrainType::Wall || terrain == TerrainType::Empty {
                    continue;
                }

                wreath_mask[n_idx] = true;
            }
        }
    }

    for (idx, &is_wreath) in wreath_mask.iter().enumerate().take(total) {
        if is_wreath {
            map.tiles[idx].liquid = shallow_liquid;
            if map.tiles[idx].terrain == TerrainType::Door
                || map.tiles[idx].terrain == TerrainType::OpenDoor
            {
                map.tiles[idx].terrain = TerrainType::Floor;
            }
        }
    }
}

/// Brogue's fillLakes: iterate lakeMap, for each unfilled lake tile pick a liquid
/// type, flood-fill connected lake tiles, create wreath.
fn fill_lakes(ctx: &mut impl BuildContext, lake_map: &mut [bool], depth: i32) {
    let width = ctx.width();
    let height = ctx.height();

    for y in 0..height {
        for x in 0..width {
            let idx = ctx.map().xy_idx(x, y);
            if lake_map[idx] {
                // Pick liquid type for this connected lake group
                let builder = LakeBuilder { depth };
                let liquid = builder.pick_liquid_type(ctx.rng());

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

                let wreath_set = fill_lake(ctx.map_mut(), lake_map, x, y, liquid);
                create_wreath(ctx.map_mut(), &wreath_set, shallow, wreath_width);
            }
        }
    }
}

// ─── cleanUpLakeBoundaries ──────────────────────────────────────────────────

/// Brogue's cleanUpLakeBoundaries: merge thin walls sandwiched between same-type
/// lake tiles. If a wall/blocking tile has the same lake type on opposite sides
/// (horizontal or vertical), it gets replaced with that lake type.
fn clean_up_lake_boundaries(map: &mut Map) {
    let width = map.width;
    let height = map.height;

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
                let idx = map.xy_idx(x, y);
                let tile = map.tiles[idx];

                // Only process blocking tiles (walls or impassable liquid)
                if tile.terrain != TerrainType::Wall { continue; }

                let is_liquid = |idx: usize| map.tiles[idx].liquid != LiquidType::None;

                // Check horizontal: any liquid on both left and right
                let left_idx = map.xy_idx(x - 1, y);
                let right_idx = map.xy_idx(x + 1, y);

                if is_liquid(left_idx) && is_liquid(right_idx) {
                    // Copy all tile data from the right neighbor (Brogue copies all layers)
                    map.tiles[idx] = map.tiles[right_idx];
                    changed = true;
                    continue;
                }

                // Check vertical: any liquid above and below
                let up_idx = map.xy_idx(x, y - 1);
                let down_idx = map.xy_idx(x, y + 1);

                if is_liquid(up_idx) && is_liquid(down_idx) {
                    map.tiles[idx] = map.tiles[down_idx];
                    changed = true;
                }
            }
        }
    }
}

// ─── MapBuilder ─────────────────────────────────────────────────────────────

impl<C: BuildContext> MapBuilder<C> for LakeBuilder {
    fn name(&self) -> &'static str { "LakeBuilder" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::TerrainCleanup) }

    fn build(&mut self, ctx: &mut C) {
        let tile_count = (ctx.width() * ctx.height()) as usize;
        let mut lake_map = vec![false; tile_count];

        // Phase 1: designLakes — generate blobs, place as Floor, mark in lakeMap
        design_lakes(ctx, &mut lake_map);

        // Phase 2: fillLakes — assign liquid types, merge via scanWidth, create wreaths
        fill_lakes(ctx, &mut lake_map, self.depth);

        // Phase 3: cleanUpLakeBoundaries — merge thin walls between same-type lakes
        clean_up_lake_boundaries(ctx.map_mut());

        // Note: isolated lake cleanup is now handled by IsolatedAreaCuller later in the pipeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Floors 1-9 must never produce lava — only water + chasm.
    #[test]
    fn pick_liquid_type_floors_1_to_9_excludes_lava() {
        let mut rng = RandomNumberGenerator::new();
        for depth in 1..=9 {
            let builder = LakeBuilder { depth };
            let mut saw_water = false;
            let mut saw_chasm = false;
            for _ in 0..2000 {
                match builder.pick_liquid_type(&mut rng) {
                    LiquidType::Water => saw_water = true,
                    LiquidType::Chasm => saw_chasm = true,
                    LiquidType::Lava => panic!("depth {} must not produce lava", depth),
                    _ => {}
                }
            }
            assert!(saw_water, "depth {} should produce water", depth);
            assert!(saw_chasm, "depth {} should produce chasm", depth);
        }
    }

    /// Floor 10 is the first floor where lava can appear.
    #[test]
    fn pick_liquid_type_floor_10_can_produce_lava() {
        let builder = LakeBuilder { depth: 10 };
        let mut rng = RandomNumberGenerator::new();
        let mut saw_water = false;
        let mut saw_lava = false;
        let mut saw_chasm = false;
        for _ in 0..2000 {
            match builder.pick_liquid_type(&mut rng) {
                LiquidType::Water => saw_water = true,
                LiquidType::Lava => saw_lava = true,
                LiquidType::Chasm => saw_chasm = true,
                _ => {}
            }
        }
        assert!(saw_water, "depth 10 should produce water");
        assert!(saw_lava, "depth 10 should produce lava");
        assert!(saw_chasm, "depth 10 should produce chasm");
    }

    /// Verify shallow-floor distribution is roughly 70% water / 30% chasm.
    #[test]
    fn pick_liquid_type_shallow_distribution() {
        let builder = LakeBuilder { depth: 5 };
        let mut rng = RandomNumberGenerator::new();
        let n = 10_000;
        let mut water = 0;
        let mut chasm = 0;
        for _ in 0..n {
            match builder.pick_liquid_type(&mut rng) {
                LiquidType::Water => water += 1,
                LiquidType::Chasm => chasm += 1,
                _ => {}
            }
        }
        let water_pct = water as f64 / n as f64;
        let chasm_pct = chasm as f64 / n as f64;
        assert!((water_pct - 0.70).abs() < 0.05, "water {:.2} expected ~0.70", water_pct);
        assert!((chasm_pct - 0.30).abs() < 0.05, "chasm {:.2} expected ~0.30", chasm_pct);
    }

    /// Verify mid-depth distribution (floors 10-17) is roughly 40/35/25.
    #[test]
    fn pick_liquid_type_mid_distribution() {
        let builder = LakeBuilder { depth: 12 };
        let mut rng = RandomNumberGenerator::new();
        let n = 10_000;
        let mut water = 0;
        let mut lava = 0;
        let mut chasm = 0;
        for _ in 0..n {
            match builder.pick_liquid_type(&mut rng) {
                LiquidType::Water => water += 1,
                LiquidType::Lava => lava += 1,
                LiquidType::Chasm => chasm += 1,
                _ => {}
            }
        }
        let water_pct = water as f64 / n as f64;
        let lava_pct = lava as f64 / n as f64;
        let chasm_pct = chasm as f64 / n as f64;
        assert!((water_pct - 0.40).abs() < 0.05, "water {:.2} expected ~0.40", water_pct);
        assert!((lava_pct - 0.35).abs() < 0.05, "lava {:.2} expected ~0.35", lava_pct);
        assert!((chasm_pct - 0.25).abs() < 0.05, "chasm {:.2} expected ~0.25", chasm_pct);
    }
}
