use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{TerrainType, LiquidType, is_walkable};
use crate::map::map::Map;
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::prelude::{DijkstraMap, Point, Algorithm2D, BaseMap, SmallVec};
use rand::prelude::*;

#[allow(dead_code)]
pub struct LakeBuilder {
    liquid_type: LiquidType,
}

// Wrapper to perform Dijkstra checks using only walkable tiles
#[allow(dead_code)]
struct WalkableMap<'a> {
    map: &'a Map,
}

impl<'a> Algorithm2D for WalkableMap<'a> {
    fn dimensions(&self) -> Point {
        self.map.dimensions()
    }
}

impl<'a> BaseMap for WalkableMap<'a> {
    fn is_opaque(&self, idx: usize) -> bool {
        self.map.is_opaque(idx)
    }

    fn get_available_exits(&self, idx: usize) -> SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let (x, y) = self.map.idx_xy(idx);

        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 { continue; }
                let nx = x + dx;
                let ny = y + dy;
                let np = Point::new(nx, ny);

                if self.map.in_bounds(np) {
                    let next_idx = self.map.xy_idx(nx, ny);
                    if is_walkable(self.map.tiles[next_idx]) {
                        let cost = if dx != 0 && dy != 0 { 1.45 } else { 1.0 };
                        exits.push((next_idx, cost));
                    }
                }
            }
        }
        exits
    }
}

#[allow(dead_code)]
impl LakeBuilder {
    pub fn new(liquid_type: LiquidType) -> Box<LakeBuilder> {
        Box::new(LakeBuilder { liquid_type })
    }

    fn merge_lakes(&self, build_data: &mut BuilderMap) {
        let width = build_data.map.width;
        let height = build_data.map.height;
        
        let mut made_change = true;
        let mut failsafe = 10;
        while made_change && failsafe > 0 {
            made_change = false;
            failsafe -= 1;
            let mut changes = Vec::new();

            for y in 1..height - 1 {
                for x in 1..width - 1 {
                    let idx = build_data.map.xy_idx(x, y);
                    if build_data.map.tiles[idx].terrain == TerrainType::Wall {
                        let mut found_match = false;

                        // Check Horizontal
                        let left = build_data.map.xy_idx(x - 1, y);
                        let right = build_data.map.xy_idx(x + 1, y);
                        if build_data.map.tiles[left].liquid == self.liquid_type 
                            && build_data.map.tiles[right].liquid == self.liquid_type 
                        {
                            found_match = true;
                        }

                        // Check Vertical
                        let up = build_data.map.xy_idx(x, y - 1);
                        let down = build_data.map.xy_idx(x, y + 1);
                        if build_data.map.tiles[up].liquid == self.liquid_type 
                            && build_data.map.tiles[down].liquid == self.liquid_type 
                        {
                            found_match = true;
                        }

                        if found_match {
                            changes.push(idx);
                            made_change = true;
                        }
                    }
                }
            }

            for idx in changes {
                build_data.map.tiles[idx].terrain = TerrainType::Floor;
                build_data.map.tiles[idx].liquid = self.liquid_type;
            }
        }
    }

    fn add_wreaths(&self, build_data: &mut BuilderMap) {
        if self.liquid_type != LiquidType::Water { return; }
        
        let mut wreath_tiles = Vec::new();
        let width = build_data.map.width;
        let height = build_data.map.height;

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if build_data.map.tiles[idx].liquid == LiquidType::None && build_data.map.tiles[idx].terrain == TerrainType::Floor {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let nx = x + dx;
                            let ny = y + dy;
                            if nx >= 0 && nx < width && ny >= 0 && ny < height {
                                let n_idx = build_data.map.xy_idx(nx, ny);
                                if build_data.map.tiles[n_idx].liquid == LiquidType::Water {
                                    wreath_tiles.push(idx);
                                    break;
                                }
                            }
                        }
                        if wreath_tiles.last() == Some(&idx) { break; }
                    }
                }
            }
        }

        for idx in wreath_tiles {
            build_data.map.tiles[idx].liquid = LiquidType::ShallowWater;
        }
    }
}

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();
        let num_lakes = rng.random_range(3..6);

        let backup_tiles = build_data.map.tiles.clone();

        for _ in 0..num_lakes {
            let config = BlobGenConfig {
                round_count: 5,
                min_blob_width: 10,
                min_blob_height: 10,
                max_blob_width: 40,
                max_blob_height: 30,
                initial_alive_percent: rng.random_range(50..60),
                birth_threshold: 5,
                survival_threshold: 4,
            };

            let initial_grid = Grid::new(build_data.map.width, build_data.map.height, BlobType::Wall);
            let (blob_grid, _, _, _, _) = create_blob(&initial_grid, &config, BlobType::Floor, BlobType::Wall);

            // Filter blob: must respect 1-tile border AND overlap with existing floor
            let mut overlaps_dungeon = false;
            for y in 1..build_data.map.height - 1 {
                for x in 1..build_data.map.width - 1 {
                    let idx = build_data.map.xy_idx(x, y);
                    if blob_grid.data[idx] == BlobType::Floor && build_data.map.tiles[idx].terrain == TerrainType::Floor {
                        overlaps_dungeon = true;
                        break;
                    }
                }
                if overlaps_dungeon { break; }
            }

            if overlaps_dungeon {
                for y in 1..build_data.map.height - 1 {
                    for x in 1..build_data.map.width - 1 {
                        let idx = build_data.map.xy_idx(x, y);
                        if blob_grid.data[idx] == BlobType::Floor {
                            build_data.map.tiles[idx].terrain = TerrainType::Floor;
                            build_data.map.tiles[idx].liquid = self.liquid_type;
                        }
                    }
                }
            }
        }

        self.merge_lakes(build_data);
        self.add_wreaths(build_data);

        // Connectivity check - ensure the level is traversable BY WALKING
        if let Some(start_pos) = &build_data.starting_position {
            let start_idx = build_data.map.xy_idx(start_pos.x, start_pos.y);
            let exit_idx = build_data.map.tiles.iter().position(|t| t.terrain == TerrainType::DownStairs);
            
            if let Some(target) = exit_idx {
                let walkable_map = WalkableMap { map: &build_data.map };
                let dijkstra = DijkstraMap::new(
                    build_data.map.width as usize, 
                    build_data.map.height as usize, 
                    &[start_idx], 
                    &walkable_map, 
                    2000.0
                );
                
                if dijkstra.map[target] >= 2000.0 {
                    build_data.map.tiles = backup_tiles;
                    return;
                }
            }
        }

        build_data.take_snapshot();
    }
}
