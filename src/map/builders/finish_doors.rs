use crate::map::tile::TerrainType;
use super::{BuilderMap, MetaMapBuilder};

/// Clean up orphaned doors after map modifications (Brogue's finishDoors logic).
/// A door is orphaned if:
/// - It has passable terrain on both horizontal AND vertical axes (open space, not between walls)
/// - OR it has 3+ wall/blocking neighbors in cardinal directions (dead-end door)
pub struct FinishDoors;

impl FinishDoors {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for FinishDoors {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let width = build_data.map.width;
        let height = build_data.map.height;
        let mut to_floor = Vec::new();

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = build_data.map.xy_idx(x, y);
                if build_data.map.tiles[idx].terrain != TerrainType::Door {
                    continue;
                }

                let left = build_data.map.tiles[build_data.map.xy_idx(x - 1, y)].terrain;
                let right = build_data.map.tiles[build_data.map.xy_idx(x + 1, y)].terrain;
                let up = build_data.map.tiles[build_data.map.xy_idx(x, y - 1)].terrain;
                let down = build_data.map.tiles[build_data.map.xy_idx(x, y + 1)].terrain;

                let is_blocking =
                    |t: TerrainType| matches!(t, TerrainType::Wall | TerrainType::Empty);
                let is_passable = |t: TerrainType| !is_blocking(t);

                // Orphaned: passable on both sides of either axis
                if (is_passable(left) && is_passable(right))
                    || (is_passable(up) && is_passable(down))
                {
                    to_floor.push(idx);
                    continue;
                }

                // Dead-end: 3+ blocking cardinal neighbors
                let blocking_count =
                    [left, right, up, down].iter().filter(|&&t| is_blocking(t)).count();
                if blocking_count >= 3 {
                    to_floor.push(idx);
                }
            }
        }

        for idx in to_floor {
            build_data.map.tiles[idx].terrain = TerrainType::Floor;
        }
    }
}
