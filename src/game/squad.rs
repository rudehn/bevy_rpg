//! Squad coordination — game-side wiring.
//!
//! The entire squad framework (components, resources,
//! `SquadPlugin`, and all systems) lives in
//! `roguelike_engine::squad`. This file wires it into The Veiled
//! Tyrant's scheduling.
//!
//! # Scheduling
//!
//! The engine's `SquadPlugin` registers its systems into two empty
//! `SystemSet`s (`SquadAlertSet`, `SquadReactionSet`) with no
//! ordering or `run_if`. We configure both sets here so the engine
//! crate never needs to know about game-side system names like
//! `fov_update_system`, `CombatDamageSet`, `death_system`, or
//! `InGameState::Running`.
//!
//! # Target
//!
//! The engine's squad systems read a [`SquadTarget`] resource
//! instead of querying for the player directly. We update
//! [`SquadTarget`] each frame from the game's player entity.

use bevy::prelude::*;

use crate::game::combat::CombatDamageSet;
use crate::player::Player;

// Re-export everything from the engine crate so existing game code
// that imports `crate::game::squad::*` (spawner, save, AI, GOAP, map
// builders, etc.) continues to resolve without changes.
pub use roguelike_engine::squad::{
    compute_squad_hp, squad_alert_system, squad_coordinator_system, squad_damage_alert_system,
    AlertLevel, Morale, SquadAlertSet,
    SquadBlackboard, SquadConfig, SquadId, SquadIdCounter, SquadLeader, SquadReactionSet, SquadRole,
    SquadTarget, SQUAD_COMM_RANGE,
};

/// The Veiled Tyrant's squad plugin.
///
/// Adds the engine's `SquadPlugin` (which registers components,
/// resources, events, and systems-in-sets) and then configures the
/// system set ordering so the squad systems run at the right points
/// relative to this game's FOV and combat pipelines.
pub struct SquadPlugin;

impl Plugin for SquadPlugin {
    fn build(&self, app: &mut App) {
        // 1. Install the engine's squad framework.
        app.add_plugins(roguelike_engine::squad::SquadPlugin)
            // 2. Configure the engine's SystemSets with game-side ordering.
            //    Alert propagation runs after FOV updates and only in-game.
            .configure_sets(
                Update,
                SquadAlertSet
                    .after(crate::game::systems::fov_update_system)
                    .run_if(in_state(crate::game::InGameState::Running)),
            )
            //    Reaction systems run after damage lands, before death.
            .configure_sets(
                Update,
                SquadReactionSet
                    .after(CombatDamageSet)
                    .before(crate::game::combat::death_system)
                    .run_if(in_state(crate::game::InGameState::Running)),
            )
            // 3. Bridge game-side state to the engine.
            .add_systems(
                Update,
                sync_squad_target
                    .run_if(in_state(crate::game::InGameState::Running)),
            );
    }
}

/// Publishes the player's current position into the engine's
/// `SquadTarget` resource so the engine's squad systems can read it
/// without directly querying for the player.
fn sync_squad_target(
    player_query: Query<&crate::components::Position, With<Player>>,
    mut target: ResMut<SquadTarget>,
) {
    target.position = player_query.single().ok().map(|p| p.to_point());
}
