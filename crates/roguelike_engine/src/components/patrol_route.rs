//! Patrol route component for monsters that guard territory.
//!
//! Attached to a monster at spawn time, `PatrolRoute` makes an idle
//! monster follow a predetermined pattern: hold position near a home
//! tile, walk a circuit of waypoints, or wander within a bounding box.
//! The absence of this component means the monster wanders freely.
//!
//! This module ships only the data types. Actual patrol execution
//! (reading the current waypoint, advancing to the next one, returning
//! home after a chase) lives in the game crate's AI loop because it
//! depends on game-side systems like pathfinding and `MonsterAI` mode
//! transitions.

use bevy::prelude::Component;
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

/// Patrol behavior attached to monsters at spawn time.
///
/// Coordinates are stored as `(i32, i32)` tuples rather than
/// `bracket_lib::prelude::Point` because the forked `bracket-lib` in
/// this workspace doesn't derive `Serialize`/`Deserialize` on `Point`.
/// The constructor methods accept `Point` for ergonomic callers.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct PatrolRoute {
    pub state: PatrolState,
}

/// The kinds of patrol a `PatrolRoute` can describe.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatrolState {
    /// Hold position. The AI may jitter within a small radius around
    /// `home`, but it won't wander away and will return here after a
    /// hunt ends.
    Sentry { home: (i32, i32) },
    /// Walk a list of waypoints in order, looping back to the first
    /// after reaching the last.
    Waypoint {
        points: Vec<(i32, i32)>,
        current_index: usize,
    },
    /// Random walk constrained to an axis-aligned bounding rectangle.
    AreaRoam { min: (i32, i32), max: (i32, i32) },
}

impl PatrolState {
    /// Build a sentry patrol that holds a single home tile.
    pub fn sentry(home: Point) -> Self {
        PatrolState::Sentry { home: (home.x, home.y) }
    }

    /// Build a waypoint patrol that loops through the given points.
    pub fn waypoint(points: &[Point]) -> Self {
        PatrolState::Waypoint {
            points: points.iter().map(|p| (p.x, p.y)).collect(),
            current_index: 0,
        }
    }

    /// Build an area-roam patrol bounded by the `min..=max` rectangle
    /// (inclusive on both corners).
    pub fn area_roam(min: Point, max: Point) -> Self {
        PatrolState::AreaRoam {
            min: (min.x, min.y),
            max: (max.x, max.y),
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
    fn sentry_stores_home_coords() {
        let s = PatrolState::sentry(Point::new(5, 7));
        match s {
            PatrolState::Sentry { home } => assert_eq!(home, (5, 7)),
            _ => panic!("expected Sentry"),
        }
    }

    #[test]
    fn waypoint_starts_at_index_zero() {
        let points = [Point::new(1, 1), Point::new(5, 1), Point::new(5, 5)];
        let s = PatrolState::waypoint(&points);
        match s {
            PatrolState::Waypoint {
                points: stored,
                current_index,
            } => {
                assert_eq!(current_index, 0);
                assert_eq!(stored, vec![(1, 1), (5, 1), (5, 5)]);
            }
            _ => panic!("expected Waypoint"),
        }
    }

    #[test]
    fn waypoint_preserves_order() {
        let points = [Point::new(10, 10), Point::new(3, 3)];
        let s = PatrolState::waypoint(&points);
        match s {
            PatrolState::Waypoint { points: stored, .. } => {
                assert_eq!(stored[0], (10, 10));
                assert_eq!(stored[1], (3, 3));
            }
            _ => panic!("expected Waypoint"),
        }
    }

    #[test]
    fn area_roam_stores_bounds() {
        let s = PatrolState::area_roam(Point::new(2, 3), Point::new(8, 9));
        match s {
            PatrolState::AreaRoam { min, max } => {
                assert_eq!(min, (2, 3));
                assert_eq!(max, (8, 9));
            }
            _ => panic!("expected AreaRoam"),
        }
    }

    #[test]
    fn patrol_route_wraps_state() {
        let route = PatrolRoute {
            state: PatrolState::sentry(Point::new(0, 0)),
        };
        assert!(matches!(route.state, PatrolState::Sentry { .. }));
    }
}
