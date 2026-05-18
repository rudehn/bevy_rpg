//! Pure grid geometry helpers.
//!
//! Distance metrics (Manhattan / Chebyshev), adjacency tests, cursor
//! clamping, area-of-effect tile enumeration, and an 8-way
//! [`Direction`](direction::Direction) enum. All functions take plain
//! `i32` coordinates and return plain values — no ECS state, no game
//! types, no allocations beyond a single `Vec<(i32, i32)>` for AoE
//! queries.

pub mod direction;
pub use direction::Direction;

/// Manhattan distance between two grid positions.
///
/// `|dx| + |dy|`. Use this for movement costs on a 4-connected grid
/// and for targeting ranges that don't allow diagonal compression.
pub fn manhattan_distance(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs() + (ay - by).abs()
}

/// Chebyshev (chessboard-king) distance between two grid positions.
///
/// `max(|dx|, |dy|)`. Use this for 8-connected movement and for
/// areas-of-effect where diagonal neighbours count as distance 1.
pub fn chebyshev_distance(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs().max((ay - by).abs())
}

/// Returns `true` when the two positions are adjacent (Chebyshev
/// distance of exactly 1) — orthogonal or diagonal neighbours.
pub fn is_adjacent(ax: i32, ay: i32, bx: i32, by: i32) -> bool {
    chebyshev_distance(ax, ay, bx, by) == 1
}

/// Clamp a cursor position so it stays inside a map of size
/// `map_width` × `map_height`. Out-of-range coordinates are clamped
/// to the nearest valid tile.
pub fn clamp_cursor(x: i32, y: i32, map_width: i32, map_height: i32) -> (i32, i32) {
    (x.clamp(0, map_width - 1), y.clamp(0, map_height - 1))
}

/// Collect every tile within `radius` (Chebyshev) of a centre point.
///
/// A radius of 0 returns only the centre tile.  A radius of 1 gives
/// the 3×3 area (9 tiles); radius `r` yields `(2r+1)^2` tiles.
pub fn tiles_in_aoe(center_x: i32, center_y: i32, radius: i32) -> Vec<(i32, i32)> {
    let mut tiles = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            tiles.push((center_x + dx, center_y + dy));
        }
    }
    tiles
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================
    // Manhattan distance
    // ========================

    #[test]
    fn manhattan_same_tile() {
        assert_eq!(manhattan_distance(5, 5, 5, 5), 0);
    }

    #[test]
    fn manhattan_horizontal() {
        assert_eq!(manhattan_distance(0, 0, 7, 0), 7);
    }

    #[test]
    fn manhattan_vertical() {
        assert_eq!(manhattan_distance(0, 0, 0, 4), 4);
    }

    #[test]
    fn manhattan_diagonal() {
        // (0,0) to (3,4) = 3+4 = 7
        assert_eq!(manhattan_distance(0, 0, 3, 4), 7);
    }

    #[test]
    fn manhattan_symmetric() {
        assert_eq!(
            manhattan_distance(2, 3, 7, 1),
            manhattan_distance(7, 1, 2, 3),
        );
    }

    #[test]
    fn manhattan_negative_coords() {
        assert_eq!(manhattan_distance(-3, -2, 4, 5), 14);
    }

    // ========================
    // Chebyshev distance
    // ========================

    #[test]
    fn chebyshev_same_tile() {
        assert_eq!(chebyshev_distance(5, 5, 5, 5), 0);
    }

    #[test]
    fn chebyshev_horizontal() {
        assert_eq!(chebyshev_distance(0, 0, 7, 0), 7);
    }

    #[test]
    fn chebyshev_vertical() {
        assert_eq!(chebyshev_distance(0, 0, 0, 4), 4);
    }

    #[test]
    fn chebyshev_diagonal() {
        // Chebyshev: max(|3|, |3|) = 3
        assert_eq!(chebyshev_distance(0, 0, 3, 3), 3);
    }

    #[test]
    fn chebyshev_asymmetric_offsets() {
        // max(|5|, |2|) = 5
        assert_eq!(chebyshev_distance(0, 0, 5, 2), 5);
    }

    #[test]
    fn chebyshev_symmetric() {
        assert_eq!(
            chebyshev_distance(1, 2, 8, 6),
            chebyshev_distance(8, 6, 1, 2),
        );
    }

    // ========================
    // Adjacency
    // ========================

    #[test]
    fn adjacent_orthogonal() {
        assert!(is_adjacent(5, 5, 6, 5)); // right
        assert!(is_adjacent(5, 5, 4, 5)); // left
        assert!(is_adjacent(5, 5, 5, 6)); // up
        assert!(is_adjacent(5, 5, 5, 4)); // down
    }

    #[test]
    fn adjacent_diagonal() {
        assert!(is_adjacent(5, 5, 6, 6));
        assert!(is_adjacent(5, 5, 4, 4));
        assert!(is_adjacent(5, 5, 6, 4));
        assert!(is_adjacent(5, 5, 4, 6));
    }

    #[test]
    fn not_adjacent_same_tile() {
        assert!(!is_adjacent(5, 5, 5, 5));
    }

    #[test]
    fn not_adjacent_two_away() {
        assert!(!is_adjacent(5, 5, 7, 5));
        assert!(!is_adjacent(5, 5, 5, 7));
        assert!(!is_adjacent(5, 5, 7, 7));
    }

    // ========================
    // Cursor bounds clamping
    // ========================

    #[test]
    fn clamp_cursor_inside_map() {
        assert_eq!(clamp_cursor(40, 30, 80, 60), (40, 30));
    }

    #[test]
    fn clamp_cursor_negative_x() {
        assert_eq!(clamp_cursor(-1, 30, 80, 60), (0, 30));
    }

    #[test]
    fn clamp_cursor_negative_y() {
        assert_eq!(clamp_cursor(10, -5, 80, 60), (10, 0));
    }

    #[test]
    fn clamp_cursor_over_width() {
        assert_eq!(clamp_cursor(80, 30, 80, 60), (79, 30));
    }

    #[test]
    fn clamp_cursor_over_height() {
        assert_eq!(clamp_cursor(10, 60, 80, 60), (10, 59));
    }

    #[test]
    fn clamp_cursor_both_negative() {
        assert_eq!(clamp_cursor(-3, -7, 80, 60), (0, 0));
    }

    #[test]
    fn clamp_cursor_both_over() {
        assert_eq!(clamp_cursor(100, 100, 80, 60), (79, 59));
    }

    #[test]
    fn clamp_cursor_origin_corner() {
        assert_eq!(clamp_cursor(0, 0, 80, 60), (0, 0));
    }

    #[test]
    fn clamp_cursor_max_corner() {
        assert_eq!(clamp_cursor(79, 59, 80, 60), (79, 59));
    }

    // ========================
    // AoE radius tiles
    // ========================

    #[test]
    fn aoe_radius_zero_single_tile() {
        let tiles = tiles_in_aoe(10, 10, 0);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0], (10, 10));
    }

    #[test]
    fn aoe_radius_one_gives_3x3() {
        let tiles = tiles_in_aoe(5, 5, 1);
        assert_eq!(tiles.len(), 9);
        // Check all 9 tiles present
        for dy in -1..=1 {
            for dx in -1..=1 {
                assert!(
                    tiles.contains(&(5 + dx, 5 + dy)),
                    "Missing tile ({}, {})",
                    5 + dx,
                    5 + dy
                );
            }
        }
    }

    #[test]
    fn aoe_radius_two_gives_5x5() {
        let tiles = tiles_in_aoe(10, 10, 2);
        assert_eq!(tiles.len(), 25);
    }

    #[test]
    fn aoe_tiles_outside_radius_excluded() {
        let tiles = tiles_in_aoe(5, 5, 1);
        // (5+2, 5) = (7, 5) should NOT be in the 3x3
        assert!(!tiles.contains(&(7, 5)));
        assert!(!tiles.contains(&(3, 5)));
        assert!(!tiles.contains(&(5, 3)));
    }

    #[test]
    fn aoe_center_always_included() {
        for r in 0..=5 {
            let tiles = tiles_in_aoe(20, 30, r);
            assert!(tiles.contains(&(20, 30)));
        }
    }

    #[test]
    fn aoe_tile_count_formula() {
        // A Chebyshev radius r produces (2r+1)^2 tiles.
        for r in 0..=4 {
            let tiles = tiles_in_aoe(0, 0, r);
            let expected = ((2 * r + 1) * (2 * r + 1)) as usize;
            assert_eq!(tiles.len(), expected, "radius {}", r);
        }
    }

    // ========================
    // Distance metrics agree on axis-aligned
    // ========================

    #[test]
    fn manhattan_equals_chebyshev_on_axis() {
        // Along a single axis, Manhattan == Chebyshev.
        for d in 0..=10 {
            assert_eq!(
                manhattan_distance(0, 0, d, 0),
                chebyshev_distance(0, 0, d, 0)
            );
            assert_eq!(
                manhattan_distance(0, 0, 0, d),
                chebyshev_distance(0, 0, 0, d)
            );
        }
    }

    #[test]
    fn manhattan_ge_chebyshev_always() {
        // Manhattan is always >= Chebyshev.
        let cases = [(0, 0, 3, 4), (1, 2, 5, 8), (10, 10, 13, 14)];
        for (ax, ay, bx, by) in cases {
            assert!(manhattan_distance(ax, ay, bx, by) >= chebyshev_distance(ax, ay, bx, by));
        }
    }
}
