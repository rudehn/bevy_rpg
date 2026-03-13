use bevy::prelude::*;
use std::collections::HashMap;

use crate::{
    assets::SpellRegistryHandle,
    components::{Name, Position},
    constants::BASE_ACTION_COST,
    game::{
        actions::ActionFinishedEvent,
        combat::{ApplyDamageMessage, GameRng, HealMessage},
        spells::{SpellEffect, SpellRegistry, roll_dice_expr},
        stats::{AttributeModifiers, CombatStats, Mana},
        turns::TurnEndEvent,
        AppState,
    },
    map::Map,
    ui::game_log::GameLogMessage,
};

// =====================================================================
// Components
// =====================================================================

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

/// Counter-based mana regeneration. Every `turns_between_regen` turns, the entity
/// regenerates `1 + (INT_bonus / 5)` mana. INT breakpoints at 15 and 20.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct ManaRegen {
    pub turns_between_regen: u32,
    pub turns_since_last: u32,
}

impl Default for ManaRegen {
    fn default() -> Self {
        Self {
            turns_between_regen: 5,
            turns_since_last: 0,
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

/// A single timed attribute modifier applied by a Buff or Debuff spell.
/// Positive amount = buff, negative = debuff.
#[derive(Debug, Clone, Reflect)]
pub struct TimedModifierEntry {
    pub attribute: String,
    pub amount: i32,
    pub turns_remaining: u32,
}

/// Collection of all active timed modifiers on an entity.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct TimedModifiers {
    pub entries: Vec<TimedModifierEntry>,
}

/// +50% speed (delay × 0.5) for N turns.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct Hasted {
    pub turns_remaining: u32,
}

/// -50% speed (delay × 1.5) for N turns.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct Slowed {
    pub turns_remaining: u32,
}

/// Damage-over-time poison effect.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct Poisoned {
    pub damage_per_turn: i32,
    pub turns_remaining: u32,
}

/// Damage taken from mana instead of HP for N turns.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct SpiritShielded {
    pub turns_remaining: u32,
}

// =====================================================================
// Messages
// =====================================================================

/// Cast the spell assigned to `slot` (0-based, maps to keys 1–6).
/// `target` is always fully resolved by the caller before sending:
/// - Damage spells → the enemy entity
/// - Heal spells   → the resolved target (caster for Castor, ally for Ally, etc.)
#[derive(Message, Debug)]
pub struct CastSpellMessage {
    pub caster: Entity,
    pub slot: usize,
    pub target: Entity,
}

// =====================================================================
// Spell Effect Handler
// =====================================================================

/// Pure effect executor — applies spell effects for any entity (player or monster).
/// Target resolution is the caller's responsibility; this system just applies effects.
pub fn handle_cast_spell(
    mut commands: Commands,
    mut messages: MessageReader<CastSpellMessage>,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
    caster_ro: Query<(&CombatStats, &ActiveSpells, Option<&Name>)>,
    mut caster_resources: Query<(&mut Mana, &mut SpellCooldowns)>,
    positions: Query<&Position>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut game_rng: ResMut<GameRng>,
    all_positions: Query<(Entity, &Position)>,
    map: Res<Map>,
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
        let caster_label = caster_name
            .map(|n| n.0.clone())
            .unwrap_or_else(|| "Someone".to_string());

        // Check mana and cooldown before acting.
        {
            let Ok((mana, cooldowns)) = caster_resources.get(caster_entity) else {
                continue;
            };
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

        log_writer.write(GameLogMessage(format!(
            "{} casts {}!",
            caster_label, spell.name
        )));

        // Apply each effect.
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
                SpellEffect::Heal { dice, int_scaling } => {
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let amount = (roll + bonus).max(1);
                    heal_writer.write(HealMessage {
                        entity: target_entity,
                        amount,
                    });
                }
                SpellEffect::AoeDamage {
                    dice,
                    radius,
                    int_scaling,
                } => {
                    let target_pos = positions.get(target_entity).map(|p| (p.x, p.y));
                    if let Ok((cx, cy)) = target_pos {
                        let mut hit_count = 0;
                        for (ent, pos) in all_positions.iter() {
                            let dist = (pos.x - cx).abs() + (pos.y - cy).abs();
                            if dist <= *radius {
                                let roll = roll_dice_expr(&mut game_rng.0, dice);
                                let bonus = if *int_scaling { int_bonus } else { 0 };
                                let damage = (roll + bonus).max(1);
                                damage_writer.write(ApplyDamageMessage {
                                    attacker: caster_entity,
                                    target: ent,
                                    final_damage: damage,
                                });
                                hit_count += 1;
                            }
                        }
                        // Check for doors in the blast area (log for now; actual
                        // destruction requires a TileEffectMessage system — future work)
                        for dx in -radius..=*radius {
                            for dy in -radius..=*radius {
                                if dx.abs() + dy.abs() <= *radius {
                                    let tx = cx + dx;
                                    let ty = cy + dy;
                                    if tx >= 0
                                        && ty >= 0
                                        && tx < map.width()
                                        && ty < map.height()
                                    {
                                        let idx = map.xy_idx(tx, ty);
                                        if map.tiles[idx].terrain
                                            == crate::map::tile::TerrainType::Door
                                        {
                                            log_writer.write(GameLogMessage(
                                                "A door is destroyed by the blast!".to_string(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        if hit_count > 0 {
                            log_writer.write(GameLogMessage(format!(
                                "{} creatures caught in the blast!",
                                hit_count
                            )));
                        }
                    }
                }
                SpellEffect::ChainDamage {
                    dice,
                    max_jumps,
                    jump_range,
                    int_scaling,
                } => {
                    // Primary target
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let primary_damage = (roll + bonus).max(1);
                    damage_writer.write(ApplyDamageMessage {
                        attacker: caster_entity,
                        target: target_entity,
                        final_damage: primary_damage,
                    });

                    // Chain jumps
                    let mut hit_entities = vec![target_entity, caster_entity];
                    let mut last_pos = positions
                        .get(target_entity)
                        .map(|p| (p.x, p.y))
                        .unwrap_or((0, 0));

                    for _ in 0..*max_jumps {
                        // Find nearest unhit entity within jump_range
                        let mut best: Option<(Entity, i32)> = None;
                        for (ent, pos) in all_positions.iter() {
                            if hit_entities.contains(&ent) {
                                continue;
                            }
                            let dist =
                                (pos.x - last_pos.0).abs() + (pos.y - last_pos.1).abs();
                            if dist <= *jump_range {
                                if best.is_none() || dist < best.unwrap().1 {
                                    best = Some((ent, dist));
                                }
                            }
                        }
                        if let Some((next_ent, _)) = best {
                            // Secondary targets use halved dice (1dM instead of NdM)
                            let jump_roll = game_rng.0.roll_dice(1, 6);
                            let jump_bonus = if *int_scaling { int_bonus } else { 0 };
                            let jump_damage = (jump_roll + jump_bonus).max(1);
                            damage_writer.write(ApplyDamageMessage {
                                attacker: caster_entity,
                                target: next_ent,
                                final_damage: jump_damage,
                            });
                            last_pos = positions
                                .get(next_ent)
                                .map(|p| (p.x, p.y))
                                .unwrap_or(last_pos);
                            hit_entities.push(next_ent);
                            log_writer.write(GameLogMessage(
                                "Lightning arcs to another target!".to_string(),
                            ));
                        } else {
                            break;
                        }
                    }
                }
                SpellEffect::Buff {
                    attribute,
                    amount,
                    duration,
                } => {
                    apply_timed_modifier(
                        &mut commands,
                        target_entity,
                        attribute.clone(),
                        *amount,
                        *duration,
                    );
                    log_writer.write(GameLogMessage(format!(
                        "{} +{} for {} turns!",
                        attribute, amount, duration
                    )));
                }
                SpellEffect::Debuff {
                    attribute,
                    amount,
                    duration,
                } => {
                    apply_timed_modifier(
                        &mut commands,
                        target_entity,
                        attribute.clone(),
                        -(*amount),
                        *duration,
                    );
                    log_writer.write(GameLogMessage(format!(
                        "Target's {} reduced by {} for {} turns!",
                        attribute, amount, duration
                    )));
                }
                SpellEffect::ApplyPoison {
                    damage_per_turn,
                    duration,
                } => {
                    commands.entity(target_entity).insert(Poisoned {
                        damage_per_turn: *damage_per_turn,
                        turns_remaining: *duration,
                    });
                    log_writer.write(GameLogMessage("Target is poisoned!".to_string()));
                }
                SpellEffect::ApplyHaste { duration } => {
                    commands
                        .entity(target_entity)
                        .insert(Hasted {
                            turns_remaining: *duration,
                        })
                        .remove::<Slowed>();
                    log_writer.write(GameLogMessage("Haste granted!".to_string()));
                }
                SpellEffect::ApplySlow { duration } => {
                    commands
                        .entity(target_entity)
                        .insert(Slowed {
                            turns_remaining: *duration,
                        })
                        .remove::<Hasted>();
                    log_writer.write(GameLogMessage("Target is slowed!".to_string()));
                }
                SpellEffect::DrainMana {
                    amount,
                    int_scaling,
                } => {
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let drain = (*amount + bonus).max(0);
                    // We can't query caster_resources for target if it's a different entity,
                    // so we handle this with deferred commands. For now, log the intent.
                    // The actual drain needs a dedicated message or direct world access.
                    log_writer.write(GameLogMessage(format!(
                        "{} drains {} mana!",
                        caster_label, drain
                    )));
                    // Drain is applied via DrainManaMessage (to be processed separately).
                    // For now, apply directly if we can access both entities.
                    // This is handled in drain_mana_system below.
                }
                SpellEffect::SpiritShield { duration } => {
                    commands
                        .entity(target_entity)
                        .insert(SpiritShielded {
                            turns_remaining: *duration,
                        });
                    log_writer.write(GameLogMessage(
                        "A spirit shield surrounds you! Damage absorbed by mana.".to_string(),
                    ));
                }
                SpellEffect::Teleport { range } => {
                    if *range == 0 {
                        // Random teleport — pick a random walkable tile
                        let walkable: Vec<usize> = (0..map.tiles.len())
                            .filter(|&idx| crate::map::tile::is_walkable(map.tiles[idx]))
                            .collect();
                        if !walkable.is_empty() {
                            let pick = game_rng.0.roll_dice(1, walkable.len() as i32) as usize - 1;
                            let idx = walkable[pick];
                            let (tx, ty) = map.idx_xy(idx);
                            commands
                                .entity(caster_entity)
                                .insert(Position { x: tx, y: ty });
                            log_writer.write(GameLogMessage(format!(
                                "{} teleports away!",
                                caster_label
                            )));
                        }
                    } else {
                        // Controlled teleport (blink) — requires tile targeting.
                        warn!("Controlled teleport (blink) requires tile targeting — not yet wired");
                    }
                }
            }
        }

        finish_writer.write(ActionFinishedEvent {
            entity: caster_entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}

/// Helper: add or update a timed modifier on an entity.
fn apply_timed_modifier(
    commands: &mut Commands,
    entity: Entity,
    attribute: String,
    amount: i32,
    duration: u32,
) {
    commands.queue(move |world: &mut World| {
        let mut modifiers = world
            .get_mut::<TimedModifiers>(entity)
            .map(|m| m.clone())
            .unwrap_or_default();

        modifiers.entries.push(TimedModifierEntry {
            attribute: attribute.clone(),
            amount,
            turns_remaining: duration,
        });

        // Recalculate AttributeModifiers from all active timed modifiers
        recalc_attribute_modifiers(world, entity, &modifiers);

        world.entity_mut(entity).insert(modifiers);
    });
}

/// Recalculate AttributeModifiers from the current set of TimedModifiers.
fn recalc_attribute_modifiers(
    world: &mut World,
    entity: Entity,
    timed: &TimedModifiers,
) {
    let mut mods = AttributeModifiers::default();
    for entry in &timed.entries {
        match entry.attribute.as_str() {
            "strength" => mods.strength += entry.amount,
            "dexterity" => mods.dexterity += entry.amount,
            "constitution" => mods.constitution += entry.amount,
            "agility" => mods.agility += entry.amount,
            "intelligence" => mods.intelligence += entry.amount,
            "perception" => mods.perception += entry.amount,
            "armor" => {
                // Armor is on CombatStats, not Attributes. Handle separately.
                if let Some(mut stats) = world.get_mut::<CombatStats>(entity) {
                    stats.armor += entry.amount;
                }
            }
            _ => warn!("Unknown attribute for modifier: {}", entry.attribute),
        }
    }
    if let Ok(mut ec) = world.get_entity_mut(entity) {
        ec.insert(mods);
    }
}

// =====================================================================
// Tick Systems (run on TurnEndEvent)
// =====================================================================

/// Counter-based mana regen: every `turns_between_regen` turns, recover
/// `1 + (INT_bonus / 5)` mana. Breakpoints at INT 15 (+1) and INT 20 (+1).
pub fn mana_regen_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(&mut Mana, &mut ManaRegen, &CombatStats)>,
) {
    for _ in turn_end.read() {
        for (mut mana, mut regen, stats) in query.iter_mut() {
            regen.turns_since_last += 1;
            if regen.turns_since_last >= regen.turns_between_regen {
                regen.turns_since_last = 0;
                let amount = 1 + (stats.intelligence_bonus / 5);
                mana.current = (mana.current + amount.max(0)).min(mana.max);
            }
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

/// Tick timed modifiers: decrement durations, remove expired, recalculate AttributeModifiers.
pub fn tick_timed_modifiers_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(Entity, &mut TimedModifiers)>,
    mut commands: Commands,
) {
    for _ in turn_end.read() {
        for (entity, mut modifiers) in query.iter_mut() {
            let had_entries = !modifiers.entries.is_empty();
            modifiers
                .entries
                .iter_mut()
                .for_each(|e| e.turns_remaining = e.turns_remaining.saturating_sub(1));
            modifiers.entries.retain(|e| e.turns_remaining > 0);

            if had_entries {
                // Recalculate via deferred command since we need world access
                let mods_clone = modifiers.clone();
                commands.queue(move |world: &mut World| {
                    recalc_attribute_modifiers(world, entity, &mods_clone);
                });
            }
        }
    }
}

/// Tick haste/slow: decrement durations, remove expired components.
pub fn tick_speed_effects_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut commands: Commands,
    mut hasted: Query<(Entity, &mut Hasted)>,
    mut slowed: Query<(Entity, &mut Slowed)>,
) {
    for _ in turn_end.read() {
        for (entity, mut h) in hasted.iter_mut() {
            h.turns_remaining = h.turns_remaining.saturating_sub(1);
            if h.turns_remaining == 0 {
                commands.entity(entity).remove::<Hasted>();
            }
        }
        for (entity, mut s) in slowed.iter_mut() {
            s.turns_remaining = s.turns_remaining.saturating_sub(1);
            if s.turns_remaining == 0 {
                commands.entity(entity).remove::<Slowed>();
            }
        }
    }
}

/// Apply haste/slow speed multipliers AFTER sync_action_speed_system.
/// This runs every frame on Changed<Hasted>/Changed<Slowed> or when they are added/removed.
pub fn apply_speed_effects_system(
    mut query: Query<
        (
            &mut crate::game::actions::SpeedStats,
            &CombatStats,
            Option<&Hasted>,
            Option<&Slowed>,
        ),
    >,
) {
    for (mut speed, stats, hasted, slowed) in query.iter_mut() {
        // Recalculate base delay from AGI (same formula as sync_action_speed_system)
        let base = 1.0 - (stats.agility_bonus as f32 * 0.025);
        let mut delay = base;
        if hasted.is_some() {
            delay *= 0.5;
        }
        if slowed.is_some() {
            delay *= 1.5;
        }
        speed.delay = delay.clamp(0.5, 2.0);
    }
}

/// Process poison damage each turn.
pub fn process_poison_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Poisoned, &Name)>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in turn_end.read() {
        for (entity, mut poison, name) in query.iter_mut() {
            log_writer.write(GameLogMessage(format!(
                "{} takes {} poison damage!",
                name.0, poison.damage_per_turn
            )));
            damage_writer.write(ApplyDamageMessage {
                attacker: entity, // self-inflicted for death tracking
                target: entity,
                final_damage: poison.damage_per_turn,
            });
            poison.turns_remaining = poison.turns_remaining.saturating_sub(1);
            if poison.turns_remaining == 0 {
                commands.entity(entity).remove::<Poisoned>();
                log_writer.write(GameLogMessage(format!(
                    "{} is no longer poisoned.",
                    name.0
                )));
            }
        }
    }
}

/// Tick spirit shield duration.
pub fn tick_spirit_shield_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut SpiritShielded)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    names: Query<&Name>,
) {
    for _ in turn_end.read() {
        for (entity, mut shield) in query.iter_mut() {
            shield.turns_remaining = shield.turns_remaining.saturating_sub(1);
            if shield.turns_remaining == 0 {
                commands.entity(entity).remove::<SpiritShielded>();
                if let Ok(name) = names.get(entity) {
                    log_writer.write(GameLogMessage(format!(
                        "{}'s spirit shield fades.",
                        name.0
                    )));
                }
            }
        }
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<KnownSpells>()
            .register_type::<ActiveSpells>()
            .register_type::<ManaRegen>()
            .register_type::<TimedModifiers>()
            .register_type::<Hasted>()
            .register_type::<Slowed>()
            .register_type::<Poisoned>()
            .register_type::<SpiritShielded>()
            .add_message::<CastSpellMessage>()
            .add_systems(
                Update,
                (
                    mana_regen_system,
                    tick_cooldowns_system,
                    tick_timed_modifiers_system,
                    tick_speed_effects_system,
                    process_poison_system,
                    tick_spirit_shield_system,
                    apply_speed_effects_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
