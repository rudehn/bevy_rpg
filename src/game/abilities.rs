use bevy::prelude::*;

// --- Faction ---

/// Determines how this entity relates to others for AI targeting and spell scoring.
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub struct Faction(pub FactionKind);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FactionKind {
    Player,
    Monster,
}

impl FactionKind {
    /// Returns true if `other` is a valid hostile target for `self`.
    pub fn is_hostile_to(&self, other: &FactionKind) -> bool {
        self != other
    }

    /// Returns true if `other` is on the same side as `self`.
    pub fn is_allied_to(&self, other: &FactionKind) -> bool {
        self == other
    }
}

// --- Passive Ability Components ---
// Each passive ability is its own component. Monsters that have it just carry the component;
// dedicated systems react to game events and trigger the effect automatically.
// Abilities have no mana cost, no cooldown, and are never "cast."

/// Inflicts poison stacks on any entity that physically attacks this one.
#[derive(Component, Debug, Clone)]
pub struct PoisonBody {
    /// How many stacks of poison the attacker receives per hit.
    pub stacks: i32,
}

/// Deals area damage to nearby entities when this entity dies.
#[derive(Component, Debug, Clone)]
pub struct ExplodeOnDeath {
    /// Tile radius of the explosion.
    pub radius: i32,
    /// Flat damage dealt to each entity in range.
    pub damage: i32,
}

/// Can revive itself once after reaching 0 HP.
#[derive(Component, Debug, Clone)]
pub struct Reanimate {
    /// HP the entity revives with.
    pub revive_hp: i32,
}

// --- Plugin ---

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, _app: &mut App) {
        // Ability trigger systems will be added here as passive abilities are implemented.
        // Each ability type has its own system listening on the relevant game event.
    }
}
