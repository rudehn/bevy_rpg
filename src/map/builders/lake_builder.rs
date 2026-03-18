use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, LiquidType, Tile, is_walkable};
use crate::map::map::Map;
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::prelude::{Point, Algorithm2D, BaseMap, SmallVec};
use rand::prelude::*;
use std::collections::{HashSet, VecDeque};

enum WreathType {
    Liquid(LiquidType),
    Decoration(Decoration),
}

/// Lake sizes to try, from largest to smallest (matching Brogue's designLakes).
const LAKE_SIZES: [(i32, i32); 4] = [
    (30, 15),  // large lake
    (28, 13),  // medium-large
    (26, 11),  // medium
    (24, 9),   // small
];

/// Max random placement attempts per lake size.
const MAX_PLACEMENT_ATTEMPTS: i32 = 20;

pub struct LakeBuilder {
    depth: i32,
}

impl LakeBuilder {
    pub fn new(depth: i32) -> Box<Self> {
        Box::new(Self { depth })
    }

    /// Pick a liquid type based on dungeon depth.
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

    /// Generate a blob on a local grid and return tile positions relative to (0,0).
    fn generate_lake_blob(
        &self,
        max_width: i32,
        max_height: i32,
        rng: &mut impl Rng,
    ) -> Option<Vec<(i32, i32)>> {
        let config = BlobGenConfig {
            round_count: 5,
            min_blob_width: max_width / 3,
            min_blob_height: max_height / 3,
            max_blob_width: max_width,
            max_blob_height: max_height,
            initial_alive_percent: rng.random_range(45..55),
            birth_threshold: 5,
            survival_threshold: 4,
        };

        // Generate on a local grid sized to the lake
        let local_grid = Grid::new(max_width + 4, max_height + 4, BlobType::Wall);
        let (blob_grid, min_x, min_y, _w, _h) =
            create_blob(&local_grid, &config, BlobType::Floor, BlobType::Wall);

        // Extract blob tile positions relative to min corner
        let mut tiles = Vec::new();
        for y in 0..blob_grid.height {
            for x in 0..blob_grid.width {
                let idx = blob_grid.xy_idx(x, y);
                if blob_grid.data[idx] == BlobType::Floor {
                    tiles.push((x - min_x, y - min_y));
                }
            }
        }

        if tiles.is_empty() { None } else { Some(tiles) }
    }

    /// Check if placing a lake at offset (ox, oy) overlaps any existing floor tile.
    fn lake_overlaps_floor(
        &self,
        build_data: &BuilderMap,
        blob_tiles: &[(i32, i32)],
        ox: i32,
        oy: i32,
    ) -> bool {
        for &(bx, by) in blob_tiles {
            let wx = ox + bx;
            let wy = oy + by;
            if wx < 1 || wy < 1 || wx >= build_data.map.width - 1 || wy >= build_data.map.height - 1 {
                continue;
            }
            let idx = build_data.map.xy_idx(wx, wy);
            if build_data.map.tiles[idx].terrain == TerrainType::Floor
                && build_data.map.tiles[idx].liquid == LiquidType::None
            {
                return true;
            }
        }
        false
    }

    /// Stamp a lake onto the map. Like Brogue, lakes overwrite walls AND floor,
    /// carving through terrain to create large open water features. Stairs and
    /// empty tiles are protected. Doors are absorbed.
    fn stamp_lake(
        &self,
        build_data: &mut BuilderMap,
        blob_tiles: &[(i32, i32)],
        ox: i32,
        oy: i32,
        liquid: LiquidType,
    ) -> Vec<usize> {
        let mut affected = Vec::new();
        for &(bx, by) in blob_tiles {
            let wx = ox + bx;
            let wy = oy + by;
            if wx < 1 || wy < 1 || wx >= build_data.map.width - 1 || wy >= build_data.map.height - 1 {
                continue;
            }
            let idx = build_data.map.xy_idx(wx, wy);
            let terrain = build_data.map.tiles[idx].terrain;

            // Lakes carve through walls and floor (like Brogue). Only protect stairs and empty.
            match terrain {
                TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::Empty => {}
                _ => {
                    build_data.map.tiles[idx].terrain = TerrainType::Floor;
                    build_data.map.tiles[idx].liquid = liquid;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                    affected.push(idx);
                }
            }
        }
        affected
    }

    /// Revert stamped lake tiles.
    fn revert_lake(
        &self,
        build_data: &mut BuilderMap,
        affected: &[usize],
        backup: &[(usize, Tile)],
    ) {
        for &(idx, tile) in backup {
            build_data.map.tiles[idx] = tile;
        }
    }

    /// Add a wreath (shallow liquid border) around lake tiles using Euclidean
    /// distance for circular shorelines (matches Brogue's createWreath).
    fn add_wreath(
        &self,
        build_data: &mut BuilderMap,
        lake_indices: &HashSet<usize>,
        liquid: LiquidType,
    ) {
        // Wreath width by liquid type (from Brogue: Water=2, Lava=0, Chasm=1)
        let (wreath_width, wreath_type) = match liquid {
            LiquidType::Water => (2, WreathType::Liquid(LiquidType::ShallowWater)),
            LiquidType::Lava => (1, WreathType::Decoration(Decoration::ScorchedEarth)),
            LiquidType::Chasm => return, // no wreath
            _ => return,
        };

        let width = build_data.map.width;
        let height = build_data.map.height;
        let mut wreath_tiles = Vec::new();

        // For each lake tile, check all tiles within wreath_width radius.
        // Wreaths carve through walls to create smooth shorelines.
        for &lake_idx in lake_indices {
            let (lx, ly) = build_data.map.idx_xy(lake_idx);
            for dy in -wreath_width..=wreath_width {
                for dx in -wreath_width..=wreath_width {
                    let nx = lx + dx;
                    let ny = ly + dy;
                    if nx < 1 || ny < 1 || nx >= width - 1 || ny >= height - 1 { continue; }
                    if dx * dx + dy * dy > wreath_width * wreath_width { continue; }

                    let n_idx = build_data.map.xy_idx(nx, ny);
                    if lake_indices.contains(&n_idx) { continue; }
                    if build_data.map.tiles[n_idx].liquid != LiquidType::None { continue; }
                    // Protect stairs and empty tiles, but carve walls and absorb doors
                    let terrain = build_data.map.tiles[n_idx].terrain;
                    if terrain == TerrainType::DownStairs || terrain == TerrainType::UpStairs
                        || terrain == TerrainType::Empty { continue; }

                    wreath_tiles.push(n_idx);
                }
            }
        }

        let unique_wreath: HashSet<usize> = wreath_tiles.into_iter().collect();
        for idx in unique_wreath {
            // Carve walls/doors to floor for the wreath
            let terrain = build_data.map.tiles[idx].terrain;
            if terrain == TerrainType::Wall || terrain == TerrainType::Door || terrain == TerrainType::OpenDoor {
                build_data.map.tiles[idx].terrain = TerrainType::Floor;
            }
            build_data.map.tiles[idx].decoration = Decoration::None;

            match wreath_type {
                WreathType::Liquid(liq) => {
                    build_data.map.tiles[idx].liquid = liq;
                }
                WreathType::Decoration(dec) => {
                    build_data.map.tiles[idx].decoration = dec;
                }
            }
        }
    }

    /// Merge adjacent lakes by converting walls sandwiched between same-liquid tiles.
    fn merge_lakes(&self, build_data: &mut BuilderMap, liquid: LiquidType) {
        let width = build_data.map.width;
        let height = build_data.map.height;

        let mut changed = true;
        let mut failsafe = 5;
        while changed && failsafe > 0 {
            changed = false;
            failsafe -= 1;
            let mut changes = Vec::new();

            for y in 1..height - 1 {
                for x in 1..width - 1 {
                    let idx = build_data.map.xy_idx(x, y);
                    if build_data.map.tiles[idx].terrain == TerrainType::Wall {
                        // Check horizontal: same liquid on both sides
                        let left = build_data.map.xy_idx(x - 1, y);
                        let right = build_data.map.xy_idx(x + 1, y);
                        if build_data.map.tiles[left].liquid == liquid
                            && build_data.map.tiles[right].liquid == liquid
                        {
                            changes.push(idx);
                            changed = true;
                            continue;
                        }
                        // Check vertical
                        let up = build_data.map.xy_idx(x, y - 1);
                        let down = build_data.map.xy_idx(x, y + 1);
                        if build_data.map.tiles[up].liquid == liquid
                            && build_data.map.tiles[down].liquid == liquid
                        {
                            changes.push(idx);
                            changed = true;
                        }
                    }
                }
            }

            for idx in changes {
                build_data.map.tiles[idx].terrain = TerrainType::Floor;
                build_data.map.tiles[idx].liquid = liquid;
            }
        }
    }
}

/// Clean up orphaned doors after lake placement (Brogue's finishDoors logic).
/// A door is orphaned if:
/// - It has passable terrain on both horizontal AND vertical sides (open space)
/// - OR it has 3+ wall/blocking neighbors in cardinal directions (dead-end)
fn finish_doors(map: &mut crate::map::map::Map) {
    let width = map.width;
    let height = map.height;
    let mut to_floor = Vec::new();

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let idx = map.xy_idx(x, y);
            if map.tiles[idx].terrain != TerrainType::Door { continue; }

            let left = map.tiles[map.xy_idx(x - 1, y)].terrain;
            let right = map.tiles[map.xy_idx(x + 1, y)].terrain;
            let up = map.tiles[map.xy_idx(x, y - 1)].terrain;
            let down = map.tiles[map.xy_idx(x, y + 1)].terrain;

            let is_blocking = |t: TerrainType| matches!(t, TerrainType::Wall | TerrainType::Empty);
            let is_passable = |t: TerrainType| !is_blocking(t);

            // Orphaned if passable on both left-right AND top-bottom
            let h_open = is_passable(left) || is_passable(right);
            let v_open = is_passable(up) || is_passable(down);
            if h_open && v_open {
                // Check if BOTH horizontal AND BOTH vertical are passable — truly open space
                if (is_passable(left) && is_passable(right))
                    || (is_passable(up) && is_passable(down))
                {
                    to_floor.push(idx);
                    continue;
                }
            }

            // Orphaned if 3+ blocking cardinal neighbors
            let blocking_count = [left, right, up, down].iter().filter(|&&t| is_blocking(t)).count();
            if blocking_count >= 3 {
                to_floor.push(idx);
            }
        }
    }

    for idx in to_floor {
        map.tiles[idx].terrain = TerrainType::Floor;
    }
}

/// Brogue-style connectivity check: flood-fill from start and verify all
/// non-lake walkable tiles are reachable. Returns true if any dry passable
/// tile is disconnected (lake disrupts passability).
fn lake_disrupts_passability(map: &Map, start_idx: usize) -> bool {
    use crate::map::tile::is_passable;

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Start flood-fill from player position
    if is_walkable(map.tiles[start_idx]) {
        queue.push_back(start_idx);
        visited.insert(start_idx);
    }

    while let Some(idx) = queue.pop_front() {
        let (x, y) = map.idx_xy(idx);
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            let np = Point::new(nx, ny);
            if !map.in_bounds(np) { continue; }
            let n_idx = map.xy_idx(nx, ny);
            if !visited.contains(&n_idx) && is_passable(map.tiles[n_idx]) {
                visited.insert(n_idx);
                queue.push_back(n_idx);
            }
        }
    }

    // Check: any passable non-liquid tile that wasn't reached?
    for (idx, tile) in map.tiles.iter().enumerate() {
        if is_passable(*tile)
            && tile.liquid == LiquidType::None
            && !visited.contains(&idx)
        {
            return true; // disconnected dry tile found
        }
    }
    false
}

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();
        let num_lakes = rng.random_range(2..5);
        let mut lakes_placed = 0;

        let start_pos = match &build_data.starting_position {
            Some(p) => *p,
            None => return,
        };
        let start_idx = build_data.map.xy_idx(start_pos.x, start_pos.y);

        // Try each lake size from largest to smallest (Brogue pattern)
        for &(lake_w, lake_h) in &LAKE_SIZES {
            if lakes_placed >= num_lakes { break; }

            let liquid = self.pick_liquid_type(&mut rng);

            // Generate a blob for this lake size
            let Some(blob_tiles) = self.generate_lake_blob(lake_w, lake_h, &mut rng) else {
                continue;
            };

            // Try random placements
            for _ in 0..MAX_PLACEMENT_ATTEMPTS {
                if lakes_placed >= num_lakes { break; }

                let ox = rng.random_range(2..build_data.map.width - lake_w - 2);
                let oy = rng.random_range(2..build_data.map.height - lake_h - 2);

                // Must overlap at least one existing floor tile
                if !self.lake_overlaps_floor(build_data, &blob_tiles, ox, oy) {
                    continue;
                }

                // Backup tiles for rollback
                let backup: Vec<(usize, Tile)> = blob_tiles.iter()
                    .filter_map(|&(bx, by)| {
                        let wx = ox + bx;
                        let wy = oy + by;
                        if wx >= 1 && wy >= 1 && wx < build_data.map.width - 1 && wy < build_data.map.height - 1 {
                            let idx = build_data.map.xy_idx(wx, wy);
                            Some((idx, build_data.map.tiles[idx]))
                        } else {
                            None
                        }
                    })
                    .collect();

                // Stamp the lake
                let affected = self.stamp_lake(build_data, &blob_tiles, ox, oy, liquid);
                if affected.is_empty() { continue; }

                // Connectivity check (Brogue-style): all non-lake passable tiles
                // must be reachable from the player start.
                if lake_disrupts_passability(&build_data.map, start_idx) {
                    self.revert_lake(build_data, &affected, &backup);
                    continue;
                }

                // Lake placed successfully
                let lake_set: HashSet<usize> = affected.iter().copied().collect();
                self.add_wreath(build_data, &lake_set, liquid);
                self.merge_lakes(build_data, liquid);
                lakes_placed += 1;
                break;
            }
        }

        // Clean up orphaned doors left by lake carving (Brogue's finishDoors)
        finish_doors(&mut build_data.map);
    }
}
