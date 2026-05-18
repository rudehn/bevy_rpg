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
    /// Hidden if the map is empty. Caller decides which records count
    /// as "hostile" (faction filter applied externally).
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

/// Decay timers and demote expired Searching/Suspicious records to
/// Hidden. Called by `awareness_tick_system` per perceiver-Awareness;
/// extracted for unit testability without a Bevy App.
pub fn tick_awareness(awareness: &mut Awareness, now: u32) {
    for record in awareness.records.values_mut() {
        let expired = match record.state {
            AwarenessState::Searching { giveup_at_turn, .. } => now > giveup_at_turn,
            AwarenessState::Suspicious { decay_at_turn, .. } => now > decay_at_turn,
            _ => false,
        };
        if expired {
            record.state = AwarenessState::Hidden;
            record.last_update_turn = now;
        }
    }
    // GC: drop Hidden records older than 200 turns to keep the map small.
    awareness.records.retain(|_, r| {
        !matches!(r.state, AwarenessState::Hidden)
            || now.saturating_sub(r.last_update_turn) <= 200
    });
}

/// Bevy system: runs once per game turn, ticks every Awareness component.
/// Reads `now` from the engine's `TurnManager.current_time`.
pub fn awareness_tick_system(
    turn_manager: Res<crate::turn::TurnManager>,
    mut perceivers: Query<&mut Awareness>,
) {
    let now = turn_manager.current_time;
    for mut a in &mut perceivers {
        tick_awareness(a.as_mut(), now);
    }
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

    #[test]
    fn searching_timer_expires_to_hidden() {
        let mut a = Awareness::default();
        let target = test_entity(99);
        a.set(
            target,
            AwarenessState::Searching {
                last_known_pos: Point::new(5, 5),
                giveup_at_turn: 10,
            },
            0,
        );
        tick_awareness(&mut a, 11);
        assert_eq!(a.get(target).unwrap().state, AwarenessState::Hidden);
    }

    #[test]
    fn suspicious_timer_expires_to_hidden() {
        let mut a = Awareness::default();
        let target = test_entity(99);
        a.set(
            target,
            AwarenessState::Suspicious {
                suspect_pos: Point::new(2, 2),
                decay_at_turn: 5,
            },
            0,
        );
        tick_awareness(&mut a, 6);
        assert_eq!(a.get(target).unwrap().state, AwarenessState::Hidden);
    }

    #[test]
    fn searching_timer_alive_holds_state() {
        let mut a = Awareness::default();
        let target = test_entity(99);
        a.set(
            target,
            AwarenessState::Searching {
                last_known_pos: Point::new(5, 5),
                giveup_at_turn: 10,
            },
            0,
        );
        tick_awareness(&mut a, 5);
        assert!(matches!(
            a.get(target).unwrap().state,
            AwarenessState::Searching { .. }
        ));
    }

    #[test]
    fn aware_state_is_untouched_by_tick() {
        let mut a = Awareness::default();
        let target = test_entity(99);
        a.set(target, AwarenessState::Aware, 0);
        tick_awareness(&mut a, 100);
        assert_eq!(a.get(target).unwrap().state, AwarenessState::Aware);
    }

    #[test]
    fn hidden_records_get_gc_after_200_turns() {
        let mut a = Awareness::default();
        let target = test_entity(99);
        a.set(target, AwarenessState::Hidden, 0);
        // Just inside the GC window — still present.
        tick_awareness(&mut a, 200);
        assert!(a.records.contains_key(&target));
        // Past the window — purged.
        tick_awareness(&mut a, 201);
        assert!(!a.records.contains_key(&target));
    }
}
