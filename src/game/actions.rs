use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{Monster, Position}, // Import Monster
    game::combat::HitEvent,          // Import HitEvent
    map::{Map, tile::is_walkable},
    player::Player, // Import Player
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Wait,
    Move { dir: Direction },
    MeleeAttack { target: Entity }, // New action variant
}

#[derive(Debug, PartialEq)]
pub enum ActionResult {
    Success,
    Failure,
    Alternate { action: Action },
}

pub fn perform_action(world: &mut World, actor: &Entity, action: &Action) -> ActionResult {
    match action {
        Action::Wait => ActionResult::Success,
        Action::Move { dir } => perform_move_action(world, actor, *dir),
        Action::MeleeAttack { target } => {
            // Send a HitEvent
            world.write_message(HitEvent {
                attacker: *actor,
                target: *target,
            });
            ActionResult::Success
        }
    }
}
fn perform_move_action(world: &mut World, actor: &Entity, dir: Direction) -> ActionResult {
    // 1. Get the current position and the Map resource
    let Some(current_pos) = world.get::<Position>(*actor).cloned() else {
        return ActionResult::Failure;
    };
    let map = world.resource::<Map>();

    // 2. Calculate target
    let offset = dir.offset();
    let target_pt = Point::new(current_pos.x + offset.x, current_pos.y + offset.y);

    // 3. Check Bounds & Walls using the Map Resource
    if !map.in_bounds(target_pt) {
        return ActionResult::Failure;
    }

    let target_idx = map.xy_idx(target_pt.x, target_pt.y);
    if !is_walkable(map.tiles[target_idx]) {
        return ActionResult::Failure;
    }

    // 4. Check for Entity Collisions (e.g., Bump-to-Attack)
    // We search the world for any other entity with a Position at target_pt
    let mut occupants = world.query::<(Entity, &Position)>();
    let bump_target = occupants
        .iter(world)
        .find(|(e, pos)| {
            **pos
                == Position {
                    x: target_pt.x,
                    y: target_pt.y,
                }
                && *e != *actor
        })
        .map(|(e, _)| e);

    if let Some(target_entity) = bump_target {
        // Check for hostility: Player attacking Monster, or Monster attacking Player
        let is_actor_player = world.get::<Player>(*actor).is_some();
        let is_actor_monster = world.get::<Monster>(*actor).is_some();
        let is_target_player = world.get::<Player>(target_entity).is_some();
        let is_target_monster = world.get::<Monster>(target_entity).is_some();

        let is_hostile_bump =
            (is_actor_player && is_target_monster) || (is_actor_monster && is_target_player);

        if is_hostile_bump {
            return ActionResult::Alternate {
                action: Action::MeleeAttack {
                    target: target_entity,
                },
            };
        } else {
            // Not hostile, or bump target is not a recognized Player/Monster, so just block movement for now
            return ActionResult::Failure;
        }
    }

    // 5. Success! Apply the movement
    if let Some(mut pos_component) = world.get_mut::<Position>(*actor) {
        pos_component.x = target_pt.x;
        pos_component.y = target_pt.y;
        ActionResult::Success
    } else {
        ActionResult::Failure
    }
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
                std::cmp::Ordering::Less => Direction::SW,
                std::cmp::Ordering::Equal => Direction::W,
                std::cmp::Ordering::Greater => Direction::NW,
            },
            std::cmp::Ordering::Equal => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::S,
                std::cmp::Ordering::Equal => Direction::NoDirection,
                std::cmp::Ordering::Greater => Direction::N,
            },
            std::cmp::Ordering::Greater => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::SE,
                std::cmp::Ordering::Equal => Direction::E,
                std::cmp::Ordering::Greater => Direction::NE,
            },
        }
    }

    pub fn offset(&self) -> Point {
        match self {
            Direction::NW => Point { x: -1, y: 1 },
            Direction::N => Point { x: 0, y: 1 },
            Direction::NE => Point { x: 1, y: 1 },
            Direction::E => Point { x: 1, y: 0 },
            Direction::SE => Point { x: 1, y: -1 },
            Direction::S => Point { x: 0, y: -1 },
            Direction::SW => Point { x: -1, y: -1 },
            Direction::W => Point { x: -1, y: 0 },
            Direction::NoDirection => Point { x: 0, y: 0 },
        }
    }
}
