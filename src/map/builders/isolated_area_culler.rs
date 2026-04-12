//! Culls any area not connected to the player-reachable region.
//!
//! Disconnected rooms, isolated lake tiles, and orphaned corridors get
//! reverted to Wall. Ensures the player can reach everything on the map.

use bevy::log::warn;
use std::collections::VecDeque;

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::tile::{is_passable, Decoration, LiquidType, TerrainType};

pub struct IsolatedAreaCuller;

impl IsolatedAreaCuller {
    pub fn new() -> Self {
        Self
    }
}

impl<C: BuildContext> MapBuilder<C> for IsolatedAreaCuller {
    fn name(&self) -> &'static str { "IsolatedAreaCuller" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::ConnectivityCull) }
    fn build(&mut self, ctx: &mut C) {
        let width = ctx.map().width;
        let height = ctx.map().height;
        let total = (width * height) as usize;

        let mut region_id = vec![0u32; total];
        let mut region_sizes: Vec<usize> = Vec::new();
        let mut current_id = 0u32;
        let mut queue = VecDeque::new();

        for y in 0..height {
            for x in 0..width {
                let idx = ctx.map().xy_idx(x, y);
                if region_id[idx] != 0 || !is_passable(ctx.map().tiles[idx]) {
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
                        let n_idx = ctx.map().xy_idx(nx, ny);
                        if region_id[n_idx] == 0 && is_passable(ctx.map().tiles[n_idx]) {
                            region_id[n_idx] = current_id;
                            queue.push_back((nx, ny));
                        }
                    }
                }

                region_sizes.push(size);
            }
        }

        if current_id == 0 { return; }

        let start_idx = ctx.starting_position().map(|pos| ctx.map().xy_idx(pos.x, pos.y));

        let keep_id = if let Some(si) = start_idx {
            let rid = region_id[si];
            if rid != 0 {
                rid
            } else {
                warn!("IsolatedAreaCuller: starting position not in any passable region");
                let (max_idx, _) = region_sizes.iter().enumerate().max_by_key(|(_, s)| *s).unwrap();
                (max_idx + 1) as u32
            }
        } else {
            let (max_idx, _) = region_sizes.iter().enumerate().max_by_key(|(_, s)| *s).unwrap();
            (max_idx + 1) as u32
        };

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = ctx.map().xy_idx(x, y);
                if region_id[idx] != 0 && region_id[idx] != keep_id {
                    ctx.map_mut().tiles[idx].terrain = TerrainType::Wall;
                    ctx.map_mut().tiles[idx].liquid = LiquidType::None;
                    ctx.map_mut().tiles[idx].decoration = Decoration::None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builders::EngineBuilderMap;
    use crate::map::tile::TerrainType;

    #[test]
    fn smaller_region_walled_off() {
        let mut ctx = EngineBuilderMap::with_seed(1, 10, 5, "test", 42);

        // Large region: x=1..4, y=1..3
        for y in 1..4 {
            for x in 1..5 {
                let i = ctx.map.xy_idx(x, y);
                ctx.map.tiles[i].terrain = TerrainType::Floor;
            }
        }
        // Small region: x=7..8, y=2
        let i72 = ctx.map.xy_idx(7, 2);
        let i82 = ctx.map.xy_idx(8, 2);
        ctx.map.tiles[i72].terrain = TerrainType::Floor;
        ctx.map.tiles[i82].terrain = TerrainType::Floor;

        IsolatedAreaCuller.build(&mut ctx);

        assert_eq!(ctx.map.tiles[ctx.map.xy_idx(2, 2)].terrain, TerrainType::Floor);
        assert_eq!(ctx.map.tiles[i72].terrain, TerrainType::Wall);
        assert_eq!(ctx.map.tiles[i82].terrain, TerrainType::Wall);
    }

    #[test]
    fn single_connected_region_unchanged() {
        let mut ctx = EngineBuilderMap::with_open_room(6, 6, 42);
        let before = ctx.map.tiles.iter().filter(|t| t.terrain == TerrainType::Floor).count();
        IsolatedAreaCuller.build(&mut ctx);
        let after = ctx.map.tiles.iter().filter(|t| t.terrain == TerrainType::Floor).count();
        assert_eq!(before, after);
    }
}
