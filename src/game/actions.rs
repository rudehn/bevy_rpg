use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::components::Position;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Wait,
    Move { dir: Direction },
}

#[derive(Debug, PartialEq)]
pub enum ActionResult {
    Success,
    Failure,
    Alternate { action: Action },
}

pub fn perform_action(commands: &mut Commands, actor: &Entity, action: &Action) -> ActionResult {
    match action {
        Action::Wait => ActionResult::Success,
        Action::Move { dir } => perform_move_action(commands, actor, dir),
    }
}

fn perform_move_action(
    commands: &mut Commands,
    actor: &Entity,
    direction: &Direction, // position: &mut Position, // Direct mutable access to the moving entity's position
                           // // Removed map: Res<Map>
                           // current_entity: Entity, // Still needed for collision queries (e.g. Without<current_entity>)
                           // move_direction: Direction,
                           // q_map: Query<&TileStorage, With<DungeonMap>>,
                           // q_blocked_tiles: Query<&TileType, With<Collider>>,
                           // q_collidable_entities: Query<&Position, (With<Collider>, Without<crate::player::Player>)>, // Corrected Without<Entity>
                           // q_tile_types: Query<&TileType>,
                           // map_size: TilemapSize, // Added map_size parameter
                           // floor_depth: i32,      // Added floor_depth parameter
) -> ActionResult {
    // let (dx, dy) = move_direction.offset().to_tuple();

    // let target_x = position.x + dx;
    // let target_y = position.y + dy;

    // // Create EcsMap for bounds checking and tile queries
    // let Ok(tile_storage) = q_map.single() else {
    //     return ActionResult::Failure;
    // };
    // let ecs_map = crate::map::map::EcsMap {
    //     tile_storage,
    //     tile_query: &q_tile_types, // Pass the query for tile types
    //     map_size,
    //     depth: floor_depth,
    // };

    // // 2. Check Bounds using EcsMap
    // if target_x < 0 || target_y < 0 || target_x >= ecs_map.width() || target_y >= ecs_map.height() {
    //     return ActionResult::Failure;
    // }

    // let target_tile_pos = TilePos {
    //     x: target_x as u32,
    //     y: target_y as u32,
    // };

    // // 3. Check Collision via TileStorage and TileType
    // if let Some(tile_entity) = tile_storage.get(&target_tile_pos) {
    //     if q_blocked_tiles.get(tile_entity).is_ok() {
    //         return ActionResult::Failure; // Block movement
    //     }
    // }

    // // Check for other collidable entities
    // for other_collider_pos in q_collidable_entities.iter() {
    //     if other_collider_pos.x == target_x && other_collider_pos.y == target_y {
    //         return ActionResult::Failure; // Block movement if another collidable entity is in the way
    //     }
    // }

    // position.x = target_x;
    // position.y = target_y;

    ActionResult::Success
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NoDirection, // No movement
}

impl Direction {
    pub const CARDINALS: [Self; 4] = [Self::N, Self::E, Self::S, Self::W];
    pub const ALL: [Self; 8] = [
        Self::N,
        Self::NE,
        Self::E,
        Self::SE,
        Self::S,
        Self::SW,
        Self::W,
        Self::NW,
    ];

    pub fn cardinals() -> &'static [Self] {
        &Self::CARDINALS
    }
    pub fn iter() -> &'static [Self] {
        &Self::ALL
    }

    pub fn opposite(&self) -> Self {
        match self {
            Direction::N => Direction::S,
            Direction::S => Direction::N,
            Direction::E => Direction::W,
            Direction::W => Direction::E,
            Direction::NW => Direction::SE,
            Direction::NE => Direction::SW,
            Direction::SW => Direction::NE,
            Direction::SE => Direction::NW,
            Direction::NoDirection => Direction::NoDirection,
        }
    }
    pub fn from_pos(current: &Position, target: &Position) -> Self {
        match target.x.cmp(&current.x) {
            std::cmp::Ordering::Less => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::NW,
                std::cmp::Ordering::Equal => Direction::W,
                std::cmp::Ordering::Greater => Direction::SW,
            },
            std::cmp::Ordering::Equal => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::N,
                std::cmp::Ordering::Equal => Direction::NoDirection,
                std::cmp::Ordering::Greater => Direction::S,
            },
            std::cmp::Ordering::Greater => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::NE,
                std::cmp::Ordering::Equal => Direction::E,
                std::cmp::Ordering::Greater => Direction::SE,
            },
        }
    }

    pub fn offset(&self) -> Point {
        match self {
            Direction::NW => Point { x: -1, y: -1 },
            Direction::N => Point { x: 0, y: -1 },
            Direction::NE => Point { x: 1, y: -1 },
            Direction::E => Point { x: 1, y: 0 },
            Direction::SE => Point { x: 1, y: 1 },
            Direction::S => Point { x: 0, y: 1 },
            Direction::SW => Point { x: -1, y: 1 },
            Direction::W => Point { x: -1, y: 0 },
            Direction::NoDirection => Point { x: 0, y: 0 },
        }
    }
}
