//! Character system: race, class, and attribute data for the player.
//!
//! See [`docs/design/CHARACTER.md`](../../docs/design/CHARACTER.md) for the
//! full design. Phase 2 introduces XP/levels, a race-driven HP formula,
//! and the DCSS-style stat-gain schedule. The XP/level systems
//! themselves live in [`crate::game::xp`]; this module owns the
//! attribute, race, and class types and the pure compose/derive helpers.

#![allow(dead_code, unused_imports)]

mod asset;
mod attributes;
mod class;
mod race;

pub use asset::{
    ClassAsset, ClassManifest, ClassManifestHandle, RaceAsset, RaceManifest, RaceManifestHandle,
    SkillAptitudes, SkillDistribution,
};
pub use attributes::{
    ability_mod, attack_attribute_bonus, compose_attributes, derive_stats, max_hp_for_level,
    AttributeDistribution, Attributes, CharacterChoice, DerivedStats,
};
pub use class::{Attribute, Class};
pub use race::{Race, RaceGainSchedule, RaceTrait};

use bevy::prelude::*;

/// Registers character-related component reflection and inserts the
/// `CharacterChoice` resource.
pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Race>()
            .register_type::<Class>()
            .register_type::<Attributes>()
            .insert_resource(CharacterChoice::default());
    }
}
