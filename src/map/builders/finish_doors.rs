//! Cleans up orphaned doors after map modifications (Brogue's finishDoors).

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::tile::TerrainType;

pub struct FinishDoors;

impl FinishDoors {
    pub fn new() -> Self {
        Self
    }
}

impl<C: BuildContext> MapBuilder<C> for FinishDoors {
    fn name(&self) -> &'static str { "FinishDoors" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::TerrainCleanup) }
    fn build(&mut self, ctx: &mut C) {
        let width = ctx.map().width;
        let height = ctx.map().height;
        let mut to_floor = Vec::new();

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = ctx.map().xy_idx(x, y);
                if ctx.map().tiles[idx].terrain != TerrainType::Door { continue; }

                let left  = ctx.map().tiles[ctx.map().xy_idx(x - 1, y)].terrain;
                let right = ctx.map().tiles[ctx.map().xy_idx(x + 1, y)].terrain;
                let up    = ctx.map().tiles[ctx.map().xy_idx(x, y - 1)].terrain;
                let down  = ctx.map().tiles[ctx.map().xy_idx(x, y + 1)].terrain;

                let is_blocking = |t: TerrainType| matches!(t, TerrainType::Wall | TerrainType::Empty);
                let is_passable = |t: TerrainType| !is_blocking(t);

                let passable_h = is_passable(left) || is_passable(right);
                let passable_v = is_passable(up) || is_passable(down);
                if passable_h && passable_v {
                    to_floor.push(idx);
                    continue;
                }

                let blocking_count = [left, right, up, down].iter().filter(|&&t| is_blocking(t)).count();
                if blocking_count >= 3 { to_floor.push(idx); }
            }
        }

        for idx in to_floor {
            ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builders::EngineBuilderMap;
    use crate::map::tile::TerrainType;

    fn idx(ctx: &EngineBuilderMap, x: i32, y: i32) -> usize {
        ctx.map.xy_idx(x, y)
    }

    #[test]
    fn orphaned_door_converted_to_floor() {
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", 42);
        let door = idx(&ctx, 2, 2);
        let left = idx(&ctx, 1, 2);
        let right = idx(&ctx, 3, 2);
        let above = idx(&ctx, 2, 1);
        let below = idx(&ctx, 2, 3);
        ctx.map.tiles[door].terrain = TerrainType::Door;
        ctx.map.tiles[left].terrain = TerrainType::Floor;
        ctx.map.tiles[right].terrain = TerrainType::Floor;
        ctx.map.tiles[above].terrain = TerrainType::Floor;
        ctx.map.tiles[below].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door].terrain, TerrainType::Floor);
    }

    #[test]
    fn valid_horizontal_door_kept() {
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", 42);
        let door = idx(&ctx, 2, 2);
        let left = idx(&ctx, 1, 2);
        let right = idx(&ctx, 3, 2);
        ctx.map.tiles[door].terrain = TerrainType::Door;
        ctx.map.tiles[left].terrain = TerrainType::Floor;
        ctx.map.tiles[right].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door].terrain, TerrainType::Door);
    }

    #[test]
    fn valid_vertical_door_kept() {
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", 42);
        let door = idx(&ctx, 2, 2);
        let above = idx(&ctx, 2, 1);
        let below = idx(&ctx, 2, 3);
        ctx.map.tiles[door].terrain = TerrainType::Door;
        ctx.map.tiles[above].terrain = TerrainType::Floor;
        ctx.map.tiles[below].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door].terrain, TerrainType::Door);
    }

    #[test]
    fn dead_end_door_converted() {
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", 42);
        let door = idx(&ctx, 2, 2);
        let left = idx(&ctx, 1, 2);
        ctx.map.tiles[door].terrain = TerrainType::Door;
        ctx.map.tiles[left].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door].terrain, TerrainType::Floor);
    }
}
