use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, LiquidType, Tile, is_walkable};
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::prelude::{Point, Algorithm2D};
use rand::prelude::*;
use std::collections::{HashSet, VecDeque};

enum WreathType {
    Liquid(LiquidType),
    #[allow(dead_code)]
    Decoration(Decoration),
}

/// Lake sizes to try, from largest to smallest (Brogue uses 30×15 down to 20×10).
const LAKE_SIZES: [(i32, i32); 6] = [
    (30, 15),
    (28, 14),
    (26, 13),
    (24, 12),
    (22, 11),
    (20, 10),
];

const MAX_PLACEMENT_ATTEMPTS: i32 = 20;

/// Brogue's fillLake scanWidth — lake tiles within this radius merge.
const LAKE_SCAN_WIDTH: i32 = 4;

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

        let local_grid = Grid::new(max_width + 4, max_height + 4, BlobType::Wall);
        let (blob_grid, min_x, min_y, _w, _h) =
            create_blob(&local_grid, &config, BlobType::Floor, BlobType::Wall);

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

    /// Phase 1: Design — stamp lake tiles as Floor terrain and mark in lake_map.
    /// Does NOT assign liquid yet (matching Brogue's designLakes).
    fn design_lake(
        &self,
        build_data: &mut BuilderMap,
        lake_map: &mut Vec<bool>,
        blob_tiles: &[(i32, i32)],
        ox: i32,
        oy: i32,
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

            // Lakes carve through walls and floor. Protect stairs and empty.
            match terrain {
                TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::Empty => {}
                _ => {
                    build_data.map.tiles[idx].terrain = TerrainType::Floor;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                    lake_map[idx] = true;
                    affected.push(idx);
                }
            }
        }
        affected
    }

    /// Revert designed lake tiles.
    fn revert_lake(
        &self,
        build_data: &mut BuilderMap,
        lake_map: &mut Vec<bool>,
        backup: &[(usize, Tile)],
    ) {
        for &(idx, tile) in backup {
            build_data.map.tiles[idx] = tile;
            lake_map[idx] = false;
        }
    }

    /// Phase 2: Fill — Brogue's fillLake. Recursive flood-fill from a lake tile,
    /// scanning within LAKE_SCAN_WIDTH to merge nearby lake tiles into the same
    /// liquid type.
    fn fill_lake(
        &self,
        build_data: &mut BuilderMap,
        lake_map: &mut Vec<bool>,
        wreath_set: &mut HashSet<usize>,
        x: i32,
        y: i32,
        liquid: LiquidType,
    ) {
        let width = build_data.map.width;
        let height = build_data.map.height;

        for dy in -LAKE_SCAN_WIDTH..=LAKE_SCAN_WIDTH {
            for dx in -LAKE_SCAN_WIDTH..=LAKE_SCAN_WIDTH {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                let idx = build_data.map.xy_idx(nx, ny);
                if lake_map[idx] {
                    lake_map[idx] = false; // mark as filled
                    build_data.map.tiles[idx].liquid = liquid;
                    wreath_set.insert(idx);
                    // Recurse to merge nearby lake tiles
                    self.fill_lake(build_data, lake_map, wreath_set, nx, ny, liquid);
                }
            }
        }
    }

    /// Create wreath around filled lake tiles (Brogue's createWreath).
    fn create_wreath(
        &self,
        build_data: &mut BuilderMap,
        wreath_set: &HashSet<usize>,
        liquid: LiquidType,
    ) {
        // Wreath width by liquid type (Brogue: Water=2, Lava=0, Chasm=1)
        let (wreath_width, wreath_type) = match liquid {
            LiquidType::Water => (2, WreathType::Liquid(LiquidType::ShallowWater)),
            LiquidType::Lava => return, // Brogue: no lava wreath
            LiquidType::Chasm => return, // TODO: chasm edge
            _ => return,
        };

        let width = build_data.map.width;
        let height = build_data.map.height;
        let mut wreath_tiles = Vec::new();

        for &lake_idx in wreath_set {
            let (lx, ly) = build_data.map.idx_xy(lake_idx);
            for dy in -wreath_width..=wreath_width {
                for dx in -wreath_width..=wreath_width {
                    let nx = lx + dx;
                    let ny = ly + dy;
                    if nx < 1 || ny < 1 || nx >= width - 1 || ny >= height - 1 { continue; }
                    if dx * dx + dy * dy > wreath_width * wreath_width { continue; }

                    let n_idx = build_data.map.xy_idx(nx, ny);
                    if wreath_set.contains(&n_idx) { continue; }
                    if build_data.map.tiles[n_idx].liquid != LiquidType::None { continue; }
                    // Wreath only places on floor/door (doesn't carve walls)
                    let terrain = build_data.map.tiles[n_idx].terrain;
                    if terrain != TerrainType::Floor && terrain != TerrainType::Door
                        && terrain != TerrainType::OpenDoor { continue; }

                    wreath_tiles.push(n_idx);
                }
            }
        }

        let unique_wreath: HashSet<usize> = wreath_tiles.into_iter().collect();
        for idx in unique_wreath {
            // Brogue converts doors in the wreath to floor
            if build_data.map.tiles[idx].terrain == TerrainType::Door
                || build_data.map.tiles[idx].terrain == TerrainType::OpenDoor
            {
                build_data.map.tiles[idx].terrain = TerrainType::Floor;
            }

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
}

/// Brogue-style connectivity check: flood-fill from start and verify all
/// non-lake passable tiles are reachable.
fn lake_disrupts_passability(
    map: &crate::map::map::Map,
    lake_map: &[bool],
    start_idx: usize,
) -> bool {
    use crate::map::tile::is_passable;

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    if is_passable(map.tiles[start_idx]) && !lake_map[start_idx] {
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
            if !visited.contains(&n_idx) && is_passable(map.tiles[n_idx]) && !lake_map[n_idx] {
                visited.insert(n_idx);
                queue.push_back(n_idx);
            }
        }
    }

    // Any passable non-lake tile that wasn't reached?
    for (idx, tile) in map.tiles.iter().enumerate() {
        if is_passable(*tile) && !lake_map[idx] && !visited.contains(&idx) {
            return true;
        }
    }
    false
}

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();

        let start_pos = match &build_data.starting_position {
            Some(p) => *p,
            None => return,
        };
        let start_idx = build_data.map.xy_idx(start_pos.x, start_pos.y);
        let tile_count = (build_data.map.width * build_data.map.height) as usize;

        // === PHASE 1: DESIGN LAKES (mark positions, carve terrain, no liquid) ===
        let mut lake_map = vec![false; tile_count];

        for &(lake_w, lake_h) in &LAKE_SIZES {
            let Some(blob_tiles) = self.generate_lake_blob(lake_w, lake_h, &mut rng) else {
                continue;
            };

            for _ in 0..MAX_PLACEMENT_ATTEMPTS {
                let ox = rng.random_range(2..build_data.map.width - lake_w - 2);
                let oy = rng.random_range(2..build_data.map.height - lake_h - 2);

                if !self.lake_overlaps_floor(build_data, &blob_tiles, ox, oy) {
                    continue;
                }

                // Build a HYPOTHETICAL lake_map to check connectivity without
                // modifying the actual map (matching Brogue's approach).
                let mut hypothetical_lake = lake_map.clone();
                for &(bx, by) in &blob_tiles {
                    let wx = ox + bx;
                    let wy = oy + by;
                    if wx >= 1 && wy >= 1 && wx < build_data.map.width - 1 && wy < build_data.map.height - 1 {
                        let idx = build_data.map.xy_idx(wx, wy);
                        let terrain = build_data.map.tiles[idx].terrain;
                        if terrain != TerrainType::DownStairs && terrain != TerrainType::UpStairs
                            && terrain != TerrainType::Empty
                        {
                            hypothetical_lake[idx] = true;
                        }
                    }
                }

                // Connectivity check on the ORIGINAL map with hypothetical lake blocked
                if lake_disrupts_passability(&build_data.map, &hypothetical_lake, start_idx) {
                    continue;
                }

                // Passed! Now actually stamp the lake.
                let affected = self.design_lake(build_data, &mut lake_map, &blob_tiles, ox, oy);
                if affected.is_empty() { continue; }

                break;
            }
        }

        // === PHASE 2: FILL LAKES (assign liquid types, merge via scanWidth) ===
        // Iterate lake_map. When we find an unfilled lake tile, pick a liquid
        // type and flood-fill all connected lake tiles (within LAKE_SCAN_WIDTH)
        // with that same liquid. This merges nearby blobs into the same type.
        let width = build_data.map.width;
        let height = build_data.map.height;

        for y in 0..height {
            for x in 0..width {
                let idx = build_data.map.xy_idx(x, y);
                if lake_map[idx] {
                    let liquid = self.pick_liquid_type(&mut rng);
                    let mut wreath_set = HashSet::new();
                    self.fill_lake(build_data, &mut lake_map, &mut wreath_set, x, y, liquid);
                    self.create_wreath(build_data, &wreath_set, liquid);
                }
            }
        }
    }
}
