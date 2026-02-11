use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;
use bevy_ecs_tilemap::map::{TilemapTexture, TilemapType};
use bevy_ecs_tilemap::tiles::TileStorage;
use bevy_ecs_tilemap::{TilemapBundle, prelude::TilePos};
use bracket_lib::prelude::Point;

use crate::components::Position;
use crate::game::DungeonTileset;
use crate::map::builders::level_builder;
use crate::map::light::{Candle, CandleSpritesheet};
use crate::map::map::{ActiveMap, GRID_SIZE, MapHistory, MapId, PlayerPosition, TILE_SIZE};
use crate::map::tile::{TileType, spawn_tile_entity};
use crate::map::{GameMap, Map};
use crate::{
    AppState,
    map::{
        light::spawn_candle,
        map::{DungeonMap, MAP_SIZE},
    },
};

pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnDungeonMessage>()
            .add_systems(OnEnter(AppState::NextLevel), map_transition_down)
            // .add_systems(OnEnter(AppState::PreviousLevel), map_transition_up)
            .add_systems(
                Update,
                spawn_dungeon.run_if(on_message::<SpawnDungeonMessage>),
            );
    }
}

#[derive(Message, Clone, Copy)]
pub struct SpawnDungeonMessage;

fn map_transition_up(
    mut active_map: ResMut<ActiveMap>,
    mut map_history: ResMut<MapHistory>,
    q_dungeon_map_component: Query<&TileStorage, With<DungeonMap>>,
    q_candles: Query<(&Position, Entity), With<Candle>>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
) {
    // Save current map state before transitioning up
    // if active_map.0.0 != 0 {
    //     save_current_map_state(
    //         &mut map_history,
    //         &q_dungeon_map_component,
    //         &q_candles,
    //         active_map.0.0,
    //     );
    // }
    active_map.0.0 -= 1;
    message_writer.write(SpawnDungeonMessage);
}

fn map_transition_down(
    mut commands: Commands,
    mut active_map: ResMut<ActiveMap>,
    // mut map_history: ResMut<MapHistory>,
    q_map: Query<(Entity, &TileStorage), With<DungeonMap>>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
) {
    // Despawn old map entities and their tiles
    for (map_entity, tile_storage) in q_map.iter() {
        for x in 0..MAP_SIZE.x {
            for y in 0..MAP_SIZE.y {
                if let Some(tile_entity) = tile_storage.get(&TilePos { x, y }) {
                    commands.entity(tile_entity).despawn();
                }
            }
        }
        commands.entity(map_entity).despawn();
    }

    // Increment floor
    active_map.0.0 += 1;

    message_writer.write(SpawnDungeonMessage);
}

// fn map_transition_down(
//     mut active_map: ResMut<ActiveMap>,
//     mut map_history: ResMut<MapHistory>,
//     q_dungeon_map_component: Query<&TileStorage, With<DungeonMap>>,
//     q_candles: Query<(&Position, Entity), With<Candle>>,
//     mut message_writer: MessageWriter<SpawnDungeonMessage>,
// ) {
//     println!("In down function");

//     // 1. Save current map state (if any) to history
//     if active_map.0.0 != 0 {
//         save_current_map_state(
//             &mut map_history,
//             &q_dungeon_map_component,
//             &q_candles,
//             active_map.0.0,
//         );
//     }
//     active_map.0.0 += 1;

//     message_writer.write(SpawnDungeonMessage);
// }

// --------------------------------------------------------------------------------
// SYSTEMS
// --------------------------------------------------------------------------------

pub fn spawn_dungeon(
    mut commands: Commands,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>, // New parameter
    mut next_state: ResMut<NextState<AppState>>,
    mut player_pos: ResMut<PlayerPosition>,
    active_map: Res<ActiveMap>,
    mut spawn_message_reader: MessageReader<SpawnDungeonMessage>,
) {
    spawn_message_reader.clear(); // Clear the event reader

    let next_depth = active_map.0.0 as i32;
    // Create the Tilemap entity
    let map_entity = commands.spawn(DungeonMap).id();

    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    // Run the builder
    let mut builder = level_builder(next_depth, MAP_SIZE.x as i32, MAP_SIZE.y as i32);
    builder.build_map();
    let game_map = builder.build_data.map.clone();
    let start = builder.build_data.starting_position.unwrap(); // Must be set
    player_pos.0 = Position {
        x: start.x,
        y: start.y,
    };

    let map_id_component = MapId(next_depth);
    spawn_map_candles(
        &mut commands,
        &game_map,
        &candle_spritesheet,
        map_id_component,
    );

    // Add the tilemap components to the map entity
    commands.entity(map_entity).insert(TilemapBundle {
        grid_size: GRID_SIZE,
        map_type: TilemapType::Square,
        size: MAP_SIZE,
        storage: tile_storage,
        texture: TilemapTexture::Single(dungeon_tileset.texture.clone()),
        tile_size: TILE_SIZE,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });

    next_state.set(AppState::InGame);
}

// pub fn spawn_dungeon(
//     mut commands: Commands,
//     dungeon_tileset: Res<DungeonTileset>,
//     candle_spritesheet: Res<CandleSpritesheet>,
//     active_map: Res<ActiveMap>,
//     mut player_pos: ResMut<PlayerPosition>,
//     mut map_history: ResMut<MapHistory>,
//     mut next_state: ResMut<NextState<AppState>>,
//     mut spawn_message_reader: MessageReader<SpawnDungeonMessage>,
//     state: Res<State<AppState>>,
// ) {
//     spawn_message_reader.clear(); // Clear the event reader

//     let next_depth = active_map.0.0 as i32;
//     // 2. Load existing map or generate new one
//     let game_map = if let Some(existing_game_map) = map_history.maps.remove(&next_depth) {
//         let stair_type = match state.get() {
//             AppState::NextLevel => TileType::UpStairs,
//             AppState::PreviousLevel => TileType::DownStairs,
//             _ => TileType::UpStairs,
//         };
//         let index = existing_game_map
//             .tiles
//             .iter()
//             .position(|&r| r == stair_type);
//         match index {
//             Some(idx) => {
//                 let (x, y) = existing_game_map.idx_xy(idx);
//                 player_pos.0 = Position { x, y };
//             }
//             None => {
//                 println!(
//                     "WARNING: Stair of type {:?} not found on map depth {}. Spawning at (0,0).",
//                     stair_type, next_depth
//                 );
//                 player_pos.0 = Position { x: 0, y: 0 };
//             }
//         }
//         // TODO - need to set player sprite transform
//         existing_game_map
//     } else {
//         println!("Building the map");
//         let mut builder = level_builder(next_depth, MAP_SIZE.x as i32, MAP_SIZE.y as i32);
//         builder.build_map();
//         let new_game_map = builder.build_data.map.clone();
//         let start = builder.build_data.starting_position.unwrap(); // Must be set
//         player_pos.0 = Position {
//             x: start.x,
//             y: start.y,
//         };
//         // TODO - need to set player sprite transform
//         new_game_map
//     };

//     // Now load up the ecs_tilemap
//     let map_id_component = MapId(next_depth);
//     println!("Spawning tiles");
//     spawn_map_tiles(&mut commands, &game_map, map_id_component, &dungeon_tileset);

//     println!("Spawning candles");
//     spawn_map_candles(
//         &mut commands,
//         &game_map,
//         &candle_spritesheet,
//         map_id_component,
//     );

//     map_history.maps.insert(next_depth, game_map);
//     println!("Setting state to INGAME");
//     next_state.set(AppState::InGame);
// }

// Helper function to save the current map's state to history
fn save_current_map_state(
    map_history: &mut MapHistory,
    q_dungeon_map_component: &Query<&TileStorage, With<DungeonMap>>,
    q_candles: &Query<(&crate::components::Position, Entity), With<crate::map::light::Candle>>,
    current_depth: i32,
) {
    if let Ok(_old_tile_storage) = q_dungeon_map_component.single() {
        let mut current_game_map = map_history.maps.remove(&current_depth).unwrap_or_else(|| {
            GameMap::new(
                current_depth,
                MAP_SIZE.x as i32,
                MAP_SIZE.y as i32,
                format!("Floor {}", current_depth),
            )
        });

        // if let Some(player_sp) = player_spawn_point {
        //     current_game_map.player_start_point = Some(player_sp.0);
        // } else {
        //     current_game_map.player_start_point = None;
        // }

        current_game_map.candle_positions.clear();
        for (candle_pos, _candle_entity) in q_candles.iter() {
            current_game_map
                .candle_positions
                .push(Point::new(candle_pos.x, candle_pos.y));
        }

        map_history.maps.insert(current_depth, current_game_map);
    }
}

// Helper function to spawn map tiles and the tilemap bundle
fn spawn_map_tiles(
    commands: &mut Commands,
    game_map: &GameMap,
    map_id_component: MapId,
    dungeon_tileset: &Res<DungeonTileset>,
) -> Entity {
    let map_entity = commands.spawn(DungeonMap).insert(map_id_component).id();

    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let pt = Point::new(x, y);
            let tile_pos = TilePos {
                x: x as u32,
                y: y as u32,
            };
            let tile_type = game_map.get_tile(pt).unwrap();

            let tile_entity = spawn_tile_entity(
                commands,
                map_entity,
                tile_pos,
                tile_type,
                pt,
                map_id_component,
            );
            tile_storage.set(&tile_pos, tile_entity);
        }
    }

    commands.entity(map_entity).insert(TilemapBundle {
        grid_size: GRID_SIZE,
        map_type: TilemapType::Square,
        size: MAP_SIZE,
        storage: tile_storage,
        texture: TilemapTexture::Single(dungeon_tileset.texture.clone()),
        tile_size: TILE_SIZE,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });

    map_entity
}

// Helper function to spawn candles for a map
fn spawn_map_candles(
    commands: &mut Commands,
    game_map: &GameMap,
    candle_spritesheet: &Res<CandleSpritesheet>,
    map_id_component: MapId,
) {
    for pt in game_map.candle_positions.iter() {
        spawn_candle(commands, candle_spritesheet, pt, map_id_component);
    }
}

fn handle_map_disabling(
    mut commands: Commands,
    active_map: Res<ActiveMap>,
    q_dungeon_maps: Query<(Entity, &MapId, Option<&Children>), With<DungeonMap>>,
) {
    for (map_entity, map_id, children) in q_dungeon_maps.iter() {
        if map_id.0 == active_map.0.0 {
            // Enable the active map
            commands.entity(map_entity).remove::<Disabled>();
            if let Some(children_entities) = children {
                for child in children_entities.iter() {
                    commands.entity(child).remove::<Disabled>();
                }
            }
        } else {
            // Disable inactive maps
            commands.entity(map_entity).insert(Disabled);
            if let Some(children_entities) = children {
                for child in children_entities.iter() {
                    commands.entity(child).insert(Disabled);
                }
            }
        }
    }
}
