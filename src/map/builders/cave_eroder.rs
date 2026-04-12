//! Erodes boxy wall edges for a more organic, cavernous feel.

use std::collections::VecDeque;

use bracket_lib::prelude::{Algorithm2D, Point};

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::map::Map;
use crate::map::tile::TerrainType;

pub struct CaveEroder {
    erosion_percent: i32,
}

impl CaveEroder {
    pub fn new() -> Self {
        Self { erosion_percent: 40 }
    }

    pub fn with_erosion(erosion_percent: i32) -> Self {
        Self { erosion_percent }
    }

    fn count_floor_neighbors(&self, map: &Map, x: i32, y: i32) -> i32 {
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

    fn check_connectivity(&self, map: &Map, start: Point) -> bool {
        let total_walkable = map.tiles.iter().filter(|t| {
            matches!(
                t.terrain,
                TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs
                | TerrainType::OpenDoor | TerrainType::Door
            )
        }).count();

        let mut visited = vec![false; map.tiles.len()];
        let mut queue = VecDeque::new();
        let mut visited_count = 0usize;

        if map.in_bounds(start) {
            let idx = map.point2d_to_index(start);
            queue.push_back(idx);
            visited[idx] = true;
        }

        while let Some(current) = queue.pop_front() {
            visited_count += 1;
            let (cx, cy) = map.idx_xy(current);
            for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
                let idx = map.xy_idx(nx, ny);
                if visited[idx] { continue; }
                if matches!(
                    map.tiles[idx].terrain,
                    TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs
                    | TerrainType::OpenDoor | TerrainType::Door
                ) {
                    visited[idx] = true;
                    queue.push_back(idx);
                }
            }
        }

        visited_count >= total_walkable
    }
}

impl<C: BuildContext> MapBuilder<C> for CaveEroder {
    fn name(&self) -> &'static str { "CaveEroder" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::TerrainCleanup) }
    fn build(&mut self, ctx: &mut C) {
        let w = ctx.map().width;
        let h = ctx.map().height;

        // Pass 1: selective erosion
        let mut candidates: Vec<usize> = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let idx = ctx.map().xy_idx(x, y);
                if ctx.map().tiles[idx].terrain != TerrainType::Wall { continue; }
                if self.count_floor_neighbors(ctx.map(), x, y) >= 3 {
                    candidates.push(idx);
                }
            }
        }

        let mut eroded: Vec<usize> = Vec::new();
        for &idx in &candidates {
            if ctx.rng().range(0, 100) < self.erosion_percent {
                eroded.push(idx);
            }
        }

        for &idx in &eroded {
            ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
        }

        // Connectivity check — revert if erosion broke it
        if let Some(start) = ctx.starting_position() {
            let start_pt = Point::new(start.x, start.y);
            if !self.check_connectivity(ctx.map(), start_pt) {
                for &idx in &eroded {
                    ctx.map_mut().tiles[idx].terrain = TerrainType::Wall;
                }
                return;
            }
        }

        // Pass 2: remove isolated wall pillars (6+ floor neighbors)
        let mut pillars: Vec<usize> = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let idx = ctx.map().xy_idx(x, y);
                if ctx.map().tiles[idx].terrain == TerrainType::Wall {
                    if self.count_floor_neighbors(ctx.map(), x, y) >= 6 {
                        pillars.push(idx);
                    }
                }
            }
        }
        for &idx in &pillars {
            ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
        }
    }
}
