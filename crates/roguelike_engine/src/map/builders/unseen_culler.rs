//! Replaces wall tiles that have no non-wall neighbours with Empty.
//!
//! Tiles completely surrounded by walls are never visible to the player
//! and waste rendering / FOV cycles. Replacing them with Empty means
//! they're skipped by the renderer and by bracket-lib's opacity checks.

use bracket_lib::prelude::Point;

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::map::Map;
use crate::map::tile::TerrainType;

#[derive(Clone)]
pub struct UnseenCuller;

impl UnseenCuller {
    pub fn new() -> Self {
        UnseenCuller
    }
}

impl<C: BuildContext> MapBuilder<C> for UnseenCuller {
    fn name(&self) -> &'static str {
        "UnseenCuller"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::TerrainCleanup)
    }
    fn build(&mut self, ctx: &mut C) {
        let width = ctx.map().width();
        let height = ctx.map().height();

        let mut update_vec = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let non_wall_tiles = count_non_walls_in_radius(ctx.map(), x, y, 1);
                if non_wall_tiles == 0 {
                    update_vec.push((x, y));
                }
            }
        }

        for (x, y) in update_vec {
            ctx.map_mut().set_tile(Point::new(x, y), TerrainType::Empty);
        }
        ctx.take_snapshot();
    }
}

fn count_non_walls_in_radius(map: &Map, x: i32, y: i32, radius: i32) -> i32 {
    let mut count = 0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x + dx;
            let ny = y + dy;
            if map
                .get_tile(Point::new(nx, ny))
                .map(|cell| cell.terrain != TerrainType::Wall)
                .unwrap_or(false)
            {
                count += 1;
            }
        }
    }
    count
}
