use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::assets::{
    ItemManifest, ItemManifestHandle, ItemSpriteAssets, ItemSpawnTable,
    ItemSpawnTableHandle, MonsterManifest, MonsterManifestHandle, MonsterSpawnTable,
    MonsterSpawnTableHandle, MonsterSpriteAssets, TileManifest, TileManifestHandle, TileSpriteAssets,
    PropManifest, PropManifestHandle, PropSpriteAssets,
    PrefabManifest, PrefabManifestHandle,
};
use crate::components::{FloorEntityMarker, InInventory, Monster, Name, Position, Item, Prop};
use crate::game::{TurnManager, spawn_monster_by_name, spawn_item, spawn_prop, items::ItemStack, turns::TurnMarker};
use crate::map::Map;
use crate::map::builders::BuilderMap;
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

/// Groups monster, item, and prop assets to keep parameter count down.
#[derive(SystemParam)]
pub struct EntityAssets<'w> {
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
    pub prop_manifests: Res<'w, Assets<PropManifest>>,
    pub prop_manifest_handle: Res<'w, PropManifestHandle>,
    pub prop_sprite_assets: Res<'w, PropSpriteAssets>,
    pub prefab_manifests: Res<'w, Assets<PrefabManifest>>,
    pub prefab_manifest_handle: Res<'w, PrefabManifestHandle>,
}

// ---------------------------------------------------------------------------
// Floor caching — preserves a visited floor's state so returning via UpStairs
// restores the map and its surviving entities rather than regenerating.
// ---------------------------------------------------------------------------

pub struct CachedFloor {
    pub map: Map,
    /// Alive monsters: their position, name, and optional squad data.
    pub monster_list: Vec<CachedMonster>,
    /// Surrounding items: their position, name, and stack count.
    pub item_list: Vec<(Point, String, u32)>,
    /// Props: their position and prop name.
    pub prop_list: Vec<(Point, String)>,
    /// Position of the DownStairs on this floor; player lands adjacent to it
    /// when returning from below (ascending).
    pub down_stairs_pos: Point,
    /// Position of the UpStairs on this floor; player lands adjacent to it
    /// when returning from above (descending).
    pub up_stairs_pos: Point,
}

/// A monster entry in the floor cache, preserving squad membership.
pub struct CachedMonster {
    pub pos: Point,
    pub name: String,
    pub squad_id: Option<u64>,
    pub is_leader: bool,
    pub squad_config: Option<crate::game::squad::SquadConfig>,
    pub home_position: Option<Point>,
}

#[derive(Resource, Default)]
pub struct FloorCache(pub HashMap<u32, CachedFloor>);

/// Set by the stair systems before triggering SpawnDungeonMessage so that
/// spawn_dungeon can restore rather than regenerate.
#[derive(Resource, Default)]
pub struct PendingFloorRestore {
    pub floor: Option<CachedFloor>,
    /// True when ascending (player came up from below); false when descending
    /// back to a previously visited floor. Determines which stairs to land near.
    pub ascending: bool,
}

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

fn find_up_stairs(map: &Map) -> Option<Point> {
    map.tiles.iter().enumerate().find_map(|(idx, tile)| {
        if tile.terrain == TerrainType::UpStairs {
            let (x, y) = map.idx_xy(idx);
            Some(Point::new(x, y))
        } else {
            None
        }
    })
}

/// Returns the first orthogonally adjacent walkable, non-stair tile to `target`.
/// Stair tiles are excluded so the player doesn't immediately re-trigger a
/// floor transition upon being placed there.
fn find_adjacent_floor(map: &Map, target: Point) -> Option<Point> {
    use TerrainType::{DownStairs, UpStairs};
    for (dx, dy) in [(0i32, 1i32), (1, 0), (0, -1), (-1, 0)] {
        let pt = Point::new(target.x + dx, target.y + dy);
        if let Some(tile) = map.get_tile(pt) {
            if is_walkable(tile) && tile.terrain != DownStairs && tile.terrain != UpStairs {
                return Some(pt);
            }
        }
    }
    // Fallback: search the whole map for any plain floor tile.
    map.tiles.iter().enumerate().find_map(|(idx, tile)| {
        use TerrainType::{DownStairs, UpStairs};
        if is_walkable(*tile) && tile.terrain != DownStairs && tile.terrain != UpStairs {
            let (x, y) = map.idx_xy(idx);
            Some(Point::new(x, y))
        } else {
            None
        }
    })
}

/// Snapshot the current floor's surviving entities into a `CachedFloor`.
fn snapshot_floor(
    map: &Map,
    monster_query: &Query<(&Position, &Name, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, &crate::game::MonsterAI), With<Monster>>,
    item_query: &Query<(&Position, &Name, Option<&ItemStack>), (With<Item>, Without<InInventory>)>,
    prop_query: &Query<(&Position, &Name), With<Prop>>,
) -> CachedFloor {
    let monster_list = monster_query
        .iter()
        .map(|(pos, name, squad_id, squad_config, is_leader, ai)| CachedMonster {
            pos: pos.to_point(),
            name: name.0.clone(),
            squad_id: squad_id.map(|s| s.0),
            is_leader,
            squad_config: squad_config.cloned(),
            home_position: ai.home_position,
        })
        .collect();

    let item_list = item_query
        .iter()
        .map(|(pos, name, stack)| {
            let count = stack.map(|s| s.count).unwrap_or(1);
            (pos.to_point(), name.0.clone(), count)
        })
        .collect();

    let prop_list = prop_query
        .iter()
        .map(|(pos, name)| (pos.to_point(), name.0.clone()))
        .collect();

    let down_stairs_pos = find_down_stairs(map).unwrap_or(Point::new(0, 0));
    let up_stairs_pos = find_up_stairs(map).unwrap_or(Point::new(0, 0));

    CachedFloor {
        map: map.clone(),
        monster_list,
        item_list,
        prop_list,
        down_stairs_pos,
        up_stairs_pos,
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
    mut pending_restore: ResMut<PendingFloorRestore>,
    q_map_markers: Query<Entity, With<DungeonECSMap>>,
    q_tiles: Query<Entity, With<TileMarker>>,
    q_floor_entities: Query<Entity, With<FloorEntityMarker>>,
    q_monsters: Query<(&Position, &Name, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, &crate::game::MonsterAI), With<Monster>>,
    q_items: Query<(&Position, &Name, Option<&ItemStack>), (With<Item>, Without<InInventory>)>,
    q_props: Query<(&Position, &Name), With<Prop>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    // Snapshot before despawning so we can return to this floor later.
    let cached = snapshot_floor(&map, &q_monsters, &q_items, &q_props);
    floor_cache.0.insert(floor.0, cached);

    despawn_floor_entities(&mut commands, &q_map_markers, &q_tiles, &q_floor_entities);
    *turn_manager = TurnManager::default();

    floor.0 += 1;
    log_writer.write(GameLogMessage(format!("Descending to floor {}", floor.0)));

    // If the destination floor was previously visited, restore it instead of generating fresh.
    pending_restore.floor = floor_cache.0.remove(&floor.0);
    pending_restore.ascending = false;

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
    q_monsters: Query<(&Position, &Name, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, &crate::game::MonsterAI), With<Monster>>,
    q_items: Query<(&Position, &Name, Option<&ItemStack>), (With<Item>, Without<InInventory>)>,
    q_props: Query<(&Position, &Name), With<Prop>>,
    mut turn_manager: ResMut<TurnManager>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    if floor.0 <= 1 {
        return;
    }

    // Snapshot current floor before leaving it.
    let cached = snapshot_floor(&map, &q_monsters, &q_items, &q_props);
    floor_cache.0.insert(floor.0, cached);

    despawn_floor_entities(&mut commands, &q_map_markers, &q_tiles, &q_floor_entities);
    *turn_manager = TurnManager::default();

    floor.0 -= 1;
    log_writer.write(GameLogMessage(format!("Ascending to floor {}", floor.0)));

    // Pull the cached floor so spawn_dungeon restores it instead of regenerating.
    pending_restore.floor = floor_cache.0.remove(&floor.0);
    pending_restore.ascending = true;

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
    for entry in build_data.spawn_list.iter() {
        if let Some(entity) = spawn_monster_by_name(
            commands,
            entry.name.as_str(),
            &entry.pos,
            turn_manager,
            &assets.monster_manifests,
            &assets.monster_manifest_handle,
            &assets.monster_sprite_assets,
        ) {
            // Attach squad components if this monster is part of a squad.
            if let (Some(squad_id), Some(squad_config)) = (entry.squad_id, entry.squad_config.clone()) {
                commands.entity(entity).insert((squad_id, squad_config));
                if entry.is_leader {
                    commands.entity(entity).insert(crate::game::squad::SquadLeader);
                }
            }
            // Guard AI: override default MonsterAI with guard behavior.
            if let Some(home) = entry.home_position {
                commands.entity(entity).insert(crate::game::MonsterAI::guard(home));
            }
        }
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

    for (pt, name) in build_data.prop_spawn_list.iter() {
        spawn_prop(
            commands,
            name.as_str(),
            pt,
            &assets.prop_manifests,
            &assets.prop_manifest_handle,
            &assets.prop_sprite_assets,
        );
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
    mut squad_counter: ResMut<crate::game::squad::SquadIdCounter>,
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

        // Restore squad ID counter
        commands.insert_resource(crate::game::squad::SquadIdCounter(save_data.squad_id_counter));

        // Spawn monsters with saved HP override and squad data
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
                if let (Some(sid), Some(cfg)) = (entry.squad_id, entry.squad_config.clone()) {
                    commands.entity(entity).insert((
                        crate::game::squad::SquadId(sid),
                        cfg,
                    ));
                    if entry.is_leader {
                        commands.entity(entity).insert(crate::game::squad::SquadLeader);
                    }
                }
                if let Some(home) = entry.home_position {
                    let home_pt = Point::new(home[0], home[1]);
                    commands.entity(entity).insert(crate::game::MonsterAI::guard(home_pt));
                }
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

        // Spawn props
        for entry in &save_data.props {
            let pt = Point::new(entry.x, entry.y);
            spawn_prop(
                &mut commands,
                &entry.name,
                &pt,
                &assets.prop_manifests,
                &assets.prop_manifest_handle,
                &assets.prop_sprite_assets,
            );
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
    } else if let Some(cached) = pending_restore.floor.take() {
        // ---------------------------------------------------------------
        // Restore a previously visited floor
        // ---------------------------------------------------------------
        let ascending = pending_restore.ascending;
        *map = cached.map;

        spawn_tiles_into_ecs(&mut commands, map_entity, &map, &tile_assets);

        for cached_mon in &cached.monster_list {
            if let Some(entity) = spawn_monster_by_name(
                &mut commands,
                cached_mon.name.as_str(),
                &cached_mon.pos,
                &mut turn_manager,
                &assets.monster_manifests,
                &assets.monster_manifest_handle,
                &assets.monster_sprite_assets,
            ) {
                if let (Some(sid), Some(cfg)) = (cached_mon.squad_id, cached_mon.squad_config.clone()) {
                    commands.entity(entity).insert((
                        crate::game::squad::SquadId(sid),
                        cfg,
                    ));
                    if cached_mon.is_leader {
                        commands.entity(entity).insert(crate::game::squad::SquadLeader);
                    }
                }
                if let Some(home) = cached_mon.home_position {
                    commands.entity(entity).insert(crate::game::MonsterAI::guard(home));
                }
            }
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

        for (pt, name) in &cached.prop_list {
            spawn_prop(
                &mut commands,
                name.as_str(),
                pt,
                &assets.prop_manifests,
                &assets.prop_manifest_handle,
                &assets.prop_sprite_assets,
            );
        }

        needs_explored_init.0 = true;

        // Land the player adjacent to the stairs they arrived through so they
        // don't immediately re-trigger the stair system.
        // Ascending (came up from below) → land near down stairs.
        // Descending (came down from above) → land near up stairs.
        let target_stairs = if ascending {
            cached.down_stairs_pos
        } else {
            cached.up_stairs_pos
        };
        find_adjacent_floor(&map, target_stairs).unwrap_or(target_stairs)
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

        let prefabs = assets.prefab_manifests
            .get(&assets.prefab_manifest_handle.0)
            .map(|m| m.prefabs.clone())
            .unwrap_or_default();

        let mut builder = level_builder(
            floor.0 as i32,
            MAP_SIZE.x as i32,
            MAP_SIZE.y as i32,
            &spawn_table.spawns,
            &item_spawn_table.spawns,
            squad_counter.clone(),
            prefabs,
        );
        builder.build_map();
        // Write the updated counter back so future floors don't reuse IDs.
        *squad_counter = builder.build_data.squad_counter.clone();
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

    // First floor intro — set the atmosphere.
    if floor.0 == 1 {
        log_writer.write(GameLogMessage(
            "The stone steps descend into darkness. Somewhere far below, the Veiled Tyrant stirs.".to_string(),
        ));
        log_writer.write(GameLogMessage(
            "Its power grows with every passing moment. You must reach the depths before it becomes unstoppable.".to_string(),
        ));
    }

    // Trigger auto-save after the new floor is fully set up.
    // (Skipped during load since apply_player_load_system hasn't run yet;
    //  auto_save_system checks for the player entity so it will self-correct.)
    auto_save_pending.0 = true;
}
