use crate::{
    assets::SpellRegistryHandle,
    components::{Monster, Position, Viewshed},
    game::{
        actions::{Direction, MovementIntent, RangedAttackIntent, WaitIntent},
        combat::Health,
        magic::{ActiveSpells, CastSpellMessage, Hasted, Poisoned, Slowed, SpellCooldowns},
        ranged::RangedCapable,
        spells::{SpellEffect, SpellRegistry},
        stats::{CombatStats, Mana},
    },
    map::{Map, tile::is_walkable},
    player::Player,
};
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, DistanceAlg, Point, a_star_search};
use rand::rng;
use rand::seq::SliceRandom;

#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

pub trait ActorAI: Send + Sync {
    /// AI now directly sends events to the world instead of returning an Action enum.
    fn execute(&mut self, entity: Entity, world: &mut World);
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
enum MonsterAIMode {
    #[default]
    Asleep,
    Hunting,
    Wandering,
}

#[derive(Default, Component)]
pub struct MonsterAI {
    mode: MonsterAIMode,
    last_known_player_position: Option<Point>,
}

impl MonsterAI {
    pub fn execute(&mut self, entity: Entity, world: &mut World) {
        let mut rng = rng();

        // --- STEP 1: READ-ONLY DATA EXTRACTION ---
        let (monster_pos, monster_viewshed, player_point, player_entity) = {
            let m_pos = world.get::<Position>(entity).map(|p| p.to_point());
            let m_view = world.get::<Viewshed>(entity).cloned().unwrap_or_default();

            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let (p_entity, p_pt) = player_query
                .iter(world)
                .next()
                .map(|(e, p)| (Some(e), Some(p.to_point())))
                .unwrap_or((None, None));

            (m_pos, m_view, p_pt, p_entity)
        };

        let Some(monster_pos) = monster_pos else { return };
        let Some(player_point) = player_point else {
            world.write_message(WaitIntent { entity });
            return;
        };

        let is_player_visible = monster_viewshed.visible_tiles.contains(&player_point);

        // --- STEP 2: STATE LOGIC ---
        match self.mode {
            MonsterAIMode::Asleep => {
                if is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
            MonsterAIMode::Hunting => {
                if is_player_visible {
                    self.last_known_player_position = Some(player_point);
                }
                if !is_player_visible && Some(monster_pos) == self.last_known_player_position {
                    self.mode = MonsterAIMode::Wandering;
                    self.last_known_player_position = None;
                }
            }
            MonsterAIMode::Wandering => {
                if is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
        }

        // --- STEP 2.5: TRY TO CAST A SPELL ---
        // Only attempt if hunting and player is visible.
        if self.mode == MonsterAIMode::Hunting && is_player_visible {
            if let Some((spell_slot, target)) = choose_spell(entity, monster_pos, player_entity, world) {
                world.write_message(CastSpellMessage {
                    caster: entity,
                    slot: spell_slot,
                    target,
                });
                return;
            }
        }

        // --- STEP 2.6: RANGED ATTACK ---
        // Only when hunting, player is visible, and the monster has a ranged capability.
        if self.mode == MonsterAIMode::Hunting && is_player_visible {
            if let Some(ranged_capable) = world.get::<RangedCapable>(entity) {
                let range = ranged_capable.range;
                let dist = bracket_lib::prelude::DistanceAlg::Pythagoras
                    .distance2d(monster_pos, player_point);
                // Use ranged attack if player is in range but NOT adjacent (prefer melee if right next to them).
                if dist > 1.5 && dist <= range as f32 {
                    if let Some(p_entity) = player_entity {
                        world.write_message(RangedAttackIntent {
                            attacker: entity,
                            target: p_entity,
                        });
                        return;
                    }
                }
            }
        }

        // --- STEP 3: PATHFINDING AND MOVEMENT ---
        let intent_to_send = {
            let map = world.resource::<Map>();

            match self.mode {
                MonsterAIMode::Hunting => {
                    if let Some(target) = self.last_known_player_position {
                        let path = a_star_search(
                            map.point2d_to_index(monster_pos),
                            map.point2d_to_index(target),
                            map,
                        );

                        if path.success && path.steps.len() > 1 {
                            let next_step = map.index_to_point2d(path.steps[1]);
                            let dir = Direction::from_pos(
                                &Position::from_point(monster_pos),
                                &Position::from_point(next_step),
                            );
                            Some(MovementIntent { entity, dir })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                MonsterAIMode::Wandering => {
                    let mut directions = Direction::ALL.to_vec();
                    directions.shuffle(&mut rng);

                    let mut chosen_dir = None;
                    for dir in directions {
                        let target = monster_pos + dir.offset();
                        if map.in_bounds(target)
                            && is_walkable(map.tiles[map.xy_idx(target.x, target.y)])
                        {
                            chosen_dir = Some(dir);
                            break;
                        }
                    }
                    chosen_dir.map(|dir| MovementIntent { entity, dir })
                }
                _ => None,
            }
        };

        if let Some(intent) = intent_to_send {
            world.write_message(intent);
        } else {
            world.write_message(WaitIntent { entity });
        }
    }
}

/// Evaluates all ready spells for `caster` and returns `(slot_index, target_entity)` for
/// the best one, or `None` if no spell is worth casting.
///
/// The target entity is fully resolved here — the spell handler receives it as-is.
///
/// Score normalization prevents nova-ing:
///   effective_score = raw_score / (sqrt(mana_cost) * ln(cooldown + 1))
fn choose_spell(
    caster: Entity,
    caster_pos: Point,
    player_entity: Option<Entity>,
    world: &mut World,
) -> Option<(usize, Entity)> {
    // --- Gather all data upfront to avoid borrow issues ---
    let (active_slots, cooldowns, mana_current, caster_hp, caster_max_hp, int_bonus) = {
        let active = world.get::<ActiveSpells>(caster)?;
        let cooldowns = world.get::<SpellCooldowns>(caster).cloned().unwrap_or_default();
        let mana = world.get::<Mana>(caster).map(|m| m.current).unwrap_or(0);
        let hp = world.get::<Health>(caster).map(|h| (h.current, h.max)).unwrap_or((1, 1));
        let int_bonus = world
            .get::<CombatStats>(caster)
            .map(|s| s.intelligence_bonus)
            .unwrap_or(0);
        (active.slots.clone(), cooldowns, mana, hp.0, hp.1, int_bonus)
    };

    let player_pos = player_entity
        .and_then(|e| world.get::<Position>(e))
        .map(|p| p.to_point());

    let player_hp = player_entity
        .and_then(|e| world.get::<Health>(e))
        .map(|h| h.current)
        .unwrap_or(30);

    let player_mana = player_entity
        .and_then(|e| world.get::<Mana>(e))
        .map(|m| m.current)
        .unwrap_or(0);

    let caster_has_haste = world.get::<Hasted>(caster).is_some();
    let _caster_has_slow = world.get::<Slowed>(caster).is_some();
    let player_has_poison = player_entity
        .map(|e| world.get::<Poisoned>(e).is_some())
        .unwrap_or(false);
    let player_has_slow = player_entity
        .map(|e| world.get::<Slowed>(e).is_some())
        .unwrap_or(false);

    let hp_pct = caster_hp as f32 / caster_max_hp.max(1) as f32;

    // Collect nearby entity positions for AoE scoring.
    // (entity, position, is_enemy)
    let nearby_entities: Vec<(Entity, Point, bool)> = {
        let mut result = Vec::new();
        let mut query = world.query::<(Entity, &Position, Option<&Player>, Option<&Monster>)>();
        for (ent, pos, is_player, is_monster) in query.iter(world) {
            if ent == caster {
                continue;
            }
            let is_enemy = is_player.is_some(); // From monster's perspective, player = enemy
            let _ = is_monster; // Allies are other monsters
            result.push((ent, pos.to_point(), is_enemy));
        }
        result
    };

    let registry_handle = world.resource::<SpellRegistryHandle>().0.clone();
    let registry = {
        let assets = world.resource::<Assets<SpellRegistry>>();
        assets.get(&registry_handle).cloned()
    };
    let Some(registry) = registry else {
        return None;
    };

    let mut best_score: f32 = 0.0;
    let mut best_slot: Option<usize> = None;
    let mut best_target: Option<Entity> = None;

    for (slot_idx, slot) in active_slots.iter().enumerate() {
        let Some(spell_id) = slot else { continue };
        let Some(spell) = registry.spells.get(spell_id) else {
            continue;
        };

        if !cooldowns.is_ready(spell_id) {
            continue;
        }
        if mana_current < spell.mana_cost {
            continue;
        }

        // Range check helper (used by multiple effect types).
        let player_in_range = if spell.range > 0 {
            player_pos
                .map(|ppos| DistanceAlg::Pythagoras.distance2d(caster_pos, ppos) <= spell.range as f32)
                .unwrap_or(false)
        } else {
            true
        };

        // Score each effect and accumulate.
        let mut raw: i32 = 0;
        let mut target: Option<Entity> = None;

        for effect in &spell.effects {
            match effect {
                SpellEffect::Damage { dice, int_scaling } => {
                    if !player_in_range {
                        continue;
                    }
                    let avg = avg_dice(dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let damage = (avg + bonus).max(1).min(player_hp);
                    raw += damage;
                    target = target.or(player_entity);
                }
                SpellEffect::Heal { dice, int_scaling } => {
                    let avg = avg_dice(dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let heal = (avg + bonus).max(1);
                    let missing_hp = caster_max_hp - caster_hp;
                    let score = if missing_hp <= 0 {
                        0
                    } else {
                        heal.min(missing_hp) * 2
                    };
                    raw += score;
                    target = target.or(Some(caster));
                }
                SpellEffect::AoeDamage {
                    dice,
                    radius,
                    int_scaling,
                } => {
                    if !player_in_range {
                        continue;
                    }
                    // Score based on exact enemy/ally count around target (player) position.
                    if let Some(ppos) = player_pos {
                        let avg = avg_dice(dice);
                        let bonus = if *int_scaling { int_bonus } else { 0 };
                        let single_damage = (avg + bonus).max(1);

                        let mut enemy_count = 1i32; // The player themselves
                        let mut ally_count = 0i32;
                        for (_, npos, is_enemy) in &nearby_entities {
                            let dist = (npos.x - ppos.x).abs() + (npos.y - ppos.y).abs();
                            if dist <= *radius {
                                if *is_enemy {
                                    enemy_count += 1;
                                } else {
                                    ally_count += 1;
                                }
                            }
                        }
                        // Also check if caster is in the blast
                        let caster_dist =
                            (caster_pos.x - ppos.x).abs() + (caster_pos.y - ppos.y).abs();
                        if caster_dist <= *radius {
                            ally_count += 1; // Don't fireball yourself
                        }

                        let score = single_damage * enemy_count - single_damage * ally_count;
                        if score > 0 {
                            raw += score;
                            target = target.or(player_entity);
                        }
                    }
                }
                SpellEffect::ChainDamage {
                    dice,
                    max_jumps,
                    int_scaling,
                    ..
                } => {
                    if !player_in_range {
                        continue;
                    }
                    let avg = avg_dice(dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let primary_damage = (avg + bonus).max(1).min(player_hp);
                    // Assume jump damage averages 1d6 = 3.5 per jump.
                    // Count nearby enemies that could be hit by jumps.
                    let jump_damage = 4; // ~1d6 average
                    let mut jump_targets = 0i32;
                    if let Some(ppos) = player_pos {
                        for (_, npos, is_enemy) in &nearby_entities {
                            if *is_enemy {
                                let dist = (npos.x - ppos.x).abs() + (npos.y - ppos.y).abs();
                                if dist <= 3 {
                                    // within reasonable jump range
                                    jump_targets += 1;
                                }
                            }
                        }
                    }
                    let actual_jumps = jump_targets.min(*max_jumps);
                    raw += primary_damage + (jump_damage + bonus.max(0)) * actual_jumps;
                    target = target.or(player_entity);
                }
                SpellEffect::Buff {
                    amount, duration, ..
                } => {
                    let score = *amount * (*duration as i32) / 4;
                    raw += score;
                    target = target.or(Some(caster));
                }
                SpellEffect::Debuff {
                    amount, duration, ..
                } => {
                    if !player_in_range {
                        continue;
                    }
                    let score = *amount * (*duration as i32) / 4;
                    raw += score;
                    target = target.or(player_entity);
                }
                SpellEffect::ApplyPoison {
                    damage_per_turn,
                    duration,
                } => {
                    if !player_in_range || player_has_poison {
                        continue;
                    }
                    raw += damage_per_turn * (*duration as i32);
                    target = target.or(player_entity);
                }
                SpellEffect::ApplyHaste { .. } => {
                    if caster_has_haste {
                        continue; // Already hasted, don't waste it
                    }
                    raw += 15; // High fixed value — speed is very powerful
                    target = target.or(Some(caster));
                }
                SpellEffect::ApplySlow { .. } => {
                    if !player_in_range || player_has_slow {
                        continue;
                    }
                    raw += 12;
                    target = target.or(player_entity);
                }
                SpellEffect::DrainMana { amount, int_scaling } => {
                    if !player_in_range || player_mana <= 0 {
                        continue;
                    }
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let drain = (*amount + bonus).max(0).min(player_mana);
                    raw += drain;
                    target = target.or(player_entity);
                }
                SpellEffect::SpiritShield { .. } => {
                    // More valuable when hurt
                    let score = if hp_pct < 0.5 { 10 } else { 3 };
                    raw += score;
                    target = target.or(Some(caster));
                }
                SpellEffect::Teleport { .. } => {
                    // Monsters generally shouldn't teleport
                    // Score 0 — skip
                }
            }
        }

        // Both raw score and a resolved target are required.
        let Some(target) = target else { continue };
        if raw <= 0 {
            continue;
        }

        let mana_weight = (spell.mana_cost as f32).sqrt().max(1.0);
        let cd_weight = ((spell.cooldown as f32) + 1.0).ln().max(1.0);
        let effective = raw as f32 / (mana_weight * cd_weight);

        if effective > best_score {
            best_score = effective;
            best_slot = Some(slot_idx);
            best_target = Some(target);
        }
    }

    if best_score > 1.0 {
        best_slot.zip(best_target)
    } else {
        None
    }
}

/// Returns the average roll for a "NdM" dice expression.
fn avg_dice(expr: &str) -> i32 {
    let parts: Vec<&str> = expr.split('d').collect();
    if parts.len() != 2 { return 0 }
    let n = parts[0].parse::<i32>().unwrap_or(1);
    let m = parts[1].parse::<i32>().unwrap_or(6);
    // Average of NdM = N * (M + 1) / 2
    n * (m + 1) / 2
}
