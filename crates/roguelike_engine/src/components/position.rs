//! Grid position component.

use bevy::ecs::component::Component;
use bracket_lib::prelude::Point;

/// An entity's integer grid position.
///
/// The engine's pathfinding, FOV, and spatial queries all read from this
/// component. `Position` is stored as two `i32` fields rather than a
/// `bracket_lib::prelude::Point` because that type doesn't derive the
/// full set of Bevy component traits in this workspace's fork; the
/// [`to_point`](Position::to_point) and [`from_point`](Position::from_point)
/// helpers let you cross the boundary when calling bracket-lib APIs.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    /// Convert to a bracket-lib `Point`.
    pub fn to_point(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Build from a bracket-lib `Point`.
    pub fn from_point(point: Point) -> Self {
        Position {
            x: point.x,
            y: point.y,
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
    fn position_roundtrips_through_point() {
        let p = Position { x: 3, y: -7 };
        let round = Position::from_point(p.to_point());
        assert_eq!(p, round);
    }

    #[test]
    fn position_from_point_matches_fields() {
        let pt = Point::new(11, 22);
        let p = Position::from_point(pt);
        assert_eq!(p.x, 11);
        assert_eq!(p.y, 22);
    }

    #[test]
    fn position_equality_is_by_coord() {
        assert_eq!(Position { x: 1, y: 2 }, Position { x: 1, y: 2 });
        assert_ne!(Position { x: 1, y: 2 }, Position { x: 2, y: 1 });
    }
}
