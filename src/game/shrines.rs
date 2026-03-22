use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::game::items::Rarity;

/// What a shrine effect does when purchased.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShrineEffectKind {
    // War effects
    FirstStrike,
    Bloodlust,
    Cleave,
    SecondWind,

    // Arcane effects
    SpellSlot,
    ManaWell,
    QuickCast,
    BloodMage,

    // Fortune effects
    Lucky,
    Scavenger,
    FireImmunity,
    GamblersMark,
}

/// Tracks which unique effects the player has purchased this run.
#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
pub struct ShrinesPurchased(pub Vec<String>);

/// Marks a shrine entity in the world.
#[derive(Component, Debug)]
pub struct ShrineMarker;

/// Stores the shrine's category and rolled effects.
#[derive(Component, Debug, Clone)]
pub struct ShrineData {
    pub category_id: String,
    pub category_name: String,
    pub effects: Vec<ShrineEffectInstance>,
}

#[derive(Debug, Clone)]
pub struct ShrineEffectInstance {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rarity: Rarity,
    pub cost: i32,
    pub kind: ShrineEffectKind,
    pub unique: bool,
}

pub struct ShrinesPlugin;

impl Plugin for ShrinesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShrinesPurchased>();
    }
}
