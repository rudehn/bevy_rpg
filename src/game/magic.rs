use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    assets::SpellRegistryHandle,
    components::{Name, Position},
    constants::BASE_ACTION_COST,
    game::{
        actions::ActionFinishedEvent,
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
    pub fn new() -> Self {
        Self {
            slots: vec![None; MAX_SPELL_SLOTS],
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

/// +50% speed (delay x 0.5) for N turns.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Hasted {
    pub turns_remaining: u32,
}

/// -50% speed (delay x 1.5) for N turns.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Slowed {
    pub turns_remaining: u32,
}

/// Stunned: entity skips its turn.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Stunned {
    pub turns_remaining: u32,
}

/// Fire damage-over-time effect.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Burning {
    pub damage_per_turn: i32,
    pub turns_remaining: u32,
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
pub fn handle_cast_spell(
    mut commands: Commands,
    mut messages: MessageReader<CastSpellMessage>,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
    caster_ro: Query<(&ActiveSpells, Option<&Name>)>,
    mut caster_resources: Query<(&mut Mana, &mut SpellCooldowns)>,
    positions: Query<&Position>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut particle_writer: MessageWriter<ParticleRequest>,
    mut game_rng: ResMut<GameRng>,
    all_positions: Query<(Entity, &Position)>,
    map: Res<Map>,
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

        let spell_damage_type = spell.damage_type;

        // Apply each effect.
        for effect in &spell.effects {
            match effect {
                SpellEffect::Damage { dice, .. } => {
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let damage = roll.max(1);
                    damage_writer.write(ApplyDamageMessage {
                        attacker: caster_entity,
                        target: target_entity,
                        final_damage: damage,
                        damage_type: spell_damage_type,
                        source: DamageSource::Spell,
                    });
                    if let (Ok(caster_pos), Ok(target_pos_c)) =
                        (positions.get(caster_entity), positions.get(target_entity))
                    {
                        let src = (caster_pos.x, caster_pos.y);
                        let dst = (target_pos_c.x, target_pos_c.y);
                        match spell_damage_type {
                            DamageType::Fire => { particle_writer.write(ParticleRequest::fire_bolt(src, dst)); },
                            DamageType::Lightning => { particle_writer.write(ParticleRequest::lightning(src, dst)); },
                            _ => { particle_writer.write(ParticleRequest::spell(
                                grid_to_world_center(src.0, src.1),
                                grid_to_world_center(dst.0, dst.1),
                                damage_type_color(spell_damage_type),
                            )); },
                        }
                    }
                }
                SpellEffect::Heal { dice, .. } => {
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let amount = roll.max(1);
                    heal_writer.write(HealMessage {
                        entity: target_entity,
                        amount,
                    });
                }
                SpellEffect::AoeDamage {
                    dice,
                    radius,
                    ..
                } => {
                    let target_pos_result = positions.get(target_entity).map(|p| (p.x, p.y));
                    if let Ok((cx, cy)) = target_pos_result {
                        let mut hit_count = 0;
                        for (ent, pos) in all_positions.iter() {
                            let dist = (pos.x - cx).abs() + (pos.y - cy).abs();
                            if dist <= *radius {
                                let roll = roll_dice_expr(&mut game_rng.0, dice);
                                let damage = roll.max(1);
                                damage_writer.write(ApplyDamageMessage {
                                    attacker: caster_entity,
                                    target: ent,
                                    final_damage: damage,
                                    damage_type: spell_damage_type,
                                    source: DamageSource::Spell,
                                });
                                hit_count += 1;
                            }
                        }
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

                        if let Ok(caster_pos) = positions.get(caster_entity) {
                            let src = (caster_pos.x, caster_pos.y);
                            let center = (cx, cy);
                            match spell_damage_type {
                                DamageType::Fire => {
                                    particle_writer.write(ParticleRequest::fire_bolt(src, center));
                                    for dx in -radius..=*radius {
                                        for dy in -radius..=*radius {
                                            if dx.abs() + dy.abs() <= *radius {
                                                particle_writer.write(ParticleRequest::fire_impact((cx + dx, cy + dy)));
                                            }
                                        }
                                    }
                                }
                                DamageType::Lightning => {
                                    particle_writer.write(ParticleRequest::lightning(src, center));
                                    for dx in -radius..=*radius {
                                        for dy in -radius..=*radius {
                                            if dx.abs() + dy.abs() <= *radius {
                                                particle_writer.write(ParticleRequest::lightning_impact((cx + dx, cy + dy)));
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    let color = damage_type_color(spell_damage_type);
                                    particle_writer.write(ParticleRequest::spell(
                                        grid_to_world_center(src.0, src.1),
                                        grid_to_world_center(center.0, center.1),
                                        color,
                                    ));
                                }
                            }
                        }
                    }
                }
                SpellEffect::ChainDamage {
                    dice,
                    max_jumps,
                    jump_range,
                    ..
                } => {
                    let roll = roll_dice_expr(&mut game_rng.0, dice);
                    let primary_damage = roll.max(1);
                    damage_writer.write(ApplyDamageMessage {
                        attacker: caster_entity,
                        target: target_entity,
                        final_damage: primary_damage,
                        damage_type: spell_damage_type,
                        source: DamageSource::Spell,
                    });

                    if let (Ok(caster_pos), Ok(target_pos_c)) =
                        (positions.get(caster_entity), positions.get(target_entity))
                    {
                        particle_writer.write(ParticleRequest::lightning(
                            (caster_pos.x, caster_pos.y),
                            (target_pos_c.x, target_pos_c.y),
                        ));
                    }

                    let mut hit_entities = vec![target_entity, caster_entity];
                    let mut last_pos = positions
                        .get(target_entity)
                        .map(|p| (p.x, p.y))
                        .unwrap_or((0, 0));

                    for _ in 0..*max_jumps {
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
                            let next_pos = positions
                                .get(next_ent)
                                .map(|p| (p.x, p.y))
                                .unwrap_or(last_pos);
                            particle_writer.write(ParticleRequest::lightning(last_pos, next_pos));

                            let jump_roll = game_rng.0.roll_dice(1, 6);
                            let jump_damage = jump_roll.max(1);
                            damage_writer.write(ApplyDamageMessage {
                                attacker: caster_entity,
                                target: next_ent,
                                final_damage: jump_damage,
                                damage_type: spell_damage_type,
                                source: DamageSource::Spell,
                            });
                            last_pos = next_pos;
                            hit_entities.push(next_ent);
                            log_writer.write(GameLogMessage(
                                "Lightning arcs to another target!".to_string(),
                            ));
                        } else {
                            break;
                        }
                    }
                }
                SpellEffect::Buff { .. } | SpellEffect::Debuff { .. } => {
                    // Buff/Debuff no longer modifies attributes — log only
                    log_writer.write(GameLogMessage("The magical energy fizzles without effect.".to_string()));
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
                    ..
                } => {
                    let drain = (*amount).max(0);
                    let caster_e = caster_entity;
                    let target_e = target_entity;
                    let label = caster_label.clone();
                    commands.queue(move |world: &mut World| {
                        let actual_drain = {
                            if let Some(mut target_mana) = world.get_mut::<Mana>(target_e) {
                                let actual = drain.min(target_mana.current);
                                target_mana.current -= actual;
                                actual
                            } else {
                                0
                            }
                        };
                        if actual_drain > 0 {
                            if let Some(mut caster_mana) = world.get_mut::<Mana>(caster_e) {
                                caster_mana.current =
                                    (caster_mana.current + actual_drain).min(caster_mana.max);
                            }
                        }
                        world.write_message(GameLogMessage(format!(
                            "{} drains {} mana!",
                            label, actual_drain
                        )));
                    });
                }
                SpellEffect::SpiritShield { .. } => {
                    // Spirit Shield removed — log only
                    log_writer.write(GameLogMessage("The spirit shield spell has no effect.".to_string()));
                }
                SpellEffect::Teleport { range } => {
                    if *range == 0 {
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
                    } else if let Some((tx, ty)) = target_pos {
                        commands
                            .entity(caster_entity)
                            .insert(Position { x: tx, y: ty });
                        log_writer.write(GameLogMessage(format!(
                            "{} blinks to ({}, {})!",
                            caster_label, tx, ty
                        )));
                    }
                }
                SpellEffect::ApplyEnrage { duration } => {
                    commands.entity(target_entity).insert(
                        crate::game::abilities::Enraged { turns_remaining: *duration }
                    );
                    log_writer.write(GameLogMessage(format!(
                        "{} enters a rage! (+50% damage for {} turns)",
                        caster_label, duration
                    )));
                }
                SpellEffect::SummonAlly { monster, count } => {
                    // Queue a pending summon — processed by a separate system
                    // to avoid exceeding the system parameter limit.
                    let caster_pos = if let Ok(pos) = positions.get(caster_entity) {
                        *pos
                    } else {
                        continue;
                    };
                    commands.insert_resource(PendingSummon {
                        caster_pos,
                        caster_label: caster_label.clone(),
                        monster_name: monster.clone(),
                        count: *count,
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

/// Apply haste/slow speed multipliers.
pub fn apply_speed_effects_system(
    mut query: Query<
        (
            &mut crate::game::actions::SpeedStats,
            Option<&Hasted>,
            Option<&Slowed>,
        ),
    >,
) {
    for (mut speed, hasted, slowed) in query.iter_mut() {
        let mut delay = 1.0f32;
        if hasted.is_some() {
            delay *= 0.5;
        }
        if slowed.is_some() {
            delay *= 1.5;
        }
        speed.delay = delay.clamp(0.5, 2.0);
    }
}

/// Tick stun duration: decrement, remove when expired.
pub fn tick_stunned_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Stunned)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    names: Query<&Name>,
) {
    for _ in turn_end.read() {
        for (entity, mut stunned) in query.iter_mut() {
            stunned.turns_remaining = stunned.turns_remaining.saturating_sub(1);
            if stunned.turns_remaining == 0 {
                commands.entity(entity).remove::<Stunned>();
                if let Ok(name) = names.get(entity) {
                    log_writer.write(GameLogMessage(format!(
                        "{} is no longer stunned.",
                        name.0
                    )));
                }
            }
        }
    }
}

/// Process burning damage each turn.
pub fn process_burning_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Burning, &Name)>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in turn_end.read() {
        for (entity, mut burning, name) in query.iter_mut() {
            log_writer.write(GameLogMessage(format!(
                "{} takes {} fire damage from burning!",
                name.0, burning.damage_per_turn
            )));
            damage_writer.write(ApplyDamageMessage {
                attacker: entity,
                target: entity,
                final_damage: burning.damage_per_turn,
                damage_type: DamageType::Fire,
                source: DamageSource::Environment,
            });
            burning.turns_remaining = burning.turns_remaining.saturating_sub(1);
            if burning.turns_remaining == 0 {
                commands.entity(entity).remove::<Burning>();
                log_writer.write(GameLogMessage(format!(
                    "{} is no longer burning.",
                    name.0
                )));
            }
        }
    }
}

// =====================================================================
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
            .register_type::<Hasted>()
            .register_type::<Slowed>()
            .register_type::<Stunned>()
            .register_type::<Burning>()
            .add_message::<CastSpellMessage>()
            .add_systems(
                Update,
                (
                    mana_regen_system,
                    tick_cooldowns_system,
                    tick_speed_effects_system,
                    tick_stunned_system,
                    process_burning_system,
                    apply_speed_effects_system,
                    process_pending_summon,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
