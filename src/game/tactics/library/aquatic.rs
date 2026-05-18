//! `SubmergeOrSurface` — aquatic movement-mode toggle. Aquatic and
//! amphibious monsters submerge into liquid tiles (gaining cover and
//! the "Submerged" component) and surface when they leave liquid.
//!
//! Lands as a high-priority tactic in aquatic monster lists so they
//! manage their submerge state before evaluating combat tactics.
//! When the toggle isn't needed (already in the correct state), the
//! tactic returns `None` and the chain proceeds normally.

use rand::RngCore;

use crate::game::tactics::resolve::{
    MovementKind, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

pub struct SubmergeOrSurface;

impl Tactic for SubmergeOrSurface {
    fn name(&self) -> &'static str {
        "SubmergeOrSurface"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        // Only aquatic / amphibious actors manage submerge state.
        if !matches!(
            snap.self_.movement,
            MovementKind::Aquatic | MovementKind::Amphibious
        ) {
            return None;
        }
        // On liquid + not submerged → submerge.
        if snap.self_.on_liquid && !snap.self_.is_submerged {
            return Some((
                TacticAction::SetSubmerged(true),
                TacticStateDelta::default(),
            ));
        }
        // Off liquid + submerged → surface.
        if !snap.self_.on_liquid && snap.self_.is_submerged {
            return Some((
                TacticAction::SetSubmerged(false),
                TacticStateDelta::default(),
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    fn aquatic_actor() -> TurnSnapshot {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.movement = MovementKind::Aquatic;
        snapshot_with(actor)
    }

    #[test]
    fn submerges_when_on_liquid_and_not_submerged() {
        let mut snap = aquatic_actor();
        snap.self_.on_liquid = true;
        snap.self_.is_submerged = false;
        let mut rng = test_rng();
        let outcome = SubmergeOrSurface.evaluate(&snap, &mut rng).unwrap();
        assert!(matches!(outcome.0, TacticAction::SetSubmerged(true)));
    }

    #[test]
    fn surfaces_when_off_liquid_and_submerged() {
        let mut snap = aquatic_actor();
        snap.self_.on_liquid = false;
        snap.self_.is_submerged = true;
        let mut rng = test_rng();
        let outcome = SubmergeOrSurface.evaluate(&snap, &mut rng).unwrap();
        assert!(matches!(outcome.0, TacticAction::SetSubmerged(false)));
    }

    #[test]
    fn passes_when_already_submerged_on_liquid() {
        let mut snap = aquatic_actor();
        snap.self_.on_liquid = true;
        snap.self_.is_submerged = true;
        let mut rng = test_rng();
        assert!(SubmergeOrSurface.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn passes_when_already_surfaced_off_liquid() {
        let mut snap = aquatic_actor();
        snap.self_.on_liquid = false;
        snap.self_.is_submerged = false;
        let mut rng = test_rng();
        assert!(SubmergeOrSurface.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn passes_for_land_actor_regardless_of_liquid_state() {
        let mut actor = test_actor(1, Point::new(5, 5));
        actor.movement = MovementKind::Land;
        actor.on_liquid = true; // standing in puddle but can't swim
        let snap = snapshot_with(actor);
        let mut rng = test_rng();
        assert!(SubmergeOrSurface.evaluate(&snap, &mut rng).is_none());
    }
}
