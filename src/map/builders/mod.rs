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
            candle_spawner::CandleSpawner,
            decoration_propagator::DecorationPropagator,
            diagonal_culler::DiagonalCuller,
            exit_points::DistantExit,
            finish_doors::FinishDoors,
            isolated_area_culler::IsolatedAreaCuller,
            item_spawner::ItemSpawner,
            lake_builder::LakeBuilder,
            monster_spawner::MonsterSpawner,
            pillar_culler::PillarCuller,
            prefab_placer::{MonsterRoleTable, PrefabPlacer},
            start_point::{StartPointBuilder, XStart, YStart},
        },
    },
};

pub mod algorithms;
mod brogelike;
mod bsp_dungeon;
mod candle_spawner;
mod cave_eroder;
pub(crate) mod choke_map;
mod corridors;
pub mod decoration_propagator;
mod diagonal_culler;
mod exit_points;
mod finish_doors;
mod isolated_area_culler;
pub mod item_spawner;
mod lake_builder;
pub mod machine_builder;
pub mod amulet_placer;
pub mod forest;
pub mod temple;
pub mod town;
pub mod monster_spawner;
mod pillar_culler;
pub mod prefab_placer;
mod room_drawer;
mod start_point;
mod unseen_culler;

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
    pub machine_spawn_list: Vec<machine_builder::MachineSpawn>,
    /// Tiles to mark with a [`crate::map::world::MapExitTile`] component
    /// in the materializer. Used by overworld edge builders + the temple
    /// entrance/exit so transitions don't require a custom terrain type.
    pub exit_tile_spawn_list: Vec<(Point, crate::map::world::MapExitTile)>,
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

    /// Mark a tile to receive a `MapExitTile` component when entities
    /// are materialized. Convention: use this for overworld edge exits
    /// (explicit destination position) and the temple entrance / exit.
    pub fn add_exit_tile(
        &mut self,
        pos: Point,
        destination_floor: u32,
        destination_pos: Option<Position>,
    ) {
        self.exit_tile_spawn_list.push((
            pos,
            crate::map::world::MapExitTile { destination_floor, destination_pos },
        ));
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
            machine_spawn_list: Vec::new(),
            exit_tile_spawn_list: Vec::new(),
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
                machine_spawn_list: Vec::new(),
                exit_tile_spawn_list: Vec::new(),
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

#[allow(dead_code)]
fn random_start_position() -> (XStart, YStart) {
    let mut rng = RandomNumberGenerator::new();
    let xroll = rng.roll_dice(1, 3);
    let x = match xroll {
        1 => XStart::LEFT,
        2 => XStart::CENTER,
        _ => XStart::RIGHT,
    };

    let yroll = rng.roll_dice(1, 3);
    let y = match yroll {
        1 => YStart::BOTTOM,
        2 => YStart::CENTER,
        _ => YStart::TOP,
    };

    (x, y)
}

/// Constructs the builder pipeline for a single dungeon floor.
///
/// # Pipeline Dependency Graph
///
/// ```text
/// Phase: Geometry
///   BrogueLikeBuilder        → sets: rooms, map terrain
///
/// Phase: TerrainCleanup
///   StartPointBuilder        → reads: rooms         → sets: starting_position
///   LakeBuilder              → reads: starting_position, map terrain
///   DiagonalCuller2          → reads: map terrain
///   PillarCuller             → reads: map terrain
///   FinishDoors              → reads: map terrain
///
/// Phase: StructurePlacement
///   PrefabPlacer             → reads: rooms          → sets: spawn_list, exclusion_zones
///
/// Phase: ConnectivityCull
///   IsolatedAreaCuller       → reads: starting_position → modifies: map terrain
///
/// Phase: Spawning (must run AFTER ConnectivityCull)
///   CandleSpawner            → reads: rooms          → sets: prop_spawn_list
///   MonsterSpawner           → reads: rooms, starting_position → sets: spawn_list
///   ItemSpawner              → reads: rooms          → sets: prop_spawn_list
///
/// Phase: Finalization
///   DistantExit              → reads: starting_position → modifies: map terrain
///   DecorationPropagator     → reads: exclusion_zones → modifies: map decoration
/// ```
pub fn floor_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    spawn_table: &[MonsterSpawnInfo],
    _item_spawn_table: &[ItemSpawnInfo],
    squad_counter: SquadIdCounter,
    prefabs: Vec<PrefabTemplate>,
    monster_manifest: &HashMap<String, MonsterAsset>,
    decoration_rules: Vec<crate::assets::DecorationRule>,
    overworld: crate::map::world::OverworldState,
) -> BuilderChain {
    // Overworld dispatch — town and forest tiles use their own pipelines
    // and skip the dungeon spawners entirely (no monsters / items yet).
    use crate::map::world::{FloorKind, floor_kind};
    if new_depth >= 0 && new_depth <= 11 {
        match floor_kind(new_depth as u32) {
            FloorKind::Town => return town_builder(new_depth, width, height, squad_counter),
            FloorKind::Forest(_) => {
                let is_entrance =
                    new_depth as u32 == overworld.temple_entrance_floor;
                return forest_builder(new_depth, width, height, squad_counter, is_entrance);
            }
            FloorKind::Temple(_) => {
                return temple_builder(
                    new_depth, width, height, squad_counter, overworld,
                );
            }
        }
    }

    let mut map_name = "Floor ".to_owned() + &new_depth.to_string();
    if new_depth == 1 {
        map_name = "Entrance".to_owned();
    }
    let profile = floor_profile_for_depth(new_depth);
    let mut builder = BuilderChain::new(new_depth, width, height, map_name, squad_counter);

    let role_table = MonsterRoleTable::from_manifest(monster_manifest, spawn_table);

    // MAP Generation
    builder.start_with(Box::new(brogelike::BrogueLikeBuilder::dungeon(
        new_depth, width, height, profile,
    )));
    // builder.with_named("DiagonalCuller", DiagonalCuller::new());
    builder.with_named("StartPoint", Box::new(StartPointBuilder::new()));
    builder.with_named("LakeBuilder", Box::new(LakeBuilder::new(new_depth)));
    // builder.with_named("DiagonalCuller2", DiagonalCuller::new());
    // builder.with_named("PillarCuller", PillarCuller::new());
    // builder.with_named("FinishDoors", FinishDoors::new());
    // builder.with_named("PrefabPlacer", PrefabPlacer::new(prefabs, role_table));
    // builder.with_named("MachineBuilder", machine_builder::MachineBuilder::new());
    // builder.with_named("IsolatedAreaCuller", IsolatedAreaCuller::new());
    // --- Spawners run after IsolatedAreaCuller so entities are never placed in walled-off regions ---
    // builder.with_named("CandleSpawner", CandleSpawner::new());
    builder.with_named("MonsterSpawner", MonsterSpawner::new(spawn_table));
    // builder.with_named("ItemSpawner", ItemSpawner::new());
    builder.with_named(
        "DecorationPropagator",
        Box::new(DecorationPropagator::new(decoration_rules, new_depth, profile.decoration_density)),
    );
    builder.with_named("DistantExit", DistantExit::new());

    builder
}

/// Build the town hub (floor 0). Procedural plaza + buildings + 8 edge
/// exits to the surrounding forest tiles + a return Portal at the
/// plaza center. No monsters, items, or chests — content lands in a
/// later phase.
pub fn town_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    squad_counter: SquadIdCounter,
) -> BuilderChain {
    let mut builder = BuilderChain::new(new_depth, width, height, "Town", squad_counter);
    builder.start_with(town::TownLayoutBuilder::new());
    builder.with_named("TownBorderStairsBuilder", town::TownBorderStairsBuilder::new());
    builder.with_named("TownPathBuilder", town::TownPathBuilder::new());
    builder.with_named("TownPortalBuilder", town::TownPortalBuilder::new());
    builder
}

/// Build one of the 8 forest tiles (floors 1..=8) — cellular automata
/// trees, one UpStairs back to the town stair the player came in on,
/// and (on the chosen entrance forest) one DownStairs to temple 1.
/// No monster / item spawns.
pub fn forest_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    squad_counter: SquadIdCounter,
    is_temple_entrance: bool,
) -> BuilderChain {
    let mut builder = BuilderChain::new(
        new_depth, width, height,
        format!("Forest {new_depth}"),
        squad_counter,
    );
    builder.start_with(forest::ForestTerrainBuilder::new());
    builder.with_named("ForestBorderStairsBuilder", forest::ForestBorderStairsBuilder::new());
    if is_temple_entrance {
        builder.with_named("TempleEntranceBuilder", forest::TempleEntranceBuilder::new());
    }
    builder
}

/// Build a temple floor (depth 9..=11). Reuses `BrogueLikeBuilder`
/// for the geometry and skips all spawners; on temple-1 wires the
/// UpStairs back to the forest entrance via `TempleUpstairsLinker`.
pub fn temple_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    squad_counter: SquadIdCounter,
    overworld: crate::map::world::OverworldState,
) -> BuilderChain {
    let profile = FloorProfile {
        cavern_weight: 30,
        force_cavern_start: false,
        target_rooms: 12,
        hallway_chance: 30,
        erosion_percent: 50,
        relaxed_fitting: true,
        decoration_density: 0.5,
    };
    let map_name = format!("Temple {}", new_depth - 8);
    let mut builder = BuilderChain::new(new_depth, width, height, map_name, squad_counter);
    builder.start_with(Box::new(brogelike::BrogueLikeBuilder::dungeon(
        new_depth, width, height, profile,
    )));
    builder.with_named("StartPoint", Box::new(StartPointBuilder::new()));
    // Temple 3 is the bottom — the amulet placer takes the spot
    // where DistantExit would have put DownStairs, so only stamp
    // downstairs for temple floors 1 and 2.
    if new_depth < 11 {
        builder.with_named("DistantExit", DistantExit::new());
    } else {
        builder.with_named("AmuletPlacer", amulet_placer::AmuletPlacerBuilder::new());
    }
    // Temple 1 needs its UpStairs to return to the forest entrance.
    if new_depth == 9
        && let Some(forest_pos) = overworld.temple_entrance_pos
    {
        builder.with_named(
            "TempleUpstairsLinker",
            temple::TempleUpstairsLinker::boxed(overworld.temple_entrance_floor, forest_pos),
        );
    }
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
    overworld: crate::map::world::OverworldState,
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
        overworld,
    )
}
