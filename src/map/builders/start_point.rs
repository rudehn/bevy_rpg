use crate::{
    components::Position,
    map::builders::{BuilderMap, MetaMapBuilder},
};
use bracket_lib::prelude::Rect;

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
            } else {
                panic!("Cannot determine starting point: No rooms have been generated.");
            }
        } else {
            panic!("Cannot determine starting point: Room data is missing.");
        }
    }
}
