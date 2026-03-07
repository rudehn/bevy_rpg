use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_ecs_tilemap::TilemapBundle;
use bevy_ecs_tilemap::map::{TilemapTexture, TilemapType};
use bevy_ecs_tilemap::prelude::TilePos;
use bevy_ecs_tilemap::tiles::TileStorage;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::assets::{
    CandleSpritesheet, MonsterManifest, MonsterManifestHandle, MonsterSpawnTable,
    MonsterSpawnTableHandle, MonsterSpriteAssets, TileManifest, TileManifestHandle, TileSpriteAssets,
};
use crate::game::{TurnManager, spawn_monster_by_name, turns::TurnMarker};
use crate::map::Map;
use crate::map::builders::BuilderMap;
use crate::map::map::TILE_SIZE; // Import BuildData
use crate::map::tile::TileType;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

use crate::{
    AppState,
    components::{FloorEntityMarker, GameEntityMarker, Position},
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
                (
                    player_stair_system,
                    map_transition_system.run_if(on_message::<MapTransitionMessage>),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Message)]
pub struct MapTransitionMessage;

fn player_stair_system(
    player_query: Query<&Position, (With<Player>, Changed<Position>)>,
    map: Res<Map>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
) {
    for pos in player_query.iter() {
        if map.in_bounds(pos.to_point()) {
            let idx = map.xy_idx(pos.x, pos.y);
            if map.tiles[idx] == TileType::DownStairs {
                transition_writer.write(MapTransitionMessage);
            }
        }
    }
}

fn map_transition_system(
    mut commands: Commands,
    mut floor: ResMut<Floor>,
    q_map: Query<(Entity, &TileStorage), With<DungeonECSMap>>,
    q_floor_entities: Query<Entity, With<FloorEntityMarker>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    // 1. Despawn old map entities and their tiles
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

    // 2. Despawn all floor-specific entities (monsters, candles, etc.)
    for entity in q_floor_entities.iter() {
        commands.entity(entity).despawn();
    }

    // 3. Reset turn manager queue (monsters are gone)
    *turn_manager = TurnManager::default();

    // 4. Increment floor
    floor.0 += 1;
    log_writer.write(GameLogMessage(format!("Descending to floor {}", floor.0)));

    message_writer.write(SpawnDungeonMessage);
}

fn spawn_tiles_into_ecs(
    commands: &mut Commands,
    map_entity: Entity,
    game_map: &Map,
    tile_manifest: &TileManifest,
    tile_sprite_assets: &TileSpriteAssets,
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

            let tile_entity = spawn_tile_entity(
                commands, 
                map_entity, 
                tile_pos, 
                tile_type, 
                pt,
                tile_manifest,
                tile_sprite_assets
            );
            tile_storage.set(&tile_pos, tile_entity);
        }
    }

    commands.entity(map_entity).insert(TilemapBundle {
        grid_size: GRID_SIZE,
        map_type: TilemapType::Square,
        size: MAP_SIZE,
        storage: tile_storage.clone(),
        texture: TilemapTexture::Single(tile_sprite_assets.handles.get("tilemap_packed.png").unwrap().clone()),
        tile_size: TILE_SIZE,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });
    tile_storage
}

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
    candle_spritesheet: Res<CandleSpritesheet>,
    floor: Res<Floor>,
    mut map: ResMut<Map>,
    mut turn_manager: ResMut<TurnManager>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_spawn_tables: Res<Assets<MonsterSpawnTable>>,
    monster_spawn_table_handle: Res<MonsterSpawnTableHandle>,
    monster_sprite_assets: Res<MonsterSpriteAssets>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_sprite_assets: Res<TileSpriteAssets>,
    mut log_writer: MessageWriter<GameLogMessage>,
    player_query: Query<Entity, With<Player>>,
    turn_marker_query: Query<Entity, With<TurnMarker>>,
) {
    let tile_manifest = tile_manifests.get(&tile_manifest_handle.0).expect("Tile manifest not loaded");

    let spawn_table = monster_spawn_tables
        .get(&monster_spawn_table_handle.0)
        .unwrap();
    // Run the builder
    let mut builder = level_builder(
        floor.0 as i32,
        MAP_SIZE.x as i32,
        MAP_SIZE.y as i32,
        &spawn_table.spawns,
    );
    builder.build_map();
    *map = builder.build_data.map.clone();

    // Bake the map into the ECS
    // Create the Tilemap entity
    let map_entity = commands
        .spawn((DungeonECSMap, GameEntityMarker, RenderLayers::layer(1)))
        .id();
    let _tile_storage = spawn_tiles_into_ecs(
        &mut commands, 
        map_entity, 
        &map, 
        tile_manifest,
        &tile_sprite_assets
    );

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

    // Re-add persistent actors to turn manager if they already exist (changing floors)
    if let Ok(player_entity) = player_query.single() {
        turn_manager.add_entity(player_entity);
    }
    if let Ok(marker_entity) = turn_marker_query.single() {
        turn_manager.add_entity(marker_entity);
    }

    log_writer.write(GameLogMessage(format!("Welcome to floor {}!", floor.0)));
}
