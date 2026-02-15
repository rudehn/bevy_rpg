use bevy::prelude::*;
use std::collections::VecDeque;

use crate::components::Player; // Use the Player component
use crate::game::AppState;
use crate::game::actions::{Action, ActionResult, perform_action}; // Import AppState

// Component to mark the entity that signifies the end of a turn round
#[derive(Component)]
pub struct TurnMarker;

// Resource to manage the turn order and current turn state
#[derive(Resource)]
pub struct TurnManager {
    pub turn_queue: VecDeque<Entity>,
    pub player_action_pending: Option<Action>,
}

impl Default for TurnManager {
    fn default() -> Self {
        Self {
            turn_queue: VecDeque::new(),
            player_action_pending: None,
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
        // Player actions are handled by `turn_manager.player_action_pending`.

        // This AI simply returns None, and the advance_turn_and_get_action system

        // will pick up the player's action from the TurnManager.

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

// Plugin to manage the game's turn order and related systems
pub struct TurnOrderPlugin;

impl Plugin for TurnOrderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_turn_order)
            .add_systems(
                Update,
                advance_turn_and_get_action.run_if(in_state(AppState::InGame)),
            );
        // TODO: Add other turn-related systems here later
    }
}
// System to set up the initial turn order when entering the InGame state
fn setup_turn_order(
    mut commands: Commands,
    query_player: Query<Entity, With<Player>>,
    query_actors: Query<Entity, (With<Actor>, Without<Player>)>,
    mut turn_manager: ResMut<TurnManager>,
) {
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

    // Add player to the queue
    if let Ok(player_entity) = query_player.single() {
        turn_manager.turn_queue.push_back(player_entity);
    }

    // Add all other actors (monsters) to the queue
    for actor_entity in query_actors.iter() {
        turn_manager.turn_queue.push_back(actor_entity);
    }

    // Add the turn marker to signal end of round
    turn_manager.turn_queue.push_back(turn_marker_entity);

    // Log the initial turn order for debugging
    info!("Initial Turn Order: {:?}", turn_manager.turn_queue);
    info!("First in Queue: {:?}", turn_manager.turn_queue.front());
}

fn advance_turn_and_get_action(world: &mut World) {
    // 1. Get the current entity and check if it's the player's turn
    // We do this in a small scope to release the borrow on 'world' immediately
    let (current_entity, is_player_turn) = {
        let mut turn_manager = world.resource_mut::<TurnManager>();

        let Some(entity) = turn_manager.turn_queue.pop_front() else {
            return; // Queue empty
        };

        // Check if this entity is the player
        let is_player = world.get::<Player>(entity).is_some();
        (entity, is_player)
    };

    // 2. Try to get the Actor component
    // If the entity doesn't have an Actor component, discard it and return
    let Some(actor) = world.get::<Actor>(current_entity) else {
        info!(
            "Non-actor entity {:?} found in turn queue, discarding.",
            current_entity
        );
        return;
    };

    // 3. Determine the Action
    let action = if is_player_turn {
        world
            .resource_mut::<TurnManager>()
            .player_action_pending
            .take()
    } else {
        // AI logic
        actor.ai.get_action(current_entity, world)
    };

    // 4. Process the Action
    if let Some(mut actual_action) = action {
        loop {
            let result = perform_action(world, &current_entity, &actual_action);
            match result {
                ActionResult::Alternate { action } => actual_action = action,
                ActionResult::Failure if is_player_turn => {
                    // Player failed? Put them back at the front to try again
                    world
                        .resource_mut::<TurnManager>()
                        .turn_queue
                        .push_front(current_entity);
                    break;
                }
                _ => {
                    // Success or Monster Failure: Move to back of queue
                    world
                        .resource_mut::<TurnManager>()
                        .turn_queue
                        .push_back(current_entity);
                    break;
                }
            }
        }
    } else if is_player_turn {
        // Player turn but no input: Re-add to front and wait
        world
            .resource_mut::<TurnManager>()
            .turn_queue
            .push_front(current_entity);
    } else {
        // Monster/Marker had no action: Move to back
        world
            .resource_mut::<TurnManager>()
            .turn_queue
            .push_back(current_entity);
    }
}
