use bracket_lib::{
    prelude::{Point, Rect},
    random::RandomNumberGenerator,
}; // Added Point

use crate::{
    assets::{ItemSpawnInfo, MonsterSpawnInfo},
    components::Position,
    map::{
        Map,
        builders::{
            candle_spawner::CandleSpawner,
            diagonal_culler::DiagonalCuller,
            exit_points::DistantExit,
            item_spawner::ItemSpawner,
            lake_builder::LakeBuilder,
            monster_spawner::MonsterSpawner,
            start_point::{StartPointBuilder, XStart, YStart},
            unseen_culler::UnseenCuller,
        },
        tile::LiquidType,
    },
};

pub mod algorithms;
mod brogelike;
mod bsp_dungeon;
mod candle_spawner;
mod choke_map;
mod corridors;
mod diagonal_culler;
mod exit_points;
pub mod item_spawner;
mod lake_builder;
pub mod monster_spawner;
mod room_drawer;
mod start_point;
mod unseen_culler;

pub struct BuilderMap {
    // pub spawn_list: Vec<(usize, String)>,
    // pub modified_spawn_list: Vec<(SpawnOptions, String)>, // includes spawn options
    pub map: Map,
    pub starting_position: Option<Position>,
    pub rooms: Option<Vec<Rect>>,
    pub corridors: Option<Vec<Vec<usize>>>,
    pub candle_spawn_points: Vec<Point>, // New field for candle spawn points
    // pub history: Vec<Map>,
    pub width: i32,
    pub height: i32,
    pub spawn_list: Vec<(Point, String)>, // Add spawn_list for entities
    pub item_spawn_list: Vec<(Point, String, u32)>, // (pos, item_name, count)
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
    pub fn new<S: ToString>(new_depth: i32, width: i32, height: i32, name: S) -> BuilderChain {
        BuilderChain {
            starter: None,
            builders: Vec::new(),
            build_data: BuilderMap {
                // spawn_list: Vec::new(),
                // modified_spawn_list: Vec::new(),
                map: Map::new(new_depth, width, height, name),
                starting_position: None,
                rooms: None,
                corridors: None,
                candle_spawn_points: Vec::new(),
                // history: Vec::new(),
                width,
                height,
                spawn_list: Vec::new(),      // Initialize spawn_list
                item_spawn_list: Vec::new(), // Initialize item_spawn_list
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

pub trait InitialMapBuilder: Send + 'static {
    fn build_map(&mut self, build_data: &mut BuilderMap);
}

pub trait MetaMapBuilder: Send + 'static {
    fn build_map(&mut self, build_data: &mut BuilderMap);
}

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
) -> BuilderChain {
    let mut map_name = "Floor ".to_owned() + &new_depth.to_string();
    if new_depth == 1 {
        map_name = "Entrance".to_owned();
    }
    let mut builder = BuilderChain::new(new_depth, width, height, map_name);

    // MAP Generation
    builder.start_with(brogelike::BrogueLikeBuilder::dungeon(
        new_depth, width, height,
    ));
    builder.with(DiagonalCuller::new());
    builder.with(StartPointBuilder::new());
    // builder.with(LakeBuilder::new(LiquidType::Water));
    builder.with(CandleSpawner::new());
    builder.with(MonsterSpawner::new(spawn_table));
    builder.with(ItemSpawner::new(item_spawn_table));
    builder.with(UnseenCuller::new());
    builder.with(DistantExit::new());

    // let (start_x, start_y) = random_start_position();
    // builder.with(AreaStartingPosition::new(start_x, start_y));

    builder
}

pub fn level_builder(
    new_depth: i32,
    width: i32,
    height: i32,
    spawn_table: &[MonsterSpawnInfo],
    item_spawn_table: &[ItemSpawnInfo],
) -> BuilderChain {
    floor_builder(new_depth, width, height, spawn_table, item_spawn_table)
}
