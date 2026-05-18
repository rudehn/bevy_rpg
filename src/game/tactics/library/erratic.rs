//! `ErraticMove` — random-direction "drunken" movement that overrides
//! the normal pathfinding chain on a per-turn dice roll.
//!
//! Used by bats, bloats, eels, drunken NPCs — anything that should
//! move unpredictably some fraction of the time. The `erratic_chance`
//! knob lives on `MonsterAI` and is rolled fresh each turn; on a
//! pass, the tactic falls through to whatever movement tactic comes
//! next in the list.

use rand::RngCore;

use crate::game::tactics::resolve::{
    AiMode, GridDir, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

pub struct ErraticMove;

impl Tactic for ErraticMove {
    fn name(&self) -> &'static str {
        "ErraticMove"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        // Sleep + Fleeing have their own movement; don't override.
        if matches!(snap.self_.mode, AiMode::Asleep | AiMode::Fleeing { .. }) {
            return None;
        }
        if snap.self_.stationary {
            return None;
        }
        if snap.self_.erratic_chance <= 0.0 {
            return None;
        }
        // Fresh roll each turn — `next_u32() as f32 / u32::MAX` in [0,1).
        let roll = (rng.next_u32() as f64 / u32::MAX as f64) as f32;
        if roll >= snap.self_.erratic_chance {
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

    /// Deterministic RNG that always yields the value the closure provides.
    struct FixedRng(u32);
    impl RngCore for FixedRng {
        fn next_u32(&mut self) -> u32 {
            self.0
        }
        fn next_u64(&mut self) -> u64 {
            self.0 as u64
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for byte in dest {
                *byte = self.0 as u8;
            }
        }
    }

    fn erratic_idle_actor() -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Idle;
        actor.erratic_chance = 0.5;
        snapshot_with(actor)
    }

    #[test]
    fn erratic_fires_when_roll_below_chance() {
        // chance 0.5, roll ~ 0.25 (u32::MAX / 4)
        let mut rng = FixedRng(u32::MAX / 4);
        let snap = erratic_idle_actor();
        let outcome = ErraticMove.evaluate(&snap, &mut rng).expect("should fire");
        assert!(matches!(outcome.0, TacticAction::Move { .. }));
    }

    #[test]
    fn erratic_passes_when_roll_above_chance() {
        // chance 0.5, roll ~ 0.75 (u32::MAX * 3/4)
        let mut rng = FixedRng((u32::MAX as u64 * 3 / 4) as u32);
        let snap = erratic_idle_actor();
        assert!(ErraticMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn erratic_passes_when_chance_is_zero() {
        let mut snap = erratic_idle_actor();
        snap.self_.erratic_chance = 0.0;
        let mut rng = FixedRng(0);
        assert!(ErraticMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn erratic_passes_when_stationary() {
        let mut snap = erratic_idle_actor();
        snap.self_.stationary = true;
        let mut rng = FixedRng(0);
        assert!(ErraticMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn erratic_passes_when_asleep() {
        let mut snap = erratic_idle_actor();
        snap.self_.mode = AiMode::Asleep;
        let mut rng = FixedRng(0);
        assert!(ErraticMove.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn erratic_passes_when_fleeing() {
        let mut snap = erratic_idle_actor();
        snap.self_.mode = AiMode::Fleeing {
            since_turn: 0,
            last_known_threat_pos: Some(Point::new(7, 5)),
        };
        let mut rng = FixedRng(0);
        assert!(ErraticMove.evaluate(&snap, &mut rng).is_none());
    }
}
