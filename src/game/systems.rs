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

pub fn fov_update_system(mut query: Query<(&mut Viewshed, Ref<Position>)>, map: Res<Map>) {
    // We check if the map itself has changed (e.g. new level loaded) because
    // viewsheds calculated on the old map will be invalid even if the entity
    // hasn't moved yet.
    let map_changed = map.is_changed();
    for (mut viewshed, position) in query.iter_mut() {
        if viewshed.dirty || map_changed || viewshed.is_changed() || position.is_changed() {
            viewshed.visible_tiles =
                field_of_view(Point::new(position.x, position.y), viewshed.range, &*map)
                    .into_iter()
                    .collect();
            viewshed.dirty = false;
        }
    }
}

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
