//! Ranged combat tactics.
//!
//! - `RangedAttack` — fire a ranged attack at the nearest visible
//!   enemy, gated on Hunting + ranged-capable + target in range.
//! - `UseAbility` — broad-gated tactic that signals the adapter to
//!   call `try_use_ability_world`. The adapter performs the actual
//!   ability selection (per-ability target + cooldown checks); this
//!   tactic just decides "yes, try". Gates on Hunting + visible
//!   enemy + at least one ready ability.

use rand::RngCore;

use crate::game::tactics::resolve::{
    AbilitySlot, AiMode, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

/// Fire a ranged attack at the nearest visible enemy. Skips adjacent
/// targets (those belong to `MeleeAdjacent`) and out-of-range targets.
pub struct RangedAttack;

impl Tactic for RangedAttack {
    fn name(&self) -> &'static str {
        "RangedAttack"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Hunting) {
            return None;
        }
        let range = snap.self_.ranged_range? as i32;
        if range == 0 {
            return None;
        }
        let target = snap.visible_enemies.first()?;
        if target.is_adjacent {
            return None;
        }
        if target.chebyshev > range {
            return None;
        }
        Some((
            TacticAction::Ranged { target: target.id },
            TacticStateDelta::default(),
        ))
    }
}

/// Defer to `try_use_ability_world` for per-ability dispatch. The
/// tactic's predicate is intentionally broad — Hunting mode + visible
/// enemy + any ready ability — and the adapter performs the precise
/// per-ability target/cooldown checks. When no ability fits, the
/// adapter writes a `WaitIntent` (turn consumed, lower tactics
/// skipped). Same wasted-turn cost as the legacy FSM dispatcher.
///
/// Slot/target in the returned `UseAbility` action are currently
/// ignored by the adapter — `try_use_ability_world` makes its own
/// selection. Both will become meaningful when ability targeting
/// moves into the tactic layer in a later phase.
pub struct UseAbility;

impl Tactic for UseAbility {
    fn name(&self) -> &'static str {
        "UseAbility"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        if !matches!(snap.self_.mode, AiMode::Hunting) {
            return None;
        }
        if snap.visible_enemies.is_empty() {
            return None;
        }
        if !snap.self_.has_useable_ability {
            return None;
        }
        Some((
            TacticAction::UseAbility {
                slot: AbilitySlot(0),
                target: None,
            },
            TacticStateDelta::default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    // ----- RangedAttack -----

    fn archer_with_target(target: Point) -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        actor.ranged_range = Some(6);
        snapshot_with_enemy(actor, target, 30)
    }

    #[test]
    fn ranged_fires_at_target_in_range() {
        let snap = archer_with_target(Point::new(8, 5)); // chebyshev = 3
        let mut rng = test_rng();
        let outcome = RangedAttack.evaluate(&snap, &mut rng).expect("should fire");
        assert!(matches!(outcome.0, TacticAction::Ranged { .. }));
    }

    #[test]
    fn ranged_passes_when_target_adjacent() {
        let snap = archer_with_target(Point::new(6, 5)); // chebyshev = 1
        let mut rng = test_rng();
        assert!(RangedAttack.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn ranged_passes_when_target_beyond_range() {
        let snap = archer_with_target(Point::new(15, 5)); // chebyshev = 10, range 6
        let mut rng = test_rng();
        assert!(RangedAttack.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn ranged_passes_when_no_ranged_capability() {
        let mut snap = archer_with_target(Point::new(8, 5));
        snap.self_.ranged_range = None;
        let mut rng = test_rng();
        assert!(RangedAttack.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn ranged_passes_when_not_hunting() {
        let mut snap = archer_with_target(Point::new(8, 5));
        snap.self_.mode = AiMode::Idle;
        let mut rng = test_rng();
        assert!(RangedAttack.evaluate(&snap, &mut rng).is_none());
    }

    // ----- UseAbility -----

    fn caster_hunting_with_ready_ability() -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        actor.has_useable_ability = true;
        snapshot_with_enemy(actor, Point::new(7, 5), 30)
    }

    #[test]
    fn use_ability_fires_when_hunting_with_visible_enemy_and_ready_ability() {
        let snap = caster_hunting_with_ready_ability();
        let mut rng = test_rng();
        let outcome = UseAbility.evaluate(&snap, &mut rng).expect("should fire");
        assert!(matches!(outcome.0, TacticAction::UseAbility { .. }));
    }

    #[test]
    fn use_ability_passes_when_not_hunting() {
        let mut snap = caster_hunting_with_ready_ability();
        snap.self_.mode = AiMode::Idle;
        let mut rng = test_rng();
        assert!(UseAbility.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn use_ability_passes_when_no_visible_enemy() {
        let mut snap = caster_hunting_with_ready_ability();
        snap.visible_enemies.clear();
        let mut rng = test_rng();
        assert!(UseAbility.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn use_ability_passes_when_no_ready_ability() {
        let mut snap = caster_hunting_with_ready_ability();
        snap.self_.has_useable_ability = false;
        let mut rng = test_rng();
        assert!(UseAbility.evaluate(&snap, &mut rng).is_none());
    }
}
