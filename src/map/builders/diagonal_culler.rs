//! Brogue's removeDiagonalOpenings: removes diagonal-only passages where two
//! passable tiles are diagonally adjacent with blocking tiles on both shared edges.
//! These create passages the player can't traverse with cardinal movement (WASD)
//! but could squeeze through diagonally. One of the blocking tiles is converted
//! to match its passable neighbor, eliminating the diagonal-only gap.

use super::{BuilderMap, MetaMapBuilder};
use crate::map::tile::TerrainType;
use rand::prelude::*;

pub struct DiagonalCuller;

impl DiagonalCuller {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for DiagonalCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let width = build_data.map.width;
        let height = build_data.map.height;
        let mut rng = rand::rng();

        // Brogue: do { } while (diagonalCornerRemoved)
        let mut changed = true;
        while changed {
            changed = false;

            for y in 0..height - 1 {
                for x in 0..width - 1 {
                    // Check both diagonal orientations (k=0: top-left/bottom-right, k=1: top-right/bottom-left)
                    for k in 0..=1i32 {
                        let ax = x + k;
                        let bx = x + (1 - k);

                        // a = (ax, y), b = (bx, y), c = (ax, y+1), d = (bx, y+1)
                        // Pattern: a is passable, b is blocking, c is blocking, d is passable
                        // This creates a diagonal-only passage from a to d.
                        let idx_a = build_data.map.xy_idx(ax, y);
                        let idx_b = build_data.map.xy_idx(bx, y);
                        let idx_c = build_data.map.xy_idx(ax, y + 1);
                        let idx_d = build_data.map.xy_idx(bx, y + 1);

                        let is_blocking = |idx: usize| {
                            let t = build_data.map.tiles[idx].terrain;
                            matches!(t, TerrainType::Wall | TerrainType::Empty)
                        };
                        let is_open = |idx: usize| !is_blocking(idx);

                        if is_open(idx_a) && is_blocking(idx_b) && is_blocking(idx_c) && is_open(idx_d) {
                            // Randomly pick which blocking tile to convert (b or c)
                            // Brogue: copies ALL layers from the passable neighbor
                            let (target_idx, source_idx) = if rng.random_bool(0.5) {
                                (idx_b, idx_a) // convert b to match a
                            } else {
                                (idx_c, idx_d) // convert c to match d
                            };

                            build_data.map.tiles[target_idx] = build_data.map.tiles[source_idx];
                            changed = true;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::TerrainType;

    /// Creates a diagonal-only passage: two floor tiles at (1,1) and (2,2)
    /// with walls at (2,1) and (1,2). The culler should resolve this by
    /// converting one of the blocking tiles to floor.
    fn idx(bm: &BuilderMap, x: i32, y: i32) -> usize {
        bm.map.xy_idx(x, y)
    }

    #[test]
    fn resolves_diagonal_passage() {
        let mut bm = BuilderMap::new_for_test(4, 4);
        let i11 = idx(&bm, 1, 1);
        let i22 = idx(&bm, 2, 2);
        let i21 = idx(&bm, 2, 1);
        let i12 = idx(&bm, 1, 2);

        bm.map.tiles[i11].terrain = TerrainType::Floor;
        bm.map.tiles[i22].terrain = TerrainType::Floor;
        assert_eq!(bm.map.tiles[i21].terrain, TerrainType::Wall);
        assert_eq!(bm.map.tiles[i12].terrain, TerrainType::Wall);

        DiagonalCuller.build_map(&mut bm);

        let b = bm.map.tiles[i21].terrain;
        let c = bm.map.tiles[i12].terrain;
        assert!(
            b == TerrainType::Floor || c == TerrainType::Floor,
            "DiagonalCuller should resolve diagonal-only passage"
        );
    }

    #[test]
    fn no_change_when_no_diagonal() {
        let mut bm = BuilderMap::new_for_test(4, 4);
        let i11 = idx(&bm, 1, 1);
        let i21 = idx(&bm, 2, 1);
        bm.map.tiles[i11].terrain = TerrainType::Floor;
        bm.map.tiles[i21].terrain = TerrainType::Floor;

        let before: Vec<_> = bm.map.tiles.iter().map(|t| t.terrain).collect();
        DiagonalCuller.build_map(&mut bm);
        let after: Vec<_> = bm.map.tiles.iter().map(|t| t.terrain).collect();

        assert_eq!(before, after);
    }
}
