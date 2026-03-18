use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, LiquidType, Tile, is_walkable};
use crate::map::map::Map;
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::prelude::{Point, Algorithm2D, BaseMap, SmallVec};
use rand::prelude::*;
use std::collections::{HashSet, VecDeque};

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

    /// Stamp a lake onto the map. Only overwrites floor tiles (not walls/stairs).
    /// Doors that fall within the lake are converted to floor + liquid.
    /// Returns the set of affected tile indices.
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

            // Only place lake on floor or door tiles — never on walls, stairs, or empty
            match terrain {
                TerrainType::Floor | TerrainType::Door | TerrainType::OpenDoor => {
                    // Doors become floor (lake absorbs them, like Brogue)
                    build_data.map.tiles[idx].terrain = TerrainType::Floor;
                    build_data.map.tiles[idx].liquid = liquid;
                    affected.push(idx);
                }
                _ => {}
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

    /// Add a wreath (shallow liquid border) around lake tiles.
    fn add_wreath(
        &self,
        build_data: &mut BuilderMap,
        lake_indices: &HashSet<usize>,
        liquid: LiquidType,
    ) {
        let width = build_data.map.width;
        let height = build_data.map.height;

        match liquid {
            LiquidType::Water => {
                // ShallowWater wreath around deep water
                let mut wreath = Vec::new();
                for y in 1..height - 1 {
                    for x in 1..width - 1 {
                        let idx = build_data.map.xy_idx(x, y);
                        if build_data.map.tiles[idx].terrain == TerrainType::Floor
                            && build_data.map.tiles[idx].liquid == LiquidType::None
                        {
                            // Check if adjacent to a lake tile
                            let mut near_lake = false;
                            for dy in -1..=1i32 {
                                for dx in -1..=1i32 {
                                    if dx == 0 && dy == 0 { continue; }
                                    let nx = x + dx;
                                    let ny = y + dy;
                                    if nx >= 0 && nx < width && ny >= 0 && ny < height {
                                        let n_idx = build_data.map.xy_idx(nx, ny);
                                        if lake_indices.contains(&n_idx) {
                                            near_lake = true;
                                            break;
                                        }
                                    }
                                }
                                if near_lake { break; }
                            }
                            if near_lake {
                                wreath.push(idx);
                            }
                        }
                    }
                }
                for idx in wreath {
                    build_data.map.tiles[idx].liquid = LiquidType::ShallowWater;
                }
            }
            LiquidType::Lava => {
                // ScorchedEarth decoration wreath (no liquid layer)
                let mut wreath = Vec::new();
                for y in 1..height - 1 {
                    for x in 1..width - 1 {
                        let idx = build_data.map.xy_idx(x, y);
                        if build_data.map.tiles[idx].terrain == TerrainType::Floor
                            && build_data.map.tiles[idx].liquid == LiquidType::None
                            && build_data.map.tiles[idx].decoration == Decoration::None
                        {
                            let mut near_lake = false;
                            for dy in -1..=1i32 {
                                for dx in -1..=1i32 {
                                    if dx == 0 && dy == 0 { continue; }
                                    let nx = x + dx;
                                    let ny = y + dy;
                                    if nx >= 0 && nx < width && ny >= 0 && ny < height {
                                        let n_idx = build_data.map.xy_idx(nx, ny);
                                        if lake_indices.contains(&n_idx) {
                                            near_lake = true;
                                            break;
                                        }
                                    }
                                }
                                if near_lake { break; }
                            }
                            if near_lake {
                                wreath.push(idx);
                            }
                        }
                    }
                }
                for idx in wreath {
                    build_data.map.tiles[idx].decoration = Decoration::ScorchedEarth;
                }
            }
            LiquidType::Chasm => {
                // No wreath for chasms
            }
            _ => {}
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

/// Flood-fill from start, counting all reachable walkable tiles.
fn count_reachable(map: &Map, start_idx: usize) -> usize {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start_idx);
    visited.insert(start_idx);

    while let Some(idx) = queue.pop_front() {
        let (x, y) = map.idx_xy(idx);
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            let np = Point::new(nx, ny);
            if !map.in_bounds(np) { continue; }
            let n_idx = map.xy_idx(nx, ny);
            if !visited.contains(&n_idx) && is_walkable(map.tiles[n_idx]) {
                visited.insert(n_idx);
                queue.push_back(n_idx);
            }
        }
    }
    visited.len()
}

/// Count total walkable tiles on the map.
fn total_walkable(map: &Map) -> usize {
    map.tiles.iter().filter(|t| is_walkable(**t)).count()
}

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();
        let num_lakes = rng.random_range(2..5);
        let mut lakes_placed = 0;

        // Pre-count walkable tiles for connectivity validation
        let start_pos = match &build_data.starting_position {
            Some(p) => *p,
            None => return,
        };
        let start_idx = build_data.map.xy_idx(start_pos.x, start_pos.y);
        let initial_reachable = count_reachable(&build_data.map, start_idx);

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

                // Connectivity check: can we still reach the same number of tiles?
                let new_reachable = count_reachable(&build_data.map, start_idx);
                if new_reachable < initial_reachable - affected.len() {
                    // Lake disconnected part of the map — revert
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
    }
}
