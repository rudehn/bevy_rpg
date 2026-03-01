use bevy::prelude::*;
use bracket_lib::random::{DiceType, RandomNumberGenerator, parse_dice_string};

use crate::components::{Monster, Name}; // Import Monster marker
use crate::game::{AppState, TurnManager};
use crate::player::Player; // Import Player marker // Import AppState for game over
use crate::ui::game_log::GameLogMessage;

// --- Components ---

/// Component for an entity's current and maximum health.
#[derive(Component, Debug)]
pub struct Health {
    pub current: i32,
    pub max: i32,
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
            .add_systems(Update, combat_system.run_if(in_state(AppState::InGame))) // Individual systems with run_if
            .add_systems(Update, death_system.run_if(in_state(AppState::InGame)));
    }
}
