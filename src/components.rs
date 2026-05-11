use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::prelude::{Reflect, ReflectComponent};

// Foundation components now live in the engine crate. Re-exported here
// so existing game code (hundreds of `use crate::components::Position`
// style imports) continues to work unchanged.
pub use roguelike_engine::components::{
    Collider, Faction, FactionKind, Inventory, Name, Position, Viewshed,
};

#[allow(dead_code)]
#[derive(Component, Debug, Clone)]
pub struct BlocksVisibility;

#[allow(dead_code)]
#[derive(Component, Debug, Clone)]
pub struct Hidden;

#[derive(Component)]
pub struct Monster;

// `Name` is re-exported from `roguelike_engine::components` above.

#[derive(Component)]
pub struct Item;

#[derive(Component)]
pub struct GameEntityMarker;

#[derive(Component)]
pub struct FloorEntityMarker;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct GodMode;

// `Inventory` is re-exported from `roguelike_engine::components` above.

/// Marker component for items currently held in an inventory (not on the floor).
/// Items with this component are invisible and excluded from floor-level queries.
#[derive(Component, Debug, Default)]
pub struct InInventory;

/// Marker for items consumed on use (potions, scrolls). Items without this
/// (staves, equipment) stay in inventory after being used.
#[derive(Component, Debug, Default)]
pub struct Consumable;

/// Marker component for items that are currently equipped by the player.
/// Equipped items remain in Inventory.items and the UI shows them with [E].
#[derive(Component, Debug, Default)]
pub struct Equipped;

/// Marker component for ammunition items (arrows, bolts, etc.).
/// Used by the ranged attack system to find consumable ammo in inventory.
#[derive(Component, Debug, Default)]
pub struct Ammo;

/// Marker for quest items required to win the game (e.g., Amulet of Yendor).
/// The escape portal checks the player's inventory for this component.
#[derive(Component, Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuestItem;

/// Marker component for items drifting in deep water currents.
/// Items with this component move 1 tile per turn toward shore.
#[derive(Component, Debug, Default)]
pub struct Drifting;

// `MovementMode` now lives in the engine crate. Re-exported so every
// `use crate::components::MovementMode` call site (41 occurrences across
// 5 files) continues to work unchanged.
pub use roguelike_engine::components::MovementMode;

/// Marker for aquatic monsters currently hiding beneath the water surface.
/// While submerged the monster is invisible and cannot be targeted by ranged
/// attacks or staff zaps. The AI removes this component before attacking.
#[derive(Component, Debug, Default)]
pub struct Submerged;

/// A key that opens a specific locked door.
#[derive(Component, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub key_name: String,
}

/// Marker entity placed at a LockedDoor tile position to store which key opens it.
#[derive(Component, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockedDoorData {
    pub key_name: String,
}

/// Marker component for props — non-item, non-monster world entities
/// (watchfires, totem poles, barricades, etc.).
#[derive(Component, Debug, Default)]
pub struct Prop;

/// Stores the manifest key for a prop (e.g., "watchfire", "candle").
/// Used to correctly restore props from cache/save, since the display
/// `Name` (e.g., "Watchfire") differs from the manifest lookup key.
#[derive(Component, Debug, Clone)]
pub struct PropKey(pub String);

/// Marker component for destructible props (e.g., barricades).
/// Entities with both `Destructible` and `Health` can be attacked via bump.
#[derive(Component, Debug, Default)]
pub struct Destructible;

/// Marker component for chest props. When bumped by the player, the chest
/// despawns and spawns level-appropriate items at its position.
#[derive(Component, Debug, Default)]
pub struct Chest;

// --- Faction ---

// `Faction` and `FactionKind` now live in `roguelike_engine::components`.
// Re-exported at the top of this file. The Veiled Tyrant's specific
// faction roster (Player, Monster, Kobold, Rat) is defined below as
// constants + constructors on the `VeiledTyrantFactions` helper.

/// Game-specific faction constants and constructors for The Veiled Tyrant.
///
/// The engine's `FactionKind` is a generic `String` newtype — it ships
/// no specific faction names. This helper centralizes the faction
/// identifiers this game uses, so both the player spawn code and any
/// future game-side faction-aware logic have one place to look.
pub struct VeiledTyrantFactions;

impl VeiledTyrantFactions {
    pub const PLAYER: &'static str = "Player";
    pub const MONSTER: &'static str = "Monster";
    pub const KOBOLD: &'static str = "Kobold";
    pub const RAT: &'static str = "Rat";

    pub fn player() -> FactionKind {
        FactionKind::new(Self::PLAYER)
    }
    pub fn monster() -> FactionKind {
        FactionKind::new(Self::MONSTER)
    }
    pub fn kobold() -> FactionKind {
        FactionKind::new(Self::KOBOLD)
    }
}

/// Tracks which entity summoned this creature. Used by the summon cap system.
#[derive(Component, Clone, Debug)]
pub struct SummonedBy {
    pub summoner: Entity,
}

/// Biological category of a monster. Orthogonal to `Faction` (which is political
/// alignment) — species is biology. Consumed by future systems like bane weapons,
/// ecology effects, and UI labels; see `docs/design/ENEMIES.md` for the canonical list.
#[derive(Component, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Species {
    Beast,
    Humanoid,
    Undead,
    Insect,
    Fungal,
    Ooze,
    Dragon,
    Construct,
    Aberration,
    #[default]
    Unknown,
}

impl std::fmt::Display for Species {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Species::Beast => "Beast",
            Species::Humanoid => "Humanoid",
            Species::Undead => "Undead",
            Species::Insect => "Insect",
            Species::Fungal => "Fungal",
            Species::Ooze => "Ooze",
            Species::Dragon => "Dragon",
            Species::Construct => "Construct",
            Species::Aberration => "Aberration",
            Species::Unknown => "Unknown",
        })
    }
}
