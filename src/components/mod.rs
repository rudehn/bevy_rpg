//! Shared ECS components used by the engine's systems.
//!
//! These are the generic, content-free components the engine ships for
//! games to attach to their entities. Games are free to define additional
//! components of their own; these are the ones the engine's own systems
//! (pathfinding, movement rules, turn scheduling, etc.) look for.

mod collider;
mod faction;
mod inventory;
mod movement_mode;
mod name;
mod patrol_route;
mod position;
mod viewshed;

pub use collider::Collider;
pub use faction::{Faction, FactionKind};
pub use inventory::Inventory;
pub use movement_mode::MovementMode;
pub use name::Name;
pub use patrol_route::{PatrolRoute, PatrolState};
pub use position::Position;
pub use viewshed::Viewshed;
