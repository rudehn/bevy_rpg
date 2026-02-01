use bracket_lib::geometry::Point;
use bracket_lib::pathfinding::DistanceAlg::PythagorasSquared;

use crate::components::Position;
use crate::map::builders::{BuilderMap, MetaMapBuilder};
use crate::map::tile::is_walkable;

#[allow(dead_code)]
#[derive(Clone)]
pub enum XStart {
    LEFT,
    CENTER,
    RIGHT,
}

#[allow(dead_code)]
#[derive(Clone)]
pub enum YStart {
    TOP,
    CENTER,
    BOTTOM,
}

#[derive(Clone)]
pub struct AreaStartingPosition {
    x: XStart,
    y: YStart,
}

impl MetaMapBuilder for AreaStartingPosition {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl AreaStartingPosition {
    #[allow(dead_code)]
    pub fn new(x: XStart, y: YStart) -> Box<AreaStartingPosition> {
        Box::new(AreaStartingPosition { x, y })
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        let seed_x;
        let seed_y;

        match self.x {
            XStart::LEFT => seed_x = 1,
            XStart::CENTER => seed_x = build_data.map.width() / 2,
            XStart::RIGHT => seed_x = build_data.map.width() - 2,
        }

        match self.y {
            YStart::TOP => seed_y = 1,
            YStart::CENTER => seed_y = build_data.map.height() / 2,
            YStart::BOTTOM => seed_y = build_data.map.height() - 2,
        }

        let mut available_floors: Vec<(usize, f32)> = Vec::new();
        for (idx, tiletype) in build_data.map.tiles.data.iter().enumerate() {
            if is_walkable(tiletype) {
                available_floors.push((
                    idx,
                    PythagorasSquared.distance2d(
                        Point::new(
                            idx as i32 % build_data.map.width(),
                            idx as i32 / build_data.map.width(),
                        ),
                        Point::new(seed_x, seed_y),
                    ),
                ));
            }
        }
        if available_floors.is_empty() {
            panic!("No valid floors to start on");
        }

        available_floors.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let start_x = available_floors[0].0 as i32 % build_data.map.width();
        let start_y = available_floors[0].0 as i32 / build_data.map.width();

        build_data.starting_position = Some(Position {
            x: start_x,
            y: start_y,
        });
    }
}

#[derive(Clone)]
pub struct RoomBasedStartingPosition {}

impl MetaMapBuilder for RoomBasedStartingPosition {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl RoomBasedStartingPosition {
    #[allow(dead_code)]
    pub fn new() -> Box<RoomBasedStartingPosition> {
        Box::new(RoomBasedStartingPosition {})
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        if let Some(rooms) = &build_data.rooms {
            let start_pos = rooms[0].center();
            build_data.starting_position = Some(Position {
                x: start_pos.x,
                y: start_pos.y,
            });
        } else {
            panic!("Room Based Staring Position only works after rooms have been created");
        }
    }
}
