use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::assets::{
    CandleSpritesheet, MonsterManifest, MonsterManifestHandle, MonsterSpawnTable,
    MonsterSpawnTableHandle, MonsterSpriteAssets, TileManifest, TileManifestHandle,
    TileSpriteAssets,
};
use crate::components::{FloorEntityMarker, Monster, Name, Position};
use crate::game::{TurnManager, spawn_monster_by_name, turns::TurnMarker};
use crate::map::Map;
use crate::map::builders::BuilderMap;
use crate::map::light::{Candle, spawn_candle};
use crate::map::tile::{TerrainType, is_walkable};
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

use crate::{
    AppState,
    map::{
        builders::level_builder,
        map::{DungeonECSMap, MAP_SIZE},
        tile::{TileMarker, spawn_tile_entity},
    },
};

/// Groups the three tile-asset resources to keep `spawn_dungeon`'s parameter
/// count within Bevy's 16-parameter limit for system functions.
#[derive(SystemParam)]
pub struct TileAssets<'w> {
    manifests: Res<'w, Assets<TileManifest>>,
    manifest_handle: Res<'w, TileManifestHandle>,
    sprite_assets: Res<'w, TileSpriteAssets>,
}

// ---------------------------------------------------------------------------
// Floor caching — preserves a visited floor's state so returning via UpStairs
// restores the map and its surviving entities rather than regenerating.
// ---------------------------------------------------------------------------

pub struct CachedFloor {
    pub map: Map,
    /// Alive monsters: their last-known grid position and manifest name.
    pub monster_list: Vec<(Point, String)>,
    pub candle_spawn_points: Vec<Point>,
    /// Position of the DownStairs on this floor; player lands adjacent to it
    /// when returning from below.
    pub down_stairs_pos: Point,
}

#[derive(Resource, Default)]
pub struct FloorCache(pub HashMap<u32, CachedFloor>);

/// Set by the ascend system before it triggers SpawnDungeonMessage so that
/// spawn_dungeon can restore rather than regenerate.
#[derive(Resource, Default)]
pub struct PendingFloorRestore(pub Option<CachedFloor>);

// ---------------------------------------------------------------------------
// Messages / resources
// ---------------------------------------------------------------------------

#[derive(Message, Clone, Copy)]
pub struct SpawnDungeonMessage;

#[derive(Message, Clone, Copy)]
pub struct MapTransitionMessage;

#[derive(Message, Clone, Copy)]
pub struct AscendStairsMessage;

#[derive(Resource)]
pub struct Floor(pub u32);

impl Default for Floor {
    fn default() -> Self {
        Floor(1)
    }
}

#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Point);

pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Floor>()
            .init_resource::<FloorCache>()
            .init_resource::<PendingFloorRestore>()
            .add_message::<SpawnDungeonMessage>()
            .add_message::<MapTransitionMessage>()
            .add_message::<AscendStairsMessage>()
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
                    ascend_stairs_system.run_if(on_message::<AscendStairsMessage>),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_down_stairs(map: &Map) -> Option<Point> {
    map.tiles.iter().enumerate().find_map(|(idx, tile)| {
        if tile.terrain == TerrainType::DownStairs {
            let (x, y) = map.idx_xy(idx);
            Some(Point::new(x, y))
        } else {
            None
        }
    })
}

/// Returns the first orthogonally adjacent walkable tile to `target`, or
/// `None` if no such tile exists.
fn find_adjacent_floor(map: &Map, target: Point) -> Option<Point> {
    for (dx, dy) in [(0i32, 1i32), (1, 0), (0, -1), (-1, 0)] {
        let pt = Point::new(target.x + dx, target.y + dy);
        if let Some(tile) = map.get_tile(pt) {
            if is_walkable(tile) {
                return Some(pt);
            }
        }
    }
    None
}

/// Snapshot the current floor's surviving entities into a `CachedFloor`.
fn snapshot_floor(
    map: &Map,
    monster_query: &Query<(&Position, &Name), With<Monster>>,
    candle_query: &Query<&Position, With<Candle>>,
) -> CachedFloor {
    let monster_list = monster_query
        .iter()
        .map(|(pos, name)| (pos.to_point(), name.0.clone()))
        .collect();

    let candle_spawn_points = candle_query
        .iter()
        .map(|pos| Point::new(pos.x, pos.y))
        .collect();

    let down_stairs_pos = find_down_stairs(map).unwrap_or(Point::new(0, 0));

    CachedFloor {
        map: map.clone(),
        monster_list,
        candle_spawn_points,
        down_stairs_pos,
    }
}

/// Despawn all entities that belong to the current floor only.
fn despawn_floor_entities(
    commands: &mut Commands,
    q_map_markers: &Query<Entity, With<DungeonECSMap>>,
    q_tiles: &Query<Entity, With<TileMarker>>,
    q_floor_entities: &Query<Entity, With<FloorEntityMarker>>,
) {
    for entity in q_map_markers.iter() {
        commands.entity(entity).despawn();
    }
    for entity in q_tiles.iter() {
        commands.entity(entity).despawn();
    }
    for entity in q_floor_entities.iter() {
        commands.entity(entity).despawn();
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn player_stair_system(
    player_query: Query<&Position, (With<Player>, Changed<Position>)>,
    map: Res<Map>,
    floor: Res<Floor>,
    mut down_writer: MessageWriter<MapTransitionMessage>,
    mut up_writer: MessageWriter<AscendStairsMessage>,
) {
    for pos in player_query.iter() {
        if map.in_bounds(pos.to_point()) {
            let idx = map.xy_idx(pos.x, pos.y);
            match map.tiles[idx].terrain {
                TerrainType::DownStairs => {
                    down_writer.write(MapTransitionMessage);
                }
                TerrainType::UpStairs if floor.0 > 1 => {
                    up_writer.write(AscendStairsMessage);
                }
                _ => {}
            }
        }
    }
}

fn map_transition_system(
    mut commands: Commands,
    mut floor: ResMut<Floor>,
    map: Res<Map>,
    mut floor_cache: ResMut<FloorCache>,
    q_map_markers: Query<Entity, With<DungeonECSMap>>,
    q_tiles: Query<Entity, With<TileMarker>>,
    q_floor_entities: Query<Entity, With<FloorEntityMarker>>,
    q_monsters: Query<(&Position, &Name), With<Monster>>,
    q_candles: Query<&Position, With<Candle>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    // Snapshot before despawning so we can return to this floor later.
    let cached = snapshot_floor(&map, &q_monsters, &q_candles);
    floor_cache.0.insert(floor.0, cached);

    despawn_floor_entities(&mut commands, &q_map_markers, &q_tiles, &q_floor_entities);
    *turn_manager = TurnManager::default();

    floor.0 += 1;
    log_writer.write(GameLogMessage(format!("Descending to floor {}", floor.0)));
    message_writer.write(SpawnDungeonMessage);
}

fn ascend_stairs_system(
    mut commands: Commands,
    mut floor: ResMut<Floor>,
    map: Res<Map>,
    mut floor_cache: ResMut<FloorCache>,
    mut pending_restore: ResMut<PendingFloorRestore>,
    q_map_markers: Query<Entity, With<DungeonECSMap>>,
    q_tiles: Query<Entity, With<TileMarker>>,
    q_floor_entities: Query<Entity, With<FloorEntityMarker>>,
    q_monsters: Query<(&Position, &Name), With<Monster>>,
    q_candles: Query<&Position, With<Candle>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    if floor.0 <= 1 {
        return;
    }

    // Snapshot current floor before leaving it.
    let cached = snapshot_floor(&map, &q_monsters, &q_candles);
    floor_cache.0.insert(floor.0, cached);

    despawn_floor_entities(&mut commands, &q_map_markers, &q_tiles, &q_floor_entities);
    *turn_manager = TurnManager::default();

    floor.0 -= 1;
    log_writer.write(GameLogMessage(format!("Ascending to floor {}", floor.0)));

    // Pull the cached floor so spawn_dungeon restores it instead of regenerating.
    pending_restore.0 = floor_cache.0.remove(&floor.0);

    message_writer.write(SpawnDungeonMessage);
}

fn spawn_tiles_into_ecs(
    commands: &mut Commands,
    map_entity: Entity,
    game_map: &Map,
    tile_assets: &TileAssets,
) {
    let tile_manifest = tile_assets
        .manifests
        .get(&tile_assets.manifest_handle.0)
        .expect("Tile manifest not loaded");

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let pt = Point::new(x, y);
            let tile = game_map.get_tile(pt).unwrap();

            let tile_entity = spawn_tile_entity(
                commands,
                map_entity,
                tile,
                pt,
                tile_manifest,
                &tile_assets.sprite_assets,
            );
            commands
                .entity(tile_entity)
                .insert(Position { x: pt.x, y: pt.y });
        }
    }
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
    for pt in build_data.candle_spawn_points.iter() {
        spawn_candle(commands, candle_spritesheet, pt);
    }

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
    mut pending_restore: ResMut<PendingFloorRestore>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_spawn_tables: Res<Assets<MonsterSpawnTable>>,
    monster_spawn_table_handle: Res<MonsterSpawnTableHandle>,
    monster_sprite_assets: Res<MonsterSpriteAssets>,
    tile_assets: TileAssets,
    mut log_writer: MessageWriter<GameLogMessage>,
    player_query: Query<Entity, With<Player>>,
    turn_marker_query: Query<Entity, (With<TurnMarker>, Without<Player>)>,
) {
    let map_entity = commands.spawn((DungeonECSMap, RenderLayers::layer(1))).id();

    let player_spawn: Point = if let Some(cached) = pending_restore.0.take() {
        // ---------------------------------------------------------------
        // Restore a previously visited floor
        // ---------------------------------------------------------------
        *map = cached.map;

        spawn_tiles_into_ecs(&mut commands, map_entity, &map, &tile_assets);

        for (pt, name) in &cached.monster_list {
            spawn_monster_by_name(
                &mut commands,
                name.as_str(),
                pt,
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
            );
        }

        for pt in &cached.candle_spawn_points {
            spawn_candle(&mut commands, &candle_spritesheet, pt);
        }

        // Land the player on a walkable tile adjacent to the down stairs so
        // they don't immediately re-trigger the stair system.
        find_adjacent_floor(&map, cached.down_stairs_pos)
            .unwrap_or(cached.down_stairs_pos)
    } else {
        // ---------------------------------------------------------------
        // Generate a fresh floor
        // ---------------------------------------------------------------
        let spawn_table = monster_spawn_tables
            .get(&monster_spawn_table_handle.0)
            .unwrap();

        let mut builder = level_builder(
            floor.0 as i32,
            MAP_SIZE.x as i32,
            MAP_SIZE.y as i32,
            &spawn_table.spawns,
        );
        builder.build_map();
        *map = builder.build_data.map.clone();

        spawn_tiles_into_ecs(&mut commands, map_entity, &map, &tile_assets);

        spawn_dungeon_entities(
            &mut commands,
            &builder.build_data,
            &mut turn_manager,
            &candle_spritesheet,
            &monster_manifests,
            &monster_manifest_handle,
            &monster_sprite_assets,
        );

        let starting_pos = builder.build_data.starting_position.unwrap_or_else(|| {
            warn!("Map builder did not set a starting position; falling back to first walkable tile.");
            map.tiles
                .iter()
                .enumerate()
                .find(|(_, t)| is_walkable(**t))
                .map(|(idx, _)| {
                    let (x, y) = map.idx_xy(idx);
                    crate::components::Position { x, y }
                })
                .expect("Map has no walkable tiles — cannot place player")
        });

        Point::new(starting_pos.x, starting_pos.y)
    };

    commands.insert_resource(PlayerSpawnPoint(player_spawn));

    // Re-add persistent actors (player + global turn marker) to the turn queue.
    if let Ok(player_entity) = player_query.single() {
        turn_manager.add_entity(player_entity);
    }
    for marker_entity in turn_marker_query.iter() {
        turn_manager.add_entity(marker_entity);
    }

    log_writer.write(GameLogMessage(format!("Welcome to floor {}!", floor.0)));
}
