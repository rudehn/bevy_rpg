//! Flee-family tactics:
//!
//! - `FleeAtLowHp` — retreat when wounded and an enemy is in sight.
//!   Gates on `AiMode::Hunting`. This is the **entry condition** that
//!   makes a monster *want* to flee. The actual transition into sticky
//!   `Fleeing` mode happens in `src/game/fleeing.rs` via the
//!   `damage_triggers_flee` system.
//! - `KiteRetreat` — back away from melee range to stay at preferred
//!   ranged distance. Gates on `AiMode::Hunting`.
//! - `FleePanicked` — drive movement while in sticky `Fleeing` mode.
//!   Gates on `AiMode::Fleeing { .. }`. Does NOT check HP threshold;
//!   the entry transition (`damage_triggers_flee`) and the exit
//!   transition (`maybe_exit_fleeing`) own the mode lifecycle.

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
/// Drive panicked movement while the actor is in the sticky `Fleeing`
/// mode. Reads `last_known_threat_pos` from the mode variant so the
/// monster keeps fleeing even after losing line-of-sight on the
/// attacker. If an enemy is currently visible, prefer fleeing from
/// the visible enemy over the remembered position.
///
/// This tactic has no HP threshold check — the entry condition was
/// already met when `Fleeing` was inserted by `damage_triggers_flee`,
/// and the exit condition is owned by `maybe_exit_fleeing`. Inside
/// `Fleeing`, a monster always tries to flee.
pub struct FleePanicked;

impl Tactic for FleePanicked {
    fn name(&self) -> &'static str {
        "FleePanicked"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn rand::RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        let AiMode::Fleeing { last_known_threat_pos, .. } = snap.self_.mode else {
            return None;
        };
        // Prefer visible enemy; fall back to remembered position.
        let flee_from = snap
            .visible_enemies
            .first()
            .map(|e| e.pos)
            .or(last_known_threat_pos)?;
        let step = snap.paths.next_flee_step(snap.self_.pos, flee_from)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;
        Some((TacticAction::Move { dir }, TacticStateDelta::default()))
    }
}

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

    // ----- FleePanicked -----

    fn fleeing_actor() -> super::TurnSnapshot {
        use super::TurnSnapshot;
        use crate::game::tactics::resolve::test_support::{
            snapshot_with, test_actor,
        };
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Fleeing {
            since_turn: 100,
            last_known_threat_pos: Some(Point::new(7, 5)), // threat to the east
        };
        let snap: TurnSnapshot = snapshot_with(actor);
        snap
    }

    #[test]
    fn flee_panicked_flees_from_last_known_threat_when_nothing_visible() {
        let snap = fleeing_actor();
        let mut rng = test_rng();
        let outcome = FleePanicked.evaluate(&snap, &mut rng).expect("should panic-flee");
        // Threat was east, monster flees west.
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::W }));
    }

    #[test]
    fn flee_panicked_prefers_visible_enemy_over_last_known_position() {
        let mut snap = fleeing_actor();
        // Visible enemy at the SOUTH, last_known_threat_pos to the EAST.
        // FleePanicked should flee NORTH (away from visible), not WEST.
        snap.visible_enemies = vec![crate::game::tactics::resolve::test_support::test_player(
            Point::new(5, 8),
            30,
        )];
        // Override is_adjacent/chebyshev for the moved test_player:
        snap.visible_enemies[0].chebyshev = 3;
        snap.visible_enemies[0].is_adjacent = false;
        let mut rng = test_rng();
        let outcome = FleePanicked.evaluate(&snap, &mut rng).unwrap();
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::N }));
    }

    #[test]
    fn flee_panicked_passes_when_not_in_fleeing_mode() {
        use crate::game::tactics::resolve::test_support::{snapshot_with, test_actor};
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        let snap = snapshot_with(actor);
        let mut rng = test_rng();
        assert!(FleePanicked.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn flee_panicked_passes_when_no_threat_info_available() {
        use crate::game::tactics::resolve::test_support::{snapshot_with, test_actor};
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Fleeing {
            since_turn: 100,
            last_known_threat_pos: None, // no remembered threat
        };
        let snap = snapshot_with(actor);
        // No visible enemies either — nothing to flee from.
        let mut rng = test_rng();
        assert!(FleePanicked.evaluate(&snap, &mut rng).is_none());
    }
}
