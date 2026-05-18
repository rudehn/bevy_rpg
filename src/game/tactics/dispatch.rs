//! Bevy adapter for the tactic resolver. Bridges ECS components into
//! [`TurnSnapshot`]s, calls [`resolve_turn`], translates the outcome
//! back into the existing intent messages, and writes state deltas
//! back to ECS components.
//!
//! Mirrors the structure of [`crate::game::combat::mod`]: the pure
//! math lives in `resolve.rs`, this file does the ECS plumbing.
//!
//! The dispatcher is scheduled in `ProcessingPhase::Brain` between
//! `goap_ai_dispatch` and `monster_ai_dispatch` during the migration.
//! Each turn it walks every entity with `(TacticBrain, MyTurn)` and
//! dispatches a tactic. FSM and GOAP entities are mutually exclusive
//! at spawn time, so an entity uses exactly one AI path.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bracket_lib::prelude::Point;
use bracket_lib::random::RandomNumberGenerator;
use rand::RngCore;

use roguelike_engine::ai::decisions::flee_direction;
use roguelike_engine::ai::monster_ai::{MonsterAI, MonsterAIMode};
use roguelike_engine::ai::pathfinding::next_step_toward_with_mode;
use roguelike_engine::components::MovementMode;
use roguelike_engine::factions::FactionMatrix;
use roguelike_engine::geometry::Direction;
use roguelike_engine::map::tile::{can_entity_enter_tile, LiquidType};
use roguelike_engine::map::Map;
use roguelike_engine::squad::{SquadId, SquadLeader};
use roguelike_engine::status::StatusEffects;

use crate::assets::MonsterManifest;
use crate::components::{Faction, Position, Submerged, Viewshed};
use crate::game::actions::{
    ActionGuard, MeleeIntent, MovementIntent, OpenChestIntent, PickUpIntent, RangedAttackIntent,
    WaitIntent,
};
use crate::game::combat::{GameRng, Health};
use crate::game::fleeing::Fleeing;
use crate::game::magic::GameStatusEffectsExt;
use crate::game::ranged::RangedCapable;
use crate::game::staves::MonsterAbilities;
use crate::game::tactics::library::{ALL_TACTIC_NAMES, TERMINAL_TACTIC_NAME};
use crate::game::tactics::resolve::{
    resolve_turn, ActorId, ActorView, AiMode, EnemyView, GridDir, IdleMovementKind, MovementKind,
    PatrolView, PathContext, Tactic, TacticAction, TacticStateDelta, TurnOutcome, TurnSnapshot,
};
use roguelike_engine::components::{PatrolRoute, PatrolState};
use crate::game::turns::MyTurn;
use crate::player::Player;

// =====================================================================
// RNG adapter
// =====================================================================

/// Bridges `bracket_lib::random::RandomNumberGenerator` (which the
/// game uses everywhere via the `GameRng` resource) to `rand`'s
/// `RngCore` trait (which the pure resolver consumes).
///
/// Bracket's RNG wraps an `XorShiftRng` internally but doesn't expose
/// the `RngCore` impl. This adapter implements it manually using
/// `next_u64` (the only `u64`-yielding method bracket exposes
/// publicly).
struct BracketRngAdapter<'a>(&'a mut RandomNumberGenerator);

impl<'a> RngCore for BracketRngAdapter<'a> {
    fn next_u32(&mut self) -> u32 {
        // Use the low 32 bits of next_u64.
        self.0.next_u64() as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let n = self.next_u64().to_le_bytes();
            let remaining = dest.len() - i;
            let chunk = remaining.min(8);
            dest[i..i + chunk].copy_from_slice(&n[..chunk]);
            i += chunk;
        }
    }
}

// =====================================================================
// Component
// =====================================================================

/// Replaces both `MonsterAI`-dispatch and `GoapAI` roles for monsters
/// migrated to the tactic registry. The actual FSM state continues to
/// live in `MonsterAI` (`mode`, `last_known_player_position`,
/// tuning knobs); `TacticBrain` only carries the per-monster ordered
/// tactic list + a slot for the last-fired tactic's name (used by
/// `nearby.rs` for display).
#[derive(Component)]
pub struct TacticBrain {
    /// `'static` slice — production lists are produced at spawn time
    /// via `Vec::leak` after looking up each name in the registry.
    /// The slice and every reference inside it outlive the application.
    pub tactics: &'static [&'static dyn Tactic],
    pub last_tactic: Option<&'static str>,
    /// Cached per-monster idle-movement style (resolver-side mirror
    /// of `crate::assets::IdleMovement`).
    pub idle_movement: IdleMovementKind,
    /// Current `PathToRandomTile` destination, or `None` when the
    /// next idle turn will pick a fresh one. Updated by the `IdleMove`
    /// tactic via `TacticStateDelta::set_roam_target`.
    pub roam_target: Option<Point>,
}

impl TacticBrain {
    pub fn new(
        tactics: &'static [&'static dyn Tactic],
        idle_movement: IdleMovementKind,
    ) -> Self {
        Self {
            tactics,
            last_tactic: None,
            idle_movement,
            roam_target: None,
        }
    }
}

// =====================================================================
// Conversions
// =====================================================================

fn grid_dir_to_direction(g: GridDir) -> Direction {
    match g {
        GridDir::N => Direction::N,
        GridDir::NE => Direction::NE,
        GridDir::E => Direction::E,
        GridDir::SE => Direction::SE,
        GridDir::S => Direction::S,
        GridDir::SW => Direction::SW,
        GridDir::W => Direction::W,
        GridDir::NW => Direction::NW,
    }
}

fn engine_mode_to_resolver(m: MonsterAIMode) -> AiMode {
    // `MonsterAIMode` is `#[non_exhaustive]`; new engine-side modes
    // collapse to Idle here until tactics learn about them.
    match m {
        MonsterAIMode::Asleep => AiMode::Asleep,
        MonsterAIMode::Idle => AiMode::Idle,
        MonsterAIMode::Hunting => AiMode::Hunting,
        _ => AiMode::Idle,
    }
}

/// Convert the resolver's mode back to the engine's. `Fleeing` collapses
/// to `Hunting` here because Phase 2 has no engine-side Fleeing variant
/// yet — Phase 2.5 introduces a game-side `Fleeing` marker component and
/// reworks this function.
fn resolver_mode_to_engine(m: AiMode) -> MonsterAIMode {
    match m {
        AiMode::Asleep => MonsterAIMode::Asleep,
        AiMode::Idle => MonsterAIMode::Idle,
        AiMode::Hunting | AiMode::Fleeing { .. } => MonsterAIMode::Hunting,
    }
}

fn movement_mode_to_kind(m: MovementMode) -> MovementKind {
    match m {
        MovementMode::Land => MovementKind::Land,
        MovementMode::ImmuneToWater => MovementKind::Amphibious,
        MovementMode::RestrictedToLiquid => MovementKind::Aquatic,
        _ => MovementKind::Land,
    }
}

pub fn asset_idle_movement_to_kind(m: &crate::assets::IdleMovement) -> IdleMovementKind {
    match m {
        crate::assets::IdleMovement::PathToRandomTile => IdleMovementKind::PathToRandomTile,
        crate::assets::IdleMovement::Patrol => IdleMovementKind::Patrol,
        crate::assets::IdleMovement::Roam => IdleMovementKind::Roam,
        crate::assets::IdleMovement::Stationary => IdleMovementKind::Stationary,
    }
}

fn patrol_route_to_view(route: &PatrolRoute) -> Option<PatrolView> {
    // `PatrolState` is `#[non_exhaustive]`; unknown future variants
    // produce None so callers fall through gracefully.
    match &route.state {
        PatrolState::Sentry { home } => Some(PatrolView::Sentry {
            home: Point::new(home.0, home.1),
            radius: roguelike_engine::ai::monster_ai::GUARD_PATROL_RADIUS,
        }),
        PatrolState::Waypoint { points, current_index } => Some(PatrolView::Waypoint {
            points: points.iter().map(|(x, y)| Point::new(*x, *y)).collect(),
            current_index: *current_index,
        }),
        PatrolState::AreaRoam { min, max } => Some(PatrolView::AreaRoam {
            min: Point::new(min.0, min.1),
            max: Point::new(max.0, max.1),
        }),
        _ => None,
    }
}

// =====================================================================
// MapPathContext — pathfinding port over Map + MovementMode
// =====================================================================

/// Wraps a shared snapshot of the live `Map` + the actor's
/// `MovementMode` so tactics can ask for pathfinding without touching
/// `Map` directly. Constructed once per actor turn; dropped after
/// `resolve_turn` returns.
struct MapPathContext {
    map: std::sync::Arc<Map>,
    mode: MovementMode,
}

impl PathContext for MapPathContext {
    fn next_step_toward(&self, from: Point, to: Point) -> Option<Point> {
        next_step_toward_with_mode(&self.map, from, to, self.mode)
    }

    fn next_flee_step(&self, from: Point, threat: Point) -> Option<Point> {
        let (dx, dy) = flee_direction(from.x, from.y, threat.x, threat.y);
        if dx == 0 && dy == 0 {
            return None;
        }
        // Try the primary flee axis first.
        let primary = Point::new(from.x + dx, from.y + dy);
        if self.map.in_bounds(primary)
            && can_entity_enter_tile(
                self.map.tiles[self.map.xy_idx(primary.x, primary.y)],
                self.mode,
            )
        {
            return Some(primary);
        }
        // Fall back to perpendicular axes.
        let perpendiculars = if dx != 0 {
            [(0, 1), (0, -1)]
        } else {
            [(1, 0), (-1, 0)]
        };
        for (px, py) in perpendiculars {
            let target = Point::new(from.x + px, from.y + py);
            if self.map.in_bounds(target)
                && can_entity_enter_tile(
                    self.map.tiles[self.map.xy_idx(target.x, target.y)],
                    self.mode,
                )
            {
                return Some(target);
            }
        }
        None
    }

    fn pick_random_nearby(
        &self,
        from: Point,
        radius: i32,
        rng: &mut dyn RngCore,
    ) -> Option<Point> {
        // Bounded random walk — try up to 8 offsets within the radius.
        for _ in 0..8 {
            let span = (radius * 2 + 1) as u32;
            let dx = (rng.next_u32() % span) as i32 - radius;
            let dy = (rng.next_u32() % span) as i32 - radius;
            if dx == 0 && dy == 0 {
                continue;
            }
            let target = Point::new(from.x + dx, from.y + dy);
            if self.map.in_bounds(target)
                && can_entity_enter_tile(
                    self.map.tiles[self.map.xy_idx(target.x, target.y)],
                    self.mode,
                )
            {
                return Some(target);
            }
        }
        None
    }

    fn pick_random_walkable(&self, rng: &mut dyn RngCore) -> Option<Point> {
        // Bounded retry — pick random tiles until one is walkable.
        // Maps are small (typically 80×60); even a 50% walkable
        // density gives <0.1% odds of 16 failures in a row.
        let width = self.map.width as u32;
        let height = self.map.height as u32;
        if width == 0 || height == 0 {
            return None;
        }
        for _ in 0..16 {
            let x = (rng.next_u32() % width) as i32;
            let y = (rng.next_u32() % height) as i32;
            let target = Point::new(x, y);
            if can_entity_enter_tile(self.map.tiles[self.map.xy_idx(x, y)], self.mode) {
                return Some(target);
            }
        }
        None
    }
}

// =====================================================================
// IdMap — per-tick Entity ↔ ActorId mapping
// =====================================================================

#[derive(Default)]
struct IdMap {
    by_actor: std::collections::HashMap<ActorId, Entity>,
    next_id: u64,
}

impl IdMap {
    fn intern(&mut self, entity: Entity) -> ActorId {
        // Linear scan: the per-tick map is tiny (typically <30 entries).
        for (id, ent) in &self.by_actor {
            if *ent == entity {
                return *id;
            }
        }
        let id = ActorId(self.next_id);
        self.next_id += 1;
        self.by_actor.insert(id, entity);
        id
    }

    fn lookup(&self, id: ActorId) -> Option<Entity> {
        self.by_actor.get(&id).copied()
    }
}

// =====================================================================
// Snapshot construction
// =====================================================================

/// A POD copy of one candidate enemy collected via Query before
/// hostility / visibility filtering. Decoupling the query iteration
/// from the per-entity logic keeps `World` borrows non-overlapping.
struct CandidateActor {
    entity: Entity,
    pos: Point,
    faction_name: Option<String>,
    hp_current: i32,
    hp_max: i32,
    is_player: bool,
}

/// Build a `TurnSnapshot` for the given entity. Returns `None` if any
/// required component is missing (the dispatcher emits a `WaitIntent`
/// in that case to avoid stalling the turn loop).
///
/// Also returns the `IdMap` so `write_intent` can resolve `ActorId`
/// back to `Entity` for melee/ranged/chest targets, and the
/// `&'static` tactic slice for the resolver call.
fn build_snapshot(
    entity: Entity,
    world: &mut World,
) -> Option<(TurnSnapshot, IdMap, &'static [&'static dyn Tactic])> {
    // --- Self reads (all extracted into owned values to drop World borrows). ---
    let pos = world.get::<Position>(entity)?.to_point();
    let (hp_current, hp_max) = {
        let hp = world.get::<Health>(entity)?;
        (hp.current, hp.max)
    };
    let (
        engine_mode,
        flee_threshold,
        kites,
        kite_distance,
        chase_distance,
        chase_leash,
        last_known_player_pos,
        stationary,
    ) = {
        let ai = world.get::<MonsterAI>(entity)?;
        (
            engine_mode_to_resolver(ai.mode),
            ai.flee_at_hp_percent,
            ai.kites,
            ai.kite_distance,
            ai.chase_distance,
            ai.chase_leash,
            ai.last_known_player_position,
            ai.stationary,
        )
    };
    // Game-side Fleeing marker overrides the engine mode. See
    // src/game/fleeing.rs and docs/design/TACTICS.md §"FSM additions".
    let mode = match world.get::<Fleeing>(entity) {
        Some(fleeing) => AiMode::Fleeing {
            since_turn: fleeing.since_turn,
            last_known_threat_pos: fleeing.last_known_threat_pos,
        },
        None => engine_mode,
    };
    let viewshed = world.get::<Viewshed>(entity)?.clone();
    let self_faction_name = world.get::<Faction>(entity).map(|f| f.0 .0.clone());
    let (tactics, idle_movement, roam_target) = {
        let brain = world.get::<TacticBrain>(entity)?;
        (brain.tactics, brain.idle_movement, brain.roam_target)
    };
    // Convert spawn-time PatrolRoute (if any) into the snapshot view.
    let patrol = world.get::<PatrolRoute>(entity).and_then(patrol_route_to_view);
    let movement_mode = world
        .get::<MovementMode>(entity)
        .copied()
        .unwrap_or_default();
    let ranged_range = world.get::<RangedCapable>(entity).map(|r| r.range);
    let has_useable_ability = world
        .get::<MonsterAbilities>(entity)
        .map(|abs| abs.0.iter().any(|a| a.current_cooldown == 0))
        .unwrap_or(false);
    let (is_stunned, is_entangled) = {
        if let Some(effects) = world.get::<StatusEffects>(entity) {
            (effects.is_stunned(), effects.is_entangled())
        } else {
            (false, false)
        }
    };
    let is_submerged = world.get::<Submerged>(entity).is_some();
    // Squad leader position — only for non-leader followers whose
    // leader is alive. Self-leaders return None, soloists return None.
    let squad_leader_pos = {
        let my_squad = world.get::<SquadId>(entity).copied();
        let is_self_leader = world.get::<SquadLeader>(entity).is_some();
        match (my_squad, is_self_leader) {
            (Some(sid), false) => {
                let mut q = world
                    .query_filtered::<(&SquadId, &Position), With<SquadLeader>>();
                q.iter(world)
                    .find_map(|(s, p)| if *s == sid { Some(p.to_point()) } else { None })
            }
            _ => None,
        }
    };
    // Liquid layer check at the actor's tile. Used by SubmergeOrSurface.
    let on_liquid = {
        let map = world.resource::<Map>();
        if !map.in_bounds(pos) {
            false
        } else {
            !matches!(
                map.tiles[map.xy_idx(pos.x, pos.y)].liquid,
                LiquidType::None
            )
        }
    };

    // --- Collect candidate enemies from a single query, then drop the query. ---
    let candidates: Vec<CandidateActor> = {
        let mut q = world
            .query::<(Entity, &Position, Option<&Faction>, Option<&Health>, Option<&Player>)>();
        q.iter(world)
            .filter_map(|(e, p, f, h, player)| {
                if e == entity {
                    return None;
                }
                let pt = p.to_point();
                if !viewshed.visible_tiles.contains(&pt) {
                    return None;
                }
                Some(CandidateActor {
                    entity: e,
                    pos: pt,
                    faction_name: f.map(|fa| fa.0 .0.clone()),
                    hp_current: h.map_or(1, |hh| hh.current),
                    hp_max: h.map_or(1, |hh| hh.max),
                    is_player: player.is_some(),
                })
            })
            .collect()
    };

    // --- Faction-filter for hostility, then build EnemyView list. ---
    let matrix = world.resource::<FactionMatrix>().clone();
    let mut id_map = IdMap::default();
    let self_id = id_map.intern(entity);
    debug_assert_eq!(self_id.0, 0, "self should always be interned first");

    let mut visible_enemies: Vec<EnemyView> = candidates
        .into_iter()
        .filter_map(|c| {
            let hostile = match (&self_faction_name, &c.faction_name) {
                (Some(sf), Some(of)) => matrix.is_hostile_to(sf, of),
                _ => false,
            };
            if !hostile {
                return None;
            }
            let cheb = (c.pos.x - pos.x).abs().max((c.pos.y - pos.y).abs());
            let id = id_map.intern(c.entity);
            Some(EnemyView {
                id,
                pos: c.pos,
                hp_current: c.hp_current,
                hp_max: c.hp_max,
                chebyshev: cheb,
                is_adjacent: cheb <= 1,
                is_player: c.is_player,
            })
        })
        .collect();
    visible_enemies.sort_by_key(|e| e.chebyshev);

    // --- Pathfinding context. Arc<Map> snapshot to avoid borrow conflicts. ---
    let map_arc = {
        let map = world.resource::<Map>();
        std::sync::Arc::new(map.clone())
    };
    let paths: Box<dyn PathContext> = Box::new(MapPathContext {
        map: map_arc,
        mode: movement_mode,
    });

    let turn = world
        .resource::<roguelike_engine::turn::TurnManager>()
        .current_time;

    let self_view = ActorView {
        id: self_id,
        pos,
        hp_current,
        hp_max,
        mode,
        movement: movement_mode_to_kind(movement_mode),
        is_stunned,
        is_entangled,
        is_submerged,
        on_liquid,
        flee_threshold,
        kites,
        kite_distance,
        chase_distance,
        chase_leash,
        last_known_player_pos,
        patrol,
        stationary,
        ranged_range,
        has_useable_ability,
        squad_leader_pos,
        idle_movement,
        roam_target,
    };

    Some((
        TurnSnapshot {
            self_: self_view,
            visible_enemies,
            paths,
            turn,
        },
        id_map,
        tactics,
    ))
}

// =====================================================================
// State delta application
// =====================================================================

fn apply_state_delta(entity: Entity, delta: &TacticStateDelta, world: &mut World) {
    if let Some(mode) = delta.set_mode
        && let Some(mut ai) = world.get_mut::<MonsterAI>(entity)
    {
        ai.mode = resolver_mode_to_engine(mode);
    }
    if let Some(last_known) = delta.set_last_known_player_pos
        && let Some(mut ai) = world.get_mut::<MonsterAI>(entity)
    {
        ai.last_known_player_position = last_known;
    }
    if let Some(chase) = delta.set_chase_distance
        && let Some(mut ai) = world.get_mut::<MonsterAI>(entity)
    {
        ai.chase_distance = chase;
    }
    if let Some(new_target) = delta.set_roam_target
        && let Some(mut brain) = world.get_mut::<TacticBrain>(entity)
    {
        brain.roam_target = new_target;
    }
    if let Some(idx) = delta.set_waypoint_index
        && let Some(mut route) = world.get_mut::<PatrolRoute>(entity)
        && let PatrolState::Waypoint { ref mut current_index, .. } = route.state
    {
        *current_index = idx;
    }
    // set_ability_cooldown is reserved for a future tactic that picks
    // a specific ability slot — today the adapter delegates to
    // try_use_ability_world which manages its own cooldowns.
}

// =====================================================================
// Intent writing
// =====================================================================

fn write_intent(entity: Entity, action: TacticAction, id_map: &IdMap, world: &mut World) {
    match action {
        TacticAction::Move { dir } => {
            world
                .resource_mut::<Messages<MovementIntent>>()
                .write(MovementIntent {
                    entity,
                    dir: grid_dir_to_direction(dir),
                });
        }
        TacticAction::Melee { target } => {
            let Some(target_entity) = id_map.lookup(target) else {
                emit_wait(entity, world);
                return;
            };
            world
                .resource_mut::<Messages<MeleeIntent>>()
                .write(MeleeIntent {
                    attacker: entity,
                    target: target_entity,
                });
        }
        TacticAction::Ranged { target } => {
            let Some(target_entity) = id_map.lookup(target) else {
                emit_wait(entity, world);
                return;
            };
            world
                .resource_mut::<Messages<RangedAttackIntent>>()
                .write(RangedAttackIntent {
                    attacker: entity,
                    target: target_entity,
                });
        }
        TacticAction::UseAbility { .. } => {
            // Defer to the existing ability dispatcher, which handles
            // per-ability target selection + cooldown setting +
            // damage/heal/summon side effects. Either way, the turn is
            // consumed — we write WaitIntent so the turn loop's
            // ActionFinishedEvent contract is satisfied.
            let _used = crate::game::ai::try_use_ability_world(entity, world);
            emit_wait(entity, world);
        }
        TacticAction::PickUp => {
            world
                .resource_mut::<Messages<PickUpIntent>>()
                .write(PickUpIntent { entity });
        }
        TacticAction::OpenChest { chest } => {
            let Some(chest_entity) = id_map.lookup(chest) else {
                emit_wait(entity, world);
                return;
            };
            world
                .resource_mut::<Messages<OpenChestIntent>>()
                .write(OpenChestIntent {
                    entity,
                    chest_entity,
                });
        }
        TacticAction::DropAtHoard | TacticAction::OrderRetreat => {
            // Phase 5 wires these (Hoarder/Commander tactics).
            emit_wait(entity, world);
        }
        TacticAction::SetSubmerged(submerge) => {
            if submerge {
                world.entity_mut(entity).insert(Submerged);
            } else {
                world.entity_mut(entity).remove::<Submerged>();
            }
            emit_wait(entity, world);
        }
        TacticAction::Wait => emit_wait(entity, world),
    }
}

fn emit_wait(entity: Entity, world: &mut World) {
    world
        .resource_mut::<Messages<WaitIntent>>()
        .write(WaitIntent { entity });
}

// =====================================================================
// Dispatch system
// =====================================================================

pub fn tactic_dispatch_system(world: &mut World) {
    let entities: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (With<TacticBrain>, With<MyTurn>)>();
        q.iter(world).collect()
    };

    for entity in entities {
        let outcome = run_one_actor(entity, world);
        if let Some(mut brain) = world.get_mut::<TacticBrain>(entity) {
            brain.last_tactic = Some(outcome.tactic_name);
        }
        // ActionGuard / MyTurn cleanup mirrors monster_ai_dispatch.
        world.entity_mut(entity).insert(ActionGuard);
        world.entity_mut(entity).remove::<MyTurn>();
    }
}

fn run_one_actor(entity: Entity, world: &mut World) -> TurnOutcome {
    let (snapshot, id_map, tactics) = match build_snapshot(entity, world) {
        Some(parts) => parts,
        None => {
            emit_wait(entity, world);
            return TurnOutcome {
                tactic_name: "SnapshotMissing",
                action: TacticAction::Wait,
                delta: TacticStateDelta::default(),
            };
        }
    };

    let outcome = {
        let mut rng = world.resource_mut::<GameRng>();
        let mut adapter = BracketRngAdapter(&mut rng.0);
        resolve_turn(&snapshot, tactics, &mut adapter)
    };

    apply_state_delta(entity, &outcome.delta, world);
    write_intent(entity, outcome.action, &id_map, world);
    outcome
}

// =====================================================================
// Startup validation
// =====================================================================

/// Startup system that iterates `MonsterManifest` and panics on the
/// first `ai: TacticList([...])` entry that references a name absent
/// from `ALL_TACTIC_NAMES`. Catches typos at boot rather than at the
/// first time the monster spawns.
///
/// Also enforces that every list ends with the terminal `Wait`
/// tactic — the dispatcher's `FallbackWait` carries a different name
/// and skips state-delta application, so a malformed list silently
/// degrades behavior.
pub fn validate_tactic_names_system(
    manifests: Res<Assets<MonsterManifest>>,
    handle: Res<crate::assets::MonsterManifestHandle>,
) {
    use crate::assets::AiConfig;
    let Some(manifest) = manifests.get(&handle.0) else {
        return; // manifest not loaded yet; the run_if gate retries next frame
    };
    for (key, monster) in manifest.monsters.iter() {
        if let AiConfig::TacticList { tactics: names, .. } = &monster.ai {
            assert!(
                !names.is_empty(),
                "monster {key:?} has empty TacticList; every list needs at least one tactic",
            );
            for name in names {
                assert!(
                    ALL_TACTIC_NAMES.contains(&name.as_str()),
                    "monster {key:?} references unknown tactic {name:?}; \
                     known tactics: {:?}",
                    ALL_TACTIC_NAMES,
                );
            }
            let last = names.last().expect("just checked non-empty");
            assert_eq!(
                last,
                TERMINAL_TACTIC_NAME,
                "monster {key:?} TacticList must end with {TERMINAL_TACTIC_NAME:?} \
                 (the dispatcher's unconditional fallback). Last entry was {last:?}.",
            );
        }
    }
}

// =====================================================================
// Plugin
// =====================================================================

/// Registers the startup name-validation check. The
/// `tactic_dispatch_system` itself is registered as part of the Brain
/// phase chain in [`crate::game::turns`] alongside the other AI
/// dispatchers — see `TurnOrderPlugin::build`.
pub struct TacticsPlugin;

impl Plugin for TacticsPlugin {
    fn build(&self, app: &mut App) {
        // Validation runs once after the manifest finishes loading.
        // The run_if gate handles the asset-loading race.
        app.add_systems(
            Update,
            run_validation_once.run_if(
                resource_exists::<crate::assets::MonsterManifestHandle>
                    .and(not(resource_exists::<TacticsValidated>)),
            ),
        );
    }
}

#[derive(Resource)]
struct TacticsValidated;

fn run_validation_once(
    mut commands: Commands,
    manifests: Res<Assets<MonsterManifest>>,
    handle: Res<crate::assets::MonsterManifestHandle>,
) {
    // Asset may still be loading — bail and retry next frame.
    if manifests.get(&handle.0).is_none() {
        return;
    }
    validate_tactic_names_system(manifests, handle);
    commands.insert_resource(TacticsValidated);
}
