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
    pub town_npc_manifests:
        Res<'w, Assets<crate::map::builders::town_npcs::TownNpcManifest>>,
    pub town_npc_manifest_handle: Res<'w, crate::map::builders::town_npcs::TownNpcManifestHandle>,
}

// ---------------------------------------------------------------------------
// Floor blueprint — the typed contract between source-of-truth (builder,
// cache, save) and the ECS materializer.
//
// Maintenance contract for adding a persisted component:
//   1. Component definition: add `Serialize, Deserialize`.
//   2. `crate::save::SavedMonster` (or `SavedItem`/`SavedProp`): add the
//      field with `#[serde(default)]` for backward compat.
//   3. `crate::map::dungeon::SnapshotQueries`: include the component in
//      the query tuple.
//   4. `crate::map::dungeon::snapshot_floor`: copy the live component
//      value into the new `SavedX` field.
//   5. `materialize_floor` (below): apply the saved value back onto the
//      spawned entity.
//
// `FloorPlan` carries the canonical `SavedMonster` / `SavedItem` /
// `SavedProp` directly — there is no intermediate `MonsterPlan` shape
// to also update. The save format and the in-memory plan share one
// type per entity kind.
// ---------------------------------------------------------------------------

/// A fully-resolved floor description ready for ECS materialization.
///
/// Built by [`plan_floor`] from a [`FloorSource`]; consumed by
/// [`materialize_floor`]. The type is pure value-data — no Bevy
/// resources, no ECS state — so plans can be constructed and inspected
/// in tests without spinning up an `App`.
pub struct FloorPlan {
    pub map: Map,
    pub monsters: Vec<SavedMonster>,
    pub items: Vec<SavedItem>,
    pub props: Vec<SavedProp>,
    /// Tiles that should receive a [`crate::map::world::MapExitTile`]
    /// component once their tile entities exist — used by overworld
    /// edges and the temple entrance / exit.
    pub exit_tiles: Vec<(Point, crate::map::world::MapExitTile)>,
    pub player_spawn: Point,
    /// Carried through from the Load path for the caller.
    pub pending_player_load: Option<crate::save::PlayerSaveData>,
    /// Set by the forest builder when it places the temple-entrance
    /// DownStairs; the orchestrator latches it onto
    /// [`crate::map::world::OverworldState::temple_entrance_pos`].
    /// `None` on every other path.
    pub overworld_edit: Option<crate::components::Position>,
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

        // Fresh-spawn monsters/items/props: build `SavedX` records with
        // `hp_current: 0` (sentinel for "use manifest default") and
        // default mutable state. The materializer's `hp_current > 0`
        // check distinguishes restored entities from fresh spawns.
        let monsters = build_data
            .spawn_list
            .into_iter()
            .map(|entry| SavedMonster {
                x: entry.pos.x,
                y: entry.pos.y,
                name: entry.name,
                hp_current: 0,
                squad_id: entry.squad_id.map(|s| s.0),
                is_leader: entry.is_leader,
                squad_config: entry.squad_config,
                patrol_route: entry.patrol_route,
                submerged: false,
                // Fresh spawns default to Hidden — perception fills in
                // on the next tick.
                awareness: crate::save::MonsterAwarenessSave::default(),
                // Fresh spawns are never panicking.
                fleeing: None,
            })
            .collect();

        let items = build_data
            .item_spawn_list
            .into_iter()
            .map(|(pt, name, count)| SavedItem {
                x: pt.x,
                y: pt.y,
                name,
                count,
                state: Default::default(),
                drifting: false,
            })
            .collect();

        let props = build_data
            .prop_spawn_list
            .into_iter()
            .map(|(pt, name)| SavedProp {
                x: pt.x,
                y: pt.y,
                name,
                ever_fired: false, // freshly placed prefab prop, hasn't fired
            })
            .collect();

        let exit_tiles = build_data.exit_tile_spawn_list;
        let overworld_edit = build_data.overworld_edit;

        FloorPlan {
            map: build_data.map,
            monsters,
            items,
            props,
            exit_tiles,
            player_spawn,
            pending_player_load: None,
            overworld_edit,
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

        // Cache/save paths use SavedMonster/Item/Prop directly — no
        // conversion needed since FloorPlan carries the canonical types.
        FloorPlan {
            map: cached.map,
            monsters: cached.monsters,
            items: cached.items,
            props: cached.props,
            exit_tiles: cached.exit_tiles,
            player_spawn,
            pending_player_load: None,
            // Restored floors don't re-emit overworld edits — the
            // orchestrator latched them on first generation.
            overworld_edit: None,
        }
    }

    fn from_save(save_data: Box<GameSaveData>) -> Self {
        let save_data = *save_data;
        let player_spawn = Point::new(save_data.player.x, save_data.player.y);
        let map = save_data_to_map(&save_data.map);

        FloorPlan {
            map,
            monsters: save_data.monsters,
            items: save_data.floor_items,
            props: save_data.props,
            // Save schema v5 doesn't persist exit tiles; v6 will.
            exit_tiles: Vec::new(),
            player_spawn,
            pending_player_load: Some(save_data.player),
            // Load path: overworld state is already on the save.
            overworld_edit: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Planning — pure conversion from a source into a `FloorPlan`
// ---------------------------------------------------------------------------

/// Build a [`FloorPlan`] from any [`FloorSource`].
///
/// Pure — no ECS, no `Commands`. Tests can call this with a hand-built
/// `BuilderMap` or `CachedFloor` and inspect the resulting plan
/// directly. This is the testable seam between "what should be on the
/// floor" and "spawn the entities".
pub fn plan_floor(source: FloorSource) -> FloorPlan {
    match source {
        FloorSource::Generate(build_data) => FloorPlan::from_builder(build_data),
        FloorSource::Restore { cached, ascending } => FloorPlan::from_cache(cached, ascending),
        FloorSource::Load(save_data) => FloorPlan::from_save(save_data),
    }
}

// ---------------------------------------------------------------------------
// Materialization — the single ECS entry point
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
    let plan = plan_floor(source);

    let mut warnings = Vec::new();

    // Spawn tiles
    spawn_tiles_into_ecs(commands, map_entity, &plan.map, tile_assets, tile_index, ascii_font);

    // Spawn monsters. `hp_current == 0` is the sentinel for "fresh
    // spawn — use the manifest default HP"; any positive value is a
    // restored HP from cache or save.
    for m in &plan.monsters {
        let pos = m.pos();
        if let Some(entity) = spawn_monster_by_name(
            commands,
            &m.name,
            &pos,
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
            if m.hp_current > 0 {
                commands.entity(entity).insert(SavedHp(m.hp_current));
            }
            if m.submerged {
                commands.entity(entity).insert(crate::components::Submerged);
            }
            // Defer awareness restore until the player entity exists.
            // `apply_saved_awareness_system` resolves this marker.
            if !matches!(m.awareness.state, crate::save::SavedAwarenessState::Hidden) {
                commands
                    .entity(entity)
                    .insert(crate::save::PendingAwarenessRestore(m.awareness.clone()));
            }
            // Restore sticky Fleeing overlay (schema v8+). Pre-v8 saves
            // default this to None and the monster loads as not-fleeing.
            if let Some(fleeing) = &m.fleeing {
                commands.entity(entity).insert(fleeing.to_component());
            }
        } else {
            warnings.push(format!("Failed to spawn monster '{}'", m.name));
        }
    }

    // Spawn items
    for i in &plan.items {
        let pos = i.pos();
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
            &pos,
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
                    .map(|a| a.max_stack())
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
        let pos = p.pos();
        match spawn_prop(
            commands,
            &p.name,
            &pos,
            &entity_assets.prop_manifests,
            &entity_assets.prop_manifest_handle,
            &entity_assets.prop_sprite_assets,
            ascii_font,
        ) {
            None => {
                warnings.push(format!("Failed to spawn prop '{}'", p.name));
            }
            Some(entity) => {
                // RFC 0002 step 4 — restore per-instance activation
                // state. spawn_prop attached EverFired(false) by
                // default for trigger props; overwrite with the saved
                // value so used altars and sprung traps survive
                // save/load.
                if p.ever_fired {
                    commands
                        .entity(entity)
                        .insert(crate::game::prop_effects::EverFired(true));
                }
            }
        }
    }

    // Stamp `MapExitTile` components on overworld edge / temple
    // entrance / temple exit tiles. The tile entity is looked up via
    // `tile_index`; if missing (out-of-bounds builder bug), we skip
    // silently — debug builds will catch it because the player won't
    // be able to transition.
    for (pos, exit) in &plan.exit_tiles {
        if let Some(&tile_entity) = tile_index.0.get(&(pos.x, pos.y)) {
            commands.entity(tile_entity).insert(*exit);
        } else {
            warnings.push(format!(
                "MapExitTile at ({}, {}) skipped — no tile entity at that coordinate",
                pos.x, pos.y
            ));
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

    // ----- plan_floor: Restore branch -----------------------------------

    /// Build a small cached floor with DownStairs at `down` and UpStairs
    /// at `up`. The map interior is all walkable so the BFS in
    /// `nearest_walkable` is trivial.
    fn cached_floor_with_stairs(down: Point, up: Point) -> super::super::dungeon::CachedFloor {
        let mut map = open_map(20, 20);
        let down_idx = map.xy_idx(down.x, down.y);
        map.tiles[down_idx].terrain = TerrainType::DownStairs;
        let up_idx = map.xy_idx(up.x, up.y);
        map.tiles[up_idx].terrain = TerrainType::UpStairs;
        super::super::dungeon::CachedFloor {
            map,
            monsters: Vec::new(),
            items: Vec::new(),
            props: Vec::new(),
            exit_tiles: Vec::new(),
            down_stairs_pos: down,
            up_stairs_pos: up,
        }
    }

    #[test]
    fn plan_floor_restore_ascending_lands_on_downstairs() {
        // Ascending = the player came up from a deeper floor, so they
        // land on the destination's DownStairs (where they originally
        // descended from).
        let cached = cached_floor_with_stairs(Point::new(7, 4), Point::new(2, 2));
        let plan = plan_floor(FloorSource::Restore { cached, ascending: true });
        assert_eq!(plan.player_spawn, Point::new(7, 4));
    }

    #[test]
    fn plan_floor_restore_descending_lands_on_upstairs() {
        let cached = cached_floor_with_stairs(Point::new(7, 4), Point::new(2, 2));
        let plan = plan_floor(FloorSource::Restore { cached, ascending: false });
        assert_eq!(plan.player_spawn, Point::new(2, 2));
    }

    #[test]
    fn plan_floor_restore_rescans_when_stored_pos_does_not_match_terrain() {
        // Build a cached floor whose `down_stairs_pos` lies on a Floor
        // tile (stale data), but the map has a real DownStairs elsewhere.
        // `from_cache` must re-scan and land the player on the actual
        // stair.
        let mut cached = cached_floor_with_stairs(Point::new(7, 4), Point::new(2, 2));
        cached.down_stairs_pos = Point::new(0, 0); // sentinel — not the real stair
        let plan = plan_floor(FloorSource::Restore { cached, ascending: true });
        assert_eq!(plan.player_spawn, Point::new(7, 4));
    }

    #[test]
    fn plan_floor_restore_carries_monsters_and_items() {
        let mut cached = cached_floor_with_stairs(Point::new(7, 4), Point::new(2, 2));
        cached.monsters.push(SavedMonster {
            x: 5,
            y: 5,
            name: "Goblin".to_string(),
            hp_current: 7,
            squad_id: None,
            is_leader: false,
            squad_config: None,
            patrol_route: None,
            submerged: false,
            awareness: Default::default(),
            fleeing: None,
        });
        cached.items.push(SavedItem {
            x: 9,
            y: 9,
            name: "Healing Potion".to_string(),
            count: 2,
            state: Default::default(),
            drifting: false,
        });
        let plan = plan_floor(FloorSource::Restore { cached, ascending: true });
        assert_eq!(plan.monsters.len(), 1);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.monsters[0].name, "Goblin");
        assert_eq!(plan.monsters[0].hp_current, 7);
        assert_eq!(plan.items[0].count, 2);
    }

    // ----- plan_floor: Generate branch ----------------------------------

    #[test]
    fn plan_floor_generate_falls_back_to_first_walkable_when_starting_pos_unset() {
        // Build a BuilderMap with a known map but no starting_position.
        // The plan should still produce a walkable spawn (the first
        // walkable tile in row-major order).
        let mut bm = super::super::builders::BuilderMap::new_for_test(10, 10);
        for t in bm.map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        // No `starting_position` set.
        let plan = plan_floor(FloorSource::Generate(bm));
        // First walkable tile in row-major order is (0, 0).
        assert_eq!(plan.player_spawn, Point::new(0, 0));
    }

    #[test]
    fn plan_floor_generate_uses_starting_position_when_set() {
        let mut bm = super::super::builders::BuilderMap::new_for_test(10, 10);
        for t in bm.map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        bm.starting_position = Some(Position { x: 5, y: 3 });
        let plan = plan_floor(FloorSource::Generate(bm));
        assert_eq!(plan.player_spawn, Point::new(5, 3));
    }

    #[test]
    fn plan_floor_generate_carries_spawn_lists() {
        use crate::map::builders::SpawnEntry;
        let mut bm = super::super::builders::BuilderMap::new_for_test(10, 10);
        for t in bm.map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        bm.starting_position = Some(Position { x: 1, y: 1 });
        bm.spawn_list.push(SpawnEntry::solo(Point::new(3, 3), "Goblin".to_string()));
        bm.item_spawn_list.push((Point::new(4, 4), "Apple".to_string(), 1));
        bm.prop_spawn_list.push((Point::new(5, 5), "Pillar".to_string()));

        let plan = plan_floor(FloorSource::Generate(bm));
        assert_eq!(plan.monsters.len(), 1);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.props.len(), 1);
        assert_eq!(plan.monsters[0].name, "Goblin");
        assert_eq!(plan.items[0].count, 1);
        assert_eq!(plan.props[0].name, "Pillar");
    }

    #[test]
    fn plan_floor_generate_propagates_overworld_edit() {
        // The forest builder records its temple-entrance position on
        // `BuilderMap::overworld_edit`; that field must reach the
        // resulting FloorPlan so the orchestrator can latch it onto
        // OverworldState.
        let mut bm = super::super::builders::BuilderMap::new_for_test(10, 10);
        for t in bm.map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        bm.starting_position = Some(Position { x: 0, y: 0 });
        bm.overworld_edit = Some(Position { x: 8, y: 8 });

        let plan = plan_floor(FloorSource::Generate(bm));
        assert_eq!(plan.overworld_edit, Some(Position { x: 8, y: 8 }));
    }

    #[test]
    fn plan_floor_restore_does_not_emit_overworld_edit() {
        // Restored floors must not re-emit overworld writes — the edit
        // was latched on first generation; reapplying it would clobber
        // any later mutation.
        let cached = cached_floor_with_stairs(Point::new(7, 4), Point::new(2, 2));
        let plan = plan_floor(FloorSource::Restore { cached, ascending: true });
        assert!(plan.overworld_edit.is_none());
    }

    #[test]
    fn plan_floor_restore_preserves_all_saved_monster_fields() {
        // Round-trip invariant: every field on a SavedMonster must
        // survive cache → plan unchanged. Now that FloorPlan carries
        // SavedMonster directly (instead of a parallel MonsterPlan
        // struct), this is a one-line copy — but the test pins the
        // contract so a future refactor can't silently drop a field.
        use crate::game::squad::SquadConfig;
        let mut cached = cached_floor_with_stairs(Point::new(7, 4), Point::new(2, 2));
        let monster = SavedMonster {
            x: 12,
            y: 7,
            name: "Goblin Chieftain".to_string(),
            hp_current: 25,
            squad_id: Some(42),
            is_leader: true,
            squad_config: Some(SquadConfig { flee_threshold: 0.3 }),
            patrol_route: None,
            submerged: true,
            awareness: Default::default(),
            fleeing: None,
        };
        cached.monsters.push(monster.clone());
        let plan = plan_floor(FloorSource::Restore { cached, ascending: true });
        let got = &plan.monsters[0];
        assert_eq!(got.x, monster.x);
        assert_eq!(got.y, monster.y);
        assert_eq!(got.name, monster.name);
        assert_eq!(got.hp_current, monster.hp_current);
        assert_eq!(got.squad_id, monster.squad_id);
        assert_eq!(got.is_leader, monster.is_leader);
        assert_eq!(got.submerged, monster.submerged);
        assert!(got.squad_config.is_some());
    }

    #[test]
    fn plan_floor_generate_marks_fresh_monsters_with_zero_hp() {
        // Builder-spawn monsters carry `hp_current: 0` so the
        // materializer applies the manifest default. Restored monsters
        // carry a positive `hp_current`.
        use crate::map::builders::SpawnEntry;
        let mut bm = super::super::builders::BuilderMap::new_for_test(10, 10);
        for t in bm.map.tiles.iter_mut() {
            *t = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
        bm.starting_position = Some(Position { x: 1, y: 1 });
        bm.spawn_list.push(SpawnEntry::solo(Point::new(3, 3), "Goblin".to_string()));

        let plan = plan_floor(FloorSource::Generate(bm));
        assert_eq!(plan.monsters.len(), 1);
        assert_eq!(plan.monsters[0].hp_current, 0,
            "fresh-spawn monsters from the builder must carry hp_current = 0 \
             (the manifest-default sentinel)");
    }
}
