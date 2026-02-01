use crate::map::{tile::TileType, Map};
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;
use bracket_lib::prelude::{Algorithm2D, BaseMap, Point};

/// A read-only adapter to view a `bevy_ecs_tilemap` as a `Map` trait object.
/// This allows pathfinding and other algorithms to run on the live ECS data.
/// It is constructed within a Bevy system.
pub struct EcsMap<'w, 's, 'a> {
    pub tile_storage: &'w TileStorage,
    pub tile_query: &'w Query<'w, 's, &'a TileType>,
    pub map_size: TilemapSize,
}

impl<'w, 's, 'a> Map for EcsMap<'w, 's, 'a> {
    fn width(&self) -> i32 {
        self.map_size.x as i32
    }

    fn height(&self) -> i32 {
        self.map_size.y as i32
    }

    fn get_tile(&self, pt: Point) -> Option<TileType> {
        if !self.in_bounds(pt) {
            return None;
        }
        let tile_pos = TilePos {
            x: pt.x as u32,
            y: pt.y as u32,
        };
        self.tile_storage
            .get(&tile_pos)
            .and_then(|tile_entity| self.tile_query.get(tile_entity).ok().copied())
    }

    /// This is a read-only adapter. Setting tiles must be done via Commands.
    fn set_tile(&mut self, _pt: Point, _tile: TileType) {
        panic!("EcsMap is a read-only adapter. Use Commands to modify the map.");
    }
}

impl<'w, 's, 'a> BaseMap for EcsMap<'w, 's, 'a> {
    fn is_opaque(&self, idx: usize) -> bool {
        let pt = self.index_to_point2d(idx);
        match self.get_tile(pt) {
            Some(TileType::Wall) => true,
            _ => false,
        }
    }

    fn get_available_exits(&self, idx: usize) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
        let mut exits = bracket_lib::prelude::SmallVec::new();
        let pt = self.index_to_point2d(idx);

        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }

                let next_pt = Point::new(pt.x + dx, pt.y + dy);
                if self.in_bounds(next_pt) {
                    if let Some(tile) = self.get_tile(next_pt) {
                        if !matches!(tile, TileType::Wall) {
                            let next_idx = self.point2d_to_index(next_pt);
                            let distance = if dx == 0 || dy == 0 { 1.0 } else { 1.45 };
                            exits.push((next_idx, distance));
                        }
                    }
                }
            }
        }

        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let p1 = self.index_to_point2d(idx1);
        let p2 = self.index_to_point2d(idx2);
        bracket_lib::prelude::DistanceAlg::Pythagoras.distance2d(p1, p2)
    }
}

impl<'w, 's, 'a> Algorithm2D for EcsMap<'w, 's, 'a> {
    fn dimensions(&self) -> Point {
        Point::new(self.map_size.x as i32, self.map_size.y as i32)
    }

    fn point2d_to_index(&self, pt: Point) -> usize {
        (pt.y as usize * self.map_size.x as usize) + pt.x as usize
    }

    fn index_to_point2d(&self, idx: usize) -> Point {
        Point::new(
            idx as i32 % self.width(),
            idx as i32 / self.width(),
        )
    }
}
