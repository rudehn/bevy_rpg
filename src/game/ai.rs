// Here we will put the generic monster ai.
// The AI's main job is to resolve to a single action that the entity wants to take
use crate::{
    components::{Position, Viewshed}, // Monster is no longer directly used here, but in spawner
    game::actions::{Action, Direction},
    map::{Map, tile::is_walkable},
    player::Player,
};
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point, a_star_search}; // Removed BaseMap, DistanceAlg
use rand::rng;
use rand::seq::SliceRandom;

#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

pub trait ActorAI: Send + Sync {
    // Modified to provide world access for AI decisions
    fn get_action(&mut self, entity: Entity, world: &mut World) -> Option<Action>;
}

#[derive(Default)]
pub struct PlayerAI; // PlayerAI does not need any internal state for now

impl ActorAI for PlayerAI {
    fn get_action(&mut self, _entity: Entity, _world: &mut World) -> Option<Action> {
        None
    }
}

#[derive(Default)]
pub struct TurnAI;

impl ActorAI for TurnAI {
    fn get_action(&mut self, _entity: Entity, _world: &mut World) -> Option<Action> {
        None
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)] // Added Default
enum MonsterAIMode {
    #[default] // Set Asleep as default
    Asleep,
    Hunting,
    Wandering,
}

#[derive(Default, Component)]
pub struct MonsterAI {
    mode: MonsterAIMode,
    last_known_player_position: Option<Point>,
    path: Vec<Point>, // Stores the current path for hunting/pursuit
}

impl ActorAI for MonsterAI {
    fn get_action(&mut self, entity: Entity, world: &mut World) -> Option<Action> {
        Some(Action::Wait)
        // let mut rng = rng();
        // let map = world.resource::<Map>();
        // let monster_pos = world.get::<Position>(entity)?.to_point();
        // let monster_viewshed = world.get::<Viewshed>(entity)?;

        // let mut player_query = world.query_filtered::<(&Position, &Viewshed), With<Player>>();
        // let Some((player_pos, _)) = player_query.iter(world).next() else {
        //     return Some(Action::Wait);
        // };
        // let player_point = player_pos.to_point();
        // let is_player_visible = monster_viewshed.visible_tiles.contains(&player_point);

        // // --- 1. State Transitions & Path Updates ---
        // match self.mode {
        //     MonsterAIMode::Asleep => {
        //         if is_player_visible {
        //             self.mode = MonsterAIMode::Hunting;
        //         } else {
        //             return Some(Action::Wait);
        //         }
        //     }
        //     MonsterAIMode::Hunting => {
        //         if is_player_visible {
        //             self.last_known_player_position = Some(player_point);
        //         }

        //         // If we reached our destination and still can't see the player, wander
        //         if !is_player_visible && Some(monster_pos) == self.last_known_player_position {
        //             self.mode = MonsterAIMode::Wandering;
        //             self.last_known_player_position = None;
        //             self.path.clear();
        //         }
        //     }
        //     MonsterAIMode::Wandering => {
        //         if is_player_visible {
        //             self.mode = MonsterAIMode::Hunting;
        //         }
        //     }
        // }

        // // --- 2. Action Logic based on Current Mode ---
        // match self.mode {
        //     MonsterAIMode::Hunting => {
        //         // Always try to update the path if we have a target
        //         if let Some(target) = self.last_known_player_position {
        //             let start_idx = map.point2d_to_index(monster_pos);
        //             let end_idx = map.point2d_to_index(target);
        //             let path = a_star_search(start_idx, end_idx, map);

        //             if path.success && path.steps.len() > 1 {
        //                 let next_step = map.index_to_point2d(path.steps[1]);
        //                 let dir = Direction::from_pos(
        //                     &Position::from_point(monster_pos),
        //                     &Position::from_point(next_step),
        //                 );
        //                 return Some(Action::Move { dir });
        //             }
        //         }
        //         Some(Action::Wait)
        //     }

        //     MonsterAIMode::Wandering => {
        //         let mut directions = Direction::ALL.to_vec();
        //         directions.shuffle(&mut rng);

        //         for dir in directions {
        //             let offset = dir.offset();
        //             let target = Point::new(monster_pos.x + offset.x, monster_pos.y + offset.y);

        //             if map.in_bounds(target)
        //                 && is_walkable(map.tiles[map.xy_idx(target.x, target.y)])
        //             {
        //                 return Some(Action::Move { dir });
        //             }
        //         }
        //         Some(Action::Wait)
        //     }

        //     MonsterAIMode::Asleep => Some(Action::Wait),
        // }
    }
}
