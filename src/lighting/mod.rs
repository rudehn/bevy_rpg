//! Per-tile light accumulation.
//!
//! Computes a [`LightMap`] (intensity + tint per tile) by Bresenham
//! line-of-sight from each registered [`LightSourceData`]. The engine
//! does no rendering — games consume `LightMap` to drive sprite tints,
//! visibility shading, ASCII colors, etc.
//!
//! # Plugin contract
//!
//! [`LightingPlugin`] registers the `LightMap` and `LightSources`
//! resources and schedules `sync_entity_lights_system` →
//! `rebuild_light_map_system` (chained) inside [`LightingSet`]. Games
//! configure the set with `.run_if(...)` and `.after(...)` to fit it
//! into their own state machine and ordering.
//!
//! # Light sources
//!
//! Two flavours coexist:
//! - **Resource-driven** — system code (e.g. fire) calls
//!   `LightSources::add` directly. Useful for transient sources whose
//!   lifecycle isn't tied to an entity.
//! - **Entity-driven** — entities carry a [`LightSource`] component;
//!   `sync_entity_lights_system` mirrors them into `LightSources` each
//!   frame they change. Used for candles, glowing props, etc.

use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D as _, Point};

use crate::components::{Position, Viewshed};
use crate::map::map::Map;
use crate::map::tile::{is_opaque, TerrainType};

// ─── Resources ────────────────────────────────────────────────────────

/// Per-tile light data, parallel to `Map.tiles`.
/// Stores both intensity [0.0, 1.0] and tint color (from the dominant light source).
#[derive(Resource, Default)]
pub struct LightMap {
    pub values: Vec<f32>,
    /// RGB tint from the brightest light source at each tile.
    pub colors: Vec<[f32; 3]>,
}

/// The authoritative list of active light sources. The light map is rebuilt
/// from this resource — not from entity queries — so there are no timing
/// issues with deferred spawns/despawns.
///
/// Systems that add/remove lights (spawner, fire, etc.) update this resource
/// directly and set `dirty = true`.
#[derive(Resource, Default)]
pub struct LightSources {
    pub sources: Vec<LightSourceData>,
    pub dirty: bool,
}

/// A single light source's properties. Stored in [`LightSources`].
#[derive(Clone)]
pub struct LightSourceData {
    pub x: i32,
    pub y: i32,
    pub radius: f32,
    pub intensity: f32,
    pub color: [f32; 3],
    pub on_wall: bool,
}

impl LightSources {
    /// Add a light source and mark dirty.
    pub fn add(&mut self, data: LightSourceData) {
        self.sources.push(data);
        self.dirty = true;
    }

    /// Remove all light sources at the given position and mark dirty.
    /// Returns the number removed.
    pub fn remove_at(&mut self, x: i32, y: i32) -> usize {
        let before = self.sources.len();
        self.sources.retain(|s| s.x != x || s.y != y);
        let removed = before - self.sources.len();
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }

    /// Remove all non-wall sources (e.g. fire cleanup on floor change).
    pub fn remove_floor_sources(&mut self) {
        let before = self.sources.len();
        self.sources.retain(|s| s.on_wall);
        if self.sources.len() != before {
            self.dirty = true;
        }
    }
}

// ─── Components ──────────────────────────────────────────────────────

/// Generic light-emitting component for entity-driven lights (candles,
/// glowing props). Synced into [`LightSources`] by
/// [`sync_entity_lights_system`].
#[derive(Component, Clone)]
pub struct LightSource {
    pub radius: f32,
    pub intensity: f32,
    pub color: Color,
    pub on_wall: bool,
}

// ─── Constants & helpers ──────────────────────────────────────────────

pub const CANDLE_RADIUS: f32 = 30.0;

pub const FUNGAL_LIGHT_RADIUS: f32 = 8.0;
pub const FUNGAL_LIGHT_INTENSITY: f32 = 1.0;
pub const FUNGAL_LIGHT_COLOR: [f32; 3] = [0.2, 1.0, 0.3];

/// Create a fungal glow light source at the given position.
pub fn fungal_light(x: i32, y: i32) -> LightSourceData {
    LightSourceData {
        x,
        y,
        radius: FUNGAL_LIGHT_RADIUS,
        intensity: FUNGAL_LIGHT_INTENSITY,
        color: FUNGAL_LIGHT_COLOR,
        on_wall: false,
    }
}

// Phosphorescent moss: softer than fungus, cyan-green. Tuned to push
// stealth into the "noticeable but not blinding" band — at the edge of
// a patch, `light_modifier` lands around -1; standing on it hits -3.
pub const PHOSPHORESCENT_MOSS_LIGHT_RADIUS: f32 = 6.0;
pub const PHOSPHORESCENT_MOSS_LIGHT_INTENSITY: f32 = 0.7;
pub const PHOSPHORESCENT_MOSS_LIGHT_COLOR: [f32; 3] = [0.4, 1.0, 0.8];

/// Create a phosphorescent-moss glow light source at the given position.
/// Used by [`apply_decoration_mutations`](crate::map::mutation::apply_decoration_mutations)
/// to register / unregister the source when the `PhosphorescentMoss`
/// decoration appears or is replaced (e.g. by fire → Ash).
pub fn phosphorescent_moss_light(x: i32, y: i32) -> LightSourceData {
    LightSourceData {
        x,
        y,
        radius: PHOSPHORESCENT_MOSS_LIGHT_RADIUS,
        intensity: PHOSPHORESCENT_MOSS_LIGHT_INTENSITY,
        color: PHOSPHORESCENT_MOSS_LIGHT_COLOR,
        on_wall: false,
    }
}

// ─── Plugin ──────────────────────────────────────────────────────────

/// Empty marker for the lighting tick set. Games configure
/// `.run_if(...)` and `.after(...)` on this set to fit lighting into
/// their schedule (e.g. after dungeon spawn, while in-game).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LightingSet;

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightMap>()
            .init_resource::<LightSources>()
            .add_systems(
                Update,
                (sync_entity_lights_system, rebuild_light_map_system)
                    .chain()
                    .in_set(LightingSet),
            );
    }
}

// ─── Systems ─────────────────────────────────────────────────────────

/// Syncs [`LightSource`] entity components into the [`LightSources`] resource.
/// Handles candles and any future entity-based lights. Resource-driven
/// sources (e.g. fire) manage [`LightSources`] directly.
pub fn sync_entity_lights_system(
    query: Query<(&Position, &LightSource), Changed<LightSource>>,
    added: Query<(&Position, &LightSource), Added<LightSource>>,
    mut removed: RemovedComponents<LightSource>,
    all_lights: Query<(&Position, &LightSource)>,
    mut light_sources: ResMut<LightSources>,
) {
    let has_changes = !added.is_empty() || removed.read().next().is_some() || !query.is_empty();
    if !has_changes {
        return;
    }

    // Full resync of entity-based (on_wall) lights. Simple and correct.
    // Remove all wall-mounted sources and re-add from entities.
    light_sources.sources.retain(|s| !s.on_wall);
    for (pos, light) in all_lights.iter() {
        if !light.on_wall {
            continue; // resource-managed sources (e.g. fire) handled elsewhere
        }
        let srgba = light.color.to_srgba();
        light_sources.sources.push(LightSourceData {
            x: pos.x,
            y: pos.y,
            radius: light.radius,
            intensity: light.intensity,
            color: [srgba.red, srgba.green, srgba.blue],
            on_wall: true,
        });
    }
    light_sources.dirty = true;
}

/// Rebuilds the light map from the [`LightSources`] resource.
/// Marks all viewsheds dirty so visibility is recomputed with new light values.
pub fn rebuild_light_map_system(
    mut light_sources: ResMut<LightSources>,
    map: Res<Map>,
    mut light_map: ResMut<LightMap>,
    mut viewshed_query: Query<&mut Viewshed>,
) {
    if !light_sources.dirty {
        return;
    }
    light_sources.dirty = false;

    let n = (map.width * map.height) as usize;
    let mut values = vec![0.0f32; n];
    let mut colors = vec![[1.0f32, 1.0, 1.0]; n];

    for source in &light_sources.sources {
        let (lx, ly) = if source.on_wall {
            floor_neighbor(&map, source.x, source.y).unwrap_or((source.x, source.y))
        } else {
            (source.x, source.y)
        };
        add_light_source(
            &map,
            &mut values,
            &mut colors,
            source.x,
            source.y,
            lx,
            ly,
            source.radius,
            source.intensity,
            source.color,
        );
    }

    *light_map = LightMap { values, colors };

    // Force viewshed re-evaluation so consumers downstream pick up the new
    // light values this frame (their own update systems trigger on
    // `Changed<Viewshed>`).
    for mut viewshed in viewshed_query.iter_mut() {
        viewshed.dirty = true;
    }
}

/// Accumulate light from a single source. Bresenham LOS prevents bleeding through walls.
fn add_light_source(
    map: &Map,
    values: &mut [f32],
    colors: &mut [[f32; 3]],
    source_x: i32,
    source_y: i32,
    los_x: i32,
    los_y: i32,
    radius: f32,
    intensity: f32,
    color: [f32; 3],
) {
    let radius_i = radius.ceil() as i32;
    for ty in (source_y - radius_i)..=(source_y + radius_i) {
        for tx in (source_x - radius_i)..=(source_x + radius_i) {
            if !map.in_bounds(Point::new(tx, ty)) {
                continue;
            }
            let dist = (((tx - source_x).pow(2) + (ty - source_y).pow(2)) as f32).sqrt();
            if dist > radius {
                continue;
            }
            if !has_los(map, los_x, los_y, tx, ty) {
                continue;
            }
            let idx = map.xy_idx(tx, ty);
            let new_val = intensity * (1.0 - dist / radius);
            if new_val > values[idx] {
                values[idx] = new_val;
                colors[idx] = color;
            }
        }
    }
}

// ─── LOS Helpers ──────────────────────────────────────────────────────

fn floor_neighbor(map: &Map, x: i32, y: i32) -> Option<(i32, i32)> {
    for (nx, ny) in [(x, y + 1), (x, y - 1), (x + 1, y), (x - 1, y)] {
        let pt = Point::new(nx, ny);
        if map.in_bounds(pt) && map.tiles[map.xy_idx(nx, ny)].terrain == TerrainType::Floor {
            return Some((nx, ny));
        }
    }
    None
}

fn has_los(map: &Map, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let (mut x, mut y) = (x0, y0);
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx - dy;
    loop {
        if x == x1 && y == y1 {
            return true;
        }
        if !(x == x0 && y == y0)
            && map.in_bounds(Point::new(x, y))
            && is_opaque(map.tiles[map.xy_idx(x, y)])
        {
            return false;
        }
        let e2 = 2 * err;
        let step_x = e2 > -dy;
        let step_y = e2 < dx;

        // Block diagonal steps through wall corners: if both cells
        // adjacent to the diagonal are opaque, light cannot pass.
        if step_x && step_y {
            let adj_h = Point::new(x + sx, y);
            let adj_v = Point::new(x, y + sy);
            let h_opaque =
                map.in_bounds(adj_h) && is_opaque(map.tiles[map.xy_idx(adj_h.x, adj_h.y)]);
            let v_opaque =
                map.in_bounds(adj_v) && is_opaque(map.tiles[map.xy_idx(adj_v.x, adj_v.y)]);
            if h_opaque && v_opaque {
                return false;
            }
        }

        if step_x {
            err -= dy;
            x += sx;
        }
        if step_y {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::map::Map;
    use crate::map::tile::TerrainType;

    /// Create a small test map and carve floor at given positions.
    fn test_map(width: i32, height: i32, floors: &[(i32, i32)]) -> Map {
        let mut map = Map::new(0, width, height, "test");
        for &(x, y) in floors {
            let idx = map.xy_idx(x, y);
            map.tiles[idx].terrain = TerrainType::Floor;
        }
        map
    }

    #[test]
    fn los_clear_path() {
        // 5x5, open corridor along row 2
        let floors: Vec<(i32, i32)> = (0..5).map(|x| (x, 2)).collect();
        let map = test_map(5, 5, &floors);
        assert!(has_los(&map, 0, 2, 4, 2));
    }

    #[test]
    fn los_blocked_by_wall() {
        // Wall in the middle of a corridor
        let map = test_map(5, 5, &[(0, 2), (1, 2), /* (2,2) is wall */ (3, 2), (4, 2)]);
        assert!(!has_los(&map, 0, 2, 4, 2));
    }

    #[test]
    fn los_blocked_by_diagonal_wall_corner() {
        // Two rooms connected only diagonally — light must not leak through:
        //   row 0: # # # #
        //   row 1: # . # #   ← room 1 at (1,1)
        //   row 2: # # . #   ← room 2 at (2,2)
        //   row 3: # # # #
        // Walls at (2,1) and (1,2) form a diagonal corner.
        let map = test_map(4, 4, &[(1, 1), (2, 2)]);
        assert!(!has_los(&map, 1, 1, 2, 2));
    }

    #[test]
    fn los_passes_diagonal_when_one_side_open() {
        // Only one adjacent cell is a wall → light can pass:
        //   row 1: . . .
        //   row 2: . . .
        //   row 3: . . .
        // All floor — diagonal should work fine.
        let floors: Vec<(i32, i32)> = (0..3)
            .flat_map(|y| (0..3).map(move |x| (x, y + 1)))
            .collect();
        let map = test_map(4, 5, &floors);
        assert!(has_los(&map, 0, 1, 2, 3));
    }

    #[test]
    fn add_then_remove_clears_resource() {
        let mut sources = LightSources::default();
        sources.add(fungal_light(3, 4));
        assert_eq!(sources.sources.len(), 1);
        assert!(sources.dirty);

        // Reset dirty for the next assertion
        sources.dirty = false;
        let removed = sources.remove_at(3, 4);
        assert_eq!(removed, 1);
        assert!(sources.dirty);
        assert!(sources.sources.is_empty());
    }

    #[test]
    fn remove_floor_sources_keeps_walls() {
        let mut sources = LightSources::default();
        sources.add(fungal_light(1, 1)); // on_wall: false
        sources.add(LightSourceData {
            x: 2,
            y: 2,
            radius: 5.0,
            intensity: 1.0,
            color: [1.0, 1.0, 1.0],
            on_wall: true,
        });
        sources.dirty = false;
        sources.remove_floor_sources();
        assert_eq!(sources.sources.len(), 1);
        assert!(sources.sources[0].on_wall);
        assert!(sources.dirty);
    }
}
