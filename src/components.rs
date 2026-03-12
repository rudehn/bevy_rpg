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
    pub fn to_point(&self) -> Point {
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

#[derive(Component, Debug, Clone)]
pub struct BlocksVisibility;

#[derive(Component, Debug, Clone)]
pub struct Hidden;

#[derive(Component)]
pub struct Monster;

#[derive(Component)]
pub struct Name(pub String);

#[derive(Component)]
pub struct Item;

#[derive(Component)]
pub struct AmuletOfBevy;

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
