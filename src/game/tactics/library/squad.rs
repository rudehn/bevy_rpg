//! `SquadLeash` — drag squad followers back toward their leader when
//! they stray too far. Mirrors the legacy FSM's `resolve_squad_leash`
//! helper, but as a first-class priority-ordered tactic that runs
//! before pursuit/patrol movement.
//!
//! The snapshot exposes `squad_leader_pos` (set by the adapter only
//! for non-leader squad members whose leader is alive and within
//! query distance); when the actor is further than `SQUAD_LEASH_RANGE`
//! tiles, this tactic emits a move toward the leader, overriding the
//! normal hunt/idle behavior chain.

use rand::RngCore;

use crate::game::tactics::resolve::{
    GridDir, Tactic, TacticAction, TacticStateDelta, TurnSnapshot,
};

/// Maximum Chebyshev distance a follower can drift from its leader
/// before this tactic fires. Matches the legacy FSM constant.
pub const SQUAD_LEASH_RANGE: i32 = 4;

pub struct SquadLeash;

impl Tactic for SquadLeash {
    fn name(&self) -> &'static str {
        "SquadLeash"
    }

    fn evaluate(
        &self,
        snap: &TurnSnapshot,
        _rng: &mut dyn RngCore,
    ) -> Option<(TacticAction, TacticStateDelta)> {
        let leader = snap.self_.squad_leader_pos?;
        let dist = (leader.x - snap.self_.pos.x)
            .abs()
            .max((leader.y - snap.self_.pos.y).abs());
        if dist <= SQUAD_LEASH_RANGE {
            return None;
        }
        if snap.self_.stationary {
            return None;
        }
        let step = snap.paths.next_step_toward(snap.self_.pos, leader)?;
        let dir = GridDir::from_step(snap.self_.pos, step)?;
        Some((TacticAction::Move { dir }, TacticStateDelta::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tactics::resolve::test_support::*;
    use bracket_lib::prelude::Point;

    #[test]
    fn leash_passes_when_no_leader() {
        let snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        let mut rng = test_rng();
        assert!(SquadLeash.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn leash_passes_when_within_range() {
        let mut snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        snap.self_.squad_leader_pos = Some(Point::new(7, 5)); // Chebyshev = 2 (<= 4)
        let mut rng = test_rng();
        assert!(SquadLeash.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn leash_fires_toward_distant_leader() {
        let mut snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        snap.self_.squad_leader_pos = Some(Point::new(15, 5)); // Chebyshev = 10
        let mut rng = test_rng();
        let outcome = SquadLeash.evaluate(&snap, &mut rng).expect("should leash");
        // ToyPaths moves one step east toward leader at (15, 5).
        assert!(matches!(outcome.0, TacticAction::Move { dir: GridDir::E }));
    }

    #[test]
    fn leash_passes_when_stationary() {
        let mut snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        snap.self_.squad_leader_pos = Some(Point::new(15, 5));
        snap.self_.stationary = true;
        let mut rng = test_rng();
        assert!(SquadLeash.evaluate(&snap, &mut rng).is_none());
    }

    #[test]
    fn leash_passes_when_path_blocked() {
        let mut snap = snapshot_with(test_actor(1, Point::new(5, 5)));
        snap.self_.squad_leader_pos = Some(Point::new(15, 5));
        snap.paths = Box::new(BlockedPaths);
        let mut rng = test_rng();
        assert!(SquadLeash.evaluate(&snap, &mut rng).is_none());
    }
}
