use bevy::ecs::{
    query::{Changed, With},
    system::Query,
};
use bevy_ecs_tilemap::tiles::TileStorage;
use bracket_lib::prelude::{Point, field_of_view};

use crate::{
    components::{Position, Viewshed},
    map::{
        EcsMap,
        map::{DungeonMap, MAP_SIZE},
        tile::TileType,
    },
};

pub fn fov_update_system(
    mut query: Query<(&mut Viewshed, &Position), Changed<Position>>,
    map_query: Query<&TileStorage, With<DungeonMap>>,
    tile_type_query: Query<&TileType>,
) {
    let Ok(tile_storage) = map_query.single() else {
        return;
    };
    let ecs_map = EcsMap {
        tile_storage,
        tile_query: &tile_type_query,
        map_size: MAP_SIZE,
    };

    for (mut viewshed, position) in query.iter_mut() {
        viewshed.visible_tiles.clear();
        viewshed.visible_tiles =
            field_of_view(Point::new(position.x, position.y), viewshed.range, &ecs_map);
    }
}
