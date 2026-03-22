//! Culls any area not connected to the player-reachable region on the map.
//! Disconnected rooms, isolated lake tiles, and orphaned corridors all get
//! reverted to Wall. Ensures the player can reach everything on the map.
//!
//! If a starting position is set, the region containing it is always kept
//! (even if it is not the largest). Otherwise, the largest region is kept.

use bevy::log::warn;

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

        // Prefer the region containing the starting position so the player
        // is never walled off. Fall back to the largest region.
        let start_idx = build_data.starting_position.as_ref().map(|pos| {
            build_data.map.xy_idx(pos.x, pos.y)
        });

        let keep: HashSet<usize> = if let Some(si) = start_idx {
            if let Some(r) = regions.iter().find(|r| r.contains(&si)) {
                r.clone()
            } else {
                // Starting position is on a non-passable tile (e.g. wall) —
                // this shouldn't happen, but fall back to the largest region.
                warn!("IsolatedAreaCuller: starting position idx {} is not in any passable region", si);
                match regions.iter().max_by_key(|r| r.len()) {
                    Some(r) => r.clone(),
                    None => return,
                }
            }
        } else {
            match regions.iter().max_by_key(|r| r.len()) {
                Some(r) => r.clone(),
                None => return,
            }
        };

        // Cull everything not in the kept region
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
