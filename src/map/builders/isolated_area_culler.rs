//! Culls any area not connected to the largest passable region on the map.
//! Disconnected rooms, isolated lake tiles, and orphaned corridors all get
//! reverted to Wall. Ensures the player can reach everything on the map.

use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, LiquidType, is_passable};
use std::collections::{HashSet, VecDeque};

pub struct IsolatedAreaCuller;

impl IsolatedAreaCuller {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for IsolatedAreaCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let width = build_data.map.width;
        let height = build_data.map.height;

        // Find all connected regions of passable tiles via flood-fill
        let mut assigned = vec![false; (width * height) as usize];
        let mut regions: Vec<HashSet<usize>> = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let idx = build_data.map.xy_idx(x, y);
                if assigned[idx] || !is_passable(build_data.map.tiles[idx]) {
                    continue;
                }

                // Flood-fill this region
                let mut region = HashSet::new();
                let mut queue = VecDeque::new();
                queue.push_back((x, y));
                assigned[idx] = true;
                region.insert(idx);

                while let Some((cx, cy)) = queue.pop_front() {
                    for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                        let n_idx = build_data.map.xy_idx(nx, ny);
                        if !assigned[n_idx] && is_passable(build_data.map.tiles[n_idx]) {
                            assigned[n_idx] = true;
                            region.insert(n_idx);
                            queue.push_back((nx, ny));
                        }
                    }
                }

                regions.push(region);
            }
        }

        // Find the largest region
        let largest = regions.iter().max_by_key(|r| r.len());
        let keep: HashSet<usize> = match largest {
            Some(r) => r.clone(),
            None => return,
        };

        // Cull everything not in the largest region
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if is_passable(build_data.map.tiles[idx]) && !keep.contains(&idx) {
                    build_data.map.tiles[idx].terrain = TerrainType::Wall;
                    build_data.map.tiles[idx].liquid = LiquidType::None;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                }
            }
        }
    }
}
