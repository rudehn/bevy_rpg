use bevy::prelude::warn;

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
        let Some(rooms) = build_data.rooms_or_warn("StartPointBuilder") else { return; };
        let Some(first_room) = rooms.first() else {
            warn!("StartPointBuilder: rooms list is empty — skipping");
            return;
        };
        let start_pos = first_room.center();
        build_data.set_starting_position(Position {
            x: start_pos.x,
            y: start_pos.y,
        });
        if build_data.map.depth() > 1 {
            build_data.map.set_tile(start_pos, TerrainType::UpStairs);
            build_data.map.set_liquid(start_pos, crate::map::tile::LiquidType::None);
        }
    }
}
