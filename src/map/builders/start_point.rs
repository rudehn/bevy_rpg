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

#[allow(dead_code)]
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
        let rooms = build_data.require_rooms("StartPointBuilder");
        let first_room = rooms.first().unwrap_or_else(||
            panic!("StartPointBuilder requires at least one room"));
        let start_pos = first_room.center();
        build_data.starting_position = Some(Position {
            x: start_pos.x,
            y: start_pos.y,
        });
        if build_data.map.depth() > 1 {
            build_data.map.set_tile(start_pos, TerrainType::UpStairs);
        }
    }
}
