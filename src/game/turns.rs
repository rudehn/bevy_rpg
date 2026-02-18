use bevy::prelude::*;
use std::collections::VecDeque;

use crate::game::AppState;
use crate::game::actions::{Action, ActionResult, Direction, perform_action};
use crate::player::{MovementTimer, Player}; // Import AppState

// Component to mark the entity that signifies the end of a turn round
#[derive(Component)]
pub struct TurnMarker;

// Resource to manage the turn order and current turn state
#[derive(Resource)]
pub struct TurnManager {
    pub turn_queue: VecDeque<Entity>,
    pub player_action_pending: Option<Action>,
    pub acting_entity: Option<Entity>,
}

impl Default for TurnManager {
    fn default() -> Self {
        Self {
            turn_queue: VecDeque::new(),
            player_action_pending: None,
            acting_entity: None,
        }
    }
}

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
    fn get_action(&self, _entity: Entity, _world: &World) -> Option<Action> {
        // TODO - implement actual monster AI
        Some(Action::Wait)
    }
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum TurnState {
    #[default]
    Waiting, // Doing nothing (map loading, etc)
    NextTurn,    // Logic to pop the next entity from the queue
    PlayerInput, // Loop here until a key is pressed
    Processing,  // AI is thinking or an action is being animated
}

// Plugin to manage the game's turn order and related systems
pub struct TurnOrderPlugin;

impl Plugin for TurnOrderPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<TurnState>()
            .add_systems(OnEnter(AppState::InGame), (setup_turn_order, start_turns))
            .add_systems(
                Update,
                (
                    select_next_actor.run_if(in_state(TurnState::NextTurn)),
                    handle_player_input.run_if(in_state(TurnState::PlayerInput)),
                    execute_action.run_if(in_state(TurnState::Processing)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn start_turns(mut next_state: ResMut<NextState<TurnState>>) {
    next_state.set(TurnState::NextTurn);
}

// System to set up the initial turn order when entering the InGame state
fn setup_turn_order(mut commands: Commands, mut turn_manager: ResMut<TurnManager>) {
    // Spawn the TurnMarker entity
    let turn_marker_entity = commands
        .spawn((
            TurnMarker,
            Actor {
                ai: Box::new(TurnAI::default()),
            },
        ))
        .id();

    // Clear any existing queue (useful if re-entering InGame state)
    turn_manager.turn_queue.clear();

    // Add the turn marker to signal end of round
    turn_manager.turn_queue.push_back(turn_marker_entity);
}

fn execute_action(world: &mut World) {
    let current_entity = world.resource_mut::<TurnManager>().acting_entity.unwrap();

    // 1. Get the action (from Player Input or AI)
    let is_player = world.get::<Player>(current_entity).is_some();
    let action = if is_player {
        world
            .resource_mut::<TurnManager>()
            .player_action_pending
            .take()
    } else {
        let actor = world
            .get::<Actor>(current_entity)
            .expect("Entity in queue has no Actor component");
        actor.ai.get_action(current_entity, world)
    };

    // 2. Process the action
    if let Some(mut actual_action) = action {
        loop {
            let result = perform_action(world, &current_entity, &actual_action);
            match result {
                ActionResult::Alternate { action } => {
                    actual_action = action;
                }
                ActionResult::Failure if is_player => {
                    // If player fails (walks into wall), let them try again
                    world
                        .resource_mut::<NextState<TurnState>>()
                        .set(TurnState::PlayerInput);
                    return;
                }
                _ => {
                    // Success or AI failure: End turn
                    break;
                }
            }
        }
    }

    // 3. Cleanup and loop back
    world
        .resource_mut::<TurnManager>()
        .turn_queue
        .push_back(current_entity);
    world
        .resource_mut::<NextState<TurnState>>()
        .set(TurnState::NextTurn);
}

fn handle_player_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn_manager: ResMut<TurnManager>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    let mut action = None;
    if keys.pressed(KeyCode::KeyW) {
        action = Some(Action::Move { dir: Direction::N });
    }
    if keys.pressed(KeyCode::KeyA) {
        action = Some(Action::Move { dir: Direction::W });
    }
    if keys.pressed(KeyCode::KeyS) {
        action = Some(Action::Move { dir: Direction::S });
    }
    if keys.pressed(KeyCode::KeyD) {
        action = Some(Action::Move { dir: Direction::E });
    }
    if keys.pressed(KeyCode::Space) {
        action = Some(Action::Wait);
    }
    if let Some(act) = action {
        turn_manager.player_action_pending = Some(act);
        next_state.set(TurnState::Processing);
    }
}

fn select_next_actor(
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    let Some(current_entity) = turn_manager.turn_queue.pop_front() else {
        return;
    };

    // Store who is acting now
    turn_manager.acting_entity = Some(current_entity);

    if let Ok(player_entity) = query_player.single() {
        if current_entity == player_entity {
            next_state.set(TurnState::PlayerInput);
        } else {
            next_state.set(TurnState::Processing);
        }
    }
}
