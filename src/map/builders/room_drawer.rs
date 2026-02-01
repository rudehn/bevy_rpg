use crate::map::{
    builders::{BuilderMap, MetaMapBuilder},
    tile::TileType,
    Map,
};
use bracket_lib::prelude::{Point, Rect};

#[derive(Clone)]
pub struct RoomDrawer {}

impl MetaMapBuilder for RoomDrawer {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl RoomDrawer {
    #[allow(dead_code)]
    pub fn new() -> Box<RoomDrawer> {
        Box::new(RoomDrawer {})
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        let rooms: Vec<Rect>;
        if let Some(rooms_builder) = &build_data.rooms {
            rooms = rooms_builder.clone();
        } else {
            panic!("Room Drawing requires a builder with room structures");
        }

        for room in rooms.iter() {
            // Carve out the interior of the room
            for y in room.y1 + 1..room.y2 {
                for x in room.x1 + 1..room.x2 {
                    let pt = Point::new(x, y);
                    build_data.map.set_tile(pt, TileType::Floor);
                }
            }
            build_data.take_snapshot();
        }
    }
}
