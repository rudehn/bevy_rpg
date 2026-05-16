//! `AmuletPlacerBuilder` — places the Amulet of Yendor (the one quest
//! item this build of the game ships with) on the deepest temple floor
//! at the most distant walkable tile from the player's start. Mirrors
//! what `DistantExit` does for `DownStairs`, except it pushes onto
//! `item_spawn_list` instead of mutating terrain.

use bracket_lib::prelude::Point;

use crate::map::builders::{BuilderMap, BuilderPhase, MetaMapBuilder};
use crate::map::tile::is_walkable;

/// Name of the quest item placed by this builder. The item must exist
/// in `assets/items.ron` with `is_quest_item: true`; otherwise spawn
/// will fall through to a regular item and the victory check will
/// never fire.
pub const AMULET_NAME: &str = "Amulet of Yendor";

pub struct AmuletPlacerBuilder;

impl AmuletPlacerBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for AmuletPlacerBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let Some(start) = build.starting_position else {
            bevy::log::warn!("AmuletPlacerBuilder: no starting_position; skipping");
            return;
        };
        let target = farthest_walkable(build, start.x, start.y);
        build.add_item_spawn(target, AMULET_NAME.to_string(), 1);
    }
}

/// Manhattan-distance scan for the walkable tile farthest from `(sx,
/// sy)`. We avoid Dijkstra here because the builder runs after
/// terrain settles — the player can always reach any walkable tile in
/// principle, and a stricter reachability check isn't needed for the
/// "one quest item per floor" placement.
fn farthest_walkable(build: &BuilderMap, sx: i32, sy: i32) -> Point {
    let mut best = Point::new(sx, sy);
    let mut best_dist: i32 = -1;
    for y in 0..build.height {
        for x in 0..build.width {
            let idx = build.map.xy_idx(x, y);
            if !is_walkable(build.map.tiles[idx]) { continue; }
            let dist = (x - sx).abs() + (y - sy).abs();
            if dist > best_dist {
                best_dist = dist;
                best = Point::new(x, y);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Position;
    use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

    fn carve(bm: &mut BuilderMap, x: i32, y: i32) {
        let idx = bm.map.xy_idx(x, y);
        bm.map.tiles[idx] = Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
    }

    #[test]
    fn amulet_lands_at_distant_walkable() {
        let mut bm = BuilderMap::new_for_test(20, 20);
        // Carve only two walkable cells: the start and a distant one.
        carve(&mut bm, 1, 1);
        carve(&mut bm, 18, 18);
        bm.set_starting_position(Position { x: 1, y: 1 });
        AmuletPlacerBuilder.build_map(&mut bm);
        assert_eq!(bm.item_spawn_list.len(), 1);
        let (pt, name, count) = &bm.item_spawn_list[0];
        assert_eq!(*pt, Point::new(18, 18));
        assert_eq!(name, AMULET_NAME);
        assert_eq!(*count, 1);
    }

    #[test]
    fn amulet_placer_skips_without_starting_position() {
        let mut bm = BuilderMap::new_for_test(20, 20);
        carve(&mut bm, 5, 5);
        AmuletPlacerBuilder.build_map(&mut bm);
        assert!(bm.item_spawn_list.is_empty());
    }
}
