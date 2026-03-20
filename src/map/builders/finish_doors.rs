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

                // Orphaned: at least one passable neighbor on each axis
                // A valid door blocks one axis entirely (wall on both sides)
                let passable_h = is_passable(left) || is_passable(right);
                let passable_v = is_passable(up) || is_passable(down);
                if passable_h && passable_v
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::TerrainType;

    #[test]
    fn orphaned_door_converted_to_floor() {
        // Door with passable neighbors on both axes → orphaned (not a chokepoint)
        //   .
        //  .+.
        //   .
        let mut bm = BuilderMap::new_for_test(5, 5);
        let door_idx = bm.map.xy_idx(2, 2);
        let left_idx = bm.map.xy_idx(1, 2);
        let right_idx = bm.map.xy_idx(3, 2);
        let above_idx = bm.map.xy_idx(2, 1);
        let below_idx = bm.map.xy_idx(2, 3);
        bm.map.tiles[door_idx].terrain = TerrainType::Door;
        bm.map.tiles[left_idx].terrain = TerrainType::Floor;
        bm.map.tiles[right_idx].terrain = TerrainType::Floor;
        bm.map.tiles[above_idx].terrain = TerrainType::Floor;
        bm.map.tiles[below_idx].terrain = TerrainType::Floor;

        FinishDoors.build_map(&mut bm);

        assert_eq!(
            bm.map.tiles[door_idx].terrain,
            TerrainType::Floor,
            "Orphaned door should become floor"
        );
    }

    #[test]
    fn valid_horizontal_door_kept() {
        // Valid door:  x#x     (# above, # below, . left, . right)
        //              .+.     Vertical axis fully blocked → valid chokepoint
        //              x#x
        let mut bm = BuilderMap::new_for_test(5, 5);
        let door_idx = bm.map.xy_idx(2, 2);
        let left_idx = bm.map.xy_idx(1, 2);
        let right_idx = bm.map.xy_idx(3, 2);
        bm.map.tiles[door_idx].terrain = TerrainType::Door;
        bm.map.tiles[left_idx].terrain = TerrainType::Floor;
        bm.map.tiles[right_idx].terrain = TerrainType::Floor;
        // above (2,1) and below (2,3) are walls (default)

        FinishDoors.build_map(&mut bm);

        assert_eq!(
            bm.map.tiles[door_idx].terrain,
            TerrainType::Door,
            "Valid horizontal door should be kept"
        );
    }

    #[test]
    fn valid_vertical_door_kept() {
        // Valid door:  x.x     (# left, # right, . above, . below)
        //              #+#     Horizontal axis fully blocked → valid chokepoint
        //              x.x
        let mut bm = BuilderMap::new_for_test(5, 5);
        let door_idx = bm.map.xy_idx(2, 2);
        let above_idx = bm.map.xy_idx(2, 1);
        let below_idx = bm.map.xy_idx(2, 3);
        bm.map.tiles[door_idx].terrain = TerrainType::Door;
        bm.map.tiles[above_idx].terrain = TerrainType::Floor;
        bm.map.tiles[below_idx].terrain = TerrainType::Floor;
        // left (1,2) and right (3,2) are walls (default)

        FinishDoors.build_map(&mut bm);

        assert_eq!(
            bm.map.tiles[door_idx].terrain,
            TerrainType::Door,
            "Valid vertical door should be kept"
        );
    }

    #[test]
    fn dead_end_door_converted() {
        let mut bm = BuilderMap::new_for_test(5, 5);
        let door_idx = bm.map.xy_idx(2, 2);
        let left_idx = bm.map.xy_idx(1, 2);
        bm.map.tiles[door_idx].terrain = TerrainType::Door;
        bm.map.tiles[left_idx].terrain = TerrainType::Floor;

        FinishDoors.build_map(&mut bm);

        assert_eq!(
            bm.map.tiles[door_idx].terrain,
            TerrainType::Floor,
            "Dead-end door should become floor"
        );
    }
}
