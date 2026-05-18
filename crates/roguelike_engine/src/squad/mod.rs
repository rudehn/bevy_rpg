//! Squad coordination framework.
//!
//! Monsters spawned as groups (rat packs, goblin war parties, kobold
//! raids) can be linked by a shared [`SquadId`] component. This module
//! provides three cooperative behaviors on top of that link:
//!
//! 1. **Shared alerting** — when any squad member spots the configured
//!    [`SquadTarget`] or takes damage, every nearby member transitions
//!    from `Asleep` to `Hunting`.
//! 2. **Collective morale** — [`squad_coordinator_system`] ticks each
//!    squad's morale as a function of member count, leader presence,
//!    collective HP, and time-since-contact. Members flee when the
//!    shared morale drops below their [`SquadConfig::flee_threshold`].
//! 3. **Shared blackboard** — [`SquadBlackboard`] holds alert level,
//!    known target position, retreat flag, and role assignments for
//!    squad-level GOAP decisions.
//!
//! # Engine/game boundary
//!
//! This module is engine-owned framework. It deliberately avoids
//! referencing any game-specific type:
//!
//! - **Target**: systems read [`SquadTarget`] (a resource), not a
//!   `Query<&Position, With<Player>>`. Games update `SquadTarget`
//!   each frame from whatever "primary threat" type they have.
//! - **Monster filter**: dropped. The queries already require
//!   `&mut MonsterAI` / `&SquadId`, which implicitly narrow to monsters.
//! - **Scheduling**: the plugin registers systems into
//!   [`SquadAlertSet`] and [`SquadReactionSet`] with no ordering or
//!   state predicates. Games configure those sets to run at the right
//!   point in their frame via `configure_sets`. See the extraction
//!   plan for the ordering recipe.

use bevy::prelude::*;
use bracket_lib::prelude::{DistanceAlg, Point};
use serde::{Deserialize, Serialize};

use crate::ai::MonsterAI;
use crate::combat::Health;
use crate::components::{Position, Viewshed};

// =====================================================================
// Components & Resources
// =====================================================================

/// Links all members of a squad. Entities with the same `SquadId`
/// share alerting, morale, and blackboard. Solo monsters have no
/// `SquadId` component.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct SquadId(pub u64);

/// Per-entity morale value (0.0 = routed, 1.0 = confident). Persists
/// across floor transitions. The squad coordinator modifies morale
/// based on shared events; individual GOAP planners read it for
/// flee/retreat decisions.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Morale(pub f32);

impl Default for Morale {
    fn default() -> Self {
        Self(0.6)
    }
}

impl Morale {
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    pub fn modify(&mut self, delta: f32) {
        self.0 = (self.0 + delta).clamp(0.0, 1.0);
    }
}

/// Marker for the squad's current leader. Used by squad-aware AI
/// (coordinator, blackboard) to identify who carries the shared
/// tactical state.
#[derive(Component, Debug, Clone)]
pub struct SquadLeader;

/// Per-entity configuration controlling squad behavior. Every member
/// of a squad carries the same config (copied from the spawn table).
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct SquadConfig {
    pub flee_threshold: f32,
}

impl Default for SquadConfig {
    fn default() -> Self {
        Self {
            flee_threshold: 0.5,
        }
    }
}

/// Global counter for generating unique [`SquadId`] values. Persisted
/// across save/load and floor transitions.
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct SquadIdCounter(pub u64);

impl SquadIdCounter {
    pub fn next(&mut self) -> SquadId {
        self.0 += 1;
        SquadId(self.0)
    }
}

/// The position squad systems alert toward and track. Games update
/// this resource each frame (typically from their player entity's
/// position). `None` means "no primary target" — squads stay at their
/// current awareness and no new alerts fire.
///
/// This resource is the engine/game boundary for squad alerting: the
/// engine never directly queries for the player. Games that have
/// multiple simultaneous targets (PvP, party-based) can still use
/// this by picking the closest threat or cycling.
#[derive(Resource, Default, Debug)]
pub struct SquadTarget {
    pub position: Option<Point>,
}

// =====================================================================
// System sets (scheduling seams for games to configure)
// =====================================================================

/// System set for squad-alert propagation. Runs [`squad_alert_system`].
///
/// Games configure this set's ordering and `run_if` via
/// `configure_sets` — the engine deliberately does not reference
/// game-side systems (`fov_update_system`) or app states
/// (`InGameState::Running`).
///
/// Typical game-side wiring:
///
/// ```ignore
/// app.configure_sets(
///     Update,
///     SquadAlertSet
///         .after(my_game_fov_system)
///         .run_if(in_state(MyGameState::Running)),
/// );
/// ```
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SquadAlertSet;

/// System set for squad-reaction systems (damage-alert + leader death).
///
/// Typical game-side wiring:
///
/// ```ignore
/// app.configure_sets(
///     Update,
///     SquadReactionSet
///         .after(CombatDamageSet)
///         .before(death_system)
///         .run_if(in_state(MyGameState::Running)),
/// );
/// ```
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SquadReactionSet;

// =====================================================================
// Plugin
// =====================================================================

/// Bevy plugin that registers squad resources and systems.
///
/// The plugin does NOT configure system ordering or `run_if`
/// predicates — that's the game's responsibility via
/// [`SquadAlertSet`] / [`SquadReactionSet`]. See the module-level
/// docs for the scheduling recipe.
pub struct SquadPlugin;

impl Plugin for SquadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SquadIdCounter>()
            .init_resource::<SquadTarget>()
            .add_systems(Update, squad_alert_system.in_set(SquadAlertSet))
            .add_systems(
                Update,
                squad_damage_alert_system.in_set(SquadReactionSet),
            );
    }
}

// =====================================================================
// Systems
// =====================================================================

/// Maximum distance (tiles) at which a squad member's alert propagates
/// to other members. Beyond this range, distant squad members stay
/// asleep until they see the target themselves.
pub const SQUAD_COMM_RANGE: f32 = 12.0;

/// When any squad member can see the configured [`SquadTarget`],
/// alert nearby squad members. Only triggers the
/// Asleep→Hunting transition — ongoing position tracking remains
/// per-individual. Members beyond [`SQUAD_COMM_RANGE`] of the
/// alerting member are not woken.
pub fn squad_alert_system(
    target: Res<SquadTarget>,
    mut squad_members: Query<(&SquadId, &mut MonsterAI, &Viewshed, &Position)>,
) {
    let Some(target_point) = target.position else {
        return;
    };

    // Pass 1: collect which squads have an alerting member, along with that member's position.
    let mut alerters: Vec<(SquadId, Point)> = Vec::new();
    for (squad_id, _ai, viewshed, pos) in squad_members.iter() {
        if viewshed.visible_tiles.contains(&target_point) {
            alerters.push((*squad_id, pos.to_point()));
        }
    }

    if alerters.is_empty() {
        return;
    }

    // Pass 2: wake squad members within communication range of any alerting member.
    for (squad_id, mut ai, _viewshed, pos) in squad_members.iter_mut() {
        let member_point = pos.to_point();
        let should_alert = alerters.iter().any(|(sid, alerter_pos)| {
            *sid == *squad_id
                && DistanceAlg::Pythagoras.distance2d(member_point, *alerter_pos)
                    <= SQUAD_COMM_RANGE
        });
        if should_alert {
            ai.alert_to_position(target_point);
        }
    }
}

/// When a squad member takes damage, alert nearby squad members.
pub fn squad_damage_alert_system(
    target: Res<SquadTarget>,
    damaged_query: Query<(&SquadId, &Position, &Health), Changed<Health>>,
    mut all_squad: Query<(&SquadId, &mut MonsterAI, &Position)>,
) {
    let Some(target_point) = target.position else {
        return;
    };

    // Collect damaged squad members and their positions.
    // Guard: skip entities whose Health just got inserted (current == max on spawn);
    // Changed<Health> fires on component insertion, not just mutation.
    let mut damaged: Vec<(SquadId, Point)> = Vec::new();
    for (squad_id, pos, health) in damaged_query.iter() {
        if health.current < health.max {
            damaged.push((*squad_id, pos.to_point()));
        }
    }

    if damaged.is_empty() {
        return;
    }

    // Alert squad members within communication range of the damaged member.
    for (squad_id, mut ai, pos) in all_squad.iter_mut() {
        let member_point = pos.to_point();
        let should_alert = damaged.iter().any(|(sid, dmg_pos)| {
            *sid == *squad_id
                && DistanceAlg::Pythagoras.distance2d(member_point, *dmg_pos)
                    <= SQUAD_COMM_RANGE
        });
        if should_alert {
            ai.alert_to_position(target_point);
        }
    }
}

// =====================================================================
// Squad Blackboard — Shared tactical state
// =====================================================================

/// Alert level for a squad, set by the coordinator.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AlertLevel {
    /// Squad has not detected the target.
    #[default]
    Unaware,
    /// At least one member has seen the target, but no combat yet.
    Alerted,
    /// Squad is actively engaged in combat with the target.
    InCombat,
}

/// Tactical role assigned to a squad member by the coordinator.
/// How each role behaves is defined by per-archetype GOAP actions
/// on the game side.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SquadRole {
    /// Go find and alert nearby same-faction sleeping monsters.
    Scout,
    /// Stay between the leader and the threat.
    Guard,
    /// Circle around to attack from the side.
    Flanker,
    /// Stay adjacent to the leader.
    Bodyguard,
    /// Shoot and reposition behind allies (ranged).
    Skirmisher,
    /// Heal, buff, stay in the back line.
    Support,
    /// The leader — issues orders, stays behind the front line.
    Commander,
}

/// Shared tactical state for a squad, attached to the squad leader
/// entity. Updated each processing cycle by
/// [`squad_coordinator_system`]. Game-side GOAP goal/action
/// configurations read this to make squad-aware decisions.
#[derive(Component, Debug, Clone, Default)]
pub struct SquadBlackboard {
    // --- Awareness ---
    pub alert_level: AlertLevel,
    pub known_player_pos: Option<Point>,
    pub turns_since_contact: u32,

    // --- Tactical state ---
    pub retreat_ordered: bool,
    pub fallback_point: Option<Point>,

    // --- Role assignments ---
    pub roles: std::collections::HashMap<Entity, SquadRole>,

    // --- Position reservation (for chokepoint defense) ---
    pub reserved_positions: std::collections::HashMap<Point, Entity>,
}

// =====================================================================
// Squad Coordinator System
// =====================================================================

/// Updates [`SquadBlackboard`] for each squad that has one, ticks
/// member morale based on squad-level factors, and decides whether
/// the squad should retreat.
///
/// Called by the game's AI brain phase. The game wires this into its
/// own scheduling (typically before GOAP dispatch so plans can read
/// the updated blackboard).
pub fn squad_coordinator_system(
    target: Res<SquadTarget>,
    mut blackboard_query: Query<(Entity, &SquadId, &mut SquadBlackboard), With<SquadLeader>>,
    member_query: Query<(
        Entity,
        &SquadId,
        &Position,
        &Health,
        Option<&Viewshed>,
        Has<SquadLeader>,
    )>,
    mut morale_query: Query<&mut Morale>,
) {
    let target_pos = target.position;

    for (_leader_entity, squad_id, mut bb) in blackboard_query.iter_mut() {
        // Gather all living members of this squad.
        let members: Vec<_> = member_query
            .iter()
            .filter(|(_, sid, _, health, _, _)| **sid == *squad_id && health.current > 0)
            .collect();

        if members.is_empty() {
            continue;
        }

        let member_count = members.len();

        // --- Awareness ---
        let any_sees_target = target_pos
            .map(|tp| {
                members.iter().any(|(_, _, _, _, viewshed, _)| {
                    viewshed
                        .map(|v| v.visible_tiles.contains(&tp))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if any_sees_target {
            bb.alert_level = AlertLevel::InCombat;
            bb.known_player_pos = target_pos;
            bb.turns_since_contact = 0;
        } else if bb.alert_level == AlertLevel::InCombat {
            bb.turns_since_contact += 1;
            if bb.turns_since_contact > 10 {
                bb.alert_level = AlertLevel::Alerted;
            }
        }

        // --- Morale modifiers (applied to each member) ---
        let has_leader = members
            .iter()
            .any(|(_, _, _, _, _, is_leader)| *is_leader);
        let has_healer = false; // TODO: detect healer role once assignments are wired up

        let total_hp: i32 = members.iter().map(|(_, _, _, h, _, _)| h.current).sum();
        let total_max_hp: i32 = members.iter().map(|(_, _, _, h, _, _)| h.max).sum();
        let squad_hp_ratio = if total_max_hp > 0 {
            total_hp as f32 / total_max_hp as f32
        } else {
            1.0
        };

        for (entity, _, _, health, _, _) in &members {
            if let Ok(mut morale) = morale_query.get_mut(*entity) {
                let mut modifier = 0.0f32;

                // Squad bonuses
                if has_leader {
                    modifier += 0.2;
                }
                if has_healer {
                    modifier += 0.1;
                }
                if member_count >= 3 {
                    modifier += 0.15;
                } else if member_count >= 2 {
                    modifier += 0.05;
                }

                // Squad HP penalties
                if squad_hp_ratio < 0.25 {
                    modifier -= 0.2;
                } else if squad_hp_ratio < 0.5 {
                    modifier -= 0.1;
                }

                // Personal HP penalties
                let hp_ratio = if health.max > 0 {
                    health.current as f32 / health.max as f32
                } else {
                    1.0
                };
                if hp_ratio < 0.25 {
                    modifier -= 0.15;
                } else if hp_ratio < 0.5 {
                    modifier -= 0.1;
                }

                // Out-of-combat recovery
                if bb.turns_since_contact > 5 {
                    modifier += 0.05;
                }

                // Apply modifier toward a target morale (smooth, not instant)
                let target_morale = (morale.0 + modifier).clamp(0.0, 1.0);
                // Blend toward target — faster when dropping, slower when recovering
                if target_morale < morale.0 {
                    morale.0 = (morale.0 - 0.05).max(target_morale); // drop quickly
                } else {
                    morale.0 = (morale.0 + 0.02).min(target_morale); // recover slowly
                }
                morale.0 = morale.0.clamp(0.0, 1.0);
            }
        }

        // --- Retreat decision ---
        let avg_morale: f32 = {
            let morales: Vec<f32> = members
                .iter()
                .filter_map(|(e, _, _, _, _, _)| morale_query.get(*e).ok().map(|m| m.0))
                .collect();
            if morales.is_empty() {
                0.5
            } else {
                morales.iter().sum::<f32>() / morales.len() as f32
            }
        };

        if avg_morale < 0.15 {
            // Rout — squad dissolves, handled by existing scatter logic
            bb.retreat_ordered = false;
        } else if avg_morale < 0.3 && !bb.retreat_ordered {
            // Order retreat
            bb.retreat_ordered = true;
        }
        if avg_morale > 0.5 {
            bb.retreat_ordered = false; // Morale recovered — cancel retreat
        }
    }
}

// =====================================================================
// Helpers
// =====================================================================

/// Compute the collective HP ratio for a squad. Returns
/// `(total_current, total_max)`.
pub fn compute_squad_hp(squad_id: SquadId, world: &mut World) -> (i32, i32) {
    let mut total_current = 0i32;
    let mut total_max = 0i32;

    let mut query = world.query::<(&SquadId, &Health)>();
    for (sid, health) in query.iter(world) {
        if *sid == squad_id {
            total_current += health.current;
            total_max += health.max;
        }
    }

    (total_current, total_max)
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morale_clamps_to_valid_range() {
        let m = Morale::new(1.5);
        assert_eq!(m.0, 1.0);
        let m = Morale::new(-0.2);
        assert_eq!(m.0, 0.0);
    }

    #[test]
    fn morale_modify_clamps_results() {
        let mut m = Morale::new(0.8);
        m.modify(0.5);
        assert_eq!(m.0, 1.0);
        m.modify(-2.0);
        assert_eq!(m.0, 0.0);
    }

    #[test]
    fn morale_default_is_0_6() {
        let m = Morale::default();
        assert!((m.0 - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn squad_id_counter_produces_unique_ids() {
        let mut c = SquadIdCounter::default();
        let a = c.next();
        let b = c.next();
        let d = c.next();
        assert_ne!(a, b);
        assert_ne!(b, d);
        assert_ne!(a, d);
    }

    #[test]
    fn squad_config_default_threshold_is_half() {
        let c = SquadConfig::default();
        assert_eq!(c.flee_threshold, 0.5);
    }

    #[test]
    fn squad_target_default_is_none() {
        let t = SquadTarget::default();
        assert!(t.position.is_none());
    }
}
