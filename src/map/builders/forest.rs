//! Forest builder — cellular automata over a `Grid<u8>` to carve an
//! organic forest layout, plus an east-west spine for the linear
//! roguelike stair flow.
//!
//! Two clearings are always carved: a western clearing (where the
//! `UpStairs` sits, mirroring the player's east-into-the-forest entry
//! from town) and an eastern clearing (where the `DownStairs` or the
//! `Amulet of Yendor` sits, depending on whether this is the final
//! floor). A 1-tile-wide corridor connects them through the map
//! centre — the player's natural east-bound path through the woods.
//! Cellular-automata trees fill the regions on either side, giving
//! the corridor an organic feel.
//!
//! CA parameters vary by depth so each forest floor has its own
//! character:
//!
//! - Forest 1 (outer woods): sparser trees, larger clearings, brighter.
//! - Forest 2 (deep woods): denser trees, gnarlier, more decay decorations.
//!
//! Connectivity is enforced by keeping only tiles reachable from the
//! west clearing, so the player can never wander into an isolated
//! pocket.

use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

use roguelike_engine::map::builders::algorithms::{
    Grid, cellular_automata_iteration, get_all_regions, randomize_grid,
};

const FLOOR_VAL: u8 = 1;
const WALL_VAL: u8 = 0;

const MIN_REGION_FRACTION: f32 = 0.25;
const MAX_CA_RETRIES: usize = 8;
/// Half-width of each end-clearing (3×3 carve when this is 1).
const END_CLEARING_HALF: i32 = 1;
/// Distance from the map's east/west border to the centre of the
/// end-clearings. 3 keeps the stair one tile inside the border-wall.
const CLEARING_INSET: i32 = 3;

// =====================================================================
// CA profile per forest depth
// =====================================================================

/// Returns `(initial_alive_percent, round_count)` tuned to depth.
/// Forest 1 is sparsest (open paths, easy to navigate); each subsequent
/// floor gets denser and gnarlier, culminating in Forest 4 — claustrophobic
/// woods where the temple entrance hides off-spine.
fn profile_for_depth(depth: i32) -> (i32, usize) {
    match depth {
        // Forest 1 (outer woods) — looser trees, larger clearings.
        1 => (50, 4),
        // Forest 2 — slightly denser, scrubbier underbrush.
        2 => (54, 4),
        // Forest 3 — deeper woods, canopy thickens.
        3 => (58, 5),
        // Forest 4 (deepest) — gnarly old-growth; temple entrance lurks
        // off the spine so the player must explore to find it.
        4 => (62, 5),
        // Beyond authored content — fall back to mid values.
        _ => (55, 5),
    }
}

// =====================================================================
// ForestTerrainBuilder — cellular automata + east-west spine.
// =====================================================================

pub struct ForestTerrainBuilder {
    pub birth_threshold: i32,
    pub survival_threshold: i32,
}

impl Default for ForestTerrainBuilder {
    fn default() -> Self {
        Self {
            birth_threshold: 5,
            survival_threshold: 4,
        }
    }
}

impl ForestTerrainBuilder {
    pub fn new() -> Box<Self> { Box::new(Self::default()) }
}

impl InitialMapBuilder for ForestTerrainBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let depth = build.map.depth;
        let (initial_alive_percent, round_count) = profile_for_depth(depth);
        let min_region_size = ((w * h) as f32 * MIN_REGION_FRACTION) as usize;

        // East-west spine: west clearing → corridor → east clearing.
        let mid_y = h / 2;
        let west_cx = CLEARING_INSET;
        let east_cx = w - 1 - CLEARING_INSET;

        // 1. Run CA until we get a healthy connected region OR exhaust retries.
        let mut grid: Grid<u8> = Grid::new(w, h, WALL_VAL);
        let mut attempts = 0;
        loop {
            attempts += 1;
            randomize_grid(&mut grid, initial_alive_percent, FLOOR_VAL, WALL_VAL);
            force_border_wall(&mut grid);
            for _ in 0..round_count {
                cellular_automata_iteration(
                    &mut grid,
                    self.birth_threshold,
                    self.survival_threshold,
                    FLOOR_VAL,
                    WALL_VAL,
                );
            }

            let regions = get_all_regions(&grid, FLOOR_VAL, WALL_VAL);
            let largest = regions.into_iter().max_by_key(|r| r.size);
            let biggest_size = largest.as_ref().map(|r| r.size).unwrap_or(0);

            if biggest_size >= min_region_size || attempts >= MAX_CA_RETRIES {
                if let Some(region) = largest {
                    for cell in grid.data.iter_mut() { *cell = WALL_VAL; }
                    for idx in region.tiles { grid.data[idx] = FLOOR_VAL; }
                }
                if biggest_size < min_region_size {
                    // Pathological CA fallback: just open the interior.
                    for y in 1..h - 1 {
                        for x in 1..w - 1 {
                            let idx = grid.xy_idx(x, y);
                            grid.data[idx] = FLOOR_VAL;
                        }
                    }
                }
                break;
            }
        }

        // 2. Force-carve the two end-clearings and the corridor between
        //    them. This guarantees the spine exists no matter what CA
        //    produced.
        carve_clearing(&mut grid, west_cx, mid_y, END_CLEARING_HALF);
        carve_clearing(&mut grid, east_cx, mid_y, END_CLEARING_HALF);
        carve_corridor(&mut grid, west_cx, east_cx, mid_y);
        force_border_wall(&mut grid);

        // 3. Final connectivity cull from the west clearing centre —
        //    everything not reachable from spawn becomes wall.
        keep_only_reachable_from(&mut grid, west_cx, mid_y);

        // 4. Stamp the grid onto the BuilderMap.
        for y in 0..h {
            for x in 0..w {
                let gidx = grid.xy_idx(x, y);
                let midx = build.map.xy_idx(x, y);
                let terrain = if grid.data[gidx] == FLOOR_VAL {
                    TerrainType::Floor
                } else {
                    TerrainType::Wall
                };
                build.map.tiles[midx] = Tile {
                    terrain,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }

        // Starting position = west clearing centre (the UpStairs spot).
        // The player lands here when arriving from the east stair on
        // the previous floor (town or Forest n-1).
        build.set_starting_position(Position { x: west_cx, y: mid_y });
    }
}

fn carve_clearing(grid: &mut Grid<u8>, cx: i32, cy: i32, half: i32) {
    let w = grid.width;
    let h = grid.height;
    for dy in -half..=half {
        for dx in -half..=half {
            let x = cx + dx;
            let y = cy + dy;
            if x <= 0 || y <= 0 || x >= w - 1 || y >= h - 1 { continue; }
            let idx = grid.xy_idx(x, y);
            grid.data[idx] = FLOOR_VAL;
        }
    }
}

fn carve_corridor(grid: &mut Grid<u8>, x_start: i32, x_end: i32, y: i32) {
    let w = grid.width;
    let h = grid.height;
    if y <= 0 || y >= h - 1 { return; }
    let (xa, xb) = if x_start < x_end { (x_start, x_end) } else { (x_end, x_start) };
    for x in xa..=xb {
        if x <= 0 || x >= w - 1 { continue; }
        let idx = grid.xy_idx(x, y);
        grid.data[idx] = FLOOR_VAL;
    }
}

// =====================================================================
// ForestStairsBuilder — stairs at the east + west ends of the spine.
// =====================================================================

pub struct ForestStairsBuilder;

impl ForestStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

/// Minimum vertical distance from the spine row for a "discoverable"
/// temple entrance — pushes it far enough off-corridor that the player
/// has to actually wander into the woods to find it.
const TEMPLE_ENTRANCE_MIN_DY: i32 = 6;

/// Last forest floor in the descent (the one hiding the temple
/// entrance). One below the temple itself.
fn deepest_forest_depth() -> i32 {
    crate::constants::MAX_FLOOR - 1
}

impl MetaMapBuilder for ForestStairsBuilder {
    // StructurePlacement (not Finalization): stairs *are* terrain placement,
    // and Spawning must run after stair tiles exist so the spawner can
    // skip them. See docs/design/SPAWNING.md §pipeline ordering.
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::StructurePlacement) }
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;
        let mid_y = h / 2;
        let west_cx = CLEARING_INSET;
        let east_cx = w - 1 - CLEARING_INSET;
        let depth = build.map.depth;
        let is_temple_entrance_floor = depth == deepest_forest_depth();

        // West clearing → UpStairs (where the player arrives).
        let up_idx = build.map.xy_idx(west_cx, mid_y);
        build.map.tiles[up_idx].terrain = TerrainType::UpStairs;
        build.map.tiles[up_idx].liquid = LiquidType::None;

        if is_temple_entrance_floor {
            // Forest 4: the temple entrance is hidden somewhere in the
            // woods, not at the predictable east clearing. Pick a random
            // walkable tile well off the east-west spine so the player
            // has to leave the corridor to find it.
            let pos = pick_temple_entrance_pos(build, west_cx, mid_y).unwrap_or(
                // Fallback: if no off-spine tile qualifies (pathological
                // CA), fall back to the east clearing so the floor is
                // still beatable.
                Point::new(east_cx, mid_y),
            );
            let idx = build.map.xy_idx(pos.x, pos.y);
            build.map.tiles[idx].terrain = TerrainType::DownStairs;
            build.map.tiles[idx].liquid = LiquidType::None;
            bevy::log::info!(
                "ForestStairsBuilder: temple-entrance DownStairs at ({}, {}) on floor {} (off-spine, |dy|={})",
                pos.x, pos.y, depth, (pos.y - mid_y).abs(),
            );
        } else {
            // Forest 1-3: DownStairs at the east clearing — standard
            // east-bound progression.
            let down_idx = build.map.xy_idx(east_cx, mid_y);
            build.map.tiles[down_idx].terrain = TerrainType::DownStairs;
            build.map.tiles[down_idx].liquid = LiquidType::None;
            bevy::log::info!(
                "ForestStairsBuilder: DownStairs at east clearing ({}, {}) on floor {}",
                east_cx, mid_y, depth,
            );
        }

        // Belt-and-suspenders: ensure the east clearing is reachable
        // from the west by walking east along the spine. The terrain
        // builder already carved this corridor, but rerun a Dijkstra
        // sanity check; if unreachable, force-carve the corridor again.
        let start_idx = build
            .map
            .point2d_to_index(Point::new(west_cx, mid_y));
        let mut walkable = vec![false; (w * h) as usize];
        for (idx, tile) in build.map.tiles.iter().enumerate() {
            walkable[idx] = !matches!(tile.terrain, TerrainType::Wall);
        }
        let dijkstra = DijkstraMap::new(
            w as usize,
            h as usize,
            &[start_idx],
            &WalkableMaskMap { walkable: &walkable, width: w as usize, height: h as usize },
            1024.0,
        );
        let east_idx = build.map.xy_idx(east_cx, mid_y);
        if dijkstra.map[east_idx] == f32::MAX {
            // Re-carve the spine on the live map. Rare path; just defends
            // against a CA + cull combo that severed the corridor.
            for x in west_cx..=east_cx {
                let idx = build.map.xy_idx(x, mid_y);
                if build.map.tiles[idx].terrain == TerrainType::Wall {
                    build.map.tiles[idx].terrain = TerrainType::Floor;
                }
            }
        }
    }
}

/// Pick a random walkable Floor tile that's well off the east-west
/// spine, reachable from the west clearing. Returns `None` only if
/// the map has no qualifying tile (pathological CA outcome).
fn pick_temple_entrance_pos(
    build: &mut BuilderMap,
    west_cx: i32,
    mid_y: i32,
) -> Option<Point> {
    let w = build.width;
    let h = build.height;

    // Reachability mask from the west clearing — we only consider
    // tiles the player can actually walk to.
    let start_idx = build.map.point2d_to_index(Point::new(west_cx, mid_y));
    let mut walkable = vec![false; (w * h) as usize];
    for (idx, tile) in build.map.tiles.iter().enumerate() {
        walkable[idx] = matches!(tile.terrain, TerrainType::Floor | TerrainType::UpStairs);
    }
    let dijkstra = DijkstraMap::new(
        w as usize,
        h as usize,
        &[start_idx],
        &WalkableMaskMap { walkable: &walkable, width: w as usize, height: h as usize },
        2048.0,
    );

    let mut candidates: Vec<Point> = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            if (y - mid_y).abs() < TEMPLE_ENTRANCE_MIN_DY {
                continue;
            }
            let idx = build.map.xy_idx(x, y);
            if !matches!(build.map.tiles[idx].terrain, TerrainType::Floor) {
                continue;
            }
            if dijkstra.map[idx] == f32::MAX {
                continue;
            }
            candidates.push(Point::new(x, y));
        }
    }

    if candidates.is_empty() {
        return None;
    }
    let pick = build.rng.range(0, candidates.len() as i32) as usize;
    Some(candidates[pick])
}

/// Minimal `BaseMap` adapter for the spine-reachability check.
struct WalkableMaskMap<'a> {
    walkable: &'a [bool],
    width: usize,
    height: usize,
}

impl<'a> bracket_lib::prelude::BaseMap for WalkableMaskMap<'a> {
    fn is_opaque(&self, _idx: usize) -> bool { false }
    fn get_available_exits(&self, idx: usize) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
        let mut exits = bracket_lib::prelude::SmallVec::new();
        let x = (idx % self.width) as i32;
        let y = (idx / self.width) as i32;
        for (dx, dy) in [(0_i32, 1_i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx as usize >= self.width || ny as usize >= self.height {
                continue;
            }
            let nidx = (ny as usize) * self.width + (nx as usize);
            if self.walkable[nidx] {
                exits.push((nidx, 1.0));
            }
        }
        exits
    }
}

impl<'a> Algorithm2D for WalkableMaskMap<'a> {
    fn dimensions(&self) -> Point {
        Point::new(self.width as i32, self.height as i32)
    }
}

// =====================================================================
// Connectivity helpers
// =====================================================================

fn force_border_wall(grid: &mut Grid<u8>) {
    let w = grid.width;
    let h = grid.height;
    for x in 0..w {
        let top = grid.xy_idx(x, 0);
        grid.data[top] = WALL_VAL;
        let bot = grid.xy_idx(x, h - 1);
        grid.data[bot] = WALL_VAL;
    }
    for y in 0..h {
        let left = grid.xy_idx(0, y);
        grid.data[left] = WALL_VAL;
        let right = grid.xy_idx(w - 1, y);
        grid.data[right] = WALL_VAL;
    }
}

fn keep_only_reachable_from(grid: &mut Grid<u8>, cx: i32, cy: i32) {
    let w = grid.width;
    let h = grid.height;
    let total = (w * h) as usize;
    let mut reachable = vec![false; total];
    let mut queue = std::collections::VecDeque::new();
    let start_idx = grid.xy_idx(cx, cy);
    if grid.data[start_idx] != FLOOR_VAL { return; }
    reachable[start_idx] = true;
    queue.push_back((cx, cy));
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
            let nidx = grid.xy_idx(nx, ny);
            if reachable[nidx] { continue; }
            if grid.data[nidx] == FLOOR_VAL {
                reachable[nidx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    for (idx, cell) in grid.data.iter_mut().enumerate() {
        if *cell == FLOOR_VAL && !reachable[idx] {
            *cell = WALL_VAL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_forest(depth: i32) -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(80, 60);
        bm.map.depth = depth;
        ForestTerrainBuilder::default().build_map(&mut bm);
        bm
    }

    #[test]
    fn forest_starting_position_at_west_clearing() {
        let bm = build_forest(1);
        let start = bm.starting_position.expect("forest must set starting_position");
        assert_eq!(start.x, CLEARING_INSET, "start x must be at west clearing");
        assert_eq!(start.y, bm.height / 2);
        let idx = bm.map.xy_idx(start.x, start.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
    }

    #[test]
    fn forest_stairs_at_west_and_east() {
        let mut bm = build_forest(1); // floor 1 — east-clearing DownStairs
        ForestStairsBuilder.build_map(&mut bm);
        let west_idx = bm.map.xy_idx(CLEARING_INSET, bm.height / 2);
        let east_idx = bm.map.xy_idx(bm.width - 1 - CLEARING_INSET, bm.height / 2);
        assert_eq!(bm.map.tiles[west_idx].terrain, TerrainType::UpStairs);
        assert_eq!(bm.map.tiles[east_idx].terrain, TerrainType::DownStairs);
    }

    /// Forest 4 (deepest forest floor) hides the temple-entrance DownStairs
    /// off the east-west spine. The east clearing stays as a regular Floor
    /// tile so the player can't just walk straight east to the temple —
    /// they have to wander to discover the entrance.
    #[test]
    fn forest_4_places_temple_entrance_off_spine() {
        let deepest = crate::constants::MAX_FLOOR - 1;
        let mut bm = build_forest(deepest);
        ForestStairsBuilder.build_map(&mut bm);
        let mid_y = bm.height / 2;
        let east_idx = bm.map.xy_idx(bm.width - 1 - CLEARING_INSET, mid_y);
        assert_ne!(
            bm.map.tiles[east_idx].terrain,
            TerrainType::DownStairs,
            "Forest 4 east clearing must NOT host the DownStairs — entrance is off-spine",
        );
        // Exactly one DownStairs tile, and it's far enough from the spine
        // to feel "discovered" rather than handed to the player.
        let mut entrances = Vec::new();
        for y in 0..bm.height {
            for x in 0..bm.width {
                let idx = bm.map.xy_idx(x, y);
                if bm.map.tiles[idx].terrain == TerrainType::DownStairs {
                    entrances.push((x, y));
                }
            }
        }
        assert_eq!(
            entrances.len(),
            1,
            "Forest 4 must place exactly one temple-entrance DownStairs",
        );
        let (_, ey) = entrances[0];
        // Allow the fallback at the east clearing too (very rare CA outcome);
        // otherwise demand |dy| >= TEMPLE_ENTRANCE_MIN_DY.
        let dy = (ey - mid_y).abs();
        assert!(
            dy >= TEMPLE_ENTRANCE_MIN_DY || dy == 0,
            "temple entrance should be off-spine, got |dy|={dy}",
        );
    }

    /// The deepest forest floor must NOT spawn the amulet — that's the
    /// temple's job now.
    #[test]
    fn forest_4_does_not_spawn_amulet() {
        let deepest = crate::constants::MAX_FLOOR - 1;
        let mut bm = build_forest(deepest);
        ForestStairsBuilder.build_map(&mut bm);
        let amulet_count = bm
            .item_spawn_list
            .iter()
            .filter(|(_, name, _)| name == "Amulet of Yendor")
            .count();
        assert_eq!(amulet_count, 0, "amulet lives in the temple, not Forest 4");
    }

    #[test]
    fn forest_west_to_east_is_walkable() {
        // Sanity: walking east-west along the spine y=h/2 should stay
        // on Floor (or Stairs) tiles all the way across.
        let mut bm = build_forest(1);
        ForestStairsBuilder.build_map(&mut bm);
        let mid_y = bm.height / 2;
        for x in CLEARING_INSET..=(bm.width - 1 - CLEARING_INSET) {
            let idx = bm.map.xy_idx(x, mid_y);
            let terrain = bm.map.tiles[idx].terrain;
            assert!(
                matches!(terrain, TerrainType::Floor | TerrainType::UpStairs | TerrainType::DownStairs),
                "spine tile ({}, {}) must be walkable, got {:?}", x, mid_y, terrain,
            );
        }
    }

    /// Forest 1 and Forest 2 use different CA profiles. We can't
    /// assert *which* is denser on any single seed, but we can at
    /// least check that both depths produce a valid spine.
    #[test]
    fn forest_2_carving_produces_walkable_spine() {
        let mut bm = build_forest(2);
        ForestStairsBuilder.build_map(&mut bm);
        let mid_y = bm.height / 2;
        for x in CLEARING_INSET..=(bm.width - 1 - CLEARING_INSET) {
            let idx = bm.map.xy_idx(x, mid_y);
            let terrain = bm.map.tiles[idx].terrain;
            assert!(
                matches!(terrain, TerrainType::Floor | TerrainType::UpStairs | TerrainType::DownStairs),
                "deep-forest spine tile ({}, {}) must be walkable, got {:?}", x, mid_y, terrain,
            );
        }
    }

    // =================================================================
    // Interior-opaque leak probe — the actual user-reported symptom.
    //
    // The 5x7-block pin test in src/game/systems.rs proves bracket-lib
    // hides interior walls when the cluster is a uniform dense block.
    // The real forest is a CA-generated sparse field with concave bays
    // and single-tile peninsulas. This test runs FOV from the player's
    // actual spawn point on a real forest map and asserts that NO tile
    // in visible_tiles is "interior opaque" (opaque with every in-bounds
    // 8-neighbor also opaque). If it fires, the pin test was insufficient
    // — bracket-lib's shadowcasting is leaking interior opaque tiles
    // into the visible set on irregular shapes, and the reverted
    // `cull_interior_opaque_from_fov` system was the right defense.
    //
    // Off-map neighbors count as opaque (a corner wall with 3 in-bounds
    // wall neighbors is still interior — no ray could reach it).
    // =================================================================

    use crate::map::tile::is_opaque;

    fn is_interior_opaque(map: &crate::map::map::Map, x: i32, y: i32) -> bool {
        let idx = map.xy_idx(x, y);
        if idx >= map.tiles.len() || !is_opaque(map.tiles[idx]) {
            return false;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                    // Off-map = opaque. Don't disqualify.
                    continue;
                }
                let nidx = map.xy_idx(nx, ny);
                if nidx < map.tiles.len() && !is_opaque(map.tiles[nidx]) {
                    return false;
                }
            }
        }
        true
    }

    /// Run FOV on a real forest map (every depth × many seeds) from
    /// the player's spawn point. Fail if any opaque tile fully boxed
    /// in by other opaque tiles appears in visible_tiles.
    #[test]
    fn forest_fov_does_not_leak_interior_opaque_trees() {
        use bracket_lib::prelude::{field_of_view_set, Point};

        const VIEWSHED_RANGE: i32 = 20; // matches assets/player.ron
        const SEEDS_PER_DEPTH: usize = 20;

        let mut total_leaks = 0_usize;
        let mut sample_leak: Option<(i32, Point, usize)> = None; // (depth, tile, count)

        for depth in 1..=4_i32 {
            for seed_iter in 0..SEEDS_PER_DEPTH {
                let bm = build_forest(depth);
                let start = bm
                    .starting_position
                    .expect("forest must set starting_position");
                let player_pt = Point::new(start.x, start.y);

                let visible = field_of_view_set(player_pt, VIEWSHED_RANGE, &bm.map);

                let leaks: Vec<Point> = visible
                    .iter()
                    .copied()
                    .filter(|p| is_interior_opaque(&bm.map, p.x, p.y))
                    .collect();

                if !leaks.is_empty() {
                    total_leaks += leaks.len();
                    if sample_leak.is_none() {
                        sample_leak = Some((depth, leaks[0], leaks.len()));
                    }
                    eprintln!(
                        "forest depth={} seed_iter={} leaked {} interior-opaque tiles into FOV; sample: {:?}",
                        depth, seed_iter, leaks.len(), &leaks[..leaks.len().min(5)],
                    );
                }
            }
        }

        assert_eq!(
            total_leaks, 0,
            "bracket-lib leaked interior-opaque tiles into visible_tiles across forest depths. \
             First sample: depth={:?}, tile={:?}, total in that FOV={:?}. \
             This validates the reverted cull_interior_opaque_from_fov system — bracket-lib's \
             shadowcasting does NOT correctly hide opaque tiles fully boxed in by opaque tiles \
             on irregular CA-generated maps. The 5x7-block pin test in src/game/systems.rs was \
             checking a uniform fortress shape and missed this case.",
            sample_leak.map(|(d, _, _)| d),
            sample_leak.map(|(_, p, _)| p),
            sample_leak.map(|(_, _, c)| c),
        );
    }

    /// Walk the spine west→east one tile at a time, accumulate every
    /// FOV's visible_tiles into a simulated `explored_tiles` set
    /// (exactly what `fov_update_system` does for an entity with
    /// `FovRevealsMap`), then check if any interior-opaque tile ended
    /// up explored. This is the closest in-process reproduction of the
    /// player's actual experience: by the time they reach the east
    /// clearing, dozens of FOV computations have been ORed together.
    #[test]
    fn walking_spine_does_not_explore_any_interior_opaque_tile() {
        use bracket_lib::prelude::{field_of_view_set, Point};

        const VIEWSHED_RANGE: i32 = 20;
        const SEEDS_PER_DEPTH: usize = 10;

        let mut total_explored_interior = 0_usize;
        let mut sample: Option<(i32, Point)> = None;

        for depth in 1..=4_i32 {
            for _ in 0..SEEDS_PER_DEPTH {
                let bm = build_forest(depth);
                let mid_y = bm.height / 2;
                let west_cx = CLEARING_INSET;
                let east_cx = bm.width - 1 - CLEARING_INSET;

                let total_cells = (bm.width * bm.height) as usize;
                let mut explored = vec![false; total_cells];

                for x in west_cx..=east_cx {
                    let idx = bm.map.xy_idx(x, mid_y);
                    if !matches!(
                        bm.map.tiles[idx].terrain,
                        TerrainType::Floor | TerrainType::UpStairs | TerrainType::DownStairs
                    ) {
                        // Spine should be walkable; if not, stop the walk.
                        break;
                    }
                    let visible = field_of_view_set(Point::new(x, mid_y), VIEWSHED_RANGE, &bm.map);
                    for pt in visible.iter() {
                        if bm.map.in_bounds(*pt) {
                            explored[bm.map.xy_idx(pt.x, pt.y)] = true;
                        }
                    }
                }

                for idx in 0..total_cells {
                    if !explored[idx] {
                        continue;
                    }
                    let (x, y) = bm.map.idx_xy(idx);
                    if is_interior_opaque(&bm.map, x, y) {
                        total_explored_interior += 1;
                        if sample.is_none() {
                            sample = Some((depth, Point::new(x, y)));
                        }
                    }
                }
            }
        }

        assert_eq!(
            total_explored_interior, 0,
            "Walking the spine explored {} interior-opaque tiles across 40 forest maps. \
             Sample: depth={:?}, tile={:?}. This is the smoking gun — even though a single \
             FOV call doesn't leak (see forest_fov_does_not_leak_*), the OR of many calls \
             does. Restore the cull system OR investigate why bracket-lib is octant-dependent.",
            total_explored_interior, sample.map(|(d, _)| d), sample.map(|(_, p)| p),
        );
    }

    /// Exhaustive sweep: for each forest map, run FOV from a random
    /// sample of walkable cells. This catches positional bugs where
    /// only certain octant alignments leak.
    #[test]
    fn forest_fov_from_random_positions_does_not_leak_interior_opaque() {
        use bracket_lib::prelude::{field_of_view_set, Point, RandomNumberGenerator};

        const VIEWSHED_RANGE: i32 = 20;
        const MAPS_PER_DEPTH: usize = 5;
        const SAMPLES_PER_MAP: usize = 200;

        let mut total_explored_interior = 0_usize;
        let mut sample_leak: Option<(i32, Point, Point)> = None; // (depth, origin, leaked_tile)

        let mut rng = RandomNumberGenerator::new();

        for depth in 1..=4_i32 {
            for _ in 0..MAPS_PER_DEPTH {
                let bm = build_forest(depth);

                let walkable: Vec<(i32, i32)> = (0..bm.height)
                    .flat_map(|y| (0..bm.width).map(move |x| (x, y)))
                    .filter(|&(x, y)| {
                        let idx = bm.map.xy_idx(x, y);
                        matches!(bm.map.tiles[idx].terrain, TerrainType::Floor)
                    })
                    .collect();

                if walkable.is_empty() {
                    continue;
                }

                for _ in 0..SAMPLES_PER_MAP {
                    let pick = rng.range(0, walkable.len() as i32) as usize;
                    let (ox, oy) = walkable[pick];
                    let origin = Point::new(ox, oy);

                    let visible = field_of_view_set(origin, VIEWSHED_RANGE, &bm.map);
                    for pt in visible.iter() {
                        if !bm.map.in_bounds(*pt) {
                            continue;
                        }
                        if is_interior_opaque(&bm.map, pt.x, pt.y) {
                            total_explored_interior += 1;
                            if sample_leak.is_none() {
                                sample_leak = Some((depth, origin, *pt));
                            }
                        }
                    }
                }
            }
        }

        assert_eq!(
            total_explored_interior, 0,
            "Random-origin FOV sweep across {} forests leaked {} interior-opaque tiles. \
             First leak: depth={:?}, origin={:?}, leaked_tile={:?}.",
            4 * MAPS_PER_DEPTH, total_explored_interior,
            sample_leak.map(|(d, _, _)| d),
            sample_leak.map(|(_, o, _)| o),
            sample_leak.map(|(_, _, t)| t),
        );
    }

    /// Same test but from the east clearing — the player walks the spine,
    /// so FOV from both ends matters.
    #[test]
    fn forest_fov_does_not_leak_interior_opaque_from_east_clearing() {
        use bracket_lib::prelude::{field_of_view_set, Point};

        const VIEWSHED_RANGE: i32 = 20;
        const SEEDS_PER_DEPTH: usize = 20;

        let mut total_leaks = 0_usize;
        let mut sample_leak: Option<(i32, Point)> = None;

        for depth in 1..=4_i32 {
            for _ in 0..SEEDS_PER_DEPTH {
                let bm = build_forest(depth);
                let east_cx = bm.width - 1 - CLEARING_INSET;
                let mid_y = bm.height / 2;
                let player_pt = Point::new(east_cx, mid_y);

                let visible = field_of_view_set(player_pt, VIEWSHED_RANGE, &bm.map);

                let leaks: Vec<Point> = visible
                    .iter()
                    .copied()
                    .filter(|p| is_interior_opaque(&bm.map, p.x, p.y))
                    .collect();

                if !leaks.is_empty() {
                    total_leaks += leaks.len();
                    if sample_leak.is_none() {
                        sample_leak = Some((depth, leaks[0]));
                    }
                }
            }
        }

        assert_eq!(
            total_leaks, 0,
            "FOV from east clearing leaked interior-opaque tiles. Sample: {:?}",
            sample_leak,
        );
    }
}
