//! Cardinal and intercardinal direction enum.

use bracket_lib::prelude::Point;

use crate::components::Position;

/// An 8-way direction plus a "no direction" sentinel.
///
/// Used for bump-movement, door-site resolution, AI flee/kite
/// direction selection, and anything else that needs to describe a
/// step on the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    /// No movement. Used when the origin and target are the same tile.
    NoDirection,
}

impl Direction {
    /// All 8 cardinal + intercardinal directions (no `NoDirection`).
    pub const ALL: [Self; 8] = [
        Self::N,
        Self::NE,
        Self::E,
        Self::SE,
        Self::S,
        Self::SW,
        Self::W,
        Self::NW,
    ];

    /// Determine the direction from `current` to `target`.
    ///
    /// Returns `NoDirection` if both positions are the same tile.
    pub fn from_pos(current: &Position, target: &Position) -> Self {
        match target.x.cmp(&current.x) {
            std::cmp::Ordering::Less => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::SW,
                std::cmp::Ordering::Equal => Direction::W,
                std::cmp::Ordering::Greater => Direction::NW,
            },
            std::cmp::Ordering::Equal => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::S,
                std::cmp::Ordering::Equal => Direction::NoDirection,
                std::cmp::Ordering::Greater => Direction::N,
            },
            std::cmp::Ordering::Greater => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::SE,
                std::cmp::Ordering::Equal => Direction::E,
                std::cmp::Ordering::Greater => Direction::NE,
            },
        }
    }

    /// The `(dx, dy)` offset for one step in this direction.
    pub fn offset(&self) -> Point {
        match self {
            Direction::NW => Point { x: -1, y: 1 },
            Direction::N => Point { x: 0, y: 1 },
            Direction::NE => Point { x: 1, y: 1 },
            Direction::E => Point { x: 1, y: 0 },
            Direction::SE => Point { x: 1, y: -1 },
            Direction::S => Point { x: 0, y: -1 },
            Direction::SW => Point { x: -1, y: -1 },
            Direction::W => Point { x: -1, y: 0 },
            Direction::NoDirection => Point { x: 0, y: 0 },
        }
    }

    /// The two cardinal directions perpendicular to this one.
    ///
    /// For intercardinal directions (NE, NW, SE, SW) and `NoDirection`,
    /// returns `(NoDirection, NoDirection)` since perpendicularity is
    /// ambiguous. Games can refine this behavior if they need it.
    pub fn perpendiculars(&self) -> (Direction, Direction) {
        match self {
            Direction::N | Direction::S => (Direction::W, Direction::E),
            Direction::E | Direction::W => (Direction::N, Direction::S),
            _ => (Direction::NoDirection, Direction::NoDirection),
        }
    }

    /// The direction 180 degrees opposite to this one.
    pub fn opposite(&self) -> Self {
        match self {
            Direction::NW => Direction::SE,
            Direction::N => Direction::S,
            Direction::NE => Direction::SW,
            Direction::E => Direction::W,
            Direction::SE => Direction::NW,
            Direction::S => Direction::N,
            Direction::SW => Direction::NE,
            Direction::W => Direction::E,
            Direction::NoDirection => Direction::NoDirection,
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
    fn all_has_8_directions() {
        assert_eq!(Direction::ALL.len(), 8);
        assert!(!Direction::ALL.contains(&Direction::NoDirection));
    }

    #[test]
    fn offsets_are_unit_steps() {
        for dir in Direction::ALL {
            let p = dir.offset();
            assert!(p.x.abs() <= 1 && p.y.abs() <= 1);
            assert!(p.x != 0 || p.y != 0, "ALL should not contain zero offset");
        }
    }

    #[test]
    fn no_direction_offset_is_zero() {
        let p = Direction::NoDirection.offset();
        assert_eq!(p.x, 0);
        assert_eq!(p.y, 0);
    }

    #[test]
    fn from_pos_same_tile_is_no_direction() {
        let p = Position { x: 5, y: 5 };
        assert_eq!(Direction::from_pos(&p, &p), Direction::NoDirection);
    }

    #[test]
    fn from_pos_cardinals() {
        let origin = Position { x: 5, y: 5 };
        assert_eq!(Direction::from_pos(&origin, &Position { x: 5, y: 6 }), Direction::N);
        assert_eq!(Direction::from_pos(&origin, &Position { x: 5, y: 4 }), Direction::S);
        assert_eq!(Direction::from_pos(&origin, &Position { x: 6, y: 5 }), Direction::E);
        assert_eq!(Direction::from_pos(&origin, &Position { x: 4, y: 5 }), Direction::W);
    }

    #[test]
    fn from_pos_diagonals() {
        let origin = Position { x: 5, y: 5 };
        assert_eq!(Direction::from_pos(&origin, &Position { x: 6, y: 6 }), Direction::NE);
        assert_eq!(Direction::from_pos(&origin, &Position { x: 4, y: 6 }), Direction::NW);
        assert_eq!(Direction::from_pos(&origin, &Position { x: 6, y: 4 }), Direction::SE);
        assert_eq!(Direction::from_pos(&origin, &Position { x: 4, y: 4 }), Direction::SW);
    }

    #[test]
    fn opposite_is_symmetric() {
        for dir in Direction::ALL {
            assert_eq!(dir.opposite().opposite(), dir);
        }
        assert_eq!(Direction::NoDirection.opposite(), Direction::NoDirection);
    }

    #[test]
    fn perpendiculars_of_cardinals_are_cardinals() {
        let (l, r) = Direction::N.perpendiculars();
        assert_eq!(l, Direction::W);
        assert_eq!(r, Direction::E);

        let (l, r) = Direction::E.perpendiculars();
        assert_eq!(l, Direction::N);
        assert_eq!(r, Direction::S);
    }
}
