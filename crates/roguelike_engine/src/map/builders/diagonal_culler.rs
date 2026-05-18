//! Brogue's removeDiagonalOpenings: removes diagonal-only passages where
//! two passable tiles are diagonally adjacent with blocking tiles on both
//! shared edges. One of the blocking tiles is converted to match its
//! passable neighbor, eliminating the diagonal-only gap.

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::tile::TerrainType;

pub struct DiagonalCuller;

impl DiagonalCuller {
    pub fn new() -> Self {
        Self
    }
}

impl<C: BuildContext> MapBuilder<C> for DiagonalCuller {
    fn name(&self) -> &'static str {
        "DiagonalCuller"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::TerrainCleanup)
    }
    fn build(&mut self, ctx: &mut C) {
        let width = ctx.map().width;
        let height = ctx.map().height;

        let mut changed = true;
        while changed {
            changed = false;

            for y in 0..height - 1 {
                for x in 0..width - 1 {
                    for k in 0..=1i32 {
                        let ax = x + k;
                        let bx = x + (1 - k);

                        let idx_a = ctx.map().xy_idx(ax, y);
                        let idx_b = ctx.map().xy_idx(bx, y);
                        let idx_c = ctx.map().xy_idx(ax, y + 1);
                        let idx_d = ctx.map().xy_idx(bx, y + 1);

                        let is_blocking = |idx: usize| {
                            let t = ctx.map().tiles[idx].terrain;
                            matches!(t, TerrainType::Wall | TerrainType::Empty)
                        };
                        let is_open = |idx: usize| !is_blocking(idx);

                        if is_open(idx_a) && is_blocking(idx_b) && is_blocking(idx_c) && is_open(idx_d) {
                            // Use the seeded RNG from the build context
                            let pick_b = ctx.rng().range(0, 2) == 0;
                            let (target_idx, source_idx) = if pick_b {
                                (idx_b, idx_a)
                            } else {
                                (idx_c, idx_d)
                            };

                            let source_tile = ctx.map().tiles[source_idx];
                            ctx.map_mut().tiles[target_idx] = source_tile;
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
    use crate::map::builders::EngineBuilderMap;
    use crate::map::tile::TerrainType;

    #[test]
    fn resolves_diagonal_passage() {
        let mut ctx = EngineBuilderMap::with_seed(1, 4, 4, "test", 42);
        let i11 = ctx.map.xy_idx(1, 1);
        let i22 = ctx.map.xy_idx(2, 2);
        let i21 = ctx.map.xy_idx(2, 1);
        let i12 = ctx.map.xy_idx(1, 2);

        ctx.map.tiles[i11].terrain = TerrainType::Floor;
        ctx.map.tiles[i22].terrain = TerrainType::Floor;

        DiagonalCuller.build(&mut ctx);

        let b = ctx.map.tiles[i21].terrain;
        let c = ctx.map.tiles[i12].terrain;
        assert!(
            b == TerrainType::Floor || c == TerrainType::Floor,
            "DiagonalCuller should resolve diagonal-only passage"
        );
    }

    #[test]
    fn no_change_when_no_diagonal() {
        let mut ctx = EngineBuilderMap::with_seed(1, 4, 4, "test", 42);
        let i11 = ctx.map.xy_idx(1, 1);
        let i21 = ctx.map.xy_idx(2, 1);
        ctx.map.tiles[i11].terrain = TerrainType::Floor;
        ctx.map.tiles[i21].terrain = TerrainType::Floor;

        let before: Vec<_> = ctx.map.tiles.iter().map(|t| t.terrain).collect();
        DiagonalCuller.build(&mut ctx);
        let after: Vec<_> = ctx.map.tiles.iter().map(|t| t.terrain).collect();

        assert_eq!(before, after);
    }
}
