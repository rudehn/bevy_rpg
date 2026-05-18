//! Connects rooms with corridors.

use bracket_lib::prelude::{DistanceAlg, Point, Rect};
use std::collections::HashSet;

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::map::Map;
use crate::map::tile::TerrainType;

/// Carve a straight corridor between two points. Returns the indices carved.
pub fn draw_corridor(map: &mut Map, x1: i32, y1: i32, x2: i32, y2: i32) -> Vec<usize> {
    let mut corridor = Vec::new();
    let mut x = x1;
    let mut y = y1;

    while x != x2 || y != y2 {
        if x < x2 {
            x += 1;
        } else if x > x2 {
            x -= 1;
        } else if y < y2 {
            y += 1;
        } else if y > y2 {
            y -= 1;
        }

        let pt = Point::new(x, y);
        if map.get_tile(pt).map(|t| t.terrain) != Some(TerrainType::Floor) {
            let idx = map.xy_idx(pt.x, pt.y);
            corridor.push(idx);
            map.set_tile(pt, TerrainType::Floor);
        }
    }
    corridor
}

#[derive(Clone)]
pub struct NearestCorridors;

impl NearestCorridors {
    pub fn new() -> Self {
        NearestCorridors
    }
}

impl<C: BuildContext> MapBuilder<C> for NearestCorridors {
    fn name(&self) -> &'static str {
        "NearestCorridors"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Geometry)
    }
    fn build(&mut self, ctx: &mut C) {
        let rooms: Vec<Rect> = match ctx.rooms() {
            Some(r) => r.clone(),
            None => return,
        };

        let mut connected: HashSet<usize> = HashSet::new();
        for (i, room) in rooms.iter().enumerate() {
            let mut room_distance: Vec<(usize, f32)> = Vec::new();
            let room_center = room.center();
            let room_center_pt = Point::new(room_center.x, room_center.y);
            for (j, other_room) in rooms.iter().enumerate() {
                if i != j && !connected.contains(&j) {
                    let other_center = other_room.center();
                    let other_center_pt = Point::new(other_center.x, other_center.y);
                    let distance = DistanceAlg::Pythagoras.distance2d(room_center_pt, other_center_pt);
                    room_distance.push((j, distance));
                }
            }

            if !room_distance.is_empty() {
                room_distance.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                let dest_center = rooms[room_distance[0].0].center();
                draw_corridor(
                    ctx.map_mut(),
                    room_center.x,
                    room_center.y,
                    dest_center.x,
                    dest_center.y,
                );
                connected.insert(i);
                ctx.take_snapshot();
            }
        }
    }
}
