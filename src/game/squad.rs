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
//!    A new leader is promoted from the survivors.
//!
//! 4. **Collective flee** — cowardly squad members check the *group's* total HP
//!    ratio (not just their own) against [`SquadConfig::flee_threshold`]. When
//!    the group is hurt enough, they all flee at once.
//!
//! ## Design choices
//!
//! - **No dynamic joining**: squads are formed at spawn time only. Solo monsters
//!   never join an existing squad, and two squads never merge.
//! - **No communication range**: all members alert regardless of distance on the
//!   same floor. This is simpler and prevents the exploit of picking off distant
//!   sentries one by one.
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
        magic::Enraged,
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
    /// No special effect.
    #[default]
    Nothing,
    /// Members lose their target and wander aimlessly.
    Scatter,
    /// Members gain a temporary damage bonus.
    Enrage,
}

impl LeaderDeathBehavior {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "scatter" => Self::Scatter,
            "enrage" => Self::Enrage,
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

/// When any squad member can see the player, alert the entire squad.
/// Only triggers the Asleep→Hunting transition — ongoing position tracking
/// remains per-individual.
fn squad_alert_system(
    mut squad_members: Query<(&SquadId, &mut MonsterAI, &Viewshed), With<Monster>>,
    player_query: Query<&Position, With<Player>>,
) {
    let Ok(player_pos) = player_query.single() else {
        return;
    };
    let player_point = player_pos.to_point();

    // Pass 1: collect which squads have a member that can see the player.
    let mut alerted_squads: std::collections::HashSet<SquadId> =
        std::collections::HashSet::new();
    for (squad_id, _ai, viewshed) in squad_members.iter() {
        if viewshed.visible_tiles.contains(&player_point) {
            alerted_squads.insert(*squad_id);
        }
    }

    if alerted_squads.is_empty() {
        return;
    }

    // Pass 2: wake all members of alerted squads.
    for (squad_id, mut ai, _viewshed) in squad_members.iter_mut() {
        if alerted_squads.contains(squad_id) {
            ai.alert_to_position(player_point);
        }
    }
}

/// When a squad member takes damage, alert the entire squad.
fn squad_damage_alert_system(
    damaged_query: Query<&SquadId, (With<Monster>, Changed<Health>)>,
    mut all_squad: Query<(&SquadId, &mut MonsterAI), With<Monster>>,
    player_query: Query<&Position, With<Player>>,
) {
    let Ok(player_pos) = player_query.single() else {
        return;
    };
    let player_point = player_pos.to_point();

    // Collect squads that had a member take damage.
    let mut damaged_squads: std::collections::HashSet<SquadId> =
        std::collections::HashSet::new();
    for squad_id in damaged_query.iter() {
        damaged_squads.insert(*squad_id);
    }

    if damaged_squads.is_empty() {
        return;
    }

    // Alert all members of damaged squads.
    for (squad_id, mut ai) in all_squad.iter_mut() {
        if damaged_squads.contains(squad_id) {
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
            LeaderDeathBehavior::Enrage => {
                for (entity, sid, _ai) in members.iter_mut() {
                    if sid == squad_id && entity != leader_entity {
                        commands.entity(entity).insert(Enraged { turns_remaining: 10 });
                    }
                }
                log_writer.write(GameLogMessage(
                    "The group flies into a rage!".to_string(),
                ));
            }
            LeaderDeathBehavior::Nothing => {}
        }

        // Promote a new leader from surviving members.
        for (entity, sid, _ai) in members.iter() {
            if sid == squad_id && entity != leader_entity {
                commands.entity(entity).insert(SquadLeader);
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the collective HP ratio for a squad. Returns `(total_current, total_max)`.
/// Used by the cowardly flee logic in `MonsterAI::execute()`.
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
