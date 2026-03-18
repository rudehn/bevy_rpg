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
        boss::TyrantPower,
        combat::{Damage, Health},
        essence::Essence,
        items::{Equipment, ItemProperties, ItemStack},
        magic::{
            ActiveSpells, Hasted, KnownSpells, ManaRegen, Slowed,
            SpellCooldowns, Stunned,
        },
        spawner::spawn_item,
        squad::{SquadConfig, SquadId, SquadIdCounter, SquadLeader},
        stats::{Armor, Dodge, Mana},
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
    pub monsters: Vec<MonsterEntry>,
    pub floor_items: Vec<ItemEntry>,
    #[serde(default)]
    pub props: Vec<PropEntry>,
    pub floor_cache: HashMap<u32, CachedFloorSave>,
    #[serde(default)]
    pub squad_id_counter: u64,
    #[serde(default)]
    pub tyrant_power: TyrantPower,
}

#[derive(Serialize, Deserialize)]
pub struct PropEntry {
    pub x: i32,
    pub y: i32,
    pub name: String,
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
    pub essence_current: i32,
    pub essence_lifetime: i32,
    pub viewshed_range: i32,
    pub damage: String,
    pub mana_current: i32,
    #[serde(default)]
    pub known_spells: KnownSpells,
    #[serde(default)]
    pub active_spells: ActiveSpells,
    #[serde(default)]
    pub mana_regen: ManaRegen,
    #[serde(default)]
    pub spell_cooldowns: SpellCooldowns,
    #[serde(default)]
    pub hasted: Option<Hasted>,
    #[serde(default)]
    pub slowed: Option<Slowed>,
    #[serde(default)]
    pub stunned: Option<Stunned>,
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
    #[serde(default)]
    pub squad_id: Option<u64>,
    #[serde(default)]
    pub is_leader: bool,
    #[serde(default)]
    pub squad_config: Option<SquadConfig>,
    #[serde(default)]
    pub patrol_route: Option<crate::game::ai::PatrolRoute>,
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
    pub monster_list: Vec<CachedMonsterSave>,
    pub item_list: Vec<([i32; 2], String, u32)>,
    #[serde(default)]
    pub prop_list: Vec<([i32; 2], String)>,
    pub down_stairs_pos: [i32; 2],
    #[serde(default)]
    pub up_stairs_pos: [i32; 2],
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CachedMonsterSave {
    pub pos: [i32; 2],
    pub name: String,
    #[serde(default)]
    pub squad_id: Option<u64>,
    #[serde(default)]
    pub is_leader: bool,
    #[serde(default)]
    pub squad_config: Option<SquadConfig>,
    #[serde(default)]
    pub patrol_route: Option<crate::game::ai::PatrolRoute>,
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

pub fn cached_floor_to_save(cached: &CachedFloor) -> CachedFloorSave {
    CachedFloorSave {
        map: map_to_save_data(&cached.map),
        monster_list: cached
            .monster_list
            .iter()
            .map(|m| CachedMonsterSave {
                pos: [m.pos.x, m.pos.y],
                name: m.name.clone(),
                squad_id: m.squad_id,
                is_leader: m.is_leader,
                squad_config: m.squad_config.clone(),
                patrol_route: m.patrol_route.clone(),
            })
            .collect(),
        item_list: cached
            .item_list
            .iter()
            .map(|(pt, name, count)| ([pt.x, pt.y], name.clone(), *count))
            .collect(),
        prop_list: cached
            .prop_list
            .iter()
            .map(|(pt, name)| ([pt.x, pt.y], name.clone()))
            .collect(),
        down_stairs_pos: [cached.down_stairs_pos.x, cached.down_stairs_pos.y],
        up_stairs_pos: [cached.up_stairs_pos.x, cached.up_stairs_pos.y],
    }
}

pub fn save_to_cached_floor(data: &CachedFloorSave) -> CachedFloor {
    use crate::map::dungeon::CachedMonster;
    CachedFloor {
        map: save_data_to_map(&data.map),
        monster_list: data
            .monster_list
            .iter()
            .map(|m| CachedMonster {
                pos: Point::new(m.pos[0], m.pos[1]),
                name: m.name.clone(),
                squad_id: m.squad_id,
                is_leader: m.is_leader,
                squad_config: m.squad_config.clone(),
                patrol_route: m.patrol_route.clone(),
            })
            .collect(),
        item_list: data
            .item_list
            .iter()
            .map(|(pos, name, count)| (Point::new(pos[0], pos[1]), name.clone(), *count))
            .collect(),
        prop_list: data
            .prop_list
            .iter()
            .map(|(pos, name)| (Point::new(pos[0], pos[1]), name.clone()))
            .collect(),
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
            &Essence,
            &Inventory,
            &Equipment,
            &Damage,
            &Viewshed,
            &Mana,
        ),
        With<Player>,
    >,
    player_magic_query: Query<
        (
            &KnownSpells,
            &ActiveSpells,
            &ManaRegen,
            &SpellCooldowns,
            Option<&Hasted>,
            Option<&Slowed>,
            Option<&Stunned>,
        ),
        With<Player>,
    >,
    inv_item_query: Query<(&Name, &ItemProperties, Has<Equipped>, Option<&ItemStack>), With<InInventory>>,
    monster_query: Query<(&Position, &Name, &Health, Option<&SquadId>, Option<&SquadConfig>, Has<SquadLeader>, Option<&crate::game::ai::PatrolRoute>), With<Monster>>,
    squad_counter: Res<SquadIdCounter>,
    floor_item_query: Query<(&Position, &Name, Option<&ItemStack>), (With<Item>, Without<InInventory>)>,
    prop_query: Query<(&Position, &Name), With<Prop>>,
    tyrant_power: Res<TyrantPower>,
) {
    auto_save_pending.0 = false;

    let Ok((pos, health, armor, dodge, essence, inventory, equipment, damage, viewshed, mana)) =
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
        .map(|(pos, name, health, squad_id, squad_config, is_leader, patrol_route)| MonsterEntry {
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

    // Magic state
    let (known_spells, active_spells, mana_regen, spell_cooldowns, hasted, slowed, stunned) =
        if let Ok((ks, as_, mr, sc, h, sl, st)) = player_magic_query.single() {
            (
                ks.clone(),
                as_.clone(),
                mr.clone(),
                sc.clone(),
                h.cloned(),
                sl.cloned(),
                st.cloned(),
            )
        } else {
            (
                KnownSpells::default(),
                ActiveSpells::default(),
                ManaRegen::default(),
                SpellCooldowns::default(),
                None, None, None,
            )
        };

    // Props
    let props: Vec<PropEntry> = prop_query
        .iter()
        .map(|(pos, name)| PropEntry { x: pos.x, y: pos.y, name: name.0.clone() })
        .collect();

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
            armor: armor.0,
            dodge: dodge.0,
            essence_current: essence.current,
            essence_lifetime: essence.lifetime,
            viewshed_range: viewshed.range,
            damage: damage.0.clone(),
            mana_current: mana.current,
            known_spells,
            active_spells,
            mana_regen,
            spell_cooldowns,
            hasted,
            slowed,
            stunned,
            inventory: inv_saves,
        },
        monsters,
        floor_items,
        props,
        floor_cache: floor_cache_save,
        squad_id_counter: squad_counter.0,
        tyrant_power: tyrant_power.clone(),
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
            &mut Essence,
            &mut Inventory,
            &mut Equipment,
            &mut Damage,
            &mut Viewshed,
            &mut Mana,
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
        mut essence,
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

    // --- Armor / Dodge / Essence ---
    armor.0 = player_data.armor;
    dodge.0 = player_data.dodge;
    essence.current = player_data.essence_current;
    essence.lifetime = player_data.essence_lifetime;

    // --- Damage / Viewshed / Mana ---
    damage.0 = player_data.damage.clone();
    viewshed.range = player_data.viewshed_range;
    viewshed.dirty = true;
    mana.current = player_data.mana_current;

    // --- Magic state ---
    if let Ok(player_entity) = player_entity_query.single() {
        commands.entity(player_entity)
            .insert(player_data.known_spells.clone())
            .insert(player_data.active_spells.clone())
            .insert(player_data.mana_regen.clone())
            .insert(player_data.spell_cooldowns.clone());

        if let Some(ref h) = player_data.hasted {
            commands.entity(player_entity).insert(h.clone());
        }
        if let Some(ref s) = player_data.slowed {
            commands.entity(player_entity).insert(s.clone());
        }
        if let Some(ref st) = player_data.stunned {
            commands.entity(player_entity).insert(st.clone());
        }
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
