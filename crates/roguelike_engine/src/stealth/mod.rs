//! Stealth subsystem — awareness state, noise map, probability helper.
//!
//! Engine-side ships pure types + state-tick systems. The game crate
//! ships modifier composition and the per-turn opposed-roll system
//! (`perception_tick_system`). See bevy_rpg's stealth-system-design.md.

pub mod awareness;
pub mod noise;
pub mod probability;

pub use awareness::{
    awareness_tick_system, tick_awareness, Awareness, AwarenessAlertEvent, AwarenessRecord,
    AwarenessState,
};
pub use noise::{noise_decay_system, noise_modifier, NoiseMap};
pub use probability::notice_probability;

use bevy::prelude::*;

/// Engine-side plugin: registers the `AwarenessAlertEvent` message.
/// Per-turn opposed-roll lives in the game crate's `StealthPlugin`.
/// Ordering against the game's `ProcessingPhase` is the game crate's
/// responsibility — `awareness_tick_system` and `noise_decay_system`
/// are exported as free functions for the game to schedule.
pub struct StealthPlugin;

impl Plugin for StealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AwarenessAlertEvent>();
    }
}
