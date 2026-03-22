use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::game::combat::{DamageType, Resistances};
use crate::game::items::Rarity;
use crate::game::magic::{ActiveSpells, MAX_SPELL_SLOTS};

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

// =====================================================================
// Active Shrine resource — set when player bumps a shrine
// =====================================================================

#[derive(Resource)]
pub struct ActiveShrine(pub Entity);

// =====================================================================
// Shrine effect marker components
// =====================================================================

/// First attack against each new enemy deals bonus damage.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct FirstStrikeAbility;

/// Heal on kill.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct BloodlustAbility;

/// Melee attacks hit adjacent enemies around the target.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleaveAbility;

/// Survive a killing blow once per floor.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct SecondWindAbility {
    pub available: bool,
}

impl Default for SecondWindAbility {
    fn default() -> Self {
        Self { available: true }
    }
}

/// Bonus mana regeneration per turn.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManaWellAbility;

/// Spell casting costs half the normal action time.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuickCastAbility;

/// Spells cost HP instead of mana.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct BloodMageAbility;

/// Reroll low attack rolls.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct LuckyAbility;

/// Chests drop higher rarity items.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScavengerAbility;

/// Crits deal triple damage; misses cost double action time.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct GamblersMarkAbility;

// =====================================================================
// Apply shrine effect
// =====================================================================

/// Applies the given shrine effect to the player entity.
/// Returns true if the effect was successfully applied.
pub fn apply_shrine_effect(
    commands: &mut Commands,
    player: Entity,
    kind: &ShrineEffectKind,
    active_spells: Option<&mut ActiveSpells>,
    resistances: Option<&mut Resistances>,
) -> bool {
    match kind {
        ShrineEffectKind::FirstStrike => {
            commands.entity(player).insert(FirstStrikeAbility);
        }
        ShrineEffectKind::Bloodlust => {
            commands.entity(player).insert(BloodlustAbility);
        }
        ShrineEffectKind::Cleave => {
            commands.entity(player).insert(CleaveAbility);
        }
        ShrineEffectKind::SecondWind => {
            commands.entity(player).insert(SecondWindAbility { available: true });
        }
        ShrineEffectKind::SpellSlot => {
            if let Some(active) = active_spells {
                if active.slots.len() < MAX_SPELL_SLOTS {
                    active.slots.push(None);
                }
            }
        }
        ShrineEffectKind::ManaWell => {
            commands.entity(player).insert(ManaWellAbility);
        }
        ShrineEffectKind::QuickCast => {
            commands.entity(player).insert(QuickCastAbility);
        }
        ShrineEffectKind::BloodMage => {
            commands.entity(player).insert(BloodMageAbility);
        }
        ShrineEffectKind::Lucky => {
            commands.entity(player).insert(LuckyAbility);
        }
        ShrineEffectKind::Scavenger => {
            commands.entity(player).insert(ScavengerAbility);
        }
        ShrineEffectKind::FireImmunity => {
            if let Some(res) = resistances {
                res.0.insert(DamageType::Fire, 100);
            }
        }
        ShrineEffectKind::GamblersMark => {
            commands.entity(player).insert(GamblersMarkAbility);
        }
    }
    true
}

pub struct ShrinesPlugin;

impl Plugin for ShrinesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShrinesPurchased>();
    }
}
