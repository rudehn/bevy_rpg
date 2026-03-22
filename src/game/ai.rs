use crate::{
    assets::SpellRegistryHandle,
    components::{Faction, FactionKind, Position, Viewshed},
    game::{
        actions::{Direction, MovementIntent, RangedAttackIntent, WaitIntent},
        ai_behaviors,
        boss::BossAI,
        combat::Health,
        magic::{ActiveSpells, CastSpellMessage, Hasted, Slowed, SpellCooldowns},
        ranged::RangedCapable,
        spells::{SpellRegistry, SpellTarget},
        stats::Mana,
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

    /// Behavior flags (copied from asset at spawn time)
    pub flee_at_hp_percent: f32,
    pub erratic_chance: f32,
    pub chase_leash: u32,
    pub kites: bool,
    pub kite_distance: u32,

    /// Runtime chase tracking
    pub chase_distance: u32,
    pub spawn_position: Option<Point>,
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
        // Stunned entities skip their turn entirely.
        if try_stun_skip(entity, world) {
            return;
        }

        // Gather read-only data about the monster and player.
        let Some(ctx) = AIContext::gather(entity, world) else {
            world.write_message(WaitIntent { entity });
            return;
        };

        // Update AI mode based on visibility.
        self.update_mode(entity, &ctx, world);

        // --- Flee check (highest priority behavior) ---
        if self.mode == MonsterAIMode::Hunting && self.flee_at_hp_percent > 0.0 {
            if let Some(health) = world.get::<Health>(entity) {
                if ai_behaviors::should_flee(health.current, health.max, self.flee_at_hp_percent) {
                    if let Some(intent) = try_flee_movement(
                        entity, ctx.monster_pos, ctx.player_point, world,
                    ) {
                        world.write_message(intent);
                    } else {
                        world.write_message(WaitIntent { entity });
                    }
                    return;
                }
            }
        }

        // Try special actions (spell, ranged) before movement.
        if self.mode == MonsterAIMode::Hunting && ctx.is_player_visible {
            if try_cast_spell(entity, ctx.monster_pos, ctx.player_entity, world) {
                return;
            }
            if try_ranged_attack(entity, ctx.monster_pos, ctx.player_point, ctx.player_entity, world) {
                return;
            }
        }

        // Resolve squad leash target (followers stay near their leader).
        let leader_leash = resolve_squad_leash(entity, ctx.monster_pos, world);

        // Pathfind and move.
        if let Some(intent) = resolve_movement(
            entity, self.mode, ctx.monster_pos, leader_leash,
            self.last_known_player_position, world,
        ) {
            world.write_message(intent);
        } else {
            world.write_message(WaitIntent { entity });
        }
    }

    /// Update AI mode transitions based on player visibility.
    fn update_mode(&mut self, entity: Entity, ctx: &AIContext, world: &mut World) {
        match self.mode {
            MonsterAIMode::Asleep => {
                if ctx.is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
            MonsterAIMode::Hunting => {
                if ctx.is_player_visible {
                    self.last_known_player_position = Some(ctx.player_point);
                }
                if !ctx.is_player_visible && Some(ctx.monster_pos) == self.last_known_player_position {
                    self.mode = MonsterAIMode::Idle;
                    self.last_known_player_position = None;

                    // Post-hunt: snap waypoint patrols to nearest waypoint.
                    if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity) {
                        if let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state {
                            if !points.is_empty() {
                                *current_index = points.iter().enumerate()
                                    .min_by_key(|(_, p)| (p.0 - ctx.monster_pos.x).abs() + (p.1 - ctx.monster_pos.y).abs())
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                            }
                        }
                    }
                }
            }
            MonsterAIMode::Idle => {
                if ctx.is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AI helper functions — extracted from MonsterAI::execute() for readability
// ---------------------------------------------------------------------------

/// Read-only snapshot of the world state needed for AI decisions.
struct AIContext {
    monster_pos: Point,
    player_point: Point,
    player_entity: Option<Entity>,
    is_player_visible: bool,
}

impl AIContext {
    fn gather(entity: Entity, world: &mut World) -> Option<Self> {
        let monster_pos = world.get::<Position>(entity)?.to_point();
        let viewshed = world.get::<Viewshed>(entity).cloned().unwrap_or_default();

        let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
        let (player_entity, player_point) = match player_query.iter(world).next() {
            Some((e, p)) => (Some(e), p.to_point()),
            None => return None,
        };
        let is_player_visible = viewshed.visible_tiles.contains(&player_point);

        Some(AIContext { monster_pos, player_point, player_entity, is_player_visible })
    }
}

/// Try to move away from the threat position (used for fleeing and kiting).
/// Tries the primary flee direction first, then perpendicular directions.
fn try_flee_movement(
    entity: Entity,
    monster_pos: Point,
    threat_pos: Point,
    world: &mut World,
) -> Option<MovementIntent> {
    let (dx, dy) = ai_behaviors::flee_direction(
        monster_pos.x, monster_pos.y,
        threat_pos.x, threat_pos.y,
    );
    if dx == 0 && dy == 0 {
        return None;
    }

    let map = world.resource::<Map>();

    // Try primary flee direction.
    let primary = Point::new(monster_pos.x + dx, monster_pos.y + dy);
    if map.in_bounds(primary) && is_walkable(map.tiles[map.xy_idx(primary.x, primary.y)]) {
        let dir = Direction::from_pos(
            &Position::from_point(monster_pos),
            &Position::from_point(primary),
        );
        return Some(MovementIntent { entity, dir });
    }

    // Try perpendicular directions.
    let perp_offsets = if dx != 0 {
        [(0, 1), (0, -1)] // Primary was horizontal, try vertical
    } else {
        [(1, 0), (-1, 0)] // Primary was vertical, try horizontal
    };

    for (px, py) in perp_offsets {
        let target = Point::new(monster_pos.x + px, monster_pos.y + py);
        if map.in_bounds(target) && is_walkable(map.tiles[map.xy_idx(target.x, target.y)]) {
            let dir = Direction::from_pos(
                &Position::from_point(monster_pos),
                &Position::from_point(target),
            );
            return Some(MovementIntent { entity, dir });
        }
    }

    None
}

/// If the entity is stunned, emit a wait + visual feedback and return true.
fn try_stun_skip(entity: Entity, world: &mut World) -> bool {
    if world.get::<crate::game::magic::Stunned>(entity).is_none() {
        return false;
    }
    let name = world
        .get::<crate::components::Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "Something".to_string());
    world.write_message(crate::ui::game_log::GameLogMessage(format!(
        "{} is stunned and cannot act!", name
    )));
    if let Some(pos) = world.get::<Position>(entity) {
        let world_pos = crate::game::particles::grid_to_world(pos.x, pos.y);
        world.write_message(crate::game::particles::ParticleRequest::FloatingText {
            world_pos,
            text: "\u{2605}".to_string(),
            color: bevy::prelude::Color::srgba(1.0, 1.0, 0.3, 1.0),
            font_size: 5.0,
        });
    }
    world.write_message(WaitIntent { entity });
    true
}

/// Try to cast a spell. Returns true if a spell was cast (caller should return).
fn try_cast_spell(entity: Entity, monster_pos: Point, player_entity: Option<Entity>, world: &mut World) -> bool {
    if let Some((spell_slot, target)) = choose_spell(entity, monster_pos, player_entity, world) {
        world.write_message(CastSpellMessage {
            caster: entity,
            slot: spell_slot,
            target,
            target_pos: None,
        });
        true
    } else {
        false
    }
}

/// Try a ranged attack if the monster has ranged capability and player is in range
/// but not adjacent (prefer melee when adjacent). Returns true if fired.
fn try_ranged_attack(
    entity: Entity,
    monster_pos: Point,
    player_point: Point,
    player_entity: Option<Entity>,
    world: &mut World,
) -> bool {
    let Some(ranged_capable) = world.get::<RangedCapable>(entity) else {
        return false;
    };
    let range = ranged_capable.range;
    let dist = DistanceAlg::Pythagoras.distance2d(monster_pos, player_point);
    if dist > 1.5 && dist <= range as f32 {
        if let Some(p_entity) = player_entity {
            world.write_message(RangedAttackIntent { attacker: entity, target: p_entity });
            return true;
        }
    }
    false
}

/// Find the squad leader's position if this entity is a non-leader follower
/// that's too far from its leader.
fn resolve_squad_leash(entity: Entity, monster_pos: Point, world: &mut World) -> Option<Point> {
    use crate::game::squad::{SquadId, SquadLeader};
    const SQUAD_LEASH_RANGE: f32 = 4.0;

    let squad_id = world.get::<SquadId>(entity).copied()?;
    if world.get::<SquadLeader>(entity).is_some() {
        return None; // Leaders don't leash
    }

    let mut leader_pos = None;
    let mut query = world.query_filtered::<(&SquadId, &Position), With<SquadLeader>>();
    for (sid, pos) in query.iter(world) {
        if *sid == squad_id {
            leader_pos = Some(pos.to_point());
            break;
        }
    }
    leader_pos.filter(|lp| DistanceAlg::Pythagoras.distance2d(monster_pos, *lp) > SQUAD_LEASH_RANGE)
}

/// Pathfind toward the appropriate target based on AI mode.
fn resolve_movement(
    entity: Entity,
    mode: MonsterAIMode,
    monster_pos: Point,
    leader_leash: Option<Point>,
    last_known_player_pos: Option<Point>,
    world: &mut World,
) -> Option<MovementIntent> {
    match mode {
        MonsterAIMode::Hunting => {
            let target = leader_leash.or(last_known_player_pos)?;
            pathfind_toward(entity, monster_pos, target, world)
        }
        MonsterAIMode::Idle => {
            if let Some(target) = leader_leash {
                pathfind_toward(entity, monster_pos, target, world)
            } else {
                let mut rng = rng();
                let map = world.resource::<Map>();
                drop(map);
                idle_movement(entity, monster_pos, world, &mut rng)
            }
        }
        _ => None,
    }
}

/// A* pathfind one step toward `target` and return a MovementIntent.
fn pathfind_toward(entity: Entity, from: Point, target: Point, world: &mut World) -> Option<MovementIntent> {
    let map = world.resource::<Map>();
    let path = a_star_search(
        map.point2d_to_index(from),
        map.point2d_to_index(target),
        map,
    );
    if path.success && path.steps.len() > 1 {
        let next_step = map.index_to_point2d(path.steps[1]);
        let dir = Direction::from_pos(
            &Position::from_point(from),
            &Position::from_point(next_step),
        );
        Some(MovementIntent { entity, dir })
    } else {
        None
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

    let (active_slots, cooldowns, mana_current, caster_hp, caster_max_hp) = {
        let active = world.get::<ActiveSpells>(caster)?;
        let cooldowns = world.get::<SpellCooldowns>(caster).cloned().unwrap_or_default();
        let mana = world.get::<Mana>(caster).map(|m| m.current).unwrap_or(0);
        let hp = world.get::<Health>(caster).map(|h| (h.current, h.max)).unwrap_or((1, 1));
        (active.slots.clone(), cooldowns, mana, hp.0, hp.1)
    };

    let caster_has_haste = world.get::<Hasted>(caster).is_some();
    let hp_pct = caster_hp as f32 / caster_max_hp.max(1) as f32;

    // Collect all nearby entities with faction info for targeting decisions.
    let nearby: Vec<NearbyEntity> = {
        let mut result = Vec::new();
        let mut query = world.query::<(Entity, &Position, &Faction, &Health, Option<&Mana>, Option<&Slowed>, Option<&Hasted>)>();
        for (ent, pos, faction, health, mana, slowed, hasted) in query.iter(world) {
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

        // Build scoring context from the resolved primary target.
        let caster_as_target = NearbyEntity {
            entity: caster,
            pos: caster_pos,
            faction: caster_faction.clone(),
            hp_current: caster_hp,
            hp_max: caster_max_hp,
            mana_current: 0,
            has_slow: false,
            has_haste: caster_has_haste,
        };
        let scoring_target = primary_target.unwrap_or(&caster_as_target);

        // Build nearby list for AoE/chain scoring (excludes primary target).
        let scoring_nearby: Vec<crate::game::spells::ScoringNearby> = nearby.iter()
            .filter(|n| n.entity != scoring_target.entity)
            .map(|n| crate::game::spells::ScoringNearby {
                pos: (n.pos.x, n.pos.y),
                is_enemy: caster_faction.is_hostile_to(&n.faction),
            })
            .collect();

        let scoring_ctx = crate::game::spells::EffectScoringCtx {
            caster_pos: (caster_pos.x, caster_pos.y),
            caster_hp_pct: hp_pct,
            caster_has_haste,
            target_hp: scoring_target.hp_current,
            target_hp_max: scoring_target.hp_max,
            target_mana: scoring_target.mana_current,
            target_has_slow: scoring_target.has_slow,
            target_has_haste: scoring_target.has_haste,
            target_is_self: resolved_entity == caster,
            target_pos: (scoring_target.pos.x, scoring_target.pos.y),
            nearby: &scoring_nearby,
        };

        // Score each effect and accumulate.
        let mut raw: i32 = 0;
        for effect in &spell.effects {
            raw += crate::game::spells::score_effect(effect, &scoring_ctx);
        }

        if raw <= 0 {
            continue;
        }

        let effective = crate::game::spells::normalize_spell_score(raw, spell.mana_cost, spell.cooldown);

        if effective > best_score {
            best_score = effective;
            best_slot = Some(slot_idx);
            best_target = Some(resolved_entity);
        }
    }

    if best_score > 1.0 {
        best_slot.zip(best_target)
    } else {
        None
    }
}

// avg_dice moved to spells.rs as a shared utility
