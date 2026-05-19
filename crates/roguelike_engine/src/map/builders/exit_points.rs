//! Finds the farthest reachable tile from the starting position and
//! places DownStairs there.
//!
//! Uses Dijkstra distance mapping from bracket-lib. If Dijkstra finds
//! no reachable tile, falls back to Manhattan distance.
//!
//! Game-specific final-floor logic (placing Amulet, Portal, etc.)
//! should be handled by a separate game-side builder that runs after
//! this one.

use bevy::log::{info, warn};
use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::tile::{LiquidType, TerrainType};

#[derive(Clone)]
pub struct DistantExit;

impl DistantExit {
    pub fn new() -> Self {
        Self
    }
}

impl<C: BuildContext> MapBuilder<C> for DistantExit {
    fn name(&self) -> &'static str {
        "DistantExit"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }
    fn build(&mut self, ctx: &mut C) {
        let Some(starting_pos) = ctx.starting_position() else {
            warn!("DistantExit: starting position not set — skipping");
            return;
        };
        let start_idx = ctx
            .map()
            .point2d_to_index(Point::new(starting_pos.x, starting_pos.y));
        let map_starts: Vec<usize> = vec![start_idx];

        // 1. Temporarily flatten doors/stairs for Dijkstra traversal.
        let original_tiles = ctx.map().tiles.clone();
        for tile in ctx.map_mut().tiles.iter_mut() {
            match tile.terrain {
                TerrainType::Door | TerrainType::UpStairs | TerrainType::DownStairs => {
                    tile.terrain = TerrainType::Floor;
                }
                _ => {}
            }
        }

        // 2. Compute Dijkstra map.
        let dijkstra_map = DijkstraMap::new(
            ctx.map().width() as usize,
            ctx.map().height() as usize,
            &map_starts,
            ctx.map(),
            3000.0,
        );

        // 3. Restore original tiles.
        ctx.map_mut().tiles = original_tiles;

        // 4. Find the farthest reachable dry floor tile.
        let mut exit_tile: Option<(usize, f32)> = None;
        for y in 0..ctx.map().height() {
            for x in 0..ctx.map().width() {
                let pt = Point::new(x, y);
                let idx = ctx.map().point2d_to_index(pt);
                let tile = ctx.map().get_tile(pt);
                if tile.map(|t| t.terrain) == Some(TerrainType::Floor)
                    && tile.map(|t| t.liquid) == Some(LiquidType::None)
                {
                    let distance = dijkstra_map.map[idx];
                    if distance != f32::MAX && distance > 0.0 {
                        let better = match exit_tile {
                            None => true,
                            Some((_, best)) => distance > best,
                        };
                        if better {
                            exit_tile = Some((idx, distance));
                        }
                    }
                }
            }
        }

        // 5. Manhattan fallback.
        if exit_tile.is_none() {
            warn!("DistantExit: Dijkstra found no reachable floor tile. Using Manhattan fallback.");
            let mut best: Option<(usize, i32)> = None;
            for y in 0..ctx.map().height() {
                for x in 0..ctx.map().width() {
                    let pt = Point::new(x, y);
                    let idx = ctx.map().point2d_to_index(pt);
                    let tile = ctx.map().get_tile(pt);
                    if tile.map(|t| t.terrain) == Some(TerrainType::Floor)
                        && tile.map(|t| t.liquid) == Some(LiquidType::None)
                    {
                        let dist = (x - starting_pos.x).abs() + (y - starting_pos.y).abs();
                        let better = match best {
                            None => true,
                            Some((_, d)) => dist > d,
                        };
                        if better {
                            best = Some((idx, dist));
                        }
                    }
                }
            }
            if let Some((idx, _)) = best {
                exit_tile = Some((idx, 0.0));
            } else {
                warn!("DistantExit: No floor tile found! Stairs will not be placed.");
                return;
            }
        }

        // 6. Place DownStairs.
        let (stairs_idx, best_dist) = exit_tile.unwrap();
        let stairs_pos = ctx.map().index_to_point2d(stairs_idx);

        info!(
            "DistantExit: placing DownStairs at ({}, {}) on floor {} (distance {:.1})",
            stairs_pos.x, stairs_pos.y, ctx.map().depth, best_dist,
        );
        ctx.map_mut().set_tile(stairs_pos, TerrainType::DownStairs);
        ctx.map_mut().set_liquid(stairs_pos, LiquidType::None);

        ctx.take_snapshot();
    }
}
