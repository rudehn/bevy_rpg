use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// --- Kept Components ---

/// Mana pool.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct Mana {
    pub current: i32,
    pub max: i32,
}

// --- New Components ---

/// Flat armor (damage reduction).
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Armor(pub i32);

/// Flat dodge chance (0-100), used in hit check.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Dodge(pub i32);

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
        app.register_type::<Mana>()
            .register_type::<Armor>()
            .register_type::<Dodge>()
            .register_type::<HitBonus>()
            .register_type::<DamageBonus>();
    }
}
