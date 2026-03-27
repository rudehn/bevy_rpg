//! Culls any area not connected to the player-reachable region on the map.
//! Disconnected rooms, isolated lake tiles, and orphaned corridors all get
//! reverted to Wall. Ensures the player can reach everything on the map.
//!
//! If a starting position is set, the region containing it is always kept
//! (even if it is not the largest). Otherwise, the largest region is kept.

use bevy::log::warn;

use super::{BuilderMap, BuilderPhase, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, LiquidType, is_passable};
use std::collections::VecDeque;

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
        let total = (width * height) as usize;

        // Region-ID map: 0 = unassigned, 1.. = region IDs
        let mut region_id = vec![0u32; total];
        let mut region_sizes: Vec<usize> = Vec::new(); // index = region_id - 1
        let mut current_id = 0u32;
        let mut queue = VecDeque::new();

        for y in 0..height {
            for x in 0..width {
                let idx = build_data.map.xy_idx(x, y);
                if region_id[idx] != 0 || !is_passable(build_data.map.tiles[idx]) {
                    continue;
                }

                current_id += 1;
                let mut size = 0usize;
                queue.push_back((x, y));
                region_id[idx] = current_id;

                while let Some((cx, cy)) = queue.pop_front() {
                    size += 1;
                    for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
                        let n_idx = build_data.map.xy_idx(nx, ny);
                        if region_id[n_idx] == 0 && is_passable(build_data.map.tiles[n_idx]) {
                            region_id[n_idx] = current_id;
                            queue.push_back((nx, ny));
                        }
                    }
                }

                region_sizes.push(size);
            }
        }

        if current_id == 0 { return; }

        // Determine which region to keep
        let start_idx = build_data.starting_position.as_ref().map(|pos| {
            build_data.map.xy_idx(pos.x, pos.y)
        });

        let keep_id = if let Some(si) = start_idx {
            let rid = region_id[si];
            if rid != 0 {
                rid
            } else {
                warn!("IsolatedAreaCuller: starting position idx {} is not in any passable region", si);
                // Fall back to the largest region
                let (max_idx, _) = region_sizes.iter().enumerate().max_by_key(|(_, s)| *s).unwrap();
                (max_idx + 1) as u32
            }
        } else {
            let (max_idx, _) = region_sizes.iter().enumerate().max_by_key(|(_, s)| *s).unwrap();
            (max_idx + 1) as u32
        };

        // Cull everything not in the kept region
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if region_id[idx] != 0 && region_id[idx] != keep_id {
                    build_data.map.tiles[idx].terrain = TerrainType::Wall;
                    build_data.map.tiles[idx].liquid = LiquidType::None;
                    build_data.map.tiles[idx].decoration = Decoration::None;
                }
            }
        }
    }

    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::ConnectivityCull) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::TerrainType;

    fn idx(bm: &BuilderMap, x: i32, y: i32) -> usize {
        bm.map.xy_idx(x, y)
    }

    #[test]
    fn smaller_region_walled_off() {
        let mut bm = BuilderMap::new_for_test(10, 5);

        // Large region: x=1..4, y=1..3 (4x3 = 12 tiles)
        for y in 1..4 {
            for x in 1..5 {
                let i = idx(&bm, x, y);
                bm.map.tiles[i].terrain = TerrainType::Floor;
            }
        }

        // Small region: x=7..8, y=2 (2 tiles)
        let i72 = idx(&bm, 7, 2);
        let i82 = idx(&bm, 8, 2);
        bm.map.tiles[i72].terrain = TerrainType::Floor;
        bm.map.tiles[i82].terrain = TerrainType::Floor;

        IsolatedAreaCuller.build_map(&mut bm);

        let i22 = idx(&bm, 2, 2);
        assert_eq!(bm.map.tiles[i22].terrain, TerrainType::Floor);
        assert_eq!(bm.map.tiles[i72].terrain, TerrainType::Wall);
        assert_eq!(bm.map.tiles[i82].terrain, TerrainType::Wall);
    }

    #[test]
    fn single_connected_region_unchanged() {
        let mut bm = BuilderMap::with_open_room(6, 6);

        let floor_before = bm.map.tiles.iter().filter(|t| t.terrain == TerrainType::Floor).count();

        IsolatedAreaCuller.build_map(&mut bm);

        let floor_after = bm.map.tiles.iter().filter(|t| t.terrain == TerrainType::Floor).count();
        assert_eq!(floor_before, floor_after);
    }
}
