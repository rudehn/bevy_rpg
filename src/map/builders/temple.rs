//! Temple pipeline — floors 9..=11 sit beneath the temple-entrance
//! forest tile. Reuses [`brogelike::BrogueLikeBuilder`] for the
//! geometry; only the upstairs / amulet hooks are new.

use bracket_lib::prelude::Point;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, MetaMapBuilder};
use crate::map::tile::TerrainType;

/// Links temple floor 1's UpStairs back to the forest temple-entrance
/// tile by stamping a `MapExitTile` on it.
///
/// Constructed with the forest floor index + entrance tile coordinates
/// (read from `OverworldState`); the builder writes them onto the exit
/// component so the player lands on the forest's DownStairs when
/// climbing out of the temple.
pub struct TempleUpstairsLinker {
    pub forest_floor: u32,
    pub forest_pos: Position,
}

impl TempleUpstairsLinker {
    pub fn boxed(forest_floor: u32, forest_pos: Position) -> Box<Self> {
        Box::new(Self { forest_floor, forest_pos })
    }
}

impl MetaMapBuilder for TempleUpstairsLinker {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        // Find the UpStairs the start-point builder dropped. Bail
        // quietly if there isn't one (shouldn't happen on temple-1).
        let Some(up) = find_upstairs(build) else {
            bevy::log::warn!(
                "TempleUpstairsLinker: no UpStairs on floor {}",
                build.map.depth,
            );
            return;
        };
        build.add_exit_tile(up, self.forest_floor, Some(self.forest_pos));
    }
}

fn find_upstairs(build: &BuilderMap) -> Option<Point> {
    for (idx, tile) in build.map.tiles.iter().enumerate() {
        if tile.terrain == TerrainType::UpStairs {
            let (x, y) = build.map.idx_xy(idx);
            return Some(Point::new(x, y));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{LiquidType, Decoration, Tile};

    fn make_build_with_upstairs() -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(20, 20);
        let idx = bm.map.xy_idx(5, 5);
        bm.map.tiles[idx] = Tile {
            terrain: TerrainType::UpStairs,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
        bm
    }

    #[test]
    fn linker_stamps_exit_tile_on_upstairs() {
        let mut bm = make_build_with_upstairs();
        let mut linker = TempleUpstairsLinker {
            forest_floor: 5,
            forest_pos: Position { x: 40, y: 30 },
        };
        linker.build_map(&mut bm);
        assert_eq!(bm.exit_tile_spawn_list.len(), 1);
        let (pt, exit) = bm.exit_tile_spawn_list[0];
        assert_eq!(pt.x, 5);
        assert_eq!(pt.y, 5);
        assert_eq!(exit.destination_floor, 5);
        assert_eq!(exit.destination_pos, Some(Position { x: 40, y: 30 }));
    }

    #[test]
    fn linker_skips_when_no_upstairs() {
        let mut bm = BuilderMap::new_for_test(20, 20);
        let mut linker = TempleUpstairsLinker {
            forest_floor: 5,
            forest_pos: Position { x: 0, y: 0 },
        };
        linker.build_map(&mut bm);
        assert!(bm.exit_tile_spawn_list.is_empty());
    }
}
