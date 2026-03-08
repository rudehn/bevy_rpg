use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{TerrainType, LiquidType};
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob, BlobType};
use bracket_lib::prelude::DijkstraMap;
use rand::prelude::*;

pub struct LakeBuilder {
    liquid_type: LiquidType,
}

impl LakeBuilder {
    pub fn new(liquid_type: LiquidType) -> Box<LakeBuilder> {
        Box::new(LakeBuilder { liquid_type })
    }
}

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();
        
        let config = BlobGenConfig {
            round_count: 4,
            min_blob_width: 5,
            min_blob_height: 5,
            max_blob_width: 25,
            max_blob_height: 25,
            initial_alive_percent: rng.random_range(45..60),
            birth_threshold: 5,
            survival_threshold: 4,
        };

        let initial_grid = Grid::new(build_data.map.width, build_data.map.height, BlobType::Wall);
        let (blob_grid, _, _, _, _) = create_blob(&initial_grid, &config, BlobType::Floor, BlobType::Wall);

        let mut potential_changes = Vec::new();
        for y in 0..build_data.map.height {
            for x in 0..build_data.map.width {
                let idx = build_data.map.xy_idx(x, y);
                if blob_grid.data[idx] == BlobType::Floor {
                    if build_data.map.tiles[idx].terrain == TerrainType::Floor {
                        potential_changes.push(idx);
                    }
                }
            }
        }

        if potential_changes.is_empty() { return; }

        let backup_tiles = build_data.map.tiles.clone();
        
        for &idx in &potential_changes {
            build_data.map.tiles[idx].liquid = self.liquid_type;
        }

        if let Some(start_pos) = &build_data.starting_position {
            let start_idx = build_data.map.xy_idx(start_pos.x, start_pos.y);
            let exit_idx = build_data.map.tiles.iter().position(|t| t.terrain == TerrainType::DownStairs);
            
            if let Some(target) = exit_idx {
                let dijkstra = DijkstraMap::new(
                    build_data.map.width as usize, 
                    build_data.map.height as usize, 
                    &[start_idx], 
                    &build_data.map, 
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
