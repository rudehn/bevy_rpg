use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    assets::SpellRegistryHandle,
    components::{Name, Position},
    constants::BASE_ACTION_COST,
    game::{
        actions::{ActionFinishedEvent, finish_turn},
        combat::{ApplyDamageMessage, DamageSource, DamageType, GameRng, HealMessage},
        particles::{ParticleRequest, damage_type_color, grid_to_world_center},
        spells::{SpellEffect, SpellRegistry, roll_dice_expr},
        stats::Mana,
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
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct KnownSpells {
    pub spells: Vec<String>,
}

/// Active spell slot assignments (index 0 = key 1, ..., 5 = key 6).
/// Always `MAX_SPELL_SLOTS` long; unused entries are None.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ActiveSpells {
    pub slots: Vec<Option<String>>,
}

pub const MAX_SPELL_SLOTS: usize = 6;

impl ActiveSpells {
    pub fn with_slots(count: usize) -> Self {
        Self {
            slots: vec![None; count.min(MAX_SPELL_SLOTS)],
        }
    }
}

/// Counter-based mana regeneration. Every `turns_between_regen` turns, the entity
/// regenerates 1 mana.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
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
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
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

// =====================================================================
// Unified Status Effects
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum StatusEffectKind {
    Hasted,
    Slowed,
    Stunned,
    Burning { damage_per_turn: i32 },
    SpiritShielded,
    Enraged,
}

impl StatusEffectKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Hasted => "Hasted",
            Self::Slowed => "Slowed",
            Self::Stunned => "Stunned",
            Self::Burning { .. } => "Burning",
            Self::SpiritShielded => "Spirit Shield",
            Self::Enraged => "Enraged",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Hasted => Color::srgb(1.0, 1.0, 0.3),
            Self::Slowed => Color::srgb(0.5, 0.5, 0.9),
            Self::Stunned => Color::srgb(1.0, 1.0, 0.0),
            Self::Burning { .. } => Color::srgb(1.0, 0.5, 0.1),
            Self::SpiritShielded => Color::srgb(0.4, 0.6, 1.0),
            Self::Enraged => Color::srgb(0.9, 0.2, 0.2),
        }
    }

    fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct ActiveStatusEffect {
    pub kind: StatusEffectKind,
    pub turns_remaining: u32,
}

/// Unified container for all status effects on an entity.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct StatusEffects(pub Vec<ActiveStatusEffect>);

impl StatusEffects {
    /// Add or refresh a status effect. If the same kind already exists, takes the longer duration.
    pub fn add(&mut self, kind: StatusEffectKind, turns: u32) {
        if let Some(existing) = self.0.iter_mut().find(|e| e.kind.same_kind(&kind)) {
            existing.turns_remaining = existing.turns_remaining.max(turns);
            if let StatusEffectKind::Burning { damage_per_turn: ref mut old } = existing.kind
                && let StatusEffectKind::Burning { damage_per_turn: new } = kind {
                    *old = (*old).max(new);
                }
        } else {
            self.0.push(ActiveStatusEffect { kind, turns_remaining: turns });
        }
    }

    pub fn remove_kind(&mut self, matcher: impl Fn(&StatusEffectKind) -> bool) {
        self.0.retain(|e| !matcher(&e.kind));
    }

    pub fn is_stunned(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Stunned))
    }

    pub fn is_hasted(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Hasted))
    }

    pub fn is_slowed(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Slowed))
    }

    pub fn is_enraged(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Enraged))
    }

    pub fn has_spirit_shield(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::SpiritShielded))
    }

    pub fn burning_damage(&self) -> Option<i32> {
        self.0.iter().find_map(|e| match e.kind {
            StatusEffectKind::Burning { damage_per_turn } => Some(damage_per_turn),
            _ => None,
        })
    }

    pub fn speed_delay_multiplier(&self) -> f32 {
        let mut delay = 1.0f32;
        if self.is_hasted() { delay *= 0.5; }
        if self.is_slowed() { delay *= 1.5; }
        delay.clamp(0.5, 2.0)
    }

    /// Tick all effects, decrementing turns_remaining. Returns expired effects.
    pub fn tick_all(&mut self) -> Vec<StatusEffectKind> {
        let mut expired = Vec::new();
        self.0.retain_mut(|effect| {
            effect.turns_remaining = effect.turns_remaining.saturating_sub(1);
            if effect.turns_remaining == 0 {
                expired.push(effect.kind);
                false
            } else {
                true
            }
        });
        expired
    }

    /// Returns display entries for UI rendering: (name, color) pairs.
    pub fn display_entries(&self) -> Vec<(&str, Color)> {
        self.0.iter().map(|e| (e.kind.name(), e.kind.color())).collect()
    }
}

// =====================================================================
// Messages
// =====================================================================

/// Cast the spell assigned to `slot` (0-based, maps to keys 1-6).
#[derive(Message, Debug)]
pub struct CastSpellMessage {
    pub caster: Entity,
    pub slot: usize,
    pub target: Entity,
    pub target_pos: Option<(i32, i32)>,
}

// =====================================================================
// Spell Effect Handler
// =====================================================================

/// Pure effect executor -- applies spell effects for any entity (player or monster).
#[allow(clippy::too_many_arguments)]
pub fn handle_cast_spell(
    mut commands: Commands,
    mut messages: MessageReader<CastSpellMessage>,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
    caster_ro: Query<(&ActiveSpells, Option<&Name>)>,
    mut caster_resources: Query<(&mut Mana, &mut SpellCooldowns, &mut crate::game::combat::Health)>,
    positions: Query<&Position>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut particle_writer: MessageWriter<ParticleRequest>,
    mut game_rng: ResMut<GameRng>,
    all_positions: Query<(Entity, &Position)>,
    map: Res<Map>,
    mut status_query: Query<&mut StatusEffects>,
) {
    let Some(registry) = spell_registries.get(&spell_registry_handle.0) else {
        return;
    };

    let messages: Vec<(Entity, usize, Entity, Option<(i32, i32)>)> = messages
        .read()
        .map(|m| (m.caster, m.slot, m.target, m.target_pos))
        .collect();

    for (caster_entity, slot, target_entity, target_pos) in messages {
        let Ok((active_spells, caster_name)) = caster_ro.get(caster_entity) else {
            continue;
        };

        let spell_id = match active_spells.slots.get(slot).and_then(|s| s.as_ref()) {
            Some(id) => id.clone(),
            None => {
                log_writer.write(GameLogMessage(format!("No spell in slot {}.", slot + 1)));
                finish_turn(&mut commands, &mut finish_writer, caster_entity, BASE_ACTION_COST);
                continue;
            }
        };

        let Some(spell) = registry.spells.get(&spell_id) else {
            log_writer.write(GameLogMessage(format!("Unknown spell: {}.", spell_id)));
            finish_turn(&mut commands, &mut finish_writer, caster_entity, BASE_ACTION_COST);
            continue;
        };

        let spell = spell.clone();
        let caster_label = caster_name
            .map(|n| n.0.clone())
            .unwrap_or_else(|| "Someone".to_string());

        // Check mana and cooldown before acting.
        {
            let Ok((mana, cooldowns, _health)) = caster_resources.get(caster_entity) else {
                continue;
            };
            if mana.current < spell.mana_cost {
                log_writer.write(GameLogMessage(format!(
                    "Not enough mana to cast {} ({}/{} MP).",
                    spell.name, mana.current, mana.max
                )));
                finish_turn(&mut commands, &mut finish_writer, caster_entity, BASE_ACTION_COST);
                continue;
            }
            if !cooldowns.is_ready(&spell_id) {
                log_writer.write(GameLogMessage(format!("{} is not ready yet.", spell.name)));
                finish_turn(&mut commands, &mut finish_writer, caster_entity, BASE_ACTION_COST);
                continue;
            }
        }

        // Deduct mana and set cooldown.
        if let Ok((mut mana, mut cooldowns, _)) = caster_resources.get_mut(caster_entity) {
            mana.current -= spell.mana_cost;
            cooldowns.set(&spell_id, spell.cooldown);
        }

        log_writer.write(GameLogMessage(format!(
            "{} casts {}!",
            caster_label, spell.name
        )));

        let spell_damage_type = spell.damage_type;

        // Apply each effect.
        for effect in &spell.effects {
            match effect {
                SpellEffect::Damage { dice, .. } => {
                    effect_damage(caster_entity, target_entity, spell_damage_type, dice,
                        &mut game_rng, &positions, &mut damage_writer, &mut particle_writer);
                }
                SpellEffect::Heal { dice, .. } => {
                    effect_heal(target_entity, dice, &mut game_rng, &mut heal_writer);
                }
                SpellEffect::AoeDamage { dice, radius, .. } => {
                    effect_aoe_damage(caster_entity, target_entity, spell_damage_type, dice, *radius,
                        &mut game_rng, &positions, &all_positions, &map,
                        &mut damage_writer, &mut log_writer, &mut particle_writer);
                }
                SpellEffect::ChainDamage { dice, max_jumps, jump_range, .. } => {
                    effect_chain_damage(caster_entity, target_entity, spell_damage_type, dice, *max_jumps, *jump_range,
                        &mut game_rng, &positions, &all_positions,
                        &mut damage_writer, &mut log_writer, &mut particle_writer);
                }
                SpellEffect::ApplyHaste { duration } => {
                    if let Ok(mut effects) = status_query.get_mut(target_entity) {
                        effect_apply_haste(&mut effects, *duration, &mut log_writer);
                    }
                }
                SpellEffect::ApplySlow { duration } => {
                    if let Ok(mut effects) = status_query.get_mut(target_entity) {
                        effect_apply_slow(&mut effects, *duration, &mut log_writer);
                    }
                }
                SpellEffect::DrainMana { amount, .. } => {
                    effect_drain_mana(&mut commands, caster_entity, target_entity, *amount, &caster_label);
                }
                SpellEffect::SpiritShield { duration } => {
                    if let Ok(mut effects) = status_query.get_mut(caster_entity) {
                        effect_spirit_shield(&mut effects, *duration, &caster_label, &mut log_writer);
                    }
                }
                SpellEffect::Teleport { range } => {
                    effect_teleport(&mut commands, caster_entity, *range, target_pos, &caster_label,
                        &mut game_rng, &map, &mut log_writer);
                }
                SpellEffect::ApplyEnrage { duration } => {
                    if let Ok(mut effects) = status_query.get_mut(target_entity) {
                        effect_apply_enrage(&mut effects, *duration, &caster_label, &mut log_writer);
                    }
                }
                SpellEffect::SummonAlly { monster, count } => {
                    if let Ok(pos) = positions.get(caster_entity) {
                        commands.insert_resource(PendingSummon {
                            caster_pos: *pos,
                            caster_label: caster_label.clone(),
                            monster_name: monster.clone(),
                            count: *count,
                        });
                    }
                }
            }
        }

        finish_turn(&mut commands, &mut finish_writer, caster_entity, BASE_ACTION_COST);
    }
}

// ---------------------------------------------------------------------------
// Spell effect handlers — one function per SpellEffect variant
// ---------------------------------------------------------------------------

/// Emit a damage-type-appropriate projectile particle from `src` to `dst`.
fn emit_spell_particle(
    particle_writer: &mut MessageWriter<ParticleRequest>,
    src: (i32, i32),
    dst: (i32, i32),
    damage_type: DamageType,
) {
    match damage_type {
        DamageType::Fire => { particle_writer.write(ParticleRequest::fire_bolt(src, dst)); },
        DamageType::Lightning => { particle_writer.write(ParticleRequest::lightning(src, dst)); },
        _ => { particle_writer.write(ParticleRequest::spell(
            grid_to_world_center(src.0, src.1),
            grid_to_world_center(dst.0, dst.1),
            damage_type_color(damage_type),
        )); },
    }
}

fn effect_damage(
    caster: Entity,
    target: Entity,
    damage_type: DamageType,
    dice: &str,
    rng: &mut ResMut<GameRng>,
    positions: &Query<&Position>,
    damage_writer: &mut MessageWriter<ApplyDamageMessage>,
    particle_writer: &mut MessageWriter<ParticleRequest>,
) {
    let damage = roll_dice_expr(&mut rng.0, dice).max(1);
    damage_writer.write(ApplyDamageMessage {
        attacker: caster,
        target,
        final_damage: damage,
        damage_type,
        source: DamageSource::Spell,
    });
    if let (Ok(cp), Ok(tp)) = (positions.get(caster), positions.get(target)) {
        emit_spell_particle(particle_writer, (cp.x, cp.y), (tp.x, tp.y), damage_type);
    }
}

fn effect_heal(
    target: Entity,
    dice: &str,
    rng: &mut ResMut<GameRng>,
    heal_writer: &mut MessageWriter<HealMessage>,
) {
    let amount = roll_dice_expr(&mut rng.0, dice).max(1);
    heal_writer.write(HealMessage { entity: target, amount });
}

#[allow(clippy::too_many_arguments)]
fn effect_aoe_damage(
    caster: Entity,
    target: Entity,
    damage_type: DamageType,
    dice: &str,
    radius: i32,
    rng: &mut ResMut<GameRng>,
    positions: &Query<&Position>,
    all_positions: &Query<(Entity, &Position)>,
    map: &Map,
    damage_writer: &mut MessageWriter<ApplyDamageMessage>,
    log_writer: &mut MessageWriter<GameLogMessage>,
    particle_writer: &mut MessageWriter<ParticleRequest>,
) {
    let Ok(tp) = positions.get(target).map(|p| (p.x, p.y)) else {
        return;
    };
    let (cx, cy) = tp;

    // Hit entities in radius.
    let mut hit_count = 0;
    for (ent, pos) in all_positions.iter() {
        let dist = (pos.x - cx).abs() + (pos.y - cy).abs();
        if dist <= radius {
            let damage = roll_dice_expr(&mut rng.0, dice).max(1);
            damage_writer.write(ApplyDamageMessage {
                attacker: caster, target: ent, final_damage: damage, damage_type, source: DamageSource::Spell,
            });
            hit_count += 1;
        }
    }

    // Check for doors destroyed by the blast.
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            if dx.abs() + dy.abs() <= radius {
                let (tx, ty) = (cx + dx, cy + dy);
                if tx >= 0 && ty >= 0 && tx < map.width() && ty < map.height() {
                    let idx = map.xy_idx(tx, ty);
                    if map.tiles[idx].terrain == crate::map::tile::TerrainType::Door {
                        log_writer.write(GameLogMessage("A door is destroyed by the blast!".to_string()));
                    }
                }
            }
        }
    }
    if hit_count > 0 {
        log_writer.write(GameLogMessage(format!("{} creatures caught in the blast!", hit_count)));
    }

    // Particles: projectile from caster to center, then impacts across radius.
    if let Ok(cp) = positions.get(caster) {
        let src = (cp.x, cp.y);
        emit_spell_particle(particle_writer, src, (cx, cy), damage_type);
        match damage_type {
            DamageType::Fire => {
                for dx in -radius..=radius {
                    for dy in -radius..=radius {
                        if dx.abs() + dy.abs() <= radius {
                            particle_writer.write(ParticleRequest::fire_impact((cx + dx, cy + dy)));
                        }
                    }
                }
            }
            DamageType::Lightning => {
                for dx in -radius..=radius {
                    for dy in -radius..=radius {
                        if dx.abs() + dy.abs() <= radius {
                            particle_writer.write(ParticleRequest::lightning_impact((cx + dx, cy + dy)));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn effect_chain_damage(
    caster: Entity,
    target: Entity,
    damage_type: DamageType,
    dice: &str,
    max_jumps: i32,
    jump_range: i32,
    rng: &mut ResMut<GameRng>,
    positions: &Query<&Position>,
    all_positions: &Query<(Entity, &Position)>,
    damage_writer: &mut MessageWriter<ApplyDamageMessage>,
    log_writer: &mut MessageWriter<GameLogMessage>,
    particle_writer: &mut MessageWriter<ParticleRequest>,
) {
    // Primary hit.
    let primary_damage = roll_dice_expr(&mut rng.0, dice).max(1);
    damage_writer.write(ApplyDamageMessage {
        attacker: caster, target, final_damage: primary_damage, damage_type, source: DamageSource::Spell,
    });
    if let (Ok(cp), Ok(tp)) = (positions.get(caster), positions.get(target)) {
        particle_writer.write(ParticleRequest::lightning((cp.x, cp.y), (tp.x, tp.y)));
    }

    // Chain jumps.
    let mut hit_entities = vec![target, caster];
    let mut last_pos = positions.get(target).map(|p| (p.x, p.y)).unwrap_or((0, 0));

    for _ in 0..max_jumps {
        let best = all_positions.iter()
            .filter(|(ent, _)| !hit_entities.contains(ent))
            .filter_map(|(ent, pos)| {
                let dist = (pos.x - last_pos.0).abs() + (pos.y - last_pos.1).abs();
                (dist <= jump_range).then_some((ent, dist))
            })
            .min_by_key(|&(_, dist)| dist);

        let Some((next_ent, _)) = best else { break };
        let next_pos = positions.get(next_ent).map(|p| (p.x, p.y)).unwrap_or(last_pos);
        particle_writer.write(ParticleRequest::lightning(last_pos, next_pos));

        let jump_damage = rng.0.roll_dice(1, 6).max(1);
        damage_writer.write(ApplyDamageMessage {
            attacker: caster, target: next_ent, final_damage: jump_damage, damage_type, source: DamageSource::Spell,
        });
        last_pos = next_pos;
        hit_entities.push(next_ent);
        log_writer.write(GameLogMessage("Lightning arcs to another target!".to_string()));
    }
}

fn effect_apply_haste(
    effects: &mut StatusEffects,
    duration: u32,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    effects.remove_kind(|k| matches!(k, StatusEffectKind::Slowed));
    effects.add(StatusEffectKind::Hasted, duration);
    log_writer.write(GameLogMessage("Haste granted!".to_string()));
}

fn effect_apply_slow(
    effects: &mut StatusEffects,
    duration: u32,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    effects.remove_kind(|k| matches!(k, StatusEffectKind::Hasted));
    effects.add(StatusEffectKind::Slowed, duration);
    log_writer.write(GameLogMessage("Target is slowed!".to_string()));
}

fn effect_drain_mana(
    commands: &mut Commands,
    caster: Entity,
    target: Entity,
    amount: i32,
    caster_label: &str,
) {
    let drain = amount.max(0);
    let label = caster_label.to_string();
    commands.queue(move |world: &mut World| {
        let actual_drain = {
            if let Some(mut target_mana) = world.get_mut::<Mana>(target) {
                let actual = drain.min(target_mana.current);
                target_mana.current -= actual;
                actual
            } else {
                0
            }
        };
        if actual_drain > 0
            && let Some(mut caster_mana) = world.get_mut::<Mana>(caster) {
                caster_mana.current = (caster_mana.current + actual_drain).min(caster_mana.max);
            }
        world.write_message(GameLogMessage(format!("{} drains {} mana!", label, actual_drain)));
    });
}

fn effect_spirit_shield(
    effects: &mut StatusEffects,
    duration: u32,
    caster_label: &str,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    effects.add(StatusEffectKind::SpiritShielded, duration);
    log_writer.write(GameLogMessage(format!(
        "{} is shielded by spirit energy! (mana absorbs damage for {} turns)",
        caster_label, duration
    )));
}

fn effect_teleport(
    commands: &mut Commands,
    caster: Entity,
    range: i32,
    target_pos: Option<(i32, i32)>,
    caster_label: &str,
    rng: &mut ResMut<GameRng>,
    map: &Map,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    if range == 0 {
        let walkable: Vec<usize> = (0..map.tiles.len())
            .filter(|&idx| crate::map::tile::is_walkable(map.tiles[idx]))
            .collect();
        if !walkable.is_empty() {
            let pick = rng.0.roll_dice(1, walkable.len() as i32) as usize - 1;
            let idx = walkable[pick];
            let (tx, ty) = map.idx_xy(idx);
            commands.entity(caster).insert(Position { x: tx, y: ty });
            log_writer.write(GameLogMessage(format!("{} teleports away!", caster_label)));
        }
    } else if let Some((tx, ty)) = target_pos {
        commands.entity(caster).insert(Position { x: tx, y: ty });
        log_writer.write(GameLogMessage(format!("{} blinks to ({}, {})!", caster_label, tx, ty)));
    }
}

fn effect_apply_enrage(
    effects: &mut StatusEffects,
    duration: u32,
    caster_label: &str,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    effects.add(StatusEffectKind::Enraged, duration);
    log_writer.write(GameLogMessage(format!(
        "{} enters a rage! (+50% damage for {} turns)",
        caster_label, duration
    )));
}

// =====================================================================
// Tick Systems (run on TurnEndEvent)
// =====================================================================

/// Counter-based mana regen: every `turns_between_regen` turns, recover 1 mana.
pub fn mana_regen_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(&mut Mana, &mut ManaRegen)>,
) {
    for _ in turn_end.read() {
        for (mut mana, mut regen) in query.iter_mut() {
            regen.turns_since_last += 1;
            if regen.turns_since_last >= regen.turns_between_regen {
                regen.turns_since_last = 0;
                let amount = 1;
                mana.current = (mana.current + amount).min(mana.max);
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

/// Unified tick system for all status effects via `StatusEffects` component.
pub fn tick_status_effects_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(Entity, &mut StatusEffects, &Name)>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in turn_end.read() {
        for (entity, mut effects, name) in query.iter_mut() {
            // Process burning damage before ticking
            if let Some(dmg) = effects.burning_damage() {
                log_writer.write(GameLogMessage(format!(
                    "{} takes {} fire damage from burning!",
                    name.0, dmg
                )));
                damage_writer.write(ApplyDamageMessage {
                    attacker: entity,
                    target: entity,
                    final_damage: dmg,
                    damage_type: DamageType::Fire,
                    source: DamageSource::Environment,
                });
            }

            let expired = effects.tick_all();
            for kind in expired {
                match kind {
                    StatusEffectKind::Stunned => {
                        log_writer.write(GameLogMessage(format!(
                            "{} is no longer stunned.",
                            name.0
                        )));
                    }
                    StatusEffectKind::SpiritShielded => {
                        log_writer.write(GameLogMessage(format!(
                            "{}'s spirit shield fades.",
                            name.0
                        )));
                    }
                    StatusEffectKind::Burning { .. } => {
                        log_writer.write(GameLogMessage(format!(
                            "{} is no longer burning.",
                            name.0
                        )));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Apply speed multipliers from unified StatusEffects.
pub fn apply_speed_effects_system(
    mut query: Query<(&mut crate::game::actions::SpeedStats, &StatusEffects)>,
) {
    for (mut speed, effects) in query.iter_mut() {
        speed.delay = effects.speed_delay_multiplier();
    }
}

// =====================================================================
// Pending Summon — deferred from handle_cast_spell to avoid param limit
// =====================================================================

/// Resource written by handle_cast_spell, consumed by process_pending_summon.
#[derive(Resource)]
pub struct PendingSummon {
    pub caster_pos: Position,
    pub caster_label: String,
    pub monster_name: String,
    pub count: u32,
}

pub fn process_pending_summon(
    mut commands: Commands,
    pending: Option<Res<PendingSummon>>,
    mut turn_manager: ResMut<crate::game::TurnManager>,
    monster_manifests: Res<Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    map: Res<Map>,
    mut log_writer: MessageWriter<GameLogMessage>,
    positions: Query<&Position>,
) {
    let Some(summon) = pending else { return; };

    let occupied: std::collections::HashSet<(i32, i32)> = positions
        .iter()
        .map(|p| (p.x, p.y))
        .collect();

    let directions = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
    let mut spawn_points = Vec::new();
    for (dx, dy) in &directions {
        let nx = summon.caster_pos.x + dx;
        let ny = summon.caster_pos.y + dy;
        let idx = map.xy_idx(nx, ny);
        if idx < map.tiles.len()
            && crate::map::tile::is_walkable(map.tiles[idx])
            && !occupied.contains(&(nx, ny))
        {
            spawn_points.push(bracket_lib::prelude::Point::new(nx, ny));
            if spawn_points.len() >= summon.count as usize {
                break;
            }
        }
    }

    if !spawn_points.is_empty() {
        let spawned = spawn_points.len();
        for point in spawn_points {
            crate::game::spawner::spawn_monster_by_name(
                &mut commands,
                &summon.monster_name,
                &point,
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
                None,
            );
        }
        log_writer.write(GameLogMessage(format!(
            "{} raises {} {}!",
            summon.caster_label, spawned, summon.monster_name
        )));
    }

    commands.remove_resource::<PendingSummon>();
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
            .register_type::<StatusEffects>()
            .register_type::<StatusEffectKind>()
            .register_type::<ActiveStatusEffect>()
            .add_message::<CastSpellMessage>()
            .add_systems(
                Update,
                handle_cast_spell.in_set(crate::game::turns::ProcessingPhase::ResolveActions),
            )
            .add_systems(
                Update,
                (
                    mana_regen_system,
                    tick_cooldowns_system,
                    tick_status_effects_system,
                    apply_speed_effects_system,
                    process_pending_summon,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_effects() -> StatusEffects {
        StatusEffects::default()
    }

    #[test]
    fn add_new_effect() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 5);
        assert!(fx.is_hasted());
        assert_eq!(fx.0.len(), 1);
        assert_eq!(fx.0[0].turns_remaining, 5);
    }

    #[test]
    fn add_refreshes_with_longer_duration() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Stunned, 3);
        fx.add(StatusEffectKind::Stunned, 5);
        assert_eq!(fx.0.len(), 1);
        assert_eq!(fx.0[0].turns_remaining, 5);
    }

    #[test]
    fn add_does_not_shorten_duration() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Stunned, 5);
        fx.add(StatusEffectKind::Stunned, 2);
        assert_eq!(fx.0[0].turns_remaining, 5);
    }

    #[test]
    fn add_burning_takes_higher_damage() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Burning { damage_per_turn: 3 }, 5);
        fx.add(StatusEffectKind::Burning { damage_per_turn: 7 }, 2);
        assert_eq!(fx.0.len(), 1);
        assert_eq!(fx.burning_damage(), Some(7));
        assert_eq!(fx.0[0].turns_remaining, 5); // kept longer duration
    }

    #[test]
    fn remove_kind_removes_matching() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 5);
        fx.add(StatusEffectKind::Slowed, 3);
        fx.remove_kind(|k| matches!(k, StatusEffectKind::Slowed));
        assert!(fx.is_hasted());
        assert!(!fx.is_slowed());
        assert_eq!(fx.0.len(), 1);
    }

    #[test]
    fn is_queries_correct() {
        let mut fx = empty_effects();
        assert!(!fx.is_stunned());
        assert!(!fx.is_hasted());
        assert!(!fx.has_spirit_shield());
        assert!(!fx.is_enraged());

        fx.add(StatusEffectKind::Stunned, 1);
        fx.add(StatusEffectKind::SpiritShielded, 3);
        fx.add(StatusEffectKind::Enraged, 99);

        assert!(fx.is_stunned());
        assert!(fx.has_spirit_shield());
        assert!(fx.is_enraged());
        assert!(!fx.is_hasted());
    }

    #[test]
    fn burning_damage_returns_none_when_absent() {
        let fx = empty_effects();
        assert_eq!(fx.burning_damage(), None);
    }

    #[test]
    fn speed_delay_no_effects() {
        let fx = empty_effects();
        assert!((fx.speed_delay_multiplier() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn speed_delay_hasted() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 5);
        assert!((fx.speed_delay_multiplier() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn speed_delay_slowed() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Slowed, 5);
        assert!((fx.speed_delay_multiplier() - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn speed_delay_hasted_and_slowed_cancel() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 5);
        fx.add(StatusEffectKind::Slowed, 5);
        // 0.5 * 1.5 = 0.75
        assert!((fx.speed_delay_multiplier() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn tick_all_decrements_and_expires() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 2);
        fx.add(StatusEffectKind::Stunned, 1);

        let expired = fx.tick_all();
        assert_eq!(expired.len(), 1);
        assert!(matches!(expired[0], StatusEffectKind::Stunned));
        assert_eq!(fx.0.len(), 1);
        assert!(fx.is_hasted());
        assert_eq!(fx.0[0].turns_remaining, 1);

        let expired = fx.tick_all();
        assert_eq!(expired.len(), 1);
        assert!(matches!(expired[0], StatusEffectKind::Hasted));
        assert!(fx.0.is_empty());
    }

    #[test]
    fn tick_all_empty_is_noop() {
        let mut fx = empty_effects();
        let expired = fx.tick_all();
        assert!(expired.is_empty());
    }

    #[test]
    fn display_entries_returns_all() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 3);
        fx.add(StatusEffectKind::Burning { damage_per_turn: 5 }, 2);
        let entries = fx.display_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "Hasted");
        assert_eq!(entries[1].0, "Burning");
    }

    #[test]
    fn multiple_different_effects_coexist() {
        let mut fx = empty_effects();
        fx.add(StatusEffectKind::Hasted, 5);
        fx.add(StatusEffectKind::Burning { damage_per_turn: 3 }, 3);
        fx.add(StatusEffectKind::SpiritShielded, 4);
        fx.add(StatusEffectKind::Enraged, 99);
        assert_eq!(fx.0.len(), 4);
        assert!(fx.is_hasted());
        assert_eq!(fx.burning_damage(), Some(3));
        assert!(fx.has_spirit_shield());
        assert!(fx.is_enraged());
    }
}
