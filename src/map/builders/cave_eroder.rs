use std::collections::{HashSet, VecDeque};

use bracket_lib::prelude::{Algorithm2D, Point};

use crate::map::tile::TerrainType;

use super::{BuilderMap, FloorProfile, MetaMapBuilder};

/// Post-processing pass that erodes boxy wall edges and smooths isolated
/// wall pillars, giving the dungeon a more organic, cavernous feel.
pub struct CaveEroder {
    erosion_percent: i32,
}

impl MetaMapBuilder for CaveEroder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.erode(build_data);
    }
}

impl CaveEroder {
    #[allow(dead_code)]
    pub fn new() -> Box<Self> {
        Box::new(Self { erosion_percent: 40 })
    }

    pub fn with_profile(profile: FloorProfile) -> Box<Self> {
        Box::new(Self { erosion_percent: profile.erosion_percent })
    }

    fn erode(&self, build_data: &mut BuilderMap) {
        let w = build_data.map.width;
        let h = build_data.map.height;
        let mut rng = bracket_lib::random::RandomNumberGenerator::new();

        // Pass 1: Selective erosion — wall tiles with 3+ floor neighbors
        // have a 40% chance of becoming floor.
        let mut candidates: Vec<usize> = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if build_data.map.tiles[idx].terrain != TerrainType::Wall {
                    continue;
                }
                let floor_neighbors = self.count_floor_neighbors(&build_data.map, x, y);
                if floor_neighbors >= 3 {
                    candidates.push(idx);
                }
            }
        }

        let mut eroded: Vec<usize> = Vec::new();
        for &idx in &candidates {
            if rng.range(0, 100) < self.erosion_percent {
                eroded.push(idx);
            }
        }

        // Apply erosion
        for &idx in &eroded {
            build_data.map.tiles[idx].terrain = TerrainType::Floor;
        }

        // Connectivity check — revert if erosion broke connectivity
        if let Some(start) = &build_data.starting_position {
            let start_pt = Point::new(start.x, start.y);
            if !self.check_connectivity(&build_data.map, start_pt) {
                // Revert all erosion
                for &idx in &eroded {
                    build_data.map.tiles[idx].terrain = TerrainType::Wall;
                }
                return;
            }
        }

        // Pass 2: Remove isolated wall pillars (wall tiles with 6+ floor
        // neighbors in the 8-cell neighborhood).
        let mut pillars: Vec<usize> = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if build_data.map.tiles[idx].terrain == TerrainType::Wall {
                    let floor_neighbors = self.count_floor_neighbors(&build_data.map, x, y);
                    if floor_neighbors >= 6 {
                        pillars.push(idx);
                    }
                }
            }
        }
        for &idx in &pillars {
            build_data.map.tiles[idx].terrain = TerrainType::Floor;
        }
    }

    fn count_floor_neighbors(&self, map: &crate::map::Map, x: i32, y: i32) -> i32 {
        let mut count = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 { continue; }
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
                let idx = map.xy_idx(nx, ny);
                match map.tiles[idx].terrain {
                    TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs
                    | TerrainType::OpenDoor | TerrainType::Door => {
                        count += 1;
                    }
                    _ => {}
                }
            }
        }
        count
    }

    fn check_connectivity(&self, map: &crate::map::Map, start: Point) -> bool {
        let total_walkable = map.tiles.iter().filter(|t| {
            matches!(
                t.terrain,
                TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs
                | TerrainType::OpenDoor | TerrainType::Door
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
                if !map.in_bounds(np) { continue; }
                let idx = map.point2d_to_index(np);
                if visited.contains(&idx) { continue; }
                let terrain = map.tiles[idx].terrain;
                if matches!(
                    terrain,
                    TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs
                    | TerrainType::OpenDoor | TerrainType::Door
                ) {
                    visited.insert(idx);
                    queue.push_back(np);
                }
            }
        }

        visited.len() >= total_walkable
    }
}
