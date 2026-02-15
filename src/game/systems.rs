use bevy::prelude::Visibility;
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
    components::{Goblin, Position, Viewshed},
    map::{Map, map::TILE_SIZE},
    player::Player, // Corrected import
};

pub fn fov_update_system(
    mut query: Query<(&mut Viewshed, &Position, Option<&Player>), Changed<Position>>,
    map: Res<Map>,
) {
    for (mut viewshed, position, player) in query.iter_mut() {
        viewshed.visible_tiles.clear();
        viewshed.visible_tiles =
            field_of_view(Point::new(position.x, position.y), viewshed.range, &*map);

        if player.is_some() {
            println!(
                "Player FOV updated at {:?} with {} visible tiles",
                position,
                viewshed.visible_tiles.len()
            );
        }
    }
}

pub fn update_goblin_visibility(
    player_query: Query<&Viewshed, (With<Player>, Changed<Viewshed>)>, // Query player viewshed, only when it changes
    mut goblin_query: Query<(&Position, &mut Visibility), With<Goblin>>, // Query goblins
) {
    let Ok(player_viewshed) = player_query.single() else {
        return; // No player or viewshed hasn't changed
    };

    println!(
        "Update Goblin Visibility: Player viewshed has {} visible tiles",
        player_viewshed.visible_tiles.len()
    );

    for (goblin_pos, mut goblin_vis) in goblin_query.iter_mut() {
        let goblin_point = Point::new(goblin_pos.x, goblin_pos.y);
        let is_visible = player_viewshed.visible_tiles.contains(&goblin_point);

        if is_visible {
            *goblin_vis = Visibility::Visible;
        } else {
            *goblin_vis = Visibility::Hidden;
        }
        println!(
            "Goblin at {:?} is_visible_to_player: {}, new visibility: {:?}",
            goblin_point, is_visible, *goblin_vis
        );
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
