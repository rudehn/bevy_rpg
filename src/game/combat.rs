use bevy::prelude::*;
use bracket_lib::random::{DiceType, RandomNumberGenerator, parse_dice_string};

use crate::components::{Monster, Name}; // Import Monster marker
use crate::game::{AppState, TurnManager};
use crate::game::turns::TurnEndEvent;
use crate::player::Player; // Import Player marker // Import AppState for game over
use crate::ui::game_log::GameLogMessage;

// --- Components ---

/// Component for an entity's current and maximum health.
#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

/// Component for health regeneration.
/// regen_rate: points gained per turn (e.g., 20 for 1 health per 5 turns)
/// regen_accumulator: accumulated points
#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct HealthRegen {
    pub regen_rate: i32,
    pub regen_accumulator: i32,
}

/// Component for an entity's damage, using dice notation (e.g., "1d6").
#[derive(Component, Debug)]
pub struct Damage(pub String);

// --- Messages ---

/// Message sent when an entity hits another entity.
#[derive(Message, Debug)] // Changed from Event to Message
pub struct HitEvent {
    pub attacker: Entity,
    pub target: Entity,
}

// --- Resources ---

/// Wrapper for bracket_lib's RandomNumberGenerator to be used as a Bevy Resource.
#[derive(Resource)]
pub struct GameRng(RandomNumberGenerator);

// --- Utility Functions ---

/// Rolls dice based on a dice notation string (e.g., "1d6").
fn roll_dice(dice_string: &str, rng: &mut RandomNumberGenerator) -> i32 {
    match parse_dice_string(dice_string) {
        Ok(dice_type) => rng.roll_dice(dice_type.n_dice, dice_type.die_type) + dice_type.bonus,
        Err(e) => {
            error!("Failed to parse dice string '{}': {}", dice_string, e);
            1 // Default to 1 damage on parse error
        }
    }
}

// --- Systems ---

/// System that handles health regeneration at the end of a global turn cycle.
fn regen_system(
    mut turn_end_events: MessageReader<TurnEndEvent>,
    mut query: Query<(&mut Health, &mut HealthRegen)>,
) {
    for _ in turn_end_events.read() {
        for (mut health, mut regen) in query.iter_mut() {
            if health.current < health.max {
                regen.regen_accumulator += regen.regen_rate;
                while regen.regen_accumulator >= 100 {
                    health.current = (health.current + 1).min(health.max);
                    regen.regen_accumulator -= 100;
                }
            } else {
                // If health is full, we cap the accumulator at 100 to prevent 
                // massive "burst" healing immediately after taking damage.
                regen.regen_accumulator = regen.regen_accumulator.min(100);
            }
        }
    }
}

/// System that processes HitEvents, calculates damage, and updates target health.
fn combat_system(
    mut hit_events: MessageReader<HitEvent>, // Changed from EventReader to MessageReader
    query_attackers: Query<(&Damage, &Name)>,
    mut query_targets: Query<(&mut Health, &Name)>,
    mut game_rng: ResMut<GameRng>, // Use GameRng resource
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in hit_events.read() {
        let (attacker_damage, attacker_name) = match query_attackers.get(event.attacker) {
            Ok(data) => data,
            Err(_) => {
                warn!("Attacker {:?} has no Damage or Name component.", event.attacker);
                continue;
            }
        };

        let (mut target_health, target_name) = match query_targets.get_mut(event.target) {
            Ok(data) => data,
            Err(_) => {
                warn!("Target {:?} has no Health or Name component.", event.target);
                continue;
            }
        };

        let rolled_damage = roll_dice(&attacker_damage.0, &mut game_rng.0); // Pass inner RNG
        target_health.current -= rolled_damage;

        let message = format!(
            "{} hits {} for {} damage.",
            attacker_name.0, target_name.0, rolled_damage
        );
        log_writer.write(GameLogMessage(message));

        info!(
            "Entity {:?} hit Entity {:?} for {} damage. Target health: {}/{}",
            event.attacker, event.target, rolled_damage, target_health.current, target_health.max
        );
    }
}

/// System that checks for entities with Health <= 0 and handles death.
fn death_system(
    mut commands: Commands,
    query_dead: Query<(Entity, &Health, &Name, Option<&Player>, Option<&Monster>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_manager: ResMut<TurnManager>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for (entity, health, name, is_player, is_monster) in query_dead.iter() {
        if health.current <= 0 {
            if is_player.is_some() {
                // Player died
                eprintln!("Game Over! You died!");
                log_writer.write(GameLogMessage("You have died!".to_string()));
                next_state.set(AppState::GameOver); // Transition to GameOver state
            } else if is_monster.is_some() {
                // Monster died
                info!("Monster {:?} died!", entity);
                log_writer.write(GameLogMessage(format!("{} dies.", name.0)));
                commands.entity(entity).despawn();
                // Remove from turn queue if present
                turn_manager.turn_queue.retain(|&(e, _)| e != entity);

                if turn_manager.acting_entity == Some(entity) {
                    turn_manager.acting_entity = None;
                }
            }
        }
    }
}

// --- Plugin ---

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameRng(RandomNumberGenerator::new())) // Initialize wrapped RNG
            .add_message::<HitEvent>() // Changed from add_event to add_message
            .register_type::<Health>()
            .register_type::<HealthRegen>()
            .add_systems(
                Update,
                (combat_system, regen_system, death_system).run_if(in_state(AppState::InGame)),
            );
    }
}
