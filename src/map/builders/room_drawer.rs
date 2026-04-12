//! Carves room interiors from the rooms list.

use bracket_lib::prelude::Rect;

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::tile::TerrainType;

#[derive(Clone)]
pub struct RoomDrawer;

impl RoomDrawer {
    pub fn new() -> Self {
        RoomDrawer
    }
}

impl<C: BuildContext> MapBuilder<C> for RoomDrawer {
    fn name(&self) -> &'static str {
        "RoomDrawer"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Geometry)
    }
    fn build(&mut self, ctx: &mut C) {
        let rooms: Vec<Rect> = match ctx.rooms() {
            Some(r) => r.clone(),
            None => return,
        };

        for room in rooms.iter() {
            for y in room.y1 + 1..room.y2 {
                for x in room.x1 + 1..room.x2 {
                    let pt = bracket_lib::prelude::Point::new(x, y);
                    ctx.map_mut().set_tile(pt, TerrainType::Floor);
                }
            }
            ctx.take_snapshot();
        }
    }
}
