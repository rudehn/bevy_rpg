use bevy::prelude::*;
use bracket_lib::random::{DiceType, RandomNumberGenerator, parse_dice_string};

use crate::components::Monster; // Import Monster marker
use crate::game::{AppState, TurnManager};
use crate::player::Player; // Import Player marker // Import AppState for game over

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
    query_attackers: Query<&Damage>,
    mut query_targets: Query<&mut Health>,
    mut game_rng: ResMut<GameRng>, // Use GameRng resource
) {
    for event in hit_events.read() {
        let attacker_damage = match query_attackers.get(event.attacker) {
            Ok(damage) => damage,
            Err(_) => {
                warn!("Attacker {:?} has no Damage component.", event.attacker);
                continue;
            }
        };

        let mut target_health = match query_targets.get_mut(event.target) {
            Ok(health) => health,
            Err(_) => {
                warn!("Target {:?} has no Health component.", event.target);
                continue;
            }
        };

        let rolled_damage = roll_dice(&attacker_damage.0, &mut game_rng.0); // Pass inner RNG
        target_health.current -= rolled_damage;

        info!(
            "Entity {:?} hit Entity {:?} for {} damage. Target health: {}/{}",
            event.attacker, event.target, rolled_damage, target_health.current, target_health.max
        );
    }
}

/// System that checks for entities with Health <= 0 and handles death.
fn death_system(
    mut commands: Commands,
    query_dead: Query<(Entity, &Health, Option<&Player>, Option<&Monster>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_manager: ResMut<TurnManager>,
) {
    for (entity, health, is_player, is_monster) in query_dead.iter() {
        if health.current <= 0 {
            if is_player.is_some() {
                // Player died
                eprintln!("Game Over! You died!");
                next_state.set(AppState::Menu); // Transition to menu or a specific game over state
            } else if is_monster.is_some() {
                // Monster died
                info!("Monster {:?} died!", entity);
                commands.entity(entity).despawn();
                // Remove from turn queue if present
                turn_manager.turn_queue.retain(|&e| e != entity);
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
