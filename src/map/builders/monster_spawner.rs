use crate::{
    assets::MonsterSpawnInfo,
    map::{builders::{BuilderMap, MetaMapBuilder}, map::Map, tile::is_walkable},
};
use bevy::prelude::*;
use bracket_lib::prelude::{Point, RandomNumberGenerator, Rect};

pub struct MonsterSpawner {
    spawn_table: Vec<MonsterSpawnInfo>,
}

impl MetaMapBuilder for MonsterSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.spawn_monsters(build_data);
    }
}

impl MonsterSpawner {
    pub fn new(spawn_table: &[MonsterSpawnInfo]) -> Box<MonsterSpawner> {
        Box::new(MonsterSpawner {
            spawn_table: spawn_table.to_vec(),
        })
    }

    fn spawn_monsters(&mut self, build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        let possible_spawns: Vec<MonsterSpawnInfo> = self
            .spawn_table
            .iter()
            .filter(|spawn| depth >= spawn.min_floor && depth <= spawn.max_floor)
            .cloned()
            .collect();

        if possible_spawns.is_empty() {
            return;
        }

        if let Some(rooms) = &build_data.rooms {
            let map = &build_data.map;
            for room in rooms.iter() {
                if rng.roll_dice(1, 2) == 1 {
                    let spawn_index = rng.range(0, possible_spawns.len());
                    let monster_to_spawn = &possible_spawns[spawn_index];

                    if let Some(pt) = self.get_walkable_room_point(room, map, &mut rng) {
                        build_data
                            .spawn_list
                            .push((pt, monster_to_spawn.monster.clone()));
                    }
                }
            }
        }
    }

    fn get_walkable_room_point(
        &self,
        room: &Rect,
        map: &Map,
        rng: &mut RandomNumberGenerator,
    ) -> Option<Point> {
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
            if is_walkable(map.tiles[idx]) {
                return Some(Point::new(x, y));
            }
        }
        None
    }
}