//! Collider marker component.

use bevy::ecs::component::Component;

/// Marks an entity as occupying its tile for pathfinding purposes.
///
/// The engine's [`crate::map::map::populate_blocked_tiles`] system
/// scans for entities with both [`Collider`] and
/// [`crate::components::Position`] and marks their tile as blocked in
/// the [`crate::map::Map`] resource. Monster AI pathfinding treats
/// blocked tiles as high-cost rather than impassable so monsters route
/// around each other instead of lining up in a corridor.
#[derive(Component)]
pub struct Collider;
