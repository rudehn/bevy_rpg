use bracket_lib::prelude::{Point, Algorithm2D, Rect};
use rand::prelude::*;
use rand::seq::SliceRandom;
use crate::map::tile::{is_walkable, Tile, TerrainType};
use crate::map::builders::{BuilderMap, FloorProfile, InitialMapBuilder};
use crate::game::actions::Direction;
use crate::map::map::Map;
use crate::map::builders::algorithms::{Grid, BlobGenConfig, create_blob};
use crate::map::builders::choke_map::ChokeMap;

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
    pub fn dungeon(_depth: i32, width: i32, height: i32, profile: FloorProfile) -> Box<Self> {
        Box::new(Self {
            width,
            height,
            profile,
        })
    }

    fn design_random_room(&self) -> RoomDesign {
        let mut rng = rand::rng();

        // Use profile cavern weight to determine if this room is a cavern.
        let is_cavern = rng.random_range(0..100) < self.profile.cavern_weight;
        let room_type = if is_cavern {
            6 // cavern
        } else {
            rng.random_range(0..6) // structured room types
        };

        let w = if is_cavern { MAX_CAVERN_SIZE } else { MAX_ROOM_SIZE };
        let h = if is_cavern { MAX_CAVERN_SIZE } else { MAX_ROOM_SIZE };
        let mut tiles = vec![TerrainType::Wall; (w * h) as usize];

        match room_type {
            0 => self.draw_cross_room(&mut tiles, w, h),
            1 => self.draw_symmetrical_cross_room(&mut tiles, w, h),
            2 => self.draw_small_room(&mut tiles, w, h),
            3 => self.draw_circular_room(&mut tiles, w, h),
            4 | 5 => self.draw_chunky_room(&mut tiles, w, h),
            _ => self.draw_cavern_room(&mut tiles, w, h),
        }

        let mut design = RoomDesign {
            tiles,
            width: w,
            height: h,
            door_sites: Vec::new(),
            is_cavern,
        };

        // Potentially attach a winding hallway
        if rng.random_range(0..100) < self.profile.hallway_chance {
            self.attach_winding_hallway(&mut design);
        } else {
            design.door_sites = self.find_door_sites(&design.tiles, w, h);
        }

        design
    }

    fn design_large_cavern(&self) -> RoomDesign {
        let w = MAX_CAVERN_SIZE;
        let h = MAX_CAVERN_SIZE;
        let mut tiles = vec![TerrainType::Wall; (w * h) as usize];
        self.draw_cavern_room(&mut tiles, w, h);
        let door_sites = self.find_door_sites(&tiles, w, h);
        RoomDesign { tiles, width: w, height: h, door_sites, is_cavern: true }
    }

    fn design_reward_room(&self) -> RoomDesign {
        let mut rng = rand::rng();
        let w = MAX_ROOM_SIZE;
        let h = MAX_ROOM_SIZE;
        let mut tiles = vec![TerrainType::Wall; (w * h) as usize];

        // Reward rooms are often specialized. Let's make a circular one with a "pedestal"
        let radius = rng.random_range(4..6);
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
        if rng.random_bool(0.5) {
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

    fn draw_cross_room(&self, tiles: &mut [TerrainType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let w1 = rng.random_range(3..10);
        let h1 = rng.random_range(3..6);
        let w2 = rng.random_range(4..15);
        let h2 = rng.random_range(2..5);

        let cx = w / 2;
        let cy = h / 2;

        self.fill_rect(tiles, w, Rect::with_size(cx - w1 / 2, cy - h1 / 2, w1, h1), TerrainType::Floor);
        let ox = rng.random_range(-2..2);
        let oy = rng.random_range(-1..1);
        self.fill_rect(tiles, w, Rect::with_size(cx - w2 / 2 + ox, cy - h2 / 2 + oy, w2, h2), TerrainType::Floor);
    }

    fn draw_symmetrical_cross_room(&self, tiles: &mut [TerrainType], w: i32, h: i32) {
        let major_w = rand::rng().random_range(4..9);
        let major_h = rand::rng().random_range(4..6);
        let minor_w = (major_w - 2).max(1);
        let minor_h = major_h + 2;

        let cx = w / 2;
        let cy = h / 2;

        self.fill_rect(tiles, w, Rect::with_size(cx - major_w / 2, cy - major_h / 2, major_w, major_h), TerrainType::Floor);
        self.fill_rect(tiles, w, Rect::with_size(cx - minor_w / 2, cy - minor_h / 2, minor_w, minor_h), TerrainType::Floor);
    }

    fn draw_small_room(&self, tiles: &mut [TerrainType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let rw = rng.random_range(3..6);
        let rh = rng.random_range(2..4);
        self.fill_rect(tiles, w, Rect::with_size(w / 2 - rw / 2, h / 2 - rh / 2, rw, rh), TerrainType::Floor);
    }

    fn draw_circular_room(&self, tiles: &mut [TerrainType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let radius = rng.random_range(2..5);
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

    fn draw_chunky_room(&self, tiles: &mut [TerrainType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let chunk_count = rng.random_range(2..6);
        let cx = w / 2;
        let cy = h / 2;

        // Core
        self.fill_rect(tiles, w, Rect::with_size(cx - 1, cy - 1, 3, 3), TerrainType::Floor);

        for _ in 0..chunk_count {
            let rx = rng.random_range(cx - 3..cx + 3);
            let ry = rng.random_range(cy - 3..cy + 3);
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
        for x in rect.x1..=rect.x2 {
            for y in rect.y1..=rect.y2 {
                if x >= 0 && x < w && y >= 0 && y < tiles.len() as i32 / w {
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
                if tiles[(y * w + x) as usize] == TerrainType::Wall {
                    if let Some(dir) = self.direction_of_door_site(tiles, w, h, pt) {
                        sites.push((pt, dir));
                    }
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
            {
                if tiles[(opp.y * w + opp.x) as usize] == TerrainType::Floor {
                    if solution.is_some() { return None; } // Multiple floor neighbors = not a door site
                    solution = Some(dir);
                }
            }
        }
        solution
    }

    fn attach_winding_hallway(&self, design: &mut RoomDesign) {
        let mut rng = rand::rng();
        let sites = self.find_door_sites(&design.tiles, design.width, design.height);
        if let Some(&(start_pt, primary_dir)) = sites.choose(&mut rng) {
            let length = rng.random_range(5..15);
            let mut curr = start_pt;
            let (perp_left, perp_right) = primary_dir.perpendiculars();

            for _ in 0..length {
                if curr.x < 0 || curr.x >= design.width || curr.y < 0 || curr.y >= design.height {
                    break;
                }
                design.tiles[(curr.y * design.width + curr.x) as usize] = TerrainType::Floor;

                // Occasionally widen the corridor
                if rng.random_range(0..100) < 30 {
                    let side = if rng.random_bool(0.5) { perp_left } else { perp_right };
                    let adj = curr + side.offset();
                    if adj.x >= 0 && adj.x < design.width && adj.y >= 0 && adj.y < design.height {
                        design.tiles[(adj.y * design.width + adj.x) as usize] = TerrainType::Floor;
                    }
                }

                // Biased random walk: 60% forward, 20% left, 20% right
                let roll = rng.random_range(0..100);
                let step_dir = if roll < 60 {
                    primary_dir
                } else if roll < 80 {
                    perp_left
                } else {
                    perp_right
                };
                curr = curr + step_dir.offset();
            }

            design.door_sites = vec![(curr, primary_dir)];
        }
    }

    fn draw_cavern_room(&self, tiles: &mut [TerrainType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let initial_grid_dims = Grid::new(w, h, TerrainType::Wall);

        // Scale blob dimensions to the available grid size.
        // On the 40×40 cavern grid these produce large organic caves;
        // on the 20×20 room grid they fall back to compact shapes.
        let (min_bw, max_bw, min_bh, max_bh) = match rng.random_range(0..3) {
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

    fn room_fits(&self, build_data: &BuilderMap, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point) -> bool {
        self.room_fits_with_padding(build_data, design, offset, ignore_dungeon_pt, 1)
    }

    /// Relaxed fit check — only requires the room floor tiles themselves to land on walls.
    /// No padding. Used for cavern-to-cavern connections so caves can nearly merge.
    fn room_fits_relaxed(&self, build_data: &BuilderMap, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point) -> bool {
        self.room_fits_with_padding(build_data, design, offset, ignore_dungeon_pt, 0)
    }

    fn room_fits_with_padding(&self, build_data: &BuilderMap, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point, padding: i32) -> bool {
        for y in 0..design.height {
            for x in 0..design.width {
                if design.tiles[(y * design.width + x) as usize] == TerrainType::Floor {
                    let dungeon_pt = Point::new(x, y) + offset;
                    if !build_data.map.in_bounds(dungeon_pt) { return false; }

                    for dx in -padding..=padding {
                        for dy in -padding..=padding {
                            let check_pt = dungeon_pt + Point::new(dx, dy);
                            if check_pt == ignore_dungeon_pt { continue; }
                            if !build_data.map.in_bounds(check_pt) { return false; }
                            let tile = build_data.map.tiles[build_data.map.xy_idx(check_pt.x, check_pt.y)];
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

    pub fn add_loops(&self, tiles: &mut Vec<Tile>, w: i32, h: i32, minimum_path_distance: i32) {
        let total_cells = (w * h) as usize;

        // Create a temporary Map instance for Dijkstra calculations
        let mut map_for_dijkstra = Map::new(1, w, h, "tmp");
        map_for_dijkstra.tiles = tiles.clone();

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
        candidates.shuffle(&mut rand::rng());

        // Use a single Dijkstra from all walkable tiles would be wrong — we need
        // per-candidate distance. But we can use BFS which is much faster than
        // bracket-lib's DijkstraMap for unweighted graphs.
        for (idx, dx, dy) in candidates {
            let (x, y) = map_for_dijkstra.idx_xy(idx);
            // Recheck the wall — a previous iteration may have turned it into a door
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

impl InitialMapBuilder for BrogueLikeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();
        let mut rooms = Vec::new();

        // 1. First room in center
        let first = if self.profile.force_cavern_start {
            self.design_large_cavern()
        } else {
            self.design_random_room()
        };
        let offset = Point::new(self.width / 2 - first.width / 2, self.height / 2 - first.height / 2);
        
        let mut min_x = i32::MAX; let mut max_x = i32::MIN;
        let mut min_y = i32::MAX; let mut max_y = i32::MIN;

        for y in 0..first.height {
            for x in 0..first.width {
                if first.tiles[(y * first.width + x) as usize] == TerrainType::Floor {
                    let dp = Point::new(x, y) + offset;
                    build_data.map.set_tile(dp, TerrainType::Floor);
                    min_x = min_x.min(dp.x); max_x = max_x.max(dp.x);
                    min_y = min_y.min(dp.y); max_y = max_y.max(dp.y);
                }
            }
        }
        rooms.push(Rect::with_exact(min_x, min_y, max_x, max_y));

        // 2. Iteratively attach normal rooms
        // Pre-allocate a terrain cache that we update incrementally instead of
        // cloning every iteration.
        let mut terrain_cache: Vec<TerrainType> = build_data.map.tiles.iter().map(|t| t.terrain).collect();
        let mut attempts = 0;
        let mut placed = 1;
        while placed < self.profile.target_rooms && attempts < 2000 {
            attempts += 1;
            let design = self.design_random_room();
            if design.door_sites.is_empty() { continue; }

            // Find all potential door sites in the dungeon
            let mut dungeon_sites = Vec::new();

            for y in 1..self.height - 1 {
                for x in 1..self.width - 1 {
                    let pt = Point::new(x, y);
                    let idx = build_data.map.xy_idx(x, y);
                    if terrain_cache[idx] == TerrainType::Wall {
                        if let Some(dir) = self.direction_of_door_site(&terrain_cache, self.width, self.height, pt) {
                            dungeon_sites.push((pt, dir));
                        }
                    }
                }
            }
            dungeon_sites.shuffle(&mut rng);

            // Use relaxed fitting for cavern rooms when profile allows (50% chance)
            let use_relaxed = self.profile.relaxed_fitting && design.is_cavern && rng.random_bool(0.5);

            'attach: for (d_pt, d_dir) in dungeon_sites {
                for (r_pt, r_dir) in &design.door_sites {
                    if d_dir == r_dir.opposite() {
                        let offset = d_pt - *r_pt;
                        let dungeon_floor_pt = d_pt + d_dir.opposite().offset();

                        let fits = if use_relaxed {
                            self.room_fits_relaxed(build_data, &design, offset, dungeon_floor_pt)
                        } else {
                            self.room_fits(build_data, &design, offset, dungeon_floor_pt)
                        };

                        if fits {
                            let mut r_min_x = i32::MAX; let mut r_max_x = i32::MIN;
                            let mut r_min_y = i32::MAX; let mut r_max_y = i32::MIN;

                            for ry in 0..design.height {
                                for rx in 0..design.width {
                                    if design.tiles[(ry * design.width + rx) as usize] == TerrainType::Floor {
                                        let dp = Point::new(rx, ry) + offset;
                                        build_data.map.set_tile(dp, TerrainType::Floor);
                                        r_min_x = r_min_x.min(dp.x); r_max_x = r_max_x.max(dp.x);
                                        r_min_y = r_min_y.min(dp.y); r_max_y = r_max_y.max(dp.y);
                                    }
                                }
                            }

                            if design.is_cavern {
                                // Wide opening for caverns: carve 2-3 tile entrance
                                build_data.map.set_tile(d_pt, TerrainType::Floor);
                                let (perp_l, perp_r) = d_dir.perpendiculars();
                                for perp in [perp_l, perp_r] {
                                    let adj = d_pt + perp.offset();
                                    if build_data.map.in_bounds(adj) {
                                        let adj_idx = build_data.map.xy_idx(adj.x, adj.y);
                                        if build_data.map.tiles[adj_idx].terrain == TerrainType::Wall {
                                            // Check both sides of the wall for floor
                                            let inner = adj + d_dir.opposite().offset();
                                            let outer = adj + d_dir.offset();
                                            if build_data.map.in_bounds(inner) && build_data.map.in_bounds(outer) {
                                                let inner_idx = build_data.map.xy_idx(inner.x, inner.y);
                                                let outer_idx = build_data.map.xy_idx(outer.x, outer.y);
                                                if build_data.map.tiles[inner_idx].terrain == TerrainType::Floor
                                                    || build_data.map.tiles[outer_idx].terrain == TerrainType::Floor
                                                {
                                                    build_data.map.set_tile(adj, TerrainType::Floor);
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                build_data.map.set_tile(d_pt, TerrainType::Door);
                            }

                            rooms.push(Rect::with_exact(r_min_x, r_min_y, r_max_x, r_max_y));
                            placed += 1;
                            // Sync terrain cache with actual map tiles
                            for (i, tile) in build_data.map.tiles.iter().enumerate() {
                                terrain_cache[i] = tile.terrain;
                            }
                            break 'attach;
                        }
                    }
                }
            }
        }

        // 3. Place Reward Room using ChokeMap
        let chokemap = ChokeMap::generate(&build_data.map);
        let mut reward_candidates = Vec::new();
        // Reuse terrain_cache (already synced after room placement loop)
        for (i, tile) in build_data.map.tiles.iter().enumerate() {
            terrain_cache[i] = tile.terrain;
        }

        for y in 1..self.height - 1 {
            for x in 1..self.width - 1 {
                let idx = build_data.map.xy_idx(x, y);
                // We look for wall tiles that could be doors (direction_of_door_site)
                // and prioritize those with high choke values (isolated regions)
                if terrain_cache[idx] == TerrainType::Wall {
                    if let Some(dir) = self.direction_of_door_site(&terrain_cache, self.width, self.height, Point::new(x, y)) {
                        let choke_val = chokemap.choke_values[idx];
                        if choke_val > 10 && choke_val < 29000 { // Isolated but not infinite
                            reward_candidates.push((Point::new(x, y), dir, choke_val));
                        }
                    }
                }
            }
        }

        // Sort by choke value descending
        reward_candidates.sort_by(|a, b| b.2.cmp(&a.2));

        let reward_design = self.design_reward_room();
        let mut reward_placed = false;

        for (d_pt, d_dir, _) in reward_candidates {
            for (r_pt, r_dir) in &reward_design.door_sites {
                if d_dir == r_dir.opposite() {
                    let offset = d_pt - *r_pt;
                    let dungeon_floor_pt = d_pt + d_dir.opposite().offset();

                    if self.room_fits(build_data, &reward_design, offset, dungeon_floor_pt) {
                        let mut r_min_x = i32::MAX; let mut r_max_x = i32::MIN;
                        let mut r_min_y = i32::MAX; let mut r_max_y = i32::MIN;

                        for ry in 0..reward_design.height {
                            for rx in 0..reward_design.width {
                                if reward_design.tiles[(ry * reward_design.width + rx) as usize] == TerrainType::Floor {
                                    let dp = Point::new(rx, ry) + offset;
                                    build_data.map.set_tile(dp, TerrainType::Floor);
                                    r_min_x = r_min_x.min(dp.x); r_max_x = r_max_x.max(dp.x);
                                    r_min_y = r_min_y.min(dp.y); r_max_y = r_max_y.max(dp.y);
                                }
                            }
                        }
                        build_data.map.set_tile(d_pt, TerrainType::Door);
                        rooms.push(Rect::with_exact(r_min_x, r_min_y, r_max_x, r_max_y));
                        reward_placed = true;
                        break;
                    }
                }
            }
            if reward_placed { break; }
        }

        build_data.rooms = Some(rooms);
        // Add loops after rooms are attached
        self.add_loops(&mut build_data.map.tiles, build_data.map.width, build_data.map.height, 20);
        build_data.take_snapshot();
    }
}
