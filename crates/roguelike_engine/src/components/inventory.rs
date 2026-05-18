//! Inventory component: an entity's list of carried items.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;

/// Holds entity IDs of all items currently carried by this entity.
///
/// The engine is agnostic about item types — `items` is a list of
/// `Entity` handles and the game's item system owns the actual
/// components. `capacity` is a soft limit the game can use to decide
/// whether pickup is allowed; the engine itself does not enforce it.
///
/// Games typically pair this with their own marker components
/// (`InInventory`, `Equipped`, etc.) on the item entities themselves.
#[derive(Component, Debug, Default)]
pub struct Inventory {
    /// Entities currently in the inventory (in whatever order the game
    /// chooses to display them).
    pub items: Vec<Entity>,
    /// Soft capacity limit. Zero means "no limit enforced by game code".
    pub capacity: usize,
}
