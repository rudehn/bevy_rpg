use std::collections::{HashSet, VecDeque};

use crate::{
    assets::MonsterSpawnInfo,
    game::squad::{LeaderDeathBehavior, SquadConfig},
    map::{builders::{BuilderMap, BuilderPhase, MetaMapBuilder, SpawnEntry}, map::Map, tile::{is_walkable, LiquidType, TerrainType}},
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

    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Spawning) }
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

        let Some(rooms) = build_data.rooms.clone() else {
            warn!("MonsterSpawner: rooms not set, skipping");
            return;
        };

        // Collect spawn entries into a local vec to avoid borrow conflicts
        // (the map borrow and squad_counter.next() both need build_data).
        let mut new_spawns: Vec<SpawnEntry> = Vec::new();

        let map = &build_data.map;
        for room in rooms.iter() {
                // 50% chance per room
                if rng.roll_dice(1, 2) == 1 {
                    let spawn_index = rng.range(0, possible_spawns.len());
                    let monster_info = &possible_spawns[spawn_index];

                    let origin = if monster_info.spawn_on_liquid {
                        self.get_liquid_room_point(room, map, &mut rng)
                    } else {
                        self.get_walkable_room_point(room, map, &mut rng)
                    };
                    if let Some(origin) = origin {
                        let squad_config = SquadConfig {
                            on_leader_death: LeaderDeathBehavior::from_str(
                                &monster_info.on_leader_death,
                            ),
                            flee_threshold: monster_info.flee_threshold,
                        };

                        if !monster_info.group.is_empty() {
                            let mut members: Vec<String> = Vec::new();
                            for gm in &monster_info.group {
                                let count = if gm.max_count > gm.min_count {
                                    rng.range(gm.min_count, gm.max_count + 1)
                                } else {
                                    gm.min_count
                                };
                                for _ in 0..count {
                                    members.push(gm.monster.clone());
                                }
                            }

                            let points =
                                find_cluster_points(origin, members.len(), map, &occupied, monster_info.spawn_on_liquid);

                            let squad_id = build_data.squad_counter.next();
                            for (i, (pt, name)) in
                                points.iter().zip(members.iter()).enumerate()
                            {
                                occupied.insert(map.xy_idx(pt.x, pt.y));
                                new_spawns.push(SpawnEntry::squad(
                                    *pt, name.clone(), squad_id, squad_config.clone(), i == 0,
                                ));
                            }
                        } else {
                            let group_size =
                                if monster_info.max_group > monster_info.min_group {
                                    rng.range(monster_info.min_group, monster_info.max_group + 1)
                                } else {
                                    monster_info.min_group
                                } as usize;

                            let points =
                                find_cluster_points(origin, group_size, map, &occupied, monster_info.spawn_on_liquid);

                            if points.len() > 1 {
                                let squad_id = build_data.squad_counter.next();
                                for (i, pt) in points.iter().enumerate() {
                                    occupied.insert(map.xy_idx(pt.x, pt.y));
                                    new_spawns.push(SpawnEntry::squad(
                                        *pt, monster_info.monster.clone(), squad_id, squad_config.clone(), i == 0,
                                    ));
                                }
                            } else {
                                for pt in &points {
                                    occupied.insert(map.xy_idx(pt.x, pt.y));
                                    new_spawns.push(SpawnEntry::solo(
                                        *pt, monster_info.monster.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }

        // Apply collected spawns via accessor.
        for entry in new_spawns {
            build_data.add_monster_spawn(entry);
        }
    }

    /// Find a random tile WITH liquid in the room (for aquatic monsters).
    fn get_liquid_room_point(
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
            if is_walkable(map.tiles[idx])
                && map.tiles[idx].liquid != LiquidType::None
                && !matches!(map.tiles[idx].terrain, TerrainType::UpStairs | TerrainType::DownStairs)
            {
                return Some(Point::new(x, y));
            }
        }
        None
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
            if is_walkable(map.tiles[idx])
                && map.tiles[idx].liquid == LiquidType::None
                && !matches!(map.tiles[idx].terrain, TerrainType::UpStairs | TerrainType::DownStairs)
            {
                return Some(Point::new(x, y));
            }
        }
        None
    }
}

/// BFS outward from `origin` to find up to `count` walkable, unoccupied tiles.
/// Uses cardinal directions only for tight cluster placement.
/// When `liquid_only` is true, tiles must have liquid (for aquatic monsters);
/// otherwise tiles must be dry (no liquid).
fn find_cluster_points(
    origin: Point,
    count: usize,
    map: &Map,
    occupied: &HashSet<usize>,
    liquid_only: bool,
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
        let liquid_ok = if liquid_only {
            map.tiles[idx].liquid != LiquidType::None
        } else {
            map.tiles[idx].liquid == LiquidType::None
        };
        if is_walkable(map.tiles[idx]) && liquid_ok && !occupied.contains(&idx) {
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
                if is_walkable(map.tiles[nidx]) && map.tiles[nidx].liquid != LiquidType::Lava && !occupied.contains(&nidx) {
                    queue.push_back(Point::new(nx, ny));
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Tile, Decoration};

    fn make_map(width: i32, height: i32, tiles: Vec<Tile>) -> Map {
        let count = (width * height) as usize;
        assert_eq!(tiles.len(), count);
        Map {
            name: "test".to_string(),
            tiles,
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth: 1,
        }
    }

    fn floor() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::None, decoration: Decoration::None }
    }

    fn deep_water() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Water, decoration: Decoration::None }
    }

    fn wall() -> Tile {
        Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None }
    }

    // ---- find_cluster_points (dry mode) ----

    #[test]
    fn cluster_finds_dry_floor_tiles() {
        // 5x5 all floor
        let map = make_map(5, 5, vec![floor(); 25]);
        let occupied = HashSet::new();
        let points = find_cluster_points(Point::new(2, 2), 3, &map, &occupied, false);
        assert_eq!(points.len(), 3);
        // All points should be dry floor
        for pt in &points {
            let idx = map.xy_idx(pt.x, pt.y);
            assert_eq!(map.tiles[idx].liquid, LiquidType::None);
        }
    }

    #[test]
    fn cluster_skips_occupied_tiles() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mut occupied = HashSet::new();
        // Occupy all but origin
        for i in 0..9 {
            if i != 4 { occupied.insert(i); }
        }
        let points = find_cluster_points(Point::new(1, 1), 3, &map, &occupied, false);
        assert_eq!(points.len(), 1); // Only origin is available
    }

    #[test]
    fn cluster_skips_walls() {
        // 3x3: center is floor, everything else is wall
        let mut tiles = vec![wall(); 9];
        tiles[4] = floor();
        let map = make_map(3, 3, tiles);
        let points = find_cluster_points(Point::new(1, 1), 5, &map, &HashSet::new(), false);
        assert_eq!(points.len(), 1); // Only center is walkable
    }

    // ---- find_cluster_points (liquid_only mode) ----

    #[test]
    fn cluster_liquid_only_finds_water_tiles() {
        // 5x5: center 3x3 is deep water, border is floor
        let mut tiles = vec![floor(); 25];
        for y in 1..4 {
            for x in 1..4 {
                tiles[(y * 5 + x) as usize] = deep_water();
            }
        }
        let map = make_map(5, 5, tiles);
        let points = find_cluster_points(Point::new(2, 2), 4, &map, &HashSet::new(), true);
        assert_eq!(points.len(), 4);
        for pt in &points {
            let idx = map.xy_idx(pt.x, pt.y);
            assert_eq!(map.tiles[idx].liquid, LiquidType::Water);
        }
    }

    #[test]
    fn cluster_liquid_only_skips_dry_tiles() {
        // 3x3: only center is water, rest is dry floor
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let points = find_cluster_points(Point::new(1, 1), 3, &map, &HashSet::new(), true);
        assert_eq!(points.len(), 1); // Only center has water
    }

    #[test]
    fn cluster_liquid_only_no_water_returns_empty() {
        // 3x3 all dry floor — origin has no water
        let map = make_map(3, 3, vec![floor(); 9]);
        let points = find_cluster_points(Point::new(1, 1), 3, &map, &HashSet::new(), true);
        assert!(points.is_empty());
    }
}
