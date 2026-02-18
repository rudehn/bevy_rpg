// Here we will put the generic monster ai.
// The AI's main job is to resolve to a single action that the entity wants to take
use crate::{
    components::Position,
    game::actions::{Action, Direction},
};
use bevy::prelude::*;
use bracket_lib::prelude::Algorithm2D;
use rand::seq::SliceRandom;

#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

pub trait ActorAI: Send + Sync {
    // Modified to provide world access for AI decisions
    fn get_action(&self, entity: Entity, world: &World) -> Option<Action>;
}

#[derive(Default)]

pub struct PlayerAI; // PlayerAI does not need any internal state for now

impl ActorAI for PlayerAI {
    fn get_action(&self, _entity: Entity, _world: &World) -> Option<Action> {
        // world
        //     .resource_mut::<TurnManager>()
        //     .player_action_pending
        //     .take()
        None
    }
}

#[derive(Default)]

pub struct TurnAI;

impl ActorAI for TurnAI {
    fn get_action(&self, _entity: Entity, _world: &World) -> Option<Action> {
        info!("Turn Marker reached! End of round processing...");
        None
    }
}

#[derive(Default)]
pub struct MonsterAI {}
impl ActorAI for MonsterAI {
    fn get_action(&self, entity: Entity, world: &World) -> Option<Action> {
        let mut rng = rand::rng();
        let map = world.resource::<crate::map::Map>();
        let pos = world.get::<Position>(entity)?;

        let mut directions = Direction::CARDINALS.to_vec();
        directions.shuffle(&mut rng);

        for dir in directions {
            let offset = dir.offset();
            let target = bracket_lib::prelude::Point::new(pos.x + offset.x, pos.y + offset.y);

            if map.in_bounds(target) {
                let idx = map.xy_idx(target.x, target.y);
                if crate::map::tile::is_walkable(map.tiles[idx]) {
                    return Some(Action::Move { dir });
                }
            }
        }

        // If no walkable direction is found, just wait
        Some(Action::Wait)
    }
}
