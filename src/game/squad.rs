//! # Squad System — Coordinated Group AI for Monster Hordes
//!
//! Monsters spawned as groups (rat packs, goblin war parties) are linked by a
//! shared [`SquadId`] component. This gives them four cooperative behaviors:
//!
//! 1. **Leader leashing** — non-leader squad members pathfind toward their
//!    [`SquadLeader`] when more than 4 tiles away. This keeps squads moving
//!    through corridors as a group. Within leash range, each member pathfinds
//!    independently (so they fan out naturally in open rooms).
//!
//! 2. **Shared alerting** — when any squad member spots the player or takes
//!    damage, every member of the squad transitions from `Asleep` to `Hunting`.
//!    This is the primary tactical consequence: you cannot silently pick off a
//!    sentry without alerting the whole group.
//!
//! 3. **Leader death effects** — each squad has one [`SquadLeader`]. When the
//!    leader dies, the squad's [`SquadConfig::on_leader_death`] fires:
//!    - `Scatter` — remaining members lose their target and wander.
//!    - `Enrage` — remaining members gain a temporary damage bonus.
//!    - `Nothing` — no special effect.
//!      A new leader is promoted from the survivors.
//!
//! 4. **Collective flee** — cowardly squad members check the *group's* total HP
//!    ratio (not just their own) against [`SquadConfig::flee_threshold`]. When
//!    the group is hurt enough, they all flee at once.
//!
//! ## Design choices
//!
//! - **No dynamic joining**: squads are formed at spawn time only. Solo monsters
//!   never join an existing squad, and two squads never merge.
//! - **Communication range**: alerting only propagates to squad members within
//!   12 tiles of the alerting member. Distant sentries stay asleep until they
//!   see the player or a nearby squad member alerts them.
//! - **No centralized state**: all squad information is derived by querying
//!   entities with matching `SquadId`. This avoids sync bugs with despawned
//!   entities and keeps save/load trivial.
//!
//! ## System ordering
//!
//! ```text
//! fov_update_system
//!   → squad_alert_system          (wakes squads when any member sees player)
//!     → monster_ai_dispatch       (individual AI runs with updated modes)
//!
//! CombatDamageSet
//!   → squad_damage_alert_system   (wakes squads when any member takes damage)
//!   → squad_leader_death_system   (scatter/enrage on leader kill)
//!     → death_system
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{Monster, Position, Viewshed},
    game::{
        MonsterAI,
        combat::{CombatDamageSet, Health},
    },
    player::Player,
    ui::game_log::GameLogMessage,
};

// ---------------------------------------------------------------------------
// Components & Resources
// ---------------------------------------------------------------------------

/// Links all members of a monster squad. Entities with the same `SquadId` share
/// alerting and morale. Solo monsters have no `SquadId` component.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct SquadId(pub u64);

/// Per-entity morale value (0.0 = routed, 1.0 = confident). Persists across
/// floor transitions. The squad coordinator modifies morale based on shared
/// events; individual GOAP planners read it for flee/retreat decisions.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Morale(pub f32);

impl Default for Morale {
    fn default() -> Self { Self(0.6) }
}

impl Morale {
    pub fn new(value: f32) -> Self { Self(value.clamp(0.0, 1.0)) }

    pub fn modify(&mut self, delta: f32) {
        self.0 = (self.0 + delta).clamp(0.0, 1.0);
    }
}

/// Marker for the squad's current leader. When the leader dies, effects from
/// [`SquadConfig::on_leader_death`] trigger and a new leader is promoted.
#[derive(Component, Debug, Clone)]
pub struct SquadLeader;

/// Per-entity configuration controlling squad behavior. Every member of a squad
/// carries the same config (copied from the spawn table entry).
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct SquadConfig {
    pub on_leader_death: LeaderDeathBehavior,
    pub flee_threshold: f32,
}

impl Default for SquadConfig {
    fn default() -> Self {
        Self {
            on_leader_death: LeaderDeathBehavior::Nothing,
            flee_threshold: 0.5,
        }
    }
}

/// What happens to remaining squad members when the leader dies.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum LeaderDeathBehavior {
    /// No special effect; squad dissolves.
    #[default]
    Nothing,
    /// Members lose their target and wander aimlessly.
    Scatter,
}

impl LeaderDeathBehavior {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "scatter" => Self::Scatter,
            _ => Self::Nothing,
        }
    }
}

/// Global counter for generating unique [`SquadId`] values. Persisted across
/// save/load and floor transitions.
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct SquadIdCounter(pub u64);

impl SquadIdCounter {
    pub fn next(&mut self) -> SquadId {
        self.0 += 1;
        SquadId(self.0)
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct SquadPlugin;

impl Plugin for SquadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SquadIdCounter>()
            .add_systems(
                Update,
                squad_alert_system
                    .after(crate::game::systems::fov_update_system)
                    .run_if(in_state(crate::game::InGameState::Running)),
            )
            .add_systems(
                Update,
                (
                    squad_damage_alert_system.after(CombatDamageSet),
                    squad_leader_death_system
                        .after(CombatDamageSet)
                        .before(crate::game::combat::death_system),
                )
                    .run_if(in_state(crate::game::InGameState::Running)),
            );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Maximum distance (tiles) at which a squad member's alert propagates to
/// other members. Beyond this range, distant squad members stay asleep.
const SQUAD_COMM_RANGE: f32 = 12.0;

/// When any squad member can see the player, alert nearby squad members.
/// Only triggers the Asleep→Hunting transition — ongoing position tracking
/// remains per-individual. Members beyond `SQUAD_COMM_RANGE` of the alerting
/// member are not woken.
fn squad_alert_system(
    mut squad_members: Query<(&SquadId, &mut MonsterAI, &Viewshed, &Position), With<Monster>>,
    player_query: Query<&Position, With<Player>>,
) {
    let Ok(player_pos) = player_query.single() else {
        return;
    };
    let player_point = player_pos.to_point();

    // Pass 1: collect which squads have an alerting member, along with that member's position.
    let mut alerters: Vec<(SquadId, bracket_lib::prelude::Point)> = Vec::new();
    for (squad_id, _ai, viewshed, pos) in squad_members.iter() {
        if viewshed.visible_tiles.contains(&player_point) {
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
                && bracket_lib::prelude::DistanceAlg::Pythagoras
                    .distance2d(member_point, *alerter_pos)
                    <= SQUAD_COMM_RANGE
        });
        if should_alert {
            ai.alert_to_position(player_point);
        }
    }
}

/// When a squad member takes damage, alert nearby squad members.
fn squad_damage_alert_system(
    damaged_query: Query<(&SquadId, &Position, &Health), (With<Monster>, Changed<Health>)>,
    mut all_squad: Query<(&SquadId, &mut MonsterAI, &Position), With<Monster>>,
    player_query: Query<&Position, With<Player>>,
) {
    let Ok(player_pos) = player_query.single() else {
        return;
    };
    let player_point = player_pos.to_point();

    // Collect damaged squad members and their positions.
    // Guard: skip entities whose Health just got inserted (current == max on spawn);
    // Changed<Health> fires on component insertion, not just mutation.
    let mut damaged: Vec<(SquadId, bracket_lib::prelude::Point)> = Vec::new();
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
                && bracket_lib::prelude::DistanceAlg::Pythagoras
                    .distance2d(member_point, *dmg_pos)
                    <= SQUAD_COMM_RANGE
        });
        if should_alert {
            ai.alert_to_position(player_point);
        }
    }
}

/// When a squad leader dies, apply morale effects and promote a new leader.
fn squad_leader_death_system(
    dead_leaders: Query<
        (Entity, &SquadId, &SquadConfig, &Health),
        (With<SquadLeader>, With<Monster>),
    >,
    mut members: Query<
        (Entity, &SquadId, &mut MonsterAI),
        (With<Monster>, Without<SquadLeader>),
    >,
    mut commands: Commands,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for (leader_entity, squad_id, config, health) in dead_leaders.iter() {
        // Only trigger when leader is actually dead (HP <= 0)
        if health.current > 0 {
            continue;
        }

        match config.on_leader_death {
            LeaderDeathBehavior::Scatter => {
                for (entity, sid, mut ai) in members.iter_mut() {
                    if sid == squad_id && entity != leader_entity {
                        ai.scatter();
                    }
                }
                log_writer.write(GameLogMessage("The group scatters!".to_string()));
            }
            LeaderDeathBehavior::Nothing => {}
        }

        // Promote a new leader from surviving members, transferring the blackboard.
        for (entity, sid, _ai) in members.iter() {
            if sid == squad_id && entity != leader_entity {
                commands.entity(entity).insert((SquadLeader, SquadBlackboard::default()));
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Squad Blackboard — Shared state for coordinated squad AI
// ---------------------------------------------------------------------------

/// Alert level for a squad, set by the coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AlertLevel {
    /// Squad has not detected the player.
    #[default]
    Unaware,
    /// At least one member has seen the player, but no combat yet.
    Alerted,
    /// Squad is actively engaged in combat with the player.
    InCombat,
}

/// Tactical role assigned to a squad member by the coordinator.
/// How each role behaves is defined by per-archetype GOAP actions.
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

/// Shared tactical state for a squad, attached to the squad leader entity.
/// Updated each processing cycle by `squad_coordinator_system`.
/// Generic — any faction can use it. Faction-specific behavior comes from
/// GOAP goal/action configurations reading this data.
#[derive(Component, Debug, Clone, Default)]
pub struct SquadBlackboard {
    // --- Awareness ---
    pub alert_level: AlertLevel,
    pub known_player_pos: Option<bracket_lib::prelude::Point>,
    pub turns_since_contact: u32,

    // --- Tactical state ---
    pub retreat_ordered: bool,
    pub fallback_point: Option<bracket_lib::prelude::Point>,

    // --- Role assignments ---
    pub roles: std::collections::HashMap<Entity, SquadRole>,

    // --- Position reservation (for chokepoint defense) ---
    pub reserved_positions: std::collections::HashMap<bracket_lib::prelude::Point, Entity>,
}

// ---------------------------------------------------------------------------
// Squad Coordinator System
// ---------------------------------------------------------------------------

/// Updates `SquadBlackboard` for each squad that has one.
/// Runs in `ProcessingPhase::Brain` before `goap_ai_dispatch` so GOAP
/// entities can read the blackboard when gathering world state.
pub fn squad_coordinator_system(
    mut blackboard_query: Query<(Entity, &SquadId, &mut SquadBlackboard), With<SquadLeader>>,
    member_query: Query<(Entity, &SquadId, &Position, &Health, Option<&Viewshed>, Has<SquadLeader>), With<Monster>>,
    mut morale_query: Query<&mut Morale>,
    player_query: Query<&Position, With<Player>>,
) {
    let player_pos = player_query.single().ok().map(|p| p.to_point());

    for (_leader_entity, squad_id, mut bb) in blackboard_query.iter_mut() {
        // Gather all living members of this squad.
        let members: Vec<_> = member_query.iter()
            .filter(|(_, sid, _, health, _, _)| **sid == *squad_id && health.current > 0)
            .collect();

        if members.is_empty() {
            continue;
        }

        let member_count = members.len();

        // --- Awareness ---
        let any_sees_player = player_pos.map(|pp| {
            members.iter().any(|(_, _, _, _, viewshed, _)| {
                viewshed.map(|v| v.visible_tiles.contains(&pp)).unwrap_or(false)
            })
        }).unwrap_or(false);

        if any_sees_player {
            bb.alert_level = AlertLevel::InCombat;
            bb.known_player_pos = player_pos;
            bb.turns_since_contact = 0;
        } else if bb.alert_level == AlertLevel::InCombat {
            bb.turns_since_contact += 1;
            if bb.turns_since_contact > 10 {
                bb.alert_level = AlertLevel::Alerted;
            }
        }

        // --- Morale modifiers (applied to each member) ---
        let has_leader = members.iter().any(|(_, _, _, _, _, is_leader)| *is_leader);
        // TODO: detect healer role once role assignment is implemented (Phase 4)
        let has_healer = false;

        let total_hp: i32 = members.iter().map(|(_, _, _, h, _, _)| h.current).sum();
        let total_max_hp: i32 = members.iter().map(|(_, _, _, h, _, _)| h.max).sum();
        let squad_hp_ratio = if total_max_hp > 0 { total_hp as f32 / total_max_hp as f32 } else { 1.0 };

        for (entity, _, _, health, _, _) in &members {
            if let Ok(mut morale) = morale_query.get_mut(*entity) {
                let mut modifier = 0.0f32;

                // Squad bonuses
                if has_leader { modifier += 0.2; }
                if has_healer { modifier += 0.1; }
                if member_count >= 3 { modifier += 0.15; }
                else if member_count >= 2 { modifier += 0.05; }

                // Squad HP penalties
                if squad_hp_ratio < 0.25 { modifier -= 0.2; }
                else if squad_hp_ratio < 0.5 { modifier -= 0.1; }

                // Personal HP penalties
                let hp_ratio = if health.max > 0 { health.current as f32 / health.max as f32 } else { 1.0 };
                if hp_ratio < 0.25 { modifier -= 0.15; }
                else if hp_ratio < 0.5 { modifier -= 0.1; }

                // Out-of-combat recovery
                if bb.turns_since_contact > 5 { modifier += 0.05; }

                // Apply modifier toward a target morale (smooth, not instant)
                let target = (morale.0 + modifier).clamp(0.0, 1.0);
                // Blend toward target — faster when dropping, slower when recovering
                if target < morale.0 {
                    morale.0 = (morale.0 - 0.05).max(target); // drop quickly
                } else {
                    morale.0 = (morale.0 + 0.02).min(target); // recover slowly
                }
                morale.0 = morale.0.clamp(0.0, 1.0);
            }
        }

        // --- Retreat decision ---
        let avg_morale: f32 = {
            let morales: Vec<f32> = members.iter()
                .filter_map(|(e, _, _, _, _, _)| morale_query.get(*e).ok().map(|m| m.0))
                .collect();
            if morales.is_empty() { 0.5 } else { morales.iter().sum::<f32>() / morales.len() as f32 }
        };

        if avg_morale < 0.15 {
            // Rout — squad dissolves, handled by existing scatter logic
            bb.retreat_ordered = false;
        } else if avg_morale < 0.3 && !bb.retreat_ordered {
            // Order retreat
            bb.retreat_ordered = true;
            // Fallback to leader's spawn position (set by spawner on MonsterAI)
            // For now, use known_player_pos inverse — flee away from player
            // Full fallback point logic comes with Dijkstra maps (Phase 8)
        }
        if avg_morale > 0.5 {
            bb.retreat_ordered = false; // Morale recovered — cancel retreat
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the collective HP ratio for a squad. Returns `(total_current, total_max)`.
#[allow(dead_code)]
pub fn compute_squad_hp(squad_id: SquadId, world: &mut World) -> (i32, i32) {
    let mut total_current = 0i32;
    let mut total_max = 0i32;

    let mut query = world.query_filtered::<(&SquadId, &Health), With<Monster>>();
    for (sid, health) in query.iter(world) {
        if *sid == squad_id {
            total_current += health.current;
            total_max += health.max;
        }
    }

    (total_current, total_max)
}
