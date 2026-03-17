use crate::{
    assets::SpellRegistryHandle,
    components::{Position, Viewshed},
    game::{
        abilities::{Faction, FactionKind},
        actions::{Direction, MovementIntent, RangedAttackIntent, WaitIntent},
        boss::BossAI,
        combat::Health,
        magic::{ActiveSpells, CastSpellMessage, Hasted, Poisoned, Slowed, SpellCooldowns},
        ranged::RangedCapable,
        spells::{SpellEffect, SpellRegistry, SpellTarget},
        stats::{CombatStats, Mana},
    },
    map::{Map, tile::is_walkable},
    player::Player,
};
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, DistanceAlg, Point, a_star_search};
use rand::rng;
use rand::seq::SliceRandom;

#[allow(dead_code)]
#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

#[allow(dead_code)]
pub trait ActorAI: Send + Sync {
    /// AI now directly sends events to the world instead of returning an Action enum.
    fn execute(&mut self, entity: Entity, world: &mut World);
}

/// Patrol behavior attached to monsters at spawn time.
/// Absence of this component means the monster wanders freely.
/// Coordinates stored as `(i32, i32)` for serde compatibility (bracket-lib Point
/// doesn't derive Serialize/Deserialize in this fork).
#[derive(Component, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatrolRoute {
    pub state: PatrolState,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PatrolState {
    /// Hold position, jitter within GUARD_PATROL_RADIUS of home.
    Sentry { home: (i32, i32) },
    /// Walk waypoints in order, loop continuously.
    Waypoint { points: Vec<(i32, i32)>, current_index: usize },
    /// Random walk constrained to a bounding rectangle.
    AreaRoam { min: (i32, i32), max: (i32, i32) },
}

impl PatrolState {
    pub fn sentry(home: Point) -> Self {
        PatrolState::Sentry { home: (home.x, home.y) }
    }
    pub fn waypoint(points: &[Point]) -> Self {
        PatrolState::Waypoint {
            points: points.iter().map(|p| (p.x, p.y)).collect(),
            current_index: 0,
        }
    }
    pub fn area_roam(min: Point, max: Point) -> Self {
        PatrolState::AreaRoam { min: (min.x, min.y), max: (max.x, max.y) }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
enum MonsterAIMode {
    #[default]
    Asleep,
    Hunting,
    Idle,
}

pub const GUARD_PATROL_RADIUS: i32 = 3;

#[derive(Default, Component)]
pub struct MonsterAI {
    mode: MonsterAIMode,
    last_known_player_position: Option<Point>,
}

impl MonsterAI {
    /// Wake this monster and point it at a target. Transitions from
    /// Asleep or Idle → Hunting; has no effect if already hunting.
    pub fn alert_to_position(&mut self, target: Point) {
        if self.mode == MonsterAIMode::Asleep || self.mode == MonsterAIMode::Idle {
            self.mode = MonsterAIMode::Hunting;
            self.last_known_player_position = Some(target);
        }
    }

    /// Force this monster into Idle mode, clearing its target.
    /// Used for squad scatter on leader death.
    pub fn scatter(&mut self) {
        self.mode = MonsterAIMode::Idle;
        self.last_known_player_position = None;
    }

    /// Returns true if this monster is not asleep (i.e., hunting, wandering, or guarding).
    #[allow(dead_code)]
    pub fn is_alert(&self) -> bool {
        self.mode != MonsterAIMode::Asleep
    }

    pub fn execute(&mut self, entity: Entity, world: &mut World) {
        let mut rng = rng();

        // --- STUN CHECK: stunned entities skip their turn ---
        if world.get::<crate::game::magic::Stunned>(entity).is_some() {
            let name = world
                .get::<crate::components::Name>(entity)
                .map(|n| n.0.clone())
                .unwrap_or_else(|| "Something".to_string());
            world.write_message(crate::ui::game_log::GameLogMessage(format!(
                "{} is stunned and cannot act!", name
            )));
            // Floating "★" particle above the stunned entity
            if let Some(pos) = world.get::<Position>(entity) {
                let world_pos = crate::game::particles::grid_to_world(pos.x, pos.y);
                world.write_message(crate::game::particles::ParticleRequest::FloatingText {
                    world_pos,
                    text: "\u{2605}".to_string(), // ★
                    color: bevy::prelude::Color::srgba(1.0, 1.0, 0.3, 1.0),
                    font_size: 5.0,
                });
            }
            world.write_message(WaitIntent { entity });
            return;
        }

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
                    self.mode = MonsterAIMode::Idle;
                    self.last_known_player_position = None;

                    // Post-hunt resume: snap waypoint patrols to nearest waypoint.
                    if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity) {
                        if let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state {
                            if !points.is_empty() {
                                *current_index = points.iter().enumerate()
                                    .min_by_key(|(_, p)| (p.0 - monster_pos.x).abs() + (p.1 - monster_pos.y).abs())
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                            }
                        }
                    }
                }
            }
            MonsterAIMode::Idle => {
                if is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
        }

        // --- STEP 2.4: COWARDLY FLEE ---
        // Cowardly monsters flee when hurt. Squad members use the group's collective
        // HP ratio against the squad's flee_threshold; solo monsters flee below 50%.
        if world.get::<crate::game::abilities::Cowardly>(entity).is_some() {
            let should_flee = if let Some(squad_id) = world.get::<crate::game::squad::SquadId>(entity) {
                let threshold = world
                    .get::<crate::game::squad::SquadConfig>(entity)
                    .map(|c| c.flee_threshold)
                    .unwrap_or(0.5);
                let (current, max) = crate::game::squad::compute_squad_hp(*squad_id, world);
                max > 0 && (current as f32 / max as f32) < threshold
            } else {
                let health = world.get::<Health>(entity);
                health.map_or(false, |h| h.current < h.max / 2)
            };
            if should_flee {
                // Greedy flee: pick adjacent walkable tile furthest from player
                let map = world.resource::<Map>();
                let mut best_dir: Option<Direction> = None;
                let mut best_dist: f32 = -1.0;

                for dir in Direction::ALL {
                    let target_pos = monster_pos + dir.offset();
                    if map.in_bounds(target_pos)
                        && is_walkable(map.tiles[map.xy_idx(target_pos.x, target_pos.y)])
                    {
                        let dist = DistanceAlg::Pythagoras.distance2d(target_pos, player_point);
                        if dist > best_dist {
                            best_dist = dist;
                            best_dir = Some(dir);
                        }
                    }
                }

                if let Some(dir) = best_dir {
                    let current_dist = DistanceAlg::Pythagoras.distance2d(monster_pos, player_point);
                    // Only flee if the best tile is actually further away
                    if best_dist > current_dist {
                        let name = world
                            .get::<crate::components::Name>(entity)
                            .map(|n| n.0.clone())
                            .unwrap_or_else(|| "Something".to_string());
                        world.write_message(crate::ui::game_log::GameLogMessage(format!(
                            "{} squeaks in fear and flees!", name
                        )));
                        world.write_message(MovementIntent { entity, dir });
                        return;
                    }
                }
                // Cornered: fall through to normal attack logic
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
                    target_pos: None,
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

        // --- STEP 2.8: SQUAD LEADER POSITION ---
        // Non-leader squad members leash to their leader: if they're too far away,
        // they pathfind toward the leader instead of the player. This keeps squads
        // moving as a group through corridors.
        const SQUAD_LEASH_RANGE: f32 = 4.0;
        let leader_leash_target: Option<Point> = {
            use crate::game::squad::{SquadId, SquadLeader};
            let squad_id = world.get::<SquadId>(entity).copied();
            let is_leader = world.get::<SquadLeader>(entity).is_some();
            if let (Some(squad_id), false) = (squad_id, is_leader) {
                // Find our squad's leader position.
                let mut leader_pos = None;
                let mut query = world.query_filtered::<(&SquadId, &Position), With<SquadLeader>>();
                for (sid, pos) in query.iter(world) {
                    if *sid == squad_id {
                        leader_pos = Some(pos.to_point());
                        break;
                    }
                }
                // Only leash if we're far enough from the leader.
                leader_pos.filter(|lp| {
                    DistanceAlg::Pythagoras.distance2d(monster_pos, *lp) > SQUAD_LEASH_RANGE
                })
            } else {
                None
            }
        };

        // --- STEP 3: PATHFINDING AND MOVEMENT ---
        let intent_to_send = {
            let map = world.resource::<Map>();

            match self.mode {
                MonsterAIMode::Hunting => {
                    // Squad followers too far from leader move toward leader instead.
                    let target = leader_leash_target.or(self.last_known_player_position);
                    if let Some(target) = target {
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
                MonsterAIMode::Idle => {
                    // Squad followers move toward their leader.
                    if let Some(target) = leader_leash_target {
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
                        // Dispatch to PatrolRoute-based idle behavior.
                        drop(map);
                        idle_movement(entity, monster_pos, world, &mut rng)
                    }
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

/// Information about a nearby entity, gathered once before spell scoring.
struct NearbyEntity {
    entity: Entity,
    pos: Point,
    faction: FactionKind,
    hp_current: i32,
    hp_max: i32,
    mana_current: i32,
    has_poison: bool,
    has_slow: bool,
    has_haste: bool,
}

/// Spells gated by boss phase. The boss has all spells in its list, but only
/// considers certain ones based on HP-driven phase. Spells NOT in this list
/// (e.g. tier-granted fireball, haste) are always available.
const BOSS_PHASE_SPELLS: &[(&str, u8)] = &[
    ("shadow_bolt", 1),
    ("mana_drain", 1),
    ("heal_self", 1),
    ("chain_lightning", 2),
    ("spirit_shield", 2),
    ("death_coil", 3),
    ("enrage", 3),
];

/// Returns the minimum boss phase required to use a spell, or 0 if ungated.
fn boss_spell_min_phase(spell_id: &str) -> u8 {
    BOSS_PHASE_SPELLS
        .iter()
        .find(|(id, _)| *id == spell_id)
        .map(|(_, phase)| *phase)
        .unwrap_or(0) // Ungated spells (fireball, haste, etc.) always available
}

/// Dispatch idle movement for a monster based on its `PatrolRoute` component.
/// Sentry: jitter near home. Waypoint: walk route. AreaRoam: bounded random walk. None: free wander.
fn idle_movement(
    entity: Entity,
    monster_pos: Point,
    world: &mut World,
    rng: &mut impl rand::Rng,
) -> Option<MovementIntent> {
    let patrol = world.get::<PatrolRoute>(entity).cloned();
    let map = world.resource::<Map>();

    match patrol.as_ref().map(|p| &p.state) {
        Some(PatrolState::Sentry { home }) => {
            let home_pt = Point::new(home.0, home.1);
            let dist = DistanceAlg::Pythagoras.distance2d(monster_pos, home_pt);
            if dist > GUARD_PATROL_RADIUS as f32 {
                let path = a_star_search(
                    map.point2d_to_index(monster_pos),
                    map.point2d_to_index(home_pt),
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
                let mut directions = Direction::ALL.to_vec();
                directions.shuffle(rng);
                directions.into_iter().find_map(|dir| {
                    let target = monster_pos + dir.offset();
                    if map.in_bounds(target)
                        && is_walkable(map.tiles[map.xy_idx(target.x, target.y)])
                        && DistanceAlg::Pythagoras.distance2d(target, home_pt) <= GUARD_PATROL_RADIUS as f32
                    {
                        Some(MovementIntent { entity, dir })
                    } else {
                        None
                    }
                })
            }
        }
        Some(PatrolState::Waypoint { points, current_index }) => {
            if points.is_empty() { return None; }
            let target = Point::new(points[*current_index].0, points[*current_index].1);
            if monster_pos == target {
                // Arrived — advance index.
                drop(map);
                if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity) {
                    if let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state {
                        *current_index = (*current_index + 1) % points.len();
                    }
                }
                // Re-borrow and pathfind to next waypoint.
                let patrol = world.get::<PatrolRoute>(entity).cloned();
                let map = world.resource::<Map>();
                if let Some(PatrolRoute { state: PatrolState::Waypoint { ref points, current_index } }) = patrol {
                    let next_target = Point::new(points[current_index].0, points[current_index].1);
                    let path = a_star_search(
                        map.point2d_to_index(monster_pos),
                        map.point2d_to_index(next_target),
                        map,
                    );
                    if path.success && path.steps.len() > 1 {
                        let next_step = map.index_to_point2d(path.steps[1]);
                        let dir = Direction::from_pos(
                            &Position::from_point(monster_pos),
                            &Position::from_point(next_step),
                        );
                        return Some(MovementIntent { entity, dir });
                    }
                }
                None
            } else {
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
                    // Pathfinding failed — skip to next waypoint.
                    drop(map);
                    if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity) {
                        if let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state {
                            *current_index = (*current_index + 1) % points.len();
                        }
                    }
                    None
                }
            }
        }
        Some(PatrolState::AreaRoam { min, max }) => {
            let (min, max) = (*min, *max);
            let mut directions = Direction::ALL.to_vec();
            directions.shuffle(rng);
            directions.into_iter().find_map(|dir| {
                let target = monster_pos + dir.offset();
                if map.in_bounds(target)
                    && is_walkable(map.tiles[map.xy_idx(target.x, target.y)])
                    && target.x >= min.0 && target.x <= max.0
                    && target.y >= min.1 && target.y <= max.1
                {
                    Some(MovementIntent { entity, dir })
                } else {
                    None
                }
            })
        }
        None => {
            // No PatrolRoute — wander freely.
            let mut directions = Direction::ALL.to_vec();
            directions.shuffle(rng);
            directions.into_iter().find_map(|dir| {
                let target = monster_pos + dir.offset();
                if map.in_bounds(target)
                    && is_walkable(map.tiles[map.xy_idx(target.x, target.y)])
                {
                    Some(MovementIntent { entity, dir })
                } else {
                    None
                }
            })
        }
    }
}

/// Evaluates all ready spells for `caster` and returns `(slot_index, target_entity)` for
/// the best one, or `None` if no spell is worth casting.
///
/// Fully faction-aware: determines friend/foe based on `Faction` component, not
/// hardcoded Player/Monster markers. This allows AI-vs-AI and player-allied AI in the future.
///
/// Score normalization prevents nova-ing:
///   effective_score = raw_score / (sqrt(mana_cost) * ln(cooldown + 1))
fn choose_spell(
    caster: Entity,
    caster_pos: Point,
    _player_entity: Option<Entity>,
    world: &mut World,
) -> Option<(usize, Entity)> {
    // --- Gather caster data upfront ---
    let caster_faction = world.get::<Faction>(caster)?.0.clone();

    // Boss phase filtering: if the caster has BossAI, only allow spells up to the current phase.
    let boss_phase = world.get::<BossAI>(caster).map(|b| b.phase);

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

    let caster_has_haste = world.get::<Hasted>(caster).is_some();
    let hp_pct = caster_hp as f32 / caster_max_hp.max(1) as f32;

    // Collect all nearby entities with faction info for targeting decisions.
    let nearby: Vec<NearbyEntity> = {
        let mut result = Vec::new();
        let mut query = world.query::<(Entity, &Position, &Faction, &Health, Option<&Mana>, Option<&Poisoned>, Option<&Slowed>, Option<&Hasted>)>();
        for (ent, pos, faction, health, mana, poisoned, slowed, hasted) in query.iter(world) {
            if ent == caster {
                continue;
            }
            result.push(NearbyEntity {
                entity: ent,
                pos: pos.to_point(),
                faction: faction.0.clone(),
                hp_current: health.current,
                hp_max: health.max,
                mana_current: mana.map(|m| m.current).unwrap_or(0),
                has_poison: poisoned.is_some(),
                has_slow: slowed.is_some(),
                has_haste: hasted.is_some(),
            });
        }
        result
    };

    // Partition nearby entities by relationship to caster.
    let enemies: Vec<&NearbyEntity> = nearby.iter().filter(|n| caster_faction.is_hostile_to(&n.faction)).collect();
    let allies: Vec<&NearbyEntity> = nearby.iter().filter(|n| caster_faction.is_allied_to(&n.faction)).collect();

    // Find the best enemy target (nearest visible enemy for single-target offensive spells).
    let nearest_enemy = enemies.iter()
        .min_by_key(|e| {
            let dx = e.pos.x - caster_pos.x;
            let dy = e.pos.y - caster_pos.y;
            dx * dx + dy * dy
        })
        .copied();

    // Find the most-wounded ally for heal/buff spells.
    let most_wounded_ally = allies.iter()
        .filter(|a| a.hp_current < a.hp_max)
        .max_by_key(|a| a.hp_max - a.hp_current)
        .copied();

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

        // Boss phase gating: skip spells the boss hasn't unlocked yet.
        if let Some(phase) = boss_phase {
            let min_phase = boss_spell_min_phase(spell_id);
            if min_phase > 0 && phase < min_phase {
                continue;
            }
        }

        // Resolve the primary target based on SpellTarget and available entities.
        let primary_target: Option<&NearbyEntity> = match spell.target {
            SpellTarget::Enemy => nearest_enemy,
            SpellTarget::Castor => None, // Self-targeted, handled below
            SpellTarget::Ally => most_wounded_ally,
            SpellTarget::AllyOrSelf => {
                // Choose most-wounded ally, or self if caster is more hurt
                let caster_missing = caster_max_hp - caster_hp;
                let ally_missing = most_wounded_ally.map(|a| a.hp_max - a.hp_current).unwrap_or(0);
                if caster_missing > ally_missing {
                    None // Will resolve to caster below
                } else {
                    most_wounded_ally
                }
            }
        };

        // For enemy-targeted spells, check range to the resolved enemy target.
        let target_in_range = match spell.target {
            SpellTarget::Enemy => {
                if spell.range > 0 {
                    primary_target
                        .map(|t| DistanceAlg::Pythagoras.distance2d(caster_pos, t.pos) <= spell.range as f32)
                        .unwrap_or(false)
                } else {
                    primary_target.is_some()
                }
            }
            SpellTarget::Ally => {
                if spell.range > 0 {
                    primary_target
                        .map(|t| DistanceAlg::Pythagoras.distance2d(caster_pos, t.pos) <= spell.range as f32)
                        .unwrap_or(false)
                } else {
                    primary_target.is_some()
                }
            }
            SpellTarget::AllyOrSelf | SpellTarget::Castor => true, // Always in range of self
        };

        if !target_in_range {
            continue;
        }

        // Resolve to entity ID.
        let resolved_entity: Option<Entity> = match spell.target {
            SpellTarget::Castor => Some(caster),
            SpellTarget::AllyOrSelf => {
                primary_target.map(|t| t.entity).or(Some(caster))
            }
            _ => primary_target.map(|t| t.entity),
        };

        let Some(resolved_entity) = resolved_entity else {
            continue; // No valid target available
        };

        // Score each effect and accumulate.
        let mut raw: i32 = 0;
        let mut target: Option<Entity> = None;

        for effect in &spell.effects {
            match effect {
                SpellEffect::Damage { dice, int_scaling } => {
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        let avg = avg_dice(dice);
                        let bonus = if *int_scaling { int_bonus } else { 0 };
                        let damage = (avg + bonus).max(1).min(enemy.hp_current);
                        raw += damage;
                        target = target.or(Some(enemy.entity));
                    }
                }
                SpellEffect::Heal { dice, int_scaling } => {
                    let avg = avg_dice(dice);
                    let bonus = if *int_scaling { int_bonus } else { 0 };
                    let heal = (avg + bonus).max(1);

                    // Score depends on who we're healing
                    let (missing_hp, heal_target) = if resolved_entity == caster {
                        (caster_max_hp - caster_hp, caster)
                    } else if let Some(ally) = primary_target {
                        (ally.hp_max - ally.hp_current, ally.entity)
                    } else {
                        continue;
                    };

                    if missing_hp <= 0 {
                        continue;
                    }
                    raw += heal.min(missing_hp) * 2;
                    target = target.or(Some(heal_target));
                }
                SpellEffect::AoeDamage {
                    dice,
                    radius,
                    int_scaling,
                } => {
                    // AoE centered on the resolved enemy target's position.
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        let center = enemy.pos;
                        let avg = avg_dice(dice);
                        let bonus = if *int_scaling { int_bonus } else { 0 };
                        let single_damage = (avg + bonus).max(1);

                        let mut enemy_count = 1i32; // The target enemy itself
                        let mut ally_count = 0i32;
                        for n in &nearby {
                            if n.entity == enemy.entity {
                                continue;
                            }
                            let dist = (n.pos.x - center.x).abs() + (n.pos.y - center.y).abs();
                            if dist <= *radius {
                                if caster_faction.is_hostile_to(&n.faction) {
                                    enemy_count += 1;
                                } else {
                                    ally_count += 1;
                                }
                            }
                        }
                        // Check if caster is in the blast
                        let caster_dist = (caster_pos.x - center.x).abs() + (caster_pos.y - center.y).abs();
                        if caster_dist <= *radius {
                            ally_count += 1;
                        }

                        let score = single_damage * enemy_count - single_damage * ally_count;
                        if score > 0 {
                            raw += score;
                            target = target.or(Some(enemy.entity));
                        }
                    }
                }
                SpellEffect::ChainDamage {
                    dice,
                    max_jumps,
                    int_scaling,
                    ..
                } => {
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        let avg = avg_dice(dice);
                        let bonus = if *int_scaling { int_bonus } else { 0 };
                        let primary_damage = (avg + bonus).max(1).min(enemy.hp_current);
                        let jump_damage = 4; // ~1d6 average
                        let mut jump_targets = 0i32;
                        for n in &nearby {
                            if n.entity == enemy.entity { continue; }
                            if caster_faction.is_hostile_to(&n.faction) {
                                let dist = (n.pos.x - enemy.pos.x).abs() + (n.pos.y - enemy.pos.y).abs();
                                if dist <= 3 {
                                    jump_targets += 1;
                                }
                            }
                        }
                        let actual_jumps = jump_targets.min(*max_jumps);
                        raw += primary_damage + (jump_damage + bonus.max(0)) * actual_jumps;
                        target = target.or(Some(enemy.entity));
                    }
                }
                SpellEffect::Buff {
                    amount, duration, ..
                } => {
                    let score = *amount * (*duration as i32) / 4;
                    raw += score;
                    target = target.or(Some(resolved_entity));
                }
                SpellEffect::Debuff {
                    amount, duration, ..
                } => {
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        let score = *amount * (*duration as i32) / 4;
                        raw += score;
                        target = target.or(Some(enemy.entity));
                    }
                }
                SpellEffect::ApplyPoison {
                    damage_per_turn,
                    duration,
                } => {
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        if enemy.has_poison { continue; }
                        raw += damage_per_turn * (*duration as i32);
                        target = target.or(Some(enemy.entity));
                    }
                }
                SpellEffect::ApplyHaste { .. } => {
                    // Self-haste or ally-haste
                    if resolved_entity == caster {
                        if caster_has_haste { continue; }
                    } else if let Some(t) = primary_target {
                        if t.has_haste { continue; }
                    }
                    raw += 15;
                    target = target.or(Some(resolved_entity));
                }
                SpellEffect::ApplySlow { .. } => {
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        if enemy.has_slow { continue; }
                        raw += 12;
                        target = target.or(Some(enemy.entity));
                    }
                }
                SpellEffect::DrainMana { amount, int_scaling } => {
                    if let Some(enemy) = primary_target.filter(|e| caster_faction.is_hostile_to(&e.faction)) {
                        if enemy.mana_current <= 0 { continue; }
                        let bonus = if *int_scaling { int_bonus } else { 0 };
                        let drain = (*amount + bonus).max(0).min(enemy.mana_current);
                        raw += drain;
                        target = target.or(Some(enemy.entity));
                    }
                }
                SpellEffect::SpiritShield { .. } => {
                    let score = if hp_pct < 0.5 { 10 } else { 3 };
                    raw += score;
                    target = target.or(Some(caster));
                }
                SpellEffect::Teleport { .. } => {
                    // Monsters generally shouldn't teleport — score 0
                }
                SpellEffect::ApplyEnrage { .. } => {
                    // High value self-buff: +50% damage is very strong
                    raw += 20;
                    target = target.or(Some(resolved_entity));
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
