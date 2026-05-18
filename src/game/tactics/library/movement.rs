//! `HuntVisibleTarget` — pathfind toward the nearest visible enemy
//! while in Hunting mode. Also records the enemy's position as the
//! new `last_known_player_position` for the chase-after-LOS-loss
//! pursuit tactic.
//!
//! `PursueLastKnownPosition` — when no enemy is visible but Hunting
//! mode is preserved (Awareness state held until expiry), walk
//! toward the last-known position. Gives up via `chase_leash`.
//!
//! `FreeWander` — bounded random walk while in Idle mode. The simplest
//! patrol — no waypoints, no home position, just pick a walkable
//! adjacent tile and step there. Used by monsters without a
//! `PatrolRoute` (e.g., the Giant Rat canary).

use rand::RngCore;
use roguelike_engine::ai::decisions::should_give_up_chase;

use crate::game::tactics::resolve::{
    AiMode, GridDir, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

pub struct HuntVisibleTarget;

impl Tactic for HuntVisibleTarget {
    fn name(&self) -> &'static str {
        "HuntVisibleTarget"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Hunting) {
            return None;
        }
        if snap.self_.stationary {
            return None;
        }
        let target = snap.visible_enemies.first()?;

        // Already adjacent: don't move (let MeleeAdjacent handle it).
        // This tactic only fires when there's a path-step to take.
        if target.is_adjacent {
            return None;
        }

        let step = snap.paths.next_step_toward(snap.self_.pos, target.pos)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;

        // Update last-known + reset chase distance (we just saw the target).
        let delta = TacticStateDelta {
            set_last_known_player_pos: Some(Some(target.pos)),
            set_chase_distance: Some(0),
            ..TacticStateDelta::default()
        };
        Some((TacticAction::Move { dir }, delta))
    }
}

pub struct PursueLastKnownPosition;

impl Tactic for PursueLastKnownPosition {
    fn name(&self) -> &'static str {
        "PursueLastKnownPosition"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Hunting) {
            return None;
        }
        if snap.self_.stationary {
            return None;
        }
        // Only fires when no enemy is visible — visible cases are
        // handled by HuntVisibleTarget upstream.
        if !snap.visible_enemies.is_empty() {
            return None;
        }
        let target = snap.self_.last_known_player_pos?;

        // Already at the last-known tile: nothing to pursue further.
        if snap.self_.pos == target {
            return None;
        }

        // Give-up check: if we've been chasing too long with no LOS,
        // surrender the hunt. The mode-update layer will downgrade
        // Hunting → Idle on its own cadence; this tactic just stops
        // emitting movement intents so the lower tactics (patrol,
        // wander) get a chance to fire.
        if should_give_up_chase(snap.self_.chase_distance, snap.self_.chase_leash) {
            return None;
        }

        let step = snap.paths.next_step_toward(snap.self_.pos, target)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;

        // Increment chase distance — we're spending a turn pursuing
        // without LOS confirmation.
        let delta = TacticStateDelta {
            set_chase_distance: Some(snap.self_.chase_distance + 1),
            ..TacticStateDelta::default()
        };
        Some((TacticAction::Move { dir }, delta))
    }
}

/// Bounded random walk while in Idle mode. Picks a random walkable
/// adjacent tile via `PathContext::pick_random_nearby` with radius 1.
/// Returns `None` (passes to next tactic) when no walkable neighbor
/// exists or when the actor is in any non-Idle mode. Stationary
/// monsters never wander.
pub struct FreeWander;

impl Tactic for FreeWander {
    fn name(&self) -> &'static str {
        "FreeWander"
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
        let step = snap.paths.pick_random_nearby(snap.self_.pos, 1, rng)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;
        Some((TacticAction::Move { dir }, TacticStateDelta::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    // ----- HuntVisibleTarget -----

    fn hunting_with_visible_target(target: Point) -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        snapshot_with_enemy(actor, target, 30)
    }

    #[test]
    fn moves_toward_visible_enemy_in_hunting_mode() {
        // Enemy at (8, 5) — east, distance 3 (not adjacent).
        let snap = hunting_with_visible_target(Point::new(8, 5));
        let mut rng = test_rng();
        let outcome = HuntVisibleTarget
            .evaluate(&snap, &mut rng)
            .expect("should hunt");
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::E }));
    }

    #[test]
    fn hunt_records_last_known_and_resets_chase_distance() {
        let snap = hunting_with_visible_target(Point::new(8, 5));
        let mut rng = test_rng();
        let outcome = HuntVisibleTarget.evaluate(&snap, &mut rng).unwrap();
        assert_eq!(outcome.1.set_last_known_player_pos, Some(Some(Point::new(8, 5))));
        assert_eq!(outcome.1.set_chase_distance, Some(0));
    }

    #[test]
    fn hunt_passes_when_target_already_adjacent() {
        // Adjacent target — let MeleeAdjacent handle it.
        let snap = hunting_with_visible_target(Point::new(6, 5));
        let mut rng = test_rng();
        assert!(HuntVisibleTarget.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn hunt_passes_when_not_hunting() {
        let mut snap = hunting_with_visible_target(Point::new(8, 5));
        snap.self_.mode = AiMode::Idle;
        let mut rng = test_rng();
        assert!(HuntVisibleTarget.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn hunt_passes_when_stationary() {
        let mut snap = hunting_with_visible_target(Point::new(8, 5));
        snap.self_.stationary = true;
        let mut rng = test_rng();
        assert!(HuntVisibleTarget.evaluate(&snap, &mut rng).is_none());
    }

    // ----- PursueLastKnownPosition -----

    fn hunting_with_last_known(last_known: Point) -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        actor.last_known_player_pos = Some(last_known);
        actor.chase_leash = 10;
        snapshot_with(actor)
    }

    #[test]
    fn pursues_toward_last_known_when_no_visible_enemy() {
        let snap = hunting_with_last_known(Point::new(8, 5));
        let mut rng = test_rng();
        let outcome = PursueLastKnownPosition
            .evaluate(&snap, &mut rng)
            .expect("should pursue");
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::E }));
    }

    #[test]
    fn pursue_increments_chase_distance() {
        let mut snap = hunting_with_last_known(Point::new(8, 5));
        snap.self_.chase_distance = 3;
        let mut rng = test_rng();
        let outcome = PursueLastKnownPosition.evaluate(&snap, &mut rng).unwrap();
        assert_eq!(outcome.1.set_chase_distance, Some(4));
    }

    #[test]
    fn pursue_passes_when_visible_enemy_exists() {
        // HuntVisibleTarget should handle this case; pursue should pass.
        let mut snap = hunting_with_last_known(Point::new(8, 5));
        snap.visible_enemies = vec![test_player(Point::new(10, 5), 30)];
        let mut rng = test_rng();
        assert!(PursueLastKnownPosition.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn pursue_passes_when_no_last_known_position() {
        let mut snap = hunting_with_last_known(Point::new(8, 5));
        snap.self_.last_known_player_pos = None;
        let mut rng = test_rng();
        assert!(PursueLastKnownPosition.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn pursue_passes_when_at_last_known_position() {
        let mut snap = hunting_with_last_known(Point::new(5, 5)); // == self pos
        snap.self_.pos = Point::new(5, 5);
        let mut rng = test_rng();
        assert!(PursueLastKnownPosition.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn pursue_passes_when_chase_leash_exceeded() {
        let mut snap = hunting_with_last_known(Point::new(8, 5));
        snap.self_.chase_distance = 10; // == chase_leash
        let mut rng = test_rng();
        assert!(PursueLastKnownPosition.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn pursue_passes_when_not_hunting() {
        let mut snap = hunting_with_last_known(Point::new(8, 5));
        snap.self_.mode = AiMode::Idle;
        let mut rng = test_rng();
        assert!(PursueLastKnownPosition.evaluate(&snap, &mut rng).is_none());
    }

    // ----- FreeWander -----

    fn idle_actor() -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Idle;
        snapshot_with(actor)
    }

    #[test]
    fn free_wander_picks_a_walkable_neighbor_when_idle() {
        // ToyPaths::pick_random_nearby returns (from.x + 1, from.y).
        // Step is one east → GridDir::E.
        let snap = idle_actor();
        let mut rng = test_rng();
        let outcome = FreeWander.evaluate(&snap, &mut rng).expect("should wander");
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::E }));
    }

    #[test]
    fn free_wander_passes_when_not_idle() {
        let mut snap = idle_actor();
        snap.self_.mode = AiMode::Hunting;
        let mut rng = test_rng();
        assert!(FreeWander.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn free_wander_passes_when_stationary() {
        let mut snap = idle_actor();
        snap.self_.stationary = true;
        let mut rng = test_rng();
        assert!(FreeWander.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn free_wander_passes_when_pathfinding_blocked() {
        let mut snap = idle_actor();
        snap.paths = Box::new(BlockedPaths);
        let mut rng = test_rng();
        assert!(FreeWander.evaluate(&snap, &mut rng).is_none());
    }
}
