//! `MeleeAdjacent` — bite/punch the nearest visible enemy when in
//! Chebyshev distance 1. Fires in any active mode (Idle/Hunting/Fleeing)
//! because a monster adjacent to a hostile target generally swings
//! regardless of whether it was wandering, hunting, or panicking.
//! Asleep monsters never act (the resolver short-circuits stunned/
//! entangled cases before reaching tactics; Asleep is gated on having
//! no visible enemies via the perception/awareness layer).

use rand::RngCore;

use crate::game::tactics::resolve::{
    AiMode, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

pub struct MeleeAdjacent;

impl Tactic for MeleeAdjacent {
    fn name(&self) -> &'static str {
        "MeleeAdjacent"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        // Asleep monsters don't attack even adjacent targets.
        if matches!(snap.self_.mode, AiMode::Asleep) {
            return None;
        }
        let target = snap.visible_enemies.iter().find(|e| e.is_adjacent)?;
        Some((
            TacticAction::Melee { target: target.id },
            TacticStateDelta::default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    #[test]
    fn melees_adjacent_player_in_hunting_mode() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        let snap = snapshot_with_enemy(actor, Point::new(6, 5), 30);
        let mut rng = test_rng();
        let outcome = MeleeAdjacent.evaluate(&snap, &mut rng);
        let (action, _delta) = outcome.expect("should melee adjacent enemy");
        assert!(matches!(action, TacticAction::Melee { .. }));
    }

    #[test]
    fn melees_adjacent_enemy_even_in_idle_mode() {
        // A wandering monster bitten by an adjacent enemy swings back.
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Idle;
        let snap = snapshot_with_enemy(actor, Point::new(6, 5), 30);
        let mut rng = test_rng();
        assert!(MeleeAdjacent.evaluate(&snap, &mut rng).is_some());
    }

    #[test]
    fn passes_when_no_adjacent_enemy_even_if_one_is_visible() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        let snap = snapshot_with_enemy(actor, Point::new(8, 5), 30); // Chebyshev = 3
        let mut rng = test_rng();
        assert!(MeleeAdjacent.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn passes_when_asleep_even_with_adjacent_enemy() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Asleep;
        let snap = snapshot_with_enemy(actor, Point::new(6, 5), 30);
        let mut rng = test_rng();
        assert!(MeleeAdjacent.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn passes_when_no_visible_enemies() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.mode = AiMode::Hunting;
        let snap = snapshot_with(actor);
        let mut rng = test_rng();
        assert!(MeleeAdjacent.evaluate(&snap, &mut rng).is_none());
    }
}
