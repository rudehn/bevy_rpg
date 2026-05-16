//! Procedural town builder.
//!
//! The town is the central hub (floor 0). The whole map is open Floor
//! — no walls, no plaza enclosure — with a handful of small buildings
//! scattered through it as the only obstacles. The player walks freely
//! to any of the 8 overworld edges; the return Portal sits at the
//! map's centre.
//!
//! Pipeline shape (registered in [`super::floor_builder`]):
//!
//! - `TownLayoutBuilder` (Geometry): open Floor + scattered buildings
//! - `MapEdgeBuilder`    (Finalization): 8 edge exits to the forest ring
//! - `TownPortalBuilder` (Finalization): Portal terrain at map centre

use bracket_lib::prelude::{Point, Rect};
use bracket_lib::random::RandomNumberGenerator;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};
use crate::map::world::{
    CardinalDir, STAIRS_PER_BORDER, arrival_at_mirror, border_stair_positions,
    cardinal_neighbor, valid_cardinal_exits,
};

/// How many buildings the layout pass tries to place.
const TARGET_BUILDINGS: usize = 8;
/// Minimum and maximum building footprint dimensions.
const BUILDING_MIN: i32 = 4;
const BUILDING_MAX: i32 = 8;
/// Keep buildings away from the map border by this many tiles so they
/// never block an overworld edge exit.
const BORDER_MARGIN: i32 = 4;
/// Half-width of the open square around the map centre kept clear so
/// the return portal + player spawn are never inside a building.
const CENTER_KEEPOUT: i32 = 3;

pub struct TownLayoutBuilder;

impl TownLayoutBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl InitialMapBuilder for TownLayoutBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;

        // 1. Fill the world with open Floor — no surrounding walls,
        //    so the player can walk freely to any overworld edge.
        for idx in 0..build.map.tiles.len() {
            build.map.tiles[idx] = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }

        // 2. Scatter a handful of small buildings. Each has a wall
        //    border, a Floor interior, and one Door punched into the
        //    side closest to the map centre.
        let center = Point::new(w / 2, h / 2);
        let mut rng = RandomNumberGenerator::new();
        let mut buildings: Vec<Rect> = Vec::new();
        let center_keepout = Rect::with_size(
            center.x - CENTER_KEEPOUT,
            center.y - CENTER_KEEPOUT,
            CENTER_KEEPOUT * 2 + 1,
            CENTER_KEEPOUT * 2 + 1,
        );

        // Reserved zones around each of the 16 border stair tiles
        // (4 per N/S/E/W) plus a 2-tile inward corridor so the player
        // can always step inland from a stair without bumping into a
        // building. `TownBorderStairsBuilder` stamps the actual tiles
        // afterwards.
        let mut stair_keepouts: Vec<Rect> = Vec::new();
        for dir in CardinalDir::ALL {
            for stair in border_stair_positions(dir) {
                let (dx, dy) = dir.mirror().delta(); // inward step
                // Cover the stair tile + a few tiles inward.
                let inland = Position { x: stair.x + dx * 3, y: stair.y + dy * 3 };
                let min_x = stair.x.min(inland.x) - 1;
                let min_y = stair.y.min(inland.y) - 1;
                let kw = (stair.x - inland.x).abs() + 3;
                let kh = (stair.y - inland.y).abs() + 3;
                stair_keepouts.push(Rect::with_size(min_x, min_y, kw, kh));
            }
        }

        let mut tries = 0;
        while buildings.len() < TARGET_BUILDINGS && tries < 200 {
            tries += 1;
            let bw = rng.range(BUILDING_MIN, BUILDING_MAX + 1);
            let bh = rng.range(BUILDING_MIN, BUILDING_MAX + 1);
            let bx = rng.range(BORDER_MARGIN, w - bw - BORDER_MARGIN);
            let by = rng.range(BORDER_MARGIN, h - bh - BORDER_MARGIN);
            let candidate = Rect::with_size(bx, by, bw, bh);

            // Don't overlap the centre keepout.
            if rects_overlap_with_margin(&center_keepout, &candidate, 1) {
                continue;
            }
            // Don't overlap any of the 8 town-stair keepouts.
            if stair_keepouts.iter().any(|r| rects_overlap_with_margin(r, &candidate, 1)) {
                continue;
            }
            // Leave at least one tile of breathing room between
            // buildings so doors are never adjacent to other walls.
            if buildings.iter().any(|r| rects_overlap_with_margin(r, &candidate, 2)) {
                continue;
            }

            stamp_building(build, candidate, center);
            buildings.push(candidate);
        }

        // 3. Record buildings as `rooms` for any downstream builder
        //    that wants to know where they are (e.g. future NPC
        //    placement). The portal builder no longer relies on this.
        build.rooms = Some(buildings);

        // 4. Player starts near the centre but offset by one tile so
        //    they aren't standing directly on the return portal.
        build.set_starting_position(Position {
            x: center.x + 1,
            y: center.y,
        });
    }
}

/// Stamps the 16 town-to-forest `DownStairs` border tiles — 4 along
/// each of the N/S/E/W borders. The K-th stair on the N border pairs
/// with the K-th stair on the destination forest's S border (and
/// likewise E↔W), so walking off one map lands the player at the
/// matching position on the destination map — the world feels
/// continuous.
pub struct TownBorderStairsBuilder;

impl TownBorderStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownBorderStairsBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::StructurePlacement)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let floor = build.map.depth as u32;
        for dir in valid_cardinal_exits(floor) {
            let Some(dest_floor) = cardinal_neighbor(floor, dir) else { continue };
            for (k, pos) in border_stair_positions(dir).into_iter().enumerate() {
                let idx = build.map.xy_idx(pos.x, pos.y);
                build.map.tiles[idx] = Tile {
                    terrain: TerrainType::DownStairs,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
                build.add_exit_tile(
                    Point::new(pos.x, pos.y),
                    dest_floor,
                    Some(arrival_at_mirror(dir, k)),
                );
            }
        }
        let _ = STAIRS_PER_BORDER; // sanity: referenced for future iteration
    }
}

/// Paints the town's dirt-path network: a 4-wide cross through the
/// centre connecting the N/S stair clusters and the E/W stair
/// clusters, plus a short connector from each building's door to the
/// nearest leg of the cross.
///
/// Paths are marked with `Decoration::Custom { id: TOWN_PATH_DECO_ID }`
/// — the renderer in `themed_tile_display` substitutes a packed-dirt
/// look. The underlying terrain stays `Floor` so movement and other
/// systems work normally.
pub struct TownPathBuilder;

impl TownPathBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownPathBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let mid_x = w / 2;
        let mid_y = h / 2;

        // 1. Cross-shaped main path — 4 tiles wide so the K=0..3 stair
        //    clusters each line up with a path lane.
        for dx in -2..=1_i32 {
            for y in 1..h - 1 {
                paint_path(build, mid_x + dx, y);
            }
        }
        for dy in -2..=1_i32 {
            for x in 1..w - 1 {
                paint_path(build, x, mid_y + dy);
            }
        }

        // 2. Connector paths from each building's door to the cross.
        //    We iterate the recorded buildings (in `rooms`) instead of
        //    scanning for Door tiles so we know which side of the
        //    building each door is on — that tells us which way to
        //    step away from the door before tunneling.
        let center = Point::new(mid_x, mid_y);
        let buildings = build.rooms.clone().unwrap_or_default();
        for building in buildings {
            let door = door_for_building(building, center);
            let (odx, ody) = outward_step(door, building);
            let start = Point::new(door.x + odx, door.y + ody);
            connect_to_cross(build, start, mid_x, mid_y);
        }
    }
}

/// Carve an L-path from `from` toward the centre cross. Picks the
/// shorter leg first (vertical vs horizontal) so the path doesn't
/// double back through the same building's wall.
fn connect_to_cross(build: &mut BuilderMap, from: Point, mid_x: i32, mid_y: i32) {
    let dist_to_horiz = (from.y - mid_y).abs();
    let dist_to_vert = (from.x - mid_x).abs();
    if dist_to_horiz <= dist_to_vert {
        // Step horizontally to the column closest to the vertical
        // cross arm, then vertically until we hit the horizontal arm.
        let (xa, xb) = if from.x < mid_x { (from.x, mid_x) } else { (mid_x, from.x) };
        for x in xa..=xb {
            paint_path(build, x, from.y);
        }
        let (ya, yb) = if from.y < mid_y { (from.y, mid_y) } else { (mid_y, from.y) };
        for y in ya..=yb {
            paint_path(build, mid_x, y);
        }
    } else {
        let (ya, yb) = if from.y < mid_y { (from.y, mid_y) } else { (mid_y, from.y) };
        for y in ya..=yb {
            paint_path(build, from.x, y);
        }
        let (xa, xb) = if from.x < mid_x { (from.x, mid_x) } else { (mid_x, from.x) };
        for x in xa..=xb {
            paint_path(build, x, mid_y);
        }
    }
}

/// Mark a Floor tile as part of the town path. Skips walls, doors,
/// stairs, portals, and decorations placed by other systems.
fn paint_path(build: &mut BuilderMap, x: i32, y: i32) {
    if x <= 0 || y <= 0 || x >= build.width - 1 || y >= build.height - 1 { return; }
    let idx = build.map.xy_idx(x, y);
    let tile = &mut build.map.tiles[idx];
    if tile.terrain != TerrainType::Floor { return; }
    tile.decoration = Decoration::Custom { id: crate::map::world::TOWN_PATH_DECO_ID };
}

/// Stamps `TerrainType::Portal` at the map centre — the win tile.
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

// ----- helpers --------------------------------------------------------------

/// Place a single building: wall border, floor interior, one door on
/// the side facing `toward`. Coordinates outside the map are ignored.
fn stamp_building(build: &mut BuilderMap, r: Rect, toward: Point) {
    // Wall border.
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
    // Door on the side closest to `toward`.
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

/// Pure helper: where the door sits on `building` if it faces `toward`.
/// Picks the midpoint of the wall closest to `toward`, never a corner.
/// Used by both `stamp_building` (to place it) and `TownPathBuilder`
/// (to know which tile to start carving the dirt path from).
fn door_for_building(building: Rect, toward: Point) -> Point {
    let cx = building.x1 + building.width() / 2;
    let cy = building.y1 + building.height() / 2;
    let dx = toward.x - cx;
    let dy = toward.y - cy;
    if dx.abs() > dy.abs() {
        if dx > 0 {
            Point::new(building.x2 - 1, cy)
        } else {
            Point::new(building.x1, cy)
        }
    } else if dy > 0 {
        Point::new(cx, building.y2 - 1)
    } else {
        Point::new(cx, building.y1)
    }
}

/// Outward step away from the building centre, used to start the path
/// carve one tile outside the door (so we never tunnel through the
/// building interior).
fn outward_step(door: Point, building: Rect) -> (i32, i32) {
    let cx = building.x1 + building.width() / 2;
    let cy = building.y1 + building.height() / 2;
    let dx = door.x - cx;
    let dy = door.y - cy;
    if dx.abs() > dy.abs() {
        (dx.signum(), 0)
    } else {
        (0, dy.signum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::is_walkable;

    fn build_town() -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(80, 60);
        bm.map.depth = 0;
        TownLayoutBuilder.build_map(&mut bm);
        bm
    }

    #[test]
    fn town_has_a_starting_position_at_center() {
        let bm = build_town();
        let start = bm.starting_position.expect("town must set starting_position");
        let cx = bm.width / 2;
        let cy = bm.height / 2;
        // Start sits one tile off centre so the player isn't on the portal.
        assert!((start.x - cx).abs() <= 2);
        assert_eq!(start.y, cy);
    }

    #[test]
    fn town_start_and_neighborhood_are_walkable() {
        let bm = build_town();
        let start = bm.starting_position.unwrap();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let x = start.x + dx;
                let y = start.y + dy;
                let idx = bm.map.xy_idx(x, y);
                let tile = bm.map.tiles[idx];
                assert!(
                    is_walkable(tile),
                    "tile ({}, {}) around start should be walkable, was {:?}", x, y, tile.terrain,
                );
            }
        }
    }

    #[test]
    fn town_portal_lands_at_map_center() {
        let mut bm = build_town();
        TownPortalBuilder.build_map(&mut bm);
        let cx = bm.width / 2;
        let cy = bm.height / 2;
        let idx = bm.map.xy_idx(cx, cy);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Portal);
    }

    #[test]
    fn town_edges_are_open_floor() {
        // No surrounding wall: every tile on the map border (except
        // any building footprint, which the BORDER_MARGIN keeps away
        // from the border) must be Floor so the player can reach an
        // overworld edge exit.
        let bm = build_town();
        let w = bm.width;
        let h = bm.height;
        for x in 0..w {
            let top = bm.map.xy_idx(x, 0);
            let bot = bm.map.xy_idx(x, h - 1);
            assert_eq!(bm.map.tiles[top].terrain, TerrainType::Floor,
                "border ({}, 0) should be Floor", x);
            assert_eq!(bm.map.tiles[bot].terrain, TerrainType::Floor,
                "border ({}, {}) should be Floor", x, h - 1);
        }
        for y in 0..h {
            let left = bm.map.xy_idx(0, y);
            let right = bm.map.xy_idx(w - 1, y);
            assert_eq!(bm.map.tiles[left].terrain, TerrainType::Floor,
                "border (0, {}) should be Floor", y);
            assert_eq!(bm.map.tiles[right].terrain, TerrainType::Floor,
                "border ({}, {}) should be Floor", w - 1, y);
        }
    }

    #[test]
    fn town_records_buildings_as_rooms() {
        let bm = build_town();
        let rooms = bm.rooms.as_ref().expect("rooms set");
        // Layout aims for `TARGET_BUILDINGS` and skips overlaps; lower
        // bound is conservative so the test isn't flaky.
        assert!(rooms.len() >= 3, "expected at least 3 buildings, got {}", rooms.len());
    }

    #[test]
    fn town_path_from_start_to_every_edge_exists() {
        // The whole point of the rework: the player can reach every
        // edge of the map without being blocked by buildings.
        let bm = build_town();
        let start = bm.starting_position.unwrap();
        let w = bm.width;
        let h = bm.height;

        // BFS over walkable tiles from `start`.
        let mut visited = vec![false; (w * h) as usize];
        let mut queue = std::collections::VecDeque::new();
        let start_idx = bm.map.xy_idx(start.x, start.y);
        visited[start_idx] = true;
        queue.push_back((start.x, start.y));
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
                let nidx = bm.map.xy_idx(nx, ny);
                if visited[nidx] { continue; }
                if !is_walkable(bm.map.tiles[nidx]) { continue; }
                visited[nidx] = true;
                queue.push_back((nx, ny));
            }
        }

        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1),
                       (w / 2, 0), (w / 2, h - 1), (0, h / 2), (w - 1, h / 2)] {
            let idx = bm.map.xy_idx(x, y);
            assert!(visited[idx],
                "edge ({}, {}) not reachable from player start ({}, {})",
                x, y, start.x, start.y,
            );
        }
    }
}
