use bevy::prelude::*;
use bracket_lib::random::{RandomNumberGenerator, parse_dice_string};

use crate::components::{Monster, Name}; // Import Monster marker
use crate::game::{AppState, TurnManager};
use crate::game::turns::TurnEndEvent;
use crate::game::stats::CombatStats;
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

/// Message sent when an entity intends to attack another entity.
#[derive(Message, Debug)]
pub struct AttackIntentMessage {
    pub attacker: Entity,
    pub target: Entity,
}

/// Message sent after a successful hit to trigger damage rolling.
#[derive(Message, Debug)]
pub struct DamageRollMessage {
    pub attacker: Entity,
    pub target: Entity,
}

/// Message sent after damage is rolled to apply armor reduction.
#[derive(Message, Debug)]
pub struct DamageReductionMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub raw_damage: i32,
}

/// Message sent after armor reduction to finally apply damage to health.
#[derive(Message, Debug)]
pub struct ApplyDamageMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub final_damage: i32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DeathEvent {
    pub attacker: Entity,
    pub target: Entity,
}

// --- Resources ---

/// Wrapper for bracket_lib's RandomNumberGenerator to be used as a Bevy Resource.
#[derive(Resource)]
pub struct GameRng(pub RandomNumberGenerator);

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

/// 1. Hit Chance Calculation: Roll 1d20 + attacker.hit_chance vs 10 + target.dodge_chance.
fn hit_check_system(
    mut intents: MessageReader<AttackIntentMessage>,
    mut roll_writer: MessageWriter<DamageRollMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(&Name, &CombatStats)>,
) {
    for intent in intents.read() {
        let Ok((attacker_name, attacker_stats)) = query.get(intent.attacker) else { continue };
        let Ok((target_name, target_stats)) = query.get(intent.target) else { continue };

        let hit_roll = game_rng.0.roll_dice(1, 20);
        let hit_target = 10 + target_stats.dodge_chance;
        let final_hit_score = hit_roll + attacker_stats.hit_chance;

        if final_hit_score >= hit_target {
            roll_writer.write(DamageRollMessage {
                attacker: intent.attacker,
                target: intent.target,
            });
        } else {
            log_writer.write(GameLogMessage(format!("{} misses {}.", attacker_name.0, target_name.0)));
        }
    }
}

/// 2. Damage Calculation: Roll attacker damage dice and add damage bonus.
fn damage_roll_system(
    mut roll_messages: MessageReader<DamageRollMessage>,
    mut reduction_writer: MessageWriter<DamageReductionMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(&Damage, &CombatStats)>,
) {
    for message in roll_messages.read() {
        let Ok((damage_dice, attacker_stats)) = query.get(message.attacker) else { continue };

        let rolled_damage = roll_dice(&damage_dice.0, &mut game_rng.0);
        let raw_damage = rolled_damage + attacker_stats.damage_bonus;

        reduction_writer.write(DamageReductionMessage {
            attacker: message.attacker,
            target: message.target,
            raw_damage,
        });
    }
}

/// 3. Armor Reduction: Subtract target armor from raw damage (min 1).
fn armor_reduction_system(
    mut reduction_messages: MessageReader<DamageReductionMessage>,
    mut apply_writer: MessageWriter<ApplyDamageMessage>,
    query: Query<&CombatStats>,
) {
    for message in reduction_messages.read() {
        let Ok(target_stats) = query.get(message.target) else { continue };

        let final_damage = (message.raw_damage - target_stats.armor).max(1);

        apply_writer.write(ApplyDamageMessage {
            attacker: message.attacker,
            target: message.target,
            final_damage,
        });
    }
}

/// 4. Damage Application: Update health and log the result.
fn damage_application_system(
    mut apply_messages: MessageReader<ApplyDamageMessage>,
    mut death_writer: MessageWriter<DeathEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut query_health: Query<(&mut Health, &Name)>,
    query_names: Query<&Name>,
) {
    for message in apply_messages.read() {
        let Ok((mut target_health, target_name)) = query_health.get_mut(message.target) else { continue };
        let Ok(attacker_name) = query_names.get(message.attacker) else { continue };

        target_health.current -= message.final_damage;

        log_writer.write(GameLogMessage(format!(
            "{} hits {} for {} damage.",
            attacker_name.0, target_name.0, message.final_damage
        )));

        if target_health.current <= 0 {
            death_writer.write(DeathEvent {
                attacker: message.attacker,
                target: message.target,
            });
        }

        info!(
            "Entity {:?} hit Entity {:?} for {} damage. Target health: {}/{}",
            message.attacker, message.target, message.final_damage, target_health.current, target_health.max
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
            }
        }
    }
}

// --- Plugin ---

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameRng(RandomNumberGenerator::new())) // Initialize wrapped RNG
            .add_message::<AttackIntentMessage>()
            .add_message::<DamageRollMessage>()
            .add_message::<DamageReductionMessage>()
            .add_message::<ApplyDamageMessage>()
            .add_message::<DeathEvent>()
            .register_type::<Health>()
            .register_type::<HealthRegen>()
            .add_systems(
                Update,
                (
                    (
                        hit_check_system,
                        damage_roll_system,
                        armor_reduction_system,
                        damage_application_system,
                    ).chain(),
                    regen_system,
                    death_system,
                ).run_if(in_state(AppState::InGame)),
            );
    }
}
