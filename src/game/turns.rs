use bevy::prelude::*;
use std::collections::VecDeque;

use crate::constants::BASE_ACTION_COST;
use crate::game::AppState;
use crate::game::actions::{
    Action, ActionCategory, ActionFinishedEvent, ActionStats, Direction, MeleeIntent,
    MovementIntent, WaitIntent, handle_melee, handle_movement, handle_wait,
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
    // Stores (Entity, Scheduled Time). We will keep this sorted.
    pub turn_queue: Vec<(Entity, u32)>,
    pub player_action_pending: Option<Action>,
    pub acting_entity: Option<Entity>,
    pub current_time: u32, // The global clock
}

impl TurnManager {
    pub fn add_entity(&mut self, entity: Entity) {
        self.turn_queue.push((entity, self.current_time));
    }
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
                    (
                        // --- Brain Systems ---
                        // These respond to the "MyTurn" component and emit Intents
                        player_ai_bridge,
                        monster_ai_dispatch,
                        marker_dispatch,
                        // --- Execution Systems ---
                        handle_movement,
                        handle_melee,
                        handle_wait,
                        // --- Cleanup ---
                        resolve_turn_end,
                        continue_turn_processing,
                    )
                        .chain()
                        .run_if(in_state(TurnState::Processing)),
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
    // Start the global clock at 0
    turn_manager.current_time = 0;
    turn_manager.add_entity(turn_marker_entity);
}

/// The turn system now just labels the entity and steps aside.
fn select_next_actor(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    // Remove the first element (lowest scheduled time)
    if turn_manager.turn_queue.is_empty() {
        return;
    }
    let (current_entity, scheduled_time) = turn_manager.turn_queue.remove(0);

    // Fast-forward the global clock to when this entity is ready
    turn_manager.current_time = scheduled_time;
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
    } else {
        // If no action was pending, the player implicitly waits.
        // This ensures ActionFinishedEvent is always sent,
        // preventing the turn manager from stalling.
        wait_events.write(WaitIntent {
            entity: player_entity,
        });
    }
    commands.entity(player_entity).remove::<MyTurn>();
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
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    query: Query<Entity, (With<TurnMarker>, With<MyTurn>)>,
) {
    let Ok(entity) = query.single() else {
        return;
    };
    finish_writer.write(ActionFinishedEvent {
        entity: entity,
        base_cost: BASE_ACTION_COST,
        category: ActionCategory::Movement,
    });
    commands.entity(entity).remove::<MyTurn>();
}

fn resolve_turn_end(
    mut events: MessageReader<ActionFinishedEvent>,
    mut turn_manager: ResMut<TurnManager>,
    stats_query: Query<&ActionStats>,
) {
    for event in events.read() {
        if let Some(entity) = turn_manager.acting_entity.take() {
            // 1. Get the entity's multipliers, defaulting to 1.0 (100%)
            let stats = stats_query.get(entity).cloned().unwrap_or_default();

            // 2. Determine which multiplier applies
            let multiplier = match event.category {
                ActionCategory::Movement => stats.move_delay,
                ActionCategory::General => stats.action_delay,
            };

            // 3. Calculate final cost and their next act time
            let final_cost = (event.base_cost as f32 * multiplier).round() as u32;
            let next_act_time = turn_manager.current_time + final_cost;

            // 4. Put them back in the queue and sort it
            turn_manager.turn_queue.push((entity, next_act_time));

            // Sort ascending by the scheduled time (the `u32` in the tuple)
            turn_manager.turn_queue.sort_by_key(|&(_, time)| time);
        }
    }
}

fn continue_turn_processing(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    if turn_manager.acting_entity.is_some() {
        return;
    }

    if turn_manager.turn_queue.is_empty() {
        next_state.set(TurnState::NextTurn);
        return;
    }

    let (next_entity, next_time) = turn_manager.turn_queue[0];
    let is_player = query_player.get(next_entity).is_ok();

    if is_player || next_time > turn_manager.current_time {
        next_state.set(TurnState::NextTurn);
    } else {
        let (entity, time) = turn_manager.turn_queue.remove(0);
        turn_manager.current_time = time;
        turn_manager.acting_entity = Some(entity);
        commands.entity(entity).insert(MyTurn);
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
