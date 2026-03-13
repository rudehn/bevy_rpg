use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// --- Faction ---

/// Determines how this entity relates to others for AI targeting and spell scoring.
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub struct Faction(pub FactionKind);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FactionKind {
    Player,
    Monster,
}

impl FactionKind {
    /// Returns true if `other` is a valid hostile target for `self`.
    pub fn is_hostile_to(&self, other: &FactionKind) -> bool {
        self != other
    }

    /// Returns true if `other` is on the same side as `self`.
    pub fn is_allied_to(&self, other: &FactionKind) -> bool {
        self == other
    }
}

// --- Monster Ability Components ---

/// Base armor value from monster definition (flat damage reduction).
#[derive(Component, Debug, Clone)]
pub struct BaseArmor(pub i32);

/// Monster flees when below 50% HP.
#[derive(Component, Debug, Clone)]
pub struct Cowardly;

// --- On-Hit Effects ---

/// A single on-hit effect that can trigger when a monster lands a melee attack.
/// Chance is a flat percentage (1–100): roll 1–100, if ≤ chance, effect triggers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OnHitEffect {
    ApplyPoison { damage_per_turn: i32, duration: u32, chance: u32 },
    ApplySlow { duration: u32, chance: u32 },
    ApplyStun { duration: u32, chance: u32 },
    AttributeDrain { attribute: String, amount: i32, duration: u32, chance: u32 },
    LifeDrain { amount: i32, chance: u32 },
}

/// Collection of on-hit effects on an entity.
#[derive(Component, Clone, Debug)]
pub struct OnHitEffects(pub Vec<OnHitEffect>);

// --- Passive Ability Components (Stubs) ---
// Each passive ability is its own component. Monsters that have it just carry the component;
// dedicated systems react to game events and trigger the effect automatically.
// Abilities have no mana cost, no cooldown, and are never "cast."

/// Inflicts poison stacks on any entity that physically attacks this one.
#[derive(Component, Debug, Clone)]
pub struct PoisonBody {
    /// How many stacks of poison the attacker receives per hit.
    pub stacks: i32,
}

/// Deals area damage to nearby entities when this entity dies.
#[derive(Component, Debug, Clone)]
pub struct ExplodeOnDeath {
    /// Tile radius of the explosion.
    pub radius: i32,
    /// Flat damage dealt to each entity in range.
    pub damage: i32,
}

/// Can revive itself once after reaching 0 HP.
#[derive(Component, Debug, Clone)]
pub struct Reanimate {
    /// HP the entity revives with.
    pub revive_hp: i32,
}

// --- On-Hit Handler System ---

use crate::components::Name;
use crate::game::combat::{
    DamageType, GameRng, HealMessage, OnHitTriggerMessage, Resistances, ResistanceLevel,
};
use crate::game::magic::{Poisoned, Slowed, Stunned, TimedModifierEntry, TimedModifiers};
use crate::game::stats::AttributeModifiers;
use crate::ui::game_log::GameLogMessage;

/// Processes on-hit effects after a melee attack deals damage.
/// Each effect has a flat chance (1–100). Rolls 1–100; if ≤ chance, triggers.
pub fn handle_on_hit_effects(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, Option<&OnHitEffects>)>,
    defender_query: Query<(&Name, Option<&Resistances>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
) {
    for msg in messages.read() {
        let Ok((attacker_name, on_hit)) = attacker_query.get(msg.attacker) else {
            continue;
        };
        let Some(on_hit) = on_hit else { continue };

        let Ok((defender_name, defender_resistances)) = defender_query.get(msg.defender) else {
            continue;
        };

        for effect in &on_hit.0 {
            match effect {
                OnHitEffect::ApplyPoison { damage_per_turn, duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        // Check poison immunity via Resistances
                        let poison_resistance = defender_resistances
                            .map(|r| r.get(&DamageType::Poison))
                            .unwrap_or(ResistanceLevel::Normal);
                        if poison_resistance == ResistanceLevel::Immune {
                            log_writer.write(GameLogMessage(format!(
                                "{} resists the venom! (poison immune)",
                                defender_name.0
                            )));
                        } else {
                            commands.entity(msg.defender).insert(Poisoned {
                                damage_per_turn: *damage_per_turn,
                                turns_remaining: *duration,
                            });
                            log_writer.write(GameLogMessage(format!(
                                "{}'s attack poisons {}! ({} dmg/turn for {} turns)",
                                attacker_name.0, defender_name.0, damage_per_turn, duration
                            )));
                        }
                    }
                }
                OnHitEffect::ApplySlow { duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        commands.entity(msg.defender).insert(Slowed {
                            turns_remaining: *duration,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack slows {}!",
                            attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::ApplyStun { duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        commands.entity(msg.defender).insert(Stunned {
                            turns_remaining: *duration,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack stuns {}!",
                            attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::AttributeDrain { attribute, amount, duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        let defender_e = msg.defender;
                        let attr = attribute.clone();
                        let amt = -*amount; // negative for drain
                        let dur = *duration;
                        commands.queue(move |world: &mut World| {
                            let mut modifiers = world
                                .get_mut::<TimedModifiers>(defender_e)
                                .map(|m| m.clone())
                                .unwrap_or_default();
                            modifiers.entries.push(TimedModifierEntry {
                                attribute: attr.clone(),
                                amount: amt,
                                turns_remaining: dur,
                            });
                            // Recalculate attribute modifiers
                            let mut mods = AttributeModifiers::default();
                            for entry in &modifiers.entries {
                                match entry.attribute.as_str() {
                                    "strength" => mods.strength += entry.amount,
                                    "dexterity" => mods.dexterity += entry.amount,
                                    "constitution" => mods.constitution += entry.amount,
                                    "agility" => mods.agility += entry.amount,
                                    "intelligence" => mods.intelligence += entry.amount,
                                    "perception" => mods.perception += entry.amount,
                                    _ => {}
                                }
                            }
                            if let Ok(mut ec) = world.get_entity_mut(defender_e) {
                                ec.insert(modifiers);
                                ec.insert(mods);
                            }
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s touch drains {}'s {}! (-{} for {} turns)",
                            attacker_name.0, defender_name.0, attribute, amount, duration
                        )));
                    }
                }
                OnHitEffect::LifeDrain { amount, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        heal_writer.write(HealMessage {
                            entity: msg.attacker,
                            amount: *amount,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{} drains {}'s life force! (heals {} HP)",
                            attacker_name.0, defender_name.0, amount
                        )));
                    }
                }
            }
        }
    }
}

// --- Plugin ---

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_on_hit_effects
                .after(crate::game::combat::CombatDamageSet)
                .run_if(in_state(crate::game::AppState::InGame)),
        );
    }
}
