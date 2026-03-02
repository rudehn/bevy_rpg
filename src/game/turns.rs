use bevy::prelude::*;
use std::collections::VecDeque;

use crate::components::GameEntityMarker;
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

/// Emitted when the global TurnMarker entity finishes its turn, signaling a full turn cycle.
#[derive(Message)]
pub struct TurnEndEvent;

/// Marker component indicating it is currently this entity's turn.
/// Execution systems or AI systems look for this to know when to act.
#[derive(Component)]
pub struct MyTurn;

#[derive(Resource, Default)]
pub struct TurnManager {
    // Stores (Entity, Scheduled Time). We will keep this sorted.
    pub turn_queue: Vec<(Entity, u32)>,
    pub player_action_pending: Option<Action>,
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
            .add_message::<TurnEndEvent>()
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
    let turn_marker_entity = commands.spawn((TurnMarker, GameEntityMarker)).id();
    turn_manager.turn_queue.clear();
    // Start the global clock at 0
    turn_manager.current_time = 0;
    turn_manager.add_entity(turn_marker_entity);
}

/// The turn system now just labels all entities ready to act.
fn select_next_actor(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    if turn_manager.turn_queue.is_empty() {
        return;
    }

    // Sort to ensure we always pick the lowest time first
    turn_manager.turn_queue.sort_by_key(|&(_, time)| time);

    let next_scheduled_time = turn_manager.turn_queue[0].1;
    turn_manager.current_time = next_scheduled_time;

    let mut player_ready = false;
    let mut npc_tagged = false;

    // Identify all actors ready at this time slice
    // We look at the queue and tag everyone whose time is <= current_time
    // But we MUST stop if we hit the player to gather input.

    let mut i = 0;
    while i < turn_manager.turn_queue.len() {
        let (entity, time) = turn_manager.turn_queue[i];
        if time > turn_manager.current_time {
            break;
        }

        if query_player.get(entity).is_ok() {
            player_ready = true;
            // If we have already tagged some NPCs this batch, we MUST process them FIRST
            // before switching to PlayerInput state.
            if npc_tagged {
                break;
            } else {
                // If no NPCs were tagged yet, the player is the very first one ready.
                // We'll tag them and go to input.
                commands.entity(entity).insert(MyTurn);
                next_state.set(TurnState::PlayerInput);
                return;
            }
        } else {
            // It's an NPC or Marker
            commands.entity(entity).insert(MyTurn);
            npc_tagged = true;
        }
        i += 1;
    }

    // Remove the entities we tagged from the queue (they will be re-inserted by resolve_turn_end)
    // IMPORTANT: We only remove the ones we tagged.
    for _ in 0..i {
        turn_manager.turn_queue.remove(0);
    }

    if npc_tagged {
        next_state.set(TurnState::Processing);
    } else if player_ready {
        // This case should be handled by the "if no NPCs tagged yet" block above,
        // but as a fallback:
        next_state.set(TurnState::PlayerInput);
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

    // Note: Player was already removed from queue in select_next_actor or continue_turn_processing
    // because they are acting NOW.

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
    let mut query = world.query_filtered::<Entity, (With<MonsterAI>, With<MyTurn>)>();
    let entities: Vec<Entity> = query.iter(world).collect();

    for entity in entities {
        if let Some(mut monster_ai) = world.entity_mut(entity).take::<MonsterAI>() {
            monster_ai.execute(entity, world);
            world.entity_mut(entity).insert(monster_ai);
            world.entity_mut(entity).remove::<MyTurn>();
        }
    }
}

/// BRIDGE: Triggers Marker Logic
fn marker_dispatch(
    mut commands: Commands,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut turn_end_writer: MessageWriter<TurnEndEvent>,
    query: Query<Entity, (With<TurnMarker>, With<MyTurn>)>,
) {
    let mut actors = 0;
    for entity in query.iter() {
        finish_writer.write(ActionFinishedEvent {
            entity: entity,
            base_cost: BASE_ACTION_COST,
            category: ActionCategory::Movement,
        });
        turn_end_writer.write(TurnEndEvent);
        commands.entity(entity).remove::<MyTurn>();
        actors += 1;
    }
}

fn resolve_turn_end(
    mut events: MessageReader<ActionFinishedEvent>,
    mut turn_manager: ResMut<TurnManager>,
    stats_query: Query<&ActionStats>,
) {
    for event in events.read() {
        let entity = event.entity;
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
        turn_manager.turn_queue.sort_by_key(|&(_, time)| time);
    }
}

fn continue_turn_processing(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    // Check if we can immediately trigger another batch of NPCs who are ready "now"
    let mut npc_added = false;

    // Sort just in case
    turn_manager.turn_queue.sort_by_key(|&(_, time)| time);

    while !turn_manager.turn_queue.is_empty() {
        let (next_entity, next_time) = turn_manager.turn_queue[0];

        if next_time > turn_manager.current_time {
            break;
        }

        if query_player.get(next_entity).is_ok() {
            // If NPCs were already added this frame, we let them act first.
            // If not, we switch to player input.
            if !npc_added {
                let (entity, _) = turn_manager.turn_queue.remove(0);
                commands.entity(entity).insert(MyTurn);
                next_state.set(TurnState::PlayerInput);
                return;
            }
            break;
        }

        let (entity, _) = turn_manager.turn_queue.remove(0);
        commands.entity(entity).insert(MyTurn);
        npc_added = true;
    }

    if !npc_added {
        next_state.set(TurnState::NextTurn);
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
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        action = Some(Action::Move { dir: Direction::N });
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        action = Some(Action::Move { dir: Direction::W });
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        action = Some(Action::Move { dir: Direction::S });
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
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
