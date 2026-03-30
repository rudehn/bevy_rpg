use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

use crate::{
    assets::{ItemManifest, ItemManifestHandle, ItemSpriteAssets},
    components::{Equipped, FloorEntityMarker, InInventory, Inventory, Item, Monster, Name, Position, Prop, Viewshed},
    game::{
        AppState,
        combat::{Damage, Health},
        enchantment::{ArmorRunic, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified, WeaponRunic},
        items::{Equipment, ItemProperties, ItemStack},
        magic::StatusEffects,
        spawner::spawn_item,
        squad::{SquadConfig, SquadId, SquadIdCounter, SquadLeader},
        staves::{Rechargeable, StaffData, StaffEffect},
        stats::{Armor, Dodge},
    },
    map::{
        dungeon::{CachedFloor, FloorCache, Floor, PendingGameLoad, PendingPlayerLoad, AutoSavePending},
        map::Map,
        tile::Tile,
    },
    player::Player,
    ui::game_log::GameLog,
};

// ---- Platform-agnostic save I/O ----
//
// Native: read/write a RON file at saves/ironveil_save.ron
// WASM:   read/write the browser's localStorage under key "ironveil_save"

#[allow(dead_code)]
const WASM_SAVE_KEY: &str = "ironveil_save";

#[cfg(not(target_arch = "wasm32"))]
pub const SAVE_FILE: &str = "ironveil_save.ron";

#[cfg(not(target_arch = "wasm32"))]
pub fn save_path() -> PathBuf {
    let dir = PathBuf::from("saves");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(SAVE_FILE)
}

/// Write serialized save data. Returns `true` on success.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_save_data(data: &str) -> bool {
    match std::fs::write(save_path(), data) {
        Ok(()) => true,
        Err(e) => { error!("Failed to write save file: {}", e); false }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn write_save_data(data: &str) -> bool {
    let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
    else {
        error!("localStorage unavailable");
        return false;
    };
    storage.set_item(WASM_SAVE_KEY, data).is_ok()
}

/// Read serialized save data, if any exists.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_save_data() -> Option<String> {
    std::fs::read_to_string(save_path())
        .map_err(|e| warn!("Could not read save file: {}", e))
        .ok()
}

#[cfg(target_arch = "wasm32")]
pub fn read_save_data() -> Option<String> {
    web_sys::window()?
        .local_storage().ok()??
        .get_item(WASM_SAVE_KEY).ok()?
}

/// Returns `true` if a save exists.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_data_exists() -> bool {
    save_path().exists()
}

#[cfg(target_arch = "wasm32")]
pub fn save_data_exists() -> bool {
    read_save_data().is_some()
}

pub fn delete_save() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = save_path();
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!("Failed to delete save file: {}", e);
            } else {
                info!("Save file deleted (permadeath).");
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
        {
            let _ = storage.remove_item(WASM_SAVE_KEY);
            info!("Save deleted from localStorage (permadeath).");
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
    save_exists.0 = save_data_exists();
}

// ---- Serializable data types ----

#[derive(Serialize, Deserialize)]
pub struct GameSaveData {
    pub floor: u32,
    pub game_log: Vec<String>,
    pub map: MapSaveData,
    pub player: PlayerSaveData,
    pub monsters: Vec<SavedMonster>,
    pub floor_items: Vec<SavedItem>,
    #[serde(default)]
    pub props: Vec<SavedProp>,
    pub floor_cache: HashMap<u32, SavedFloorData>,
    #[serde(default)]
    pub squad_id_counter: u64,
}

// ---------------------------------------------------------------------------
// Unified entity types — shared by GameSaveData, CachedFloor, and
// SavedFloorData. Adding a new persistent field only requires updating
// these types + the queries that populate them.
// ---------------------------------------------------------------------------

/// A monster's mutable state, shared by save files and the floor cache.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedMonster {
    pub x: i32,
    pub y: i32,
    pub name: String,
    #[serde(default)]
    pub hp_current: i32,
    #[serde(default)]
    pub squad_id: Option<u64>,
    #[serde(default)]
    pub is_leader: bool,
    #[serde(default)]
    pub squad_config: Option<SquadConfig>,
    #[serde(default)]
    pub patrol_route: Option<crate::game::ai::PatrolRoute>,
    #[serde(default)]
    pub submerged: bool,
}

/// A floor item's mutable state, shared by save files and the floor cache.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedItem {
    pub x: i32,
    pub y: i32,
    pub name: String,
    #[serde(default = "default_stack_count")]
    pub count: u32,
    #[serde(default)]
    pub enchantment: Option<i32>,
    #[serde(default)]
    pub weapon_runic: Option<WeaponRunic>,
    #[serde(default)]
    pub armor_runic: Option<ArmorRunic>,
    #[serde(default)]
    pub runic_identified: Option<bool>,
    #[serde(default)]
    pub staff_effect: Option<StaffEffect>,
    #[serde(default)]
    pub base_recharge: Option<u32>,
    #[serde(default)]
    pub staff_charges: Option<i32>,
    #[serde(default)]
    pub staff_max_charges: Option<i32>,
    #[serde(default)]
    pub staff_recharge_timer: Option<u32>,
    #[serde(default)]
    pub staff_recharge_rate: Option<u32>,
    #[serde(default)]
    pub drifting: bool,
}

/// A prop's state, shared by save files and the floor cache.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedProp {
    pub x: i32,
    pub y: i32,
    pub name: String,
}

/// A complete floor snapshot, shared by the in-memory floor cache and
/// serialized save files. Uses `MapSaveData` so it can be serialized
/// without a separate conversion type.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedFloorData {
    pub map: MapSaveData,
    pub monsters: Vec<SavedMonster>,
    pub items: Vec<SavedItem>,
    #[serde(default)]
    pub props: Vec<SavedProp>,
    pub down_stairs_pos: [i32; 2],
    #[serde(default)]
    pub up_stairs_pos: [i32; 2],
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
    pub armor: i32,
    pub dodge: i32,
    pub viewshed_range: i32,
    pub damage: String,
    #[serde(default)]
    pub status_effects: StatusEffects,
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
    #[serde(default)]
    pub enchantment: Option<i32>,
    #[serde(default)]
    pub weapon_runic: Option<WeaponRunic>,
    #[serde(default)]
    pub armor_runic: Option<ArmorRunic>,
    #[serde(default)]
    pub runic_identified: Option<bool>,
    #[serde(default)]
    pub staff_effect: Option<StaffEffect>,
    #[serde(default)]
    pub base_recharge: Option<u32>,
    #[serde(default)]
    pub staff_charges: Option<i32>,
    #[serde(default)]
    pub staff_max_charges: Option<i32>,
    #[serde(default)]
    pub staff_recharge_timer: Option<u32>,
    #[serde(default)]
    pub staff_recharge_rate: Option<u32>,
}

fn default_stack_count() -> u32 { 1 }
fn default_stack_max() -> u32 { 1 }

/// Backward-compatible alias: save files and `SavedFloorCache` still
/// reference this name.
pub type CachedFloorSave = SavedFloorData;

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
    let tile_count = (data.width * data.height) as usize;
    Map {
        name: data.name.clone(),
        tiles: data.tiles.clone(),
        explored_tiles: data.explored.clone(),
        blocked: vec![false; tile_count],
        width: data.width,
        height: data.height,
        depth: data.depth,
    }
}

pub fn cached_floor_to_save(cached: &CachedFloor) -> SavedFloorData {
    SavedFloorData {
        map: map_to_save_data(&cached.map),
        monsters: cached.monsters.clone(),
        items: cached.items.clone(),
        props: cached.props.clone(),
        down_stairs_pos: [cached.down_stairs_pos.x, cached.down_stairs_pos.y],
        up_stairs_pos: [cached.up_stairs_pos.x, cached.up_stairs_pos.y],
    }
}

pub fn save_to_cached_floor(data: &SavedFloorData) -> CachedFloor {
    CachedFloor {
        map: save_data_to_map(&data.map),
        monsters: data.monsters.clone(),
        items: data.items.clone(),
        props: data.props.clone(),
        down_stairs_pos: Point::new(data.down_stairs_pos[0], data.down_stairs_pos[1]),
        up_stairs_pos: Point::new(data.up_stairs_pos[0], data.up_stairs_pos[1]),
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
            &Armor,
            &Dodge,
            &Inventory,
            &Equipment,
            &Damage,
            &Viewshed,
        ),
        With<Player>,
    >,
    player_status_query: Query<&StatusEffects, With<Player>>,
    inv_item_query: Query<(&Name, &ItemProperties, Has<Equipped>, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>, Option<&StaffData>, Option<&Rechargeable>), With<InInventory>>,
    monster_query: Query<(&Position, &Name, &Health, Option<&SquadId>, Option<&SquadConfig>, Has<SquadLeader>, Option<&crate::game::ai::PatrolRoute>, Has<crate::components::Submerged>), With<Monster>>,
    squad_counter: Res<SquadIdCounter>,
    floor_item_query: Query<(&Position, &Name, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>, Option<&StaffData>, Option<&Rechargeable>, Has<crate::components::Drifting>), (With<Item>, Without<InInventory>)>,
    prop_query: Query<(&Position, &Name), With<Prop>>,
) {
    auto_save_pending.0 = false;

    let Ok((pos, health, armor, dodge, inventory, equipment, damage, viewshed)) =
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
            let Ok((name, props, is_equipped, stack, enchant, weapon_runic, armor_runic, runic_id, staff_data, rechargeable)) = inv_item_query.get(item_entity) else {
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
                enchantment: enchant.map(|e| e.level),
                weapon_runic: weapon_runic.map(|w| w.0),
                armor_runic: armor_runic.map(|a| a.0),
                runic_identified: runic_id.map(|r| r.0),
                staff_effect: staff_data.map(|s| s.effect),
                base_recharge: staff_data.map(|s| s.base_recharge),
                staff_charges: rechargeable.map(|r| r.charges),
                staff_max_charges: rechargeable.map(|r| r.max_charges),
                staff_recharge_timer: rechargeable.map(|r| r.recharge_timer),
                staff_recharge_rate: rechargeable.map(|r| r.recharge_rate),
            })
        })
        .collect();

    // Floor monsters
    let monsters: Vec<SavedMonster> = monster_query
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

    // Floor items (not in inventory)
    let floor_items: Vec<SavedItem> = floor_item_query
        .iter()
        .map(|(pos, name, stack, enchant, weapon_runic, armor_runic, runic_id, staff_data, rechargeable, is_drifting)| SavedItem {
            x: pos.x,
            y: pos.y,
            name: name.0.clone(),
            count: stack.map(|s| s.count).unwrap_or(1),
            enchantment: enchant.map(|e| e.level),
            weapon_runic: weapon_runic.map(|w| w.0),
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

    // Status effects
    let status_effects = player_status_query
        .single()
        .cloned()
        .unwrap_or_default();

    // Props
    let props: Vec<SavedProp> = prop_query
        .iter()
        .map(|(pos, name)| SavedProp { x: pos.x, y: pos.y, name: name.0.clone() })
        .collect();

    // Floor cache
    let floor_cache_save: HashMap<u32, SavedFloorData> = floor_cache
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
            armor: armor.0,
            dodge: dodge.0,
            viewshed_range: viewshed.range,
            damage: damage.0.clone(),
            status_effects,
            inventory: inv_saves,
        },
        monsters,
        floor_items,
        props,
        floor_cache: floor_cache_save,
        squad_id_counter: squad_counter.0,
    };

    match ron::ser::to_string_pretty(&save_data, ron::ser::PrettyConfig::default()) {
        Ok(serialized) => {
            if write_save_data(&serialized) {
                info!("Game saved.");
                save_exists.0 = true;
            }
        }
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
            &mut Armor,
            &mut Dodge,
            &mut Inventory,
            &mut Equipment,
            &mut Damage,
            &mut Viewshed,
        ),
        With<Player>,
    >,
    player_entity_query: Query<Entity, With<Player>>,
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
        mut armor,
        mut dodge,
        mut inventory,
        mut equipment,
        mut damage,
        mut viewshed,
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

    // --- Armor / Dodge ---
    armor.0 = player_data.armor;
    dodge.0 = player_data.dodge;

    // --- Damage / Viewshed ---
    damage.0 = player_data.damage.clone();
    viewshed.range = player_data.viewshed_range;
    viewshed.dirty = true;

    // --- Status effects ---
    if let Ok(player_entity) = player_entity_query.single() {
        commands.entity(player_entity)
            .insert(player_data.status_effects.clone());
    }

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
            None,
            None,
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

        // Restore enchantment and runic data
        if let Some(level) = item_save.enchantment {
            commands.entity(item_entity).insert(Enchantment { level });
        }
        if let Some(runic) = item_save.weapon_runic {
            commands.entity(item_entity).insert(ItemWeaponRunic(runic));
        }
        if let Some(runic) = item_save.armor_runic {
            commands.entity(item_entity).insert(ItemArmorRunic(runic));
        }
        if let Some(identified) = item_save.runic_identified {
            commands.entity(item_entity).insert(RunicIdentified(identified));
        }

        // Restore staff data
        if let Some(effect) = item_save.staff_effect {
            let base_recharge = item_save.base_recharge.unwrap_or(250);
            commands.entity(item_entity).insert(StaffData { effect, base_recharge });
            if let (Some(charges), Some(max_charges), Some(recharge_timer), Some(recharge_rate)) = (
                item_save.staff_charges,
                item_save.staff_max_charges,
                item_save.staff_recharge_timer,
                item_save.staff_recharge_rate,
            ) {
                commands.entity(item_entity).insert(Rechargeable {
                    charges,
                    max_charges,
                    recharge_timer,
                    recharge_rate,
                });
            }
        }

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
pub struct SavedFloorCache(pub HashMap<u32, SavedFloorData>);
