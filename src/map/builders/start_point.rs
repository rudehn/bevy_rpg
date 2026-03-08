use crate::{
    components::Position,
    map::{
        builders::{BuilderMap, MetaMapBuilder},
        tile::TerrainType,
    },
};

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

#[derive(Clone)]
pub struct StartPointBuilder {}

impl MetaMapBuilder for StartPointBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl StartPointBuilder {
    #[allow(dead_code)]
    pub fn new() -> Box<StartPointBuilder> {
        Box::new(StartPointBuilder {})
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        if let Some(rooms) = &build_data.rooms {
            if let Some(first_room) = rooms.first() {
                let start_pos = first_room.center();
                build_data.starting_position = Some(Position {
                    x: start_pos.x,
                    y: start_pos.y,
                });
                if build_data.map.depth() > 1 {
                    build_data.map.set_tile(start_pos, TerrainType::UpStairs);
                }
            } else {
                panic!("Cannot determine starting point: No rooms have been generated.");
            }
        } else {
            panic!("Cannot determine starting point: Room data is missing.");
        }
    }
}
