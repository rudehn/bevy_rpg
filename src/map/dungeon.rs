use bevy::prelude::*;
use bevy_ecs_tilemap::TilemapBundle;
use bevy_ecs_tilemap::map::TilemapType;
use bevy_ecs_tilemap::tiles::TileStorage;
use bevy_ecs_tilemap::{map::TilemapTexture, prelude::TilePos};
use bracket_lib::prelude::Point;

use crate::map::Map;
use crate::map::map::TILE_SIZE;
use crate::{
    AppState,
    game::{DungeonTileset, spawn_goblin},
    map::{
        builders::level_builder,
        light::{CandleSpritesheet, spawn_candle},
        map::{DungeonECSMap, GRID_SIZE, MAP_SIZE},
        tile::spawn_tile_entity,
    },
};

#[derive(Message, Clone, Copy)]
pub struct SpawnDungeonMessage;
#[derive(Resource)]
pub struct Floor(pub u32);

impl Default for Floor {
    fn default() -> Self {
        Floor(1) // Start at floor 1
    }
}
#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Point);
pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Floor>()
            .add_message::<SpawnDungeonMessage>()
            .add_message::<MapTransitionMessage>()
            .add_systems(
                OnEnter(AppState::InGame),
                |mut writer: MessageWriter<SpawnDungeonMessage>| {
                    writer.write(SpawnDungeonMessage);
                },
            )
            .add_systems(
                Update,
                spawn_dungeon.run_if(on_message::<SpawnDungeonMessage>),
            )
            .add_systems(
                Update,
                map_transition_system.run_if(on_message::<MapTransitionMessage>),
            );
    }
}

#[derive(Message)]
pub struct MapTransitionMessage;

fn map_transition_system(
    mut commands: Commands,
    mut floor: ResMut<Floor>,
    q_map: Query<(Entity, &TileStorage), With<DungeonECSMap>>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
) {
    // Despawn old map entities and their tiles
    for (map_entity, tile_storage) in q_map.iter() {
        for x in 0..MAP_SIZE.x {
            for y in 0..MAP_SIZE.y {
                if let Some(tile_entity) = tile_storage.get(&TilePos { x, y }) {
                    commands.entity(tile_entity).despawn();
                }
            }
        }
        commands.entity(map_entity).despawn();
    }

    // Increment floor
    floor.0 += 1;
    println!("Entering floor {}", floor.0);

    message_writer.write(SpawnDungeonMessage);
}

pub fn spawn_dungeon(
    mut commands: Commands,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>, // New parameter
    floor: Res<Floor>,
    mut map: ResMut<Map>,
) {
    // Run the builder
    let mut builder = level_builder(floor.0 as i32, MAP_SIZE.x as i32, MAP_SIZE.y as i32);
    builder.build_map();
    *map = builder.build_data.map.clone();

    // Bake the map into the ECS
    // Create the Tilemap entity
    let map_entity = commands.spawn(DungeonECSMap).id();
    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    for y in 0..builder.build_data.map.height() {
        for x in 0..builder.build_data.map.width() {
            let pt = Point::new(x, y);
            let tile_pos = TilePos {
                x: x as u32,
                y: y as u32,
            };
            let tile_type = builder.build_data.map.get_tile(pt).unwrap();

            let tile_entity = spawn_tile_entity(&mut commands, map_entity, tile_pos, tile_type, pt);
            tile_storage.set(&tile_pos, tile_entity);
        }
    }

    // Spawn candles
    for pt in builder.build_data.candle_spawn_points.iter() {
        spawn_candle(&mut commands, &candle_spritesheet, pt);
    }

    // Spawn entities from the builder's spawn list
    for (pt, name) in builder.build_data.spawn_list.iter() {
        match name.as_str() {
            "Goblin" => {
                spawn_goblin(&mut commands, &dungeon_tileset, pt);
            }
            _ => {
                // Ignore other entity types for now
            }
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
