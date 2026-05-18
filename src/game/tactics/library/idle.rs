//! `IdleMove` — what the actor does when no combat tactic fires.
//!
//! Replaces `FreeWander` and `ErraticMove` from earlier phases. The
//! per-monster `IdleMovement` knob (read from `monsters.ron`) selects
//! one of four behaviours:
//!
//! - `PathToRandomTile` (default): pick a random walkable destination,
//!   pathfind toward it, and pick a new destination once arrived or
//!   blocked. Most monsters use this.
//! - `Patrol`: walk waypoints from a spawn-time `PatrolRoute::Waypoint`.
//!   When that route is absent the tactic passes.
//! - `Roam`: bounded random walk from a spawn-time
//!   `PatrolRoute::AreaRoam`. When that route is absent the tactic
//!   passes (no fallback bounds — design contract).
//! - `Stationary`: never produce idle movement.
//!
//! The tactic gates on `AiMode::Idle` only — combat-mode movement is
//! the job of the dedicated combat tactics (`HuntVisibleTarget`,
//! `PursueLastKnownPosition`, etc.).

use rand::RngCore;

use crate::game::tactics::resolve::{
    AiMode, GridDir, IdleMovementKind, PatrolView, Tactic, TacticAction, TacticStateDelta,
    TurnSnapshot,
};

pub struct IdleMove;

impl Tactic for IdleMove {
    fn name(&self) -> &'static str {
        "IdleMove"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Idle) {
            return None;
        }
        if snap.self_.stationary {
            return None;
        }
        match snap.self_.idle_movement {
            IdleMovementKind::Stationary => None,
            IdleMovementKind::PathToRandomTile => path_to_random_tile(snap, rng),
            IdleMovementKind::Patrol => patrol_step(snap),
            IdleMovementKind::Roam => roam_step(snap, rng),
        }
    }
}

/// `PathToRandomTile` dispatch: keep walking toward the current
/// `roam_target`. If we've arrived, or the target was never set, or
/// the pathfinder can't make progress, pick a new random walkable
/// tile and emit it as a state delta.
fn path_to_random_tile(
    snap: &TurnSnapshot,
    rng: &mut dyn RngCore,
) -> Option<(TacticAction, TacticStateDelta)> {
    let pos = snap.self_.pos;
    // Use the current target if we have one and we're not on top of it.
    let (target, refresh) = match snap.self_.roam_target {
        Some(t) if t != pos => (t, false),
        _ => (snap.paths.pick_random_walkable(rng)?, true),
    };
    match snap.paths.next_step_toward(pos, target) {
        Some(step) => {
            let dir = GridDir::from_step(pos, step)?;
            let delta = if refresh {
                TacticStateDelta {
                    set_roam_target: Some(Some(target)),
                    ..TacticStateDelta::default()
                }
            } else {
                TacticStateDelta::default()
            };
            Some((TacticAction::Move { dir }, delta))
        }
        None => {
            // Old target is unreachable from here. Refresh and try
            // again next turn rather than wasting two turns on a
            // failed pathfind.
            let new_target = snap.paths.pick_random_walkable(rng)?;
            let delta = TacticStateDelta {
                set_roam_target: Some(Some(new_target)),
                ..TacticStateDelta::default()
            };
            Some((TacticAction::Wait, delta))
        }
    }
}

/// `Patrol` dispatch: read the spawn-time `PatrolView::Waypoint`
/// route, walk to the current waypoint, advance the index on arrival
/// (or on pathfind failure so we skip blocked entries).
fn patrol_step(snap: &TurnSnapshot) -> Option<(TacticAction, TacticStateDelta)> {
    let Some(PatrolView::Waypoint { points, current_index }) = snap.self_.patrol.as_ref()
    else {
        return None;
    };
    if points.is_empty() {
        return None;
    }
    let pos = snap.self_.pos;
    let target = points[*current_index];
    if pos == target {
        // Arrived — advance and step toward the next waypoint this turn.
        let next_idx = (*current_index + 1) % points.len();
        let next_target = points[next_idx];
        let step = snap.paths.next_step_toward(pos, next_target)?;
        let dir = GridDir::from_step(pos, step)?;
        let delta = TacticStateDelta {
            set_waypoint_index: Some(next_idx),
            ..TacticStateDelta::default()
        };
        return Some((TacticAction::Move { dir }, delta));
    }
    match snap.paths.next_step_toward(pos, target) {
        Some(step) => {
            let dir = GridDir::from_step(pos, step)?;
            Some((TacticAction::Move { dir }, TacticStateDelta::default()))
        }
        None => {
            // Blocked — skip this waypoint, try the next one next turn.
            let next_idx = (*current_index + 1) % points.len();
            let delta = TacticStateDelta {
                set_waypoint_index: Some(next_idx),
                ..TacticStateDelta::default()
            };
            Some((TacticAction::Wait, delta))
        }
    }
}

/// `Roam` dispatch: bounded random walk within
/// `PatrolView::AreaRoam { min, max }`. Returns `None` (passes the
/// tactic) when no `PatrolRoute::AreaRoam` is attached — no fallback
/// to global wander, by design.
fn roam_step(
    snap: &TurnSnapshot,
    rng: &mut dyn RngCore,
) -> Option<(TacticAction, TacticStateDelta)> {
    let Some(PatrolView::AreaRoam { min, max }) = snap.self_.patrol.as_ref() else {
        return None;
    };
    // Try a handful of random offsets; reject moves outside the box.
    for _ in 0..8 {
        let step = snap.paths.pick_random_nearby(snap.self_.pos, 1, rng)?;
        if step.x < min.x || step.x > max.x || step.y < min.y || step.y > max.y {
            continue;
        }
        let dir = GridDir::from_step(snap.self_.pos, step)?;
        return Some((TacticAction::Move { dir }, TacticStateDelta::default()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    fn idle_actor() -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Idle;
        snapshot_with(actor)
    }

    // ----- Mode + stationary gating -----

    #[test]
    fn passes_when_not_idle() {
        let mut snap = idle_actor();
        snap.self_.mode = AiMode::Hunting;
        let mut rng = test_rng();
        assert!(IdleMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn passes_when_stationary_flag_set() {
        let mut snap = idle_actor();
        snap.self_.stationary = true;
        let mut rng = test_rng();
        assert!(IdleMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn passes_when_idle_movement_is_stationary() {
        let mut snap = idle_actor();
        snap.self_.idle_movement = IdleMovementKind::Stationary;
        let mut rng = test_rng();
        assert!(IdleMove.evaluate(&snap, &mut rng).is_none());
    }

    // ----- PathToRandomTile -----

    #[test]
    fn path_to_random_picks_a_target_when_none_set() {
        let snap = idle_actor();
        let mut rng = test_rng();
        let (action, delta) = IdleMove.evaluate(&snap, &mut rng).expect("should pick target");
        // ToyPaths::pick_random_walkable returns (20, 20).
        assert_eq!(delta.set_roam_target, Some(Some(Point::new(20, 20))));
        assert!(matches!(action, TacticAction::Move { .. }));
    }

    #[test]
    fn path_to_random_keeps_existing_target_when_not_arrived() {
        let mut snap = idle_actor();
        snap.self_.roam_target = Some(Point::new(15, 15));
        let mut rng = test_rng();
        let (action, delta) = IdleMove.evaluate(&snap, &mut rng).expect("should walk");
        // No refresh — we're still en route to (15, 15).
        assert_eq!(delta.set_roam_target, None);
        assert!(matches!(action, TacticAction::Move { .. }));
    }

    #[test]
    fn path_to_random_refreshes_target_when_arrived() {
        let mut snap = idle_actor();
        snap.self_.pos = Point::new(7, 7);
        snap.self_.roam_target = Some(Point::new(7, 7)); // Already there
        let mut rng = test_rng();
        let (_action, delta) = IdleMove.evaluate(&snap, &mut rng).expect("should refresh");
        assert_eq!(delta.set_roam_target, Some(Some(Point::new(20, 20))));
    }

    #[test]
    fn path_to_random_refreshes_on_pathfind_failure() {
        let mut snap = idle_actor();
        snap.self_.roam_target = Some(Point::new(15, 15));
        snap.paths = Box::new(BlockedPathsButPickWorks);
        let mut rng = test_rng();
        let (action, delta) = IdleMove.evaluate(&snap, &mut rng).expect("should refresh");
        assert!(matches!(action, TacticAction::Wait));
        assert!(delta.set_roam_target.is_some());
    }

    /// Path context where `next_step_toward` always fails but
    /// `pick_random_walkable` works. Used to test the refresh-on-block
    /// branch of PathToRandomTile.
    struct BlockedPathsButPickWorks;
    impl PathContext for BlockedPathsButPickWorks {
        fn next_step_toward(&self, _: Point, _: Point) -> Option<Point> {
            None
        }
        fn next_flee_step(&self, _: Point, _: Point) -> Option<Point> {
            None
        }
        fn pick_random_nearby(&self, _: Point, _: i32, _: &mut dyn RngCore) -> Option<Point> {
            None
        }
        fn pick_random_walkable(&self, _: &mut dyn RngCore) -> Option<Point> {
            Some(Point::new(30, 30))
        }
    }
    use crate::game::tactics::resolve::PathContext;

    // ----- Patrol -----

    #[test]
    fn patrol_passes_when_no_patrol_route_attached() {
        let mut snap = idle_actor();
        snap.self_.idle_movement = IdleMovementKind::Patrol;
        let mut rng = test_rng();
        // No patrol view set; tactic should pass.
        assert!(IdleMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn patrol_walks_toward_current_waypoint() {
        let mut snap = idle_actor();
        snap.self_.idle_movement = IdleMovementKind::Patrol;
        snap.self_.patrol = Some(PatrolView::Waypoint {
            points: vec![Point::new(10, 5), Point::new(10, 10)],
            current_index: 0,
        });
        let mut rng = test_rng();
        let (action, delta) = IdleMove.evaluate(&snap, &mut rng).unwrap();
        // ToyPaths steps east toward (10, 5).
        assert!(matches!(action, TacticAction::Move { dir: GridDir::E }));
        assert_eq!(delta.set_waypoint_index, None);
    }

    #[test]
    fn patrol_advances_on_arrival() {
        let mut snap = idle_actor();
        snap.self_.pos = Point::new(10, 5);
        snap.self_.idle_movement = IdleMovementKind::Patrol;
        snap.self_.patrol = Some(PatrolView::Waypoint {
            points: vec![Point::new(10, 5), Point::new(10, 10)],
            current_index: 0,
        });
        let mut rng = test_rng();
        let (_, delta) = IdleMove.evaluate(&snap, &mut rng).unwrap();
        assert_eq!(delta.set_waypoint_index, Some(1));
    }

    // ----- Roam -----

    #[test]
    fn roam_passes_when_no_patrol_route_attached() {
        let mut snap = idle_actor();
        snap.self_.idle_movement = IdleMovementKind::Roam;
        let mut rng = test_rng();
        assert!(IdleMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn roam_steps_within_bounds() {
        let mut snap = idle_actor();
        snap.self_.idle_movement = IdleMovementKind::Roam;
        snap.self_.patrol = Some(PatrolView::AreaRoam {
            min: Point::new(0, 0),
            max: Point::new(100, 100),
        });
        let mut rng = test_rng();
        // ToyPaths returns (pos.x + 1, pos.y) — inside the box.
        let (action, _) = IdleMove.evaluate(&snap, &mut rng).expect("should roam");
        assert!(matches!(action, TacticAction::Move { dir: GridDir::E }));
    }

    #[test]
    fn roam_passes_when_only_proposed_step_is_outside_bounds() {
        let mut snap = idle_actor();
        snap.self_.pos = Point::new(5, 5);
        snap.self_.idle_movement = IdleMovementKind::Roam;
        // Tiny box that excludes (6, 5) — ToyPaths' only proposal.
        snap.self_.patrol = Some(PatrolView::AreaRoam {
            min: Point::new(4, 4),
            max: Point::new(5, 5),
        });
        let mut rng = test_rng();
        assert!(IdleMove.evaluate(&snap, &mut rng).is_none());
    }
}
