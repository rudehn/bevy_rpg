use crate::{
    components::{Collider, Goblin, Name, Position, Viewshed},
    constants::ENTITY_INDEX,
    game::DungeonTileset,
    map::{
        builders::{BuilderMap, MetaMapBuilder},
        map::MAP_SIZE,
    },
};
use bevy::prelude::*;
use bracket_lib::prelude::{Point, RandomNumberGenerator, Rect};

pub struct GoblinSpawner {}

impl MetaMapBuilder for GoblinSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.spawn_goblins(build_data);
    }
}

impl GoblinSpawner {
    pub fn new() -> Box<GoblinSpawner> {
        Box::new(GoblinSpawner {})
    }

    fn spawn_goblins(&mut self, build_data: &mut BuilderMap) {
        if let Some(rooms) = &build_data.rooms {
            let mut rng = RandomNumberGenerator::new();

            for room in rooms.iter() {
                // Decide whether to spawn a goblin (0 or 1 per room)
                if rng.roll_dice(1, 2) == 1 {
                    // 50% chance to spawn a goblin
                    let (x, y) = self.get_random_room_point(&room, &mut rng);
                    build_data
                        .spawn_list
                        .push((Point::new(x, y), "Goblin".to_string()));
                }
            }
        }
    }

    fn get_random_room_point(&self, room: &Rect, rng: &mut RandomNumberGenerator) -> (i32, i32) {
        // Generate random x within room.x1 + 1 and room.x2 - 1 (inclusive)
        let x = rng.roll_dice(1, room.width() - 2) + room.x1 + 1;
        // Generate random y within room.y1 + 1 and room.y2 - 1 (inclusive)
        let y = rng.roll_dice(1, room.height() - 2) + room.y1 + 1;
        (x, y)
    }
}