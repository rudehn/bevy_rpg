use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::components::{FloorEntityMarker, InInventory, Monster, Name, Position, Item, Prop};
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
                (
                    spawn_dungeon
                        .run_if(on_message::<SpawnDungeonMessage>),
                    apply_floor_entry_shrine_effects
                        .run_if(on_message::<SpawnDungeonMessage>)
                        .after(spawn_dungeon),
                )
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
    monster_query: &Query<(&Position, &Name, &crate::game::combat::Health, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>), With<Monster>>,
    item_query: &Query<(&Position, &Name, Option<&ItemStack>), (With<Item>, Without<InInventory>)>,
    prop_query: &Query<(&Position, &Name), With<Prop>>,
) -> CachedFloor {
    use crate::save::{SavedMonster, SavedItem, SavedProp};

    let monsters = monster_query
        .iter()
        .map(|(pos, name, health, squad_id, squad_config, is_leader, patrol_route)| SavedMonster {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            hp_current: health.current,
            squad_id: squad_id.map(|s| s.0),
            is_leader,
            squad_config: squad_config.cloned(),
            patrol_route: patrol_route.cloned(),
        })
        .collect();

    let items = item_query
        .iter()
        .map(|(pos, name, stack)| SavedItem {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            count: stack.map(|s| s.count).unwrap_or(1),
        })
        .collect();

    let props = prop_query
        .iter()
        .map(|(pos, name)| SavedProp {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
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
    q_monsters: Query<(&Position, &Name, &crate::game::combat::Health, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>), With<Monster>>,
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
    q_monsters: Query<(&Position, &Name, &crate::game::combat::Health, Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>, Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>), With<Monster>>,
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

/// Groups extra resources for `spawn_dungeon` to stay within Bevy's
/// 16-SystemParam limit.
#[derive(bevy::ecs::system::SystemParam)]
struct SpawnDungeonExtras<'w> {
    auto_save_pending: ResMut<'w, AutoSavePending>,
    needs_explored_init: ResMut<'w, NeedsExploredInit>,
    squad_counter: ResMut<'w, crate::game::squad::SquadIdCounter>,
    shrines_purchased: Res<'w, crate::game::shrines::ShrinesPurchased>,
}

pub fn spawn_dungeon(
    mut commands: Commands,
    floor: Res<Floor>,
    mut map: ResMut<Map>,
    mut turn_manager: ResMut<TurnManager>,
    mut pending_restore: ResMut<PendingFloorRestore>,
    mut pending_game_load: ResMut<PendingGameLoad>,
    mut pending_player_load: ResMut<PendingPlayerLoad>,
    assets: EntityAssets,
    tile_assets: TileAssets,
    mut extras: SpawnDungeonExtras,
    mut log_writer: MessageWriter<GameLogMessage>,
    player_query: Query<Entity, With<Player>>,
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
        commands.insert_resource(save_data.tyrant_aspects.clone());

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
        let shrine_categories = assets
            .shrines_catalogs
            .get(&assets.shrines_catalog_handle.0)
            .map(|c| c.categories.clone())
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
            shrine_categories,
            &extras.shrines_purchased,
        );
        builder.build_map();
        // Write the updated counter back so future floors don't reuse IDs.
        *extras.squad_counter = builder.build_data.squad_counter.clone();

        // Initialize TyrantAspects and reset RunStats on new game (floor 1, generate path only)
        if floor.0 == 1 {
            commands.insert_resource(crate::game::boss::TyrantAspects::new_random());
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
    commands.insert_resource(PlayerSpawnPoint(spawn));

    if let Some(player_save) = result.pending_player_load {
        pending_player_load.0 = Some(player_save);
    }

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
            "The stone steps descend into darkness. Somewhere far below, the Veiled Tyrant stirs."
                .to_string(),
        ));
        log_writer.write(GameLogMessage(
            "Its power grows with every passing moment. You must reach the depths before it becomes unstoppable."
                .to_string(),
        ));
    }

    // Trigger auto-save after the new floor is fully set up.
    // (Skipped during load since apply_player_load_system hasn't run yet;
    //  auto_save_system checks for the player entity so it will self-correct.)
    extras.auto_save_pending.0 = true;
}

/// Apply shrine effects that trigger on floor entry:
/// - ManaWell: restore mana to max
/// - SecondWind: reset availability
fn apply_floor_entry_shrine_effects(
    mut player_query: Query<(
        Option<&crate::game::shrines::ManaWellAbility>,
        Option<&mut crate::game::shrines::SecondWindAbility>,
        Option<&mut crate::game::stats::Mana>,
    ), With<Player>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Ok((mana_well, second_wind, mana)) = player_query.single_mut() else {
        return;
    };

    // ManaWell: full mana on floor entry
    if mana_well.is_some() {
        if let Some(mut mana) = mana {
            if mana.current < mana.max {
                mana.current = mana.max;
                log_writer.write(GameLogMessage(
                    "Mana Well: your mana is fully restored!".to_string(),
                ));
            }
        }
    }

    // SecondWind: reset availability on new floor
    if let Some(mut sw) = second_wind {
        if !sw.available {
            sw.available = true;
            log_writer.write(GameLogMessage(
                "Second Wind is available again.".to_string(),
            ));
        }
    }
}
