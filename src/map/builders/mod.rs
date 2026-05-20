use bracket_lib::{
    prelude::{Point, Rect},
    random::RandomNumberGenerator,
};

use bevy::log::{debug, warn};
use std::collections::HashMap;
use std::time::Instant;

use crate::{
    assets::{ItemSpawnInfo, MonsterAsset, MonsterSpawnInfo, PrefabTemplate},
    components::Position,
    game::squad::{SquadConfig, SquadId, SquadIdCounter},
    map::{
        Map,
        builders::{
            decoration_propagator::DecorationPropagator,
            voronoi_spawner::VoronoiSpawner,
        },
    },
};

pub mod algorithms;
pub mod candle_spawner;
pub mod decoration_propagator;
pub mod exit_points;
pub mod forest;
pub mod item_spawner;
pub mod prefab_placer;
pub mod temple;
pub mod town;
pub mod voronoi_spawner;

/// A single monster spawn entry, optionally linked to a squad.
pub struct SpawnEntry {
    pub pos: Point,
    pub name: String,
    pub squad_id: Option<SquadId>,
    pub squad_config: Option<SquadConfig>,
    pub is_leader: bool,
    pub patrol_route: Option<crate::game::ai::PatrolRoute>,
}

impl SpawnEntry {
    /// Create a solo spawn with no squad affiliation.
    pub fn solo(pos: Point, name: String) -> Self {
        Self {
            pos,
            name,
            squad_id: None,
            squad_config: None,
            is_leader: false,
            patrol_route: None,
        }
    }

    /// Create a squad member spawn.
    pub fn squad(
        pos: Point,
        name: String,
        id: SquadId,
        config: SquadConfig,
        is_leader: bool,
    ) -> Self {
        Self {
            pos,
            name,
            squad_id: Some(id),
            squad_config: Some(config),
            is_leader,
            patrol_route: None,
        }
    }
}

// Re-export engine builder framework types so game builders can
// reference them and future migrations can incrementally convert.
pub use roguelike_engine::map::builders::{
    BuildContext, BuilderPhase as EngineBuildPhase, EngineBuilderMap, MapBuilder,
    BuilderChain as EngineBuilderChain,
};

#[allow(dead_code)]
pub struct BuilderMap {
    pub map: Map,
    pub starting_position: Option<Position>,
    pub rooms: Option<Vec<Rect>>,
    pub width: i32,
    pub height: i32,
    pub spawn_list: Vec<SpawnEntry>,
    pub item_spawn_list: Vec<(Point, String, u32)>, // (pos, item_name, count)
    pub prop_spawn_list: Vec<(Point, String)>,      // (pos, prop_name)
    pub squad_counter: SquadIdCounter,
    pub decoration_exclusion_zones: Vec<Rect>,
    /// Seeded RNG for deterministic map generation.
    pub rng: RandomNumberGenerator,
}

/// Implement the engine's `BuildContext` so engine builders can operate on
/// `BuilderMap` via `&mut dyn BuildContext` without knowing about spawn lists.
impl BuildContext for BuilderMap {
    fn map(&self) -> &Map { &self.map }
    fn map_mut(&mut self) -> &mut Map { &mut self.map }
    fn width(&self) -> i32 { self.width }
    fn height(&self) -> i32 { self.height }
    fn rooms(&self) -> Option<&Vec<Rect>> { self.rooms.as_ref() }
    fn set_rooms(&mut self, rooms: Vec<Rect>) { self.rooms = Some(rooms); }
    fn starting_position(&self) -> Option<Position> { self.starting_position }
    fn set_starting_position(&mut self, pos: Position) { self.starting_position = Some(pos); }
    fn exclusion_zones(&self) -> &[Rect] { &self.decoration_exclusion_zones }
    fn add_exclusion_zone(&mut self, rect: Rect) { self.decoration_exclusion_zones.push(rect); }
    fn rng(&mut self) -> &mut RandomNumberGenerator { &mut self.rng }
}

impl BuilderMap {
    fn take_snapshot(&mut self) {
        // if SHOW_MAPGEN_VISUALIZER {
        //     let mut snapshot = self.map.clone();
        //     for c in snapshot.tiles.iter_mut() {
        //         c.set_flags(DISCOVERED);
        //     }
        //     self.history.push(snapshot);
        // }
    }

    // --- Validation helpers ---

    /// Returns rooms if set, or logs a warning and returns `None`.
    pub fn rooms_or_warn(&self, builder: &str) -> Option<&Vec<Rect>> {
        self.rooms.as_ref().or_else(|| {
            warn!("{builder}: rooms not set by a prior builder — skipping");
            None
        })
    }

    /// Returns starting_position if set, or logs a warning and returns `None`.
    pub fn starting_position_or_warn(&self, builder: &str) -> Option<&Position> {
        self.starting_position.as_ref().or_else(|| {
            warn!("{builder}: starting_position not set by a prior builder — skipping");
            None
        })
    }

    // --- Accessor methods (Phase 3) ---

    pub fn add_monster_spawn(&mut self, entry: SpawnEntry) {
        self.spawn_list.push(entry);
    }

    pub fn add_item_spawn(&mut self, pos: Point, name: String, count: u32) {
        self.item_spawn_list.push((pos, name, count));
    }

    pub fn add_prop_spawn(&mut self, pos: Point, name: String) {
        self.prop_spawn_list.push((pos, name));
    }

    pub fn add_exclusion_zone(&mut self, rect: Rect) {
        self.decoration_exclusion_zones.push(rect);
    }

    pub fn exclusion_zones(&self) -> &[Rect] {
        &self.decoration_exclusion_zones
    }

    pub fn rooms(&self) -> Option<&Vec<Rect>> {
        self.rooms.as_ref()
    }

    pub fn set_starting_position(&mut self, pos: Position) {
        self.starting_position = Some(pos);
    }

    // --- Test constructors (Phase 1) ---

    #[cfg(test)]
    pub fn new_for_test(width: i32, height: i32) -> Self {
        BuilderMap {
            map: Map::new(1, width, height, "test"),
            starting_position: None,
            rooms: None,
            width,
            height,
            spawn_list: Vec::new(),
            item_spawn_list: Vec::new(),
            prop_spawn_list: Vec::new(),
            squad_counter: SquadIdCounter::default(),
            decoration_exclusion_zones: Vec::new(),
            rng: RandomNumberGenerator::new(),
        }
    }

    #[cfg(test)]
    pub fn with_open_room(width: i32, height: i32) -> Self {
        use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

        let mut bm = Self::new_for_test(width, height);
        // Carve floor interior, leave wall border
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = bm.map.xy_idx(x, y);
                bm.map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }
        bm.rooms = Some(vec![Rect::with_size(1, 1, width - 2, height - 2)]);
        bm
    }
}

pub struct BuilderChain {
    starter: Option<(&'static str, Box<dyn InitialMapBuilder>)>,
    builders: Vec<(&'static str, Box<dyn MetaMapBuilder>)>,
    pub build_data: BuilderMap,
}

impl BuilderChain {
    pub fn new<S: ToString>(
        new_depth: i32,
        width: i32,
        height: i32,
        name: S,
        squad_counter: SquadIdCounter,
    ) -> BuilderChain {
        BuilderChain {
            starter: None,
            builders: Vec::new(),
            build_data: BuilderMap {
                map: Map::new(new_depth, width, height, name),
                starting_position: None,
                rooms: None,
                width,
                height,
                spawn_list: Vec::new(),
                item_spawn_list: Vec::new(),
                prop_spawn_list: Vec::new(),
                squad_counter,
                decoration_exclusion_zones: Vec::new(),
                rng: RandomNumberGenerator::new(),
            },
        }
    }

    pub fn start_with(&mut self, starter: Box<dyn InitialMapBuilder>) {
        match self.starter {
            None => self.starter = Some(("starter", starter)),
            Some(_) => panic!("You can only have one starting builder."),
        };
    }

    /// Register a meta builder with a display name (used for timing logs).
    fn with_named(&mut self, name: &'static str, metabuilder: Box<dyn MetaMapBuilder>) {
        self.builders.push((name, metabuilder));
    }

    pub fn build_map(&mut self) {
        let total_start = Instant::now();

        match &mut self.starter {
            None => panic!("Cannot run a map builder chain without a starting build system"),
            Some((name, starter)) => {
                let start = Instant::now();
                starter.build_map(&mut self.build_data);
                debug!(
                    "Builder [{}]: {:.1}ms",
                    name,
                    start.elapsed().as_secs_f64() * 1000.0
                );
            }
        }

        // Build additional layers in turn, verifying phase ordering in debug builds.
        let mut last_phase: Option<BuilderPhase> = None;
        for (i, (name, metabuilder)) in self.builders.iter_mut().enumerate() {
            let label = if name.is_empty() {
                format!("meta_{}", i)
            } else {
                name.to_string()
            };

            if let Some(phase) = metabuilder.phase() {
                if let Some(prev) = last_phase {
                    debug_assert!(
                        phase >= prev,
                        "Builder [{label}] phase {phase:?} is earlier than previous phase {prev:?}. \
                         Reorder the pipeline in floor_builder()."
                    );
                }
                last_phase = Some(phase);
            }

            let start = Instant::now();
            metabuilder.build_map(&mut self.build_data);
            debug!(
                "Builder [{}]: {:.1}ms",
                label,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }

        debug!(
            "Total build_map: {:.1}ms",
            total_start.elapsed().as_secs_f64() * 1000.0
        );
    }

    // pub fn spawn_entities(&mut self, ecs: &mut World) {
    //     let mut all_storages = ecs.borrow::<AllStoragesViewMut>().unwrap();
    //     for entity in self.build_data.spawn_list.iter() {
    //         spawns::spawn_entity(&mut all_storages, &(&entity.0, &entity.1));
    //     }
    //     for entity in self.build_data.modified_spawn_list.iter() {
    //         spawns::spawn_entity_with_options(&mut all_storages, &(&entity.0, &entity.1));
    //     }
    // }
}

// `FloorProfile` struct now lives in the engine. Re-exported here so
// game code using `crate::map::builders::FloorProfile` compiles unchanged.
pub use roguelike_engine::map::builders::FloorProfile;

/// The Veiled Tyrant's depth-based floor profile mapping.
///
/// Can't be an inherent impl on `FloorProfile` because the struct
/// now lives in the engine crate. Free function instead.
pub fn floor_profile_for_depth(depth: i32) -> FloorProfile {
    match depth {
        1..=3 => FloorProfile {
            cavern_weight: 20,
            force_cavern_start: false,
            target_rooms: 20,
            hallway_chance: 25,
            erosion_percent: 20,
            relaxed_fitting: false,
            decoration_density: 0.6,
        },
        4..=7 => FloorProfile {
            cavern_weight: 40,
            force_cavern_start: true,
            target_rooms: 18,
            hallway_chance: 40,
            erosion_percent: 40,
            relaxed_fitting: true,
            decoration_density: 0.8,
        },
        _ => FloorProfile {
            cavern_weight: 60,
            force_cavern_start: true,
            target_rooms: 14,
            hallway_chance: 50,
            erosion_percent: 55,
            relaxed_fitting: true,
            decoration_density: 1.0,
        },
    }
}

pub trait InitialMapBuilder: Send + 'static {
    fn build_map(&mut self, build_data: &mut BuilderMap);
}

/// Logical pipeline phases. Builders declare their phase; `BuilderChain`
/// enforces monotonically non-decreasing phase order via `debug_assert!`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuilderPhase {
    /// Initial map generation (rooms, corridors, terrain).
    Geometry,
    /// Terrain post-processing (culling, doors, lakes).
    TerrainCleanup,
    /// Prefabs, boss rooms, connectivity validation.
    StructurePlacement,
    /// Connectivity culling (IsolatedAreaCuller).
    ConnectivityCull,
    /// Entity spawning (monsters, items, props, shrines).
    Spawning,
    /// Exit placement, decoration, final passes.
    Finalization,
}

pub trait MetaMapBuilder: Send + 'static {
    fn build_map(&mut self, build_data: &mut BuilderMap);

    /// Declare which pipeline phase this builder belongs to.
    /// Returns `None` by default (no phase enforcement).
    fn phase(&self) -> Option<BuilderPhase> {
        None
    }
}

/// Dispatch a builder chain for a single floor based on its
/// `FloorKind` (town → forest → temple). Each chain is small and
/// hand-built in the per-level functions below; the legacy generic
/// pipeline (BrogueLike → cullers → PrefabPlacer → spawners → exits)
/// is gone with the dungeon milestone — see the per-level builders for
/// the actual phase order they assemble.
pub fn floor_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    spawn_table: &[MonsterSpawnInfo],
    _item_spawn_table: &[ItemSpawnInfo],
    squad_counter: SquadIdCounter,
    _prefabs: Vec<PrefabTemplate>,
    _monster_manifest: &HashMap<String, MonsterAsset>,
    decoration_rules: Vec<crate::assets::DecorationRule>,
) -> BuilderChain {
    use crate::map::world::{FloorKind, floor_kind};
    match floor_kind(new_depth as u32) {
        FloorKind::Town => town_builder(new_depth, width, height, squad_counter),
        FloorKind::Forest { .. } => {
            forest_builder(new_depth, width, height, squad_counter, spawn_table, decoration_rules)
        }
        FloorKind::Temple => temple_builder(new_depth, width, height, squad_counter),
    }
}

/// Build the town hub (floor 0). Open Floor with a handful of small
/// buildings, a Portal at the centre (the win-condition return point),
/// a DownStairs on the east border into Forest 1, an organic dirt-
/// road network, and Townsfolk NPCs placed per the hardcoded
/// `town::TOWN_NPC_SPAWNS` roster.
pub fn town_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    squad_counter: SquadIdCounter,
) -> BuilderChain {
    let mut builder = BuilderChain::new(new_depth, width, height, "Town", squad_counter);
    builder.start_with(town::TownLayoutBuilder::new());
    builder.with_named("TownPortalBuilder", town::TownPortalBuilder::new());
    builder.with_named("TownDownStairsBuilder", town::TownDownStairsBuilder::new());
    // Path network runs after the layout so it picks up Portal +
    // DownStairs + every building door already on the map and
    // connects them via organic pathfinding (A*-style with per-tile
    // noise).
    builder.with_named("TownPathBuilder", town::TownPathBuilder::new());
    // NPC placement runs LAST so it can read the finalised map
    // (avoiding water, building interiors, stairs) and queue
    // SpawnEntry's with PatrolRoute components.
    builder.with_named("TownNpcBuilder", town::TownNpcBuilder::new());
    builder
}

/// Build one of the forest floors (1..=`MAX_FLOOR`). Cellular-automata
/// trees + west/east end-clearings connected by a corridor (the spine).
/// `<` sits at the west clearing (player arrival), `>` at the east
/// clearing on non-final floors, and the Amulet at the east clearing
/// on the final floor. Decoration propagation seeds grass/foliage
/// across walkable tiles for visual texture.
pub fn forest_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    squad_counter: SquadIdCounter,
    spawn_table: &[MonsterSpawnInfo],
    decoration_rules: Vec<crate::assets::DecorationRule>,
) -> BuilderChain {
    let map_name = format!("Forest {new_depth}");
    let mut builder = BuilderChain::new(new_depth, width, height, map_name, squad_counter);
    builder.start_with(forest::ForestTerrainBuilder::new());
    // Stairs are placed before decoration propagation so the seed
    // points don't accidentally land on the UpStairs / DownStairs
    // tile (decorations are skipped on non-Floor terrain). Stairs also
    // run before VoronoiSpawner so the spawner sees the stair tiles
    // and skips them. See docs/design/SPAWNING.md.
    builder.with_named("ForestStairsBuilder", forest::ForestStairsBuilder::new());
    // Voronoi-cell monster spawner — drops one pack per chosen cell,
    // weighted by cell size, excluding a buffer around the player's
    // starting clearing. Currently active for forest only; town has
    // no spawn entries so plugging it in would be a no-op.
    builder.with_named("VoronoiSpawner", VoronoiSpawner::new(spawn_table));
    // Decoration density scales with depth: deeper forests get more
    // rubble, dead grass, and foliage. Curve is gentle from Forest 1
    // (open, navigable) to Forest 4 (claustrophobic, overgrown — the
    // floor where the temple entrance hides).
    let density = match new_depth {
        1 => 0.20,
        2 => 0.27,
        3 => 0.33,
        _ => 0.40,
    };
    builder.with_named(
        "DecorationPropagator",
        Box::new(DecorationPropagator::new(decoration_rules, new_depth, density)),
    );
    builder
}

/// Build the cult temple (floor `MAX_FLOOR`). Linear stone corridor
/// from the entry UpStairs (where the player arrives from Forest 4)
/// east to a sanctum chamber holding the Amulet of Yendor. Sealed
/// interior — everything outside the carved corridor + sanctum is
/// solid wall. No spawn table yet; cultists arrive in a future pass.
pub fn temple_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    squad_counter: SquadIdCounter,
) -> BuilderChain {
    let mut builder = BuilderChain::new(new_depth, width, height, "Temple", squad_counter);
    builder.start_with(temple::TempleLayoutBuilder::new());
    builder.with_named("TempleStairsBuilder", temple::TempleStairsBuilder::new());
    builder
}

pub fn level_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    spawn_table: &[MonsterSpawnInfo],
    item_spawn_table: &[ItemSpawnInfo],
    squad_counter: SquadIdCounter,
    prefabs: Vec<PrefabTemplate>,
    monster_manifest: &HashMap<String, MonsterAsset>,
    decoration_rules: Vec<crate::assets::DecorationRule>,
) -> BuilderChain {
    floor_builder(
        new_depth,
        width,
        height,
        spawn_table,
        item_spawn_table,
        squad_counter,
        prefabs,
        monster_manifest,
        decoration_rules,
    )
}
