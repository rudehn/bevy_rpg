//! Tile data types and helpers for a layered grid-based map.
//!
//! Each map cell is a [`Tile`] with three layers:
//!
//! - [`TerrainType`] — the physical structure (wall, floor, door family,
//!   stairs, portal). Drives walkability, opacity, and door state.
//! - [`LiquidType`] — an overlay (water, lava, chasm). Modifies
//!   walkability and pathfinding cost.
//! - [`Decoration`] — a visual/interactable overlay (grass, rubble, moss,
//!   cobweb, etc.). May affect FOV, movement cost, and promotions.
//!
//! All three enums are `#[non_exhaustive]` with a `Custom { id: u32 }`
//! escape hatch so games can add new terrain/liquid/decoration types
//! without editing the engine.

use crate::components::MovementMode;
use serde::{Deserialize, Serialize};

// =====================================================================
// TerrainType
// =====================================================================

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, bevy::ecs::component::Component, Serialize, Deserialize)]
pub enum TerrainType {
    #[default]
    Wall,
    Floor,
    DownStairs,
    UpStairs,
    Empty,
    Door,
    OpenDoor,
    /// Renders as Wall until discovered, then converts to Door.
    HiddenDoor,
    /// Requires a matching key item to open. Renders as a locked door.
    LockedDoor,
    /// Escape portal on the final floor. Walkable, non-opaque.
    Portal,
    /// Game-defined custom terrain. Metadata via a game-side registry.
    Custom { id: u32 },
}

impl TerrainType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Wall => "Wall",
            Self::Floor => "Floor",
            Self::DownStairs => "DownStairs",
            Self::UpStairs => "UpStairs",
            Self::Empty => "Empty",
            Self::Door => "Door",
            Self::OpenDoor => "OpenDoor",
            Self::HiddenDoor => "HiddenDoor",
            Self::LockedDoor => "LockedDoor",
            Self::Portal => "Portal",
            Self::Custom { .. } => "Custom",
        }
    }

    /// Chance (0-100) of igniting when exposed to adjacent fire.
    pub fn flammability(&self) -> u8 {
        match self {
            Self::Door | Self::OpenDoor => 20,
            _ => 0,
        }
    }

    /// What this terrain becomes when stepped on. None = no step promotion.
    pub fn on_step_promotion(&self) -> Option<PromotionTarget> {
        None
    }

    /// Timed promotion rule. None = no passive change.
    pub fn timed_promotion(&self) -> Option<PromotionRule> {
        match self {
            // Open doors close automatically next turn (Brogue: 10000/10000 = 100%).
            Self::OpenDoor => Some(PromotionRule {
                target: PromotionTarget::Terrain(TerrainType::Door),
                chance_per_turn: 10000,
            }),
            _ => None,
        }
    }
}

// =====================================================================
// LiquidType
// =====================================================================

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, bevy::ecs::component::Component, Serialize, Deserialize)]
pub enum LiquidType {
    #[default]
    None,
    Water,
    ShallowWater,
    Lava,
    /// Impassable void — no wreath, blocks everything.
    Chasm,
    /// Game-defined custom liquid.
    Custom { id: u32 },
}

impl LiquidType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Water => "Water",
            Self::ShallowWater => "ShallowWater",
            Self::Lava => "Lava",
            Self::Chasm => "Chasm",
            Self::Custom { .. } => "Custom",
        }
    }
}

// =====================================================================
// Decoration
// =====================================================================

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Decoration {
    #[default]
    None,
    Grass,
    TallGrass,
    /// Naturally placed dead vegetation. Does NOT regrow.
    DeadGrass,
    Rubble,
    Moss,
    Fungus,
    Cobweb,
    Bloodstain,
    /// TallGrass that was trampled. Regrows into TallGrass over time.
    TrampledGrass,
    /// Fungus that was trampled. Regrows into Fungus over time.
    TrampledFungus,
    /// Sputtering remains of a fire. Decays to Ash.
    Embers,
    /// Burned-out remains. Purely visual.
    Ash,
    /// Floor cracked by an explosion. Collapses into a chasm after a few turns.
    CrackedFloor,
    /// Bioluminescent moss. Emits a soft cyan-green glow — `apply_decoration_mutations`
    /// registers a `phosphorescent_moss_light` source on this tile. The light
    /// intensity raises the stealth penalty for anyone standing in/near the
    /// patch via the game-side stealth pipeline.
    PhosphorescentMoss,
    /// Game-defined custom decoration.
    Custom { id: u32 },
}

impl Decoration {
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Grass => "Grass",
            Self::TallGrass => "TallGrass",
            Self::DeadGrass => "DeadGrass",
            Self::Rubble => "Rubble",
            Self::Moss => "Moss",
            Self::Fungus => "Fungus",
            Self::Cobweb => "Cobweb",
            Self::Bloodstain => "Bloodstain",
            Self::TrampledGrass => "TrumpledGrass",
            Self::TrampledFungus => "TrumpledFungus",
            Self::Embers => "Embers",
            Self::Ash => "Ash",
            Self::CrackedFloor => "CrackedFloor",
            Self::PhosphorescentMoss => "PhosphorescentMoss",
            Self::Custom { .. } => "Custom",
        }
    }

    /// Chance (0-100) of igniting when exposed to adjacent fire.
    pub fn flammability(&self) -> u8 {
        match self {
            Self::TallGrass => 75,
            Self::Grass => 50,
            Self::DeadGrass => 60,
            Self::Fungus => 40,
            Self::Cobweb => 100,
            Self::Moss => 30,
            // Phosphorescent moss is a damp fungal mat — burns, but reluctantly.
            // Burning it triggers the engine's decoration-mutation handler,
            // which then removes the registered light source.
            Self::PhosphorescentMoss => 20,
            Self::TrampledGrass => 40,
            Self::TrampledFungus => 30,
            _ => 0,
        }
    }

    /// Movement cost multiplier. Values > 1.0 slow movement.
    pub fn movement_cost(&self) -> f32 {
        match self {
            _ => 1.0,
        }
    }

    /// Whether this decoration blocks line of sight.
    pub fn blocks_fov(&self) -> bool {
        matches!(self, Self::TallGrass | Self::Fungus)
    }

    /// What this decoration becomes when stepped on. None = no step promotion.
    pub fn on_step_promotion(&self) -> Option<PromotionTarget> {
        match self {
            Self::TallGrass => Some(PromotionTarget::Decoration(Decoration::TrampledGrass)),
            Self::Fungus => Some(PromotionTarget::Decoration(Decoration::TrampledFungus)),
            _ => std::option::Option::None,
        }
    }

    /// Timed promotion rule. None = no passive change.
    pub fn timed_promotion(&self) -> Option<PromotionRule> {
        match self {
            Self::TrampledGrass => Some(PromotionRule {
                target: PromotionTarget::Decoration(Decoration::TallGrass),
                chance_per_turn: 100,
            }),
            Self::TrampledFungus => Some(PromotionRule {
                target: PromotionTarget::Decoration(Decoration::Fungus),
                chance_per_turn: 100,
            }),
            Self::Embers => Some(PromotionRule {
                target: PromotionTarget::Decoration(Decoration::Ash),
                chance_per_turn: 1000,
            }),
            Self::CrackedFloor => Some(PromotionRule {
                target: PromotionTarget::Liquid(LiquidType::Chasm),
                chance_per_turn: 3300,
            }),
            _ => std::option::Option::None,
        }
    }

    /// Whether stepping on this decoration entangles the creature.
    pub fn entangles(&self) -> bool {
        matches!(self, Self::Cobweb)
    }
}

// =====================================================================
// Tile
// =====================================================================

/// A single map cell with three layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainType,
    pub liquid: LiquidType,
    pub decoration: Decoration,
}

// =====================================================================
// Tile promotion system (Brogue-aligned)
// =====================================================================

/// What a tile promotes into. Can target any of the three layers.
#[derive(Debug, Clone, Copy)]
pub enum PromotionTarget {
    Decoration(Decoration),
    Terrain(TerrainType),
    Liquid(LiquidType),
}

/// A timed promotion rule: what a tile becomes and at what rate.
#[derive(Debug, Clone, Copy)]
pub struct PromotionRule {
    pub target: PromotionTarget,
    /// Chance per turn out of 10000 (Brogue scale). 10000 = 100%, 100 = 1%.
    pub chance_per_turn: u16,
}

// =====================================================================
// Helper functions
// =====================================================================

/// Returns true if the tile is walkable (both terrain AND liquid allow entry).
///
/// Doesn't account for movement modes — use [`can_entity_enter_tile`]
/// for mode-aware checks.
pub fn is_walkable(tile: Tile) -> bool {
    let terrain_walkable = match tile.terrain {
        TerrainType::Wall => false,
        TerrainType::Floor => true,
        TerrainType::DownStairs => true,
        TerrainType::UpStairs => true,
        TerrainType::Empty => false,
        TerrainType::Door => false,
        TerrainType::OpenDoor => true,
        TerrainType::HiddenDoor => false,
        TerrainType::LockedDoor => false,
        TerrainType::Portal => true,
        TerrainType::Custom { .. } => false, // conservative default
    };

    let liquid_walkable = match tile.liquid {
        LiquidType::None => true,
        LiquidType::Water => true,
        LiquidType::ShallowWater => true,
        LiquidType::Lava => false,
        LiquidType::Chasm => false,
        LiquidType::Custom { .. } => false, // conservative default
    };

    terrain_walkable && liquid_walkable
}

/// Mode-aware walkability check.
pub fn can_entity_enter_tile(tile: Tile, mode: MovementMode) -> bool {
    match mode {
        MovementMode::Land | MovementMode::ImmuneToWater => is_walkable(tile),
        MovementMode::RestrictedToLiquid => {
            tile.liquid != LiquidType::None && is_walkable(tile)
        }
        _ => is_walkable(tile),
    }
}

/// Topological passability for connectivity checks (flood-fill, ChokeMap).
///
/// More permissive than [`is_walkable`]: doors and locked doors are
/// passable so connectivity analysis doesn't reject maps where these
/// are the only link between regions.
pub fn is_passable(tile: Tile) -> bool {
    match tile.terrain {
        TerrainType::Wall => false,
        TerrainType::Empty => false,
        TerrainType::Custom { .. } => false,
        _ => true,
    }
}

/// Brogue's T_LAKE_PATHING_BLOCKER / T_PATHING_BLOCKER concept.
///
/// These tiles are physically walkable but AI and level design should
/// avoid them. Deep water, lava, and chasm are pathing blockers.
pub fn is_pathing_blocker(tile: Tile) -> bool {
    match tile.liquid {
        LiquidType::Water => true,
        LiquidType::Lava => true,
        LiquidType::Chasm => true,
        _ => false,
    }
}

/// Returns true if this tile blocks line of sight.
pub fn is_opaque(tile: Tile) -> bool {
    let terrain_opaque = matches!(
        tile.terrain,
        TerrainType::Wall | TerrainType::Door | TerrainType::HiddenDoor | TerrainType::LockedDoor
    );
    terrain_opaque || tile.decoration.blocks_fov()
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(terrain: TerrainType, liquid: LiquidType) -> Tile {
        Tile {
            terrain,
            liquid,
            decoration: Decoration::None,
        }
    }

    // ---- is_walkable ----

    #[test]
    fn floor_no_liquid_is_walkable() {
        assert!(is_walkable(tile(TerrainType::Floor, LiquidType::None)));
    }

    #[test]
    fn wall_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Wall, LiquidType::None)));
    }

    #[test]
    fn floor_with_deep_water_is_walkable() {
        assert!(is_walkable(tile(TerrainType::Floor, LiquidType::Water)));
    }

    #[test]
    fn floor_with_shallow_water_is_walkable() {
        assert!(is_walkable(tile(TerrainType::Floor, LiquidType::ShallowWater)));
    }

    #[test]
    fn floor_with_lava_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Floor, LiquidType::Lava)));
    }

    #[test]
    fn floor_with_chasm_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Floor, LiquidType::Chasm)));
    }

    #[test]
    fn door_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Door, LiquidType::None)));
    }

    #[test]
    fn open_door_is_walkable() {
        assert!(is_walkable(tile(TerrainType::OpenDoor, LiquidType::None)));
    }

    #[test]
    fn stairs_are_walkable() {
        assert!(is_walkable(tile(TerrainType::DownStairs, LiquidType::None)));
        assert!(is_walkable(tile(TerrainType::UpStairs, LiquidType::None)));
    }

    // ---- can_entity_enter_tile ----

    #[test]
    fn land_mode_enters_floor() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::None),
            MovementMode::Land,
        ));
    }

    #[test]
    fn land_mode_enters_deep_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Water),
            MovementMode::Land,
        ));
    }

    #[test]
    fn land_mode_blocked_by_wall() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Wall, LiquidType::None),
            MovementMode::Land,
        ));
    }

    #[test]
    fn land_mode_blocked_by_lava() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Lava),
            MovementMode::Land,
        ));
    }

    #[test]
    fn immune_to_water_enters_deep_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Water),
            MovementMode::ImmuneToWater,
        ));
    }

    #[test]
    fn immune_to_water_enters_floor() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::None),
            MovementMode::ImmuneToWater,
        ));
    }

    #[test]
    fn restricted_to_liquid_enters_deep_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Water),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_enters_shallow_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::ShallowWater),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_blocked_by_dry_floor() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::None),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_blocked_by_wall_with_water() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Wall, LiquidType::Water),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_blocked_by_lava() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Lava),
            MovementMode::RestrictedToLiquid,
        ));
    }

    // ---- is_pathing_blocker ----

    #[test]
    fn deep_water_is_pathing_blocker() {
        assert!(is_pathing_blocker(tile(TerrainType::Floor, LiquidType::Water)));
    }

    #[test]
    fn lava_is_pathing_blocker() {
        assert!(is_pathing_blocker(tile(TerrainType::Floor, LiquidType::Lava)));
    }

    #[test]
    fn chasm_is_pathing_blocker() {
        assert!(is_pathing_blocker(tile(TerrainType::Floor, LiquidType::Chasm)));
    }

    #[test]
    fn shallow_water_is_not_pathing_blocker() {
        assert!(!is_pathing_blocker(tile(TerrainType::Floor, LiquidType::ShallowWater)));
    }

    #[test]
    fn dry_floor_is_not_pathing_blocker() {
        assert!(!is_pathing_blocker(tile(TerrainType::Floor, LiquidType::None)));
    }

    // ---- is_opaque ----

    #[test]
    fn wall_is_opaque() {
        assert!(is_opaque(tile(TerrainType::Wall, LiquidType::None)));
    }

    #[test]
    fn floor_is_not_opaque() {
        assert!(!is_opaque(tile(TerrainType::Floor, LiquidType::None)));
    }

    #[test]
    fn deep_water_is_not_opaque() {
        assert!(!is_opaque(tile(TerrainType::Floor, LiquidType::Water)));
    }

    #[test]
    fn closed_door_is_opaque() {
        assert!(is_opaque(tile(TerrainType::Door, LiquidType::None)));
    }

    // ---- CrackedFloor timed promotion ----

    #[test]
    fn cracked_floor_promotes_to_chasm() {
        let rule = Decoration::CrackedFloor
            .timed_promotion()
            .expect("CrackedFloor should have a timed promotion");
        assert!(
            matches!(rule.target, PromotionTarget::Liquid(LiquidType::Chasm)),
            "CrackedFloor should promote to Chasm"
        );
        assert_eq!(rule.chance_per_turn, 3300);
    }

    // ---- Custom variant conservative defaults ----

    #[test]
    fn custom_terrain_not_walkable_by_default() {
        assert!(!is_walkable(tile(TerrainType::Custom { id: 99 }, LiquidType::None)));
    }

    #[test]
    fn custom_liquid_not_walkable_by_default() {
        assert!(!is_walkable(tile(TerrainType::Floor, LiquidType::Custom { id: 1 })));
    }

    #[test]
    fn custom_terrain_not_passable_by_default() {
        assert!(!is_passable(tile(TerrainType::Custom { id: 1 }, LiquidType::None)));
    }

    // ---- Decoration::PhosphorescentMoss metadata ----

    #[test]
    fn phosphorescent_moss_has_correct_name() {
        assert_eq!(Decoration::PhosphorescentMoss.name(), "PhosphorescentMoss");
    }

    #[test]
    fn phosphorescent_moss_is_low_flammability() {
        // 20% — damp glowing mat; burns, but reluctantly.
        assert_eq!(Decoration::PhosphorescentMoss.flammability(), 20);
    }

    #[test]
    fn phosphorescent_moss_does_not_block_fov() {
        assert!(!Decoration::PhosphorescentMoss.blocks_fov());
    }

    #[test]
    fn phosphorescent_moss_has_no_step_or_timed_promotion() {
        assert!(Decoration::PhosphorescentMoss.on_step_promotion().is_none());
        assert!(Decoration::PhosphorescentMoss.timed_promotion().is_none());
    }

    #[test]
    fn phosphorescent_moss_does_not_entangle() {
        assert!(!Decoration::PhosphorescentMoss.entangles());
    }
}
