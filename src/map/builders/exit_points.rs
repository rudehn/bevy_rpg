use bevy::log::{debug, warn};
use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::constants::MAX_FLOOR;
use crate::map::{
    builders::{BuilderMap, MetaMapBuilder},
    tile::TerrainType,
};

#[derive(Clone)]
pub struct DistantExit {}

impl MetaMapBuilder for DistantExit {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl DistantExit {
    #[allow(dead_code)]
    pub fn new() -> Box<DistantExit> {
        Box::new(DistantExit {})
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        let starting_pos = build_data.require_starting_position("DistantExit").clone();
        let start_idx = build_data
            .map
            .point2d_to_index(Point::new(starting_pos.x, starting_pos.y));
        let map_starts: Vec<usize> = vec![start_idx];

        // 1. Temporarily swap doors and stairs for floors so Dijkstra can
        //    traverse through them (UpStairs at the start position on floors > 1
        //    must be passable for distance computation to work).
        let original_tiles = build_data.map.tiles.clone();
        for tile in build_data.map.tiles.iter_mut() {
            match tile.terrain {
                TerrainType::Door | TerrainType::UpStairs | TerrainType::DownStairs => {
                    tile.terrain = TerrainType::Floor;
                }
                _ => {}
            }
        }

        // 2. Compute the Dijkstra map
        let dijkstra_map = DijkstraMap::new(
            build_data.map.width() as usize,
            build_data.map.height() as usize,
            &map_starts,
            &build_data.map,
            3000.0,
        );

        // 3. Restore the original tiles (putting the doors/stairs back)
        build_data.map.tiles = original_tiles;

        let mut exit_tile: Option<(usize, f32)> = None;
        for y in 0..build_data.map.height() {
            for x in 0..build_data.map.width() {
                let pt = Point::new(x, y);
                let idx = build_data.map.point2d_to_index(pt);
                let terrain = build_data.map.get_tile(pt).map(|t| t.terrain);
                // Consider any walkable non-stair tile as exit candidate
                if terrain == Some(TerrainType::Floor) {
                    let distance_to_start = dijkstra_map.map[idx];
                    if distance_to_start != std::f32::MAX && distance_to_start > 0.0 {
                        let better = match exit_tile {
                            None => true,
                            Some((_, best)) => distance_to_start > best,
                        };
                        if better {
                            exit_tile = Some((idx, distance_to_start));
                        }
                    }
                }
            }
        }

        // Fallback: if Dijkstra found nothing, pick any floor tile (Manhattan-farthest from start)
        if exit_tile.is_none() {
            warn!(
                "DistantExit: Dijkstra found no reachable floor tile from start ({}, {}). Using Manhattan fallback.",
                starting_pos.x, starting_pos.y
            );
            let mut best: Option<(usize, i32)> = None;
            for y in 0..build_data.map.height() {
                for x in 0..build_data.map.width() {
                    let pt = Point::new(x, y);
                    let idx = build_data.map.point2d_to_index(pt);
                    if build_data.map.get_tile(pt).map(|t| t.terrain) == Some(TerrainType::Floor) {
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
                warn!("DistantExit: No floor tile found on entire map! Stairs will not be placed.");
                return;
            }
        }

        let (stairs_idx, best_dist) = exit_tile.unwrap();
        let stairs_pos = build_data.map.index_to_point2d(stairs_idx);

        if build_data.map.depth >= MAX_FLOOR {
            // Boss room handled by BossRoomBuilder — no stairs on final floor
            debug!(
                "DistantExit: floor {} is MAX_FLOOR ({}), skipping stair placement",
                build_data.map.depth, MAX_FLOOR
            );
            return;
        }

        debug!(
            "DistantExit: placing DownStairs at ({}, {}) on floor {} (distance {:.1})",
            stairs_pos.x, stairs_pos.y, build_data.map.depth, best_dist
        );
        build_data.map.set_tile(stairs_pos, TerrainType::DownStairs);

        build_data.take_snapshot();
    }
}
