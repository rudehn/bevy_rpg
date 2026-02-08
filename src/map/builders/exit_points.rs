use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::map::{
    builders::{BuilderMap, MetaMapBuilder},
    tile::TileType,
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
        let starting_pos = build_data.starting_position.as_ref().unwrap().clone();
        let start_idx = build_data
            .map
            .point2d_to_index(Point::new(starting_pos.x, starting_pos.y));
        let map_starts: Vec<usize> = vec![start_idx];
        // Need to remove the doors since they block paths

        // let dijkstra_map = DijkstraMap::new(build_data.map.width() as usize, build_data.map.height() as usize, &map_starts , &map_clone, 3000.0);
        let dijkstra_map = DijkstraMap::new(
            build_data.map.width() as usize,
            build_data.map.height() as usize,
            &map_starts,
            &*build_data.map,
            3000.0,
        );
        let mut exit_tile = (0, 0.0f32);
        for y in 0..build_data.map.height() {
            for x in 0..build_data.map.width() {
                let pt = Point::new(x, y);
                let idx = build_data.map.point2d_to_index(pt);
                if build_data.map.get_tile(pt) == Some(TileType::Floor) {
                    let distance_to_start = dijkstra_map.map[idx];
                    if distance_to_start != std::f32::MAX {
                        // If it is further away than our current exit candidate, move the exit
                        if distance_to_start > exit_tile.1 {
                            exit_tile.0 = idx;
                            exit_tile.1 = distance_to_start;
                        }
                    }
                }
            }
        }

        // Place a staircase
        let stairs_idx = exit_tile.0;
        let stairs_pos = build_data.map.index_to_point2d(stairs_idx);
        build_data.map.set_tile(stairs_pos, TileType::DownStairs);
        build_data.take_snapshot();
    }
}
