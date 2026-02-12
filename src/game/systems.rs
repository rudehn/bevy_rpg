use bevy::ecs::{
    query::{Changed, With},
    system::{Query, Res},
};
use bevy::prelude::Visibility; // Corrected import
use bevy_ecs_tilemap::tiles::TileStorage;
use bracket_lib::prelude::{Point, field_of_view};

use crate::{
    components::{Goblin, Position, Viewshed},
    player::Player, // Corrected import
    map::{
        EcsMap,
        dungeon::Floor,
        map::{DungeonMap, MAP_SIZE},
        tile::TileType,
    },
};

pub fn fov_update_system(
    mut query: Query<(&mut Viewshed, &Position, Option<&Player>), Changed<Position>>, // Added Option<&Player>
    map_query: Query<&TileStorage, With<DungeonMap>>,
    tile_type_query: Query<&TileType>,
    floor: Res<Floor>, // Added Floor resource
) {
    let Ok(tile_storage) = map_query.single() else {
        return;
    };
    let ecs_map = EcsMap {
        tile_storage,
        tile_query: &tile_type_query,
        map_size: MAP_SIZE,
        depth: floor.0 as i32, // Pass depth from Floor resource
    };

    for (mut viewshed, position, player) in query.iter_mut() {
        viewshed.visible_tiles.clear();
        viewshed.visible_tiles =
            field_of_view(Point::new(position.x, position.y), viewshed.range, &ecs_map);

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
