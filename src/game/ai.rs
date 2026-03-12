use crate::{
    assets::SpellRegistryHandle,
    components::{Position, Viewshed},
    game::{
        actions::{Direction, MovementIntent, RangedAttackIntent, WaitIntent},
        combat::{Health},
        magic::{ActiveSpells, CastSpellMessage, SpellCooldowns},
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
/// The target entity is fully resolved here:
///   - `Damage` effects  → `player_entity`
///   - `HealCaster` effects → `caster` itself
///
/// Score normalization prevents nova-ing:
///   effective_score = raw_score / (sqrt(mana_cost) * ln(cooldown + 1))
fn choose_spell(
    caster: Entity,
    caster_pos: Point,
    player_entity: Option<Entity>,
    world: &mut World,
) -> Option<(usize, Entity)> {
    // Extract what we need in a scope to drop borrows.
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

    let registry_handle = world.resource::<SpellRegistryHandle>().0.clone();
    let registry = {
        let assets = world.resource::<Assets<SpellRegistry>>();
        assets.get(&registry_handle).cloned()
    };
    let Some(registry) = registry else { return None };

    let mut best_score: f32 = 0.0;
    let mut best_slot: Option<usize> = None;
    let mut best_target: Option<Entity> = None;

    for (slot_idx, slot) in active_slots.iter().enumerate() {
        let Some(spell_id) = slot else { continue };
        let Some(spell) = registry.spells.get(spell_id) else { continue };

        if !cooldowns.is_ready(spell_id) { continue }
        if mana_current < spell.mana_cost { continue }

        // Score the spell and determine its target entity.
        let (raw, target): (i32, Option<Entity>) = spell.effects.iter().fold(
            (0, None),
            |(acc_score, acc_target), effect| match effect {
                SpellEffect::Damage { dice, int_scaling } => {
                    // Range check: skip if player is out of range (range 0 means unlimited).
                    if spell.range > 0 {
                        if let Some(ppos) = player_pos {
                            let dist_sq = DistanceAlg::Pythagoras.distance2d(caster_pos, ppos);
                            if dist_sq > spell.range as f32 {
                                return (acc_score, acc_target);
                            }
                        } else {
                            return (acc_score, acc_target);
                        }
                    }
                    let avg = avg_dice(dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let damage = (avg + bonus).max(1).min(player_hp);
                    // Damage hits the player (enemy of the monster).
                    (acc_score + damage, acc_target.or(player_entity))
                }
                SpellEffect::HealCaster { dice, int_scaling } => {
                    let avg = avg_dice(dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let heal = (avg + bonus).max(1);
                    let missing_hp = caster_max_hp - caster_hp;
                    let score = if missing_hp <= 0 { 0 } else { heal.min(missing_hp) * 2 };
                    // HealCaster always targets the caster itself.
                    (acc_score + score, acc_target.or(Some(caster)))
                }
            },
        );

        // Both raw score and a resolved target are required.
        let Some(target) = target else { continue };
        if raw <= 0 { continue }

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
