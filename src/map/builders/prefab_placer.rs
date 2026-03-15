use bracket_lib::prelude::{Algorithm2D, Point, RandomNumberGenerator};
use std::collections::{HashSet, VecDeque};

use crate::{
    assets::PrefabTemplate,
    game::squad::SquadConfig,
    map::tile::TerrainType,
};

use super::{BuilderMap, MetaMapBuilder, SpawnEntry};

/// Placement chance: ~40% per floor for MVP.
const PREFAB_CHANCE: i32 = 40;

pub struct PrefabPlacer {
    prefabs: Vec<PrefabTemplate>,
}

impl MetaMapBuilder for PrefabPlacer {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.place_prefabs(build_data);
    }
}

impl PrefabPlacer {
    pub fn new(prefabs: Vec<PrefabTemplate>) -> Box<Self> {
        Box::new(Self { prefabs })
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

        let prefab = eligible[rng.range(0, eligible.len() as i32) as usize].clone();

        let try_room = prefab.placement != "wall";
        let try_wall = prefab.placement != "room";

        // Try room-overlay placement first.
        if try_room && self.try_room_placement(build_data, &prefab, &mut rng) {
            return;
        }

        // Try wall-carve placement.
        if try_wall {
            self.try_wall_carve_placement(build_data, &prefab, &mut rng);
        }
    }

    /// Original room-overlay placement: center the prefab inside a room large enough.
    fn try_room_placement(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        rng: &mut RandomNumberGenerator,
    ) -> bool {
        let Some(rooms) = build_data.rooms.as_ref() else {
            return false;
        };

        // Collect candidate offsets up front to avoid borrowing rooms while mutating build_data.
        let mut candidate_offsets: Vec<(i32, i32)> = rooms
            .iter()
            .filter_map(|r| {
                let rw = r.x2 - r.x1 + 1;
                let rh = r.y2 - r.y1 + 1;
                if rw >= prefab.width && rh >= prefab.height {
                    Some((r.x1 + (rw - prefab.width) / 2, r.y1 + (rh - prefab.height) / 2))
                } else {
                    None
                }
            })
            .collect();

        if candidate_offsets.is_empty() {
            return false;
        }

        // Shuffle candidates.
        let n = candidate_offsets.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            candidate_offsets.swap(i as usize, j as usize);
        }

        for (offset_x, offset_y) in &candidate_offsets {
            if self.try_stamp_prefab(build_data, prefab, *offset_x, *offset_y) {
                return true;
            }
        }

        false
    }

    /// Wall-carve placement: find a solid wall region adjacent to existing floor,
    /// carve the prefab into it, and add a door connection.
    fn try_wall_carve_placement(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        rng: &mut RandomNumberGenerator,
    ) -> bool {
        let map_w = build_data.map.width;
        let map_h = build_data.map.height;

        // Collect candidate positions where the prefab fits entirely in walls
        // and has at least one edge adjacent to floor.
        let mut candidates: Vec<(i32, i32)> = Vec::new();

        for oy in 1..map_h - prefab.height - 1 {
            for ox in 1..map_w - prefab.width - 1 {
                if self.wall_carve_fits(build_data, prefab, ox, oy) {
                    candidates.push((ox, oy));
                }
            }
        }

        if candidates.is_empty() {
            return false;
        }

        // Shuffle candidates.
        let n = candidates.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            candidates.swap(i as usize, j as usize);
        }

        for (ox, oy) in candidates {
            // Find connection point: a wall tile on the prefab border adjacent to existing floor.
            if let Some(door_pt) = self.find_connection_point(build_data, prefab, ox, oy) {
                // Snapshot
                let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();

                // Carve the prefab tiles
                for (py, row_str) in prefab.tiles.iter().enumerate() {
                    for (px, ch) in row_str.chars().enumerate() {
                        let wx = ox + px as i32;
                        let wy = oy + py as i32;
                        let pt = Point::new(wx, wy);
                        if !build_data.map.in_bounds(pt) { continue; }
                        let idx = build_data.map.xy_idx(wx, wy);
                        snapshot.push((idx, build_data.map.tiles[idx]));

                        match ch {
                            '#' => build_data.map.tiles[idx].terrain = TerrainType::Wall,
                            '.' => build_data.map.tiles[idx].terrain = TerrainType::Floor,
                            '+' => build_data.map.tiles[idx].terrain = TerrainType::Door,
                            _ => {}
                        }
                    }
                }

                // Place door at connection point
                let door_idx = build_data.map.xy_idx(door_pt.x, door_pt.y);
                snapshot.push((door_idx, build_data.map.tiles[door_idx]));
                build_data.map.tiles[door_idx].terrain = TerrainType::Door;

                // Connectivity check
                let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
                if let Some(start) = start {
                    if !check_connectivity(&build_data.map, start) {
                        for (idx, tile) in &snapshot {
                            build_data.map.tiles[*idx] = *tile;
                        }
                        continue;
                    }
                }

                // Success — add spawns
                self.add_prefab_spawns(build_data, prefab, ox, oy);
                return true;
            }
        }

        false
    }

    /// Check if the prefab footprint is entirely wall tiles (suitable for carving).
    fn wall_carve_fits(&self, build_data: &BuilderMap, prefab: &PrefabTemplate, ox: i32, oy: i32) -> bool {
        for py in 0..prefab.height {
            for px in 0..prefab.width {
                let wx = ox + px;
                let wy = oy + py;
                let pt = Point::new(wx, wy);
                if !build_data.map.in_bounds(pt) { return false; }
                let idx = build_data.map.xy_idx(wx, wy);
                if build_data.map.tiles[idx].terrain != TerrainType::Wall {
                    return false;
                }
            }
        }
        true
    }

    /// Find a tile on the prefab border that is adjacent to existing floor in the dungeon.
    /// Returns the wall tile that should become a door.
    fn find_connection_point(&self, build_data: &BuilderMap, prefab: &PrefabTemplate, ox: i32, oy: i32) -> Option<Point> {
        let deltas = [(0, -1), (0, 1), (-1, 0), (1, 0)];

        // Check all border tiles of the prefab footprint
        for py in 0..prefab.height {
            for px in 0..prefab.width {
                // Only border tiles
                if px > 0 && px < prefab.width - 1 && py > 0 && py < prefab.height - 1 {
                    continue;
                }

                let wx = ox + px;
                let wy = oy + py;

                // Check if any neighbor outside the prefab is floor
                for (dx, dy) in &deltas {
                    let nx = wx + dx;
                    let ny = wy + dy;
                    // Must be outside the prefab footprint
                    if nx >= ox && nx < ox + prefab.width && ny >= oy && ny < oy + prefab.height {
                        continue;
                    }
                    let pt = Point::new(nx, ny);
                    if !build_data.map.in_bounds(pt) { continue; }
                    let idx = build_data.map.xy_idx(nx, ny);
                    if build_data.map.tiles[idx].terrain == TerrainType::Floor {
                        return Some(Point::new(wx, wy));
                    }
                }
            }
        }
        None
    }

    /// Stamp a prefab at the given offset with connectivity check and snapshot-revert.
    fn try_stamp_prefab(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        offset_x: i32,
        offset_y: i32,
    ) -> bool {
        let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();

        for (py, row_str) in prefab.tiles.iter().enumerate() {
            for (px, ch) in row_str.chars().enumerate() {
                let wx = offset_x + px as i32;
                let wy = offset_y + py as i32;
                let pt = Point::new(wx, wy);
                if !build_data.map.in_bounds(pt) { continue; }
                let idx = build_data.map.xy_idx(wx, wy);
                snapshot.push((idx, build_data.map.tiles[idx]));

                match ch {
                    '#' => build_data.map.tiles[idx].terrain = TerrainType::Wall,
                    '.' => build_data.map.tiles[idx].terrain = TerrainType::Floor,
                    '+' => build_data.map.tiles[idx].terrain = TerrainType::Door,
                    ' ' => {}
                    _ => {}
                }
            }
        }

        // Connectivity check
        let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
        if let Some(start) = start {
            if !check_connectivity(&build_data.map, start) {
                for (idx, tile) in &snapshot {
                    build_data.map.tiles[*idx] = *tile;
                }
                return false;
            }
        }

        // Success — add spawns.
        self.add_prefab_spawns(build_data, prefab, offset_x, offset_y);
        true
    }

    /// Add monster, prop, and item spawns for a successfully placed prefab.
    fn add_prefab_spawns(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        offset_x: i32,
        offset_y: i32,
    ) {
        let has_squad = prefab.monster_spawns.iter().any(|m| m.squad);
        let squad_id = if has_squad {
            Some(build_data.squad_counter.next())
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

        for pe in &prefab.props {
            let wx = offset_x + pe.x;
            let wy = offset_y + pe.y;
            build_data.prop_spawn_list.push((Point::new(wx, wy), pe.prop.clone()));
        }

        for ie in &prefab.item_spawns {
            let wx = offset_x + ie.x;
            let wy = offset_y + ie.y;
            if let Some(ref item_name) = ie.item {
                build_data.item_spawn_list.push((Point::new(wx, wy), item_name.clone(), 1));
            }
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
