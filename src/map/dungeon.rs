use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::components::{FloorEntityMarker, InInventory, Monster, Name, Position, Item, Prop};
use crate::game::enchantment::{Enchantment, ItemWeaponRunic, ItemArmorRunic, RunicIdentified};
use crate::game::staves::{StaffData, Rechargeable};
use crate::constants::MAX_FLOOR;
use crate::game::{TurnManager, items::ItemStack, turns::TurnMarker};
use crate::map::floor_materializer::{
    EntityAssets, FloorResult, FloorSource, TileAssets, materialize_floor,
};
use crate::map::Map;
use crate::map::tile::{TerrainType, is_walkable};
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

use crate::{
    AppState,
    map::{
        builders::level_builder,
        map::{DungeonECSMap, MAP_SIZE, NeedsExploredInit},
        tile::TileMarker,
    },
};

// ---------------------------------------------------------------------------
// Floor caching — preserves a visited floor's state so returning via UpStairs
// restores the map and its surviving entities rather than regenerating.
// ---------------------------------------------------------------------------

pub struct CachedFloor {
    pub map: Map,
    /// Alive monsters with mutable state (HP, squad, patrol).
    pub monsters: Vec<crate::save::SavedMonster>,
    /// Floor items with stack counts.
    pub items: Vec<crate::save::SavedItem>,
    /// Props.
    pub props: Vec<crate::save::SavedProp>,
    /// Position of the DownStairs on this floor; player lands adjacent to it
    /// when returning from below (ascending).
    pub down_stairs_pos: Point,
    /// Position of the UpStairs on this floor; player lands adjacent to it
    /// when returning from above (descending).
    pub up_stairs_pos: Point,
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

/// Marker component added to the player after a floor transition.
/// Prevents `player_stair_system` from immediately re-triggering when
/// the player spawns on a stair tile. Removed on the first position change
/// after spawning.
#[derive(Component)]
pub struct StairCooldown;

#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Point);

impl Default for PlayerSpawnPoint {
    fn default() -> Self {
        Self(Point::new(0, 0))
    }
}

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
            .init_resource::<PlayerSpawnPoint>()
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

/// Snapshot the current floor's surviving entities into a `CachedFloor`.
fn snapshot_floor(
    map: &Map,
    monster_query: &Query<(&Position, &Name, &crate::game::combat::Health, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>, Has<crate::components::Submerged>), With<Monster>>,
    item_query: &Query<(&Position, &Name, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>, Option<&StaffData>, Option<&Rechargeable>, Has<crate::components::Drifting>), (With<Item>, Without<InInventory>)>,
    prop_query: &Query<(&Position, &Name, Option<&crate::components::PropKey>), With<Prop>>,
) -> CachedFloor {
    use crate::save::{SavedMonster, SavedItem, SavedProp};

    let monsters = monster_query
        .iter()
        .map(|(pos, name, health, squad_id, squad_config, is_leader, patrol_route, is_submerged)| SavedMonster {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            hp_current: health.current,
            squad_id: squad_id.map(|s| s.0),
            is_leader,
            squad_config: squad_config.cloned(),
            patrol_route: patrol_route.cloned(),
            submerged: is_submerged,
        })
        .collect();

    let items = item_query
        .iter()
        .map(|(pos, name, stack, enchant, weapon_runic, armor_runic, runic_id, staff_data, rechargeable, is_drifting)| SavedItem {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            count: stack.map(|s| s.count).unwrap_or(1),
            enchantment: enchant.map(|e| e.level),
            weapon_runic: weapon_runic.map(|w| w.0.clone()),
            armor_runic: armor_runic.map(|a| a.0),
            runic_identified: runic_id.map(|r| r.0),
            staff_effect: staff_data.map(|s| s.effect),
            base_recharge: staff_data.map(|s| s.base_recharge),
            staff_charges: rechargeable.map(|r| r.charges),
            staff_max_charges: rechargeable.map(|r| r.max_charges),
            staff_recharge_timer: rechargeable.map(|r| r.recharge_timer),
            staff_recharge_rate: rechargeable.map(|r| r.recharge_rate),
            drifting: is_drifting,
        })
        .collect();

    let props = prop_query
        .iter()
        .map(|(pos, name, prop_key)| SavedProp {
            x: pos.x,
            y: pos.y,
            // Use the manifest key if available; fall back to display name
            // for backward compatibility with old saves.
            name: prop_key.map(|k| k.0.clone()).unwrap_or_else(|| name.0.clone()),
        })
        .collect();

    let fallback_pos = map
        .tiles
        .iter()
        .enumerate()
        .find_map(|(idx, tile)| {
            if is_walkable(*tile) {
                let (x, y) = map.idx_xy(idx);
                Some(Point::new(x, y))
            } else {
                None
            }
        })
        .unwrap_or(Point::new(1, 1));

    if find_down_stairs(map).is_none() && map.depth < MAX_FLOOR {
        warn!("snapshot_floor: no DownStairs found on floor {}", map.depth);
    }
    if find_up_stairs(map).is_none() && map.depth > 1 {
        warn!("snapshot_floor: no UpStairs found on floor {}", map.depth);
    }

    let down_stairs_pos = find_down_stairs(map).unwrap_or(fallback_pos);
    let up_stairs_pos = find_up_stairs(map).unwrap_or(fallback_pos);

    CachedFloor {
        map: map.clone(),
        monsters,
        items,
        props,
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
    mut commands: Commands,
    player_query: Query<(Entity, &Position, Has<StairCooldown>), (With<Player>, Changed<Position>)>,
    map: Res<Map>,
    floor: Res<Floor>,
    mut down_writer: MessageWriter<MapTransitionMessage>,
    mut up_writer: MessageWriter<AscendStairsMessage>,
    quest_item_query: Query<(), (With<crate::components::QuestItem>, With<InInventory>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut run_summary: ResMut<crate::game::RunSummary>,
    run_stats: Res<crate::game::RunStats>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for (entity, pos, has_cooldown) in player_query.iter() {
        if has_cooldown {
            // First position change after floor transition — consume the cooldown
            // and skip the stair check so we don't immediately re-trigger.
            commands.entity(entity).remove::<StairCooldown>();
            continue;
        }
        if map.in_bounds(pos.to_point()) {
            let idx = map.xy_idx(pos.x, pos.y);
            match map.tiles[idx].terrain {
                TerrainType::DownStairs => {
                    down_writer.write(MapTransitionMessage);
                }
                TerrainType::UpStairs if floor.0 > 1 => {
                    up_writer.write(AscendStairsMessage);
                }
                TerrainType::Portal => {
                    if quest_item_query.iter().next().is_some() {
                        // Player has the Amulet of Yendor — victory!
                        *run_summary = crate::game::RunSummary {
                            floor_reached: floor.0,
                            cause: "Escaped through the portal with the Amulet of Yendor.".to_string(),
                            victory: true,
                            enemies_killed: run_stats.enemies_killed,
                        };
                        crate::save::delete_save();
                        next_state.set(AppState::Victory);
                    } else {
                        log_writer.write(GameLogMessage(
                            "The portal hums with energy, but refuses to let you pass. You sense it requires something...".to_string()
                        ));
                    }
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
    q_monsters: Query<(&Position, &Name, &crate::game::combat::Health, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>, Has<crate::components::Submerged>), With<Monster>>,
    q_items: Query<(&Position, &Name, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>, Option<&StaffData>, Option<&Rechargeable>, Has<crate::components::Drifting>), (With<Item>, Without<InInventory>)>,
    q_props: Query<(&Position, &Name, Option<&crate::components::PropKey>), With<Prop>>,
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
    q_monsters: Query<(&Position, &Name, &crate::game::combat::Health, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>, Has<crate::components::Submerged>), With<Monster>>,
    q_items: Query<(&Position, &Name, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>, Option<&StaffData>, Option<&Rechargeable>, Has<crate::components::Drifting>), (With<Item>, Without<InInventory>)>,
    q_props: Query<(&Position, &Name, Option<&crate::components::PropKey>), With<Prop>>,
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

/// Groups extra resources for `spawn_dungeon` to stay within Bevy's
/// 16-SystemParam limit.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SpawnDungeonExtras<'w> {
    auto_save_pending: ResMut<'w, AutoSavePending>,
    needs_explored_init: ResMut<'w, NeedsExploredInit>,
    squad_counter: ResMut<'w, crate::game::squad::SquadIdCounter>,
}

pub fn spawn_dungeon(
    mut commands: Commands,
    floor: Res<Floor>,
    mut map: ResMut<Map>,
    mut turn_manager: ResMut<TurnManager>,
    mut pending_restore: ResMut<PendingFloorRestore>,
    mut pending_game_load: ResMut<PendingGameLoad>,
    mut pending_player_load: ResMut<PendingPlayerLoad>,
    mut player_spawn_point: ResMut<PlayerSpawnPoint>,
    assets: EntityAssets,
    tile_assets: TileAssets,
    mut extras: SpawnDungeonExtras,
    mut tile_index: ResMut<crate::map::tile::TileEntityIndex>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut player_query: Query<(Entity, &mut Position, &mut Transform), With<Player>>,
    turn_marker_query: Query<Entity, (With<TurnMarker>, Without<Player>)>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
) {
    let map_entity = commands.spawn((DungeonECSMap, RenderLayers::layer(1))).id();

    // ---------------------------------------------------------------
    // Determine floor source + handle resource-level side effects
    // ---------------------------------------------------------------
    let source = if let Some(save_data) = pending_game_load.0.take() {
        // Load path: extract resource-level state before materialization
        use crate::save::SavedFloorCache;

        commands.insert_resource(crate::game::squad::SquadIdCounter(save_data.squad_id_counter));

        let saved_floor_cache: std::collections::HashMap<u32, crate::save::CachedFloorSave> =
            save_data.floor_cache.clone();
        commands.insert_resource(SavedFloorCache(saved_floor_cache));

        extras.needs_explored_init.0 = true;

        FloorSource::Load(save_data)
    } else if let Some(cached) = pending_restore.floor.take() {
        // Restore path
        let ascending = pending_restore.ascending;
        extras.needs_explored_init.0 = true;

        FloorSource::Restore { cached, ascending }
    } else {
        // Generate path: run the builder pipeline
        let spawn_table = assets
            .monster_spawn_tables
            .get(&assets.monster_spawn_table_handle.0)
            .unwrap();
        let item_spawn_table = assets
            .item_spawn_tables
            .get(&assets.item_spawn_table_handle.0)
            .unwrap();
        let prefabs = assets
            .prefab_manifests
            .get(&assets.prefab_manifest_handle.0)
            .map(|m| m.prefabs.clone())
            .unwrap_or_default();
        let monster_manifest = assets
            .monster_manifests
            .get(&assets.monster_manifest_handle.0)
            .unwrap();
        let decoration_rules = assets
            .decoration_catalogs
            .get(&assets.decoration_catalog_handle.0)
            .map(|c| c.rules.clone())
            .unwrap_or_default();

        let mut builder = level_builder(
            floor.0 as i32,
            MAP_SIZE.x as i32,
            MAP_SIZE.y as i32,
            &spawn_table.spawns,
            &item_spawn_table.spawns,
            extras.squad_counter.clone(),
            prefabs,
            &monster_manifest.monsters,
            decoration_rules,
        );
        builder.build_map();
        // Write the updated counter back so future floors don't reuse IDs.
        *extras.squad_counter = builder.build_data.squad_counter.clone();

        // Reset RunStats on new game (floor 1, generate path only)
        if floor.0 == 1 {
            commands.insert_resource(crate::game::RunStats::default());
        }

        FloorSource::Generate(builder.build_data)
    };

    // ---------------------------------------------------------------
    // Materialize floor entities (single code path for all sources)
    // ---------------------------------------------------------------
    let result: FloorResult = materialize_floor(
        &mut commands,
        map_entity,
        &assets,
        &tile_assets,
        &mut tile_index,
        &mut turn_manager,
        ascii_font.as_deref(),
        source,
    );

    for warning in &result.warnings {
        warn!("{}", warning);
    }

    *map = result.map;
    let spawn = result.player_spawn;
    let spawn_idx = map.xy_idx(spawn.x, spawn.y);
    let spawn_tile = map.tiles[spawn_idx];
    if !crate::map::tile::is_walkable(spawn_tile) {
        warn!(
            "Player spawn ({}, {}) is NOT walkable! terrain={:?} liquid={:?}",
            spawn.x, spawn.y, spawn_tile.terrain, spawn_tile.liquid
        );
    }
    info!(
        "spawn_dungeon: floor={}, player_spawn=({}, {}), tile={:?}",
        floor.0, spawn.x, spawn.y, spawn_tile.terrain
    );
    player_spawn_point.0 = spawn;

    if let Some(player_save) = result.pending_player_load {
        pending_player_load.0 = Some(player_save);
    }

    // Teleport the player to the spawn point and prevent stair re-trigger.
    if let Ok((player_entity, mut player_pos, mut player_tf)) = player_query.single_mut() {
        info!(
            "spawn_dungeon: teleporting player from ({}, {}) to ({}, {})",
            player_pos.x, player_pos.y, spawn.x, spawn.y
        );
        player_pos.x = spawn.x;
        player_pos.y = spawn.y;
        player_tf.translation.x = spawn.x as f32 * crate::map::map::GRID_SIZE.x;
        player_tf.translation.y = spawn.y as f32 * crate::map::map::GRID_SIZE.y;
        commands.entity(player_entity).insert(crate::map::dungeon::StairCooldown);
        turn_manager.add_entity(player_entity);
    }
    // Re-add turn marker. It persists across floors but the queue was reset.
    for marker_entity in turn_marker_query.iter() {
        turn_manager.add_entity(marker_entity);
    }

    info!(
        "spawn_dungeon: turn_queue has {} entities, current_time={}",
        turn_manager.turn_queue.len(), turn_manager.current_time
    );
    // Log the first few entries to see their scheduled times
    for (i, (entity, time)) in turn_manager.turn_queue.iter().enumerate().take(5) {
        info!("  queue[{}]: entity={:?}, time={}", i, entity, time);
    }

    log_writer.write(GameLogMessage(format!("Welcome to floor {}!", floor.0)));

    // First floor intro — set the atmosphere.
    if floor.0 == 1 {
        log_writer.write(GameLogMessage(
            "The stone steps descend into darkness. You must find a way out."
                .to_string(),
        ));
    }

    // Trigger auto-save after the new floor is fully set up.
    // (Skipped during load since apply_player_load_system hasn't run yet;
    //  auto_save_system checks for the player entity so it will self-correct.)
    extras.auto_save_pending.0 = true;
}

