use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::Position,
    game::AppState,
    map::{Map, tile::{is_opaque, TerrainType}},
};

// --- Resource ---

/// Per-tile light intensity, parallel to `Map.tiles`.
/// Values are in [0.0, 1.0]; 0.0 = no candle light, 1.0 = fully lit.
/// Built once per floor load; queried every frame by `update_tile_visibility`.
#[derive(Resource, Default)]
pub struct LightMap {
    pub values: Vec<f32>,
}

// --- Constants ---

pub const CANDLE_RADIUS: f32 = 30.0;

// --- Components ---

#[derive(Component)]
pub struct Candle;

#[derive(Component, Deref, DerefMut)]
pub struct AnimationTimer(pub Timer);

// --- Plugin ---

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightMap>().add_systems(
            Update,
            (
                rebuild_light_map_system,
                animate_candles,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// --- Systems ---

/// Rebuilds the full-floor light map when:
/// - New candles are spawned (floor load / save restore), OR
/// - The Map resource changes (door opened, terrain updated)
///   Skips if no candles are present yet to avoid rebuilding with empty data on floor-load frames.
pub fn rebuild_light_map_system(
    added_candles: Query<(), Added<Candle>>,
    all_candles: Query<&Position, With<Candle>>,
    map: Res<Map>,
    mut light_map: ResMut<LightMap>,
) {
    let needs_rebuild = !added_candles.is_empty() || map.is_changed();
    if !needs_rebuild || all_candles.is_empty() {
        return;
    }

    let n = (map.width * map.height) as usize;
    let mut values = vec![0.0f32; n];
    let radius_i = CANDLE_RADIUS.ceil() as i32;

    for pos in all_candles.iter() {
        let (cx, cy) = (pos.x, pos.y);
        // Candles sit on wall tiles. Trace LOS from the adjacent floor tile so
        // the ray doesn't immediately clip neighbouring wall tiles along the
        // same wall row, which would darken the room corners.
        let (lx, ly) = floor_neighbor(&map, cx, cy).unwrap_or((cx, cy));
        for ty in (cy - radius_i)..=(cy + radius_i) {
            for tx in (cx - radius_i)..=(cx + radius_i) {
                if !map.in_bounds(Point::new(tx, ty)) {
                    continue;
                }
                let dist = (((tx - cx).pow(2) + (ty - cy).pow(2)) as f32).sqrt();
                if dist > CANDLE_RADIUS {
                    continue;
                }
                if has_los(&map, lx, ly, tx, ty) {
                    let idx = map.xy_idx(tx, ty);
                    values[idx] = values[idx].max(1.0 - dist / CANDLE_RADIUS);
                }
            }
        }
    }

    *light_map = LightMap { values };
}

fn animate_candles(
    time: Res<Time>,
    mut query: Query<(&mut AnimationTimer, &mut Sprite), With<Candle>>,
) {
    for (mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.is_finished()
            && let Some(ref mut texture_atlas) = sprite.texture_atlas
        {
            texture_atlas.index = (texture_atlas.index + 1) % 4;
        }
    }
}

// --- LOS Helpers ---

/// Returns the first 4-directional floor-tile neighbour of a wall tile, or
/// `None` if there are none.  Used so LOS rays start from inside the room
/// rather than from the wall tile itself, preventing adjacent wall tiles from
/// casting spurious shadows on nearby floor corners.
fn floor_neighbor(map: &Map, x: i32, y: i32) -> Option<(i32, i32)> {
    for (nx, ny) in [(x, y + 1), (x, y - 1), (x + 1, y), (x - 1, y)] {
        let pt = Point::new(nx, ny);
        if map.in_bounds(pt) && map.tiles[map.xy_idx(nx, ny)].terrain == TerrainType::Floor {
            return Some((nx, ny));
        }
    }
    None
}

/// Bresenham line-of-sight from (x0,y0) to (x1,y1).
/// Intermediate tiles that are opaque block LOS; the destination tile is never
/// checked for opacity so that wall faces facing the candle still receive light.
fn has_los(map: &Map, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let (mut x, mut y) = (x0, y0);
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx - dy;
    loop {
        if x == x1 && y == y1 {
            return true;
        }
        // Skip the candle's own tile; check everything else
        if !(x == x0 && y == y0) && map.in_bounds(Point::new(x, y))
            && is_opaque(map.tiles[map.xy_idx(x, y)]) {
                return false;
            }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}
