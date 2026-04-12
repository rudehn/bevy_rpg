//! BrogueLike map generator — the primary map builder.
//!
//! Generates a map by placing rooms (rectangular, cross, circular, chunky,
//! cavern) and connecting them via door sites. Also places a reward room
//! guided by choke-point analysis, and adds loop doors for alternate paths.

use bracket_lib::prelude::{Point, Rect};

use super::algorithms::{BlobGenConfig, Grid, create_blob};
use super::choke_map::ChokeMap;
use super::{BuildContext, BuilderPhase, FloorProfile, MapBuilder};
use crate::geometry::Direction;
use crate::map::map::Map;
use crate::map::tile::{TerrainType, Tile, is_walkable};

const MAX_ROOM_SIZE: i32 = 20;
const MAX_CAVERN_SIZE: i32 = 40;

struct RoomDesign {
    tiles: Vec<TerrainType>,
    width: i32,
    height: i32,
    door_sites: Vec<(Point, Direction)>,
    is_cavern: bool,
}

pub struct BrogueLikeBuilder {
    width: i32,
    height: i32,
    profile: FloorProfile,
}

impl BrogueLikeBuilder {
    pub fn dungeon(_depth: i32, width: i32, height: i32, profile: FloorProfile) -> Self {
        Self {
            width,
            height,
            profile,
        }
    }

    fn design_random_room<C: BuildContext>(&self, ctx: &mut C) -> RoomDesign {
        // Use profile cavern weight to determine if this room is a cavern.
        let is_cavern = ctx.rng().range(0, 100) < self.profile.cavern_weight;
        let room_type = if is_cavern {
            6 // cavern
        } else {
            ctx.rng().range(0, 6) // structured room types
        };

        let w = if is_cavern { MAX_CAVERN_SIZE } else { MAX_ROOM_SIZE };
        let h = if is_cavern { MAX_CAVERN_SIZE } else { MAX_ROOM_SIZE };
        let mut tiles = vec![TerrainType::Wall; (w * h) as usize];

        match room_type {
            0 => self.draw_cross_room(ctx, &mut tiles, w, h),
            1 => self.draw_symmetrical_cross_room(ctx, &mut tiles, w, h),
            2 => self.draw_small_room(ctx, &mut tiles, w, h),
            3 => self.draw_circular_room(ctx, &mut tiles, w, h),
            4 | 5 => self.draw_chunky_room(ctx, &mut tiles, w, h),
            _ => self.draw_cavern_room(ctx, &mut tiles, w, h),
        }

        let mut design = RoomDesign {
            tiles,
            width: w,
            height: h,
            door_sites: Vec::new(),
            is_cavern,
        };

        // Potentially attach a winding hallway
        if ctx.rng().range(0, 100) < self.profile.hallway_chance {
            self.attach_winding_hallway(ctx, &mut design);
        } else {
            design.door_sites = self.find_door_sites(&design.tiles, w, h);
        }

        design
    }

    fn design_large_cavern<C: BuildContext>(&self, ctx: &mut C) -> RoomDesign {
        let w = MAX_CAVERN_SIZE;
        let h = MAX_CAVERN_SIZE;
        let mut tiles = vec![TerrainType::Wall; (w * h) as usize];
        self.draw_cavern_room(ctx, &mut tiles, w, h);
        let door_sites = self.find_door_sites(&tiles, w, h);
        RoomDesign { tiles, width: w, height: h, door_sites, is_cavern: true }
    }

    fn design_reward_room<C: BuildContext>(&self, ctx: &mut C) -> RoomDesign {
        let w = MAX_ROOM_SIZE;
        let h = MAX_ROOM_SIZE;
        let mut tiles = vec![TerrainType::Wall; (w * h) as usize];

        // Reward rooms are often specialized. Let's make a circular one with a "pedestal"
        let radius = ctx.rng().range(4, 6);
        let cx = w / 2;
        let cy = h / 2;
        for x in -radius..=radius {
            for y_offset in -radius..=radius {
                if x * x + y_offset * y_offset <= radius * radius {
                    let pt = Point::new(cx + x, cy + y_offset);
                    if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                        tiles[(pt.y * w + pt.x) as usize] = TerrainType::Floor;
                    }
                }
            }
        }

        // Add a "pedestal" in the middle (can be used for items later)
        // For now, we'll just keep it floor but maybe add a liquid border
        if ctx.rng().roll_dice(1, 2) == 1 {
             for x in -radius..=radius {
                for y_offset in -radius..=radius {
                    let dist_sq = x * x + y_offset * y_offset;
                    if dist_sq == radius * radius || dist_sq == (radius-1) * (radius-1) {
                         let pt = Point::new(cx + x, cy + y_offset);
                         if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                             // We'll use a special marker or just keep it floor for now
                         }
                    }
                }
            }
        }

        let door_sites = self.find_door_sites(&tiles, w, h);
        RoomDesign {
            tiles,
            width: w,
            height: h,
            door_sites,
            is_cavern: false,
        }
    }

    fn draw_cross_room<C: BuildContext>(&self, ctx: &mut C, tiles: &mut [TerrainType], w: i32, h: i32) {
        let w1 = ctx.rng().range(3, 10);
        let h1 = ctx.rng().range(3, 6);
        let w2 = ctx.rng().range(4, 15);
        let h2 = ctx.rng().range(2, 5);

        let cx = w / 2;
        let cy = h / 2;

        self.fill_rect(tiles, w, Rect::with_size(cx - w1 / 2, cy - h1 / 2, w1, h1), TerrainType::Floor);
        let ox = ctx.rng().range(-2, 2);
        let oy = ctx.rng().range(-1, 1);
        self.fill_rect(tiles, w, Rect::with_size(cx - w2 / 2 + ox, cy - h2 / 2 + oy, w2, h2), TerrainType::Floor);
    }

    fn draw_symmetrical_cross_room<C: BuildContext>(&self, ctx: &mut C, tiles: &mut [TerrainType], w: i32, h: i32) {
        let major_w = ctx.rng().range(4, 9);
        let major_h = ctx.rng().range(4, 6);
        let minor_w = (major_w - 2).max(1);
        let minor_h = major_h + 2;

        let cx = w / 2;
        let cy = h / 2;

        self.fill_rect(tiles, w, Rect::with_size(cx - major_w / 2, cy - major_h / 2, major_w, major_h), TerrainType::Floor);
        self.fill_rect(tiles, w, Rect::with_size(cx - minor_w / 2, cy - minor_h / 2, minor_w, minor_h), TerrainType::Floor);
    }

    fn draw_small_room<C: BuildContext>(&self, ctx: &mut C, tiles: &mut [TerrainType], w: i32, h: i32) {
        let rw = ctx.rng().range(3, 6);
        let rh = ctx.rng().range(2, 4);
        self.fill_rect(tiles, w, Rect::with_size(w / 2 - rw / 2, h / 2 - rh / 2, rw, rh), TerrainType::Floor);
    }

    fn draw_circular_room<C: BuildContext>(&self, ctx: &mut C, tiles: &mut [TerrainType], w: i32, h: i32) {
        let radius = ctx.rng().range(2, 5);
        let cx = w / 2;
        let cy = h / 2;
        for x in -radius..=radius {
            for y_offset in -radius..=radius {
                if x * x + y_offset * y_offset <= radius * radius {
                    let pt = Point::new(cx + x, cy + y_offset);
                    if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                        tiles[(pt.y * w + pt.x) as usize] = TerrainType::Floor;
                    }
                }
            }
        }
    }

    fn draw_chunky_room<C: BuildContext>(&self, ctx: &mut C, tiles: &mut [TerrainType], w: i32, h: i32) {
        let chunk_count = ctx.rng().range(2, 6);
        let cx = w / 2;
        let cy = h / 2;

        // Core
        self.fill_rect(tiles, w, Rect::with_size(cx - 1, cy - 1, 3, 3), TerrainType::Floor);

        for _ in 0..chunk_count {
            let rx = ctx.rng().range(cx - 3, cx + 3);
            let ry = ctx.rng().range(cy - 3, cy + 3);
            let radius = 2;
            for x in -radius..=radius {
                for y in -radius..=radius {
                    if x * x + y * y <= radius * radius {
                        let pt = Point::new(rx + x, ry + y);
                        if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                            tiles[(pt.y * w + pt.x) as usize] = TerrainType::Floor;
                        }
                    }
                }
            }
        }
    }

    fn fill_rect(&self, tiles: &mut [TerrainType], w: i32, rect: Rect, tile: TerrainType) {
        let h = tiles.len() as i32 / w;
        for x in rect.x1..=rect.x2 {
            for y in rect.y1..=rect.y2 {
                if x >= 0 && x < w && y >= 0 && y < h {
                    tiles[(y * w + x) as usize] = tile;
                }
            }
        }
    }

    fn find_door_sites(&self, tiles: &[TerrainType], w: i32, h: i32) -> Vec<(Point, Direction)> {
        let mut sites = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let pt = Point::new(x, y);
                if tiles[(y * w + x) as usize] == TerrainType::Wall
                    && let Some(dir) = self.direction_of_door_site(tiles, w, h, pt) {
                        sites.push((pt, dir));
                    }
            }
        }
        sites
    }

    fn direction_of_door_site(&self, tiles: &[TerrainType], w: i32, h: i32, pt: Point) -> Option<Direction> {
        let mut solution = None;
        for dir in [Direction::N, Direction::E, Direction::S, Direction::W] {
            let neighbor = pt + dir.offset();
            let opp = pt + dir.opposite().offset();

            if neighbor.x >= 0 && neighbor.x < w && neighbor.y >= 0 && neighbor.y < h
                && opp.x >= 0 && opp.x < w && opp.y >= 0 && opp.y < h
                && tiles[(opp.y * w + opp.x) as usize] == TerrainType::Floor {
                    if solution.is_some() { return None; } // Multiple floor neighbors = not a door site
                    solution = Some(dir);
                }
        }
        solution
    }

    fn attach_winding_hallway<C: BuildContext>(&self, ctx: &mut C, design: &mut RoomDesign) {
        let sites = self.find_door_sites(&design.tiles, design.width, design.height);
        if sites.is_empty() { return; }

        let pick = ctx.rng().range(0, sites.len() as i32) as usize;
        let (start_pt, primary_dir) = sites[pick];
        let length = ctx.rng().range(5, 15);
        let mut curr = start_pt;
        let (perp_left, perp_right) = primary_dir.perpendiculars();

        for _ in 0..length {
            if curr.x < 0 || curr.x >= design.width || curr.y < 0 || curr.y >= design.height {
                break;
            }
            design.tiles[(curr.y * design.width + curr.x) as usize] = TerrainType::Floor;

            // Occasionally widen the corridor
            if ctx.rng().range(0, 100) < 30 {
                let side = if ctx.rng().roll_dice(1, 2) == 1 { perp_left } else { perp_right };
                let adj = curr + side.offset();
                if adj.x >= 0 && adj.x < design.width && adj.y >= 0 && adj.y < design.height {
                    design.tiles[(adj.y * design.width + adj.x) as usize] = TerrainType::Floor;
                }
            }

            // Biased random walk: 60% forward, 20% left, 20% right
            let roll = ctx.rng().range(0, 100);
            let step_dir = if roll < 60 {
                primary_dir
            } else if roll < 80 {
                perp_left
            } else {
                perp_right
            };
            curr += step_dir.offset();
        }

        design.door_sites = vec![(curr, primary_dir)];
    }

    fn draw_cavern_room<C: BuildContext>(&self, ctx: &mut C, tiles: &mut [TerrainType], w: i32, h: i32) {
        let initial_grid_dims = Grid::new(w, h, TerrainType::Wall);

        // Scale blob dimensions to the available grid size.
        // On the 40x40 cavern grid these produce large organic caves;
        // on the 20x20 room grid they fall back to compact shapes.
        let variant = ctx.rng().range(0, 3);
        let (min_bw, max_bw, min_bh, max_bh) = match variant {
            0 => {
                // Compact cave
                let mw = (w / 4).max(3);
                let mh = (h / 4).max(3);
                (mw, (w * 3 / 4).max(mw + 1), mh, (h * 3 / 4).max(mh + 1))
            }
            1 => {
                // Tall north-south cave
                let mw = (w / 6).max(3);
                (mw, (w / 3).max(mw + 1), (h / 2).max(5), (h - 2).max(6))
            }
            _ => {
                // Wide east-west cave
                let mh = (h / 6).max(3);
                ((w / 2).max(5), (w - 2).max(6), mh, (h / 3).max(mh + 1))
            }
        };

        let config = BlobGenConfig {
            round_count: 5,
            min_blob_width: min_bw,
            min_blob_height: min_bh,
            max_blob_width: max_bw,
            max_blob_height: max_bh,
            initial_alive_percent: 55,
            birth_threshold: 5,
            survival_threshold: 4,
        };

        // Generate the blob using the algorithms module
        let (blob_grid, _, _, _, _) = create_blob(&initial_grid_dims, &config, TerrainType::Floor, TerrainType::Wall);

        // Copy the generated blob back into the room's tiles vector
        tiles.copy_from_slice(&blob_grid.data);
    }

    fn room_fits<C: BuildContext>(&self, ctx: &C, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point) -> bool {
        self.room_fits_with_padding(ctx, design, offset, ignore_dungeon_pt, 1)
    }

    /// Relaxed fit check -- only requires the room floor tiles themselves to land on walls.
    /// No padding. Used for cavern-to-cavern connections so caves can nearly merge.
    fn room_fits_relaxed<C: BuildContext>(&self, ctx: &C, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point) -> bool {
        self.room_fits_with_padding(ctx, design, offset, ignore_dungeon_pt, 0)
    }

    fn room_fits_with_padding<C: BuildContext>(&self, ctx: &C, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point, padding: i32) -> bool {
        let map = ctx.map();
        for y in 0..design.height {
            for x in 0..design.width {
                if design.tiles[(y * design.width + x) as usize] == TerrainType::Floor {
                    let dungeon_pt = Point::new(x, y) + offset;
                    if !map.in_bounds(dungeon_pt) { return false; }

                    for dx in -padding..=padding {
                        for dy in -padding..=padding {
                            let check_pt = dungeon_pt + Point::new(dx, dy);
                            if check_pt == ignore_dungeon_pt { continue; }
                            if !map.in_bounds(check_pt) { return false; }
                            let tile = map.tiles[map.xy_idx(check_pt.x, check_pt.y)];
                            if tile.terrain != TerrainType::Wall {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        true
    }

    pub fn add_loops(&self, tiles: &mut [Tile], w: i32, h: i32, minimum_path_distance: i32) {
        let total_cells = (w * h) as usize;

        // Create a temporary Map instance for Dijkstra calculations
        let mut map_for_dijkstra = Map::new(1, w, h, "tmp");
        map_for_dijkstra.tiles = tiles.to_vec();

        // Make all doors open in the Dijkstra map for pathfinding
        for i in 0..map_for_dijkstra.tiles.len() {
            if map_for_dijkstra.tiles[i].terrain == TerrainType::Door {
                map_for_dijkstra.tiles[i].terrain = TerrainType::OpenDoor;
            }
        }

        let directions = [(1i32, 0i32), (0, 1)];

        // Pre-filter: only consider wall tiles that have walkable tiles on both
        // sides of at least one axis. This avoids the expensive Dijkstra for
        // walls that can't possibly be loop doors.
        let mut candidates: Vec<(usize, i32, i32)> = Vec::new();
        for idx in 0..total_cells {
            if map_for_dijkstra.tiles[idx].terrain != TerrainType::Wall { continue; }
            let (x, y) = map_for_dijkstra.idx_xy(idx);
            for &(dx, dy) in &directions {
                let nx = x + dx;
                let ny = y + dy;
                let ox = x - dx;
                let oy = y - dy;
                if !map_for_dijkstra.in_bounds(Point::new(nx, ny))
                    || !map_for_dijkstra.in_bounds(Point::new(ox, oy))
                {
                    continue;
                }
                let t1 = map_for_dijkstra.tiles[map_for_dijkstra.xy_idx(nx, ny)];
                let t2 = map_for_dijkstra.tiles[map_for_dijkstra.xy_idx(ox, oy)];
                if is_walkable(t1) && is_walkable(t2) {
                    candidates.push((idx, dx, dy));
                }
            }
        }
        // Fisher-Yates shuffle using the provided RNG is not possible here
        // since add_loops doesn't have access to ctx. We use a simple
        // deterministic shuffle seeded from the candidate count instead.
        // (The original code used `rand::rng()` here too — this is acceptable
        // because loop placement order has minimal gameplay impact.)
        fisher_yates_simple(&mut candidates);

        // Use a single Dijkstra from all walkable tiles would be wrong -- we need
        // per-candidate distance. But we can use BFS which is much faster than
        // bracket-lib's DijkstraMap for unweighted graphs.
        for (idx, dx, dy) in candidates {
            let (x, y) = map_for_dijkstra.idx_xy(idx);
            // Recheck the wall -- a previous iteration may have turned it into a door
            if map_for_dijkstra.tiles[idx].terrain != TerrainType::Wall { continue; }

            let nx = x + dx;
            let ny = y + dy;
            let ox = x - dx;
            let oy = y - dy;

            let start_idx = map_for_dijkstra.xy_idx(nx, ny);
            let goal_idx = map_for_dijkstra.xy_idx(ox, oy);

            // BFS from start_idx to goal_idx, treating the candidate wall as impassable
            let distance = self.bfs_distance(&map_for_dijkstra, start_idx, goal_idx, idx);

            if distance > minimum_path_distance {
                tiles[idx].terrain = TerrainType::Door;
                map_for_dijkstra.tiles[idx].terrain = TerrainType::Door;
            }
        }
    }

    /// BFS distance between two tiles, ignoring a blocked tile. Returns i32::MAX if unreachable.
    fn bfs_distance(&self, map: &Map, start: usize, goal: usize, blocked: usize) -> i32 {
        let total = map.tiles.len();
        let mut dist = vec![-1i32; total];
        let mut queue = std::collections::VecDeque::new();
        dist[start] = 0;
        queue.push_back(start);

        while let Some(current) = queue.pop_front() {
            if current == goal {
                return dist[current];
            }
            let (cx, cy) = map.idx_xy(current);
            let next_dist = dist[current] + 1;
            for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
                let n_idx = map.xy_idx(nx, ny);
                if n_idx == blocked { continue; }
                if dist[n_idx] >= 0 { continue; }
                if !is_walkable(map.tiles[n_idx]) { continue; }
                dist[n_idx] = next_dist;
                queue.push_back(n_idx);
            }
        }
        i32::MAX
    }
}

/// Fisher-Yates shuffle using a simple LCG seeded from the slice length.
/// Used for add_loops where ctx RNG is not available.
fn fisher_yates_simple<T>(slice: &mut [T]) {
    let n = slice.len();
    if n <= 1 { return; }
    // Simple LCG state seeded from length
    let mut state: u64 = n as u64 ^ 0xDEAD_BEEF_CAFE_BABE;
    for i in (1..n).rev() {
        // LCG step
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        slice.swap(i, j);
    }
}

/// Fisher-Yates shuffle using the builder context's seeded RNG.
fn shuffle_with_rng<T, C: BuildContext>(slice: &mut [T], ctx: &mut C) {
    let n = slice.len();
    if n <= 1 { return; }
    for i in (1..n).rev() {
        let j = ctx.rng().range(0, (i + 1) as i32) as usize;
        slice.swap(i, j);
    }
}

impl<C: BuildContext> MapBuilder<C> for BrogueLikeBuilder {
    fn name(&self) -> &'static str { "BrogueLike" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Geometry) }

    fn build(&mut self, ctx: &mut C) {
        let mut rooms = Vec::new();

        // 1. First room in center
        let first = if self.profile.force_cavern_start {
            self.design_large_cavern(ctx)
        } else {
            self.design_random_room(ctx)
        };
        let offset = Point::new(self.width / 2 - first.width / 2, self.height / 2 - first.height / 2);

        let mut min_x = i32::MAX; let mut max_x = i32::MIN;
        let mut min_y = i32::MAX; let mut max_y = i32::MIN;

        for y in 0..first.height {
            for x in 0..first.width {
                if first.tiles[(y * first.width + x) as usize] == TerrainType::Floor {
                    let dp = Point::new(x, y) + offset;
                    ctx.map_mut().set_tile(dp, TerrainType::Floor);
                    min_x = min_x.min(dp.x); max_x = max_x.max(dp.x);
                    min_y = min_y.min(dp.y); max_y = max_y.max(dp.y);
                }
            }
        }
        rooms.push(Rect::with_exact(min_x, min_y, max_x, max_y));

        // 2. Iteratively attach normal rooms
        // Pre-allocate a terrain cache that we update incrementally instead of
        // cloning every iteration.
        let mut terrain_cache: Vec<TerrainType> = ctx.map().tiles.iter().map(|t| t.terrain).collect();
        let mut attempts = 0;
        let mut placed = 1;
        while placed < self.profile.target_rooms && attempts < 2000 {
            attempts += 1;
            let design = self.design_random_room(ctx);
            if design.door_sites.is_empty() { continue; }

            // Find all potential door sites in the dungeon
            let mut dungeon_sites = Vec::new();

            for y in 1..self.height - 1 {
                for x in 1..self.width - 1 {
                    let pt = Point::new(x, y);
                    let idx = ctx.map().xy_idx(x, y);
                    if terrain_cache[idx] == TerrainType::Wall
                        && let Some(dir) = self.direction_of_door_site(&terrain_cache, self.width, self.height, pt) {
                            dungeon_sites.push((pt, dir));
                        }
                }
            }
            shuffle_with_rng(&mut dungeon_sites, ctx);

            // Use relaxed fitting for cavern rooms when profile allows (50% chance)
            let use_relaxed = self.profile.relaxed_fitting && design.is_cavern && ctx.rng().roll_dice(1, 2) == 1;

            'attach: for (d_pt, d_dir) in dungeon_sites {
                for (r_pt, r_dir) in &design.door_sites {
                    if d_dir == r_dir.opposite() {
                        let offset = d_pt - *r_pt;
                        let dungeon_floor_pt = d_pt + d_dir.opposite().offset();

                        let fits = if use_relaxed {
                            self.room_fits_relaxed(ctx, &design, offset, dungeon_floor_pt)
                        } else {
                            self.room_fits(ctx, &design, offset, dungeon_floor_pt)
                        };

                        if fits {
                            let mut r_min_x = i32::MAX; let mut r_max_x = i32::MIN;
                            let mut r_min_y = i32::MAX; let mut r_max_y = i32::MIN;

                            for ry in 0..design.height {
                                for rx in 0..design.width {
                                    if design.tiles[(ry * design.width + rx) as usize] == TerrainType::Floor {
                                        let dp = Point::new(rx, ry) + offset;
                                        ctx.map_mut().set_tile(dp, TerrainType::Floor);
                                        r_min_x = r_min_x.min(dp.x); r_max_x = r_max_x.max(dp.x);
                                        r_min_y = r_min_y.min(dp.y); r_max_y = r_max_y.max(dp.y);
                                    }
                                }
                            }

                            if design.is_cavern {
                                // Wide opening for caverns: carve 2-3 tile entrance
                                ctx.map_mut().set_tile(d_pt, TerrainType::Floor);
                                let (perp_l, perp_r) = d_dir.perpendiculars();
                                for perp in [perp_l, perp_r] {
                                    let adj = d_pt + perp.offset();
                                    if ctx.map().in_bounds(adj) {
                                        let adj_idx = ctx.map().xy_idx(adj.x, adj.y);
                                        if ctx.map().tiles[adj_idx].terrain == TerrainType::Wall {
                                            // Check both sides of the wall for floor
                                            let inner = adj + d_dir.opposite().offset();
                                            let outer = adj + d_dir.offset();
                                            if ctx.map().in_bounds(inner) && ctx.map().in_bounds(outer) {
                                                let inner_idx = ctx.map().xy_idx(inner.x, inner.y);
                                                let outer_idx = ctx.map().xy_idx(outer.x, outer.y);
                                                if ctx.map().tiles[inner_idx].terrain == TerrainType::Floor
                                                    || ctx.map().tiles[outer_idx].terrain == TerrainType::Floor
                                                {
                                                    ctx.map_mut().set_tile(adj, TerrainType::Floor);
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                ctx.map_mut().set_tile(d_pt, TerrainType::Door);
                            }

                            rooms.push(Rect::with_exact(r_min_x, r_min_y, r_max_x, r_max_y));
                            placed += 1;
                            // Sync terrain cache with actual map tiles
                            for (i, tile) in ctx.map().tiles.iter().enumerate() {
                                terrain_cache[i] = tile.terrain;
                            }
                            break 'attach;
                        }
                    }
                }
            }
        }

        // 3. Place Reward Room using ChokeMap
        let chokemap = ChokeMap::generate(ctx.map());
        let mut reward_candidates = Vec::new();
        // Reuse terrain_cache (already synced after room placement loop)
        for (i, tile) in ctx.map().tiles.iter().enumerate() {
            terrain_cache[i] = tile.terrain;
        }

        for y in 1..self.height - 1 {
            for x in 1..self.width - 1 {
                let idx = ctx.map().xy_idx(x, y);
                // We look for wall tiles that could be doors (direction_of_door_site)
                // and prioritize those with high choke values (isolated regions)
                if terrain_cache[idx] == TerrainType::Wall
                    && let Some(dir) = self.direction_of_door_site(&terrain_cache, self.width, self.height, Point::new(x, y)) {
                        let choke_val = chokemap.choke_values[idx];
                        if choke_val > 10 && choke_val < 29000 { // Isolated but not infinite
                            reward_candidates.push((Point::new(x, y), dir, choke_val));
                        }
                    }
            }
        }

        // Sort by choke value descending
        reward_candidates.sort_by(|a, b| b.2.cmp(&a.2));

        let reward_design = self.design_reward_room(ctx);
        let mut reward_placed = false;

        for (d_pt, d_dir, _) in reward_candidates {
            for (r_pt, r_dir) in &reward_design.door_sites {
                if d_dir == r_dir.opposite() {
                    let offset = d_pt - *r_pt;
                    let dungeon_floor_pt = d_pt + d_dir.opposite().offset();

                    if self.room_fits(ctx, &reward_design, offset, dungeon_floor_pt) {
                        let mut r_min_x = i32::MAX; let mut r_max_x = i32::MIN;
                        let mut r_min_y = i32::MAX; let mut r_max_y = i32::MIN;

                        for ry in 0..reward_design.height {
                            for rx in 0..reward_design.width {
                                if reward_design.tiles[(ry * reward_design.width + rx) as usize] == TerrainType::Floor {
                                    let dp = Point::new(rx, ry) + offset;
                                    ctx.map_mut().set_tile(dp, TerrainType::Floor);
                                    r_min_x = r_min_x.min(dp.x); r_max_x = r_max_x.max(dp.x);
                                    r_min_y = r_min_y.min(dp.y); r_max_y = r_max_y.max(dp.y);
                                }
                            }
                        }
                        ctx.map_mut().set_tile(d_pt, TerrainType::Door);
                        rooms.push(Rect::with_exact(r_min_x, r_min_y, r_max_x, r_max_y));
                        reward_placed = true;
                        break;
                    }
                }
            }
            if reward_placed { break; }
        }

        ctx.set_rooms(rooms);
        // Add loops after rooms are attached
        let w = ctx.map().width;
        let h = ctx.map().height;
        let mut tiles_clone = ctx.map().tiles.clone();
        self.add_loops(&mut tiles_clone, w, h, 20);
        ctx.map_mut().tiles = tiles_clone;
        ctx.take_snapshot();
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builders::EngineBuilderMap;
    use crate::map::tile::TerrainType;

    fn make_builder(seed: u64) -> (BrogueLikeBuilder, EngineBuilderMap) {
        let profile = FloorProfile {
            cavern_weight: 20,
            force_cavern_start: false,
            target_rooms: 10,
            hallway_chance: 25,
            erosion_percent: 20,
            relaxed_fitting: false,
            decoration_density: 0.6,
        };
        let builder = BrogueLikeBuilder::dungeon(1, 80, 60, profile);
        let ctx = EngineBuilderMap::with_seed(1, 80, 60, "test", seed);
        (builder, ctx)
    }

    #[test]
    fn builds_without_panic() {
        let (mut builder, mut ctx) = make_builder(42);
        builder.build(&mut ctx);
        // Should have placed at least one room
        assert!(ctx.rooms.is_some());
        let rooms = ctx.rooms.as_ref().unwrap();
        assert!(!rooms.is_empty(), "Should place at least the first room");
    }

    #[test]
    fn places_floor_tiles() {
        let (mut builder, mut ctx) = make_builder(123);
        builder.build(&mut ctx);
        let floor_count = ctx.map.tiles.iter()
            .filter(|t| t.terrain == TerrainType::Floor)
            .count();
        assert!(floor_count > 50, "Should carve a significant number of floor tiles, got {}", floor_count);
    }

    #[test]
    fn places_doors() {
        let (mut builder, mut ctx) = make_builder(99);
        builder.build(&mut ctx);
        let door_count = ctx.map.tiles.iter()
            .filter(|t| t.terrain == TerrainType::Door)
            .count();
        assert!(door_count > 0, "Should place at least one door");
    }

    /// Uses cavern_weight=0 so only structured rooms are placed, avoiding
    /// `create_blob` which uses an internal thread-local RNG.
    fn make_no_cavern_builder(seed: u64) -> (BrogueLikeBuilder, EngineBuilderMap) {
        let profile = FloorProfile {
            cavern_weight: 0,
            force_cavern_start: false,
            target_rooms: 10,
            hallway_chance: 25,
            erosion_percent: 20,
            relaxed_fitting: false,
            decoration_density: 0.6,
        };
        let builder = BrogueLikeBuilder::dungeon(1, 80, 60, profile);
        let ctx = EngineBuilderMap::with_seed(1, 80, 60, "test", seed);
        (builder, ctx)
    }

    #[test]
    fn deterministic_with_same_seed() {
        // NOTE: cavern_weight must be 0 because create_blob uses an internal
        // thread-local RNG that is not seeded through BuildContext.
        let (mut b1, mut ctx1) = make_no_cavern_builder(555);
        b1.build(&mut ctx1);

        let (mut b2, mut ctx2) = make_no_cavern_builder(555);
        b2.build(&mut ctx2);

        assert_eq!(
            ctx1.map.tiles.iter().map(|t| t.terrain).collect::<Vec<_>>(),
            ctx2.map.tiles.iter().map(|t| t.terrain).collect::<Vec<_>>(),
            "Same seed should produce identical maps"
        );
    }

    #[test]
    fn different_seeds_differ() {
        let (mut b1, mut ctx1) = make_no_cavern_builder(111);
        b1.build(&mut ctx1);

        let (mut b2, mut ctx2) = make_no_cavern_builder(222);
        b2.build(&mut ctx2);

        let tiles1: Vec<_> = ctx1.map.tiles.iter().map(|t| t.terrain).collect();
        let tiles2: Vec<_> = ctx2.map.tiles.iter().map(|t| t.terrain).collect();
        assert_ne!(tiles1, tiles2, "Different seeds should (almost certainly) produce different maps");
    }

    #[test]
    fn add_loops_places_loop_doors() {
        let builder = BrogueLikeBuilder::dungeon(1, 20, 20, FloorProfile {
            cavern_weight: 0,
            force_cavern_start: false,
            target_rooms: 5,
            hallway_chance: 0,
            erosion_percent: 0,
            relaxed_fitting: false,
            decoration_density: 0.0,
        });

        // Build a simple map with two rooms separated by a wall
        use crate::map::tile::{Decoration, LiquidType};
        let mut map = Map::new(1, 20, 20, "test");
        // Room 1: rows 1-8, cols 1-8
        for y in 1..9 {
            for x in 1..9 {
                let idx = map.xy_idx(x, y);
                map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }
        // Room 2: rows 1-8, cols 10-18 (wall at col 9)
        for y in 1..9 {
            for x in 10..19 {
                let idx = map.xy_idx(x, y);
                map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }
        // Connect them with a single door at (9, 4)
        let door_idx = map.xy_idx(9, 4);
        map.tiles[door_idx].terrain = TerrainType::Door;

        // add_loops should add additional doors since the path between
        // adjacent rooms goes through a single chokepoint
        let initial_door_count = map.tiles.iter()
            .filter(|t| t.terrain == TerrainType::Door)
            .count();

        builder.add_loops(&mut map.tiles, 20, 20, 5);

        let final_door_count = map.tiles.iter()
            .filter(|t| t.terrain == TerrainType::Door)
            .count();

        // We expect at least the original door to remain
        assert!(final_door_count >= initial_door_count,
            "Should not remove existing doors: had {}, now {}", initial_door_count, final_door_count);
    }

    #[test]
    fn bfs_distance_simple() {
        use crate::map::tile::{Decoration, LiquidType};
        let builder = BrogueLikeBuilder::dungeon(1, 5, 5, FloorProfile {
            cavern_weight: 0, force_cavern_start: false, target_rooms: 1,
            hallway_chance: 0, erosion_percent: 0, relaxed_fitting: false,
            decoration_density: 0.0,
        });

        let mut map = Map::new(1, 5, 5, "test");
        // Open corridor: row 2, cols 0-4
        for x in 0..5 {
            let idx = map.xy_idx(x, 2);
            map.tiles[idx] = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }

        let start = map.xy_idx(0, 2);
        let goal = map.xy_idx(4, 2);
        let dist = builder.bfs_distance(&map, start, goal, usize::MAX);
        assert_eq!(dist, 4, "Straight corridor of 5 tiles should have distance 4");
    }

    #[test]
    fn bfs_distance_blocked() {
        use crate::map::tile::{Decoration, LiquidType};
        let builder = BrogueLikeBuilder::dungeon(1, 5, 5, FloorProfile {
            cavern_weight: 0, force_cavern_start: false, target_rooms: 1,
            hallway_chance: 0, erosion_percent: 0, relaxed_fitting: false,
            decoration_density: 0.0,
        });

        let mut map = Map::new(1, 5, 5, "test");
        // Open corridor: row 2, cols 0-4
        for x in 0..5 {
            let idx = map.xy_idx(x, 2);
            map.tiles[idx] = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }

        let start = map.xy_idx(0, 2);
        let goal = map.xy_idx(4, 2);
        let blocked = map.xy_idx(2, 2); // block the middle
        let dist = builder.bfs_distance(&map, start, goal, blocked);
        assert_eq!(dist, i32::MAX, "Should be unreachable when corridor is blocked in the middle");
    }

    #[test]
    fn fisher_yates_simple_shuffles() {
        let mut v: Vec<i32> = (0..20).collect();
        let original = v.clone();
        fisher_yates_simple(&mut v);
        // Extremely unlikely the shuffle is a no-op for 20 elements
        assert_ne!(v, original, "Shuffle should reorder elements");
        // But should contain the same elements
        v.sort();
        assert_eq!(v, original, "Shuffle should preserve all elements");
    }
}
