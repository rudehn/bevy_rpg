use bevy::prelude::*;

use crate::components::GameEntityMarker;
use crate::constants::BASE_ACTION_COST;
use crate::game::AppState;
use crate::game::InGameState;
use crate::game::actions::{
    Action, ActionFinishedEvent, ActionGuard, ActionKind, Direction, FreeActionEvent, MeleeIntent,
    MovementIntent, OpenChestIntent, OpenDoorIntent, UnlockDoorIntent, PendingPlayerAction,
    PickUpIntent, SpeedStats, WaitIntent, dispatch_player_action, finish_turn, handle_door_open,
    handle_unlock_door, handle_melee, handle_movement, handle_open_chest, handle_pickup,
    handle_wait,
};
use crate::game::ai::MonsterAI;
use crate::game::magic::StatusEffects;
use crate::game::targeting::{TargetingContext, TargetingMode};
use crate::map::map::populate_blocked_tiles;
use crate::player::{MovementTimer, Player};
use crate::ui::game_log::GameLogMessage;

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

/// Ordered phases within `TurnState::Processing`.
///
/// Domain plugins register their action handlers into the appropriate phase
/// (usually `ResolveActions`) instead of editing this plugin. To add a new
/// action handler:
///
/// 1. Define the intent message + handler in your domain module
/// 2. In your plugin's `build()`:
///    ```ignore
///    app.add_message::<MyIntent>()
///       .add_systems(Update, handle_my_action.in_set(ProcessingPhase::ResolveActions));
///    ```
/// 3. Add the dispatch arm in `dispatch_player_action` (actions.rs)
/// 4. Add the key binding in `handle_player_input` (turns.rs)
///
/// That's it — no changes needed in `TurnOrderPlugin`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProcessingPhase {
    /// Pre-execution: populate blocked tiles, dispatch intents from AI/player.
    Brain,
    /// Resolves movement first since it can redirect to melee/door intents.
    ResolveMovement,
    /// All other action handlers (items, spells, melee, ranged, etc.).
    ResolveActions,
    /// Post-execution: resolve turn end, continue processing.
    Cleanup,
}

pub struct TurnOrderPlugin;

impl Plugin for TurnOrderPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<TurnState>()
            .init_resource::<PendingPlayerAction>()
            // Core intent messages written by dispatch_player_action.
            .add_message::<MovementIntent>()
            .add_message::<MeleeIntent>()
            .add_message::<WaitIntent>()
            .add_message::<PickUpIntent>()
            .add_message::<OpenDoorIntent>()
            .add_message::<UnlockDoorIntent>()
            .add_message::<OpenChestIntent>()
            // RangedAttackIntent registered by RangedPlugin.
            // UseItemMessage registered by EffectsPlugin.
            // Tile mutation message (syncs Map resource ↔ ECS tile entities).
            .add_message::<crate::map::tile::TileMutationMessage>()
            .add_message::<crate::map::tile::DecorationMutationMessage>()
            // GOAP action messages.
            .add_message::<crate::game::goap::DropAtHoardMessage>()
            // Turn-lifecycle messages.
            .add_message::<ActionFinishedEvent>()
            .add_message::<FreeActionEvent>()
            .add_message::<TurnEndEvent>()
            // --- Processing phase ordering ---
            .configure_sets(
                Update,
                (
                    ProcessingPhase::Brain,
                    ProcessingPhase::ResolveMovement,
                    ProcessingPhase::ResolveActions,
                    ProcessingPhase::Cleanup,
                )
                    .chain()
                    .run_if(in_state(TurnState::Processing)),
            )
            .add_systems(OnEnter(AppState::InGame), (setup_turn_order, start_turns))
            .add_systems(
                Update,
                (
                    select_next_actor
                        .run_if(in_state(TurnState::NextTurn))
                        .after(crate::game::systems::fov_update_system),
                    handle_player_input
                        .run_if(
                            in_state(TurnState::PlayerInput).and(in_state(InGameState::Running)),
                        ),
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // --- Brain phase ---
            .add_systems(
                Update,
                (
                    populate_blocked_tiles,
                    crate::game::squad::squad_coordinator_system,
                    dispatch_player_action,
                    crate::game::goap::goap_ai_dispatch,
                    monster_ai_dispatch,
                    marker_dispatch,
                )
                    .chain()
                    .in_set(ProcessingPhase::Brain),
            )
            // --- Movement phase (runs before other handlers since it can redirect) ---
            .add_systems(
                Update,
                handle_movement.in_set(ProcessingPhase::ResolveMovement),
            )
            // --- Action handlers (core) ---
            .add_systems(
                Update,
                (
                    handle_melee,
                    // handle_ranged_attack registered by RangedPlugin.
                    handle_door_open,
                    handle_unlock_door,
                    handle_open_chest,
                    handle_pickup,
                    handle_wait,
                    crate::game::goap::handle_drop_at_hoard,
                    // handle_use_item registered by EffectsPlugin.
                )
                    .in_set(ProcessingPhase::ResolveActions),
            )
            // --- Cleanup phase ---
            .add_systems(
                Update,
                (
                    crate::game::tile_promotion::tile_promotion_tick_system,
                    crate::map::tile::apply_tile_mutations,
                    crate::map::tile::apply_decoration_mutations,
                    action_guard_safety_net,
                    resolve_free_actions,
                    resolve_turn_end,
                    continue_turn_processing,
                )
                    .chain()
                    .in_set(ProcessingPhase::Cleanup),
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
    query_all: Query<Entity>,
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
    let mut npc_tagged = 0u32;
    const MAX_NPC_BATCH: u32 = 16;

    // Identify actors ready at this time slice, batching NPCs up to MAX_NPC_BATCH.
    // We MUST stop if we hit the player to gather input.

    let mut i = 0;
    while i < turn_queue_len(turn_manager.as_ref()) {
        let (entity, time) = turn_manager.turn_queue[i];
        if time > turn_manager.current_time {
            break;
        }

        // Safety check: ensure entity still exists in the world
        if !query_all.contains(entity) {
            turn_manager.turn_queue.remove(i);
            continue;
        }

        if query_player.get(entity).is_ok() {
            player_ready = true;
            // If we have already tagged some NPCs this batch, we MUST process them FIRST
            // before switching to PlayerInput state.
            if npc_tagged > 0 {
                break;
            } else {
                info!(
                    "TURN: select → Player at time={}, queue_size={}, next_npc_time={}",
                    time,
                    turn_manager.turn_queue.len(),
                    turn_manager
                        .turn_queue
                        .first()
                        .map(|(_, t)| *t)
                        .unwrap_or(0)
                );
                commands.queue(move |world: &mut World| {
                    if let Ok(mut ec) = world.get_entity_mut(entity) {
                        ec.insert(MyTurn);
                    }
                });
                next_state.set(TurnState::PlayerInput);
                return;
            }
        } else {
            // It's an NPC or Marker — cap batch size to spread work across frames
            if npc_tagged >= MAX_NPC_BATCH {
                break;
            }
            commands.queue(move |world: &mut World| {
                if let Ok(mut ec) = world.get_entity_mut(entity) {
                    ec.insert(MyTurn);
                }
            });
            npc_tagged += 1;
        }
        i += 1;
    }

    // Remove the entities we tagged from the queue (they will be re-inserted by resolve_turn_end)
    // IMPORTANT: We only remove the ones we tagged.
    for _ in 0..i {
        turn_manager.turn_queue.remove(0);
    }

    if npc_tagged > 0 {
        info!(
            "select_next_actor: tagged {} NPCs at time {}, queue_remaining={}",
            npc_tagged,
            turn_manager.current_time,
            turn_manager.turn_queue.len()
        );
        next_state.set(TurnState::Processing);
    } else if player_ready {
        // This case should be handled by the "if no NPCs tagged yet" block above,
        // but as a fallback:
        next_state.set(TurnState::PlayerInput);
    }
}

fn turn_queue_len(tm: &TurnManager) -> usize {
    tm.turn_queue.len()
}

/// BRIDGE: Triggers Monster AI
fn monster_ai_dispatch(world: &mut World) {
    let mut query = world.query_filtered::<Entity, (With<MonsterAI>, With<MyTurn>)>();
    let entities: Vec<Entity> = query.iter(world).collect();

    for entity in entities {
        if let Some(mut monster_ai) = world.entity_mut(entity).take::<MonsterAI>() {
            world.entity_mut(entity).insert(ActionGuard);
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
    for entity in query.iter() {
        finish_writer.write(ActionFinishedEvent {
            entity,
            base_cost: BASE_ACTION_COST,
            action_kind: ActionKind::Movement,
        });
        turn_end_writer.write(TurnEndEvent);
        commands.entity(entity).remove::<MyTurn>();
    }
}

/// Safety net: if any entity still has `ActionGuard` after all handlers ran,
/// it means a handler forgot to call `finish_turn()`. Emit a fallback event
/// so the turn loop doesn't stall, and log a warning to surface the bug.
fn action_guard_safety_net(
    mut commands: Commands,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    query: Query<Entity, With<ActionGuard>>,
) {
    for entity in query.iter() {
        warn!(
            "ActionGuard still present on {entity:?} — a handler forgot to call finish_turn(). Emitting fallback."
        );
        finish_turn(&mut commands, &mut finish_writer, entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// Handles `FreeActionEvent` — re-queues the entity at the *same* current time so
/// the turn is not consumed, then immediately returns to `PlayerInput` state.
/// Only ever emitted for the player; monsters always emit `ActionFinishedEvent`.
fn resolve_free_actions(
    mut events: MessageReader<FreeActionEvent>,
    mut turn_manager: ResMut<TurnManager>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    for event in events.read() {
        // Re-insert at current_time — no time penalty.
        let current_time = turn_manager.current_time;
        turn_manager.turn_queue.push((event.entity, current_time));
        turn_manager.turn_queue.sort_by_key(|&(_, t)| t);
        next_state.set(TurnState::PlayerInput);
    }
}

fn resolve_turn_end(
    mut commands: Commands,
    mut events: MessageReader<ActionFinishedEvent>,
    mut turn_manager: ResMut<TurnManager>,
    stats_query: Query<&SpeedStats>,
    speed_runic_query: Query<Entity, With<crate::game::enchantment::SpeedRunicProc>>,
    riposte_query: Query<Entity, With<crate::game::combat::RiposteReady>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let current_time = turn_manager.current_time;
    let mut any = false;
    for event in events.read() {
        // Dedup: don't add an entity that's already in the queue (can happen during
        // floor transitions when spawn_dungeon adds entities and resolve_turn_end
        // processes stale ActionFinishedEvents from the previous floor).
        if turn_manager
            .turn_queue
            .iter()
            .any(|(e, _)| *e == event.entity)
        {
            continue;
        }
        let stats = stats_query.get(event.entity).cloned().unwrap_or_default();
        let delay = stats.delay_for(event.action_kind);
        let mut cost = (event.base_cost as f32 * delay).round() as u32;

        if delay > 2.0 {
            info!(
                "High-delay entity {:?} — delay={}, base_cost={}, final_cost={}",
                event.entity, delay, event.base_cost, cost
            );
        }

        // Speed runic: override cost to 0 for a free turn
        if speed_runic_query.get(event.entity).is_ok() {
            cost = 0;
            commands
                .entity(event.entity)
                .remove::<crate::game::enchantment::SpeedRunicProc>();
        }

        // Riposte: override cost to 0 for a free melee counter-attack
        if riposte_query.get(event.entity).is_ok() {
            cost = 0;
            commands
                .entity(event.entity)
                .remove::<crate::game::combat::RiposteReady>();
            log_writer.write(GameLogMessage("Riposte!".to_string()));
        }

        turn_manager
            .turn_queue
            .push((event.entity, current_time + cost));
        any = true;
    }
    if any {
        turn_manager.turn_queue.sort_by_key(|&(_, t)| t);
    }
}

fn continue_turn_processing(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
    query_player: Query<Entity, With<Player>>,
    mut next_state: ResMut<NextState<TurnState>>,
) {
    // Check if we can immediately trigger another batch of NPCs who are ready "now"
    let mut npc_added = 0u32;
    const MAX_NPC_BATCH: u32 = 16;
    // Queue is already sorted by resolve_turn_end; no redundant sort needed.

    while !turn_manager.turn_queue.is_empty() {
        let (next_entity, next_time) = turn_manager.turn_queue[0];

        if next_time > turn_manager.current_time {
            break;
        }

        if query_player.get(next_entity).is_ok() {
            // If NPCs were already added this frame, we let them act first.
            // If not, we switch to player input.
            if npc_added == 0 {
                let (entity, _) = turn_manager.turn_queue.remove(0);
                commands.queue(move |world: &mut World| {
                    if let Ok(mut ec) = world.get_entity_mut(entity) {
                        ec.insert(MyTurn);
                    }
                });
                next_state.set(TurnState::PlayerInput);
                return;
            }
            break;
        }

        // Cap batch size to avoid lag spikes from many simultaneous NPC turns
        if npc_added >= MAX_NPC_BATCH {
            break;
        }

        let (entity, _) = turn_manager.turn_queue.remove(0);
        commands.queue(move |world: &mut World| {
            if let Ok(mut ec) = world.get_entity_mut(entity) {
                ec.insert(MyTurn);
            }
        });
        npc_added += 1;
    }

    if npc_added == 0 {
        next_state.set(TurnState::NextTurn);
    }
}

/// Pre-check: if the player is stunned, skip their input and go straight to Processing.
/// This keeps stun logic out of the turn system — it's a status effect concern.
/// If the player is stunned or entangled, returns `Some(message)`.
/// The caller (handle_player_input) should override any input to Wait.
fn player_status_override(effects: &StatusEffects) -> Option<&'static str> {
    if effects.is_entangled() {
        Some("You struggle against the cobwebs!")
    } else if effects.is_stunned() {
        Some("You are stunned and cannot act!")
    } else {
        None
    }
}

fn handle_player_input(
    time: Res<Time>,
    mut timer: ResMut<MovementTimer>,
    keys: Res<ButtonInput<KeyCode>>,
    mut pending: ResMut<PendingPlayerAction>,
    mut next_turn_state: ResMut<NextState<TurnState>>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    mut targeting_context: ResMut<TargetingContext>,
    player_effects: Query<&StatusEffects, With<Player>>,
    mut log_writer: MessageWriter<crate::ui::game_log::GameLogMessage>,
) {
    let mut action = None;

    // --- Held/repeated: movement (timer-gated so it auto-repeats while held) ---
    timer.0.tick(time.delta());
    if timer.0.is_finished() {
        if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
            action = Some(Action::Move { dir: Direction::N });
        } else if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            action = Some(Action::Move { dir: Direction::W });
        } else if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
            action = Some(Action::Move { dir: Direction::S });
        } else if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            action = Some(Action::Move { dir: Direction::E });
        }
    }

    // --- One-shot actions (just_pressed — never missed even on quick taps) ---
    if keys.just_pressed(KeyCode::Space) {
        action = Some(Action::Wait);
    }
    if keys.just_pressed(KeyCode::KeyG) {
        action = Some(Action::PickUp);
    }

    // F — fire ranged weapon (enters targeting mode).
    if keys.just_pressed(KeyCode::KeyF) {
        targeting_context.mode = TargetingMode::RangedAttack;
        next_ingame.set(InGameState::Targeting);
        // Do NOT transition to Processing — wait for targeting to complete.
    }

    // Z — zap a staff (opens staff selection screen).
    if keys.just_pressed(KeyCode::KeyZ) {
        next_ingame.set(InGameState::StaffSelect);
    }

    if let Some(act) = action {
        // Stunned/entangled: any input becomes a Wait turn with a message.
        // The player must press a key each turn — no auto-resolve cascade.
        if let Ok(effects) = player_effects.single() {
            if let Some(msg) = player_status_override(effects) {
                log_writer.write(crate::ui::game_log::GameLogMessage(msg.to_string()));
                pending.0 = Some(Action::Wait);
                next_turn_state.set(TurnState::Processing);
                return;
            }
        }

        info!(
            "TURN: player input → {:?}, transitioning to Processing",
            act
        );
        pending.0 = Some(act);
        next_turn_state.set(TurnState::Processing);
    }
}
