use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{Collider, Monster, Position},
    constants::BASE_ACTION_COST,
    game::combat::HitEvent,
    map::{Map, tile::is_walkable},
    player::Player,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Wait,
    Move { dir: Direction },
    MeleeAttack { target: Entity },
}

// --- Events ---

#[derive(Message)]
pub struct MovementIntent {
    pub entity: Entity,
    pub dir: Direction,
}

#[derive(Message)]
pub struct MeleeIntent {
    pub attacker: Entity,
    pub target: Entity,
}

#[derive(Message)]
pub struct WaitIntent {
    pub entity: Entity,
}

#[derive(Component, Clone)]
pub struct ActionStats {
    pub move_delay: f32,   // e.g., 0.5 for half time
    pub action_delay: f32, // e.g., 2.0 for double time
}

impl Default for ActionStats {
    fn default() -> Self {
        Self {
            move_delay: 1.0,
            action_delay: 1.0,
        }
    }
}

pub enum ActionCategory {
    Movement,
    General,
}

/// Emitted by any action system when an action successfully resolves (or fails)
/// to signal the turn manager to move to the next entity.
#[derive(Message)]
pub struct ActionFinishedEvent {
    pub entity: Entity,
    pub base_cost: u32,
    pub category: ActionCategory,
}

// --- Systems ---

/// Handles movement. If a collision with a hostile entity is detected,
/// it converts the movement into a MeleeIntent instead.
pub fn handle_movement(
    mut intents: MessageReader<MovementIntent>,
    mut melee_writer: MessageWriter<MeleeIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut actors_query: Query<(Entity, &mut Position, Has<Player>, Has<Monster>, Has<Collider>)>,
    map: Res<Map>,
) {
    for intent in intents.read() {
        let Ok((_, pos, _, _, _)) = actors_query.get(intent.entity) else {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.entity,
                base_cost: BASE_ACTION_COST,
                category: ActionCategory::Movement,
            });
            continue;
        };

        let target_pt = pos.to_point() + intent.dir.offset();

        // 1. Bounds/Wall Check
        if !map.in_bounds(target_pt)
            || !is_walkable(map.tiles[map.xy_idx(target_pt.x, target_pt.y)])
        {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.entity,
                base_cost: BASE_ACTION_COST,
                category: ActionCategory::Movement,
            });
            continue;
        }

        // 2. Occupant Check (Bump-to-Attack / Block)
        let mut bump_target = None;
        for (e, other_pos, other_is_player, other_is_monster, other_has_collider) in actors_query.iter() {
            if other_pos.to_point() == target_pt && e != intent.entity {
                bump_target = Some((e, other_is_player, other_is_monster, other_has_collider));
                break;
            }
        }

        if let Some((target_entity, target_is_player, target_is_monster, target_has_collider)) = bump_target {
            let actor_is_player = actors_query
                .get(intent.entity)
                .map(|(_, _, p, _, _)| p)
                .unwrap_or(false);
            let actor_is_monster = actors_query
                .get(intent.entity)
                .map(|(_, _, _, m, _)| m)
                .unwrap_or(false);

            let is_hostile =
                (actor_is_player && target_is_monster) || (actor_is_monster && target_is_player);

            if is_hostile {
                melee_writer.write(MeleeIntent {
                    attacker: intent.entity,
                    target: target_entity,
                });
                continue;
            } else if target_has_collider {
                // Blocked by friendly/neutral with a Collider
                finish_writer.write(ActionFinishedEvent {
                    entity: intent.entity,
                    base_cost: BASE_ACTION_COST,
                    category: ActionCategory::Movement,
                });
                continue;
            }
            // If neither hostile nor blocking collider, fall through to movement
        }

        // 3. Apply Movement
        if let Ok((_, mut pos, _, _, _)) = actors_query.get_mut(intent.entity) {
            pos.x = target_pt.x;
            pos.y = target_pt.y;
        }
        finish_writer.write(ActionFinishedEvent {
            entity: intent.entity,
            base_cost: BASE_ACTION_COST,
            category: ActionCategory::Movement,
        });
    }
}

pub fn handle_melee(
    mut intents: MessageReader<MeleeIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut hit_events: MessageWriter<HitEvent>,
) {
    for intent in intents.read() {
        hit_events.write(HitEvent {
            attacker: intent.attacker,
            target: intent.target,
        });
        finish_writer.write(ActionFinishedEvent {
            entity: intent.attacker,
            base_cost: BASE_ACTION_COST,
            category: ActionCategory::General,
        });
    }
}

pub fn handle_wait(
    mut intents: MessageReader<WaitIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    for intent in intents.read() {
        finish_writer.write(ActionFinishedEvent {
            entity: intent.entity,
            base_cost: BASE_ACTION_COST,
            category: ActionCategory::General,
        });
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
    NoDirection,
}

impl Direction {
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
