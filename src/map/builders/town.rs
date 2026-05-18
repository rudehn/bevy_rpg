//! Procedural town builder — the hub at floor 0.
//!
//! The town is an open Floor map with a handful of small buildings
//! scattered through it. Two special tiles:
//!
//! - **Portal** at the map centre — the win condition return point
//!   once the player has the Amulet of Yendor (otherwise just hums
//!   ominously).
//! - **DownStairs** on the south border — the descent into the forest
//!   (floor 1).
//!
//! Pipeline shape (registered in [`super::floor_builder`]):
//!
//! - `TownLayoutBuilder`     (Geometry):     open Floor + scattered buildings
//! - `TownPortalBuilder`     (Finalization): Portal terrain at map centre
//! - `TownDownStairsBuilder` (Finalization): one DownStairs on south border

use bracket_lib::prelude::{Point, Rect};
use bracket_lib::random::RandomNumberGenerator;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

/// How many buildings the layout pass tries to place.
const TARGET_BUILDINGS: usize = 8;
/// Minimum and maximum building footprint dimensions.
const BUILDING_MIN: i32 = 4;
const BUILDING_MAX: i32 = 8;
/// Keep buildings away from the map border by this many tiles so they
/// never block the south-border DownStairs.
const BORDER_MARGIN: i32 = 4;
/// Half-width of the open square around the map centre kept clear so
/// the return portal + player spawn are never inside a building.
const CENTER_KEEPOUT: i32 = 3;

// =====================================================================
// TownLayoutBuilder — open floor + scattered buildings.
// =====================================================================

pub struct TownLayoutBuilder;

impl TownLayoutBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl InitialMapBuilder for TownLayoutBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;

        // 1. Fill with open Floor.
        for idx in 0..build.map.tiles.len() {
            build.map.tiles[idx] = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }

        // 2. Scatter a handful of small buildings around the centre.
        let center = Point::new(w / 2, h / 2);
        let mut rng = RandomNumberGenerator::new();
        let mut buildings: Vec<Rect> = Vec::new();
        let center_keepout = Rect::with_size(
            center.x - CENTER_KEEPOUT,
            center.y - CENTER_KEEPOUT,
            CENTER_KEEPOUT * 2 + 1,
            CENTER_KEEPOUT * 2 + 1,
        );

        // Reserved corridor in front of the south-border DownStairs so
        // the player can always step into town from the stair without
        // bumping into a building.
        let down_stair_keepout = Rect::with_size(w / 2 - 2, h - 5, 5, 4);

        let mut tries = 0;
        while buildings.len() < TARGET_BUILDINGS && tries < 200 {
            tries += 1;
            let bw = rng.range(BUILDING_MIN, BUILDING_MAX + 1);
            let bh = rng.range(BUILDING_MIN, BUILDING_MAX + 1);
            let bx = rng.range(BORDER_MARGIN, w - bw - BORDER_MARGIN);
            let by = rng.range(BORDER_MARGIN, h - bh - BORDER_MARGIN);
            let candidate = Rect::with_size(bx, by, bw, bh);

            if rects_overlap_with_margin(&center_keepout, &candidate, 1) {
                continue;
            }
            if rects_overlap_with_margin(&down_stair_keepout, &candidate, 1) {
                continue;
            }
            if buildings.iter().any(|r| rects_overlap_with_margin(r, &candidate, 2)) {
                continue;
            }

            stamp_building(build, candidate, center);
            buildings.push(candidate);
        }

        build.rooms = Some(buildings);

        // Player starts one tile off-centre so they aren't standing on
        // the portal.
        build.set_starting_position(Position {
            x: center.x + 1,
            y: center.y,
        });
    }
}

// =====================================================================
// TownPortalBuilder — the return Portal at the map centre.
// =====================================================================

pub struct TownPortalBuilder;

impl TownPortalBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownPortalBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let cx = build.width / 2;
        let cy = build.height / 2;
        let idx = build.map.xy_idx(cx, cy);
        build.map.tiles[idx].terrain = TerrainType::Portal;
        build.map.tiles[idx].liquid = LiquidType::None;
        build.map.tiles[idx].decoration = Decoration::None;
    }
}

// =====================================================================
// TownDownStairsBuilder — single `>` on the south border into Forest 1.
// =====================================================================

pub struct TownDownStairsBuilder;

impl TownDownStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownDownStairsBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        // South-border midpoint, one tile inside the border so the
        // stair is reachable from the interior.
        let x = build.width / 2;
        let y = build.height - 2;
        let idx = build.map.xy_idx(x, y);
        build.map.tiles[idx].terrain = TerrainType::DownStairs;
        build.map.tiles[idx].liquid = LiquidType::None;
        build.map.tiles[idx].decoration = Decoration::None;
    }
}

// =====================================================================
// Helpers (private)
// =====================================================================

fn stamp_building(build: &mut BuilderMap, r: Rect, toward: Point) {
    for y in r.y1..r.y2 {
        for x in r.x1..r.x2 {
            if x < 0 || y < 0 || x >= build.width || y >= build.height { continue; }
            let on_border = x == r.x1 || x == r.x2 - 1 || y == r.y1 || y == r.y2 - 1;
            let idx = build.map.xy_idx(x, y);
            build.map.tiles[idx] = Tile {
                terrain: if on_border { TerrainType::Wall } else { TerrainType::Floor },
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
    }
    let door = door_for_building(r, toward);
    if door.x >= 0 && door.y >= 0 && door.x < build.width && door.y < build.height {
        let idx = build.map.xy_idx(door.x, door.y);
        build.map.tiles[idx].terrain = TerrainType::Door;
        build.map.tiles[idx].liquid = LiquidType::None;
    }
}

fn rects_overlap_with_margin(a: &Rect, b: &Rect, margin: i32) -> bool {
    !(a.x2 + margin <= b.x1
        || b.x2 + margin <= a.x1
        || a.y2 + margin <= b.y1
        || b.y2 + margin <= a.y1)
}

fn door_for_building(building: Rect, toward: Point) -> Point {
    let cx = building.x1 + building.width() / 2;
    let cy = building.y1 + building.height() / 2;
    let dx = toward.x - cx;
    let dy = toward.y - cy;
    if dx.abs() > dy.abs() {
        if dx > 0 { Point::new(building.x2 - 1, cy) } else { Point::new(building.x1, cy) }
    } else if dy > 0 {
        Point::new(cx, building.y2 - 1)
    } else {
        Point::new(cx, building.y1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn town_layout_sets_starting_position() {
        let mut bm = BuilderMap::new_for_test(80, 60);
        TownLayoutBuilder.build_map(&mut bm);
        assert!(bm.starting_position.is_some());
    }

    #[test]
    fn town_portal_stamped_at_centre() {
        let mut bm = BuilderMap::new_for_test(80, 60);
        TownLayoutBuilder.build_map(&mut bm);
        TownPortalBuilder.build_map(&mut bm);
        let idx = bm.map.xy_idx(40, 30);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Portal);
    }

    #[test]
    fn town_down_stairs_on_south_border() {
        let mut bm = BuilderMap::new_for_test(80, 60);
        TownLayoutBuilder.build_map(&mut bm);
        TownDownStairsBuilder.build_map(&mut bm);
        let count = bm
            .map
            .tiles
            .iter()
            .filter(|t| t.terrain == TerrainType::DownStairs)
            .count();
        assert_eq!(count, 1);
    }
}
