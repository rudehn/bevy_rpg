//! Pure pathfinding helpers wrapping bracket-lib's A* search.
//!
//! These functions take a [`Map`] (and optionally a [`MovementMode`]) and
//! return the next step or full path from one point to another. They are
//! pure — no ECS, no Bevy — so they're easy to test and compose in any
//! AI loop.

use bracket_lib::prelude::{a_star_search, Point};

use crate::components::MovementMode;
use crate::map::map::{Map, MapWithMode};

/// Compute the next step from `from` toward `to` using A* pathfinding.
///
/// Returns `Some(Point)` with the first step toward the target, or `None`
/// if no path exists (target unreachable or already at target). Uses the
/// default `Map` pathfinding costs (no movement mode awareness).
///
/// This wraps bracket-lib's `a_star_search` and returns only the first
/// step since AI typically recomputes each turn anyway (the map changes).
pub fn next_step_toward(map: &Map, from: Point, to: Point) -> Option<Point> {
    if from == to {
        return None;
    }
    if !map.in_bounds(from) || !map.in_bounds(to) {
        return None;
    }
    let start = map.xy_idx(from.x, from.y);
    let end = map.xy_idx(to.x, to.y);
    let path = a_star_search(start, end, map);
    if path.success && path.steps.len() > 1 {
        let (nx, ny) = map.idx_xy(path.steps[1]);
        Some(Point::new(nx, ny))
    } else {
        None
    }
}

/// Compute the next step using movement-mode-aware pathfinding.
///
/// Like [`next_step_toward`] but respects the entity's [`MovementMode`]:
/// - `Land` avoids deep water
/// - `ImmuneToWater` can cross water
/// - `RestrictedToLiquid` can only move through liquid tiles
pub fn next_step_toward_with_mode(
    map: &Map,
    from: Point,
    to: Point,
    mode: MovementMode,
) -> Option<Point> {
    if from == to {
        return None;
    }
    if !map.in_bounds(from) || !map.in_bounds(to) {
        return None;
    }
    let map_with_mode = MapWithMode { map, mode };
    let start = map.xy_idx(from.x, from.y);
    let end = map.xy_idx(to.x, to.y);
    let path = a_star_search(start, end, &map_with_mode);
    if path.success && path.steps.len() > 1 {
        let (nx, ny) = map.idx_xy(path.steps[1]);
        Some(Point::new(nx, ny))
    } else {
        None
    }
}

/// Compute the full path from `from` to `to`.
///
/// Returns the complete path as a Vec of Points (including the starting
/// position), or an empty Vec if no path exists. Useful for debugging
/// or for multi-step planning.
pub fn find_path(map: &Map, from: Point, to: Point) -> Vec<Point> {
    if !map.in_bounds(from) || !map.in_bounds(to) {
        return Vec::new();
    }
    let start = map.xy_idx(from.x, from.y);
    let end = map.xy_idx(to.x, to.y);
    let path = a_star_search(start, end, map);
    if path.success {
        path.steps
            .iter()
            .map(|&idx| {
                let (x, y) = map.idx_xy(idx);
                Point::new(x, y)
            })
            .collect()
    } else {
        Vec::new()
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

    fn floor() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }
    fn wall() -> Tile {
        Tile {
            terrain: TerrainType::Wall,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }
    fn deep_water() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::Water,
            decoration: Decoration::None,
        }
    }

    fn make_map(width: i32, height: i32, tiles: Vec<Tile>) -> Map {
        let count = (width * height) as usize;
        assert_eq!(tiles.len(), count);
        Map {
            name: "test".to_string(),
            tiles,
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth: 1,
        }
    }

    /// Create an open map with floor interior and wall border.
    fn open_map(w: i32, h: i32) -> Map {
        let mut tiles = vec![floor(); (w * h) as usize];
        for x in 0..w {
            tiles[(0 * w + x) as usize] = wall();
            tiles[((h - 1) * w + x) as usize] = wall();
        }
        for y in 0..h {
            tiles[(y * w) as usize] = wall();
            tiles[(y * w + (w - 1)) as usize] = wall();
        }
        make_map(w, h, tiles)
    }

    // ---- next_step_toward ----

    #[test]
    fn next_step_direct_path() {
        let map = open_map(10, 10);
        let step = next_step_toward(&map, Point::new(1, 1), Point::new(3, 1));
        assert_eq!(step, Some(Point::new(2, 1)));
    }

    #[test]
    fn next_step_around_obstacle() {
        let mut map = open_map(10, 10);
        // Place a wall at (2,1) to force a detour
        let idx = map.xy_idx(2, 1);
        map.tiles[idx] = wall();
        let step = next_step_toward(&map, Point::new(1, 1), Point::new(3, 1));
        assert!(step.is_some());
        let s = step.unwrap();
        // Must not step into the wall
        assert_ne!(s, Point::new(2, 1));
    }

    #[test]
    fn next_step_no_path() {
        let mut map = open_map(10, 10);
        // Surround (5,5) with walls
        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let idx = map.xy_idx(5 + dx, 5 + dy);
                map.tiles[idx] = wall();
            }
        }
        let step = next_step_toward(&map, Point::new(1, 1), Point::new(5, 5));
        assert_eq!(step, None);
    }

    #[test]
    fn next_step_already_at_target() {
        let map = open_map(10, 10);
        let step = next_step_toward(&map, Point::new(3, 3), Point::new(3, 3));
        assert_eq!(step, None);
    }

    #[test]
    fn next_step_out_of_bounds() {
        let map = open_map(10, 10);
        assert_eq!(
            next_step_toward(&map, Point::new(-1, 0), Point::new(3, 3)),
            None
        );
        assert_eq!(
            next_step_toward(&map, Point::new(1, 1), Point::new(20, 20)),
            None
        );
    }

    // ---- next_step_toward_with_mode ----

    #[test]
    fn next_step_with_mode_avoids_water() {
        // 5-wide corridor with water in the middle row
        // Layout (5x5, walled border):
        //   W W W W W
        //   W . . . W
        //   W . ~ . W    ~ = deep water at (2,2)
        //   W . . . W
        //   W W W W W
        let mut map = open_map(5, 5);
        let idx = map.xy_idx(2, 2);
        map.tiles[idx] = deep_water();

        // Land mode should avoid the water tile
        let step = next_step_toward_with_mode(
            &map,
            Point::new(1, 2),
            Point::new(3, 2),
            MovementMode::Land,
        );
        assert!(step.is_some());
        let s = step.unwrap();
        // Should not step into water
        assert_ne!(s, Point::new(2, 2));
    }

    #[test]
    fn next_step_with_mode_crosses_water() {
        let mut map = open_map(5, 5);
        let idx = map.xy_idx(2, 2);
        map.tiles[idx] = deep_water();

        // ImmuneToWater can cross directly through water
        let step = next_step_toward_with_mode(
            &map,
            Point::new(1, 2),
            Point::new(3, 2),
            MovementMode::ImmuneToWater,
        );
        assert_eq!(step, Some(Point::new(2, 2)));
    }

    // ---- find_path ----

    #[test]
    fn find_path_returns_full_path() {
        let map = open_map(10, 10);
        let path = find_path(&map, Point::new(1, 1), Point::new(3, 1));
        assert!(!path.is_empty());
        // Path should start at (1,1) and end at (3,1)
        assert_eq!(*path.first().unwrap(), Point::new(1, 1));
        assert_eq!(*path.last().unwrap(), Point::new(3, 1));
        // Direct horizontal path: 3 steps (1,1) -> (2,1) -> (3,1)
        assert_eq!(path.len(), 3);
    }

    #[test]
    fn find_path_empty_on_failure() {
        let mut map = open_map(10, 10);
        // Surround (5,5) with walls
        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let idx = map.xy_idx(5 + dx, 5 + dy);
                map.tiles[idx] = wall();
            }
        }
        let path = find_path(&map, Point::new(1, 1), Point::new(5, 5));
        assert!(path.is_empty());
    }
}
