//! Pure AI tactic resolver. No Bevy, no ECS, no globals.
//!
//! One call resolves one monster's turn from snapshot to action. The
//! Bevy adapter ([`crate::game::tactics::dispatch`]) gathers a
//! [`TurnSnapshot`] from ECS components, runs the tactic list via
//! [`resolve_turn`], writes the resulting intent message, and applies
//! the [`TacticStateDelta`] back to ECS components.
//!
//! ## Two-tier structure
//!
//! - [`resolve_turn`] — the single entry point. Walks the tactic list
//!   top-to-bottom, first non-`None` wins. Always returns *something*
//!   (falls through to `Wait` if every tactic passes).
//! - [`Tactic`] trait — implementors are zero-sized structs in the
//!   `library/` subdirectory. Each has one method: `evaluate`.
//!
//! ## Snapshots
//!
//! Snapshots are plain-data views the adapter builds from ECS
//! components. None of them implements `Default` — explicit
//! construction at the adapter boundary catches the "I forgot to copy
//! `attrs`" wiring bug at compile time rather than producing silent
//! wrong-behavior at runtime. The pattern mirrors
//! [`crate::game::combat::resolve`].
//!
//! ## State updates
//!
//! Tactics never mutate the snapshot or `World`. State updates flow
//! back through [`TacticStateDelta`]; the adapter applies them after
//! the action lands. This keeps every tactic unit-testable with a
//! hand-built snapshot — see the `#[cfg(test)]` modules of each
//! shipping tactic.

use bracket_lib::prelude::Point;
use rand::RngCore;

// =====================================================================
// Identity & opaque handles
// =====================================================================

/// Opaque entity handle the resolver passes around. The adapter holds
/// the `ActorId ↔ bevy::Entity` mapping for one tick; the resolver
/// only ever compares ids by equality and forwards them in outcomes.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ActorId(pub u64);

/// Index into the monster's ability list. The adapter resolves the
/// slot back to a concrete `AbilityKind` when dispatching.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AbilitySlot(pub u8);

/// Eight-way direction. Tactics emit these; the adapter converts to
/// the engine's existing `Direction` type before writing
/// `MovementIntent`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GridDir {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl GridDir {
    /// Map a one-step `(dx, dy)` delta (each in -1..=1, not both zero)
    /// to a direction.
    pub fn from_delta(dx: i32, dy: i32) -> Option<GridDir> {
        match (dx.signum(), dy.signum()) {
            (0, -1) => Some(GridDir::N),
            (1, -1) => Some(GridDir::NE),
            (1, 0) => Some(GridDir::E),
            (1, 1) => Some(GridDir::SE),
            (0, 1) => Some(GridDir::S),
            (-1, 1) => Some(GridDir::SW),
            (-1, 0) => Some(GridDir::W),
            (-1, -1) => Some(GridDir::NW),
            _ => None,
        }
    }

    /// Compute the direction from `from` to `to` when they are exactly
    /// one tile apart on the eight-way grid.
    pub fn from_step(from: Point, to: Point) -> Option<GridDir> {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        if dx.abs() > 1 || dy.abs() > 1 || (dx == 0 && dy == 0) {
            return None;
        }
        Self::from_delta(dx, dy)
    }
}

// =====================================================================
// FSM mode (mirror of engine's MonsterAIMode + Fleeing addition)
// =====================================================================

/// The high-level FSM state a monster is in. Mirrors the engine's
/// `MonsterAIMode` for the existing variants. `Fleeing` is the new
/// sticky mode the migration adds (see `TACTICS.md` §"FSM additions").
///
/// The resolver type lives separately from the engine type so the
/// pure module has no Bevy dependency. The adapter converts between
/// them at the boundary.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AiMode {
    Asleep,
    Idle,
    Hunting,
    /// Sticky panic state. Entered via the `damage_triggers_flee`
    /// system; exited only via the time-and-condition gated
    /// `maybe_exit_fleeing` system, never spontaneously by a tactic.
    Fleeing {
        since_turn: u32,
        last_known_threat_pos: Option<Point>,
    },
}

/// How an actor moves over terrain. Mirrors the engine's
/// `MovementMode`. The adapter copies the value into the snapshot.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum MovementKind {
    #[default]
    Land,
    Aquatic,
    Amphibious,
    Flying,
}

// =====================================================================
// Snapshot views
// =====================================================================

/// Read view of the actor whose turn this is. No `Default` —
/// every field must be set explicitly at the adapter boundary.
#[derive(Clone, Debug)]
pub struct ActorView {
    pub id: ActorId,
    pub pos: Point,
    pub hp_current: i32,
    pub hp_max: i32,
    pub mode: AiMode,
    pub movement: MovementKind,
    pub is_stunned: bool,
    pub is_entangled: bool,
    pub is_submerged: bool,
    pub on_liquid: bool,
    /// 0.0 means "never flee". Read from `MonsterAI.flee_at_hp_percent`.
    pub flee_threshold: f32,
    pub kites: bool,
    pub kite_distance: u32,
    pub erratic_chance: f32,
    pub chase_distance: u32,
    pub chase_leash: u32,
    pub last_known_player_pos: Option<Point>,
    pub patrol: Option<PatrolView>,
    pub stationary: bool,
    /// `None` for monsters without ranged abilities.
    pub ranged_range: Option<u32>,
}

/// Read view of a visible enemy. The snapshot builder pre-filters by
/// faction hostility and viewshed, and sorts nearest-first.
#[derive(Clone, Debug)]
pub struct EnemyView {
    pub id: ActorId,
    pub pos: Point,
    pub hp_current: i32,
    pub hp_max: i32,
    /// Chebyshev distance (max of |dx|, |dy|). Pre-computed for the
    /// adjacency check most tactics need.
    pub chebyshev: i32,
    pub is_adjacent: bool,
    pub is_player: bool,
}

/// Read view of a patrol route. The waypoint variant carries the
/// current index; tactics update it via `TacticStateDelta`.
#[derive(Clone, Debug)]
pub enum PatrolView {
    Sentry {
        home: Point,
        radius: i32,
    },
    Waypoint {
        points: Vec<Point>,
        current_index: usize,
    },
    AreaRoam {
        min: Point,
        max: Point,
    },
    FreeWander,
}

/// Pathfinding port. Tactics ask "give me the next step" and receive
/// a single tile or `None`. The adapter implements this over the live
/// `Map` + `MovementMode`. Tests inject toy implementations (see
/// [`test_support`]).
pub trait PathContext: Send {
    /// Single step from `from` toward `to`, respecting walkability +
    /// movement mode. `None` when no walkable adjacent tile makes
    /// progress.
    fn next_step_toward(&self, from: Point, to: Point) -> Option<Point>;

    /// Single step AWAY from `threat`. Tries the primary flee axis
    /// first, falls back to perpendicular options. `None` when
    /// cornered (every adjacent tile is blocked or moves toward).
    fn next_flee_step(&self, from: Point, threat: Point) -> Option<Point>;

    /// Pick a random walkable tile within `radius` of `from`. Used by
    /// `AreaRoam` and erratic movement.
    fn pick_random_nearby(
        &self,
        from: Point,
        radius: i32,
        rng: &mut dyn RngCore,
    ) -> Option<Point>;
}

/// Everything a tactic is allowed to read about the world this turn.
/// Built once per actor turn by the adapter; passed by reference to
/// every tactic in the chain.
///
/// No `Default`. Constructors at the adapter boundary; test fixtures
/// at the test boundary.
pub struct TurnSnapshot {
    pub self_: ActorView,
    /// Faction-filtered, viewshed-filtered, sorted nearest-first by
    /// Chebyshev distance. Empty when nothing hostile is visible.
    pub visible_enemies: Vec<EnemyView>,
    /// Pathfinding port. See [`PathContext`].
    pub paths: Box<dyn PathContext>,
    /// Current global turn counter (used by Fleeing mode entry/exit).
    pub turn: u32,
}

// =====================================================================
// Outcome
// =====================================================================

/// What a tactic decided. The adapter translates each variant into the
/// matching existing Bevy intent message.
#[derive(Copy, Clone, Debug)]
pub enum TacticAction {
    Move { dir: GridDir },
    Melee { target: ActorId },
    Ranged { target: ActorId },
    UseAbility { slot: AbilitySlot, target: Option<ActorId> },
    PickUp,
    OpenChest { chest: ActorId },
    DropAtHoard,
    SetSubmerged(bool),
    OrderRetreat,
    Wait,
}

/// Persistent state updates the tactic produced. The adapter applies
/// them to the live ECS components after the action lands. The
/// resolver never mutates the snapshot itself.
#[derive(Clone, Debug, Default)]
pub struct TacticStateDelta {
    pub set_mode: Option<AiMode>,
    pub set_last_known_player_pos: Option<Option<Point>>,
    pub set_chase_distance: Option<u32>,
    pub set_waypoint_index: Option<usize>,
    pub set_ability_cooldown: Option<(AbilitySlot, u32)>,
}

/// One full turn's outcome. `tactic_name` powers the `last_tactic`
/// display in the `nearby.rs` UI sidebar without coupling the UI to
/// an enum.
#[derive(Clone, Debug)]
pub struct TurnOutcome {
    pub tactic_name: &'static str,
    pub action: TacticAction,
    pub delta: TacticStateDelta,
}

// =====================================================================
// Tactic trait
// =====================================================================

/// One named decision rule. Pure: inputs are `&TurnSnapshot` and a
/// mutable RNG; output is `Option<(TacticAction, TacticStateDelta)>`.
///
/// - `Some(...)` = this tactic fired. The dispatcher stops scanning.
/// - `None` = pass; try the next tactic.
///
/// Implementors are zero-sized structs. Per-monster customization
/// comes from which tactics are in the list and in what order, not
/// from constructor args.
pub trait Tactic: Send + Sync {
    fn name(&self) -> &'static str;
    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)>;
}

/// An ordered list of tactic references. Spawned monsters carry one
/// of these on their `TacticBrain` component. The `'static` lifetime
/// matches the `phf::Map`-backed registry; [`resolve_turn`] itself
/// accepts any lifetime so unit tests can use stack-local fixtures.
pub type TacticList = &'static [&'static dyn Tactic];

// =====================================================================
// Pre-tactic hard-skip checks
// =====================================================================

/// Universal "you can't act this turn" checks that run before any
/// tactic. Returns `Some(Wait outcome)` when the actor is stunned or
/// entangled; `None` to proceed to tactic evaluation.
fn maybe_skip_turn(snap: &TurnSnapshot) -> Option<TurnOutcome> {
    if snap.self_.is_stunned {
        return Some(TurnOutcome {
            tactic_name: "StunnedSkip",
            action: TacticAction::Wait,
            delta: TacticStateDelta::default(),
        });
    }
    if snap.self_.is_entangled {
        return Some(TurnOutcome {
            tactic_name: "EntangledSkip",
            action: TacticAction::Wait,
            delta: TacticStateDelta::default(),
        });
    }
    None
}

// =====================================================================
// The entry point
// =====================================================================

/// Run the tactic list. First non-`None` wins; falls through to
/// `Wait` if every tactic passes (every well-formed monster list ends
/// with `Wait`, so the fallback should never fire in practice).
///
/// Always returns exactly one outcome — the adapter is guaranteed to
/// have something to write.
pub fn resolve_turn(
    snap: &TurnSnapshot,
    tactics: &[&dyn Tactic],
    rng: &mut dyn RngCore,
) -> TurnOutcome {
    if let Some(skip) = maybe_skip_turn(snap) {
        return skip;
    }
    for tactic in tactics {
        if let Some((action, delta)) = tactic.evaluate(snap, rng) {
            return TurnOutcome {
                tactic_name: tactic.name(),
                action,
                delta,
            };
        }
    }
    TurnOutcome {
        tactic_name: "FallbackWait",
        action: TacticAction::Wait,
        delta: TacticStateDelta::default(),
    }
}

// =====================================================================
// Test support
// =====================================================================

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared helpers for unit-testing tactics. Each shipping tactic
    //! has its own `#[cfg(test)] mod tests` that uses these.

    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    /// Path context that always returns a one-step move toward the
    /// target along the dominant axis. Useful for testing tactics
    /// where pathfinding success is incidental to the test.
    pub struct ToyPaths;

    impl PathContext for ToyPaths {
        fn next_step_toward(&self, from: Point, to: Point) -> Option<Point> {
            if from == to {
                return None;
            }
            let dx = to.x - from.x;
            let dy = to.y - from.y;
            Some(Point::new(
                from.x + dx.signum(),
                from.y + dy.signum(),
            ))
        }
        fn next_flee_step(&self, from: Point, threat: Point) -> Option<Point> {
            let dx = from.x - threat.x;
            let dy = from.y - threat.y;
            if dx == 0 && dy == 0 {
                return None;
            }
            // Match `flee_direction`'s axis preference (larger axis wins).
            let (step_dx, step_dy) = if dx.abs() >= dy.abs() {
                (dx.signum(), 0)
            } else {
                (0, dy.signum())
            };
            Some(Point::new(from.x + step_dx, from.y + step_dy))
        }
        fn pick_random_nearby(
            &self,
            from: Point,
            _radius: i32,
            _rng: &mut dyn RngCore,
        ) -> Option<Point> {
            Some(Point::new(from.x + 1, from.y))
        }
    }

    /// Path context that fails every query. Useful for testing the
    /// "cornered" path of flee/hunt tactics.
    pub struct BlockedPaths;

    impl PathContext for BlockedPaths {
        fn next_step_toward(&self, _from: Point, _to: Point) -> Option<Point> {
            None
        }
        fn next_flee_step(&self, _from: Point, _threat: Point) -> Option<Point> {
            None
        }
        fn pick_random_nearby(
            &self,
            _from: Point,
            _radius: i32,
            _rng: &mut dyn RngCore,
        ) -> Option<Point> {
            None
        }
    }

    /// Build a minimal `ActorView`. Tests override fields they care
    /// about and accept defaults for the rest. Note: this lives in
    /// `test_support` rather than `Default::default()` to keep the
    /// production type `Default`-free.
    pub fn test_actor(id: u64, pos: Point) -> ActorView {
        ActorView {
            id: ActorId(id),
            pos,
            hp_current: 10,
            hp_max: 10,
            mode: AiMode::Idle,
            movement: MovementKind::Land,
            is_stunned: false,
            is_entangled: false,
            is_submerged: false,
            on_liquid: false,
            flee_threshold: 0.0,
            kites: false,
            kite_distance: 0,
            erratic_chance: 0.0,
            chase_distance: 0,
            chase_leash: 0,
            last_known_player_pos: None,
            patrol: None,
            stationary: false,
            ranged_range: None,
        }
    }

    /// Build a minimal `EnemyView` representing the player.
    pub fn test_player(pos: Point, hp: i32) -> EnemyView {
        let chebyshev = pos.x.abs().max(pos.y.abs()); // placeholder; tests set explicitly
        EnemyView {
            id: ActorId(0),
            pos,
            hp_current: hp,
            hp_max: hp,
            chebyshev,
            is_adjacent: chebyshev <= 1,
            is_player: true,
        }
    }

    /// Build a snapshot with the given actor + no visible enemies.
    /// Tests can mutate `.visible_enemies` after construction.
    pub fn snapshot_with(self_: ActorView) -> TurnSnapshot {
        TurnSnapshot {
            self_,
            visible_enemies: Vec::new(),
            paths: Box::new(ToyPaths),
            turn: 0,
        }
    }

    /// Snapshot with one player enemy at `enemy_pos` relative to
    /// self. Computes Chebyshev distance correctly.
    pub fn snapshot_with_enemy(self_: ActorView, enemy_pos: Point, enemy_hp: i32) -> TurnSnapshot {
        let cheb = (enemy_pos.x - self_.pos.x)
            .abs()
            .max((enemy_pos.y - self_.pos.y).abs());
        let enemy = EnemyView {
            id: ActorId(0),
            pos: enemy_pos,
            hp_current: enemy_hp,
            hp_max: enemy_hp,
            chebyshev: cheb,
            is_adjacent: cheb <= 1,
            is_player: true,
        };
        TurnSnapshot {
            self_,
            visible_enemies: vec![enemy],
            paths: Box::new(ToyPaths),
            turn: 0,
        }
    }

    /// Convenience RNG for tests — deterministic seed.
    pub fn test_rng() -> SmallRng {
        SmallRng::seed_from_u64(0)
    }
}

// =====================================================================
// Resolver-level tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    /// A tactic that always returns `None`. For testing fallthrough.
    struct AlwaysPass;
    impl Tactic for AlwaysPass {
        fn name(&self) -> &'static str {
            "AlwaysPass"
        }
        fn evaluate(
            &self,
            _snap: &TurnSnapshot,
            _rng: &mut dyn RngCore,
        ) -> Option<(TacticAction, TacticStateDelta)> {
            None
        }
    }

    /// A tactic that always returns `Wait` with a tagged name. For
    /// testing first-match-wins.
    struct AlwaysWait;
    impl Tactic for AlwaysWait {
        fn name(&self) -> &'static str {
            "AlwaysWait"
        }
        fn evaluate(
            &self,
            _snap: &TurnSnapshot,
            _rng: &mut dyn RngCore,
        ) -> Option<(TacticAction, TacticStateDelta)> {
            Some((TacticAction::Wait, TacticStateDelta::default()))
        }
    }

    #[test]
    fn falls_through_to_wait_when_every_tactic_passes() {
        let snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        let mut rng = test_rng();
        let pass1: &dyn Tactic = &AlwaysPass;
        let pass2: &dyn Tactic = &AlwaysPass;
        let tactics: &[&dyn Tactic] = &[pass1, pass2];
        let outcome = resolve_turn(&snap, tactics, &mut rng);
        assert_eq!(outcome.tactic_name, "FallbackWait");
        assert!(matches!(outcome.action, TacticAction::Wait));
    }

    #[test]
    fn first_matching_tactic_wins_and_lower_tactics_dont_run() {
        let snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        let mut rng = test_rng();
        let wait_tactic: &dyn Tactic = &AlwaysWait;
        let pass_tactic: &dyn Tactic = &AlwaysPass;
        let tactics: &[&dyn Tactic] = &[wait_tactic, pass_tactic];
        let outcome = resolve_turn(&snap, tactics, &mut rng);
        assert_eq!(outcome.tactic_name, "AlwaysWait");
    }

    #[test]
    fn stun_short_circuits_before_tactic_evaluation() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.is_stunned = true;
        let snap = snapshot_with(actor);
        let mut rng = test_rng();
        let wait_tactic: &dyn Tactic = &AlwaysWait;
        let tactics: &[&dyn Tactic] = &[wait_tactic];
        let outcome = resolve_turn(&snap, tactics, &mut rng);
        assert_eq!(outcome.tactic_name, "StunnedSkip");
    }

    #[test]
    fn entangle_short_circuits_before_tactic_evaluation() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.is_entangled = true;
        let snap = snapshot_with(actor);
        let mut rng = test_rng();
        let wait_tactic: &dyn Tactic = &AlwaysWait;
        let tactics: &[&dyn Tactic] = &[wait_tactic];
        let outcome = resolve_turn(&snap, tactics, &mut rng);
        assert_eq!(outcome.tactic_name, "EntangledSkip");
    }

    #[test]
    fn grid_dir_from_step_handles_all_eight_directions() {
        let origin = Point::new(0, 0);
        assert_eq!(GridDir::from_step(origin, Point::new(0, -1)), Some(GridDir::N));
        assert_eq!(GridDir::from_step(origin, Point::new(1, -1)), Some(GridDir::NE));
        assert_eq!(GridDir::from_step(origin, Point::new(1, 0)), Some(GridDir::E));
        assert_eq!(GridDir::from_step(origin, Point::new(1, 1)), Some(GridDir::SE));
        assert_eq!(GridDir::from_step(origin, Point::new(0, 1)), Some(GridDir::S));
        assert_eq!(GridDir::from_step(origin, Point::new(-1, 1)), Some(GridDir::SW));
        assert_eq!(GridDir::from_step(origin, Point::new(-1, 0)), Some(GridDir::W));
        assert_eq!(GridDir::from_step(origin, Point::new(-1, -1)), Some(GridDir::NW));
    }

    #[test]
    fn grid_dir_rejects_same_point_and_far_targets() {
        let origin = Point::new(0, 0);
        assert_eq!(GridDir::from_step(origin, origin), None);
        assert_eq!(GridDir::from_step(origin, Point::new(2, 0)), None);
        assert_eq!(GridDir::from_step(origin, Point::new(-2, -2)), None);
    }

    #[test]
    fn empty_tactic_list_still_returns_fallback_wait() {
        let snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        let mut rng = test_rng();
        let tactics: &[&dyn Tactic] = &[];
        let outcome = resolve_turn(&snap, tactics, &mut rng);
        assert_eq!(outcome.tactic_name, "FallbackWait");
        assert!(matches!(outcome.action, TacticAction::Wait));
    }
}
