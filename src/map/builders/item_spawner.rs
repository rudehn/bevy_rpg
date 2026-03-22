use bevy::prelude::*;
use crate::map::{builders::{BuilderMap, MetaMapBuilder}, map::Map, tile::{is_walkable, LiquidType}};
use bracket_lib::prelude::{Point, RandomNumberGenerator, Rect};

pub struct ItemSpawner;

impl MetaMapBuilder for ItemSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.spawn_chests(build_data);
    }
}

impl ItemSpawner {
    pub fn new() -> Box<ItemSpawner> {
        Box::new(ItemSpawner)
    }

    fn spawn_chests(&mut self, build_data: &mut BuilderMap) {
        let mut rng = RandomNumberGenerator::new();

        let Some(rooms) = build_data.rooms.clone() else {
            warn!("ItemSpawner: rooms not set, skipping");
            return;
        };

        let mut pending_props: Vec<(Point, String)> = Vec::new();
        {
            let map = &build_data.map;
            for room in rooms.iter() {
                // ~50% chance per room to place a chest
                if rng.range(0, 100) >= 50 {
                    continue;
                }

                if let Some(pt) = walkable_room_point(room, map, &mut rng) {
                    pending_props.push((pt, "chest".to_string()));
                }
            }
        }
        for (pt, name) in pending_props {
            build_data.add_prop_spawn(pt, name);
        }
    }
}

fn walkable_room_point(room: &Rect, map: &Map, rng: &mut RandomNumberGenerator) -> Option<Point> {
    for _ in 0..20 {
        let x = if room.width() > 2 {
            rng.roll_dice(1, room.width() - 2) + room.x1 + 1
        } else {
            room.x1 + 1
        };
        let y = if room.height() > 2 {
            rng.roll_dice(1, room.height() - 2) + room.y1 + 1
        } else {
            room.y1 + 1
        };
        let idx = map.xy_idx(x, y);
        if is_walkable(map.tiles[idx]) && map.tiles[idx].liquid == LiquidType::None {
            return Some(Point::new(x, y));
        }
    }
    None
}
