//! Removes single isolated wall tiles (pillars) that are completely surrounded
//! by non-wall tiles. These look out of place and serve no structural purpose.

use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::TerrainType;

pub struct PillarCuller;

impl PillarCuller {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for PillarCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let width = build_data.map.width;
        let height = build_data.map.height;
        let mut to_remove = Vec::new();

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if build_data.map.tiles[idx].terrain != TerrainType::Wall {
                    continue;
                }

                // Count wall neighbors in all 8 directions
                let mut wall_count = 0;
                for dy in -1..=1i32 {
                    for dx in -1..=1i32 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                        let n_idx = build_data.map.xy_idx(nx, ny);
                        if build_data.map.tiles[n_idx].terrain == TerrainType::Wall
                            || build_data.map.tiles[n_idx].terrain == TerrainType::Empty
                        {
                            wall_count += 1;
                        }
                    }
                }

                // If no wall neighbors at all, this is an isolated pillar — remove it
                if wall_count == 0 {
                    to_remove.push(idx);
                }
            }
        }

        for idx in to_remove {
            build_data.map.tiles[idx].terrain = TerrainType::Floor;
        }
    }
}
