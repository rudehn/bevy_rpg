//! Field-of-view component.

use std::collections::HashSet;

use bevy::ecs::component::Component;
use bracket_lib::prelude::Point;

/// Per-entity field-of-view cache.
///
/// `visible_tiles` is populated each turn by the engine's FOV system
/// (typically via bracket-lib's `field_of_view_set`). `range` is the
/// entity's vision radius in tiles. `dirty` is set whenever the entity
/// moves or the map's opacity changes, so the FOV system knows which
/// viewsheds need recomputing.
///
/// Games attach this component to the player and to any monster that
/// needs to track what it can see. Monsters without a `Viewshed` are
/// treated as blind (they act only on direct adjacency or shared
/// squad alerts).
#[derive(Component, Clone, Default)]
pub struct Viewshed {
    /// Tiles currently visible to this entity.
    pub visible_tiles: HashSet<Point>,
    /// Vision range in tiles (Manhattan distance).
    pub range: i32,
    /// True when `visible_tiles` needs recomputation.
    pub dirty: bool,
}

impl Viewshed {
    /// Build a fresh viewshed with the given vision range. `dirty`
    /// defaults to `true` so the FOV system computes visibility the
    /// first time it runs.
    pub fn new(range: i32) -> Self {
        Self {
            visible_tiles: HashSet::new(),
            range,
            dirty: true,
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_dirty() {
        let v = Viewshed::new(8);
        assert!(v.dirty);
        assert_eq!(v.range, 8);
        assert!(v.visible_tiles.is_empty());
    }

    #[test]
    fn default_is_not_dirty() {
        // `Default` is used by some save/load paths that then mark dirty
        // explicitly — so the default value must be consistent with
        // "not yet computed but not forced to recompute".
        let v = Viewshed::default();
        assert!(!v.dirty);
        assert_eq!(v.range, 0);
        assert!(v.visible_tiles.is_empty());
    }

    #[test]
    fn visible_tiles_round_trip() {
        let mut v = Viewshed::new(4);
        v.visible_tiles.insert(Point::new(1, 1));
        v.visible_tiles.insert(Point::new(2, 3));
        assert!(v.visible_tiles.contains(&Point::new(1, 1)));
        assert!(!v.visible_tiles.contains(&Point::new(5, 5)));
        assert_eq!(v.visible_tiles.len(), 2);
    }
}
