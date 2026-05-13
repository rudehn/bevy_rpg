use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Maximum value of the armor roll. When a physical hit lands, the
/// damage pipeline rolls a random integer in `[0, Armor.0 + skill]`
/// inclusive and subtracts that from the raw damage (clamped at 0).
/// A value of 10 thus reduces incoming damage by 0..=10 randomly —
/// not a flat 10.
///
/// Non-physical damage (Poison / Fire / Lightning) skips the armor
/// roll entirely; resistance percentages handle those.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Armor(pub i32);

/// Flat dodge chance (0-100), used in hit check.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Dodge(pub i32);

/// Flat damage reduction from a shield. Unlike `Armor`, Block is
/// **flat (not rolled)** and applies to **all damage types** —
/// physical, fire, lightning, poison. Block is consumed before the
/// armor roll so it shrinks raw damage at the front of the pipeline.
/// Shield items' `defense` field flows here via `compute_stat_delta`
/// (any item in the `OffHand` slot routes its defense to Block).
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Block(pub i32);

/// Flat bonus added to the d20 attack roll.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct HitBonus(pub i32);

/// Flat damage added after dice roll.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct DamageBonus(pub i32);

// --- Plugin ---

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Armor>()
            .register_type::<Dodge>()
            .register_type::<Block>()
            .register_type::<HitBonus>()
            .register_type::<DamageBonus>();
    }
}
