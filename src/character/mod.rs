//! Character system: race, class, and attribute data for the player.
//!
//! See [`docs/design/CHARACTER.md`](../../docs/design/CHARACTER.md) for the
//! full design. This commit (Phase 1, step 1) introduces the data types and
//! asset loaders only — runtime integration with combat math and the
//! character creation UI come in subsequent commits. The module-level
//! `dead_code` allow covers fields/methods that exist for future consumers;
//! tests already exercise the deserialization paths.

#![allow(dead_code, unused_imports)]

mod asset;
mod attributes;
mod class;
mod race;

pub use asset::{
    ClassAsset, ClassManifest, ClassManifestHandle, RaceAsset, RaceManifest, RaceManifestHandle,
};
pub use attributes::{
    ability_mod, compose_attributes, derive_stats, Attributes, CharacterChoice, DerivedStats,
};
pub use class::{Attribute, Class};
pub use race::{Race, RaceTrait};

use bevy::prelude::*;

/// Registers character-related component reflection. Asset loading itself is
/// wired in `crate::assets::LoadingPlugin` so the loading-state transition
/// keeps a single source of truth.
pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Race>()
            .register_type::<Class>()
            .register_type::<Attributes>();
    }
}
