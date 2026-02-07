use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::TileType;
use crate::map::Map;
use bracket_lib::prelude::Point;

#[derive(Clone)]
pub struct UnseenCuller {}

impl UnseenCuller {
    pub fn new() -> Box<UnseenCuller> {
        Box::new(UnseenCuller {})
    }
}

impl MetaMapBuilder for UnseenCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let width = build_data.map.width();
        let height = build_data.map.height();

        let mut update_vec = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let non_wall_tiles = count_non_walls_in_radius(
                    &*build_data.map, // Pass a reference to the dyn Map
                    x, y, 1
                );
                if non_wall_tiles == 0 {
                    update_vec.push((x, y));
                }
            }
        }

        for (x, y) in update_vec {
            build_data.map.set_tile(Point::new(x, y), TileType::Empty);
        }

        build_data.take_snapshot();
    }
}

fn count_non_walls_in_radius(map: &dyn Map, x: i32, y: i32, radius: i32) -> i32 {
    let mut count = 0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x + dx;
            let ny = y + dy; // Fixed bug here (was y + y)
            
            // Check if within bounds and if it's not a wall
            if map.get_tile(Point::new(nx, ny))
                .map(|cell| cell != TileType::Wall)
                .unwrap_or(false)
            {
                count += 1;
            }
        }
    }
    count
}

