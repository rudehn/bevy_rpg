use bevy::prelude::*;

use crate::game::actions::Action;

// TODO add turn order queue Resource. This should be a TurnQueue(VecDeque<Entity>)

// TODO add a resource that stores an Option<Action> enum value

// TODO fill in an enemy AI class that generates a move action in a random direction
#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

pub trait ActorAI: Send + Sync {
    // TODO - we might need some way to query the ecs for state
    fn get_action(&self) -> Option<Action>;
}

pub struct MonsterAI {}
impl ActorAI for MonsterAI {
    fn get_action(&self) -> Option<Action> {
        // TODO - return a random move direction
        None
    }
}

pub fn get_player_action(mut commands: Commands) {
    // This function should always run while we're in the INGAME state
    // It inspects the keyboard input and if the player presses WASD, it sets
    // the player's action as a move action in the appropriate direction
    // TODO - figure out how to store the player's action
}

pub fn process_next_entity_in_queue(mut commands: Commands) {
    // Here we go through our turn order queue and get the next entity to perform an action
    // If there is no action, such as the player hasn't pressed a key yet, we return early from the system
    // If there is an action, we pop the player off the front of the queue
    // Then we dispatch an event indicating who's turn it is
    // And finally we add the player to the back of the queue
    // For now, we don't worry about speed. We'll assume all entities have the same speed
}

pub fn process_entity_action(mut commands: Commands) {
    // Here we get the entity who's turn it is, triggered by an event
    // For the player, fetch the player's action from state and perform it
    // If the player's action results in a failure, don't waist the players turn
    // And add the player back at the beginning of the queue so they will have another turn
    // For enemies, query their AI and get the action and perform it. It is ok if their turn can fail
    //
    // The action can return a success, failure or alternative action
    // If an alternative action is detected, keep trying to resolve that action untill we finish at a success or failure
    // This may look like a move action that turns into an attack action (not yet implemented)
}

// I want to keep a single IN_GAME state rather than multiple player, monster, turn end states. The design
// must allow for that.
