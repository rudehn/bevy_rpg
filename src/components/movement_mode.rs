//! Movement mode component: how an entity interacts with terrain.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Determines how an entity interacts with terrain for movement and pathfinding.
///
/// The engine's pathfinding and movement-cost systems consult this component
/// to decide which tiles an entity can enter. Games attach it to monster
/// and player entities based on their type — aquatic monsters use
/// `RestrictedToLiquid`, amphibians use `ImmuneToWater`, everything else
/// uses the default `Land`.
///
/// The enum is `#[non_exhaustive]` so the engine can add new movement
/// categories (e.g., flying, phasing) in patch releases without breaking
/// games that match on the enum. Games using `match` on this type MUST
/// provide a `_ =>` fallback arm.
#[non_exhaustive]
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum MovementMode {
    /// Normal ground movement; shallow water passable but deep water
    /// penalized, lava impassable.
    #[default]
    Land,
    /// Ignores water penalties and item-displacement drift. Amphibians,
    /// water elementals, aquatic humanoids.
    ImmuneToWater,
    /// Can ONLY move on liquid tiles (shallow or deep water). Eels,
    /// kraken, anything that cannot leave the water.
    RestrictedToLiquid,
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_land() {
        assert_eq!(MovementMode::default(), MovementMode::Land);
    }

    #[test]
    fn modes_are_distinct() {
        assert_ne!(MovementMode::Land, MovementMode::ImmuneToWater);
        assert_ne!(MovementMode::Land, MovementMode::RestrictedToLiquid);
        assert_ne!(MovementMode::ImmuneToWater, MovementMode::RestrictedToLiquid);
    }

    #[test]
    fn mode_is_copy() {
        // Copy should be cheap; this test exists mainly to pin the trait
        // so removing Copy requires an explicit decision.
        let a = MovementMode::RestrictedToLiquid;
        let b = a;
        let _c = a; // still usable after the copy
        assert_eq!(a, b);
    }
}
