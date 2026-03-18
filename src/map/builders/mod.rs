use bracket_lib::{
    prelude::{Point, Rect},
    random::RandomNumberGenerator,
};

use std::collections::HashMap;

use crate::{
    assets::{ItemSpawnInfo, MonsterAsset, MonsterSpawnInfo, PrefabTemplate},
    components::Position,
    game::squad::{SquadConfig, SquadId, SquadIdCounter},
    map::{
        Map,
        builders::{
            candle_spawner::CandleSpawner,
            cave_eroder::CaveEroder,
            diagonal_culler::DiagonalCuller,
            exit_points::DistantExit,
            item_spawner::ItemSpawner,
            lake_builder::LakeBuilder,
            monster_spawner::MonsterSpawner,
            prefab_placer::{MonsterRoleTable, PrefabPlacer},
            start_point::{StartPointBuilder, XStart, YStart},
            unseen_culler::UnseenCuller,
        },
    },
};

pub mod algorithms;
mod brogelike;
mod bsp_dungeon;
mod candle_spawner;
mod cave_eroder;
mod choke_map;
mod corridors;
mod diagonal_culler;
mod exit_points;
pub mod item_spawner;
mod lake_builder;
pub mod monster_spawner;
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
        Self { pos, name, squad_id: None, squad_config: None, is_leader: false, patrol_route: None }
    }

    /// Create a squad member spawn.
    pub fn squad(pos: Point, name: String, id: SquadId, config: SquadConfig, is_leader: bool) -> Self {
        Self { pos, name, squad_id: Some(id), squad_config: Some(config), is_leader, patrol_route: None }
    }
}

#[allow(dead_code)]
pub struct BuilderMap {
    // pub spawn_list: Vec<(usize, String)>,
    // pub modified_spawn_list: Vec<(SpawnOptions, String)>, // includes spawn options
    pub map: Map,
    pub starting_position: Option<Position>,
    pub rooms: Option<Vec<Rect>>,
    pub corridors: Option<Vec<Vec<usize>>>,
    // pub history: Vec<Map>,
    pub width: i32,
    pub height: i32,
    pub spawn_list: Vec<SpawnEntry>,
    pub item_spawn_list: Vec<(Point, String, u32)>, // (pos, item_name, count)
    pub prop_spawn_list: Vec<(Point, String)>,       // (pos, prop_name)
    pub squad_counter: SquadIdCounter,
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
}

pub struct BuilderChain {
    starter: Option<Box<dyn InitialMapBuilder>>,
    builders: Vec<Box<dyn MetaMapBuilder>>,
    pub build_data: BuilderMap,
}

impl BuilderChain {
    pub fn new<S: ToString>(new_depth: i32, width: i32, height: i32, name: S, squad_counter: SquadIdCounter) -> BuilderChain {
        BuilderChain {
            starter: None,
            builders: Vec::new(),
            build_data: BuilderMap {
                map: Map::new(new_depth, width, height, name),
                starting_position: None,
                rooms: None,
                corridors: None,
                width,
                height,
                spawn_list: Vec::new(),
                item_spawn_list: Vec::new(),
                prop_spawn_list: Vec::new(),
                squad_counter,
            },
        }
    }

    pub fn start_with(&mut self, starter: Box<dyn InitialMapBuilder>) {
        match self.starter {
            None => self.starter = Some(starter),
            Some(_) => panic!("You can only have one starting builder."),
        };
    }

    pub fn with(&mut self, metabuilder: Box<dyn MetaMapBuilder>) {
        self.builders.push(metabuilder);
    }

    pub fn build_map(&mut self) {
        match &mut self.starter {
            None => panic!("Cannot run a map builder chain without a starting build system"),
            Some(starter) => {
                // Build the starting map
                starter.build_map(&mut self.build_data);
            }
        }

        // Build additional layers in turn
        for metabuilder in self.builders.iter_mut() {
            metabuilder.build_map(&mut self.build_data);
        }
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

/// Controls how organic vs. structured the generated floor feels.
#[derive(Clone, Copy)]
pub struct FloorProfile {
    /// Probability weight for cavern rooms (0-100). Higher = more caves.
    pub cavern_weight: i32,
    /// Whether the first room is forced to be a large cavern.
    pub force_cavern_start: bool,
    /// Target number of rooms to place.
    pub target_rooms: i32,
    /// Hallway attachment chance (0-100).
    pub hallway_chance: i32,
    /// Erosion chance per eligible wall tile (0-100).
    pub erosion_percent: i32,
    /// Whether cavern rooms can use relaxed (no-padding) fitting.
    pub relaxed_fitting: bool,
}

impl FloorProfile {
    pub fn for_depth(depth: i32) -> Self {
        match depth {
            1..=3 => Self {
                cavern_weight: 20,
                force_cavern_start: false,
                target_rooms: 20,
                hallway_chance: 25,
                erosion_percent: 20,
                relaxed_fitting: false,
            },
            4..=7 => Self {
                cavern_weight: 40,
                force_cavern_start: true,
                target_rooms: 18,
                hallway_chance: 40,
                erosion_percent: 40,
                relaxed_fitting: true,
            },
            _ => Self {
                cavern_weight: 60,
                force_cavern_start: true,
                target_rooms: 14,
                hallway_chance: 50,
                erosion_percent: 55,
                relaxed_fitting: true,
            },
        }
    }
}

pub trait InitialMapBuilder: Send + 'static {
    fn build_map(&mut self, build_data: &mut BuilderMap);
}

pub trait MetaMapBuilder: Send + 'static {
    fn build_map(&mut self, build_data: &mut BuilderMap);
}

#[allow(dead_code)]
fn random_start_position() -> (XStart, YStart) {
    let x;
    let mut rng = RandomNumberGenerator::new();
    let xroll = rng.roll_dice(1, 3);
    match xroll {
        1 => x = XStart::LEFT,
        2 => x = XStart::CENTER,
        _ => x = XStart::RIGHT,
    }

    let y;
    let yroll = rng.roll_dice(1, 3);
    match yroll {
        1 => y = YStart::BOTTOM,
        2 => y = YStart::CENTER,
        _ => y = YStart::TOP,
    }

    (x, y)
}

pub fn floor_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    spawn_table: &[MonsterSpawnInfo],
    item_spawn_table: &[ItemSpawnInfo],
    squad_counter: SquadIdCounter,
    prefabs: Vec<PrefabTemplate>,
    monster_manifest: &HashMap<String, MonsterAsset>,
) -> BuilderChain {
    let mut map_name = "Floor ".to_owned() + &new_depth.to_string();
    if new_depth == 1 {
        map_name = "Entrance".to_owned();
    }
    let profile = FloorProfile::for_depth(new_depth);
    let mut builder = BuilderChain::new(new_depth, width, height, map_name, squad_counter);

    let role_table = MonsterRoleTable::from_manifest(monster_manifest, spawn_table);

    // MAP Generation
    builder.start_with(brogelike::BrogueLikeBuilder::dungeon(
        new_depth, width, height, profile,
    ));
    builder.with(DiagonalCuller::new());
    builder.with(StartPointBuilder::new());
    builder.with(LakeBuilder::new(new_depth));
    builder.with(PrefabPlacer::new(prefabs, role_table));
    builder.with(CandleSpawner::new());
    builder.with(MonsterSpawner::new(spawn_table));
    builder.with(ItemSpawner::new(item_spawn_table));
    builder.with(UnseenCuller::new());
    builder.with(DistantExit::new());

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
) -> BuilderChain {
    floor_builder(new_depth, width, height, spawn_table, item_spawn_table, squad_counter, prefabs, monster_manifest)
}
