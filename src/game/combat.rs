use bevy::prelude::*;
use bracket_lib::random::{RandomNumberGenerator, parse_dice_string};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::components::{FloorEntityMarker, InInventory, Inventory, Monster, Name, GodMode, Position};
use crate::game::stats::{Armor, DamageBonus, Dodge, HitBonus};
use crate::game::turns::TurnEndEvent;
use crate::game::{AppState, RunSummary, TurnManager};
use crate::map::dungeon::Floor;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// --- Damage Types & Resistances ---

/// The elemental/physical type of damage dealt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Reflect)]
pub enum DamageType {
    #[default]
    Physical,
    Fire,
    Lightning,
    Poison,
}

impl DamageType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fire" => DamageType::Fire,
            "lightning" => DamageType::Lightning,
            "poison" => DamageType::Poison,
            _ => DamageType::Physical,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DamageType::Physical => "physical",
            DamageType::Fire => "fire",
            DamageType::Lightning => "lightning",
            DamageType::Poison => "poison",
        }
    }
}

/// Per-entity resistance map. Values are percentages.
/// 0 = normal, 50 = 50% reduction, 100 = immune, >100 = heals.
/// Negative = vulnerability (takes extra damage).
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Resistances(pub HashMap<DamageType, i32>);

impl Resistances {
    pub fn get(&self, damage_type: &DamageType) -> i32 {
        self.0.get(damage_type).copied().unwrap_or(0)
    }
}

/// Tags an entity's melee damage with a specific type.
#[derive(Component, Debug, Clone)]
pub struct DamageTypeTag(pub DamageType);

/// Where the damage originated from (melee, spell, poison tick, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageSource {
    Melee,
    Ranged,
    Spell,
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

/// Suppresses HP regen for N turns after taking damage.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect, Default)]
#[reflect(Component)]
pub struct RegenSuppression(pub u32);

/// Component for an entity's damage, using dice notation (e.g., "1d6").
#[derive(Component, Debug)]
pub struct Damage(pub String);

/// Marker on the player entity: next melee attack costs 0 time (riposte after dodge).
#[derive(Component, Debug, Clone)]
pub struct RiposteReady;


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
    pub is_crit: bool,
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
#[allow(dead_code)]
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
#[allow(dead_code)]
#[derive(Message, Debug)]
pub struct MissMessage {
    pub attacker: Entity,
    pub target: Entity,
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

// --- Pure computation helpers (testable without ECS) ---

/// Apply armor reduction to raw damage. Armor can fully negate damage.
pub fn compute_after_armor(raw_damage: i32, armor: i32) -> i32 {
    (raw_damage - armor).max(0)
}

/// Apply a resistance percentage to damage.
/// Returns the final damage (negative = heal via Absorb, 0 = immune).
pub fn apply_resistance(damage: i32, resist_percent: i32) -> i32 {
    let multiplier = 1.0 - (resist_percent as f32 / 100.0);
    (damage as f32 * multiplier).round() as i32
}

/// Apply status multipliers to base damage.
/// `is_enraged`: +50%. `is_terrified`: -25%.
/// Crits are handled upstream by doubling the damage dice, not here.
pub fn apply_damage_multipliers(base: i32, is_enraged: bool, is_terrified: bool) -> i32 {
    let mut damage = base;
    if is_enraged {
        damage = damage * 3 / 2;
    }
    if is_terrified {
        damage = damage * 3 / 4;
    }
    damage.max(1)
}

// --- Systems ---

/// System that handles health regeneration at the end of a global turn cycle.
fn regen_system(
    mut turn_end_events: MessageReader<TurnEndEvent>,
    mut query: Query<(&mut Health, &mut HealthRegen, Has<RegenSuppression>, Option<&crate::game::magic::StatusEffects>)>,
) {
    for _ in turn_end_events.read() {
        for (mut health, mut regen, is_suppressed, status_effects) in query.iter_mut() {
            // Suppress regen if entity has RegenSuppression or is Poisoned
            if is_suppressed || status_effects.is_some_and(|fx| fx.is_poisoned()) {
                continue;
            }
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

/// 1. Hit Chance: d20 + hit_bonus >= 4 + dodge_bonus (natural 20 always hits)
fn hit_check_system(
    mut commands: Commands,
    mut intents: MessageReader<AttackIntentMessage>,
    mut roll_writer: MessageWriter<DamageRollMessage>,
    mut miss_writer: MessageWriter<MissMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(&Name, Option<&Dodge>, Option<&HitBonus>, Has<Player>)>,
    player_equipment_query: Query<&crate::game::items::Equipment, With<Player>>,
    weapon_props_query: Query<&crate::game::items::ItemProperties>,
) {
    for intent in intents.read() {
        let Ok((attacker_name, _, attacker_hit_bonus, is_player)) = query.get(intent.attacker) else {
            continue;
        };
        let Ok((target_name, target_dodge, _, target_is_player)) = query.get(intent.target) else {
            continue;
        };

        let hit_roll = game_rng.0.roll_dice(1, 20);

        let hit_bonus = attacker_hit_bonus.map(|h| h.0).unwrap_or(0);
        let dodge_val = target_dodge.map(|d| d.0).unwrap_or(0);
        let dodge_target = 4 + dodge_val;
        let is_natural_20 = hit_roll == 20;

        if is_natural_20 || (hit_roll + hit_bonus >= dodge_target) {
            roll_writer.write(DamageRollMessage {
                attacker: intent.attacker,
                target: intent.target,
                damage_type: intent.damage_type,
                source: intent.source,
                is_crit: is_natural_20,
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

            // Riposte: when the player dodges an attack and has a Riposte weapon,
            // grant a free melee attack on their next turn.
            if target_is_player && intent.source == DamageSource::Melee {
                if let Ok(equipment) = player_equipment_query.get(intent.target) {
                    if let Some(weapon_entity) = equipment.weapon {
                        if let Ok(props) = weapon_props_query.get(weapon_entity) {
                            if props.weapon_ability.as_deref() == Some("Riposte") {
                                commands.entity(intent.target).insert(RiposteReady);
                                log_writer.write(GameLogMessage("You prepare a riposte!".to_string()));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 2. Damage Calculation: Roll attacker damage dice. Crits (nat 20) double the dice.
fn damage_roll_system(
    mut commands: Commands,
    mut roll_messages: MessageReader<DamageRollMessage>,
    mut reduction_writer: MessageWriter<DamageReductionMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(
        &Damage,
        Option<&crate::game::magic::StatusEffects>,
        Has<crate::game::abilities::Terrified>,
        Option<&DamageBonus>,
        Has<Player>,
    )>,
    player_equipment_query: Query<&crate::game::items::Equipment, With<Player>>,
    weapon_props_query: Query<&crate::game::items::ItemProperties>,
    target_ai_query: Query<&crate::game::MonsterAI>,
) {
    for message in roll_messages.read() {
        let Ok((damage_dice, status_effects, is_terrified, damage_bonus, attacker_is_player)) = query.get(message.attacker) else {
            continue;
        };

        let base_roll = roll_dice(&damage_dice.0, &mut game_rng.0);
        let rolled_damage = if message.is_crit {
            base_roll + roll_dice(&damage_dice.0, &mut game_rng.0)
        } else {
            base_roll
        };

        let bonus = damage_bonus.map(|d| d.0).unwrap_or(0);

        let is_enraged = status_effects.map(|e| e.is_enraged()).unwrap_or(false);
        let mut raw_damage = apply_damage_multipliers(rolled_damage + bonus, is_enraged, is_terrified);

        // Backstab: player with Backstab weapon attacking a sleeping monster deals triple damage.
        if attacker_is_player && message.source == DamageSource::Melee {
            if let Ok(equipment) = player_equipment_query.get(message.attacker) {
                if let Some(weapon_entity) = equipment.weapon {
                    if let Ok(props) = weapon_props_query.get(weapon_entity) {
                        if props.weapon_ability.as_deref() == Some("Backstab") {
                            if let Ok(target_ai) = target_ai_query.get(message.target) {
                                if target_ai.is_asleep() {
                                    raw_damage *= 3;
                                    log_writer.write(GameLogMessage("Backstab! Triple damage!".to_string()));
                                }
                            }
                        }
                    }
                }
            }
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
    query: Query<(Option<&Armor>, Option<&crate::game::abilities::RallyBuff>, Option<&Resistances>, &Name)>,
) {
    for message in reduction_messages.read() {
        let Ok((armor, rally_buff, resistances, target_name)) = query.get(message.target) else {
            continue;
        };

        // Armor reduction: only Physical damage applies armor
        let after_armor = if message.damage_type == DamageType::Physical {
            let armor_val = armor.map(|a| a.0).unwrap_or(0)
                + rally_buff.map(|r| r.armor_bonus).unwrap_or(0);
            compute_after_armor(message.raw_damage, armor_val)
        } else {
            message.raw_damage // Non-physical skips armor
        };

        // Resistance percentage
        let resist_percent = resistances
            .map(|r| r.get(&message.damage_type))
            .unwrap_or(0);
        let final_damage = apply_resistance(after_armor, resist_percent);

        // Log resistance effects
        if resist_percent >= 100 {
            log_writer.write(GameLogMessage(format!(
                "{} is immune to {} damage!", target_name.0, message.damage_type.name()
            )));
        } else if resist_percent > 0 {
            log_writer.write(GameLogMessage(format!(
                "{} resists the {} damage.", target_name.0, message.damage_type.name()
            )));
        } else if resist_percent < 0 {
            log_writer.write(GameLogMessage(format!(
                "{} is weak to {}!", target_name.0, message.damage_type.name()
            )));
        }

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
///    Absorb resistance: negative final_damage means heal instead.
fn damage_application_system(
    mut commands: Commands,
    mut apply_messages: MessageReader<ApplyDamageMessage>,
    mut death_writer: MessageWriter<DeathEvent>,
    mut on_hit_writer: MessageWriter<crate::game::abilities::OnHitTriggerMessage>,
    mut on_being_hit_writer: MessageWriter<crate::game::abilities::OnBeingHitTriggerMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut query_health: Query<(
        &mut Health,
        &Name,
        Has<GodMode>,
        Has<Player>,
    )>,
    query_names: Query<(&Name, Has<Player>)>,
    mut run_stats: ResMut<crate::game::RunStats>,
) {
    for message in apply_messages.read() {
        let Ok((mut target_health, target_name, has_god_mode, target_is_player)) =
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

        // Track last attacker for the death screen cause-of-death line.
        if target_is_player {
            run_stats.last_hit_by = attacker_name.0.clone();
        }

        if message.final_damage > 0 {
            target_health.current -= message.final_damage;
            // Suppress HP regen for 5 turns after taking damage
            commands.entity(message.target).insert(RegenSuppression(5));
        }

        // Emit ability trigger messages for on-hit and on-being-hit handlers.
        // Only for direct attacks (melee/ranged), not environment/spell DoTs.
        if message.source == DamageSource::Melee || message.source == DamageSource::Ranged {
            on_hit_writer.write(crate::game::abilities::OnHitTriggerMessage {
                attacker: message.attacker,
                defender: message.target,
                final_damage: message.final_damage,
                source: message.source,
            });
            on_being_hit_writer.write(crate::game::abilities::OnBeingHitTriggerMessage {
                attacker: message.attacker,
                defender: message.target,
                final_damage: message.final_damage,
                source: message.source,
                damage_type: message.damage_type,
            });
        }

        let verb = if is_player { "hit" } else { "hits" };
        log_writer.write(GameLogMessage(format!(
            "{} {} {} for {} damage.",
            attacker_name.0, verb, target_name.0, message.final_damage
        )));

        if target_health.current <= 0 {
            death_writer.write(DeathEvent {
                attacker: message.attacker,
                target: message.target,
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

/// Tick down regen suppression each turn end. Removes the component when it reaches 0.
fn tick_regen_suppression(
    mut commands: Commands,
    mut turn_end_events: MessageReader<TurnEndEvent>,
    mut query: Query<(Entity, &mut RegenSuppression)>,
) {
    for _ in turn_end_events.read() {
        for (entity, mut suppression) in query.iter_mut() {
            if suppression.0 <= 1 {
                commands.entity(entity).remove::<RegenSuppression>();
            } else {
                suppression.0 -= 1;
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

/// Drop all held inventory items at the entity's death position.
/// Runs before `death_system` so items are placed before the entity is despawned.
pub fn drop_inventory_on_death(
    mut commands: Commands,
    query: Query<(Entity, &Health, &Position, &Inventory)>,
) {
    for (entity, health, pos, inventory) in query.iter() {
        if health.current > 0 || inventory.items.is_empty() {
            continue;
        }
        for &item_entity in &inventory.items {
            commands
                .entity(item_entity)
                .remove::<InInventory>()
                .insert(Position { x: pos.x, y: pos.y })
                .insert(Visibility::Inherited)
                .insert(FloorEntityMarker);
        }
    }
}

/// System that checks for entities with Health <= 0 and handles death.
pub fn death_system(
    mut commands: Commands,
    mut query_dead: Query<(Entity, &mut Health, &Name, Option<&Player>, Option<&Monster>, Has<crate::components::Destructible>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_manager: ResMut<TurnManager>,
    mut log_writer: MessageWriter<GameLogMessage>,
    floor: Res<Floor>,
    mut run_summary: ResMut<RunSummary>,
    mut run_stats: ResMut<crate::game::RunStats>,
) {
    for (entity, mut health, name, is_player, is_monster, is_destructible) in query_dead.iter_mut() {
        if health.current <= 0 {
            if is_player.is_some() {
                // Player died — permadeath: erase the save
                eprintln!("Game Over! You died!");
                log_writer.write(GameLogMessage("You have died!".to_string()));
                let cause = if run_stats.last_hit_by.is_empty() {
                    "Unknown".to_string()
                } else {
                    format!("Slain by a {} on floor {}.", run_stats.last_hit_by, floor.0)
                };
                *run_summary = RunSummary {
                    floor_reached: floor.0,
                    cause,
                    victory: false,
                    enemies_killed: run_stats.enemies_killed,
                };
                crate::save::delete_save();
                next_state.set(AppState::GameOver);
            } else if is_monster.is_some() {
                // Monster died
                run_stats.enemies_killed += 1;
                info!("Monster {:?} died!", entity);
                log_writer.write(GameLogMessage(format!("{} dies.", name.0)));
                commands.entity(entity).despawn();
                // Remove from turn queue if present
                turn_manager.turn_queue.retain(|&(e, _)| e != entity);
            } else if is_destructible {
                // Destructible prop destroyed (e.g., barricade)
                log_writer.write(GameLogMessage(format!("The {} crumbles!", name.0.to_lowercase())));
                commands.entity(entity).despawn();
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
            .add_message::<DeathEvent>()
            .register_type::<Health>()
            .register_type::<HealthRegen>()
            .register_type::<RegenSuppression>()
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
                    tick_regen_suppression,
                    handle_heal_system,
                    handle_toggle_god_mode_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- compute_after_armor ---

    #[test]
    fn armor_reduces_damage() {
        assert_eq!(compute_after_armor(10, 3), 7);
    }

    #[test]
    fn armor_can_reduce_to_zero() {
        assert_eq!(compute_after_armor(5, 100), 0);
    }

    #[test]
    fn zero_armor_passes_through() {
        assert_eq!(compute_after_armor(8, 0), 8);
    }

    // --- apply_resistance ---

    #[test]
    fn resistance_zero_is_normal() {
        assert_eq!(apply_resistance(10, 0), 10);
    }

    #[test]
    fn resistance_50_halves_damage() {
        assert_eq!(apply_resistance(10, 50), 5);
    }

    #[test]
    fn resistance_100_is_immune() {
        assert_eq!(apply_resistance(10, 100), 0);
    }

    #[test]
    fn resistance_150_heals() {
        assert_eq!(apply_resistance(10, 150), -5);
    }

    #[test]
    fn resistance_negative_50_is_vulnerable() {
        assert_eq!(apply_resistance(10, -50), 15);
    }

    // --- apply_damage_multipliers ---

    #[test]
    fn no_multipliers_passes_through() {
        assert_eq!(apply_damage_multipliers(10, false, false), 10);
    }

    #[test]
    fn enraged_adds_50_percent() {
        assert_eq!(apply_damage_multipliers(10, true, false), 15);
    }

    #[test]
    fn terrified_reduces_25_percent() {
        assert_eq!(apply_damage_multipliers(10, false, true), 7);
    }

    #[test]
    fn enraged_and_terrified_stack_multiplicatively() {
        // 10 * 1.5 (enrage) = 15, then 15 * 0.75 (terrified) = 11
        assert_eq!(apply_damage_multipliers(10, true, true), 11);
    }

    #[test]
    fn minimum_damage_is_one() {
        assert_eq!(apply_damage_multipliers(1, false, true), 1);
    }

    // --- Resistance component ---

    #[test]
    fn resistances_default_to_zero() {
        let r = Resistances::default();
        assert_eq!(r.get(&DamageType::Fire), 0);
    }

    #[test]
    fn resistances_lookup() {
        let mut map = HashMap::new();
        map.insert(DamageType::Fire, 100);
        map.insert(DamageType::Lightning, -50);
        let r = Resistances(map);
        assert_eq!(r.get(&DamageType::Fire), 100);
        assert_eq!(r.get(&DamageType::Lightning), -50);
        assert_eq!(r.get(&DamageType::Physical), 0);
    }

    // --- DamageBonus ---

    #[test]
    fn damage_bonus_adds_to_base() {
        // Simulated: rolled 4, bonus 2, no multipliers
        let result = apply_damage_multipliers(4 + 2, false, false);
        assert_eq!(result, 6);
    }

    // --- RegenSuppression ---

    #[test]
    fn regen_suppression_decrements() {
        // Simulated: 5 turns, tick 4 times = 1 remaining
        let mut turns = 5u32;
        for _ in 0..4 {
            turns -= 1;
        }
        assert_eq!(turns, 1);
    }

    // --- DamageType parsing ---

    #[test]
    fn damage_type_from_str() {
        assert_eq!(DamageType::from_str("fire"), DamageType::Fire);
        assert_eq!(DamageType::from_str("LIGHTNING"), DamageType::Lightning);
        assert_eq!(DamageType::from_str("poison"), DamageType::Poison);
        assert_eq!(DamageType::from_str("unknown"), DamageType::Physical);
        assert_eq!(DamageType::from_str(""), DamageType::Physical);
    }

    // --- Full pipeline integration: armor + resistance ---

    #[test]
    fn armor_then_resistance_vulnerable() {
        let after_armor = compute_after_armor(20, 5); // 15
        let final_damage = apply_resistance(after_armor, -50); // 15 * 1.5 = 22.5 -> 23
        assert_eq!(final_damage, 23);
    }

    #[test]
    fn armor_then_resistance_immune() {
        let after_armor = compute_after_armor(20, 5); // 15
        let final_damage = apply_resistance(after_armor, 100); // 0
        assert_eq!(final_damage, 0);
    }

    #[test]
    fn armor_then_resistance_absorb() {
        let after_armor = compute_after_armor(20, 5); // 15
        let final_damage = apply_resistance(after_armor, 150); // 15 * -0.5 = -7.5 -> -8
        assert_eq!(final_damage, -8);
    }

}
