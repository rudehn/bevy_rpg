use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    assets_plugin::DungeonTileset,
    components::Collider,
    map::{
        builders::level_builder,
        tile::{FLOOR, TileType, WALL},
    },
};

// --------------------------------------------------------------------------------
// CONFIGURATION
// --------------------------------------------------------------------------------
pub const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
pub const GRID_SIZE: TilemapGridSize = TilemapGridSize { x: 16.0, y: 16.0 };
pub const MAP_SIZE: TilemapSize = TilemapSize { x: 80, y: 60 };

#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Point);

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TilemapPlugin) // Required by bevy_ecs_tilemap
            .add_systems(Startup, spawn_dungeon);
    }
}

// Tag for the entity that holds the map storage
#[derive(Component)]
pub struct DungeonMap;

// --------------------------------------------------------------------------------
// SYSTEMS
// --------------------------------------------------------------------------------

pub fn spawn_dungeon(mut commands: Commands, dungeon_tileset: Res<DungeonTileset>) {
    // Create the Tilemap entity
    let map_entity = commands.spawn(DungeonMap).id();

    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    // Run the builder
    let mut builder = level_builder(1, MAP_SIZE.x as i32, MAP_SIZE.y as i32);
    builder.build_map();

    // Bake the map into the ECS
    for y in 0..builder.build_data.map.height() {
        for x in 0..builder.build_data.map.width() {
            let pt = Point::new(x, y);
            let tile_pos = TilePos {
                x: x as u32,
                y: y as u32,
            };
            let tile_type = builder.build_data.map.get_tile(pt).unwrap();

            let texture_index = match tile_type {
                TileType::Floor => FLOOR,
                TileType::Wall => WALL,
                TileType::DownStairs => FLOOR, // Placeholder
            };

            let mut command = commands.spawn((
                TileBundle {
                    position: tile_pos,
                    tilemap_id: TilemapId(map_entity),
                    texture_index: TileTextureIndex(texture_index as u32),
                    ..Default::default()
                },
                tile_type,
            ));

            if tile_type == TileType::Wall {
                command.insert(Collider);
            }

            tile_storage.set(&tile_pos, command.id());
        }
    }

    // Add the tilemap components to the map entity
    commands.entity(map_entity).insert(TilemapBundle {
        grid_size: GRID_SIZE,
        map_type: TilemapType::Square,
        size: MAP_SIZE,
        storage: tile_storage,
        texture: TilemapTexture::Single(dungeon_tileset.texture.clone()),
        tile_size: TILE_SIZE,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });

    // Insert the player spawn point as a resource
    let spawn_point = builder.build_data.starting_position.unwrap();
    commands.insert_resource(PlayerSpawnPoint(Point::new(spawn_point.x, spawn_point.y)));
}
