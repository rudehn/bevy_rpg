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
use crate::game::abilities::{
    handle_burning_strike, handle_poison_strike, handle_stunning_blow, handle_life_drain, handle_knockback,
    handle_slow_strike, handle_pack_tactics, handle_war_cry, handle_rough_body, handle_enrage,
    handle_split_on_hit, handle_explode_on_death, handle_summon_on_death,
    rally_aura_system, terrify_aura_system, mimic_reveal_system,
};
use crate::game::combat::CombatDamageSet;
use crate::game::effects::handle_use_item;
use crate::game::enchantment::{handle_weapon_runic_proc, handle_armor_runic_proc};
use crate::game::items::{handle_equip_item, handle_unequip_item, handle_drop_item};
use crate::game::machines::handle_machine_bump;
use crate::game::magic::StatusEffects;
use crate::game::ranged::handle_ranged_attack;
use crate::game::staves::handle_zap_staff;
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

    /// Insert an entity at a specific scheduled time, maintaining sorted order.
    #[allow(dead_code)]
    pub fn insert_at(&mut self, entity: Entity, time: u32) {
        self.turn_queue.push((entity, time));
        self.sort_queue();
    }

    /// Sort the turn queue by scheduled time (stable sort preserves insertion order for ties).
    #[allow(dead_code)]
    pub fn sort_queue(&mut self) {
        self.turn_queue.sort_by_key(|&(_, time)| time);
    }

    /// Peek at the next scheduled time without removing anything.
    /// Returns `None` if the queue is empty.
    #[allow(dead_code)]
    pub fn peek_time(&self) -> Option<u32> {
        self.turn_queue.first().map(|&(_, t)| t)
    }
}

/// Compute the re-insertion time for an entity after completing an action.
/// `base_cost` is the raw action cost, `delay` is the speed multiplier from `SpeedStats`.
/// Returns the scheduled time = `current_time + round(base_cost * delay)`.
pub fn compute_reinsert_time(current_time: u32, base_cost: u32, delay: f32) -> u32 {
    let cost = (base_cost as f32 * delay).round() as u32;
    current_time + cost
}

/// Outcome of a pure dequeue operation (no ECS side-effects).
#[derive(Debug, PartialEq, Eq)]
pub enum DequeueOutcome {
    /// The player entity is next to act.
    PlayerReady(Entity),
    /// A batch of NPC entities is ready to act.
    NpcBatch(Vec<Entity>),
    /// The queue is empty or no actors are scheduled at current_time.
    Empty,
}

/// Maximum number of NPCs that can act in a single batch before yielding.
pub const MAX_NPC_BATCH: u32 = 16;

/// Pure dequeue logic: determines which entities should act next without touching ECS.
/// `is_player` is a closure that returns `true` if an entity is the player.
/// Entities returned have been removed from `turn_manager.turn_queue`.
pub fn dequeue_next_batch_pure(
    turn_manager: &mut TurnManager,
    is_player: impl Fn(Entity) -> bool,
) -> DequeueOutcome {
    let mut npc_batch: Vec<Entity> = Vec::new();

    while !turn_manager.turn_queue.is_empty() {
        let (entity, time) = turn_manager.turn_queue[0];

        if time > turn_manager.current_time {
            break;
        }

        if is_player(entity) {
            if !npc_batch.is_empty() {
                break;
            }
            turn_manager.turn_queue.remove(0);
            return DequeueOutcome::PlayerReady(entity);
        }

        if npc_batch.len() as u32 >= MAX_NPC_BATCH {
            break;
        }

        turn_manager.turn_queue.remove(0);
        npc_batch.push(entity);
    }

    if !npc_batch.is_empty() {
        DequeueOutcome::NpcBatch(npc_batch)
    } else {
        DequeueOutcome::Empty
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

/// Central plugin for the turn processing pipeline.
///
/// ALL turn-phase handler registrations live here — this is the single source of truth
/// for what runs during each `ProcessingPhase`.
///
/// ## Turn Processing Pipeline
///
/// ```text
/// TurnState::Processing
/// ├── Brain (chained, sequential)
/// │   ├── populate_blocked_tiles
/// │   ├── squad_coordinator_system
/// │   ├── dispatch_player_action
/// │   ├── goap_ai_dispatch
/// │   ├── monster_ai_dispatch
/// │   └── marker_dispatch
/// ├── ResolveMovement
/// │   └── handle_movement
/// ├── ResolveActions (parallel)
/// │   ├── handle_melee
/// │   ├── handle_ranged_attack          (from ranged.rs)
/// │   ├── handle_door_open
/// │   ├── handle_unlock_door
/// │   ├── handle_open_chest
/// │   ├── handle_pickup
/// │   ├── handle_wait
/// │   ├── handle_use_item               (from effects.rs)
/// │   ├── handle_zap_staff              (from staves.rs)
/// │   ├── handle_equip_item             (from items.rs)
/// │   ├── handle_unequip_item           (from items.rs)
/// │   ├── handle_drop_item              (from items.rs)
/// │   ├── handle_machine_bump           (from machines.rs)
/// │   └── handle_drop_at_hoard          (from goap.rs)
/// ├── Combat Reactions (.after(CombatDamageSet))
/// │   ├── handle_weapon_runic_proc      (from enchantment.rs)
/// │   ├── handle_armor_runic_proc       (from enchantment.rs)
/// │   ├── handle_burning_strike         (from abilities.rs)
/// │   ├── handle_stunning_blow          (from abilities.rs)
/// │   ├── handle_life_drain             (from abilities.rs)
/// │   ├── handle_knockback              (from abilities.rs)
/// │   ├── handle_slow_strike            (from abilities.rs)
/// │   ├── handle_pack_tactics           (from abilities.rs)
/// │   ├── handle_war_cry                (from abilities.rs)
/// │   ├── handle_rough_body             (from abilities.rs)
/// │   ├── handle_enrage                 (from abilities.rs)
/// │   ├── handle_split_on_hit           (from abilities.rs)
/// │   ├── handle_explode_on_death       (from abilities.rs)
/// │   ├── handle_summon_on_death        (from abilities.rs)
/// │   ├── rally_aura_system             (from abilities.rs)
/// │   ├── terrify_aura_system           (from abilities.rs)
/// │   └── mimic_reveal_system           (from abilities.rs)
/// └── Cleanup (chained, sequential)
///     ├── action_guard_safety_net
///     ├── resolve_free_actions
///     ├── resolve_turn_end              → emits TurnEndEvent
///     ├── apply_dot_damage_system
///     ├── tick_status_durations_system
///     ├── fire_tick_system
///     ├── gas_tick_system
///     ├── tile_promotion_tick_system
///     ├── apply_tile_mutations
///     ├── apply_decoration_mutations
///     └── continue_turn_processing
/// ```
///
/// Domain plugins (RangedPlugin, EffectsPlugin, etc.) register only their
/// message types and resources. Handler system scheduling is centralized here.
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
            // --- Action handlers (ALL registered here — single source of truth) ---
            .add_systems(
                Update,
                (
                    // Core actions
                    handle_melee,
                    handle_ranged_attack,
                    handle_door_open,
                    handle_unlock_door,
                    handle_open_chest,
                    handle_pickup,
                    handle_wait,
                    // Item actions
                    handle_use_item,
                    handle_zap_staff,
                    handle_equip_item,
                    handle_unequip_item,
                    handle_drop_item,
                    // Environment actions
                    handle_machine_bump,
                    crate::game::goap::handle_drop_at_hoard,
                )
                    .in_set(ProcessingPhase::ResolveActions),
            )
            // --- Combat reaction handlers (run after damage is applied) ---
            .add_systems(
                Update,
                (
                    // Weapon/armor runic procs (from enchantment.rs)
                    handle_weapon_runic_proc,
                    handle_armor_runic_proc,
                    // On-hit ability triggers (from abilities.rs)
                    handle_burning_strike,
                    handle_poison_strike,
                    handle_stunning_blow,
                    handle_life_drain,
                    handle_knockback,
                    handle_slow_strike,
                    handle_pack_tactics,
                    handle_war_cry,
                    // On-being-hit ability triggers
                    handle_rough_body,
                    handle_enrage,
                    handle_split_on_hit,
                    // On-death triggers
                    handle_explode_on_death,
                    handle_summon_on_death,
                    // Aura systems (run on turn end)
                    rally_aura_system,
                    terrify_aura_system,
                    // Mimic reveal (run on turn end)
                    mimic_reveal_system,
                )
                    .after(CombatDamageSet)
                    .run_if(in_state(AppState::InGame)),
            )
            // --- Cleanup phase ---
            .add_systems(
                Update,
                (
                    // 1. Safety net + free actions
                    action_guard_safety_net,
                    resolve_free_actions,
                    // 2. End the turn → emits TurnEndEvent
                    resolve_turn_end,
                    // 3. Per-turn effects (read TurnEndEvent, write mutations)
                    crate::game::magic::apply_dot_damage_system,
                    crate::game::magic::tick_status_durations_system,
                    crate::game::fire::fire_tick_system,
                    crate::game::gas::gas_tick_system,
                    crate::game::tile_promotion::tile_promotion_tick_system,
                    // 4. Apply all queued mutations
                    crate::map::tile::apply_tile_mutations,
                    crate::map::tile::apply_decoration_mutations,
                    // 5. Advance to next actor
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

    // Sort and advance time to the next scheduled actor.
    turn_manager.turn_queue.sort_by_key(|&(_, time)| time);
    turn_manager.current_time = turn_manager.turn_queue[0].1;

    // Purge despawned entities before dequeuing.
    turn_manager.turn_queue.retain(|(e, _)| query_all.contains(*e));

    match dequeue_next_batch(&mut commands, &mut turn_manager, &query_player) {
        DequeueBatchResult::PlayerReady => {
            next_state.set(TurnState::PlayerInput);
        }
        DequeueBatchResult::NpcBatch(_) => {
            next_state.set(TurnState::Processing);
        }
        DequeueBatchResult::Empty => {}
    }
}

#[allow(dead_code)]
fn turn_queue_len(tm: &TurnManager) -> usize {
    tm.turn_queue.len()
}

/// Result of dequeuing the next batch from the turn queue.
enum DequeueBatchResult {
    /// Player is next — entity already has MyTurn.
    PlayerReady,
    /// One or more NPCs were tagged with MyTurn.
    #[allow(dead_code)]
    NpcBatch(u32),
    /// Queue is empty or no actors are ready at current_time.
    Empty,
}

/// Shared logic for dequeuing actors from the turn queue. Tags entities with
/// MyTurn and removes them from the queue. Delegates to `dequeue_next_batch_pure`
/// for the core logic, then applies ECS side-effects (inserting MyTurn components).
fn dequeue_next_batch(
    commands: &mut Commands,
    turn_manager: &mut TurnManager,
    player_query: &Query<Entity, With<Player>>,
) -> DequeueBatchResult {
    let outcome = dequeue_next_batch_pure(turn_manager, |e| player_query.get(e).is_ok());

    match outcome {
        DequeueOutcome::PlayerReady(entity) => {
            commands.queue(move |world: &mut World| {
                if let Ok(mut ec) = world.get_entity_mut(entity) {
                    ec.insert(MyTurn);
                }
            });
            DequeueBatchResult::PlayerReady
        }
        DequeueOutcome::NpcBatch(entities) => {
            let count = entities.len() as u32;
            for entity in entities {
                commands.queue(move |world: &mut World| {
                    if let Ok(mut ec) = world.get_entity_mut(entity) {
                        ec.insert(MyTurn);
                    }
                });
            }
            DequeueBatchResult::NpcBatch(count)
        }
        DequeueOutcome::Empty => DequeueBatchResult::Empty,
    }
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
        let reinsert_time = compute_reinsert_time(current_time, event.base_cost, delay);
        let mut cost = reinsert_time - current_time;

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
    match dequeue_next_batch(&mut commands, &mut turn_manager, &query_player) {
        DequeueBatchResult::PlayerReady => {
            next_state.set(TurnState::PlayerInput);
        }
        DequeueBatchResult::NpcBatch(_) => {
            // Stay in Processing — NPCs will act this frame.
        }
        DequeueBatchResult::Empty => {
            next_state.set(TurnState::NextTurn);
        }
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

    // Staff usage is now handled via Inventory → U on a staff item.

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::actions::{ActionKind, SpeedStats};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn entity(n: u32) -> Entity {
        Entity::from_raw_u32(n).expect("valid test entity index")
    }

    /// Build a TurnManager with a given current_time and pre-sorted queue entries.
    fn make_tm(current_time: u32, entries: &[(Entity, u32)]) -> TurnManager {
        let mut tm = TurnManager {
            turn_queue: entries.to_vec(),
            current_time,
        };
        tm.sort_queue();
        tm
    }

    // -----------------------------------------------------------------------
    // TurnManager basic operations
    // -----------------------------------------------------------------------

    #[test]
    fn add_entity_inserts_at_current_time() {
        let mut tm = TurnManager::default();
        tm.current_time = 50;
        tm.add_entity(entity(1));
        assert_eq!(tm.turn_queue, vec![(entity(1), 50)]);
    }

    #[test]
    fn insert_at_maintains_sorted_order() {
        let mut tm = TurnManager::default();
        tm.insert_at(entity(1), 300);
        tm.insert_at(entity(2), 100);
        tm.insert_at(entity(3), 200);

        let times: Vec<u32> = tm.turn_queue.iter().map(|&(_, t)| t).collect();
        assert_eq!(times, vec![100, 200, 300]);
    }

    #[test]
    fn sort_queue_stable_for_equal_times() {
        // Entities inserted in order at the same time should keep that order after sort.
        let mut tm = TurnManager::default();
        tm.turn_queue.push((entity(1), 100));
        tm.turn_queue.push((entity(2), 100));
        tm.turn_queue.push((entity(3), 100));
        tm.sort_queue();

        let entities: Vec<Entity> = tm.turn_queue.iter().map(|&(e, _)| e).collect();
        assert_eq!(entities, vec![entity(1), entity(2), entity(3)]);
    }

    #[test]
    fn peek_time_returns_lowest() {
        let tm = make_tm(0, &[(entity(1), 50), (entity(2), 100)]);
        assert_eq!(tm.peek_time(), Some(50));
    }

    #[test]
    fn peek_time_empty_returns_none() {
        let tm = TurnManager::default();
        assert_eq!(tm.peek_time(), None);
    }

    // -----------------------------------------------------------------------
    // compute_reinsert_time
    // -----------------------------------------------------------------------

    #[test]
    fn reinsert_time_default_speed() {
        // delay=1.0, base_cost=100 → reinsert at current_time+100
        assert_eq!(compute_reinsert_time(0, 100, 1.0), 100);
    }

    #[test]
    fn reinsert_time_slow_entity() {
        // delay=1.5, base_cost=100 → 150
        assert_eq!(compute_reinsert_time(0, 100, 1.5), 150);
    }

    #[test]
    fn reinsert_time_fast_entity() {
        // delay=0.5, base_cost=100 → 50
        assert_eq!(compute_reinsert_time(0, 100, 0.5), 50);
    }

    #[test]
    fn reinsert_time_rounds_correctly() {
        // delay=0.33, base_cost=100 → 33.0 → 33
        assert_eq!(compute_reinsert_time(0, 100, 0.33), 33);
        // delay=0.335, base_cost=100 → 33.5 → 34 (rounds up at .5)
        assert_eq!(compute_reinsert_time(0, 100, 0.335), 34);
    }

    #[test]
    fn reinsert_time_with_nonzero_current_time() {
        assert_eq!(compute_reinsert_time(500, 100, 1.0), 600);
        assert_eq!(compute_reinsert_time(500, 100, 2.0), 700);
    }

    #[test]
    fn reinsert_time_zero_cost_free_action() {
        // base_cost=0 means no time passes regardless of delay
        assert_eq!(compute_reinsert_time(100, 0, 1.0), 100);
        assert_eq!(compute_reinsert_time(100, 0, 3.0), 100);
    }

    // -----------------------------------------------------------------------
    // SpeedStats::delay_for (verifying the mapping used in resolve_turn_end)
    // -----------------------------------------------------------------------

    #[test]
    fn speed_stats_delay_for_movement() {
        let stats = SpeedStats::new(0.8, 1.2);
        assert_eq!(stats.delay_for(ActionKind::Movement), 0.8);
    }

    #[test]
    fn speed_stats_delay_for_attack() {
        let stats = SpeedStats::new(0.8, 1.2);
        assert_eq!(stats.delay_for(ActionKind::Attack), 1.2);
    }

    #[test]
    fn speed_stats_default_is_one() {
        let stats = SpeedStats::default();
        assert_eq!(stats.delay_for(ActionKind::Movement), 1.0);
        assert_eq!(stats.delay_for(ActionKind::Attack), 1.0);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — empty queue
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_empty_queue_returns_empty() {
        let mut tm = TurnManager::default();
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::Empty);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — single entity
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_single_npc() {
        let npc = entity(1);
        let mut tm = make_tm(0, &[(npc, 0)]);
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));
        assert!(tm.turn_queue.is_empty());
    }

    #[test]
    fn dequeue_single_player() {
        let player = entity(1);
        let mut tm = make_tm(0, &[(player, 0)]);
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
        assert!(tm.turn_queue.is_empty());
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — player comes first
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_player_first_returns_player_ready() {
        let player = entity(1);
        let npc_a = entity(2);
        let mut tm = make_tm(100, &[(player, 100), (npc_a, 100)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
        // NPC should remain in queue
        assert_eq!(tm.turn_queue.len(), 1);
        assert_eq!(tm.turn_queue[0].0, npc_a);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — NPCs batch before player
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_npcs_batch_before_player_at_same_time() {
        let npc_a = entity(1);
        let npc_b = entity(2);
        let player = entity(3);

        // NPCs are in queue before the player (insertion order), all at time 0.
        let mut tm = make_tm(0, &[(npc_a, 0), (npc_b, 0), (player, 0)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));
        // Player is still in the queue, not yet dequeued.
        assert_eq!(tm.turn_queue.len(), 1);
        assert_eq!(tm.turn_queue[0].0, player);
    }

    #[test]
    fn dequeue_npcs_before_player_different_times() {
        let npc = entity(1);
        let player = entity(2);

        // NPC at time 50, player at time 100, current_time=100.
        // NPC's time < current_time, so it acts. Player at exactly current_time.
        let mut tm = make_tm(100, &[(npc, 50), (player, 100)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));
        // Player deferred to next dequeue cycle.
        assert_eq!(tm.turn_queue.len(), 1);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — NPC batch limit (MAX_NPC_BATCH)
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_respects_max_npc_batch() {
        let mut entries: Vec<(Entity, u32)> = Vec::new();
        // 20 NPCs all at time 0
        for i in 0..20 {
            entries.push((entity(i), 0));
        }
        let mut tm = make_tm(0, &entries);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        match result {
            DequeueOutcome::NpcBatch(batch) => {
                assert_eq!(batch.len(), MAX_NPC_BATCH as usize);
            }
            other => panic!("Expected NpcBatch, got {:?}", other),
        }
        // Remaining 4 NPCs still in queue
        assert_eq!(tm.turn_queue.len(), 4);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — future entities not dequeued
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_skips_future_entities() {
        let npc = entity(1);
        let mut tm = make_tm(0, &[(npc, 100)]);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::Empty);
        // Entity remains in queue
        assert_eq!(tm.turn_queue.len(), 1);
    }

    #[test]
    fn dequeue_takes_ready_entities_leaves_future() {
        let npc_ready = entity(1);
        let npc_future = entity(2);
        let mut tm = make_tm(100, &[(npc_ready, 100), (npc_future, 200)]);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc_ready]));
        assert_eq!(tm.turn_queue.len(), 1);
        assert_eq!(tm.turn_queue[0], (npc_future, 200));
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure — entities at or below current_time all dequeue
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_entities_at_and_below_current_time() {
        let npc_a = entity(1);
        let npc_b = entity(2);
        let npc_c = entity(3);
        // current_time=100. Entity at 50, 100, 150.
        let mut tm = make_tm(100, &[(npc_a, 50), (npc_b, 100), (npc_c, 150)]);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));
        assert_eq!(tm.turn_queue.len(), 1);
        assert_eq!(tm.turn_queue[0], (npc_c, 150));
    }

    // -----------------------------------------------------------------------
    // Full turn cycle simulation
    // -----------------------------------------------------------------------

    #[test]
    fn full_turn_cycle_player_and_npc() {
        let player = entity(1);
        let npc = entity(2);

        // Both start at time 0.
        let mut tm = make_tm(0, &[(player, 0), (npc, 0)]);

        // Advance time to the first scheduled actor.
        tm.current_time = tm.peek_time().unwrap();

        // Player is first (lower entity id, same time).
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));

        // Simulate player action: re-insert at current_time + 100 (base cost, delay=1.0)
        let player_reinsert = compute_reinsert_time(tm.current_time, 100, 1.0);
        tm.insert_at(player, player_reinsert);

        // Now dequeue again — NPC should be next.
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));

        // Simulate NPC action: re-insert with delay=1.5 (slow monster)
        let npc_reinsert = compute_reinsert_time(tm.current_time, 100, 1.5);
        tm.insert_at(npc, npc_reinsert);

        // Advance to next cycle.
        tm.sort_queue();
        tm.current_time = tm.peek_time().unwrap();

        // Player at 100, NPC at 150 → player goes first.
        assert_eq!(tm.current_time, 100);
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
    }

    #[test]
    fn fast_entity_acts_more_often() {
        let fast = entity(1);
        let slow = entity(2);

        let mut tm = make_tm(0, &[(fast, 0), (slow, 0)]);
        tm.current_time = 0;

        // Dequeue both NPCs.
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![fast, slow]));

        // Re-insert: fast at delay=0.5 (cost=50), slow at delay=2.0 (cost=200).
        tm.insert_at(fast, compute_reinsert_time(0, 100, 0.5));
        tm.insert_at(slow, compute_reinsert_time(0, 100, 2.0));

        // Advance to next.
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 50);

        // Only fast is ready.
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![fast]));

        // Re-insert fast again.
        tm.insert_at(fast, compute_reinsert_time(50, 100, 0.5));

        // Advance.
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 100);

        // Only fast again (slow at 200).
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![fast]));
    }

    // -----------------------------------------------------------------------
    // Free action: re-insertion at same time
    // -----------------------------------------------------------------------

    #[test]
    fn free_action_reinserts_at_current_time() {
        let player = entity(1);
        let npc = entity(2);

        let mut tm = make_tm(100, &[(npc, 200)]);

        // Simulate a free action: player goes back in at current_time.
        tm.insert_at(player, tm.current_time);

        assert_eq!(tm.turn_queue[0], (player, 100));
        // Player acts before the NPC at 200.
        tm.current_time = tm.peek_time().unwrap();
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
    }

    // -----------------------------------------------------------------------
    // Dedup: entity already in queue should not be double-inserted
    // (mirrors the check in resolve_turn_end)
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_prevents_double_insertion() {
        let ent = entity(1);
        let mut tm = make_tm(0, &[(ent, 100)]);

        // Simulate the dedup check from resolve_turn_end.
        let already_present = tm.turn_queue.iter().any(|(e, _)| *e == ent);
        assert!(already_present);

        // Should NOT insert again.
        if !already_present {
            tm.insert_at(ent, 200);
        }
        assert_eq!(tm.turn_queue.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Edge: identical scheduled times — stable ordering preserved
    // -----------------------------------------------------------------------

    #[test]
    fn stable_order_with_identical_times() {
        let a = entity(10);
        let b = entity(20);
        let c = entity(30);

        let mut tm = TurnManager {
            turn_queue: vec![(a, 100), (b, 100), (c, 100)],
            current_time: 100,
        };
        // sort_by_key is stable — original order preserved for equal keys.
        tm.sort_queue();

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![a, b, c]));
    }

    // -----------------------------------------------------------------------
    // Edge: player scheduled after NPCs — NPCs batch, player waits
    // -----------------------------------------------------------------------

    #[test]
    fn player_scheduled_later_waits() {
        let player = entity(1);
        let npc = entity(2);

        let mut tm = make_tm(100, &[(npc, 100), (player, 200)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));
        // Player still in queue at future time.
        assert_eq!(tm.turn_queue.len(), 1);
        assert_eq!(tm.turn_queue[0], (player, 200));
    }

    // -----------------------------------------------------------------------
    // Mixed scenario: multiple dequeue rounds
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_dequeue_rounds() {
        let player = entity(1);
        let npc_a = entity(2);
        let npc_b = entity(3);

        // NPCs at 0, player at 50.
        let mut tm = make_tm(0, &[(npc_a, 0), (npc_b, 0), (player, 50)]);

        // Round 1: NPCs batch.
        let r1 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r1, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));

        // Re-insert NPCs at time 100.
        tm.insert_at(npc_a, 100);
        tm.insert_at(npc_b, 100);

        // Round 2: nothing at current_time=0 anymore, empty.
        let r2 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r2, DequeueOutcome::Empty);

        // Advance time to next actor (player at 50).
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 50);

        // Round 3: player is ready.
        let r3 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r3, DequeueOutcome::PlayerReady(player));

        // Re-insert player at 150.
        tm.insert_at(player, 150);

        // Advance.
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 100);

        // Round 4: both NPCs batch.
        let r4 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r4, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));
    }
}
