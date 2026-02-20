use bevy::prelude::*;
use bevy_ecs_tilemap::TilemapBundle;
use bevy_ecs_tilemap::map::TilemapType;
use bevy_ecs_tilemap::tiles::TileStorage;
use bevy_ecs_tilemap::{map::TilemapTexture, prelude::TilePos};
use bracket_lib::prelude::Point;

use crate::assets::{
    CandleSpritesheet, DungeonTileset, MonsterManifest, MonsterManifestHandle, MonsterSpriteAssets,
};
use crate::game::{TurnManager, spawn_monster_by_name};
use crate::map::Map;
use crate::map::builders::BuilderMap;
use crate::map::map::TILE_SIZE; // Import BuildData

use crate::{
    AppState,
    map::{
        builders::level_builder,
        light::spawn_candle,
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

    message_writer.write(SpawnDungeonMessage);
}

fn spawn_tiles_into_ecs(
    commands: &mut Commands,
    map_entity: Entity,
    game_map: &Map,
    dungeon_tileset: &Res<DungeonTileset>,
) -> TileStorage {
    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let pt = Point::new(x, y);
            let tile_pos = TilePos {
                x: x as u32,
                y: y as u32,
            };
            let tile_type = game_map.get_tile(pt).unwrap();

            let tile_entity = spawn_tile_entity(commands, map_entity, tile_pos, tile_type, pt);
            tile_storage.set(&tile_pos, tile_entity);
        }
    }

    commands.entity(map_entity).insert(TilemapBundle {
        grid_size: GRID_SIZE,
        map_type: TilemapType::Square,
        size: MAP_SIZE,
        storage: tile_storage.clone(),
        texture: TilemapTexture::Single(dungeon_tileset.texture.clone()),
        tile_size: TILE_SIZE,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });
    tile_storage
}

// ... existing code ...

fn spawn_dungeon_entities(
    commands: &mut Commands,
    build_data: &BuilderMap,
    turn_manager: &mut ResMut<TurnManager>,
    candle_spritesheet: &Res<CandleSpritesheet>,
    monster_manifests: &Res<Assets<MonsterManifest>>,
    monster_manifest_handle: &Res<MonsterManifestHandle>,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) {
    // Spawn candles
    for pt in build_data.candle_spawn_points.iter() {
        spawn_candle(commands, candle_spritesheet, pt);
    }

    // Spawn entities from the builder's spawn list
    for (pt, name) in build_data.spawn_list.iter() {
        spawn_monster_by_name(
            commands,
            name.as_str(),
            pt,
            turn_manager,
            monster_manifests,
            monster_manifest_handle,
            monster_sprite_assets,
        );
    }
}

pub fn spawn_dungeon(
    mut commands: Commands,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>, // New parameter
    floor: Res<Floor>,
    mut map: ResMut<Map>,
    mut turn_manager: ResMut<TurnManager>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_sprite_assets: Res<MonsterSpriteAssets>,
) {
    // Run the builder
    let mut builder = level_builder(floor.0 as i32, MAP_SIZE.x as i32, MAP_SIZE.y as i32);
    builder.build_map();
    *map = builder.build_data.map.clone();

    // Bake the map into the ECS
    // Create the Tilemap entity
    let map_entity = commands.spawn(DungeonECSMap).id();
    let tile_storage = spawn_tiles_into_ecs(&mut commands, map_entity, &map, &dungeon_tileset);

    spawn_dungeon_entities(
        &mut commands,
        &builder.build_data,
        &mut turn_manager,
        &candle_spritesheet,
        &monster_manifests,
        &monster_manifest_handle,
        &monster_sprite_assets,
    );

    // Insert the player spawn point as a resource
    let spawn_point = builder.build_data.starting_position.unwrap();
    commands.insert_resource(PlayerSpawnPoint(Point::new(spawn_point.x, spawn_point.y)));
}
