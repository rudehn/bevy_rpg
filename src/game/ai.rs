use crate::{
    components::{Faction, FactionKind, MovementMode, Position, Viewshed},
    game::factions::FactionMatrix,
    game::{
        actions::{Direction, MovementIntent, RangedAttackIntent, WaitIntent},
        combat::{ApplyDamageMessage, DamageSource, Health, HealMessage},
        magic::StatusEffects,
        ranged::RangedCapable,
        staves::{MonsterAbilities, MonsterAbilityKind},
    },
    map::{Map, map::MapWithMode, tile::can_entity_enter_tile},
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

        // --- Submerge / Surface logic for aquatic monsters ---
        let movement_mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
        if movement_mode == MovementMode::RestrictedToLiquid {
            let map = world.resource::<Map>();
            let idx = map.xy_idx(ctx.monster_pos.x, ctx.monster_pos.y);
            let on_liquid = map.tiles[idx].liquid != crate::map::tile::LiquidType::None;

            if on_liquid && !has_adjacent_enemy(entity, ctx.monster_pos, world) {
                // Submerge when on liquid with no adjacent enemies
                world.commands().entity(entity).insert(crate::components::Submerged);
            } else {
                // Surface when enemy is adjacent or not on liquid
                world.commands().entity(entity).remove::<crate::components::Submerged>();
            }
        }

        // Helper closure: surface before any attack action
        let surface = |ent: Entity, w: &mut World| {
            w.commands().entity(ent).remove::<crate::components::Submerged>();
        };

        // --- Flee check (highest priority behavior) ---
        // Only flee when the player is visible — a monster that rounds a corner
        // and loses sight of the threat should stop fleeing and resume normal AI.
        if self.mode == MonsterAIMode::Hunting && self.flee_at_hp_percent > 0.0 && ctx.is_player_visible
            && let Some(health) = world.get::<Health>(entity)
                && should_flee(health.current, health.max, self.flee_at_hp_percent)
                    && let Some(intent) = try_flee_movement(
                        entity, ctx.monster_pos, ctx.player_point, world,
                    ) {
                        surface(entity, world);
                        world.write_message(intent);
                        return;
                    }
                    // All flee directions blocked (cornered) — fall through to
                    // normal behavior so the monster can still attack or act.

        // Try special actions (spell, ranged) before kiting.
        // Ranged monsters fire first, THEN kite on their next turn.
        if self.mode == MonsterAIMode::Hunting && ctx.is_player_visible {
            if try_use_ability(entity, ctx.monster_pos, ctx.player_entity, world) {
                surface(entity, world);
                return;
            }
            if try_ranged_attack(entity, ctx.monster_pos, ctx.player_point, ctx.player_entity, world) {
                surface(entity, world);
                return;
            }
        }

        // --- Kite check (ranged monsters retreat when player is too close) ---
        // Runs AFTER ranged attack so archers shoot-then-retreat, not retreat-forever.
        if self.mode == MonsterAIMode::Hunting && self.kites && ctx.is_player_visible
            && should_kite_retreat(
                ctx.monster_pos.x, ctx.monster_pos.y,
                ctx.player_point.x, ctx.player_point.y,
                self.kite_distance,
            )
                && let Some(intent) = try_flee_movement(
                    entity, ctx.monster_pos, ctx.player_point, world,
                ) {
                    surface(entity, world);
                    world.write_message(intent);
                    return;
                }
                // Retreat blocked — fall through to normal pathfinding.

        // --- Erratic movement check (before normal pathfinding) ---
        if self.mode == MonsterAIMode::Hunting && self.erratic_chance > 0.0 {
            let mut rng_inst = rng();
            let roll: f32 = rand::Rng::random(&mut rng_inst);
            if should_move_erratically(self.erratic_chance, roll) {
                let mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
                let map = world.resource::<Map>();
                let mut directions = [Direction::N, Direction::E, Direction::S, Direction::W].to_vec();
                directions.shuffle(&mut rng_inst);
                let erratic_intent = directions.into_iter().find_map(|dir| {
                    let target = ctx.monster_pos + dir.offset();
                    if map.in_bounds(target)
                        && can_entity_enter_tile(map.tiles[map.xy_idx(target.x, target.y)], mode)
                    {
                        Some(MovementIntent { entity, dir })
                    } else {
                        None
                    }
                });
                if let Some(intent) = erratic_intent {
                    world.write_message(intent);
                    return;
                }
                // If no valid erratic direction, fall through to normal pathfinding.
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
            let name = world.get::<crate::components::Name>(entity)
                .map(|n| n.0.clone())
                .unwrap_or_else(|| format!("{entity:?}"));
            bevy::log::info!(
                "FSM {name}: WaitIntent (mode={:?}, last_known={:?}, leash={:?})",
                self.mode, self.last_known_player_position, leader_leash
            );
            world.write_message(WaitIntent { entity });
        }
    }

    /// Update AI mode transitions based on player visibility.
    fn update_mode(&mut self, entity: Entity, ctx: &AIContext, world: &mut World) {
        match self.mode {
            MonsterAIMode::Asleep => {
                if ctx.is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                    self.last_known_player_position = Some(ctx.player_point);
                    self.chase_distance = 0;
                }
            }
            MonsterAIMode::Hunting => {
                if ctx.is_player_visible {
                    self.last_known_player_position = Some(ctx.player_point);
                    self.chase_distance = 0; // Reset chase tracking when player is visible
                } else {
                    // Player not visible — increment chase distance for leash tracking
                    self.chase_distance += 1;

                    // Land monsters give up faster when the player's last known
                    // position is on deep water (unreachable territory).
                    if let Some(last_pos) = self.last_known_player_position {
                        let movement_mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
                        if movement_mode == MovementMode::Land {
                            let map = world.resource::<Map>();
                            let idx = map.xy_idx(last_pos.x, last_pos.y);
                            if map.tiles[idx].liquid == crate::map::tile::LiquidType::Water {
                                self.chase_distance += 2;
                            }
                        }
                    }

                    // Chase leash: give up if chased too far without seeing player
                    if should_give_up_chase(self.chase_distance, self.chase_leash) {
                        self.mode = MonsterAIMode::Idle;
                        self.last_known_player_position = None;
                        self.chase_distance = 0;

                        // Post-hunt: snap waypoint patrols to nearest waypoint.
                        snap_to_nearest_waypoint(entity, ctx.monster_pos, world);
                        return;
                    }
                }
                if !ctx.is_player_visible && Some(ctx.monster_pos) == self.last_known_player_position {
                    self.mode = MonsterAIMode::Idle;
                    self.last_known_player_position = None;
                    self.chase_distance = 0;

                    // Post-hunt: snap waypoint patrols to nearest waypoint.
                    snap_to_nearest_waypoint(entity, ctx.monster_pos, world);
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
        let mut viewshed = world.get::<Viewshed>(entity).cloned().unwrap_or_default();

        // If the viewshed hasn't been computed yet, calculate it now so the AI
        // doesn't skip its first turn due to an empty visible_tiles.
        // NOTE: We only check `dirty` here, NOT `visible_tiles.is_empty()`.
        // A monster in a sealed room may legitimately have an empty viewshed
        // after computation; re-checking is_empty() would force an expensive
        // field_of_view() call every single turn for such monsters.
        if viewshed.dirty {
            let map = world.resource::<Map>();
            viewshed.visible_tiles =
                bracket_lib::prelude::field_of_view(monster_pos, viewshed.range, map);
            viewshed.dirty = false;
            // Write the computed viewshed back to the entity.
            if let Some(mut vs) = world.get_mut::<Viewshed>(entity) {
                vs.visible_tiles = viewshed.visible_tiles.clone();
                vs.dirty = false;
            }
        }

        let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
        let (player_entity, player_point) = match player_query.iter(world).next() {
            Some((e, p)) => (Some(e), p.to_point()),
            None => return None,
        };
        let is_player_visible = viewshed.visible_tiles.contains(&player_point);

        Some(AIContext { monster_pos, player_point, player_entity, is_player_visible })
    }
}

/// Returns true if any hostile entity is adjacent (Chebyshev distance 1) to the given position.
fn has_adjacent_enemy(entity: Entity, pos: Point, world: &mut World) -> bool {
    let monster_faction = world.get::<Faction>(entity).map(|f| f.0.clone());
    let faction_matrix = world.resource::<FactionMatrix>().clone();
    let mut query = world.query::<(Entity, &Position, &Faction)>();
    for (other, other_pos, other_faction) in query.iter(world) {
        if other == entity { continue; }
        let dist = (other_pos.x - pos.x).abs().max((other_pos.y - pos.y).abs());
        if dist <= 1 {
            if let Some(ref mf) = monster_faction {
                if faction_matrix.is_hostile_to(&mf.0, &other_faction.0.0) {
                    return true;
                }
            }
        }
    }
    false
}

/// Snap waypoint patrols to the nearest waypoint after a hunt ends.
fn snap_to_nearest_waypoint(entity: Entity, monster_pos: Point, world: &mut World) {
    if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity)
        && let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state
            && !points.is_empty() {
                *current_index = points.iter().enumerate()
                    .min_by_key(|(_, p)| (p.0 - monster_pos.x).abs() + (p.1 - monster_pos.y).abs())
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
}

/// Try to move away from the threat position (used for fleeing and kiting).
/// Tries the primary flee direction first, then perpendicular directions.
pub fn try_flee_movement(
    entity: Entity,
    monster_pos: Point,
    threat_pos: Point,
    world: &mut World,
) -> Option<MovementIntent> {
    let (dx, dy) = flee_direction(
        monster_pos.x, monster_pos.y,
        threat_pos.x, threat_pos.y,
    );
    if dx == 0 && dy == 0 {
        return None;
    }

    let mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
    let map = world.resource::<Map>();

    // Try primary flee direction.
    let primary = Point::new(monster_pos.x + dx, monster_pos.y + dy);
    if map.in_bounds(primary) && can_entity_enter_tile(map.tiles[map.xy_idx(primary.x, primary.y)], mode) {
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
        if map.in_bounds(target) && can_entity_enter_tile(map.tiles[map.xy_idx(target.x, target.y)], mode) {
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
pub fn try_stun_skip(entity: Entity, world: &mut World) -> bool {
    let Some(effects) = world.get::<StatusEffects>(entity) else {
        return false;
    };
    if !effects.is_stunned() {
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

/// Try to use a monster ability. Returns true if an ability was used (caller should return).
/// Public entry point for GOAP entities to attempt using their best ability.
pub fn try_use_ability_world(entity: Entity, world: &mut World) -> bool {
    let pos = world.get::<Position>(entity).map(|p| p.to_point()).unwrap_or(Point::new(0, 0));
    let player_entity = {
        let mut q = world.query_filtered::<Entity, With<Player>>();
        q.iter(world).next()
    };
    try_use_ability(entity, pos, player_entity, world)
}

fn try_use_ability(entity: Entity, monster_pos: Point, player_entity: Option<Entity>, world: &mut World) -> bool {
    // Read abilities
    let abilities = world.get::<MonsterAbilities>(entity).cloned();
    let Some(abilities) = abilities else { return false; };

    let caster_faction = world.get::<Faction>(entity).map(|f| f.0.clone());
    let caster_health = world.get::<Health>(entity).map(|h| (h.current, h.max));
    let caster_effects = world.get::<StatusEffects>(entity).cloned();
    let caster_name = world.get::<crate::components::Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "Something".to_string());

    let faction_matrix = world.resource::<FactionMatrix>().clone();

    // Find nearest enemy
    let nearest_enemy: Option<(Entity, Point)> = {
        let mut query = world.query::<(Entity, &Position, &Faction, &Health)>();
        let mut best: Option<(Entity, Point, i32)> = None;
        for (ent, pos, faction, _health) in query.iter(world) {
            if ent == entity { continue; }
            if let Some(ref cf) = caster_faction {
                if !faction_matrix.is_hostile_to(&cf.0, &faction.0.0) { continue; }
            }
            let dist = (pos.x - monster_pos.x).abs() + (pos.y - monster_pos.y).abs();
            if best.is_none() || dist < best.as_ref().unwrap().2 {
                best = Some((ent, pos.to_point(), dist));
            }
        }
        best.map(|(e, p, _)| (e, p))
    };

    // Find most-wounded ally for heals
    let most_wounded_ally: Option<Entity> = {
        let mut query = world.query::<(Entity, &Position, &Faction, &Health)>();
        let mut best: Option<(Entity, i32)> = None;
        for (ent, _pos, faction, health) in query.iter(world) {
            if ent == entity { continue; }
            if let Some(ref cf) = caster_faction {
                if !faction_matrix.is_allied_to(&cf.0, &faction.0.0) { continue; }
            }
            let missing = health.max - health.current;
            if missing > 0 && (best.is_none() || missing > best.as_ref().unwrap().1) {
                best = Some((ent, missing));
            }
        }
        best.map(|(e, _)| e)
    };

    let hp_pct = caster_health.map(|(c, m)| c as f32 / m.max(1) as f32).unwrap_or(1.0);

    for (idx, ability) in abilities.0.iter().enumerate() {
        if ability.current_cooldown > 0 { continue; }

        match &ability.kind {
            MonsterAbilityKind::Bolt { dice, damage_type } => {
                let Some((target_entity, target_pos)) = nearest_enemy else { continue; };
                let dist = (target_pos.x - monster_pos.x).abs() + (target_pos.y - monster_pos.y).abs();
                if dist > ability.range as i32 { continue; }

                let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                let damage = crate::game::staves::roll_dice_expr(&mut rng, dice).max(1);
                world.write_message(ApplyDamageMessage {
                    attacker: entity,
                    target: target_entity,
                    final_damage: damage,
                    damage_type: *damage_type,
                    source: DamageSource::Spell,
                });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                // Set cooldown
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::Heal { dice } => {
                // Heal self if low HP, or heal ally
                let heal_self = hp_pct < 0.6;
                let target = if heal_self {
                    Some(entity)
                } else {
                    most_wounded_ally
                };
                let Some(target) = target else { continue; };

                let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                let amount = crate::game::staves::roll_dice_expr(&mut rng, dice).max(1);
                world.write_message(HealMessage { entity: target, amount });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::ApplyStatus { effect, duration } => {
                let Some((target_entity, target_pos)) = nearest_enemy else { continue; };
                let dist = (target_pos.x - monster_pos.x).abs() + (target_pos.y - monster_pos.y).abs();
                if dist > ability.range as i32 { continue; }

                if let Some(mut effects) = world.get_mut::<StatusEffects>(target_entity) {
                    effects.add(*effect, *duration);
                }
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::SelfBuff { effect, duration } => {
                // Check if already have this effect
                let already_has = caster_effects.as_ref().map(|e| {
                    e.0.iter().any(|ae| std::mem::discriminant(&ae.kind) == std::mem::discriminant(effect))
                }).unwrap_or(false);
                if already_has { continue; }

                if let Some(mut effects) = world.get_mut::<StatusEffects>(entity) {
                    effects.add(*effect, *duration);
                }
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::Summon { monster, count } => {
                // Summon allied monsters
                let caster_pos_comp = world.get::<Position>(entity).cloned();
                let Some(caster_pos_comp) = caster_pos_comp else { continue; };
                world.insert_resource(crate::game::magic::PendingSummon {
                    caster_pos: caster_pos_comp,
                    caster_label: caster_name.clone(),
                    monster_name: monster.clone(),
                    count: *count,
                });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
        }
    }
    false
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
    if dist > 1.5 && dist <= range as f32
        && let Some(p_entity) = player_entity {
            world.write_message(RangedAttackIntent { attacker: entity, target: p_entity });
            return true;
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
                let _ = map;
                idle_movement(entity, monster_pos, world, &mut rng)
            }
        }
        _ => None,
    }
}

/// A* pathfind one step toward `target` and return a MovementIntent.
/// Uses the entity's `MovementMode` for mode-aware pathing costs.
pub fn pathfind_toward(entity: Entity, from: Point, target: Point, world: &mut World) -> Option<MovementIntent> {
    let mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
    let map = world.resource::<Map>();
    let map_with_mode = MapWithMode { map, mode };
    let path = a_star_search(
        map_with_mode.point2d_to_index(from),
        map_with_mode.point2d_to_index(target),
        &map_with_mode,
    );
    if path.success && path.steps.len() > 1 {
        let next_step = map_with_mode.index_to_point2d(path.steps[1]);
        let dir = Direction::from_pos(
            &Position::from_point(from),
            &Position::from_point(next_step),
        );
        Some(MovementIntent { entity, dir })
    } else {
        None
    }
}

/// Spells gated by boss phase comment preserved for reference.

/// Dispatch idle movement for a monster based on its `PatrolRoute` component.
/// Sentry: jitter near home. Waypoint: walk route. AreaRoam: bounded random walk. None: free wander.
pub fn idle_movement(
    entity: Entity,
    monster_pos: Point,
    world: &mut World,
    rng: &mut impl rand::Rng,
) -> Option<MovementIntent> {
    let patrol = world.get::<PatrolRoute>(entity).cloned();
    let mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
    let map = world.resource::<Map>();
    let map_with_mode = MapWithMode { map, mode };

    match patrol.as_ref().map(|p| &p.state) {
        Some(PatrolState::Sentry { home }) => {
            let home_pt = Point::new(home.0, home.1);
            let dist = DistanceAlg::Pythagoras.distance2d(monster_pos, home_pt);
            if dist > GUARD_PATROL_RADIUS as f32 {
                let path = a_star_search(
                    map_with_mode.point2d_to_index(monster_pos),
                    map_with_mode.point2d_to_index(home_pt),
                    &map_with_mode,
                );
                if path.success && path.steps.len() > 1 {
                    let next_step = map_with_mode.index_to_point2d(path.steps[1]);
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
                        && can_entity_enter_tile(map.tiles[map.xy_idx(target.x, target.y)], mode)
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
                let _ = map;
                drop(map_with_mode);
                if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity)
                    && let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state {
                        *current_index = (*current_index + 1) % points.len();
                    }
                // Re-borrow and pathfind to next waypoint.
                let patrol = world.get::<PatrolRoute>(entity).cloned();
                let map = world.resource::<Map>();
                let map_with_mode = MapWithMode { map, mode };
                if let Some(PatrolRoute { state: PatrolState::Waypoint { ref points, current_index } }) = patrol {
                    let next_target = Point::new(points[current_index].0, points[current_index].1);
                    let path = a_star_search(
                        map_with_mode.point2d_to_index(monster_pos),
                        map_with_mode.point2d_to_index(next_target),
                        &map_with_mode,
                    );
                    if path.success && path.steps.len() > 1 {
                        let next_step = map_with_mode.index_to_point2d(path.steps[1]);
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
                    map_with_mode.point2d_to_index(monster_pos),
                    map_with_mode.point2d_to_index(target),
                    &map_with_mode,
                );
                if path.success && path.steps.len() > 1 {
                    let next_step = map_with_mode.index_to_point2d(path.steps[1]);
                    let dir = Direction::from_pos(
                        &Position::from_point(monster_pos),
                        &Position::from_point(next_step),
                    );
                    Some(MovementIntent { entity, dir })
                } else {
                    // Pathfinding failed — skip to next waypoint.
                    let _ = map;
                    drop(map_with_mode);
                    if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity)
                        && let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state {
                            *current_index = (*current_index + 1) % points.len();
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
                    && can_entity_enter_tile(map.tiles[map.xy_idx(target.x, target.y)], mode)
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
                    && can_entity_enter_tile(map.tiles[map.xy_idx(target.x, target.y)], mode)
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
// Old choose_spell code — find the end marker below and delete everything between.
// =====================================================================
// AI Behavior Helpers (pure decision functions)
// =====================================================================

/// Should this monster flee? Returns true if current HP ratio is below the threshold.
fn should_flee(current_hp: i32, max_hp: i32, flee_threshold: f32) -> bool {
    if flee_threshold <= 0.0 || max_hp <= 0 {
        return false;
    }
    (current_hp as f32 / max_hp as f32) < flee_threshold
}

/// Should this monster move erratically this turn?
fn should_move_erratically(erratic_chance: f32, roll: f32) -> bool {
    erratic_chance > 0.0 && roll < erratic_chance
}

/// Should this monster give up chasing and return to idle?
fn should_give_up_chase(chase_distance: u32, chase_leash: u32) -> bool {
    chase_leash > 0 && chase_distance >= chase_leash
}

/// Should a kiting monster retreat from the player?
fn should_kite_retreat(
    monster_x: i32, monster_y: i32, player_x: i32, player_y: i32, kite_distance: u32,
) -> bool {
    let dx = (monster_x - player_x).abs();
    let dy = (monster_y - player_y).abs();
    (dx * dx + dy * dy) < (kite_distance as i32 * kite_distance as i32)
}

/// Pick the best cardinal direction to flee AWAY from a threat position.
pub fn flee_direction(monster_x: i32, monster_y: i32, threat_x: i32, threat_y: i32) -> (i32, i32) {
    let dx = monster_x - threat_x;
    let dy = monster_y - threat_y;
    if dx == 0 && dy == 0 {
        return (0, 0);
    }
    if dx.abs() >= dy.abs() { (dx.signum(), 0) } else { (0, dy.signum()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flee_when_below_threshold() {
        assert!(should_flee(2, 10, 0.3));
    }

    #[test]
    fn no_flee_when_above_threshold() {
        assert!(!should_flee(5, 10, 0.3));
    }

    #[test]
    fn no_flee_when_threshold_zero() {
        assert!(!should_flee(1, 10, 0.0));
    }

    #[test]
    fn no_flee_when_at_exact_threshold() {
        assert!(!should_flee(3, 10, 0.3));
    }

    #[test]
    fn no_flee_when_max_hp_zero() {
        assert!(!should_flee(0, 0, 0.5));
    }

    #[test]
    fn erratic_with_low_roll() {
        assert!(should_move_erratically(0.3, 0.1));
    }

    #[test]
    fn not_erratic_with_high_roll() {
        assert!(!should_move_erratically(0.3, 0.5));
    }

    #[test]
    fn never_erratic_when_chance_zero() {
        assert!(!should_move_erratically(0.0, 0.0));
    }

    #[test]
    fn give_up_when_leash_exceeded() {
        assert!(should_give_up_chase(10, 8));
    }

    #[test]
    fn keep_chasing_within_leash() {
        assert!(!should_give_up_chase(5, 8));
    }

    #[test]
    fn give_up_at_exact_leash() {
        assert!(should_give_up_chase(8, 8));
    }

    #[test]
    fn never_give_up_when_leash_zero() {
        assert!(!should_give_up_chase(100, 0));
    }

    #[test]
    fn kite_when_adjacent() {
        assert!(should_kite_retreat(5, 5, 6, 5, 3));
    }

    #[test]
    fn kite_when_close() {
        assert!(should_kite_retreat(5, 5, 7, 5, 3));
    }

    #[test]
    fn no_kite_when_at_distance() {
        assert!(!should_kite_retreat(5, 5, 8, 5, 3));
    }

    #[test]
    fn no_kite_when_far() {
        assert!(!should_kite_retreat(5, 5, 10, 5, 3));
    }

    #[test]
    fn flee_away_east() {
        assert_eq!(flee_direction(5, 5, 2, 5), (1, 0));
    }

    #[test]
    fn flee_away_west() {
        assert_eq!(flee_direction(5, 5, 8, 5), (-1, 0));
    }

    #[test]
    fn flee_away_north() {
        assert_eq!(flee_direction(5, 5, 5, 8), (0, -1));
    }

    #[test]
    fn flee_away_south() {
        assert_eq!(flee_direction(5, 5, 5, 2), (0, 1));
    }

    #[test]
    fn flee_on_top_of_threat() {
        assert_eq!(flee_direction(5, 5, 5, 5), (0, 0));
    }
}
