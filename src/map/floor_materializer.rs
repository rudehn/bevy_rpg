use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::assets::{
    DecorationCatalog, DecorationCatalogHandle, ItemManifest, ItemManifestHandle, ItemSpawnTable,
    ItemSpawnTableHandle, ItemSpriteAssets, MonsterManifest, MonsterManifestHandle,
    MonsterSpawnTable, MonsterSpawnTableHandle, MonsterSpriteAssets, PrefabManifest,
    PrefabManifestHandle, PropManifest, PropManifestHandle, PropSpriteAssets, TileManifest,
    TileManifestHandle, TileSpriteAssets,
};
use crate::components::Position;
use crate::game::ai::PatrolRoute;
use crate::game::items::ItemStack;
use crate::game::squad::{SquadConfig, SquadId, SquadLeader};
use crate::game::TurnManager;
use crate::game::{spawn_item, spawn_monster_by_name, spawn_prop};
use crate::map::builders::BuilderMap;
use crate::map::tile::{is_walkable, spawn_tile_entity, TerrainType};
use crate::map::Map;
use crate::save::{
    save_data_to_map, GameSaveData, SavedHp, SavedItem, SavedMonster, SavedProp,
};

use super::dungeon::CachedFloor;

// ---------------------------------------------------------------------------
// SystemParam bundles (moved from dungeon.rs)
// ---------------------------------------------------------------------------

/// Groups the three tile-asset resources to keep `spawn_dungeon`'s parameter
/// count within Bevy's 16-parameter limit for system functions.
#[derive(SystemParam)]
pub struct TileAssets<'w> {
    pub manifests: Res<'w, Assets<TileManifest>>,
    pub manifest_handle: Res<'w, TileManifestHandle>,
    pub sprite_assets: Res<'w, TileSpriteAssets>,
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
    pub decoration_catalogs: Res<'w, Assets<DecorationCatalog>>,
    pub decoration_catalog_handle: Res<'w, DecorationCatalogHandle>,
}

// ---------------------------------------------------------------------------
// Private intermediate types — the uniform floor plan
// ---------------------------------------------------------------------------

struct MonsterPlan {
    pos: Point,
    name: String,
    squad_id: Option<u64>,
    is_leader: bool,
    squad_config: Option<SquadConfig>,
    patrol_route: Option<PatrolRoute>,
    saved_hp: Option<i32>,
}

struct ItemPlan {
    pos: Point,
    name: String,
    count: u32,
}

struct PropPlan {
    pos: Point,
    name: String,
}

struct FloorPlan {
    map: Map,
    monsters: Vec<MonsterPlan>,
    items: Vec<ItemPlan>,
    props: Vec<PropPlan>,
    player_spawn: Point,
    /// Carried through from the Load path for the caller.
    pending_player_load: Option<crate::save::PlayerSaveData>,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// What to build the floor from. Wraps raw source data directly.
pub enum FloorSource {
    /// Fresh generation from the builder pipeline.
    Generate(BuilderMap),
    /// Restoring a previously visited floor from cache.
    Restore {
        cached: CachedFloor,
        ascending: bool,
    },
    /// Loading from a save file.
    Load(Box<GameSaveData>),
}

/// Everything the caller needs after materialization.
pub struct FloorResult {
    pub player_spawn: Point,
    pub map: Map,
    /// Only set on the Load path — player state to restore.
    pub pending_player_load: Option<crate::save::PlayerSaveData>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
        if is_walkable(*tile) && tile.terrain != DownStairs && tile.terrain != UpStairs {
            let (x, y) = map.idx_xy(idx);
            Some(Point::new(x, y))
        } else {
            None
        }
    })
}

pub(crate) fn spawn_tiles_into_ecs(
    commands: &mut Commands,
    map_entity: Entity,
    game_map: &Map,
    tile_assets: &TileAssets,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
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
                ascii_font,
            );
            commands
                .entity(tile_entity)
                .insert(Position { x: pt.x, y: pt.y });
        }
    }
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

/// Convert saved monsters to MonsterPlans. `restore_hp` controls whether
/// saved HP is applied (true for cache/load, false would skip).
fn monsters_from_saved(saved: Vec<SavedMonster>, restore_hp: bool) -> Vec<MonsterPlan> {
    saved
        .into_iter()
        .map(|m| MonsterPlan {
            pos: Point::new(m.x, m.y),
            name: m.name,
            squad_id: m.squad_id,
            is_leader: m.is_leader,
            squad_config: m.squad_config,
            patrol_route: m.patrol_route,
            saved_hp: if restore_hp && m.hp_current > 0 {
                Some(m.hp_current)
            } else {
                None
            },
        })
        .collect()
}

fn items_from_saved(saved: Vec<SavedItem>) -> Vec<ItemPlan> {
    saved
        .into_iter()
        .map(|i| ItemPlan {
            pos: Point::new(i.x, i.y),
            name: i.name,
            count: i.count,
        })
        .collect()
}

fn props_from_saved(saved: Vec<SavedProp>) -> Vec<PropPlan> {
    saved
        .into_iter()
        .map(|p| PropPlan {
            pos: Point::new(p.x, p.y),
            name: p.name,
        })
        .collect()
}

impl FloorPlan {
    fn from_builder(build_data: BuilderMap) -> Self {
        let starting_pos = build_data.starting_position.unwrap_or_else(|| {
            warn!("Map builder did not set a starting position; falling back to first walkable tile.");
            build_data
                .map
                .tiles
                .iter()
                .enumerate()
                .find(|(_, t)| is_walkable(**t))
                .map(|(idx, _)| {
                    let (x, y) = build_data.map.idx_xy(idx);
                    Position { x, y }
                })
                .expect("Map has no walkable tiles — cannot place player")
        });

        let starting_pt = Point::new(starting_pos.x, starting_pos.y);
        let player_spawn = if build_data
            .map
            .get_tile(starting_pt)
            .map(|t| t.terrain == TerrainType::UpStairs)
            .unwrap_or(false)
        {
            find_adjacent_floor(&build_data.map, starting_pt).unwrap_or(starting_pt)
        } else {
            starting_pt
        };

        let monsters = build_data
            .spawn_list
            .into_iter()
            .map(|entry| MonsterPlan {
                pos: entry.pos,
                name: entry.name,
                squad_id: entry.squad_id.map(|s| s.0),
                is_leader: entry.is_leader,
                squad_config: entry.squad_config,
                patrol_route: entry.patrol_route,
                saved_hp: None,
            })
            .collect();

        let items = build_data
            .item_spawn_list
            .into_iter()
            .map(|(pt, name, count)| ItemPlan {
                pos: pt,
                name,
                count,
            })
            .collect();

        let props = build_data
            .prop_spawn_list
            .into_iter()
            .map(|(pt, name)| PropPlan { pos: pt, name })
            .collect();

        FloorPlan {
            map: build_data.map,
            monsters,
            items,
            props,
            player_spawn,
            pending_player_load: None,
        }
    }

    fn from_cache(cached: CachedFloor, ascending: bool) -> Self {
        let target_stairs = if ascending {
            cached.down_stairs_pos
        } else {
            cached.up_stairs_pos
        };
        let player_spawn =
            find_adjacent_floor(&cached.map, target_stairs).unwrap_or(target_stairs);

        let monsters = monsters_from_saved(cached.monsters, true);
        let items = items_from_saved(cached.items);
        let props = props_from_saved(cached.props);

        FloorPlan {
            map: cached.map,
            monsters,
            items,
            props,
            player_spawn,
            pending_player_load: None,
        }
    }

    fn from_save(save_data: Box<GameSaveData>) -> Self {
        let save_data = *save_data;
        let player_spawn = Point::new(save_data.player.x, save_data.player.y);
        let map = save_data_to_map(&save_data.map);

        let monsters = monsters_from_saved(save_data.monsters, true);
        let items = items_from_saved(save_data.floor_items);
        let props = props_from_saved(save_data.props);

        FloorPlan {
            map,
            monsters,
            items,
            props,
            player_spawn,
            pending_player_load: Some(save_data.player),
        }
    }
}

// ---------------------------------------------------------------------------
// Materialization — the single entry point
// ---------------------------------------------------------------------------

/// Materialize a floor from any source into live ECS entities.
///
/// Handles tile spawning, monster/item/prop spawning with squad/patrol/HP
/// modifiers, and player spawn point computation. The caller handles
/// resource-level concerns (turn queue, auto-save, pending player load, etc.).
pub fn materialize_floor(
    commands: &mut Commands,
    map_entity: Entity,
    entity_assets: &EntityAssets,
    tile_assets: &TileAssets,
    turn_manager: &mut ResMut<TurnManager>,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
    source: FloorSource,
) -> FloorResult {
    let plan = match source {
        FloorSource::Generate(build_data) => FloorPlan::from_builder(build_data),
        FloorSource::Restore { cached, ascending } => FloorPlan::from_cache(cached, ascending),
        FloorSource::Load(save_data) => FloorPlan::from_save(save_data),
    };

    let mut warnings = Vec::new();

    // Spawn tiles
    spawn_tiles_into_ecs(commands, map_entity, &plan.map, tile_assets, ascii_font);

    // Spawn monsters
    for m in &plan.monsters {
        if let Some(entity) = spawn_monster_by_name(
            commands,
            &m.name,
            &m.pos,
            turn_manager,
            &entity_assets.monster_manifests,
            &entity_assets.monster_manifest_handle,
            &entity_assets.monster_sprite_assets,
            ascii_font,
        ) {
            if let (Some(squad_id), Some(squad_config)) = (m.squad_id, m.squad_config.clone()) {
                commands
                    .entity(entity)
                    .insert((SquadId(squad_id), squad_config));
                if m.is_leader {
                    commands.entity(entity).insert(SquadLeader);
                }
            }
            if let Some(patrol_route) = m.patrol_route.clone() {
                commands.entity(entity).insert(patrol_route);
            }
            if let Some(hp) = m.saved_hp {
                commands.entity(entity).insert(SavedHp(hp));
            }
        } else {
            warnings.push(format!("Failed to spawn monster '{}'", m.name));
        }
    }

    // Spawn items
    for i in &plan.items {
        if let Some(entity) = spawn_item(
            commands,
            &i.name,
            &i.pos,
            &entity_assets.item_manifests,
            &entity_assets.item_manifest_handle,
            &entity_assets.item_sprite_assets,
            ascii_font,
        ) {
            if i.count > 1 {
                let max_stack = entity_assets
                    .item_manifests
                    .get(&entity_assets.item_manifest_handle.0)
                    .and_then(|m| m.items.get(i.name.as_str()))
                    .map(|a| a.max_stack)
                    .unwrap_or(1);
                commands
                    .entity(entity)
                    .insert(ItemStack { count: i.count, max_stack });
            }
        } else {
            warnings.push(format!("Failed to spawn item '{}'", i.name));
        }
    }

    // Spawn props
    for p in &plan.props {
        if spawn_prop(
            commands,
            &p.name,
            &p.pos,
            &entity_assets.prop_manifests,
            &entity_assets.prop_manifest_handle,
            &entity_assets.prop_sprite_assets,
            ascii_font,
        )
        .is_none()
        {
            warnings.push(format!("Failed to spawn prop '{}'", p.name));
        }
    }

    FloorResult {
        player_spawn: plan.player_spawn,
        map: plan.map,
        pending_player_load: plan.pending_player_load,
        warnings,
    }
}
