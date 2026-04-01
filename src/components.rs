use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::prelude::{Reflect, ReflectComponent};
use bracket_lib::prelude::Point;

#[derive(Component)]
pub struct Collider;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn to_point(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn from_point(point: Point) -> Self {
        Position {
            x: point.x,
            y: point.y,
        }
    }
}

#[derive(Component, Clone, Default)]
pub struct Viewshed {
    pub visible_tiles: Vec<Point>,
    pub range: i32,
    pub dirty: bool,
}

impl Viewshed {
    pub fn new(range: i32) -> Self {
        Self {
            visible_tiles: Vec::new(),
            range,
            dirty: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Component, Debug, Clone)]
pub struct BlocksVisibility;

#[allow(dead_code)]
#[derive(Component, Debug, Clone)]
pub struct Hidden;

#[derive(Component)]
pub struct Monster;

#[derive(Component)]
pub struct Name(pub String);

#[derive(Component)]
pub struct Item;

#[derive(Component)]
pub struct GameEntityMarker;

#[derive(Component)]
pub struct FloorEntityMarker;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct GodMode;

/// Holds entity IDs of all items currently in the player's inventory.
#[derive(Component, Debug, Default)]
pub struct Inventory {
    pub items: Vec<Entity>,
    pub capacity: usize,
}

/// Marker component for items currently held in an inventory (not on the floor).
/// Items with this component are invisible and excluded from floor-level queries.
#[derive(Component, Debug, Default)]
pub struct InInventory;

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

/// Determines how an entity interacts with terrain for movement and pathfinding.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MovementMode {
    #[default]
    Land,              // Normal movement, deep water penalized
    ImmuneToWater,     // Ignores water penalties, no item displacement
    RestrictedToLiquid, // Can ONLY move on liquid tiles (eels, kraken)
}

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

/// Determines how this entity relates to others for AI targeting and spell scoring.
/// Hostility is resolved via the `FactionMatrix` resource, not by comparing kinds directly.
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub struct Faction(pub FactionKind);

/// String-based faction identifier. Hostility between factions is determined
/// by the `FactionMatrix` resource loaded from `factions.ron`.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct FactionKind(pub String);

impl FactionKind {
    pub const PLAYER: &str = "Player";
    pub const MONSTER: &str = "Monster";
    pub const KOBOLD: &str = "Kobold";

    pub fn player() -> Self { Self(Self::PLAYER.to_string()) }
    pub fn monster() -> Self { Self(Self::MONSTER.to_string()) }
    pub fn kobold() -> Self { Self(Self::KOBOLD.to_string()) }
}
