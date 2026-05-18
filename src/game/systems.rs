use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::world::Ref;
use bevy::prelude::{Resource, Visibility};
use bevy::{
    ecs::{
        query::{Changed, Has, With, Without},
        system::{Query, Res},
    },
    transform::components::Transform,
};
use bracket_lib::prelude::{Algorithm2D, Point, field_of_view};

use crate::map::map::GRID_SIZE;
use crate::{
    components::{InInventory, Item, Monster, Position, Prop, Submerged, Viewshed},
    map::Map,
    player::Player,
};

/// When active, all tiles, monsters, items, and props are visible at full brightness.
#[derive(Resource, Default)]
pub struct Omniscient(pub bool);

// fov_update_system is now provided by the engine's FovPlugin.
// Re-export so existing call sites (`crate::game::systems::fov_update_system`)
// continue to compile.
pub use roguelike_engine::components::fov_update_system;

pub fn update_monster_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    omniscient: Res<Omniscient>,
    mut monster_query: Query<(&Position, &mut Visibility, Has<Submerged>), With<Monster>>,
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };

    for (monster_pos, mut monster_vis, is_submerged) in monster_query.iter_mut() {
        if is_submerged {
            *monster_vis = Visibility::Hidden;
            continue;
        }

        let monster_point = Point::new(monster_pos.x, monster_pos.y);
        let is_visible = omniscient.0 || player_viewshed.visible_tiles.contains(&monster_point);

        if is_visible {
            *monster_vis = Visibility::Visible;
        } else {
            *monster_vis = Visibility::Hidden;
        }
    }
}

/// Updates floor item visibility to match the player's explored/visible state.
/// - Unexplored tile: Hidden
/// - Explored, not visible: Visible (item "memory")
/// - Currently visible: Full brightness
pub fn update_item_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    omniscient: Res<Omniscient>,
    mut item_query: Query<(&Position, &mut Visibility), (With<Item>, Without<InInventory>)>,
) {
    let Ok(viewshed) = player_query.single() else {
        return;
    };

    for (pos, mut vis) in item_query.iter_mut() {
        if !map.in_bounds(Point::new(pos.x, pos.y)) {
            continue;
        }
        let idx = map.xy_idx(pos.x, pos.y);
        let pt = Point::new(pos.x, pos.y);

        if omniscient.0 || viewshed.visible_tiles.contains(&pt) {
            *vis = Visibility::Visible;
        } else if map.explored_tiles[idx] {
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// Updates prop visibility to match the player's explored/visible state.
/// Same logic as items: explored-but-not-visible tiles show dimmed.
pub fn update_prop_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    omniscient: Res<Omniscient>,
    mut prop_query: Query<(&Position, &mut Visibility), With<Prop>>,
) {
    let Ok(viewshed) = player_query.single() else {
        return;
    };

    for (pos, mut vis) in prop_query.iter_mut() {
        if !map.in_bounds(Point::new(pos.x, pos.y)) {
            continue;
        }
        let idx = map.xy_idx(pos.x, pos.y);
        let pt = Point::new(pos.x, pos.y);

        if omniscient.0 || viewshed.visible_tiles.contains(&pt) {
            *vis = Visibility::Visible;
        } else if map.explored_tiles[idx] {
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

pub fn sync_entity_transforms(mut query: Query<(&mut Transform, &Position), Changed<Position>>) {
    let config = GRID_SIZE;
    for (mut transform, pos) in query.iter_mut() {
        // Calculate the center of the tile
        let x = pos.x as f32 * config.x;
        let y = pos.y as f32 * config.y;

        // Update the translation.
        // We keep the existing Z-axis to maintain layering (Player on top of Items)
        transform.translation.x = x;
        transform.translation.y = y;
    }
}

/// Mark a viewshed dirty whenever its entity's `Position` changes, so the
/// engine's FOV system recomputes visibility on the same frame the move
/// resolves. Without this, only the player's first-turn FOV (set dirty by
/// `Viewshed::new`) would compute correctly; subsequent moves would only
/// update when an unrelated system happened to flip the dirty flag (door
/// opens, tile mutations, equipping a vision-bonus item) — producing the
/// "FOV randomly updates after several turns" symptom.
///
/// Catches every position-mutation path automatically: player movement,
/// monster AI, blink staff, knockback, chasm landing, etc.
pub fn mark_moved_viewsheds_dirty(
    mut query: Query<&mut Viewshed, Changed<Position>>,
) {
    for mut viewshed in query.iter_mut() {
        viewshed.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    fn changed_position_marks_viewshed_dirty() {
        // The bug repro: a clean viewshed must flip dirty when its entity's
        // position is mutated, so the engine FOV system will recompute it.
        let mut app = App::new();
        app.add_systems(Update, mark_moved_viewsheds_dirty);

        let entity = app
            .world_mut()
            .spawn((
                Position { x: 5, y: 5 },
                Viewshed { range: 8, dirty: false, ..Default::default() },
            ))
            .id();

        // First frame after spawn: Bevy treats every initial component write
        // as Changed, so the watcher fires once and the viewshed becomes dirty.
        app.update();
        assert!(app.world().get::<Viewshed>(entity).unwrap().dirty);

        // Reset and confirm a true mutation also flips it.
        app.world_mut().get_mut::<Viewshed>(entity).unwrap().dirty = false;
        let mut pos = app.world_mut().get_mut::<Position>(entity).unwrap();
        pos.x = 6;
        drop(pos);
        app.update();
        assert!(
            app.world().get::<Viewshed>(entity).unwrap().dirty,
            "viewshed must be marked dirty after Position mutation"
        );
    }

    #[test]
    fn unchanged_position_does_not_mark_dirty() {
        // Idle frames must not gratuitously dirty viewsheds — that would
        // make the engine recompute FOV every tick for every entity.
        let mut app = App::new();
        app.add_systems(Update, mark_moved_viewsheds_dirty);

        let entity = app
            .world_mut()
            .spawn((
                Position { x: 5, y: 5 },
                Viewshed { range: 8, dirty: false, ..Default::default() },
            ))
            .id();

        // Burn the first frame (initial-spawn Changed signal flips dirty).
        app.update();
        app.world_mut().get_mut::<Viewshed>(entity).unwrap().dirty = false;

        // Now a frame with no position writes — dirty must stay false.
        app.update();
        assert!(
            !app.world().get::<Viewshed>(entity).unwrap().dirty,
            "viewshed must NOT be re-dirtied when position is unchanged"
        );
    }

    #[test]
    fn fov_updates_on_consecutive_moves() {
        // End-to-end smoke test: with the engine FOV system + the watcher
        // both registered, two back-to-back moves both produce fresh
        // visible_tiles (i.e. neither move "skips" recomputation).
        use roguelike_engine::components::FovSet;
        use roguelike_engine::components::fov_update_system;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(
            Update,
            (
                mark_moved_viewsheds_dirty.before(FovSet),
                fov_update_system.in_set(FovSet),
            ),
        );

        // Open 20x20 floor.
        use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};
        let w: i32 = 20;
        let h: i32 = 20;
        let mut tiles = vec![
            Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
            (w * h) as usize
        ];
        // Walls on the border so FOV has something to bound against.
        for x in 0..w {
            tiles[(0 * w + x) as usize].terrain = TerrainType::Wall;
            tiles[((h - 1) * w + x) as usize].terrain = TerrainType::Wall;
        }
        for y in 0..h {
            tiles[(y * w) as usize].terrain = TerrainType::Wall;
            tiles[(y * w + (w - 1)) as usize].terrain = TerrainType::Wall;
        }
        app.insert_resource(Map {
            name: "fov_test".into(),
            tiles,
            explored_tiles: vec![false; (w * h) as usize],
            blocked: vec![false; (w * h) as usize],
            width: w,
            height: h,
            depth: 1,
        });

        let entity = app
            .world_mut()
            .spawn((
                Position { x: 10, y: 10 },
                Viewshed::new(6),
            ))
            .id();

        // First frame: initial FOV computes around (10, 10).
        app.update();
        let visible_a: std::collections::HashSet<Point> = app
            .world()
            .get::<Viewshed>(entity)
            .unwrap()
            .visible_tiles
            .clone();
        assert!(visible_a.contains(&Point::new(10, 10)));

        // Move and run another frame.
        app.world_mut().get_mut::<Position>(entity).unwrap().x = 12;
        app.update();
        let visible_b: std::collections::HashSet<Point> = app
            .world()
            .get::<Viewshed>(entity)
            .unwrap()
            .visible_tiles
            .clone();
        assert!(visible_b.contains(&Point::new(12, 10)));
        assert_ne!(
            visible_a, visible_b,
            "consecutive moves must produce different visible_tiles sets"
        );

        // Move again — this is the case that was failing before the fix.
        app.world_mut().get_mut::<Position>(entity).unwrap().x = 14;
        app.update();
        let visible_c: std::collections::HashSet<Point> = app
            .world()
            .get::<Viewshed>(entity)
            .unwrap()
            .visible_tiles
            .clone();
        assert!(visible_c.contains(&Point::new(14, 10)));
        assert_ne!(
            visible_b, visible_c,
            "third consecutive move must also recompute FOV"
        );
    }
}
