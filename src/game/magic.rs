use bevy::prelude::*;
use std::collections::HashMap;

use crate::{
    assets::SpellRegistryHandle,
    components::Name,
    constants::BASE_ACTION_COST,
    game::{
        actions::ActionFinishedEvent,
        combat::{ApplyDamageMessage, GameRng, HealMessage},
        spells::{SpellEffect, SpellRegistry, roll_dice_expr},
        stats::{CombatStats, Mana},
        turns::TurnEndEvent,
        AppState,
    },
    ui::game_log::GameLogMessage,
};

// --- Components ---

/// Spell IDs the entity has learned.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct KnownSpells {
    pub spells: Vec<String>,
}

/// Active spell slot assignments (index 0 = key 1, …, 5 = key 6).
/// Always `MAX_SPELL_SLOTS` long; unused entries are None.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct ActiveSpells {
    pub slots: Vec<Option<String>>,
}

pub const MAX_SPELL_SLOTS: usize = 6;

impl ActiveSpells {
    pub fn new() -> Self {
        Self {
            slots: vec![None; MAX_SPELL_SLOTS],
        }
    }
}

/// Per-entity spell cooldowns. Key = spell ID, value = turns remaining (0 = ready).
#[derive(Component, Debug, Clone, Default)]
pub struct SpellCooldowns {
    pub cooldowns: HashMap<String, u32>,
}

impl SpellCooldowns {
    pub fn is_ready(&self, spell_id: &str) -> bool {
        self.cooldowns.get(spell_id).copied().unwrap_or(0) == 0
    }

    pub fn set(&mut self, spell_id: &str, turns: u32) {
        if turns > 0 {
            self.cooldowns.insert(spell_id.to_string(), turns);
        }
    }

    pub fn tick(&mut self) {
        for val in self.cooldowns.values_mut() {
            *val = val.saturating_sub(1);
        }
    }
}

// --- Messages ---

/// Cast the spell assigned to `slot` (0-based, maps to keys 1–6).
/// `target` is always fully resolved by the caller before sending:
/// - Damage spells  → the enemy entity
/// - HealCaster spells → the caster entity itself
#[derive(Message, Debug)]
pub struct CastSpellMessage {
    pub caster: Entity,
    pub slot: usize,
    pub target: Entity,
}

// --- Systems ---

/// Pure effect executor — applies spell effects for any entity (player or monster).
/// Target resolution is the caller's responsibility; this system just applies effects.
///
/// For each effect in the spell:
///   - `Damage`     → sends `ApplyDamageMessage` to `msg.target`
///   - `HealCaster` → sends `HealMessage` to `msg.caster`
pub fn handle_cast_spell(
    mut messages: MessageReader<CastSpellMessage>,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
    caster_ro: Query<(&CombatStats, &ActiveSpells, Option<&Name>)>,
    mut caster_resources: Query<(&mut Mana, &mut SpellCooldowns)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut game_rng: ResMut<GameRng>,
) {
    let Some(registry) = spell_registries.get(&spell_registry_handle.0) else {
        return;
    };

    let messages: Vec<(Entity, usize, Entity)> = messages
        .read()
        .map(|m| (m.caster, m.slot, m.target))
        .collect();

    for (caster_entity, slot, target_entity) in messages {
        let Ok((stats, active_spells, caster_name)) = caster_ro.get(caster_entity) else {
            continue;
        };

        let spell_id = match active_spells.slots.get(slot).and_then(|s| s.as_ref()) {
            Some(id) => id.clone(),
            None => {
                log_writer.write(GameLogMessage(format!("No spell in slot {}.", slot + 1)));
                finish_writer.write(ActionFinishedEvent {
                    entity: caster_entity,
                    base_cost: BASE_ACTION_COST,
                });
                continue;
            }
        };

        let Some(spell) = registry.spells.get(&spell_id) else {
            log_writer.write(GameLogMessage(format!("Unknown spell: {}.", spell_id)));
            finish_writer.write(ActionFinishedEvent {
                entity: caster_entity,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        };

        let spell = spell.clone();
        let int_bonus = stats.intelligence_bonus;
        let caster_label = caster_name.map(|n| n.0.clone()).unwrap_or_else(|| "Someone".to_string());

        // Check mana and cooldown before acting.
        {
            let Ok((mana, cooldowns)) = caster_resources.get(caster_entity) else { continue };
            if mana.current < spell.mana_cost {
                log_writer.write(GameLogMessage(format!(
                    "Not enough mana to cast {} ({}/{} MP).",
                    spell.name, mana.current, mana.max
                )));
                finish_writer.write(ActionFinishedEvent {
                    entity: caster_entity,
                    base_cost: BASE_ACTION_COST,
                });
                continue;
            }
            if !cooldowns.is_ready(&spell_id) {
                log_writer.write(GameLogMessage(format!("{} is not ready yet.", spell.name)));
                finish_writer.write(ActionFinishedEvent {
                    entity: caster_entity,
                    base_cost: BASE_ACTION_COST,
                });
                continue;
            }
        }

        // Deduct mana and set cooldown.
        if let Ok((mut mana, mut cooldowns)) = caster_resources.get_mut(caster_entity) {
            mana.current -= spell.mana_cost;
            cooldowns.set(&spell_id, spell.cooldown);
        }

        log_writer.write(GameLogMessage(format!("{} casts {}!", caster_label, spell.name)));

        // Apply each effect via messages. Damage → ApplyDamageMessage, HealCaster → HealMessage.
        for effect in &spell.effects {
            match effect {
                SpellEffect::Damage { dice, int_scaling } => {
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let damage = (roll + bonus).max(1);
                    damage_writer.write(ApplyDamageMessage {
                        attacker: caster_entity,
                        target: target_entity,
                        final_damage: damage,
                    });
                }
                SpellEffect::HealCaster { dice, int_scaling } => {
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let amount = (roll + bonus).max(1);
                    heal_writer.write(HealMessage {
                        entity: caster_entity,
                        amount,
                    });
                }
            }
        }

        finish_writer.write(ActionFinishedEvent {
            entity: caster_entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}

/// On each full turn cycle, regenerate mana for all entities that have it.
pub fn mana_regen_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut mana_query: Query<(&mut Mana, &CombatStats)>,
) {
    for _ in turn_end.read() {
        for (mut mana, stats) in mana_query.iter_mut() {
            let regen = (stats.intelligence_bonus + 1).max(1);
            mana.current = (mana.current + regen).min(mana.max);
        }
    }
}

/// Decrements all spell cooldowns by 1 each full turn cycle.
pub fn tick_cooldowns_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut cooldown_query: Query<&mut SpellCooldowns>,
) {
    for _ in turn_end.read() {
        for mut cooldowns in cooldown_query.iter_mut() {
            cooldowns.tick();
        }
    }
}

// --- Plugin ---

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<KnownSpells>()
            .register_type::<ActiveSpells>()
            .add_message::<CastSpellMessage>()
            .add_systems(
                Update,
                (mana_regen_system, tick_cooldowns_system).run_if(in_state(AppState::InGame)),
            );
    }
}
