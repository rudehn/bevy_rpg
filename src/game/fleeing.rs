//! Sticky `Fleeing` state for monsters that took damage.
//!
//! When a monster's HP drops below its `MonsterAI.flee_at_hp_percent`
//! threshold, [`damage_triggers_flee`] inserts a [`Fleeing`] component
//! that drives panicked behavior for at least [`FLEE_MIN_TURNS`] turns.
//! [`maybe_exit_fleeing`] removes it once the monster has been safe
//! for long enough AND HP has recovered above a hysteresis margin.
//!
//! This is a **game-side overlay** on the engine's `MonsterAIMode`
//! enum — the engine's mode stays `Asleep`/`Idle`/`Hunting` and the
//! `Fleeing` marker rides on top. The tactic dispatcher's snapshot
//! builder checks for the marker and synthesizes
//! `AiMode::Fleeing { .. }` for the resolver, so tactics like
//! `FleePanicked` only see the `Fleeing` variant of `AiMode`.
//!
//! See `docs/design/TACTICS.md` §"FSM additions: the Fleeing mode"
//! for the design rationale.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use roguelike_engine::ai::monster_ai::{MonsterAI, MonsterAIMode};
use roguelike_engine::turn::TurnManager;

use crate::components::{Faction, Position, Viewshed};
use crate::game::combat::{DamageEvent, Health};
use crate::game::tactics::TacticBrain;
use crate::game::turns::ProcessingPhase;
use roguelike_engine::factions::FactionMatrix;

/// Minimum number of turns a monster must remain in `Fleeing` before
/// the exit transition can fire. Prevents one-turn flee flickers when
/// the player retreats out of view immediately after wounding.
pub const FLEE_MIN_TURNS: u32 = 10;

/// Extra HP margin (as a fraction of max) that must be recovered
/// above `flee_at_hp_percent` before the exit transition fires.
/// Without this, a monster that flees at 30% HP and recovers to 31%
/// would immediately re-engage, then drop to 29% on the next hit and
/// flee again — twitchy "flicker" behavior. With margin 0.15, the
/// 30%-flee monster only exits Fleeing at >= 45% HP.
pub const FLEE_HYSTERESIS_MARGIN: f32 = 0.15;

/// Game-side marker component overlaying the engine's `MonsterAIMode`.
/// When present, the tactic snapshot builder synthesizes
/// `AiMode::Fleeing { .. }` regardless of the engine mode value.
///
/// Persists across save/load via [`SavedFleeing`] in the save schema
/// (added at schema v8).
#[derive(Component, Debug, Clone, Copy)]
pub struct Fleeing {
    /// `TurnManager.current_time` at the moment of entry. Used to
    /// gate the exit transition on `FLEE_MIN_TURNS` elapsed.
    pub since_turn: u32,
    /// Position of the attacker at the moment of entry (if known).
    /// Tactics flee away from this position when no current threat
    /// is visible.
    pub last_known_threat_pos: Option<Point>,
}

/// Save-wire representation of [`Fleeing`]. `Point` doesn't derive
/// `Serialize` directly, so we flatten to `(x, y)` here. Optional
/// fields default to `None` for v7-and-earlier saves.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SavedFleeing {
    pub since_turn: u32,
    #[serde(default)]
    pub last_known_threat_x: Option<i32>,
    #[serde(default)]
    pub last_known_threat_y: Option<i32>,
}

impl SavedFleeing {
    pub fn from_component(f: &Fleeing) -> Self {
        Self {
            since_turn: f.since_turn,
            last_known_threat_x: f.last_known_threat_pos.map(|p| p.x),
            last_known_threat_y: f.last_known_threat_pos.map(|p| p.y),
        }
    }

    pub fn to_component(&self) -> Fleeing {
        let last_known_threat_pos = match (self.last_known_threat_x, self.last_known_threat_y) {
            (Some(x), Some(y)) => Some(Point::new(x, y)),
            _ => None,
        };
        Fleeing {
            since_turn: self.since_turn,
            last_known_threat_pos,
        }
    }
}

// =====================================================================
// Transitions
// =====================================================================

/// Reads `DamageEvent`s and inserts `Fleeing` on targets whose HP
/// drops below their flee threshold. Idempotent — already-fleeing
/// monsters keep their existing `since_turn` (no flee-timer reset on
/// continued damage; the timer measures "time since panic started").
///
/// Asleep monsters are excluded — they wake to Hunting via the
/// awareness/perception path before damage can trigger flee. This
/// preserves the "consciousness before panic" invariant from the
/// design doc.
pub fn damage_triggers_flee(
    mut damage_events: MessageReader<DamageEvent>,
    mut commands: Commands,
    // Filtered on `TacticBrain` so the Fleeing layer only enrolls
    // monsters on the new AI path. FSM/GOAP monsters continue to
    // handle flee reactively per-turn in their own dispatchers; once
    // they migrate, the filter becomes a no-op and the line stays
    // for cheap explicit intent.
    monsters: Query<(&Health, &MonsterAI, Option<&Fleeing>), With<TacticBrain>>,
    attackers: Query<&Position>,
    turn_manager: Res<TurnManager>,
) {
    for event in damage_events.read() {
        let Ok((hp, ai, already_fleeing)) = monsters.get(event.target) else {
            continue;
        };
        // Already fleeing — keep the existing since_turn.
        if already_fleeing.is_some() {
            continue;
        }
        if ai.flee_at_hp_percent <= 0.0 {
            continue;
        }
        if ai.mode == MonsterAIMode::Asleep {
            continue;
        }
        if hp.max <= 0 {
            continue;
        }
        let hp_pct = hp.current as f32 / hp.max as f32;
        if hp_pct >= ai.flee_at_hp_percent {
            continue;
        }
        let attacker_pos = event
            .attacker
            .and_then(|a| attackers.get(a).ok())
            .map(|p| p.to_point());
        commands.entity(event.target).insert(Fleeing {
            since_turn: turn_manager.current_time,
            last_known_threat_pos: attacker_pos,
        });
    }
}

/// Removes `Fleeing` from monsters that have been safe for at least
/// `FLEE_MIN_TURNS` AND recovered HP above
/// `flee_at_hp_percent + FLEE_HYSTERESIS_MARGIN`. "Safe" means no
/// hostile faction member is currently visible.
pub fn maybe_exit_fleeing(
    mut commands: Commands,
    fleeing_q: Query<(
        Entity,
        &Fleeing,
        &Health,
        &MonsterAI,
        &Viewshed,
        Option<&Faction>,
    )>,
    others: Query<(Entity, &Position, Option<&Faction>)>,
    turn_manager: Res<TurnManager>,
    matrix: Res<FactionMatrix>,
) {
    for (entity, fleeing, hp, ai, viewshed, my_faction) in &fleeing_q {
        let elapsed = turn_manager
            .current_time
            .saturating_sub(fleeing.since_turn);
        if elapsed < FLEE_MIN_TURNS {
            continue;
        }
        let recovery_threshold = (ai.flee_at_hp_percent + FLEE_HYSTERESIS_MARGIN).min(1.0);
        if hp.max <= 0 {
            continue;
        }
        let hp_pct = hp.current as f32 / hp.max as f32;
        if hp_pct < recovery_threshold {
            continue;
        }
        // Any hostile in viewshed blocks exit.
        let threat_visible = others.iter().any(|(other, pos, other_faction)| {
            if other == entity {
                return false;
            }
            if !viewshed.visible_tiles.contains(&pos.to_point()) {
                return false;
            }
            crate::game::factions::factions_hostile(my_faction, other_faction, &matrix)
        });
        if threat_visible {
            continue;
        }
        // Transition out: remove marker, downgrade engine mode to Idle.
        commands.entity(entity).remove::<Fleeing>();
        commands.queue(move |world: &mut World| {
            if let Some(mut ai) = world.get_mut::<MonsterAI>(entity) {
                ai.mode = MonsterAIMode::Idle;
                ai.chase_distance = 0;
            }
        });
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct FleeingPlugin;

impl Plugin for FleeingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (damage_triggers_flee, maybe_exit_fleeing)
                .in_set(ProcessingPhase::ResolveActions),
        );
    }
}

// =====================================================================
// Tests
// =====================================================================
//
// The transition systems read from World resources and queries, so
// unit tests would need a minimal Bevy `App` to exercise them. The
// pure logic — flee threshold check + hysteresis math — is small
// enough to inline-test as helpers. Phase 3's canary migration
// (Giant Rat) provides the integration test by playthrough.

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure helper: should the entry transition fire?
    fn should_enter_flee(hp_current: i32, hp_max: i32, threshold: f32) -> bool {
        if threshold <= 0.0 || hp_max <= 0 {
            return false;
        }
        (hp_current as f32 / hp_max as f32) < threshold
    }

    /// Pure helper: has HP recovered enough to exit flee?
    fn hp_recovered(hp_current: i32, hp_max: i32, threshold: f32, margin: f32) -> bool {
        if hp_max <= 0 {
            return false;
        }
        let pct = hp_current as f32 / hp_max as f32;
        pct >= (threshold + margin).min(1.0)
    }

    #[test]
    fn no_flee_when_threshold_zero() {
        assert!(!should_enter_flee(2, 10, 0.0));
    }

    #[test]
    fn no_flee_when_hp_above_threshold() {
        assert!(!should_enter_flee(5, 10, 0.3));
    }

    #[test]
    fn flees_when_hp_below_threshold() {
        assert!(should_enter_flee(2, 10, 0.3));
    }

    #[test]
    fn no_recovery_when_pct_below_margin() {
        // threshold 0.3, margin 0.15 → recovery at 0.45
        assert!(!hp_recovered(4, 10, 0.3, 0.15)); // 40% — below recovery
    }

    #[test]
    fn recovery_when_pct_above_margin() {
        assert!(hp_recovered(5, 10, 0.3, 0.15)); // 50% — above 45%
    }

    #[test]
    fn recovery_clamped_at_one() {
        // threshold 0.9 + margin 0.15 = 1.05 → clamped to 1.0
        assert!(hp_recovered(10, 10, 0.9, 0.15));
        assert!(!hp_recovered(9, 10, 0.9, 0.15));
    }

    #[test]
    fn no_recovery_when_max_zero() {
        assert!(!hp_recovered(0, 0, 0.3, 0.15));
    }
}
