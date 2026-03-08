use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{is_walkable, TileType};
use bracket_lib::prelude::Point;
use rand::prelude::*;

#[derive(Clone)]
pub struct DiagonalCuller {}

impl DiagonalCuller {
    pub fn new() -> Box<DiagonalCuller> {
        Box::new(DiagonalCuller {})
    }
}

impl MetaMapBuilder for DiagonalCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let width = build_data.map.width();
        let height = build_data.map.height();
        let mut rng = rand::rng();

        let mut diagonal_corner_removed = true;
        while diagonal_corner_removed {
            diagonal_corner_removed = false;
            let mut changes = Vec::new();

            for y in 0..height - 1 {
                for x in 0..width - 1 {
                    for k in 0..=1 {
                        let idx_a = build_data.map.xy_idx(x + k, y);
                        let idx_b = build_data.map.xy_idx(x + (1 - k), y);
                        let idx_c = build_data.map.xy_idx(x + k, y + 1);
                        let idx_d = build_data.map.xy_idx(x + (1 - k), y + 1);

                        let tile_a = build_data.map.tiles[idx_a];
                        let tile_b = build_data.map.tiles[idx_b];
                        let tile_c = build_data.map.tiles[idx_c];
                        let tile_d = build_data.map.tiles[idx_d];

                        if is_walkable(tile_a)
                            && !is_walkable(tile_b)
                            && !is_walkable(tile_c)
                            && is_walkable(tile_d)
                        {
                            // Diagonal crack found:
                            // (x+k, y+1): Wall    (x+1-k, y+1): Floor
                            // (x+k, y):   Floor   (x+1-k, y):   Wall
                            // OR vice versa depending on k.
                            
                            // We choose one of the walls to fill.
                            let (target_x, source_x, target_y) = if rng.random_bool(0.5) {
                                (x + (1 - k), x + k, y)
                            } else {
                                (x + k, x + (1 - k), y + 1)
                            };

                            changes.push((target_x, target_y, build_data.map.tiles[build_data.map.xy_idx(source_x, target_y)]));
                            diagonal_corner_removed = true;
                        }
                    }
                }
            }

            for (tx, ty, tile) in changes {
                build_data.map.set_tile(Point::new(tx, ty), tile);
            }
            
            // If we found any, we loop again to catch any new cracks created by the filling.
            // In practice, this usually converges very quickly.
        }

        build_data.take_snapshot();
    }
}
