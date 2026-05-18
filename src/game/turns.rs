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
use crate::game::combat::CombatDamageSet;
use crate::game::magic::{GameStatusEffectsExt, StatusEffects};
use crate::game::targeting::{TargetingContext, TargetingMode};
use crate::map::map::populate_blocked_tiles;
use crate::player::{MovementTimer, Player};
use crate::ui::game_log::GameLogMessage;

#[derive(Component)]
pub struct TurnMarker;

// `TurnEndEvent` lives in the engine (`roguelike_engine::turn`) so engine
// systems (tile promotion, future sims) can listen for it. Re-exported
// here so existing `crate::game::turns::TurnEndEvent` sites compile.
pub use roguelike_engine::turn::TurnEndEvent;

/// Marker component indicating it is currently this entity's turn.
/// Execution systems or AI systems look for this to know when to act.
#[derive(Component)]
pub struct MyTurn;

// Pure turn scheduling primitives live in the engine crate. They are
// re-exported here so existing game code can continue to use
// `crate::game::turns::{TurnManager, DequeueOutcome, ...}` unchanged.
pub use roguelike_engine::turn::{
    compute_reinsert_time, dequeue_next_batch_pure, DequeueOutcome, TurnManager, MAX_NPC_BATCH,
};

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

/// System set for handlers that must run after the combat damage pipeline has
/// applied damage. Use this for on-hit, on-being-hit, and on-death reactions
/// (runic procs, bleed, explode-on-death, auras, etc.).
///
/// Configured by `TurnOrderPlugin` to run `.after(CombatDamageSet)` and gated
/// on `AppState::InGame`. Game-content plugins (AbilitiesPlugin,
/// EnchantmentPlugin, etc.) register their systems via `.in_set(CombatReactionSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CombatReactionSet;

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
/// │   ├── handle_gas_on_death          (from abilities.rs)
/// │   ├── handle_summoner_death        (from abilities.rs)
/// │   ├── rally_aura_system             (from abilities.rs)
/// │   ├── terrify_aura_system           (from abilities.rs)
/// │   └── mimic_reveal_system           (from abilities.rs)
/// └── Cleanup (chained, sequential)
///     ├── action_guard_safety_net
///     ├── resolve_free_actions
///     ├── resolve_turn_end              → emits TurnEndEvent
///     ├── status_effect_tick_system       (from engine StatusEffectSet)
///     ├── status_expiry_log_system
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
            // Tile mutation messages + apply systems live in the engine
            // (MapMutationPlugin). Tile promotion lives in the engine
            // (TilePromotionPlugin). Both are added below.
            .add_plugins((
                roguelike_engine::map::MapMutationPlugin,
                roguelike_engine::map::TilePromotionPlugin,
            ))
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
            // Engine status-effect tick runs inside the Cleanup phase,
            // after resolve_turn_end and before the game's expiry log.
            .configure_sets(
                Update,
                crate::game::magic::StatusEffectSet
                    .in_set(ProcessingPhase::Cleanup)
                    .after(resolve_turn_end)
                    .before(crate::game::magic::status_expiry_log_system),
            )
            // Combat reactions run after the damage pipeline in the main frame.
            .configure_sets(
                Update,
                CombatReactionSet
                    .after(CombatDamageSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnEnter(AppState::InGame), (setup_turn_order, start_turns))
            .add_systems(
                Update,
                (
                    select_next_actor
                        .run_if(in_state(TurnState::NextTurn))
                        .after(roguelike_engine::components::FovSet),
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
                    // Update MonsterAI.mode for every (MonsterAI, MyTurn)
                    // entity before any dispatcher reads it. Replaces the
                    // mode-update logic that lived inside execute_monster_ai.
                    crate::game::ai::refresh_monster_modes_system,
                    crate::game::tactics::dispatch::tactic_dispatch_system,
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
            // --- Engine-side action handlers (core action set only) ---
            // Domain plugins (AbilitiesPlugin, EnchantmentPlugin, ItemsPlugin,
            // EffectsPlugin, StavesPlugin, MachinesPlugin, RangedPlugin, GoapPlugin)
            // register their own handlers in `ProcessingPhase::ResolveActions`
            // and `CombatReactionSet`.
            .add_systems(
                Update,
                (
                    handle_melee,
                    handle_door_open,
                    handle_unlock_door,
                    handle_open_chest,
                    handle_pickup,
                    handle_wait,
                )
                    .in_set(ProcessingPhase::ResolveActions),
            )
            // --- Cleanup phase ---
            //
            // Phase order (enforced via SystemSet config + .chain()):
            //   1. action_guard_safety_net → resolve_free_actions
            //   2. resolve_turn_end (emits TurnEndEvent)
            //   3. game-side per-turn ticks emit mutation messages
            //      (status_expiry_log, fire, gas)
            //   4. TilePromotionSet (engine) emits mutation messages
            //   5. MapMutationSet (engine) applies them to Map + tile entities
            //   6. chasm_fall_reaction_system reads LiquidMutationMessage
            //      and runs game-specific consequences (fall, lava kill)
            //   7. continue_turn_processing
            .configure_sets(
                Update,
                roguelike_engine::map::TilePromotionSet
                    .in_set(ProcessingPhase::Cleanup),
            )
            .configure_sets(
                Update,
                roguelike_engine::map::MapMutationSet
                    .in_set(ProcessingPhase::Cleanup)
                    .after(roguelike_engine::map::TilePromotionSet),
            )
            .add_systems(
                Update,
                (
                    action_guard_safety_net,
                    resolve_free_actions,
                    resolve_turn_end,
                    crate::game::magic::status_expiry_log_system,
                    crate::game::fire::fire_tick_system,
                    crate::game::gas::gas_tick_system,
                )
                    .chain()
                    .in_set(ProcessingPhase::Cleanup)
                    .before(roguelike_engine::map::TilePromotionSet),
            )
            .add_systems(
                Update,
                (
                    crate::map::tile::chasm_fall_reaction_system,
                    continue_turn_processing,
                )
                    .chain()
                    .in_set(ProcessingPhase::Cleanup)
                    .after(roguelike_engine::map::MapMutationSet),
            );
    }
}

fn start_turns(mut next_state: ResMut<NextState<TurnState>>) {
    next_state.set(TurnState::NextTurn);
}

fn setup_turn_order(mut commands: Commands, mut turn_manager: ResMut<TurnManager>) {
    let turn_marker_entity = commands.spawn((TurnMarker, GameEntityMarker)).id();
    *turn_manager = TurnManager::default();
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
    if turn_manager.is_empty() {
        return;
    }

    // Advance time to the next scheduled actor (BinaryHeap is always sorted).
    turn_manager.current_time = turn_manager.peek_time().unwrap();

    // Despawned entities are removed from the queue by death_system and
    // other cleanup systems via remove_entity(). The dequeue_next_batch
    // helper also safely handles stale entities (get_entity_mut check).

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
    tm.len()
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

// `monster_ai_dispatch` was deleted in Phase 4d. The FSM path it
// triggered (`crate::game::ai::execute_monster_ai`) is gone; every
// monster now uses either `goap_ai_dispatch` or
// `tactic_dispatch_system`, both of which read `MonsterAI.mode` that
// `refresh_monster_modes_system` keeps up to date.

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
        turn_manager.insert_at(event.entity, current_time);
        next_state.set(TurnState::PlayerInput);
    }
}

fn resolve_turn_end(
    mut commands: Commands,
    mut events: MessageReader<ActionFinishedEvent>,
    mut turn_manager: ResMut<TurnManager>,
    stats_query: Query<&SpeedStats>,
    speed_runic_query: Query<Entity, With<crate::game::enchantment::SpeedRunicProc>>,
) {
    let current_time = turn_manager.current_time;
    for event in events.read() {
        // Dedup: don't add an entity that's already in the queue (can happen during
        // floor transitions when spawn_dungeon adds entities and resolve_turn_end
        // processes stale ActionFinishedEvents from the previous floor).
        if turn_manager.contains(event.entity) {
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

        turn_manager.insert_at(event.entity, current_time + cost);
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
    use crate::game::actions::{ActionKind, SpeedStats};

    // Pure turn-manager / dequeue / reinsert-time tests now live in
    // `roguelike_engine::turn::tests`. Only game-side tests (that exercise
    // `SpeedStats`, which is a game type) remain here.

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
}
