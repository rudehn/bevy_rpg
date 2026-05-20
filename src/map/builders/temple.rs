//! Temple builder — the cult shrine at the bottom of the descent.
//!
//! Floor `MAX_FLOOR`. A linear stone corridor leads from the entrance
//! (where the player arrives via UpStairs from Forest 4) east to a
//! widened sanctum chamber, with the Amulet of Yendor on a pedestal at
//! the chamber's centre. Everything else is wall — there are no side
//! rooms today; the floor is intentionally small and theatrical so the
//! climax of the run reads as a single deliberate beat.
//!
//! This is the active-cult-shrine placeholder. Cultists aren't on the
//! roster yet; when they're added, they'll spawn through the standard
//! `VoronoiSpawner` path on top of this layout (the corridor is open
//! walkable Floor, so the spawner has plenty of cells to fill). The
//! shape is also designed so a future "descend deeper into the temple"
//! expansion can drop sub-levels through a `DownStairs` placed at the
//! sanctum.
//!
//! No connection to forest builders or town builders — this floor has
//! its own theme (`FloorKind::Temple`, cold stone) and its own builder
//! chain.

use bracket_lib::prelude::Point;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

/// Distance from the west/east border to the corridor end-points.
const CORRIDOR_INSET: i32 = 8;
/// Half-height of the corridor (1 = 3 tiles tall; 2 = 5 tiles tall).
const CORRIDOR_HALF_HEIGHT: i32 = 1;
/// Half-extent of the sanctum chamber at the east end (3 = 7×7 room).
const SANCTUM_HALF: i32 = 3;

// =====================================================================
// TempleLayoutBuilder — carves the corridor + sanctum into solid wall.
// =====================================================================

pub struct TempleLayoutBuilder;

impl TempleLayoutBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl InitialMapBuilder for TempleLayoutBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let mid_y = h / 2;
        let west_cx = CORRIDOR_INSET;
        let east_cx = w - 1 - CORRIDOR_INSET;

        // 1. Fill the whole map with wall — the temple is a sealed
        //    interior carved out of solid stone.
        for idx in 0..build.map.tiles.len() {
            build.map.tiles[idx] = Tile {
                terrain: TerrainType::Wall,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }

        // 2. Carve the entry corridor — runs east-west at mid_y, a few
        //    tiles tall so it feels like a hall rather than a tunnel.
        for x in west_cx..=east_cx {
            for dy in -CORRIDOR_HALF_HEIGHT..=CORRIDOR_HALF_HEIGHT {
                let y = mid_y + dy;
                if y <= 0 || y >= h - 1 { continue; }
                let idx = build.map.xy_idx(x, y);
                build.map.tiles[idx].terrain = TerrainType::Floor;
            }
        }

        // 3. Carve the sanctum chamber at the east end.
        for dy in -SANCTUM_HALF..=SANCTUM_HALF {
            for dx in -SANCTUM_HALF..=SANCTUM_HALF {
                let x = east_cx + dx;
                let y = mid_y + dy;
                if x <= 0 || y <= 0 || x >= w - 1 || y >= h - 1 { continue; }
                let idx = build.map.xy_idx(x, y);
                build.map.tiles[idx].terrain = TerrainType::Floor;
            }
        }

        // Player arrival point: west end of the corridor (where the
        // UpStairs will be stamped by TempleStairsBuilder).
        build.set_starting_position(Position { x: west_cx, y: mid_y });
    }
}

// =====================================================================
// TempleStairsBuilder — UpStairs at the corridor entry, Amulet in the
// sanctum.
// =====================================================================

pub struct TempleStairsBuilder;

impl TempleStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TempleStairsBuilder {
    // Same rationale as ForestStairsBuilder — stair tiles are terrain
    // placement, must exist before Spawning so VoronoiSpawner can skip
    // them (relevant when cultists land in a future pass).
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::StructurePlacement) }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let mid_y = h / 2;
        let west_cx = CORRIDOR_INSET;
        let east_cx = w - 1 - CORRIDOR_INSET;

        // UpStairs at the corridor's west end — back to Forest 4.
        let up_idx = build.map.xy_idx(west_cx, mid_y);
        build.map.tiles[up_idx].terrain = TerrainType::UpStairs;
        build.map.tiles[up_idx].liquid = LiquidType::None;

        // Amulet of Yendor at the sanctum centre. Spawned as an item
        // (not a terrain tile) so the player can pick it up normally
        // and the win-condition check on the town Portal fires.
        build.add_item_spawn(
            Point::new(east_cx, mid_y),
            "Amulet of Yendor".to_string(),
            1,
        );
        bevy::log::info!(
            "TempleStairsBuilder: UpStairs at ({}, {}), Amulet at sanctum ({}, {})",
            west_cx, mid_y, east_cx, mid_y,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_temple() -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(80, 60);
        bm.map.depth = crate::constants::MAX_FLOOR as i32;
        TempleLayoutBuilder.build_map(&mut bm);
        TempleStairsBuilder.build_map(&mut bm);
        bm
    }

    #[test]
    fn temple_starting_position_at_corridor_west_end() {
        let bm = build_temple();
        let start = bm.starting_position.expect("temple must set starting_position");
        assert_eq!(start.x, CORRIDOR_INSET);
        assert_eq!(start.y, bm.height / 2);
    }

    #[test]
    fn temple_places_upstairs_at_corridor_entry() {
        let bm = build_temple();
        let idx = bm.map.xy_idx(CORRIDOR_INSET, bm.height / 2);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::UpStairs);
    }

    #[test]
    fn temple_spawns_amulet_in_sanctum() {
        let bm = build_temple();
        let amulet_count = bm
            .item_spawn_list
            .iter()
            .filter(|(_, name, _)| name == "Amulet of Yendor")
            .count();
        assert_eq!(amulet_count, 1);
    }

    #[test]
    fn temple_corridor_is_walkable_end_to_end() {
        let bm = build_temple();
        let mid_y = bm.height / 2;
        let west_cx = CORRIDOR_INSET;
        let east_cx = bm.width - 1 - CORRIDOR_INSET;
        for x in west_cx..=east_cx {
            let idx = bm.map.xy_idx(x, mid_y);
            let terrain = bm.map.tiles[idx].terrain;
            assert!(
                matches!(terrain, TerrainType::Floor | TerrainType::UpStairs),
                "temple corridor tile ({}, {}) must be walkable, got {:?}", x, mid_y, terrain,
            );
        }
    }

    #[test]
    fn temple_outside_corridor_and_sanctum_is_wall() {
        // Sample a few tiles well outside the carved area — they must
        // still be walls (the temple is a sealed interior).
        let bm = build_temple();
        for (x, y) in [(2, 2), (2, bm.height - 3), (bm.width - 3, 2)] {
            let idx = bm.map.xy_idx(x, y);
            assert_eq!(
                bm.map.tiles[idx].terrain,
                TerrainType::Wall,
                "tile ({x}, {y}) should be solid wall outside the carved interior",
            );
        }
    }
}
