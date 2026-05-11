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
use crate::components::{Collider, Position};
use crate::game::ai::PatrolRoute;
use crate::game::items::ItemStack;
use crate::game::machines::{Machine, MachineEffect, MachineTrigger, MachineUsed};
use crate::game::squad::{SquadConfig, SquadId, SquadLeader};
use crate::game::TurnManager;
use crate::game::{spawn_item, spawn_monster_by_name, spawn_prop};
use crate::map::builders::BuilderMap;
use crate::map::tile::{is_walkable, spawn_tile_entity, TerrainType, TileEntityIndex};
use crate::map::Map;
use crate::game::enchantment::{ArmorRunic, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified, WeaponRunic};
use crate::game::staves::{Rechargeable, StaffData, StaffEffect};
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
    submerged: bool,
}

struct ItemPlan {
    pos: Point,
    name: String,
    count: u32,
    /// Saved mutable item state; default means freshly generated (roll random).
    state: crate::save::ItemMutableState,
    drifting: bool,
}

struct PropPlan {
    pos: Point,
    name: String,
}

struct MachinePlan {
    pos: Point,
    prop_name: String,
    trigger: MachineTrigger,
    effect: MachineEffect,
    consume_on_use: bool,
}

struct FloorPlan {
    map: Map,
    monsters: Vec<MonsterPlan>,
    items: Vec<ItemPlan>,
    props: Vec<PropPlan>,
    machines: Vec<MachinePlan>,
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

/// Returns `target` if walkable, otherwise BFS outward to find the nearest
/// walkable tile. Stair tiles are allowed — `StairCooldown` prevents
/// `player_stair_system` from re-triggering after a floor transition.
pub fn nearest_walkable(map: &Map, target: Point) -> Point {
    // Fast path: target itself is fine.
    if let Some(tile) = map.get_tile(target)
        && is_walkable(tile) {
            return target;
        }
    // BFS outward through all terrain to find the nearest walkable tile.
    let total = (map.width * map.height) as usize;
    let mut visited = vec![false; total];
    let mut queue = std::collections::VecDeque::new();
    let start_idx = map.xy_idx(target.x, target.y);
    visited[start_idx] = true;
    queue.push_back(start_idx);
    while let Some(idx) = queue.pop_front() {
        let (x, y) = map.idx_xy(idx);
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
            let nidx = map.xy_idx(nx, ny);
            if visited[nidx] { continue; }
            visited[nidx] = true;
            if is_walkable(map.tiles[nidx]) {
                return Point::new(nx, ny);
            }
            queue.push_back(nidx);
        }
    }
    // Absolute last resort — should never happen on a valid map.
    warn!("nearest_walkable: no walkable tile found anywhere on map!");
    target
}

/// Like [`nearest_walkable`] but skips tiles whose `(x, y)` is in `occupied`.
/// Used to scatter a group of fallen entities so they don't all stack on the
/// same tile. Falls back to [`nearest_walkable`] (ignoring occupancy) if no
/// free tile is reachable — better to overlap than to lose the entity.
pub fn nearest_walkable_avoiding(
    map: &Map,
    target: Point,
    occupied: &std::collections::HashSet<(i32, i32)>,
) -> Point {
    // Fast path: target itself is free.
    if let Some(tile) = map.get_tile(target)
        && is_walkable(tile)
        && !occupied.contains(&(target.x, target.y))
    {
        return target;
    }
    let total = (map.width * map.height) as usize;
    let mut visited = vec![false; total];
    let mut queue = std::collections::VecDeque::new();
    let start_idx = map.xy_idx(target.x, target.y);
    visited[start_idx] = true;
    queue.push_back(start_idx);
    while let Some(idx) = queue.pop_front() {
        let (x, y) = map.idx_xy(idx);
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
            let nidx = map.xy_idx(nx, ny);
            if visited[nidx] { continue; }
            visited[nidx] = true;
            if is_walkable(map.tiles[nidx]) && !occupied.contains(&(nx, ny)) {
                return Point::new(nx, ny);
            }
            queue.push_back(nidx);
        }
    }
    // No free walkable tile found — fall back to the plain nearest-walkable
    // so the entity still spawns somewhere (at worst, stacked on another).
    nearest_walkable(map, target)
}

pub(crate) fn spawn_tiles_into_ecs(
    commands: &mut Commands,
    map_entity: Entity,
    game_map: &Map,
    tile_assets: &TileAssets,
    tile_index: &mut TileEntityIndex,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
) {
    let tile_manifest = tile_assets
        .manifests
        .get(&tile_assets.manifest_handle.0)
        .expect("Tile manifest not loaded");

    tile_index.0.clear();

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
                ascii_font,
            );
            commands
                .entity(tile_entity)
                .insert(Position { x: pt.x, y: pt.y });
            tile_index.0.insert((x, y), tile_entity);
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
            name: m.name.clone(),
            squad_id: m.squad_id,
            is_leader: m.is_leader,
            squad_config: m.squad_config,
            patrol_route: m.patrol_route,
            saved_hp: if restore_hp && m.hp_current > 0 {
                Some(m.hp_current)
            } else {
                None
            },
            submerged: m.submerged,
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
            state: i.state,
            drifting: i.drifting,
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
        let start_tile = build_data.map.get_tile(starting_pt);
        info!(
            "from_builder: starting_position=({}, {}), terrain={:?}, liquid={:?}",
            starting_pt.x, starting_pt.y,
            start_tile.map(|t| t.terrain),
            start_tile.map(|t| t.liquid),
        );
        let player_spawn = nearest_walkable(&build_data.map, starting_pt);

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
                submerged: false,
            })
            .collect();

        let items = build_data
            .item_spawn_list
            .into_iter()
            .map(|(pt, name, count)| ItemPlan {
                pos: pt,
                name,
                count,
                state: Default::default(),
                drifting: false,
            })
            .collect();

        let props = build_data
            .prop_spawn_list
            .into_iter()
            .map(|(pt, name)| PropPlan { pos: pt, name })
            .collect();

        let machines = build_data
            .machine_spawn_list
            .into_iter()
            .map(|ms| MachinePlan {
                pos: ms.pos,
                prop_name: ms.prop_name,
                trigger: ms.trigger,
                effect: ms.effect,
                consume_on_use: ms.consume_on_use,
            })
            .collect();

        FloorPlan {
            map: build_data.map,
            monsters,
            items,
            props,
            machines,
            player_spawn,
            pending_player_load: None,
        }
    }

    fn from_cache(cached: CachedFloor, ascending: bool) -> Self {
        // When ascending (going UP), land near the DownStairs (where the
        // player originally descended from this floor).
        // When descending (going DOWN) to a previously visited floor, land
        // near the UpStairs (where the player originally arrived).
        let stored_pos = if ascending {
            cached.down_stairs_pos
        } else {
            cached.up_stairs_pos
        };

        // Validate: the stored position should actually be the expected stair
        // type on the map. If not (e.g. save-compat default [0,0] or stale
        // data), re-scan the map for the correct stair tile.
        let expected_terrain = if ascending {
            TerrainType::DownStairs
        } else {
            TerrainType::UpStairs
        };
        let target_stairs = if cached
            .map
            .get_tile(stored_pos)
            .map(|t| t.terrain == expected_terrain)
            .unwrap_or(false)
        {
            stored_pos
        } else {
            // Stored position doesn't match — scan the map for the stair tile.
            warn!(
                "from_cache: stored stair pos ({}, {}) does not contain {:?}; re-scanning map",
                stored_pos.x, stored_pos.y, expected_terrain
            );
            cached
                .map
                .tiles
                .iter()
                .enumerate()
                .find_map(|(idx, tile)| {
                    if tile.terrain == expected_terrain {
                        let (x, y) = cached.map.idx_xy(idx);
                        Some(Point::new(x, y))
                    } else {
                        None
                    }
                })
                .unwrap_or(stored_pos)
        };
        let player_spawn = nearest_walkable(&cached.map, target_stairs);

        let monsters = monsters_from_saved(cached.monsters, true);
        let items = items_from_saved(cached.items);
        let props = props_from_saved(cached.props);

        FloorPlan {
            map: cached.map,
            monsters,
            items,
            props,
            machines: Vec::new(),
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
            machines: Vec::new(),
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
    tile_index: &mut TileEntityIndex,
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
    spawn_tiles_into_ecs(commands, map_entity, &plan.map, tile_assets, tile_index, ascii_font);

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
                    commands.entity(entity).insert((
                        SquadLeader,
                        crate::game::squad::SquadBlackboard::default(),
                    ));
                }
            }
            if let Some(patrol_route) = m.patrol_route.clone() {
                commands.entity(entity).insert(patrol_route);
            }
            if let Some(hp) = m.saved_hp {
                commands.entity(entity).insert(SavedHp(hp));
            }
            if m.submerged {
                commands.entity(entity).insert(crate::components::Submerged);
            }
        } else {
            warnings.push(format!("Failed to spawn monster '{}'", m.name));
        }
    }

    // Spawn items
    for i in &plan.items {
        // If the item has saved enchantment data, skip random enchantment rolling
        // by passing None for enchant_floor_depth.
        let has_saved_enchantment = i.state.enchantment.is_some()
            || i.state.weapon_runic.is_some()
            || i.state.armor_runic.is_some();
        let enchant_depth = if has_saved_enchantment {
            None
        } else {
            Some(plan.map.depth as u32)
        };

        if let Some(entity) = spawn_item(
            commands,
            &i.name,
            &i.pos,
            &entity_assets.item_manifests,
            &entity_assets.item_manifest_handle,
            &entity_assets.item_sprite_assets,
            ascii_font,
            enchant_depth,
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

            // Restore saved enchantment, runic, and staff data
            crate::save::restore_item_mutable_state(commands, entity, &i.state);

            // Restore drifting state
            if i.drifting {
                commands.entity(entity).insert(crate::components::Drifting);
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

    // Spawn machines
    for m in &plan.machines {
        if m.prop_name.is_empty() {
            // Invisible trigger — no visual prop, just a position with machine components
            let entity = commands.spawn((
                crate::components::Position { x: m.pos.x, y: m.pos.y },
                crate::components::GameEntityMarker,
                crate::components::FloorEntityMarker,
                Machine,
                m.trigger.clone(),
                m.effect.clone(),
                MachineUsed(false),
                crate::components::Name("Trap".to_string()),
            )).id();
            if m.consume_on_use {
                commands.entity(entity).insert(crate::game::machines::MachineConsumeOnUse);
            }
        } else if let Some(entity) = spawn_prop(
            commands,
            &m.prop_name,
            &m.pos,
            &entity_assets.prop_manifests,
            &entity_assets.prop_manifest_handle,
            &entity_assets.prop_sprite_assets,
            ascii_font,
        ) {
            commands.entity(entity).insert((
                Machine,
                m.trigger.clone(),
                m.effect.clone(),
                MachineUsed(false),
                Collider,
            ));
            if m.consume_on_use {
                commands.entity(entity).insert(crate::game::machines::MachineConsumeOnUse);
            }
        } else {
            warnings.push(format!("Failed to spawn machine prop '{}'", m.prop_name));
        }
    }

    FloorResult {
        player_spawn: plan.player_spawn,
        map: plan.map,
        pending_player_load: plan.pending_player_load,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, LiquidType, Tile, TerrainType};
    use std::collections::HashSet;

    fn open_map(w: i32, h: i32) -> Map {
        let mut map = Map::new(1, w, h, "test");
        for t in map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        map
    }

    #[test]
    fn nearest_walkable_avoiding_returns_target_when_free() {
        let map = open_map(10, 10);
        let occupied = HashSet::new();
        let got = nearest_walkable_avoiding(&map, Point::new(5, 5), &occupied);
        assert_eq!((got.x, got.y), (5, 5));
    }

    #[test]
    fn nearest_walkable_avoiding_skips_occupied() {
        // (5, 5) is walkable but marked occupied — BFS picks a neighbor.
        let map = open_map(10, 10);
        let mut occupied = HashSet::new();
        occupied.insert((5, 5));
        let got = nearest_walkable_avoiding(&map, Point::new(5, 5), &occupied);
        assert_ne!((got.x, got.y), (5, 5));
        // And the neighbor is adjacent (Manhattan distance 1).
        assert_eq!((got.x - 5).abs() + (got.y - 5).abs(), 1);
    }

    #[test]
    fn nearest_walkable_avoiding_scatters_cluster() {
        // Four entities all drop at (5, 5). They should land on four
        // *distinct* tiles in the general area, not stack.
        let map = open_map(10, 10);
        let mut occupied = HashSet::new();
        let mut placements = Vec::new();
        for _ in 0..4 {
            let p = nearest_walkable_avoiding(&map, Point::new(5, 5), &occupied);
            occupied.insert((p.x, p.y));
            placements.push((p.x, p.y));
        }
        let unique: HashSet<_> = placements.iter().copied().collect();
        assert_eq!(unique.len(), 4, "expected 4 distinct tiles, got {placements:?}");
        // Every placement sits within Manhattan distance 2 of (5, 5).
        for (x, y) in &placements {
            assert!((x - 5).abs() + (y - 5).abs() <= 2,
                "placement ({x},{y}) too far from drop point");
        }
    }

    #[test]
    fn nearest_walkable_avoiding_falls_back_when_map_is_full() {
        // A 1×1 open map: sole walkable tile is marked occupied. The fallback
        // path returns that tile anyway rather than looping or panicking.
        let map = open_map(1, 1);
        let mut occupied = HashSet::new();
        occupied.insert((0, 0));
        let got = nearest_walkable_avoiding(&map, Point::new(0, 0), &occupied);
        assert_eq!((got.x, got.y), (0, 0));
    }
}
