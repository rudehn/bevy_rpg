//! Field-of-view computation system and supporting types.
//!
//! Every entity with a [`Viewshed`] and a [`Position`] can have its
//! visible-tile set recomputed when `viewshed.dirty` is true. Entities
//! that also carry the [`FovRevealsMap`] marker (typically just the
//! player) additionally stamp their visible tiles into
//! [`Map::explored_tiles`] so the map renderer knows which tiles the
//! player has seen at least once.

use bevy::prelude::*;
use bracket_lib::prelude::{field_of_view_set, Point};

use crate::components::{Position, Viewshed};
use crate::map::Map;

// =====================================================================
// Components & sets
// =====================================================================

/// Marker component: entities with this component update
/// [`Map::explored_tiles`] when their FOV is computed. Typically
/// attached only to the player.
#[derive(Component, Debug, Clone)]
pub struct FovRevealsMap;

/// System set that contains the FOV update system.
///
/// Games can order their own systems relative to this set with
/// `.before(FovSet)` / `.after(FovSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FovSet;

// =====================================================================
// Plugin
// =====================================================================

/// Registers the [`fov_update_system`] into the [`Update`] schedule
/// under the [`FovSet`] system set.
pub struct FovPlugin;

impl Plugin for FovPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fov_update_system.in_set(FovSet));
    }
}

// =====================================================================
// System
// =====================================================================

/// Recomputes the field of view for every entity whose [`Viewshed`] is
/// marked dirty.
///
/// For each dirty viewshed the system:
/// 1. Clears the old `visible_tiles`.
/// 2. Calls bracket-lib's symmetric shadow-casting FOV.
/// 3. Filters results to in-bounds tiles.
/// 4. Clears the `dirty` flag.
/// 5. If the entity has [`FovRevealsMap`], marks each visible tile as
///    explored in the [`Map`] resource.
pub fn fov_update_system(
    mut map: ResMut<Map>,
    mut query: Query<(&Position, &mut Viewshed, Option<&FovRevealsMap>)>,
) {
    for (pos, mut viewshed, reveals_map) in query.iter_mut() {
        if !viewshed.dirty {
            continue;
        }

        viewshed.visible_tiles.clear();

        let visible = field_of_view_set(Point::new(pos.x, pos.y), viewshed.range, &*map);

        // Only keep tiles that fall within the map boundaries.
        viewshed.visible_tiles = visible.into_iter().filter(|p| map.in_bounds(*p)).collect();

        viewshed.dirty = false;

        // If this entity reveals the map (typically the player), mark
        // visible tiles as explored so the renderer can show them even
        // after the entity moves away.
        if reveals_map.is_some() {
            for pt in viewshed.visible_tiles.iter() {
                let idx = map.xy_idx(pt.x, pt.y);
                map.explored_tiles[idx] = true;
            }
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

    // ---- Helpers ----

    fn floor_tile() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }

    fn wall_tile() -> Tile {
        Tile {
            terrain: TerrainType::Wall,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }

    /// Creates an open room with wall borders.
    fn open_map(w: i32, h: i32) -> Map {
        let count = (w * h) as usize;
        let mut tiles = vec![floor_tile(); count];
        // Walls on border
        for x in 0..w {
            tiles[(0 * w + x) as usize] = wall_tile();
            tiles[((h - 1) * w + x) as usize] = wall_tile();
        }
        for y in 0..h {
            tiles[(y * w) as usize] = wall_tile();
            tiles[(y * w + (w - 1)) as usize] = wall_tile();
        }
        Map {
            name: "test".into(),
            tiles,
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width: w,
            height: h,
            depth: 1,
        }
    }

    // ---- Test cases ----

    #[test]
    fn fov_populates_visible_tiles() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, fov_update_system);
        app.insert_resource(open_map(20, 20));

        let entity = app
            .world_mut()
            .spawn((Position { x: 10, y: 10 }, Viewshed::new(8)))
            .id();

        app.update();

        let viewshed = app.world().get::<Viewshed>(entity).unwrap();
        assert!(
            !viewshed.visible_tiles.is_empty(),
            "FOV should populate visible tiles for a dirty viewshed"
        );
    }

    #[test]
    fn fov_clears_dirty_flag() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, fov_update_system);
        app.insert_resource(open_map(20, 20));

        let entity = app
            .world_mut()
            .spawn((Position { x: 10, y: 10 }, Viewshed::new(8)))
            .id();

        app.update();

        let viewshed = app.world().get::<Viewshed>(entity).unwrap();
        assert!(!viewshed.dirty, "dirty flag should be cleared after FOV runs");
    }

    #[test]
    fn fov_skips_clean_viewsheds() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, fov_update_system);
        app.insert_resource(open_map(20, 20));

        // Spawn with dirty=false (default)
        let entity = app
            .world_mut()
            .spawn((Position { x: 10, y: 10 }, Viewshed::default()))
            .id();

        app.update();

        let viewshed = app.world().get::<Viewshed>(entity).unwrap();
        assert!(
            viewshed.visible_tiles.is_empty(),
            "clean viewshed should not be recomputed"
        );
    }

    #[test]
    fn fov_reveals_map_marks_explored() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, fov_update_system);
        app.insert_resource(open_map(20, 20));

        let entity = app
            .world_mut()
            .spawn((Position { x: 10, y: 10 }, Viewshed::new(8), FovRevealsMap))
            .id();

        app.update();

        let map = app.world().resource::<Map>();
        let viewshed = app.world().get::<Viewshed>(entity).unwrap();

        // Every visible tile should be marked explored.
        for pt in viewshed.visible_tiles.iter() {
            let idx = map.xy_idx(pt.x, pt.y);
            assert!(
                map.explored_tiles[idx],
                "tile ({},{}) is visible but not explored",
                pt.x,
                pt.y
            );
        }
        // At least some tiles should be explored.
        assert!(
            map.explored_tiles.iter().any(|&e| e),
            "at least some tiles should be explored"
        );
    }

    #[test]
    fn fov_without_reveals_does_not_explore() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, fov_update_system);
        app.insert_resource(open_map(20, 20));

        // No FovRevealsMap marker.
        let entity = app
            .world_mut()
            .spawn((Position { x: 10, y: 10 }, Viewshed::new(8)))
            .id();

        app.update();

        let map = app.world().resource::<Map>();
        let viewshed = app.world().get::<Viewshed>(entity).unwrap();

        // Viewshed should be populated...
        assert!(!viewshed.visible_tiles.is_empty());
        // ...but no tiles should be explored.
        assert!(
            !map.explored_tiles.iter().any(|&e| e),
            "explored_tiles should remain untouched without FovRevealsMap"
        );
    }

    #[test]
    fn walls_block_fov() {
        let w = 20;
        let h = 20;
        let mut map = open_map(w, h);

        // Place a wall column at x=12 from y=8 to y=12, blocking view east.
        for y in 8..=12 {
            let idx = map.xy_idx(12, y);
            map.tiles[idx] = wall_tile();
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, fov_update_system);
        app.insert_resource(map);

        let entity = app
            .world_mut()
            .spawn((Position { x: 10, y: 10 }, Viewshed::new(8)))
            .id();

        app.update();

        let viewshed = app.world().get::<Viewshed>(entity).unwrap();

        // Tiles well behind the wall (e.g., x=15, y=10) should NOT be visible.
        let blocked_pt = Point::new(15, 10);
        assert!(
            !viewshed.visible_tiles.contains(&blocked_pt),
            "tile behind wall should be blocked from FOV"
        );

        // But tiles on the near side of the wall should be visible.
        let near_pt = Point::new(11, 10);
        assert!(
            viewshed.visible_tiles.contains(&near_pt),
            "tile in front of wall should be visible"
        );
    }
}
