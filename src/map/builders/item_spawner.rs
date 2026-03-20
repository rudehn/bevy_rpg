use bevy::prelude::*;
use crate::{
    assets::ItemSpawnInfo,
    map::{builders::{BuilderMap, MetaMapBuilder}, map::Map, tile::{is_walkable, LiquidType}},
};
use bracket_lib::prelude::{Point, RandomNumberGenerator, Rect};

pub struct ItemSpawner {
    spawn_table: Vec<ItemSpawnInfo>,
}

impl MetaMapBuilder for ItemSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.spawn_items(build_data);
    }
}

impl ItemSpawner {
    pub fn new(spawn_table: &[ItemSpawnInfo]) -> Box<ItemSpawner> {
        Box::new(ItemSpawner {
            spawn_table: spawn_table.to_vec(),
        })
    }

    fn spawn_items(&mut self, build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        let candidates: Vec<&ItemSpawnInfo> = self
            .spawn_table
            .iter()
            .filter(|s| depth >= s.min_floor && depth <= s.max_floor)
            .collect();

        if candidates.is_empty() {
            return;
        }

        let total_weight: i32 = candidates.iter().map(|s| s.weight).sum();

        let Some(rooms) = build_data.rooms.clone() else {
            warn!("ItemSpawner: rooms not set, skipping");
            return;
        };

        let mut pending_spawns: Vec<(Point, String, u32)> = Vec::new();
        {
            let map = &build_data.map;
            for room in rooms.iter() {
                let roll = rng.range(0, total_weight);
                let mut acc = 0;
                let chosen = candidates.iter().find(|s| {
                    acc += s.weight;
                    roll < acc
                });

                if let Some(spawn_info) = chosen {
                    if let Some(pt) = walkable_room_point(room, map, &mut rng) {
                        let count = if spawn_info.max_count > 1 {
                            rng.range(spawn_info.min_count, spawn_info.max_count + 1)
                        } else {
                            1
                        };
                        pending_spawns.push((pt, spawn_info.item.clone(), count));
                    }
                }
            }
        }
        for (pt, name, count) in pending_spawns {
            build_data.add_item_spawn(pt, name, count);
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
