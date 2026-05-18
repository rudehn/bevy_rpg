//! `FleeAtLowHp` — retreat when wounded and an enemy is in sight.
//! `KiteRetreat` — back away from melee range to stay at preferred
//! ranged distance.
//!
//! Both gate on `AiMode::Hunting`. The sticky `Fleeing` mode (Phase 2.5)
//! has its own tactic `FleePanicked` in a separate file; this file
//! covers only the reactive flee/kite behaviors that fire from the
//! Hunting state.

use rand::RngCore;
use roguelike_engine::ai::decisions::{should_flee, should_kite_retreat};

use crate::game::tactics::resolve::{
    AiMode, GridDir, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

/// Flee from the nearest visible enemy when HP is below the actor's
/// `flee_threshold`. Fires only while Hunting — wandering monsters
/// that take damage transition to `Fleeing` via the
/// `damage_triggers_flee` system (Phase 2.5), not via this tactic.
pub struct FleeAtLowHp;

impl Tactic for FleeAtLowHp {
    fn name(&self) -> &'static str {
        "FleeAtLowHp"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Hunting) {
            return None;
        }
        if !should_flee(
            snap.self_.hp_current,
            snap.self_.hp_max,
            snap.self_.flee_threshold,
        ) {
            return None;
        }
        let threat = snap.visible_enemies.first()?;
        let step = snap.paths.next_flee_step(snap.self_.pos, threat.pos)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;
        Some((TacticAction::Move { dir }, TacticStateDelta::default()))
    }
}

/// Back off from melee range. Fires for kiting monsters
/// (`kites == true`) when an enemy gets within `kite_distance`. The
/// monster steps one tile away to maintain ranged spacing.
pub struct KiteRetreat;

impl Tactic for KiteRetreat {
    fn name(&self) -> &'static str {
        "KiteRetreat"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Hunting) {
            return None;
        }
        if !snap.self_.kites || snap.self_.kite_distance == 0 {
            return None;
        }
        let threat = snap.visible_enemies.first()?;
        if !should_kite_retreat(
            snap.self_.pos.x,
            snap.self_.pos.y,
            threat.pos.x,
            threat.pos.y,
            snap.self_.kite_distance,
        ) {
            return None;
        }
        let step = snap.paths.next_flee_step(snap.self_.pos, threat.pos)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;
        Some((TacticAction::Move { dir }, TacticStateDelta::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    // ----- FleeAtLowHp -----

    fn hunting_at_low_hp(threat: Point) -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        actor.hp_current = 2;
        actor.hp_max = 10;
        actor.flee_threshold = 0.3;
        snapshot_with_enemy(actor, threat, 30)
    }

    #[test]
    fn flees_west_when_threat_to_east_and_hp_below_threshold() {
        let snap = hunting_at_low_hp(Point::new(6, 5));
        let mut rng = test_rng();
        let outcome = FleeAtLowHp.evaluate(&snap, &mut rng).expect("should flee");
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::W }));
    }

    #[test]
    fn flee_passes_when_hp_above_threshold() {
        let mut snap = hunting_at_low_hp(Point::new(6, 5));
        snap.self_.hp_current = 9; // 90%
        let mut rng = test_rng();
        assert!(FleeAtLowHp.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn flee_passes_when_flee_threshold_is_zero() {
        let mut snap = hunting_at_low_hp(Point::new(6, 5));
        snap.self_.flee_threshold = 0.0;
        let mut rng = test_rng();
        assert!(FleeAtLowHp.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn flee_passes_when_no_visible_enemy() {
        let mut snap = hunting_at_low_hp(Point::new(6, 5));
        snap.visible_enemies.clear();
        let mut rng = test_rng();
        assert!(FleeAtLowHp.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn flee_passes_when_in_idle_mode() {
        let mut snap = hunting_at_low_hp(Point::new(6, 5));
        snap.self_.mode = AiMode::Idle;
        let mut rng = test_rng();
        assert!(FleeAtLowHp.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn flee_passes_when_pathfinding_blocked() {
        let mut snap = hunting_at_low_hp(Point::new(6, 5));
        snap.paths = Box::new(BlockedPaths);
        let mut rng = test_rng();
        // Cornered monster falls through to lower tactics (likely melee).
        assert!(FleeAtLowHp.evaluate(&snap, &mut rng).is_none());
    }

    // ----- KiteRetreat -----

    fn hunting_kiter_close(threat: Point) -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        actor.kites = true;
        actor.kite_distance = 3;
        snapshot_with_enemy(actor, threat, 30)
    }

    #[test]
    fn kites_back_when_threat_inside_kite_distance() {
        // Threat at distance 1 (adjacent) — well inside kite_distance 3.
        let snap = hunting_kiter_close(Point::new(6, 5));
        let mut rng = test_rng();
        let outcome = KiteRetreat.evaluate(&snap, &mut rng).expect("should kite");
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::W }));
    }

    #[test]
    fn kite_passes_when_threat_beyond_kite_distance() {
        // Threat at distance 5 — outside kite_distance 3.
        let snap = hunting_kiter_close(Point::new(10, 5));
        let mut rng = test_rng();
        assert!(KiteRetreat.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn kite_passes_when_not_a_kiter() {
        let mut snap = hunting_kiter_close(Point::new(6, 5));
        snap.self_.kites = false;
        let mut rng = test_rng();
        assert!(KiteRetreat.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn kite_passes_when_in_idle_mode() {
        let mut snap = hunting_kiter_close(Point::new(6, 5));
        snap.self_.mode = AiMode::Idle;
        let mut rng = test_rng();
        assert!(KiteRetreat.evaluate(&snap, &mut rng).is_none());
    }
}
