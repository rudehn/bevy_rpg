//! Stealth subsystem — awareness state, noise map, probability helper.
//!
//! Engine-side ships pure types + state-tick systems. The game crate
//! ships modifier composition and the per-turn opposed-roll system
//! (`perception_tick_system`). See bevy_rpg's stealth-system-design.md.

pub mod awareness;
pub mod noise;
pub mod probability;

pub use awareness::{Awareness, AwarenessAlertEvent, AwarenessRecord, AwarenessState};
pub use noise::{noise_decay_system, noise_modifier, NoiseMap};
pub use probability::notice_probability;
