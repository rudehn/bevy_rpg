//! Removes isolated wall pillars completely surrounded by non-wall tiles.

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::tile::TerrainType;

pub struct PillarCuller;

impl PillarCuller {
    pub fn new() -> Self {
        Self
    }
}

impl<C: BuildContext> MapBuilder<C> for PillarCuller {
    fn name(&self) -> &'static str {
        "PillarCuller"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::TerrainCleanup)
    }
    fn build(&mut self, ctx: &mut C) {
        let width = ctx.map().width;
        let height = ctx.map().height;
        let mut to_remove = Vec::new();

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = ctx.map().xy_idx(x, y);
                if ctx.map().tiles[idx].terrain != TerrainType::Wall {
                    continue;
                }

                let mut wall_count = 0;
                for dy in -1..=1i32 {
                    for dx in -1..=1i32 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx < 0 || ny < 0 || nx >= width || ny >= height {
                            continue;
                        }
                        let n_idx = ctx.map().xy_idx(nx, ny);
                        if ctx.map().tiles[n_idx].terrain == TerrainType::Wall
                            || ctx.map().tiles[n_idx].terrain == TerrainType::Empty
                        {
                            wall_count += 1;
                        }
                    }
                }

                if wall_count == 0 {
                    to_remove.push(idx);
                }
            }
        }

        for idx in to_remove {
            ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builders::EngineBuilderMap;
    use crate::map::tile::TerrainType;

    #[test]
    fn removes_isolated_pillar() {
        let mut ctx = EngineBuilderMap::with_open_room(5, 5, 42);
        let center = ctx.map.xy_idx(2, 2);
        ctx.map.tiles[center].terrain = TerrainType::Wall;

        PillarCuller.build(&mut ctx);

        assert_eq!(
            ctx.map.tiles[center].terrain,
            TerrainType::Floor,
            "Isolated pillar should be removed"
        );
    }

    #[test]
    fn keeps_wall_connected_to_border() {
        let mut ctx = EngineBuilderMap::with_open_room(5, 5, 42);
        let idx = ctx.map.xy_idx(1, 1);
        ctx.map.tiles[idx].terrain = TerrainType::Wall;

        PillarCuller.build(&mut ctx);

        assert_eq!(
            ctx.map.tiles[idx].terrain,
            TerrainType::Wall,
            "Wall connected to border should be kept"
        );
    }
}
