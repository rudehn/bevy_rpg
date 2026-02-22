use bevy::prelude::*;
use std::collections::VecDeque;

use crate::game::AppState;
use crate::game::actions::{
    Action, ActionFinishedEvent, Direction, MeleeIntent, MovementIntent, WaitIntent, handle_melee,
    handle_movement, handle_wait,
};
use crate::game::ai::MonsterAI;
use crate::player::{MovementTimer, Player};

#[derive(Component)]
pub struct TurnMarker;

/// Marker component indicating it is currently this entity's turn.
/// Execution systems or AI systems look for this to know when to act.
#[derive(Component)]
pub struct MyTurn;

#[derive(Resource, Default)]
pub struct TurnManager {
    pub turn_queue: VecDeque<Entity>,
    pub player_action_pending: Option<Action>,
    pub acting_entity: Option<Entity>,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum TurnState {
    #[default]
    Waiting,
    NextTurn,
    PlayerInput,
    Processing,
}

pub struct TurnOrderPlugin;

impl Plugin for TurnOrderPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<TurnState>()
            .add_message::<MovementIntent>()
            .add_message::<MeleeIntent>()
            .add_message::<WaitIntent>()
            .add_message::<ActionFinishedEvent>()
            .add_systems(OnEnter(AppState::InGame), (setup_turn_order, start_turns))
            .add_systems(
                Update,
                (
                    select_next_actor.run_if(in_state(TurnState::NextTurn)),
                    handle_player_input.run_if(in_state(TurnState::PlayerInput)),
                    // --- Brain Systems ---
                    // These respond to the "MyTurn" component and emit Intents
                    player_ai_bridge.run_if(in_state(TurnState::Processing)),
                    monster_ai_dispatch.run_if(in_state(TurnState::Processing)),
                    marker_dispatch.run_if(in_state(TurnState::Processing)),
                    // --- Execution Systems ---
                    handle_movement.run_if(in_state(TurnState::Processing)),
                    handle_melee.run_if(in_state(TurnState::Processing)),
                    handle_wait.run_if(in_state(TurnState::Processing)),
                    // --- Cleanup ---
                    resolve_turn_end.run_if(in_state(TurnState::Processing)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn start_turns(mut next_state: ResMut<NextState<TurnState>>) {
    next_state.set(TurnState::NextTurn);
}

fn setup_turn_order(mut commands: Commands, mut turn_manager: ResMut<TurnManager>) {
    let turn_marker_entity = commands.spawn(TurnMarker).id();
    turn_manager.turn_queue.clear();
    turn_manager.turn_queue.push_back(turn_marker_entity);
}

/// The turn system now just labels the entity and steps aside.
fn select_next_actor(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    let Some(current_entity) = turn_manager.turn_queue.pop_front() else {
        return;
    };
    turn_manager.acting_entity = Some(current_entity);

    // Tag the entity so its specific "Brain System" knows to fire
    commands.entity(current_entity).insert(MyTurn);

    if let Ok(player_entity) = query_player.single() {
        if current_entity == player_entity {
            next_state.set(TurnState::PlayerInput);
        } else {
            next_state.set(TurnState::Processing);
        }
    }
}

/// BRIDGE: Converts Player Input Resource into Action Intents
fn player_ai_bridge(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    mut move_events: MessageWriter<MovementIntent>,
    mut melee_events: MessageWriter<MeleeIntent>,
    mut wait_events: MessageWriter<WaitIntent>,
    query: Query<Entity, (With<Player>, With<MyTurn>)>,
) {
    let Ok(player_entity) = query.single() else {
        return;
    };

    // We only act if there's a pending action from the input system
    if let Some(action) = turn_manager.player_action_pending.take() {
        match action {
            Action::Wait => {
                wait_events.write(WaitIntent {
                    entity: player_entity,
                });
            }

            Action::Move { dir } => {
                move_events.write(MovementIntent {
                    entity: player_entity,
                    dir,
                });
            }
            Action::MeleeAttack { target } => {
                melee_events.write(MeleeIntent {
                    attacker: player_entity,
                    target,
                });
            }
        }
        // Remove MyTurn so we don't dispatch again
        commands.entity(player_entity).remove::<MyTurn>();
    }
}

/// BRIDGE: Triggers Monster AI
fn monster_ai_dispatch(world: &mut World) {
    // We use a query to find monsters whose turn it is
    let mut query = world.query_filtered::<Entity, (With<MonsterAI>, With<MyTurn>)>();
    let Some(entity) = query.iter(world).next() else {
        return;
    };

    let mut monster_ai = world.entity_mut(entity).take::<MonsterAI>().unwrap();
    monster_ai.execute(entity, world);
    world.entity_mut(entity).insert(monster_ai);
    world.entity_mut(entity).remove::<MyTurn>();
}

/// BRIDGE: Triggers Marker Logic
fn marker_dispatch(
    mut commands: Commands,
    mut wait_events: MessageWriter<WaitIntent>,
    query: Query<Entity, (With<TurnMarker>, With<MyTurn>)>,
) {
    let Ok(entity) = query.single() else {
        return;
    };
    wait_events.write(WaitIntent { entity });
    commands.entity(entity).remove::<MyTurn>();
}

fn resolve_turn_end(
    mut events: MessageReader<ActionFinishedEvent>,
    mut turn_manager: ResMut<TurnManager>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    for _ in events.read() {
        if let Some(entity) = turn_manager.acting_entity.take() {
            turn_manager.turn_queue.push_back(entity);
            next_state.set(TurnState::NextTurn);
        }
    }
}

fn handle_player_input(
    time: Res<Time>,
    mut timer: ResMut<MovementTimer>,
    keys: Res<ButtonInput<KeyCode>>,
    mut turn_manager: ResMut<TurnManager>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.is_finished() {
        return;
    }

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
