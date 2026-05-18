//! `WaitTactic` — unconditional fallback that always emits a Wait
//! intent. Every monster's tactic list ends with this entry so the
//! dispatcher never falls through to `FallbackWait`.

use rand::RngCore;

use crate::game::tactics::resolve::{
    Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

pub struct WaitTactic;

impl Tactic for WaitTactic {
    fn name(&self) -> &'static str {
        "Wait"
    }

    fn evaluate(
        &self,
        _snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        Some((TacticAction::Wait, TacticStateDelta::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    #[test]
    fn always_fires_with_wait_action() {
        let snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        let mut rng = test_rng();
        let outcome = WaitTactic.evaluate(&snap, &mut rng);
        let (action, _delta) = outcome.expect("WaitTactic must always fire");
        assert!(matches!(action, TacticAction::Wait));
    }

    #[test]
    fn fires_even_with_visible_enemies() {
        // Wait is the unconditional fallback — it doesn't care about world state.
        let actor = test_actor(1, Point::new(5, 5));
        let snap = snapshot_with_enemy(actor, Point::new(6, 5), 30);
        let mut rng = test_rng();
        let outcome = WaitTactic.evaluate(&snap, &mut rng);
        assert!(outcome.is_some());
    }
}
