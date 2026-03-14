use bracket_lib::prelude::{Algorithm2D, Point, RandomNumberGenerator};
use std::collections::{HashSet, VecDeque};

use crate::{
    assets::PrefabTemplate,
    game::squad::{SquadConfig, SquadIdCounter},
    map::tile::TerrainType,
};

use super::{BuilderMap, MetaMapBuilder, SpawnEntry};

/// Placement chance: ~40% per floor for MVP.
const PREFAB_CHANCE: i32 = 40;

pub struct PrefabPlacer {
    prefabs: Vec<PrefabTemplate>,
    squad_counter: SquadIdCounter,
}

impl MetaMapBuilder for PrefabPlacer {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.place_prefabs(build_data);
    }
}

impl PrefabPlacer {
    pub fn new(prefabs: Vec<PrefabTemplate>, squad_counter: SquadIdCounter) -> Box<Self> {
        Box::new(Self { prefabs, squad_counter })
    }

    fn place_prefabs(&mut self, build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        // Roll placement chance.
        if rng.range(0, 100) >= PREFAB_CHANCE {
            return;
        }

        // Filter prefabs eligible for this floor depth.
        let eligible: Vec<&PrefabTemplate> = self
            .prefabs
            .iter()
            .filter(|p| depth >= p.min_floor && depth <= p.max_floor)
            .collect();

        if eligible.is_empty() {
            return;
        }

        let prefab = eligible[rng.range(0, eligible.len() as i32) as usize];

        // Find a room large enough to contain the prefab.
        let Some(rooms) = build_data.rooms.as_ref() else {
            return;
        };

        let mut candidate_rooms: Vec<usize> = rooms
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                let rw = r.x2 - r.x1 + 1;
                let rh = r.y2 - r.y1 + 1;
                rw >= prefab.width && rh >= prefab.height
            })
            .map(|(i, _)| i)
            .collect();

        if candidate_rooms.is_empty() {
            return;
        }

        // Shuffle and try rooms until we find one that maintains connectivity.
        let n = candidate_rooms.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            candidate_rooms.swap(i as usize, j as usize);
        }

        for room_idx in &candidate_rooms {
            let room = &rooms[*room_idx];

            // Center the prefab within the room.
            let rw = room.x2 - room.x1 + 1;
            let rh = room.y2 - room.y1 + 1;
            let offset_x = room.x1 + (rw - prefab.width) / 2;
            let offset_y = room.y1 + (rh - prefab.height) / 2;

            // Snapshot the tiles we're about to overwrite.
            let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();

            for (py, row_str) in prefab.tiles.iter().enumerate() {
                for (px, ch) in row_str.chars().enumerate() {
                    let wx = offset_x + px as i32;
                    let wy = offset_y + py as i32;
                    let pt = Point::new(wx, wy);
                    if !build_data.map.in_bounds(pt) {
                        continue;
                    }
                    let idx = build_data.map.xy_idx(wx, wy);
                    snapshot.push((idx, build_data.map.tiles[idx]));

                    match ch {
                        '#' => build_data.map.tiles[idx].terrain = TerrainType::Wall,
                        '.' => build_data.map.tiles[idx].terrain = TerrainType::Floor,
                        '+' => build_data.map.tiles[idx].terrain = TerrainType::Door,
                        ' ' => {} // unchanged
                        _ => {}
                    }
                }
            }

            // Connectivity check: flood fill from starting position.
            let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
            if let Some(start) = start {
                if !check_connectivity(&build_data.map, start) {
                    // Restore snapshot and try next room.
                    for (idx, tile) in &snapshot {
                        build_data.map.tiles[*idx] = *tile;
                    }
                    continue;
                }
            }

            // Prefab placed successfully. Add spawns.

            // Assign a squad ID to all squad-linked monster spawns.
            let has_squad = prefab.monster_spawns.iter().any(|m| m.squad);
            let squad_id = if has_squad {
                let id = self.squad_counter.next();
                Some(id)
            } else {
                None
            };

            let squad_config = if has_squad {
                let behavior = crate::game::squad::LeaderDeathBehavior::from_str(&prefab.on_leader_death);
                Some(SquadConfig {
                    on_leader_death: behavior,
                    flee_threshold: prefab.flee_threshold,
                })
            } else {
                None
            };

            let mut is_first_squad_member = true;
            for ms in &prefab.monster_spawns {
                let wx = offset_x + ms.x;
                let wy = offset_y + ms.y;
                let pos = Point::new(wx, wy);

                if let Some(ref monster_name) = ms.monster {
                    let home = if ms.guard { Some(pos) } else { None };

                    let mut entry = if ms.squad {
                        if let (Some(sid), Some(cfg)) = (squad_id, squad_config.clone()) {
                            let leader = is_first_squad_member;
                            is_first_squad_member = false;
                            SpawnEntry::squad(pos, monster_name.clone(), sid, cfg, leader)
                        } else {
                            SpawnEntry::solo(pos, monster_name.clone())
                        }
                    } else {
                        SpawnEntry::solo(pos, monster_name.clone())
                    };
                    entry.home_position = home;
                    build_data.spawn_list.push(entry);
                }
            }

            // Props.
            for pe in &prefab.props {
                let wx = offset_x + pe.x;
                let wy = offset_y + pe.y;
                build_data.prop_spawn_list.push((Point::new(wx, wy), pe.prop.clone()));
            }

            // Items.
            for ie in &prefab.item_spawns {
                let wx = offset_x + ie.x;
                let wy = offset_y + ie.y;
                if let Some(ref item_name) = ie.item {
                    build_data.item_spawn_list.push((Point::new(wx, wy), item_name.clone(), 1));
                }
            }

            // Only place one prefab per floor for MVP.
            return;
        }
    }
}

/// Flood fill from `start` — returns true if all floor tiles are reachable.
fn check_connectivity(map: &crate::map::Map, start: Point) -> bool {
    let total_walkable = map.tiles.iter().filter(|t| {
        matches!(
            t.terrain,
            TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::OpenDoor | TerrainType::Door
        )
    }).count();

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    if map.in_bounds(start) {
        queue.push_back(start);
        visited.insert(map.point2d_to_index(start));
    }

    while let Some(pt) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let np = Point::new(pt.x + dx, pt.y + dy);
            if !map.in_bounds(np) {
                continue;
            }
            let idx = map.point2d_to_index(np);
            if visited.contains(&idx) {
                continue;
            }
            let terrain = map.tiles[idx].terrain;
            if matches!(
                terrain,
                TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::OpenDoor | TerrainType::Door
            ) {
                visited.insert(idx);
                queue.push_back(np);
            }
        }
    }

    visited.len() >= total_walkable
}
