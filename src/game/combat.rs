use bevy::prelude::*;
use bracket_lib::random::{RandomNumberGenerator, parse_dice_string};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::components::{Monster, Name, GodMode};
use crate::game::level::{Experience, ExperienceReward};
use crate::game::magic::{Enraged, SpiritShielded};
use crate::game::stats::{CombatStats, Level, Mana};
use crate::game::turns::TurnEndEvent;
use crate::game::{AppState, RunSummary, TurnManager};
use crate::map::dungeon::Floor;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// --- Damage Types & Resistances ---

/// The elemental/physical type of damage dealt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DamageType {
    #[default]
    Physical,
    Fire,
    Ice,
    Lightning,
    Necrotic,
    Poison,
}

impl DamageType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fire" => DamageType::Fire,
            "ice" => DamageType::Ice,
            "lightning" => DamageType::Lightning,
            "necrotic" => DamageType::Necrotic,
            "poison" => DamageType::Poison,
            _ => DamageType::Physical,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DamageType::Physical => "physical",
            DamageType::Fire => "fire",
            DamageType::Ice => "ice",
            DamageType::Lightning => "lightning",
            DamageType::Necrotic => "necrotic",
            DamageType::Poison => "poison",
        }
    }
}

/// How much an entity resists a particular damage type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResistanceLevel {
    /// Takes 150% damage.
    Weak,
    /// Takes normal damage.
    Normal,
    /// Takes 50% damage (min 1).
    Resistant,
    /// Takes 0 damage.
    Immune,
    /// Heals instead of taking damage.
    Absorb,
}

impl ResistanceLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "weak" => ResistanceLevel::Weak,
            "resistant" => ResistanceLevel::Resistant,
            "immune" => ResistanceLevel::Immune,
            "absorb" => ResistanceLevel::Absorb,
            _ => ResistanceLevel::Normal,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ResistanceLevel::Weak => "weak",
            ResistanceLevel::Normal => "normal",
            ResistanceLevel::Resistant => "resistant",
            ResistanceLevel::Immune => "immune",
            ResistanceLevel::Absorb => "absorb",
        }
    }
}

/// Per-entity resistance map. Missing entries default to Normal.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Resistances(pub HashMap<DamageType, ResistanceLevel>);

impl Resistances {
    pub fn get(&self, damage_type: &DamageType) -> ResistanceLevel {
        self.0.get(damage_type).copied().unwrap_or(ResistanceLevel::Normal)
    }
}

/// Tags an entity's melee damage with a specific type.
#[derive(Component, Debug, Clone)]
pub struct DamageTypeTag(pub DamageType);

/// Where the damage originated from (melee, spell, poison tick, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageSource {
    Melee,
    Spell,
    Poison,
    Environment,
}

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
    pub damage_type: DamageType,
    pub source: DamageSource,
}

/// Message sent after a successful hit to trigger damage rolling.
#[derive(Message, Debug)]
pub struct DamageRollMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub damage_type: DamageType,
    pub source: DamageSource,
}

/// Message sent after damage is rolled to apply armor reduction.
#[derive(Message, Debug)]
pub struct DamageReductionMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub raw_damage: i32,
    pub damage_type: DamageType,
    pub source: DamageSource,
}

/// Message sent after armor reduction to finally apply damage to health.
#[derive(Message, Debug)]
pub struct ApplyDamageMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub final_damage: i32,
    pub damage_type: DamageType,
    pub source: DamageSource,
}

/// Message sent to heal an entity.
#[derive(Message, Debug)]
pub struct HealMessage {
    pub entity: Entity,
    pub amount: i32,
}

/// Message sent when an attack misses its target.
#[derive(Message, Debug)]
pub struct MissMessage {
    pub attacker: Entity,
    pub target: Entity,
}

/// Message emitted after a successful melee hit to trigger on-hit effects.
#[derive(Message, Debug)]
pub struct OnHitTriggerMessage {
    pub attacker: Entity,
    pub defender: Entity,
}

/// Message sent to toggle GodMode on an entity.
#[derive(Message, Debug)]
pub struct ToggleGodModeMessage {
    pub entity: Entity,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DeathEvent {
    pub attacker: Entity,
    pub target: Entity,
    pub xp: i32,
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
    mut miss_writer: MessageWriter<MissMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(&Name, &CombatStats, Has<Player>)>,
) {
    for intent in intents.read() {
        let Ok((attacker_name, attacker_stats, is_player)) = query.get(intent.attacker) else {
            continue;
        };
        let Ok((target_name, target_stats, _)) = query.get(intent.target) else {
            continue;
        };

        let hit_roll = game_rng.0.roll_dice(1, 20);
        let hit_target = 10 + target_stats.dodge_chance;
        let final_hit_score = hit_roll + attacker_stats.hit_chance;

        if final_hit_score >= hit_target {
            roll_writer.write(DamageRollMessage {
                attacker: intent.attacker,
                target: intent.target,
                damage_type: intent.damage_type,
                source: intent.source,
            });
        } else {
            let verb = if is_player { "miss" } else { "misses" };
            log_writer.write(GameLogMessage(format!(
                "{} {} {}.",
                attacker_name.0, verb, target_name.0
            )));
            miss_writer.write(MissMessage {
                attacker: intent.attacker,
                target: intent.target,
            });
        }
    }
}

/// 2. Damage Calculation: Roll attacker damage dice and add damage bonus.
/// Enraged attackers deal 150% damage.
fn damage_roll_system(
    mut roll_messages: MessageReader<DamageRollMessage>,
    mut reduction_writer: MessageWriter<DamageReductionMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(&Damage, &CombatStats, Has<Enraged>)>,
) {
    for message in roll_messages.read() {
        let Ok((damage_dice, attacker_stats, is_enraged)) = query.get(message.attacker) else {
            continue;
        };

        let rolled_damage = roll_dice(&damage_dice.0, &mut game_rng.0);
        let mut raw_damage = rolled_damage + attacker_stats.damage_bonus;

        // Enraged: +50% damage
        if is_enraged {
            raw_damage = raw_damage * 3 / 2;
        }

        reduction_writer.write(DamageReductionMessage {
            attacker: message.attacker,
            target: message.target,
            raw_damage,
            damage_type: message.damage_type,
            source: message.source,
        });
    }
}

/// 3. Armor Reduction + Resistance: Subtract armor, then apply resistance multiplier.
fn armor_reduction_system(
    mut reduction_messages: MessageReader<DamageReductionMessage>,
    mut apply_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    query: Query<(&CombatStats, Option<&Resistances>, &Name)>,
) {
    for message in reduction_messages.read() {
        let Ok((target_stats, resistances, target_name)) = query.get(message.target) else {
            continue;
        };

        // Armor reduction (physical mitigation)
        let after_armor = (message.raw_damage - target_stats.armor).max(1);

        // Resistance multiplier
        let resistance = resistances
            .map(|r| r.get(&message.damage_type))
            .unwrap_or(ResistanceLevel::Normal);

        let final_damage = match resistance {
            ResistanceLevel::Weak => {
                log_writer.write(GameLogMessage(format!(
                    "{} is weak to {}!",
                    target_name.0,
                    message.damage_type.name()
                )));
                (after_armor * 3 / 2).max(1)
            }
            ResistanceLevel::Normal => after_armor,
            ResistanceLevel::Resistant => {
                log_writer.write(GameLogMessage(format!(
                    "{} resists the {} damage.",
                    target_name.0,
                    message.damage_type.name()
                )));
                (after_armor / 2).max(1)
            }
            ResistanceLevel::Immune => {
                log_writer.write(GameLogMessage(format!(
                    "{} is immune to {} damage!",
                    target_name.0,
                    message.damage_type.name()
                )));
                0
            }
            ResistanceLevel::Absorb => {
                // Negative damage signals healing in damage_application_system
                log_writer.write(GameLogMessage(format!(
                    "{} absorbs the {}! (+{} HP)",
                    target_name.0,
                    message.damage_type.name(),
                    after_armor
                )));
                -after_armor
            }
        };

        apply_writer.write(ApplyDamageMessage {
            attacker: message.attacker,
            target: message.target,
            final_damage,
            damage_type: message.damage_type,
            source: message.source,
        });
    }
}

/// 4. Damage Application: Update health and log the result.
/// Spirit Shield: if the target has `SpiritShielded`, damage is absorbed by mana first.
/// Absorb resistance: negative final_damage means heal instead.
fn damage_application_system(
    mut apply_messages: MessageReader<ApplyDamageMessage>,
    mut death_writer: MessageWriter<DeathEvent>,
    mut on_hit_writer: MessageWriter<OnHitTriggerMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut query_health: Query<(
        &mut Health,
        &Name,
        Option<&ExperienceReward>,
        Has<GodMode>,
        Has<SpiritShielded>,
    )>,
    mut mana_query: Query<&mut Mana>,
    query_names: Query<(&Name, Has<Player>)>,
) {
    for message in apply_messages.read() {
        let Ok((mut target_health, target_name, xp_reward, has_god_mode, has_spirit_shield)) =
            query_health.get_mut(message.target)
        else {
            continue;
        };

        // Absorb resistance: heal instead of damage
        if message.final_damage < 0 {
            let heal = (-message.final_damage).min(target_health.max - target_health.current);
            target_health.current += heal;
            continue;
        }

        // Immune: 0 damage = skip entirely
        if message.final_damage == 0 {
            continue;
        }

        if has_god_mode {
            info!("{} is in GodMode, ignoring damage!", target_name.0);
            continue;
        }

        let Ok((attacker_name, is_player)) = query_names.get(message.attacker) else {
            continue;
        };

        let mut remaining_damage = message.final_damage;

        // Spirit Shield: absorb damage from mana first
        if has_spirit_shield {
            if let Ok(mut mana) = mana_query.get_mut(message.target) {
                let absorbed = remaining_damage.min(mana.current);
                mana.current -= absorbed;
                remaining_damage -= absorbed;
                if absorbed > 0 {
                    log_writer.write(GameLogMessage(format!(
                        "{}'s spirit shield absorbs {} damage! (Mana: {}/{})",
                        target_name.0, absorbed, mana.current, mana.max
                    )));
                }
            }
        }

        if remaining_damage > 0 {
            target_health.current -= remaining_damage;
        }

        let verb = if is_player { "hit" } else { "hits" };
        log_writer.write(GameLogMessage(format!(
            "{} {} {} for {} damage.",
            attacker_name.0, verb, target_name.0, message.final_damage
        )));

        // Trigger on-hit effects for melee attacks that dealt damage
        if message.source == DamageSource::Melee && remaining_damage > 0 {
            on_hit_writer.write(OnHitTriggerMessage {
                attacker: message.attacker,
                defender: message.target,
            });
        }

        if target_health.current <= 0 {
            death_writer.write(DeathEvent {
                attacker: message.attacker,
                target: message.target,
                xp: xp_reward.map(|r| r.0).unwrap_or(0),
            });
        }

        info!(
            "Entity {:?} hit Entity {:?} for {} damage. Target health: {}/{}",
            message.attacker,
            message.target,
            message.final_damage,
            target_health.current,
            target_health.max
        );
    }
}

/// System that handles healing for entities.
pub fn handle_heal_system(
    mut messages: MessageReader<HealMessage>,
    mut query: Query<(&mut Health, &Name)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if let Ok((mut health, name)) = query.get_mut(msg.entity) {
            let old_health = health.current;
            health.current = (health.current + msg.amount).min(health.max);
            let healed_amount = health.current - old_health;
            if healed_amount > 0 {
                log_writer.write(GameLogMessage(format!("{} is healed for {} HP.", name.0, healed_amount)));
            }
        }
    }
}

/// System that toggles GodMode on an entity.
pub fn handle_toggle_god_mode_system(
    mut commands: Commands,
    mut messages: MessageReader<ToggleGodModeMessage>,
    query: Query<(&Name, Has<GodMode>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if let Ok((name, has_god_mode)) = query.get(msg.entity) {
            if has_god_mode {
                commands.entity(msg.entity).remove::<GodMode>();
                log_writer.write(GameLogMessage(format!("{} Godmode DISABLED.", name.0)));
            } else {
                commands.entity(msg.entity).insert(GodMode);
                log_writer.write(GameLogMessage(format!("{} Godmode ENABLED.", name.0)));
            }
        }
    }
}

/// System that checks for entities with Health <= 0 and handles death.
pub fn death_system(
    mut commands: Commands,
    query_dead: Query<(Entity, &Health, &Name, Option<&Player>, Option<&Monster>, Option<&Experience>, Option<&Level>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_manager: ResMut<TurnManager>,
    mut log_writer: MessageWriter<GameLogMessage>,
    floor: Res<Floor>,
    mut run_summary: ResMut<RunSummary>,
) {
    for (entity, health, name, is_player, is_monster, exp, level) in query_dead.iter() {
        if health.current <= 0 {
            if is_player.is_some() {
                // Player died — permadeath: erase the save
                eprintln!("Game Over! You died!");
                log_writer.write(GameLogMessage("You have died!".to_string()));
                *run_summary = RunSummary {
                    floor_reached: floor.0,
                    level: level.map(|l| l.value).unwrap_or(1),
                    xp_earned: exp.map(|e| e.current).unwrap_or(0),
                    cause: "Unknown".to_string(),
                    victory: false,
                };
                crate::save::delete_save();
                next_state.set(AppState::GameOver);
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

/// System set label for the damage resolution pipeline.
/// Use `.after(CombatDamageSet)` to guarantee a system runs after damage is applied
/// and `DeathEvent` messages have been written.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CombatDamageSet;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameRng(RandomNumberGenerator::new())) // Initialize wrapped RNG
            .add_message::<AttackIntentMessage>()
            .add_message::<DamageRollMessage>()
            .add_message::<DamageReductionMessage>()
            .add_message::<ApplyDamageMessage>()
            .add_message::<HealMessage>()
            .add_message::<MissMessage>()
            .add_message::<ToggleGodModeMessage>()
            .add_message::<OnHitTriggerMessage>()
            .add_message::<DeathEvent>()
            .register_type::<Health>()
            .register_type::<HealthRegen>()
            .register_type::<GodMode>()
            .configure_sets(Update, CombatDamageSet.run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                (
                    (
                        hit_check_system,
                        damage_roll_system,
                        armor_reduction_system,
                        damage_application_system,
                    )
                        .chain()
                        .in_set(CombatDamageSet),
                    regen_system,
                    death_system,
                    handle_heal_system,
                    handle_toggle_god_mode_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
