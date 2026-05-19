//! Monster AI state component.
//!
//! `MonsterAI` is the component attached to every NPC monster that the
//! engine's state-machine-style AI tracks. It owns:
//!
//! - The current high-level mode (`Asleep`, `Hunting`, `Idle`)
//! - Tunable behavior knobs (flee HP threshold, kite distance, chase
//!   leash, erratic movement chance, stationary flag)
//! - Runtime tracking state (last-known player position, chase distance
//!   since last FOV sighting, spawn position for patrol snapback)
//!
//! The engine ships the data structure and a handful of pure state
//! transitions (`alert_to_position`, `scatter`, `is_asleep`, ...).
//! The game crate owns the actual per-turn execution loop — it needs
//! full access to `Map`, `StatusEffects`, ability dispatch, and other
//! game-specific systems, so the `execute` logic stays as a free
//! function in the game crate. Fields that the game's execute loop
//! needs to read/write are `pub` on purpose.

use bevy::ecs::component::Component;
use bracket_lib::prelude::Point;

/// Radius (in tiles) around a sentry's home position where guarding
/// monsters jitter while waiting for intruders. Used by both the
/// engine's patrol helper code and game-side idle-movement logic.
pub const GUARD_PATROL_RADIUS: i32 = 3;

/// High-level AI mode. The game's execute loop reads this to decide
/// whether to pathfind toward the player, wander, or idle.
#[non_exhaustive]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum MonsterAIMode {
    /// Has not yet noticed the player. Skips turns unless alerted by
    /// another squad member or woken by damage.
    #[default]
    Asleep,
    /// Actively pursuing a known player position.
    Hunting,
    /// Has lost the player; wanders or follows a patrol route until
    /// re-alerted.
    Idle,
}

/// Per-monster AI state: mode, tunable behavior knobs, and runtime
/// tracking state.
///
/// Fields marked `pub` are intentionally exposed so games can
/// implement their own turn-execution loops. The blessed methods
/// ([`alert_to_position`], [`scatter`], etc.) cover common state
/// transitions and should be preferred where they apply.
///
/// [`alert_to_position`]: MonsterAI::alert_to_position
/// [`scatter`]: MonsterAI::scatter
#[derive(Default, Component)]
pub struct MonsterAI {
    /// Current high-level mode.
    pub mode: MonsterAIMode,
    /// The last tile the monster saw the player on. Used as a
    /// pathfinding target while chasing after the player leaves sight.
    pub last_known_player_position: Option<Point>,

    // --- Behavior knobs (copied from game asset data at spawn time) ---
    /// Flee when HP drops below this fraction of max. `0.0` disables
    /// fleeing entirely.
    pub flee_at_hp_percent: f32,
    /// Probability in `[0.0, 1.0]` of making a random move instead of
    /// the optimal pathfind result on any given turn.
    pub erratic_chance: f32,
    /// Maximum number of turns to chase without seeing the player
    /// before giving up and returning to idle. `0` disables the leash.
    pub chase_leash: u32,
    /// True for ranged monsters that retreat when the player gets
    /// closer than `kite_distance`.
    pub kites: bool,
    /// Kite retreat threshold in tiles (squared distance comparison).
    pub kite_distance: u32,

    /// If true, the monster never moves — it only uses abilities
    /// and ranged attacks. Used for turret-like enemies.
    pub stationary: bool,

    // --- Runtime chase tracking ---
    /// Number of turns since the monster last saw the player.
    /// Compared against `chase_leash` to decide when to give up.
    pub chase_distance: u32,
    /// Starting position recorded at spawn time. Used by patrol
    /// snapback logic to pick the nearest waypoint after a hunt ends.
    pub spawn_position: Option<Point>,
}

impl MonsterAI {
    /// Wake this monster and point it at a target position.
    ///
    /// Transitions from `Asleep` or `Idle` → `Hunting`. Has no effect
    /// if the monster is already hunting. Used by the squad alert
    /// system to wake every member of a squad when any member spots
    /// the player or takes damage.
    pub fn alert_to_position(&mut self, target: Point) {
        if self.mode == MonsterAIMode::Asleep || self.mode == MonsterAIMode::Idle {
            self.mode = MonsterAIMode::Hunting;
            self.last_known_player_position = Some(target);
        }
    }

    /// Force this monster into `Idle` mode, clearing its target.
    ///
    /// Used for squad scatter on leader death: surviving members lose
    /// their focus and wander until they spot the player themselves.
    pub fn scatter(&mut self) {
        self.mode = MonsterAIMode::Idle;
        self.last_known_player_position = None;
    }

    /// Returns `true` if this monster is not asleep (hunting or idle).
    pub fn is_alert(&self) -> bool {
        self.mode != MonsterAIMode::Asleep
    }

    /// Returns `true` if this monster is still asleep (hasn't seen the
    /// player yet).
    pub fn is_asleep(&self) -> bool {
        self.mode == MonsterAIMode::Asleep
    }

    /// Human-readable label for the current mode. Used by tooltip /
    /// nearby UI to show "Sleeping", "Hunting", or "Wandering".
    pub fn display_state(&self) -> &'static str {
        match self.mode {
            MonsterAIMode::Asleep => "Sleeping",
            MonsterAIMode::Hunting => "Hunting",
            MonsterAIMode::Idle => "Wandering",
            // `#[non_exhaustive]`: fall back to a neutral label if a
            // future engine variant doesn't have one yet.
            _ => "Active",
        }
    }

    /// Awareness-driven mode update. The highest awareness state across
    /// all records determines the mode:
    ///
    /// - `Aware` → `Hunting` — there's current line of sight to a
    ///   target (or the perceiver was just attacked).
    /// - `Searching` → `Idle` — the perceiver knows something is out
    ///   there but doesn't have current LOS. An `Asleep` monster on
    ///   a successful perception roll wakes up to `Idle`; a `Hunting`
    ///   monster that lost LOS stays `Hunting` until the
    ///   `Searching` timer expires back to `Hidden`.
    /// - `Hidden` → preserve current mode (so `Asleep` stays asleep
    ///   until a perception roll flips a record).
    ///
    /// Awareness records' `last_known_pos` is not copied into
    /// `last_known_player_position` here — the caller is responsible
    /// for that wiring at the perception-tick site, since awareness
    /// state is target-keyed and only the caller knows which record
    /// describes the player.
    pub fn update_mode_from_awareness(&mut self, awareness: &crate::stealth::Awareness) {
        use crate::stealth::AwarenessState;
        match awareness.highest() {
            AwarenessState::Aware => {
                self.mode = MonsterAIMode::Hunting;
            }
            AwarenessState::Searching { .. } => {
                if self.mode == MonsterAIMode::Asleep {
                    self.mode = MonsterAIMode::Idle;
                }
                // Hunting stays Hunting until Searching expires to Hidden.
            }
            AwarenessState::Hidden => {
                // Preserve current default — Asleep keeps sleeping.
            }
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[test]
    fn default_is_asleep() {
        let ai = MonsterAI::default();
        assert!(ai.is_asleep());
        assert!(!ai.is_alert());
        assert_eq!(ai.mode, MonsterAIMode::Asleep);
    }

    #[test]
    fn alert_wakes_from_asleep() {
        let mut ai = MonsterAI::default();
        ai.alert_to_position(Point::new(10, 10));
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
        assert_eq!(ai.last_known_player_position, Some(Point::new(10, 10)));
    }

    #[test]
    fn alert_wakes_from_idle() {
        let mut ai = MonsterAI::default();
        ai.mode = MonsterAIMode::Idle;
        ai.alert_to_position(Point::new(5, 5));
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
    }

    #[test]
    fn alert_does_not_reset_hunting_target() {
        // If the monster is already hunting, a new alert shouldn't
        // overwrite its existing last-known position — the execute
        // loop is tracking the player directly and the alert is stale.
        let mut ai = MonsterAI::default();
        ai.mode = MonsterAIMode::Hunting;
        ai.last_known_player_position = Some(Point::new(1, 1));
        ai.alert_to_position(Point::new(99, 99));
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
        assert_eq!(ai.last_known_player_position, Some(Point::new(1, 1)));
    }

    #[test]
    fn scatter_clears_target_and_idles() {
        let mut ai = MonsterAI::default();
        ai.mode = MonsterAIMode::Hunting;
        ai.last_known_player_position = Some(Point::new(3, 3));
        ai.scatter();
        assert_eq!(ai.mode, MonsterAIMode::Idle);
        assert_eq!(ai.last_known_player_position, None);
    }

    #[test]
    fn display_state_covers_blessed_modes() {
        let mut ai = MonsterAI::default();
        assert_eq!(ai.display_state(), "Sleeping");
        ai.mode = MonsterAIMode::Hunting;
        assert_eq!(ai.display_state(), "Hunting");
        ai.mode = MonsterAIMode::Idle;
        assert_eq!(ai.display_state(), "Wandering");
    }

    #[test]
    fn awareness_aware_drives_hunting() {
        use crate::stealth::{Awareness, AwarenessState};
        let mut ai = MonsterAI::default();
        let mut aware = Awareness::default();
        let target = Entity::from_raw_u32(42).expect("valid test entity");
        aware.set(target, AwarenessState::Aware, 0);
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
    }

    #[test]
    fn awareness_searching_wakes_asleep_to_idle() {
        use crate::stealth::{Awareness, AwarenessState};
        let mut ai = MonsterAI::default();
        ai.mode = MonsterAIMode::Asleep;
        let mut aware = Awareness::default();
        let target = Entity::from_raw_u32(42).expect("valid test entity");
        aware.set(
            target,
            AwarenessState::Searching {
                last_known_pos: Point::new(3, 3),
                giveup_at_turn: 100,
            },
            0,
        );
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Idle);
    }

    #[test]
    fn awareness_searching_preserves_hunting() {
        // A monster that was Hunting and then lost LOS sits in
        // Searching — should stay Hunting until the Searching timer
        // expires to Hidden, not be downgraded to Idle.
        use crate::stealth::{Awareness, AwarenessState};
        let mut ai = MonsterAI::default();
        ai.mode = MonsterAIMode::Hunting;
        let mut aware = Awareness::default();
        let target = Entity::from_raw_u32(42).expect("valid test entity");
        aware.set(
            target,
            AwarenessState::Searching {
                last_known_pos: Point::new(3, 3),
                giveup_at_turn: 100,
            },
            0,
        );
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
    }

    #[test]
    fn awareness_hidden_keeps_asleep_default() {
        use crate::stealth::Awareness;
        let mut ai = MonsterAI::default();
        ai.mode = MonsterAIMode::Asleep;
        let aware = Awareness::default(); // empty == Hidden
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Asleep);
    }


    #[test]
    fn is_alert_is_inverse_of_is_asleep_for_blessed_modes() {
        let mut ai = MonsterAI::default();
        // Asleep → not alert, is asleep.
        assert!(!ai.is_alert() && ai.is_asleep());
        ai.mode = MonsterAIMode::Hunting;
        assert!(ai.is_alert() && !ai.is_asleep());
        ai.mode = MonsterAIMode::Idle;
        assert!(ai.is_alert() && !ai.is_asleep());
    }
}
