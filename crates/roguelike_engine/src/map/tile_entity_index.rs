//! Spatial index from grid `(x, y)` to tile [`Entity`].
//!
//! Built once per floor by the game's tile spawner; queried by mutation
//! systems to find the ECS tile entity that mirrors a given map cell.

use std::collections::HashMap;

use bevy::prelude::*;

/// Maps grid coordinates to the tile [`Entity`] holding that cell's
/// renderable components (terrain/liquid components, sprite, ASCII glyph).
///
/// Populated by the game when materializing a floor; the engine only
/// reads it. Kept as a public field so games can rebuild it however they
/// like (typically once per floor load).
#[derive(Resource, Default)]
pub struct TileEntityIndex(pub HashMap<(i32, i32), Entity>);
