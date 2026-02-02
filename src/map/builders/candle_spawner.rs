use bracket_lib::prelude::{Point, RandomNumberGenerator, Rect};

use super::{BuilderMap, MetaMapBuilder};

pub struct CandleSpawner;

impl MetaMapBuilder for CandleSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl CandleSpawner {
    #[allow(dead_code)]
    pub fn new() -> Box<CandleSpawner> {
        Box::new(CandleSpawner {})
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        if let Some(rooms) = &build_data.rooms {
            let mut rng = RandomNumberGenerator::new();
            for room in rooms.iter() {
                let center_x = (room.x1 + room.x2) / 2;
                let center_y = (room.y1 + room.y2) / 2;
                let center_pt = Point::new(center_x, center_y);

                // Find a random wall tile for the candle
                let mut found_wall = false;
                let mut attempts = 0;
                while !found_wall && attempts < 100 {
                    let wall_x = rng.roll_dice(1, room.width()) + room.x1;
                    let wall_y = rng.roll_dice(1, room.height()) + room.y1;
                    let wall_pt = Point::new(wall_x, wall_y);

                    // Check if it's on the perimeter of the room (a wall)
                    if (wall_x == room.x1 || wall_x == room.x2 || wall_y == room.y1 || wall_y == room.y2)
                        // And not a corner (to avoid potential pathing issues)
                        && !((wall_x == room.x1 && wall_y == room.y1)
                            || (wall_x == room.x1 && wall_y == room.y2)
                            || (wall_x == room.x2 && wall_y == room.y1)
                            || (wall_x == room.x2 && wall_y == room.y2))
                    {
                        // Ensure it's not the center of the room (where player might spawn)
                        if wall_pt != center_pt {
                             build_data.candle_spawn_points.push(wall_pt);
                             found_wall = true;
                        }
                    }
                    attempts += 1;
                }
            }
        }
    }
}
