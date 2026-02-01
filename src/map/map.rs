use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

use crate::{
    assets_plugin::DungeonTileset,
    components::Collider,
    map::tile::{FLOOR, TileType, WALL},
};

// --------------------------------------------------------------------------------
// CONFIGURATION
// --------------------------------------------------------------------------------
pub const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
pub const GRID_SIZE: TilemapGridSize = TilemapGridSize { x: 16.0, y: 16.0 };
pub const MAP_SIZE: TilemapSize = TilemapSize { x: 80, y: 60 };

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

fn spawn_dungeon(mut commands: Commands, dungeon_tileset: ResMut<DungeonTileset>) {
    // 2. Create the TileStorage (The container for all tiles)
    // We create it empty first, and `TilemapBundle` will populate it.
    let tile_storage = TileStorage::empty(MAP_SIZE);

    // 3. Spawn the Map Entity
    let map_entity = commands
        .spawn((
            TilemapBundle {
                grid_size: GRID_SIZE,
                map_type: TilemapType::Square,
                size: MAP_SIZE,
                texture: TilemapTexture::Single(dungeon_tileset.texture.clone()),
                tile_size: TILE_SIZE,
                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                storage: tile_storage,
                ..Default::default()
            },
            DungeonMap,
        ))
        .id();

    // 4. Fill the map (Procedural Generation)
    // We need to get the storage component *after* spawning,
    // but since we are inside the same system, we create the tile entities
    // and manually tell them which map they belong to.

    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    for x in 0..MAP_SIZE.x {
        for y in 0..MAP_SIZE.y {
            let tile_pos = TilePos { x, y };

            // Simple Logic: Borders are walls, inside is floor
            let is_border = x == 0 || x == MAP_SIZE.x - 1 || y == 0 || y == MAP_SIZE.y - 1;
            // Add some random pillars (pseudo-random for example)
            let is_pillar = (x % 4 == 0) && (y % 4 == 0);

            let (texture_index, tile_type) = if is_border || is_pillar {
                (WALL, TileType::Wall)
            } else {
                (FLOOR, TileType::Floor)
            };

            // Spawn the individual tile entity
            let mut command = commands.spawn((
                TileBundle {
                    position: tile_pos,
                    tilemap_id: TilemapId(map_entity),
                    texture_index: TileTextureIndex(texture_index as u32),
                    ..Default::default()
                },
                tile_type, // Attach our logic component
            ));

            if tile_type == TileType::Wall {
                command.insert(Collider);
            }

            // Store the entity in the storage container
            tile_storage.set(&tile_pos, command.id());
        }
    }

    // 5. Update the map entity with the populated storage
    // This allows us to query the map later to find out what is at (x, y)
    commands.entity(map_entity).insert(tile_storage);
}
