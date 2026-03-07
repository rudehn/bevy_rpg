use std::collections::{VecDeque, HashSet};
use bracket_lib::prelude::{Point, Algorithm2D, Rect};
use rand::prelude::*;
use crate::map::tile::TileType;
use crate::map::builders::{BuilderMap, InitialMapBuilder};
use crate::game::actions::Direction;

const MAX_ROOM_SIZE: i32 = 20;

struct RoomDesign {
    tiles: Vec<TileType>,
    width: i32,
    height: i32,
    door_sites: Vec<(Point, Direction)>,
}

pub struct BrogueLikeBuilder {
    width: i32,
    height: i32,
    depth: i32,
}

impl BrogueLikeBuilder {
    pub fn dungeon(depth: i32, width: i32, height: i32) -> Box<Self> {
        Box::new(Self {
            width,
            height,
            depth,
        })
    }

    fn design_random_room(&self) -> RoomDesign {
        let mut rng = rand::rng();
        // Brogue rooms are designed on a large grid but we'll use a local one for efficiency
        let w = MAX_ROOM_SIZE;
        let h = MAX_ROOM_SIZE;
        let mut tiles = vec![TileType::Wall; (w * h) as usize];

        let room_type = rng.random_range(0..6);
        match room_type {
            0 => self.draw_cross_room(&mut tiles, w, h),
            1 => self.draw_symmetrical_cross_room(&mut tiles, w, h),
            2 => self.draw_small_room(&mut tiles, w, h),
            3 => self.draw_circular_room(&mut tiles, w, h),
            4 => self.draw_chunky_room(&mut tiles, w, h),
            _ => self.draw_cavern_room(&mut tiles, w, h),
        }

        let mut design = RoomDesign {
            tiles,
            width: w,
            height: h,
            door_sites: Vec::new(),
        };

        // Potentially attach a hallway
        if rng.random_range(0..100) < 25 {
            self.attach_hallway(&mut design);
        } else {
            design.door_sites = self.find_door_sites(&design.tiles, w, h);
        }

        design
    }

    fn draw_cross_room(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let w1 = rng.random_range(3..10);
        let h1 = rng.random_range(3..6);
        let w2 = rng.random_range(4..15);
        let h2 = rng.random_range(2..5);

        let cx = w / 2;
        let cy = h / 2;

        self.fill_rect(tiles, w, Rect::with_size(cx - w1 / 2, cy - h1 / 2, w1, h1), TileType::Floor);
        let ox = rng.random_range(-2..2);
        let oy = rng.random_range(-1..1);
        self.fill_rect(tiles, w, Rect::with_size(cx - w2 / 2 + ox, cy - h2 / 2 + oy, w2, h2), TileType::Floor);
    }

    fn draw_symmetrical_cross_room(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let major_w = rng.random_range(4..9);
        let major_h = rng.random_range(4..6);
        let minor_w = (major_w - 2).max(1);
        let minor_h = major_h + 2;

        let cx = w / 2;
        let cy = h / 2;

        self.fill_rect(tiles, w, Rect::with_size(cx - major_w / 2, cy - major_h / 2, major_w, major_h), TileType::Floor);
        self.fill_rect(tiles, w, Rect::with_size(cx - minor_w / 2, cy - minor_h / 2, minor_w, minor_h), TileType::Floor);
    }

    fn draw_small_room(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let rw = rng.random_range(3..6);
        let rh = rng.random_range(2..4);
        self.fill_rect(tiles, w, Rect::with_size(w / 2 - rw / 2, h / 2 - rh / 2, rw, rh), TileType::Floor);
    }

    fn draw_circular_room(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let radius = rng.random_range(2..5);
        let cx = w / 2;
        let cy = h / 2;
        for x in -radius..=radius {
            for y in -radius..=radius {
                if x * x + y * y <= radius * radius {
                    let pt = Point::new(cx + x, cy + y);
                    if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                        tiles[(pt.y * w + pt.x) as usize] = TileType::Floor;
                    }
                }
            }
        }
    }

    fn draw_chunky_room(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut rng = rand::rng();
        let chunk_count = rng.random_range(2..6);
        let cx = w / 2;
        let cy = h / 2;

        // Core
        self.fill_rect(tiles, w, Rect::with_size(cx - 1, cy - 1, 3, 3), TileType::Floor);

        for _ in 0..chunk_count {
            let rx = rng.random_range(cx - 3..cx + 3);
            let ry = rng.random_range(cy - 3..cy + 3);
            let radius = 2;
            for x in -radius..=radius {
                for y in -radius..=radius {
                    if x * x + y * y <= radius * radius {
                        let pt = Point::new(rx + x, ry + y);
                        if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                            tiles[(pt.y * w + pt.x) as usize] = TileType::Floor;
                        }
                    }
                }
            }
        }
    }

    fn fill_rect(&self, tiles: &mut [TileType], w: i32, rect: Rect, tile: TileType) {
        for x in rect.x1..=rect.x2 {
            for y in rect.y1..=rect.y2 {
                if x >= 0 && x < w && y >= 0 && y < tiles.len() as i32 / w {
                    tiles[(y * w + x) as usize] = tile;
                }
            }
        }
    }

    fn find_door_sites(&self, tiles: &[TileType], w: i32, h: i32) -> Vec<(Point, Direction)> {
        let mut sites = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let pt = Point::new(x, y);
                if tiles[(y * w + x) as usize] == TileType::Wall {
                    if let Some(dir) = self.direction_of_door_site(tiles, w, h, pt) {
                        sites.push((pt, dir));
                    }
                }
            }
        }
        sites
    }

    fn direction_of_door_site(&self, tiles: &[TileType], w: i32, h: i32, pt: Point) -> Option<Direction> {
        let mut solution = None;
        for dir in [Direction::N, Direction::E, Direction::S, Direction::W] {
            let neighbor = pt + dir.offset();
            let opp = pt + dir.opposite().offset();

            if neighbor.x >= 0 && neighbor.x < w && neighbor.y >= 0 && neighbor.y < h
                && opp.x >= 0 && opp.x < w && opp.y >= 0 && opp.y < h
            {
                if tiles[(opp.y * w + opp.x) as usize] == TileType::Floor {
                    if solution.is_some() { return None; } // Multiple floor neighbors = not a door site
                    solution = Some(dir);
                }
            }
        }
        solution
    }

    fn attach_hallway(&self, design: &mut RoomDesign) {
        let mut rng = rand::rng();
        let sites = self.find_door_sites(&design.tiles, design.width, design.height);
        if let Some(&(start_pt, dir)) = sites.choose(&mut rng) {
            let length = rng.random_range(3..8);
            let mut curr = start_pt;
            for _ in 0..length {
                if curr.x < 0 || curr.x >= design.width || curr.y < 0 || curr.y >= design.height { break; }
                design.tiles[(curr.y * design.width + curr.x) as usize] = TileType::Floor;
                curr = curr + dir.offset();
            }
            // The new door site is at the end of the hallway
            design.door_sites = vec![(curr, dir)];
        }
    }

    fn draw_cavern_room(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut rng = rand::rng();
        
        // 1. Randomize
        for tile in tiles.iter_mut() {
            if rng.random_range(0..100) < 55 {
                *tile = TileType::Floor;
            } else {
                *tile = TileType::Wall;
            }
        }

        // 2. Iterate CA
        for _ in 0..4 {
            let old_tiles = tiles.to_vec();
            for y in 1..h-1 {
                for x in 1..w-1 {
                    let idx = (y * w + x) as usize;
                    let neighbors = self.count_floor_neighbors(&old_tiles, w, h, x, y);
                    if old_tiles[idx] == TileType::Wall {
                        if neighbors >= 5 {
                            tiles[idx] = TileType::Floor;
                        }
                    } else {
                        if neighbors >= 4 {
                            tiles[idx] = TileType::Floor;
                        } else {
                            tiles[idx] = TileType::Wall;
                        }
                    }
                }
            }
        }

        // 3. Keep only the largest region
        self.retain_largest_region(tiles, w, h);
    }

    fn count_floor_neighbors(&self, tiles: &[TileType], w: i32, h: i32, x: i32, y: i32) -> i32 {
        let mut count = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 { continue; }
                let nx = x + dx;
                let ny = y + dy;
                if nx >= 0 && nx < w && ny >= 0 && ny < h {
                    if tiles[(ny * w + nx) as usize] == TileType::Floor {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn retain_largest_region(&self, tiles: &mut [TileType], w: i32, h: i32) {
        let mut visited = vec![false; (w * h) as usize];
        let mut regions: Vec<Vec<usize>> = Vec::new();

        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if tiles[idx] == TileType::Floor && !visited[idx] {
                    let mut region = Vec::new();
                    let mut queue = VecDeque::new();
                    queue.push_back(idx);
                    visited[idx] = true;

                    while let Some(curr_idx) = queue.pop_front() {
                        region.push(curr_idx);
                        let (cx, cy) = (curr_idx as i32 % w, curr_idx as i32 / w);

                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                if dx == 0 && dy == 0 { continue; }
                                let nx = cx + dx;
                                let ny = cy + dy;
                                if nx >= 0 && nx < w && ny >= 0 && ny < h {
                                    let n_idx = (ny * w + nx) as usize;
                                    if tiles[n_idx] == TileType::Floor && !visited[n_idx] {
                                        visited[n_idx] = true;
                                        queue.push_back(n_idx);
                                    }
                                }
                            }
                        }
                    }
                    regions.push(region);
                }
            }
        }

        if let Some(largest) = regions.iter().max_by_key(|r| r.len()) {
            let largest_set: HashSet<usize> = largest.iter().cloned().collect();
            for (idx, tile) in tiles.iter_mut().enumerate() {
                if !largest_set.contains(&idx) {
                    *tile = TileType::Wall;
                }
            }
        } else {
            for tile in tiles.iter_mut() {
                *tile = TileType::Wall;
            }
        }
    }

    fn room_fits(&self, build_data: &BuilderMap, design: &RoomDesign, offset: Point, ignore_dungeon_pt: Point) -> bool {
        for y in 0..design.height {
            for x in 0..design.width {
                if design.tiles[(y * design.width + x) as usize] == TileType::Floor {
                    let dungeon_pt = Point::new(x, y) + offset;
                    if !build_data.map.in_bounds(dungeon_pt) { return false; }

                    // Check 3x3 padding
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            let check_pt = dungeon_pt + Point::new(dx, dy);
                            if check_pt == ignore_dungeon_pt { continue; }
                            if !build_data.map.in_bounds(check_pt) { return false; }
                            let idx = build_data.map.xy_idx(check_pt.x, check_pt.y);
                            if build_data.map.tiles[idx] != TileType::Wall {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        true
    }
}

impl InitialMapBuilder for BrogueLikeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let mut rng = rand::rng();
        let mut rooms = Vec::new();

        // 1. First room in center
        let first = self.design_random_room();
        let offset = Point::new(self.width / 2 - first.width / 2, self.height / 2 - first.height / 2);
        
        let mut min_x = i32::MAX; let mut max_x = i32::MIN;
        let mut min_y = i32::MAX; let mut max_y = i32::MIN;

        for y in 0..first.height {
            for x in 0..first.width {
                if first.tiles[(y * first.width + x) as usize] == TileType::Floor {
                    let dp = Point::new(x, y) + offset;
                    build_data.map.set_tile(dp, TileType::Floor);
                    min_x = min_x.min(dp.x); max_x = max_x.max(dp.x);
                    min_y = min_y.min(dp.y); max_y = max_y.max(dp.y);
                }
            }
        }
        rooms.push(Rect::with_exact(min_x, min_y, max_x, max_y));

        // 2. Iteratively attach
        let mut attempts = 0;
        let mut placed = 1;
        while placed < 30 && attempts < 2000 {
            attempts += 1;
            let design = self.design_random_room();
            if design.door_sites.is_empty() { continue; }

            // Find all potential door sites in the dungeon
            let mut dungeon_sites = Vec::new();
            for y in 1..self.height - 1 {
                for x in 1..self.width - 1 {
                    let pt = Point::new(x, y);
                    let idx = build_data.map.xy_idx(x, y);
                    if build_data.map.tiles[idx] == TileType::Wall {
                        if let Some(dir) = self.direction_of_door_site(&build_data.map.tiles, self.width, self.height, pt) {
                            dungeon_sites.push((pt, dir));
                        }
                    }
                }
            }
            dungeon_sites.shuffle(&mut rng);

            'attach: for (d_pt, d_dir) in dungeon_sites {
                for (r_pt, r_dir) in &design.door_sites {
                    if d_dir == r_dir.opposite() {
                        let offset = d_pt - *r_pt;
                        
                        // Check if we can fit the room (ignoring the tile that connected them)
                        // The "ignore" tile is the floor tile in the dungeon that d_pt is adjacent to.
                        let dungeon_floor_pt = d_pt + d_dir.opposite().offset();

                        if self.room_fits(build_data, &design, offset, dungeon_floor_pt) {
                            // Carve room
                            let mut r_min_x = i32::MAX; let mut r_max_x = i32::MIN;
                            let mut r_min_y = i32::MAX; let mut r_max_y = i32::MIN;

                            for ry in 0..design.height {
                                for rx in 0..design.width {
                                    if design.tiles[(ry * design.width + rx) as usize] == TileType::Floor {
                                        let dp = Point::new(rx, ry) + offset;
                                        build_data.map.set_tile(dp, TileType::Floor);
                                        r_min_x = r_min_x.min(dp.x); r_max_x = r_max_x.max(dp.x);
                                        r_min_y = r_min_y.min(dp.y); r_max_y = r_max_y.max(dp.y);
                                    }
                                }
                            }
                            // Don't forget the door itself!
                            build_data.map.set_tile(d_pt, TileType::Door);
                            
                            rooms.push(Rect::with_exact(r_min_x, r_min_y, r_max_x, r_max_y));
                            placed += 1;
                            break 'attach;
                        }
                    }
                }
            }
        }
        build_data.rooms = Some(rooms);
    }
}
