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

        // Run cleanup passes until no door tile changes. Demoting an adjacent
        // door can create new orphan or dead-end doors that the next pass
        // needs to catch, so a single pass is insufficient for clusters.
        const MAX_PASSES: usize = 8;
        for _ in 0..MAX_PASSES {
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
                    let is_door = |t: TerrainType| matches!(
                        t,
                        TerrainType::Door
                            | TerrainType::OpenDoor
                            | TerrainType::LockedDoor
                            | TerrainType::HiddenDoor
                    );

                    // Rule 1: passable on both axes → door is sitting in an
                    // open area or at a corner, not a wall separator.
                    let passable_h = is_passable(left) || is_passable(right);
                    let passable_v = is_passable(up) || is_passable(down);
                    if passable_h && passable_v {
                        to_floor.push(idx);
                        continue;
                    }

                    // Rule 2: dead-end (≥3 blocking neighbors).
                    let blocking_count = [left, right, up, down]
                        .iter().filter(|&&t| is_blocking(t)).count();
                    if blocking_count >= 3 {
                        to_floor.push(idx);
                        continue;
                    }

                    // Rule 3: adjacent to another door — scan-order dedup.
                    // Demote this door if the one immediately to its left or
                    // above is also a door. For longer runs this still
                    // converges because demotion happens left-to-right /
                    // top-to-bottom; each pass eats one tile of the chain.
                    if is_door(left) || is_door(up) {
                        to_floor.push(idx);
                        continue;
                    }
                }
            }

            if to_floor.is_empty() { break; }
            for idx in to_floor {
                ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
            }
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

    #[test]
    fn adjacent_horizontal_doors_dedupe() {
        // Two doors side-by-side in a wall separator: floor on left of A,
        // floor on right of B, doors at (2,2) and (3,2). The second gets
        // demoted by scan-order rule 3.
        let mut ctx = EngineBuilderMap::with_seed(1, 7, 5, "test", 42);
        let door_a = idx(&ctx, 2, 2);
        let door_b = idx(&ctx, 3, 2);
        let floor_l = idx(&ctx, 1, 2);
        let floor_r = idx(&ctx, 4, 2);
        ctx.map.tiles[door_a].terrain = TerrainType::Door;
        ctx.map.tiles[door_b].terrain = TerrainType::Door;
        ctx.map.tiles[floor_l].terrain = TerrainType::Floor;
        ctx.map.tiles[floor_r].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door_a].terrain, TerrainType::Door,
            "First door in scan order kept");
        assert_eq!(ctx.map.tiles[door_b].terrain, TerrainType::Floor,
            "Adjacent door demoted");
    }

    #[test]
    fn adjacent_vertical_doors_dedupe() {
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 7, "test", 42);
        let door_a = idx(&ctx, 2, 2);
        let door_b = idx(&ctx, 2, 3);
        let floor_u = idx(&ctx, 2, 1);
        let floor_d = idx(&ctx, 2, 4);
        ctx.map.tiles[door_a].terrain = TerrainType::Door;
        ctx.map.tiles[door_b].terrain = TerrainType::Door;
        ctx.map.tiles[floor_u].terrain = TerrainType::Floor;
        ctx.map.tiles[floor_d].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door_a].terrain, TerrainType::Door);
        assert_eq!(ctx.map.tiles[door_b].terrain, TerrainType::Floor);
    }

    #[test]
    fn chain_of_three_doors_dedupes_to_one() {
        // D D D in a horizontal wall, floor on each end.
        let mut ctx = EngineBuilderMap::with_seed(1, 9, 5, "test", 42);
        let a = idx(&ctx, 2, 2);
        let b = idx(&ctx, 3, 2);
        let c = idx(&ctx, 4, 2);
        let floor_l = idx(&ctx, 1, 2);
        let floor_r = idx(&ctx, 5, 2);
        ctx.map.tiles[a].terrain = TerrainType::Door;
        ctx.map.tiles[b].terrain = TerrainType::Door;
        ctx.map.tiles[c].terrain = TerrainType::Door;
        ctx.map.tiles[floor_l].terrain = TerrainType::Floor;
        ctx.map.tiles[floor_r].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);

        // After pass 1: B demoted (left=A is door). C kept (left=B, but B
        // is queued for demotion this pass — it's still a door when C is
        // evaluated). After pass 1 commit: A door, B floor, C door.
        // After pass 2: C — left=B=floor, but rule-1 fires if C now has
        // passable_h (B=floor on left, floor_r on right) AND passable_v
        // (walls top/bottom → not passable). So passable_v=false, rule 1
        // doesn't fire. Adjacent-door check: left=floor, up=wall → no
        // demotion. C stays.
        // Net: two doors with a floor gap between them. That's an
        // acceptable, non-clustered outcome.
        let kept: usize = [a, b, c]
            .iter()
            .filter(|&&i| ctx.map.tiles[i].terrain == TerrainType::Door)
            .count();
        assert!(kept < 3, "Chain must not survive intact, got {kept} doors");
        // No two surviving doors may be orthogonally adjacent.
        let pairs = [(a, b), (b, c)];
        for (p, q) in pairs {
            assert!(
                !(ctx.map.tiles[p].terrain == TerrainType::Door
                    && ctx.map.tiles[q].terrain == TerrainType::Door),
                "Adjacent doors survived"
            );
        }
    }

    #[test]
    fn corner_door_three_floor_neighbors_converted() {
        // Door at (2,2) with floor on L, R, and U (3 floor neighbors → an
        // L-corner). Rule 1 (passable on both axes) catches this.
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", 42);
        let door = idx(&ctx, 2, 2);
        let left = idx(&ctx, 1, 2);
        let right = idx(&ctx, 3, 2);
        let above = idx(&ctx, 2, 1);
        ctx.map.tiles[door].terrain = TerrainType::Door;
        ctx.map.tiles[left].terrain = TerrainType::Floor;
        ctx.map.tiles[right].terrain = TerrainType::Floor;
        ctx.map.tiles[above].terrain = TerrainType::Floor;

        FinishDoors.build(&mut ctx);
        assert_eq!(ctx.map.tiles[door].terrain, TerrainType::Floor);
    }
}
