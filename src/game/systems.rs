use bevy::ecs::change_detection::DetectChanges;
use bevy::prelude::{Or, Visibility};
use bevy::{
    ecs::{
        query::{Changed, With},
        system::{Query, Res},
    },
    transform::components::Transform,
};
use bracket_lib::prelude::{Point, field_of_view};

use crate::map::map::GRID_SIZE;
use crate::{
    components::{Monster, Position, Viewshed},
    map::Map,
    player::Player, // Corrected import
};

pub fn fov_update_system(
    mut query: Query<(&mut Viewshed, &Position), Or<(Changed<Position>, Changed<Viewshed>)>>,
    map: Res<Map>,
) {
    for (mut viewshed, position) in query.iter_mut() {
        viewshed.visible_tiles.clear();
        viewshed.visible_tiles =
            field_of_view(Point::new(position.x, position.y), viewshed.range, &*map);
        viewshed.dirty = false;
    }
}

pub fn update_monster_visibility(
    player_query: Query<&Viewshed, With<Player>>, // Query player viewshed
    mut monster_query: Query<(&Position, &mut Visibility), With<Monster>>, // Query monsters
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };

    for (monster_pos, mut monster_vis) in monster_query.iter_mut() {
        let monster_point = Point::new(monster_pos.x, monster_pos.y);
        let is_visible = player_viewshed.visible_tiles.contains(&monster_point);

        if is_visible {
            *monster_vis = Visibility::Visible;
        } else {
            *monster_vis = Visibility::Hidden;
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
