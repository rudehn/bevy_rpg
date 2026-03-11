use bevy::prelude::*;

use crate::assets::SpellRegistryHandle;
use crate::components::GameEntityMarker;
use crate::constants::BASE_ACTION_COST;
use crate::game::AppState;
use crate::game::actions::{
    Action, ActionFinishedEvent, Direction, FreeActionEvent, MeleeIntent, MovementIntent,
    OpenDoorIntent, PendingPlayerAction, PickUpIntent, RangedAttackIntent, SpeedStats, WaitIntent,
    dispatch_player_action,
    handle_door_open, handle_melee, handle_movement, handle_pickup, handle_wait,
};
use crate::game::ai::MonsterAI;
use crate::game::effects::handle_use_item;
use crate::game::magic::{ActiveSpells, handle_cast_spell};
use crate::game::ranged::handle_ranged_attack;
use crate::game::targeting::TargetingMode;
use crate::game::items::{handle_drop_item, handle_equip_item, handle_unequip_item};
use crate::game::spells::{SpellRegistry, SpellTarget};
use crate::game::targeting::TargetingContext;
use crate::game::InGameState;
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
            .init_resource::<PendingPlayerAction>()
            // Intent messages used by the Processing chain's handler systems.
            .add_message::<MovementIntent>()
            .add_message::<MeleeIntent>()
            .add_message::<WaitIntent>()
            .add_message::<PickUpIntent>()
            .add_message::<OpenDoorIntent>()
            .add_message::<RangedAttackIntent>()
            // Turn-lifecycle messages.
            .add_message::<ActionFinishedEvent>()
            .add_message::<FreeActionEvent>()
            .add_message::<TurnEndEvent>()
            .add_systems(OnEnter(AppState::InGame), (setup_turn_order, start_turns))
            .add_systems(
                Update,
                (
                    select_next_actor.run_if(in_state(TurnState::NextTurn)),
                    // Only accept movement input when no UI screen is open.
                    handle_player_input.run_if(
                        in_state(TurnState::PlayerInput).and(in_state(InGameState::Running))
                    ),
                    (
                        // --- Brain Systems ---
                        dispatch_player_action,
                        monster_ai_dispatch,
                        marker_dispatch,
                        // --- Execution Systems ---
                        handle_movement,
                        handle_melee,
                        handle_ranged_attack,
                        handle_door_open,
                        handle_pickup,
                        handle_wait,
                        handle_equip_item,
                        handle_unequip_item,
                        handle_drop_item,
                        handle_use_item,
                        handle_cast_spell,
                        // --- Cleanup ---
                        resolve_free_actions,
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
    let mut npc_tagged = false;

    // Identify all actors ready at this time slice
    // We look at the queue and tag everyone whose time is <= current_time
    // But we MUST stop if we hit the player to gather input.

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
            if npc_tagged {
                break;
            } else {
                // If no NPCs were tagged yet, the player is the very first one ready.
                // We'll tag them and go to input.
                commands.queue(move |world: &mut World| {
                    if let Ok(mut ec) = world.get_entity_mut(entity) {
                        ec.insert(MyTurn);
                    }
                });
                next_state.set(TurnState::PlayerInput);
                return;
            }
        } else {
            // It's an NPC or Marker
            commands.queue(move |world: &mut World| {
                if let Ok(mut ec) = world.get_entity_mut(entity) {
                    ec.insert(MyTurn);
                }
            });
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

fn turn_queue_len(tm: &TurnManager) -> usize {
    tm.turn_queue.len()
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
    for entity in query.iter() {
        finish_writer.write(ActionFinishedEvent {
            entity: entity,
            base_cost: BASE_ACTION_COST,
        });
        turn_end_writer.write(TurnEndEvent);
        commands.entity(entity).remove::<MyTurn>();
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
    mut events: MessageReader<ActionFinishedEvent>,
    mut turn_manager: ResMut<TurnManager>,
    stats_query: Query<&SpeedStats>,
) {
    let current_time = turn_manager.current_time;
    let mut any = false;
    for event in events.read() {
        let stats = stats_query.get(event.entity).cloned().unwrap_or_default();
        let cost = (event.base_cost as f32 * stats.delay).round() as u32;
        turn_manager.turn_queue.push((event.entity, current_time + cost));
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
    let mut npc_added = false;
    // Queue is already sorted by resolve_turn_end; no redundant sort needed.

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

        let (entity, _) = turn_manager.turn_queue.remove(0);
        commands.queue(move |world: &mut World| {
            if let Ok(mut ec) = world.get_entity_mut(entity) {
                ec.insert(MyTurn);
            }
        });
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
    mut pending: ResMut<PendingPlayerAction>,
    mut next_turn_state: ResMut<NextState<TurnState>>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    mut targeting_context: ResMut<TargetingContext>,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
    player_active_spells: Query<&ActiveSpells, With<Player>>,
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

    // Spell slots 1–6.
    let spell_keys = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
        KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
    ];
    for (i, &key) in spell_keys.iter().enumerate() {
        if keys.just_pressed(key) {
            let needs_targeting = player_active_spells.single().ok().and_then(|active| {
                let spell_id = active.slots.get(i)?.as_deref()?;
                let registry = spell_registries.get(&spell_registry_handle.0)?;
                let spell = registry.spells.get(spell_id)?;
                Some(spell.target == SpellTarget::NearestEnemy)
            }).unwrap_or(false);

            if needs_targeting {
                targeting_context.mode = TargetingMode::Spell { slot: i };
                next_ingame.set(InGameState::Targeting);
            } else {
                action = Some(Action::CastSpell { slot: i, target: None });
            }
            break;
        }
    }

    if let Some(act) = action {
        pending.0 = Some(act);
        next_turn_state.set(TurnState::Processing);
    }
}
