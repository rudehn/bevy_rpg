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

/// Shield "SH" value — the additive bonus to a per-attack block check.
/// When a hit lands and Block > 0, the defender rolls
/// `d20 + floor(Shields_skill/4) + Block` against DC 17. On pass, the
/// hit's damage is **fully negated** (zeroed) for every damage type.
/// On fail, no reduction applies. Shield items' `defense` field flows
/// here via `compute_stat_delta` (any item in the `OffHand` slot routes
/// its defense to Block). The number of blocks an entity can attempt
/// per turn is bounded by [`MaxShieldBlocks`].
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Block(pub i32);

/// Cap on shield block attempts per turn. Set from the equipped
/// shield's `max_blocks` field (1 buckler / 2 kite / 3 tower).
/// Reset is automatic — counters live in [`ShieldBlocksUsed`].
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct MaxShieldBlocks(pub u32);

/// Counter of shield blocks consumed since the entity's last turn end.
/// Only **successful** blocks decrement the budget; failed checks
/// still let the entity try again next swing. Reset to 0 on
/// `TurnEndEvent` for the matching entity.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ShieldBlocksUsed(pub u32);

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
            .register_type::<MaxShieldBlocks>()
            .register_type::<ShieldBlocksUsed>()
            .register_type::<HitBonus>()
            .register_type::<DamageBonus>();
    }
}
