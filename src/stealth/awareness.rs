//! Per-perceiver, per-target awareness model. See bevy_rpg's
//! `docs/superpowers/specs/2026-05-16-stealth-system-design.md`.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwarenessState {
    Hidden,
    Suspicious { suspect_pos: Point, decay_at_turn: u32 },
    Searching  { last_known_pos: Point, giveup_at_turn: u32 },
    Aware,
}

impl AwarenessState {
    /// Strength ordering for `Awareness::highest`. Hidden = 0, Aware = 3.
    pub fn rank(&self) -> u8 {
        match self {
            AwarenessState::Hidden => 0,
            AwarenessState::Suspicious { .. } => 1,
            AwarenessState::Searching { .. } => 2,
            AwarenessState::Aware => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AwarenessRecord {
    pub state: AwarenessState,
    /// Game-turn the record was last touched. Used by the tick system
    /// to GC stale Hidden records and to drive timer expirations.
    pub last_update_turn: u32,
    /// Last known position of the target at the time it was Aware.
    /// Set when transitioning Aware → Searching.
    pub last_seen_pos: Option<Point>,
}

#[derive(Component, Default, Debug)]
pub struct Awareness {
    pub records: HashMap<Entity, AwarenessRecord>,
}

impl Awareness {
    pub fn get(&self, target: Entity) -> Option<&AwarenessRecord> {
        self.records.get(&target)
    }

    pub fn set(&mut self, target: Entity, state: AwarenessState, now: u32) {
        let entry = self.records.entry(target).or_insert(AwarenessRecord {
            state: AwarenessState::Hidden,
            last_update_turn: now,
            last_seen_pos: None,
        });
        entry.state = state;
        entry.last_update_turn = now;
    }

    /// Returns the highest-ranked state across all records; defaults to
    /// Hidden if the map is empty.
    pub fn highest(&self) -> AwarenessState {
        self.records
            .values()
            .map(|r| r.state)
            .max_by_key(|s| s.rank())
            .unwrap_or(AwarenessState::Hidden)
    }
}

#[derive(Message, Debug, Clone)]
pub struct AwarenessAlertEvent {
    pub seeker: Entity,
    pub target: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entity(n: u32) -> Entity {
        Entity::from_raw_u32(n).expect("valid test entity index")
    }

    #[test]
    fn rank_ordering_is_total() {
        assert!(
            AwarenessState::Hidden.rank()
                < AwarenessState::Suspicious {
                    suspect_pos: Point::new(0, 0),
                    decay_at_turn: 0
                }
                .rank()
        );
        assert!(
            AwarenessState::Suspicious {
                suspect_pos: Point::new(0, 0),
                decay_at_turn: 0
            }
            .rank()
                < AwarenessState::Searching {
                    last_known_pos: Point::new(0, 0),
                    giveup_at_turn: 0
                }
                .rank()
        );
        assert!(
            AwarenessState::Searching {
                last_known_pos: Point::new(0, 0),
                giveup_at_turn: 0
            }
            .rank()
                < AwarenessState::Aware.rank()
        );
    }

    #[test]
    fn empty_awareness_returns_hidden() {
        let a = Awareness::default();
        assert_eq!(a.highest(), AwarenessState::Hidden);
    }

    #[test]
    fn highest_returns_strongest_state() {
        let mut a = Awareness::default();
        let e1 = test_entity(1);
        let e2 = test_entity(2);
        a.set(e1, AwarenessState::Hidden, 0);
        a.set(
            e2,
            AwarenessState::Searching {
                last_known_pos: Point::new(3, 4),
                giveup_at_turn: 10,
            },
            0,
        );
        assert!(matches!(a.highest(), AwarenessState::Searching { .. }));
    }
}
