use std::collections::{HashSet, VecDeque};

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

        // Track occupied tiles so hordes don't overlap each other or the player start.
        let mut occupied: HashSet<usize> = HashSet::new();
        if let Some(start) = &build_data.starting_position {
            occupied.insert(build_data.map.xy_idx(start.x, start.y));
        }

        if let Some(rooms) = &build_data.rooms {
            let map = &build_data.map;
            for room in rooms.iter() {
                // 50% chance per room
                if rng.roll_dice(1, 2) == 1 {
                    let spawn_index = rng.range(0, possible_spawns.len());
                    let monster_info = &possible_spawns[spawn_index];

                    if let Some(origin) = self.get_walkable_room_point(room, map, &mut rng) {
                        // Roll group size
                        let group_size = if monster_info.max_group > monster_info.min_group {
                            rng.range(monster_info.min_group, monster_info.max_group + 1)
                        } else {
                            monster_info.min_group
                        } as usize;

                        let points = find_cluster_points(origin, group_size, map, &occupied);

                        for pt in &points {
                            let idx = map.xy_idx(pt.x, pt.y);
                            occupied.insert(idx);
                            build_data
                                .spawn_list
                                .push((*pt, monster_info.monster.clone()));
                        }
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

/// BFS outward from `origin` to find up to `count` walkable, unoccupied tiles.
/// Uses cardinal directions only for tight cluster placement.
fn find_cluster_points(
    origin: Point,
    count: usize,
    map: &Map,
    occupied: &HashSet<usize>,
) -> Vec<Point> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    let origin_idx = map.xy_idx(origin.x, origin.y);
    queue.push_back(origin);
    visited.insert(origin_idx);

    let deltas: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];

    while let Some(pt) = queue.pop_front() {
        let idx = map.xy_idx(pt.x, pt.y);
        if is_walkable(map.tiles[idx]) && !occupied.contains(&idx) {
            result.push(pt);
            if result.len() >= count {
                break;
            }
        }

        for (dx, dy) in &deltas {
            let nx = pt.x + dx;
            let ny = pt.y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if !visited.contains(&nidx) {
                visited.insert(nidx);
                if is_walkable(map.tiles[nidx]) && !occupied.contains(&nidx) {
                    queue.push_back(Point::new(nx, ny));
                }
            }
        }
    }

    result
}
