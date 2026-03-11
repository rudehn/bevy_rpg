use std::{collections::HashMap, path::PathBuf};

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

use crate::{
    assets::{ItemManifest, ItemManifestHandle, ItemSpriteAssets},
    components::{Equipped, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Item, Monster, Name, Position, Viewshed},
    game::{
        AppState,
        combat::{Damage, Health},
        items::{Equipment, ItemProperties, ItemStack},
        level::{AvailableStatPoints, Experience},
        spawner::spawn_item,
        stats::{AttributeModifiers, Attributes, Level, Mana},
    },
    map::{
        dungeon::{CachedFloor, FloorCache, Floor, PendingGameLoad, PendingPlayerLoad, AutoSavePending},
        light::Candle,
        map::Map,
        tile::Tile,
    },
    player::Player,
    ui::game_log::GameLog,
};

// ---- Save file path ----

pub const SAVE_FILE: &str = "ironveil_save.ron";

pub fn save_path() -> PathBuf {
    let dir = PathBuf::from("saves");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(SAVE_FILE)
}

pub fn delete_save() {
    let path = save_path();
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            warn!("Failed to delete save file: {}", e);
        } else {
            info!("Save file deleted (permadeath).");
        }
    }
}

// ---- Resources ----

/// Whether a save file currently exists. Read by the menu to enable the Continue button.
#[derive(Resource, Default)]
pub struct SaveExists(pub bool);

// ---- Temporary component ----

/// Placed on monsters during load; overrides their HP once stat_recalculation_system runs.
#[derive(Component)]
pub struct SavedHp(pub i32);

// ---- Plugin ----

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveExists>()
            .init_resource::<PendingGameLoad>()
            .init_resource::<PendingPlayerLoad>()
            .init_resource::<AutoSavePending>()
            .add_systems(Startup, check_save_exists)
            .add_systems(OnEnter(AppState::Menu), check_save_exists)
            .add_systems(
                Update,
                (
                    apply_player_load_system.run_if(|r: Res<PendingPlayerLoad>| r.0.is_some()),
                    apply_saved_hp_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // auto_save and exit-save both run in Last so they execute AFTER Bevy's
            // close_when_requested system (which runs in Update and sends AppExit).
            // The runner only checks AppExit after the full schedule (including Last),
            // so the save completes in the same frame the window is closed.
            .add_systems(
                Last,
                (
                    save_on_exit_system.before(auto_save_system),
                    auto_save_system.run_if(|r: Res<AutoSavePending>| r.0),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn check_save_exists(mut save_exists: ResMut<SaveExists>) {
    save_exists.0 = save_path().exists();
}

// ---- Serializable data types ----

#[derive(Serialize, Deserialize)]
pub struct GameSaveData {
    pub floor: u32,
    pub game_log: Vec<String>,
    pub map: MapSaveData,
    pub player: PlayerSaveData,
    pub monsters: Vec<MonsterEntry>,
    pub floor_items: Vec<ItemEntry>,
    pub candles: Vec<[i32; 2]>,
    pub floor_cache: HashMap<u32, CachedFloorSave>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MapSaveData {
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub name: String,
    pub tiles: Vec<Tile>,
    pub explored: Vec<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct PlayerSaveData {
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub level: i32,
    pub xp: i32,
    pub xp_to_next: i32,
    pub spell_slots_unlocked: u8,
    pub stat_points: u32,
    pub str: i32,
    pub dex: i32,
    pub con: i32,
    pub agi: i32,
    pub int: i32,
    pub per: i32,
    pub str_mod: i32,
    pub dex_mod: i32,
    pub con_mod: i32,
    pub agi_mod: i32,
    pub int_mod: i32,
    pub per_mod: i32,
    pub viewshed_range: i32,
    pub damage: String,
    pub mana_current: i32,
    pub inventory: Vec<InventoryItemSave>,
}

#[derive(Serialize, Deserialize)]
pub struct InventoryItemSave {
    pub name: String,
    pub properties: ItemProperties,
    pub equipped_slot: Option<String>,
    #[serde(default = "default_stack_count")]
    pub count: u32,
    #[serde(default = "default_stack_max")]
    pub max_stack: u32,
}

fn default_stack_count() -> u32 { 1 }
fn default_stack_max() -> u32 { 1 }

#[derive(Serialize, Deserialize)]
pub struct MonsterEntry {
    pub x: i32,
    pub y: i32,
    pub name: String,
    pub hp_current: i32,
}

#[derive(Serialize, Deserialize)]
pub struct ItemEntry {
    pub x: i32,
    pub y: i32,
    pub name: String,
    #[serde(default = "default_stack_count")]
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CachedFloorSave {
    pub map: MapSaveData,
    pub monster_list: Vec<([i32; 2], String)>,
    pub item_list: Vec<([i32; 2], String, u32)>,
    pub candle_spawn_points: Vec<[i32; 2]>,
    pub down_stairs_pos: [i32; 2],
}

// ---- Conversion helpers ----

pub fn map_to_save_data(map: &Map) -> MapSaveData {
    MapSaveData {
        width: map.width,
        height: map.height,
        depth: map.depth,
        name: map.name.clone(),
        tiles: map.tiles.clone(),
        explored: map.explored_tiles.clone(),
    }
}

pub fn save_data_to_map(data: &MapSaveData) -> Map {
    Map {
        name: data.name.clone(),
        tiles: data.tiles.clone(),
        explored_tiles: data.explored.clone(),
        width: data.width,
        height: data.height,
        depth: data.depth,
    }
}

pub fn cached_floor_to_save(cached: &CachedFloor) -> CachedFloorSave {
    CachedFloorSave {
        map: map_to_save_data(&cached.map),
        monster_list: cached
            .monster_list
            .iter()
            .map(|(pt, name)| ([pt.x, pt.y], name.clone()))
            .collect(),
        item_list: cached
            .item_list
            .iter()
            .map(|(pt, name, count)| ([pt.x, pt.y], name.clone(), *count))
            .collect(),
        candle_spawn_points: cached
            .candle_spawn_points
            .iter()
            .map(|pt| [pt.x, pt.y])
            .collect(),
        down_stairs_pos: [cached.down_stairs_pos.x, cached.down_stairs_pos.y],
    }
}

pub fn save_to_cached_floor(data: &CachedFloorSave) -> CachedFloor {
    CachedFloor {
        map: save_data_to_map(&data.map),
        monster_list: data
            .monster_list
            .iter()
            .map(|(pos, name)| (Point::new(pos[0], pos[1]), name.clone()))
            .collect(),
        item_list: data
            .item_list
            .iter()
            .map(|(pos, name, count)| (Point::new(pos[0], pos[1]), name.clone(), *count))
            .collect(),
        candle_spawn_points: data
            .candle_spawn_points
            .iter()
            .map(|pos| Point::new(pos[0], pos[1]))
            .collect(),
        down_stairs_pos: Point::new(data.down_stairs_pos[0], data.down_stairs_pos[1]),
    }
}

// ---- Auto-save system ----

#[allow(clippy::too_many_arguments)]
pub fn auto_save_system(
    mut auto_save_pending: ResMut<AutoSavePending>,
    mut save_exists: ResMut<SaveExists>,
    map: Res<Map>,
    floor: Res<Floor>,
    game_log: Res<GameLog>,
    floor_cache: Res<FloorCache>,
    player_query: Query<
        (
            &Position,
            &Health,
            &Level,
            &Experience,
            &AvailableStatPoints,
            &Attributes,
            &AttributeModifiers,
            &Inventory,
            &Equipment,
            &Damage,
            &Viewshed,
            &Mana,
        ),
        With<Player>,
    >,
    inv_item_query: Query<(&Name, &ItemProperties, Has<Equipped>, Option<&ItemStack>), With<InInventory>>,
    monster_query: Query<(&Position, &Name, &Health), With<Monster>>,
    floor_item_query: Query<(&Position, &Name, Option<&ItemStack>), (With<Item>, Without<InInventory>)>,
    candle_query: Query<&Position, With<Candle>>,
) {
    auto_save_pending.0 = false;

    let Ok((pos, health, level, exp, stat_points, attrs, attr_mods, inventory, equipment, damage, viewshed, mana)) =
        player_query.single()
    else {
        warn!("Auto-save skipped: no player entity found.");
        return;
    };

    // Inventory items
    let inv_saves: Vec<InventoryItemSave> = inventory
        .items
        .iter()
        .filter_map(|&item_entity| {
            let Ok((name, props, is_equipped, stack)) = inv_item_query.get(item_entity) else {
                return None;
            };
            let equipped_slot = if is_equipped {
                equipment.find_slot(item_entity).map(|s| s.to_string())
            } else {
                None
            };
            let (count, max_stack) = stack.map(|s| (s.count, s.max_stack)).unwrap_or((1, 1));
            Some(InventoryItemSave {
                name: name.0.clone(),
                properties: props.clone(),
                equipped_slot,
                count,
                max_stack,
            })
        })
        .collect();

    // Floor monsters
    let monsters: Vec<MonsterEntry> = monster_query
        .iter()
        .map(|(pos, name, health)| MonsterEntry {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            hp_current: health.current,
        })
        .collect();

    // Floor items (not in inventory)
    let floor_items: Vec<ItemEntry> = floor_item_query
        .iter()
        .map(|(pos, name, stack)| ItemEntry {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            count: stack.map(|s| s.count).unwrap_or(1),
        })
        .collect();

    // Candles
    let candles: Vec<[i32; 2]> = candle_query.iter().map(|pos| [pos.x, pos.y]).collect();

    // Floor cache
    let floor_cache_save: HashMap<u32, CachedFloorSave> = floor_cache
        .0
        .iter()
        .map(|(k, v)| (*k, cached_floor_to_save(v)))
        .collect();

    let save_data = GameSaveData {
        floor: floor.0,
        game_log: game_log.entries.clone(),
        map: map_to_save_data(&map),
        player: PlayerSaveData {
            x: pos.x,
            y: pos.y,
            hp: health.current,
            level: level.value,
            xp: exp.current,
            xp_to_next: exp.next_level,
            spell_slots_unlocked: exp.spell_slots_unlocked,
            stat_points: stat_points.0,
            str: attrs.strength,
            dex: attrs.dexterity,
            con: attrs.constitution,
            agi: attrs.agility,
            int: attrs.intelligence,
            per: attrs.perception,
            str_mod: attr_mods.strength,
            dex_mod: attr_mods.dexterity,
            con_mod: attr_mods.constitution,
            agi_mod: attr_mods.agility,
            int_mod: attr_mods.intelligence,
            per_mod: attr_mods.perception,
            viewshed_range: viewshed.range,
            damage: damage.0.clone(),
            mana_current: mana.current,
            inventory: inv_saves,
        },
        monsters,
        floor_items,
        candles,
        floor_cache: floor_cache_save,
    };

    match ron::ser::to_string_pretty(&save_data, ron::ser::PrettyConfig::default()) {
        Ok(serialized) => match std::fs::write(save_path(), serialized) {
            Ok(()) => {
                info!("Game saved to {:?}", save_path());
                save_exists.0 = true;
            }
            Err(e) => error!("Failed to write save file: {}", e),
        },
        Err(e) => error!("Failed to serialize save data: {}", e),
    }
}

// ---- Player load system ----
// Runs one time after spawn_dungeon sets PendingPlayerLoad.

pub fn apply_player_load_system(
    mut pending: ResMut<PendingPlayerLoad>,
    mut commands: Commands,
    mut player_query: Query<
        (
            &mut Position,
            &mut Health,
            &mut Level,
            &mut Experience,
            &mut AvailableStatPoints,
            &mut Attributes,
            &mut AttributeModifiers,
            &mut Inventory,
            &mut Equipment,
            &mut Damage,
            &mut Viewshed,
            &mut Mana,
        ),
        With<Player>,
    >,
    item_manifests: Res<Assets<ItemManifest>>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_sprite_assets: Res<ItemSpriteAssets>,
    mut floor_cache: ResMut<FloorCache>,
    saved_floor_cache: Option<Res<SavedFloorCache>>,
    mut save_exists: ResMut<SaveExists>,
) {
    let Some(player_data) = pending.0.take() else { return };

    let Ok((
        mut pos,
        mut health,
        mut level,
        mut exp,
        mut stat_points,
        mut attrs,
        mut attr_mods,
        mut inventory,
        mut equipment,
        mut damage,
        mut viewshed,
        mut mana,
    )) = player_query.single_mut()
    else {
        warn!("apply_player_load_system: no player entity yet, requeueing.");
        pending.0 = Some(player_data);
        return;
    };

    // --- Position ---
    pos.x = player_data.x;
    pos.y = player_data.y;

    // --- Health ---
    health.current = player_data.hp;

    // --- Level / XP ---
    level.value = player_data.level;
    exp.current = player_data.xp;
    exp.next_level = player_data.xp_to_next;
    exp.spell_slots_unlocked = player_data.spell_slots_unlocked;
    stat_points.0 = player_data.stat_points;

    // --- Attributes ---
    attrs.strength = player_data.str;
    attrs.dexterity = player_data.dex;
    attrs.constitution = player_data.con;
    attrs.agility = player_data.agi;
    attrs.intelligence = player_data.int;
    attrs.perception = player_data.per;

    // --- Attribute modifiers (from equipment) ---
    attr_mods.strength = player_data.str_mod;
    attr_mods.dexterity = player_data.dex_mod;
    attr_mods.constitution = player_data.con_mod;
    attr_mods.agility = player_data.agi_mod;
    attr_mods.intelligence = player_data.int_mod;
    attr_mods.perception = player_data.per_mod;

    // --- Damage / Viewshed / Mana ---
    damage.0 = player_data.damage.clone();
    viewshed.range = player_data.viewshed_range;
    viewshed.dirty = true;
    mana.current = player_data.mana_current;

    // --- Inventory ---
    inventory.items.clear();
    *equipment = Equipment::default();

    let dummy_pt = Point::new(0, 0);
    for item_save in &player_data.inventory {
        let Some(item_entity) = spawn_item(
            &mut commands,
            &item_save.name,
            &dummy_pt,
            &item_manifests,
            &item_manifest_handle,
            &item_sprite_assets,
        ) else {
            continue;
        };

        // Override properties from save (preserves any stat tweaks)
        commands
            .entity(item_entity)
            .insert(item_save.properties.clone())
            .insert(ItemStack { count: item_save.count, max_stack: item_save.max_stack })
            .insert(InInventory)
            .insert(Visibility::Hidden)
            .remove::<FloorEntityMarker>();

        inventory.items.push(item_entity);

        if let Some(ref slot) = item_save.equipped_slot {
            commands.entity(item_entity).insert(Equipped);
            equipment.set_slot(slot, Some(item_entity));
        }
    }

    // --- Restore floor cache ---
    if let Some(saved_cache) = saved_floor_cache {
        for (floor_num, cached_save) in &saved_cache.0 {
            floor_cache.0.insert(*floor_num, save_to_cached_floor(cached_save));
        }
    }

    save_exists.0 = true;
    info!("Player state restored from save.");
}

// ---- Save-on-exit system ----
// Triggers auto_save_system in the same frame when the app is about to exit.

pub fn save_on_exit_system(
    mut exit_events: MessageReader<AppExit>,
    mut auto_save_pending: ResMut<AutoSavePending>,
) {
    if !exit_events.is_empty() {
        exit_events.clear();
        auto_save_pending.0 = true;
    }
}

// ---- HP override system ----
// Applies SavedHp to monsters after stat_recalculation_system has run.

pub fn apply_saved_hp_system(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Health, &SavedHp)>,
) {
    for (entity, mut health, saved) in query.iter_mut() {
        health.current = saved.0.min(health.max);
        commands.entity(entity).remove::<SavedHp>();
    }
}

// ---- Helper resource to pass floor cache through the load pipeline ----

/// Temporarily holds the serialized floor cache loaded from disk.
/// Consumed by apply_player_load_system to restore FloorCache.
#[derive(Resource, Default)]
pub struct SavedFloorCache(pub HashMap<u32, CachedFloorSave>);
