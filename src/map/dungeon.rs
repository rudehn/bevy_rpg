use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::assets::{
    CandleSpritesheet, ItemManifest, ItemManifestHandle, ItemSpriteAssets, ItemSpawnTable,
    ItemSpawnTableHandle, MonsterManifest, MonsterManifestHandle, MonsterSpawnTable,
    MonsterSpawnTableHandle, MonsterSpriteAssets, TileManifest, TileManifestHandle, TileSpriteAssets,
};
use crate::components::{FloorEntityMarker, Monster, Name, Position, Item};
use crate::game::{TurnManager, spawn_monster_by_name, spawn_item, items::ItemStack, turns::TurnMarker};
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
        map::{DungeonECSMap, MAP_SIZE, NeedsExploredInit},
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

/// Groups monster, item, and candle assets to keep parameter count down.
#[derive(SystemParam)]
pub struct EntityAssets<'w> {
    pub candle_spritesheet: Res<'w, CandleSpritesheet>,
    pub monster_manifests: Res<'w, Assets<MonsterManifest>>,
    pub monster_manifest_handle: Res<'w, MonsterManifestHandle>,
    pub monster_spawn_tables: Res<'w, Assets<MonsterSpawnTable>>,
    pub monster_spawn_table_handle: Res<'w, MonsterSpawnTableHandle>,
    pub monster_sprite_assets: Res<'w, MonsterSpriteAssets>,
    pub item_manifests: Res<'w, Assets<ItemManifest>>,
    pub item_manifest_handle: Res<'w, ItemManifestHandle>,
    pub item_sprite_assets: Res<'w, ItemSpriteAssets>,
    pub item_spawn_tables: Res<'w, Assets<ItemSpawnTable>>,
    pub item_spawn_table_handle: Res<'w, ItemSpawnTableHandle>,
}

// ---------------------------------------------------------------------------
// Floor caching — preserves a visited floor's state so returning via UpStairs
// restores the map and its surviving entities rather than regenerating.
// ---------------------------------------------------------------------------

pub struct CachedFloor {
    pub map: Map,
    /// Alive monsters: their last-known grid position and manifest name.
    pub monster_list: Vec<(Point, String)>,
    /// Surrounding items: their position, name, and stack count.
    pub item_list: Vec<(Point, String, u32)>,
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

/// Set by the menu when the player chooses "Continue". Consumed by spawn_dungeon.
#[derive(Resource, Default)]
pub struct PendingGameLoad(pub Option<Box<crate::save::GameSaveData>>);

/// Set by spawn_dungeon during a load; consumed by apply_player_load_system.
#[derive(Resource, Default)]
pub struct PendingPlayerLoad(pub Option<crate::save::PlayerSaveData>);

/// Set by spawn_dungeon after floor setup; triggers auto_save_system.
#[derive(Resource, Default)]
pub struct AutoSavePending(pub bool);

#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Point);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpawnDungeonSet;

pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Floor>()
            .init_resource::<FloorCache>()
            .init_resource::<PendingFloorRestore>()
            .init_resource::<PendingGameLoad>()
            .init_resource::<PendingPlayerLoad>()
            .init_resource::<AutoSavePending>()
            .add_message::<SpawnDungeonMessage>()
            .add_message::<MapTransitionMessage>()
            .add_message::<AscendStairsMessage>()
            .configure_sets(Update, SpawnDungeonSet)
            .add_systems(
                OnEnter(AppState::InGame),
                |mut writer: MessageWriter<SpawnDungeonMessage>| {
                    writer.write(SpawnDungeonMessage);
                },
            )
            .add_systems(
                Update,
                spawn_dungeon
                    .run_if(on_message::<SpawnDungeonMessage>)
                    .in_set(SpawnDungeonSet),
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
    item_query: &Query<(&Position, &Name, Option<&ItemStack>), With<Item>>,
    candle_query: &Query<&Position, With<Candle>>,
) -> CachedFloor {
    let monster_list = monster_query
        .iter()
        .map(|(pos, name)| (pos.to_point(), name.0.clone()))
        .collect();

    let item_list = item_query
        .iter()
        .map(|(pos, name, stack)| {
            let count = stack.map(|s| s.count).unwrap_or(1);
            (pos.to_point(), name.0.clone(), count)
        })
        .collect();

    let candle_spawn_points = candle_query
        .iter()
        .map(|pos| Point::new(pos.x, pos.y))
        .collect();

    let down_stairs_pos = find_down_stairs(map).unwrap_or(Point::new(0, 0));

    CachedFloor {
        map: map.clone(),
        monster_list,
        item_list,
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
    q_items: Query<(&Position, &Name, Option<&ItemStack>), With<Item>>,
    q_candles: Query<&Position, With<Candle>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    // Snapshot before despawning so we can return to this floor later.
    let cached = snapshot_floor(&map, &q_monsters, &q_items, &q_candles);
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
    q_items: Query<(&Position, &Name, Option<&ItemStack>), With<Item>>,
    q_candles: Query<&Position, With<Candle>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    if floor.0 <= 1 {
        return;
    }

    // Snapshot current floor before leaving it.
    let cached = snapshot_floor(&map, &q_monsters, &q_items, &q_candles);
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
    assets: &EntityAssets,
) {
    for pt in build_data.candle_spawn_points.iter() {
        spawn_candle(commands, &assets.candle_spritesheet, pt);
    }

    for (pt, name) in build_data.spawn_list.iter() {
        spawn_monster_by_name(
            commands,
            name.as_str(),
            pt,
            turn_manager,
            &assets.monster_manifests,
            &assets.monster_manifest_handle,
            &assets.monster_sprite_assets,
        );
    }

    for (pt, name, count) in build_data.item_spawn_list.iter() {
        if let Some(entity) = spawn_item(
            commands,
            name.as_str(),
            pt,
            &assets.item_manifests,
            &assets.item_manifest_handle,
            &assets.item_sprite_assets,
        ) {
            if *count > 1 {
                let max_stack = assets.item_manifests
                    .get(&assets.item_manifest_handle.0)
                    .and_then(|m| m.items.get(name.as_str()))
                    .map(|a| a.max_stack)
                    .unwrap_or(1);
                commands.entity(entity).insert(crate::game::items::ItemStack { count: *count, max_stack });
            }
        }
    }
}

pub fn spawn_dungeon(
    mut commands: Commands,
    floor: Res<Floor>,
    mut map: ResMut<Map>,
    mut turn_manager: ResMut<TurnManager>,
    mut pending_restore: ResMut<PendingFloorRestore>,
    mut pending_game_load: ResMut<PendingGameLoad>,
    mut pending_player_load: ResMut<PendingPlayerLoad>,
    mut auto_save_pending: ResMut<AutoSavePending>,
    mut needs_explored_init: ResMut<NeedsExploredInit>,
    assets: EntityAssets,
    tile_assets: TileAssets,
    mut log_writer: MessageWriter<GameLogMessage>,
    player_query: Query<Entity, With<Player>>,
    turn_marker_query: Query<Entity, (With<TurnMarker>, Without<Player>)>,
) {
    let map_entity = commands.spawn((DungeonECSMap, RenderLayers::layer(1))).id();

    let player_spawn: Point = if let Some(save_data) = pending_game_load.0.take() {
        // ---------------------------------------------------------------
        // LOAD PATH: Restore full game state from disk
        // ---------------------------------------------------------------
        use crate::save::{SavedHp, save_data_to_map, SavedFloorCache};

        // Restore map
        *map = save_data_to_map(&save_data.map);
        spawn_tiles_into_ecs(&mut commands, map_entity, &map, &tile_assets);

        // Spawn monsters with saved HP override
        for entry in &save_data.monsters {
            let pt = Point::new(entry.x, entry.y);
            if let Some(entity) = spawn_monster_by_name(
                &mut commands,
                &entry.name,
                &pt,
                &mut turn_manager,
                &assets.monster_manifests,
                &assets.monster_manifest_handle,
                &assets.monster_sprite_assets,
            ) {
                commands.entity(entity).insert(SavedHp(entry.hp_current));
            }
        }

        // Spawn floor items
        for entry in &save_data.floor_items {
            let pt = Point::new(entry.x, entry.y);
            if let Some(entity) = spawn_item(
                &mut commands,
                &entry.name,
                &pt,
                &assets.item_manifests,
                &assets.item_manifest_handle,
                &assets.item_sprite_assets,
            ) {
                if entry.count > 1 {
                    let max_stack = assets.item_manifests
                        .get(&assets.item_manifest_handle.0)
                        .and_then(|m| m.items.get(entry.name.as_str()))
                        .map(|a| a.max_stack)
                        .unwrap_or(1);
                    commands.entity(entity).insert(ItemStack { count: entry.count, max_stack });
                }
            }
        }

        // Spawn candles
        for pos in &save_data.candles {
            let pt = Point::new(pos[0], pos[1]);
            spawn_candle(&mut commands, &assets.candle_spritesheet, &pt);
        }

        // Pass the floor cache save data to apply_player_load_system
        let saved_floor_cache: std::collections::HashMap<u32, crate::save::CachedFloorSave> =
            save_data.floor_cache.clone();
        commands.insert_resource(SavedFloorCache(saved_floor_cache));

        // Restore game log
        // (done indirectly — apply_player_load_system handles player state,
        //  GameLog is reset here from save)
        // We pass the full save to PendingPlayerLoad so the game log is also restored
        let player_spawn_pt = Point::new(save_data.player.x, save_data.player.y);

        pending_player_load.0 = Some(save_data.player);
        needs_explored_init.0 = true;

        player_spawn_pt
    } else if let Some(cached) = pending_restore.0.take() {
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
                &assets.monster_manifests,
                &assets.monster_manifest_handle,
                &assets.monster_sprite_assets,
            );
        }

        for (pt, name, count) in &cached.item_list {
            if let Some(entity) = spawn_item(
                &mut commands,
                name.as_str(),
                pt,
                &assets.item_manifests,
                &assets.item_manifest_handle,
                &assets.item_sprite_assets,
            ) {
                if *count > 1 {
                    let max_stack = assets.item_manifests
                        .get(&assets.item_manifest_handle.0)
                        .and_then(|m| m.items.get(name.as_str()))
                        .map(|a| a.max_stack)
                        .unwrap_or(1);
                    commands.entity(entity).insert(ItemStack { count: *count, max_stack });
                }
            }
        }

        for pt in &cached.candle_spawn_points {
            spawn_candle(&mut commands, &assets.candle_spritesheet, pt);
        }

        needs_explored_init.0 = true;

        // Land the player on a walkable tile adjacent to the down stairs so
        // they don't immediately re-trigger the stair system.
        find_adjacent_floor(&map, cached.down_stairs_pos)
            .unwrap_or(cached.down_stairs_pos)
    } else {
        // ---------------------------------------------------------------
        // Generate a fresh floor
        // ---------------------------------------------------------------
        let spawn_table = assets.monster_spawn_tables
            .get(&assets.monster_spawn_table_handle.0)
            .unwrap();
        let item_spawn_table = assets.item_spawn_tables
            .get(&assets.item_spawn_table_handle.0)
            .unwrap();

        let mut builder = level_builder(
            floor.0 as i32,
            MAP_SIZE.x as i32,
            MAP_SIZE.y as i32,
            &spawn_table.spawns,
            &item_spawn_table.spawns,
        );
        builder.build_map();
        *map = builder.build_data.map.clone();

        spawn_tiles_into_ecs(&mut commands, map_entity, &map, &tile_assets);

        spawn_dungeon_entities(
            &mut commands,
            &builder.build_data,
            &mut turn_manager,
            &assets,
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

        let starting_pt = Point::new(starting_pos.x, starting_pos.y);
        // If the builder placed UpStairs at the starting position (depth > 1),
        // step the player off it so player_stair_system doesn't immediately
        // fire AscendStairsMessage on the first Changed<Position>.
        if map
            .get_tile(starting_pt)
            .map(|t| t.terrain == TerrainType::UpStairs)
            .unwrap_or(false)
        {
            find_adjacent_floor(&map, starting_pt).unwrap_or(starting_pt)
        } else {
            starting_pt
        }
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

    // Trigger auto-save after the new floor is fully set up.
    // (Skipped during load since apply_player_load_system hasn't run yet;
    //  auto_save_system checks for the player entity so it will self-correct.)
    auto_save_pending.0 = true;
}
