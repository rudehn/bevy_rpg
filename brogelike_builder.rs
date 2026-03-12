use std::{
    cmp::{max, min},
    collections::{HashMap, VecDeque},
};

use crate::{
    map::{
        Cell, Direction, Grid, Map, T_CAN_BE_BRIDGED, T_OBSTRUCTS_PASSABILITY, T_PATHING_BLOCKER,
        TileType,
        builders::{BuilderMap, InitialMapBuilder, algorithms::create_blob_on_map},
    },
    settings::{
        AMULET_LEVEL, CAVE_MIN_HEIGHT, CAVE_MIN_WIDTH, HORIZONTAL_CORRIDOR_MAX_LENGTH,
        HORIZONTAL_CORRIDOR_MIN_LENGTH, VERTICAL_CORRIDOR_MAX_LENGTH, VERTICAL_CORRIDOR_MIN_LENGTH,
    },
    utils::clamp,
}; // From crate::components::Position
use crate::{
    map::{
        DungeonFeatureConfig, T_LAKE_PATHING_BLOCKER, T_OBSTRUCTS_SURFACE_EFFECTS,
        T_OBSTRUCTS_VISION, get_auto_generator_catalog, get_dungeon_feature_catalog,
        recalculate_static_lighting, tiletype,
    },
    rng::{range, roll_dice},
};
use bracket_lib::{
    color::WEBGRAY,
    prelude::{DijkstraMap, Rect, Tile},
};
use rand::seq::{IndexedRandom, SliceRandom}; // From bracket_lib::prelude::Rect // From crate::rng (bringing functions into scope)

struct DungeonProfile {
    corridor_chance: i32,
    door_chance: u8,
    // Room frequencies:
    //      0. Cross room
    //      1. Small symmetrical cross room
    //      2. Small room
    //      3. Circular room
    //      4. Chunky room
    //      5. Cave
    //      6. Cavern (the kind that fills a level)
    //      7. Entrance room (the big upside-down T room at the start of depth 1)
    room_frequencies: [i32; 8],
    // any other parameters (room frequencies, etc.)
}
impl DungeonProfile {
    pub fn default(depth: i32) -> Self {
        let mut dp = Self {
            corridor_chance: 0,
            door_chance: 60,
            room_frequencies: [2, 1, 1, 1, 7, 1, 0, 0],
        };

        let descent_pct = clamp(100 * (depth - 1) / (AMULET_LEVEL - 1), 0, 100);
        dp.room_frequencies[0] += 20 * (100 - descent_pct) / 100;
        dp.room_frequencies[1] += 10 * (100 - descent_pct) / 100;
        dp.room_frequencies[3] += 7 * (100 - descent_pct) / 100;
        dp.room_frequencies[5] += 10 * descent_pct / 100;

        dp.corridor_chance += 15 * (100 - descent_pct) / 100;

        dp
    }
    pub fn first_room(depth: i32) -> Self {
        let mut dp = Self {
            corridor_chance: 0,
            door_chance: 60,
            room_frequencies: [10, 0, 0, 3, 7, 10, 10, 0],
        };

        let descent_pct = clamp(100 * (depth - 1) / (AMULET_LEVEL - 1), 0, 100);
        if depth == 1 {
            // All dungeons start with the entrance room on depth 1.
            for i in 0..dp.room_frequencies.len() {
                dp.room_frequencies[i] = 0;
            }
            dp.room_frequencies[7] = 1;
        } else {
            dp.room_frequencies[6] += 50 * descent_pct / 100
        }
        dp
    }
}

#[derive(Clone)]
pub struct BrogueLikeBuilder {
    width: i32,
    height: i32,
    depth: i32,
    rooms: Vec<Rect>,
}

impl BrogueLikeBuilder {
    pub fn new(depth: i32, width: i32, height: i32) -> Box<Self> {
        Box::new(Self {
            width,
            height,
            depth,
            rooms: Vec::new(),
        })
    }

    // https://github.com/tsadok/brogue/blob/master/src/brogue/Architect.c#L2205
    fn design_random_room(
        &self,
        attach_doors: bool,
        attach_hallway: bool,
        room_frequencies: &[i32; 8],
    ) -> (Grid<TileType>, Option<HashMap<Direction, (i32, i32)>>) {
        let mut grid = Grid::new(self.width, self.height, TileType::Wall);

        let mut sum = 0;
        for freq in room_frequencies {
            sum += *freq;
        }
        let mut room_idx = crate::rng::range(0, sum - 1);
        let mut i = 0;
        for freq in room_frequencies {
            if room_idx < *freq {
                break;
            } else {
                room_idx -= *freq;
                i += 1;
            }
        }

        match i {
            0 => self.design_cross_room(&mut grid),
            1 => self.design_symmetrical_cross_room(&mut grid),
            2 => self.design_small_room(&mut grid),
            3 => self.design_circular_room(&mut grid),
            4 => self.design_chunky_room(&mut grid),
            5 => {
                let cavern_type = crate::rng::range(0, 3);
                match cavern_type {
                    0 => self.design_cavern(&mut grid, 3, 12, 4, 8),
                    1 => self.design_cavern(&mut grid, 3, 12, 15, self.height as usize - 2),
                    _ => self.design_cavern(&mut grid, 20, self.height as usize - 2, 4, 8),
                }
            }
            6 => {
                self.design_cavern(
                    &mut grid,
                    CAVE_MIN_WIDTH,
                    self.width as usize - 2,
                    CAVE_MIN_HEIGHT,
                    self.height as usize - 2,
                );
            }
            _ => self.design_entrance_room(&mut grid),
        };
        let doors = if attach_doors {
            let mut doors = self.choose_random_door_sites(&grid);
            if attach_hallway && !doors.is_empty() {
                self.attach_hallway_to(&mut grid, &mut doors);
            }
            Some(doors)
        } else {
            None
        };
        (grid, doors)
    }

    fn attach_hallway_to(
        &self,
        grid: &mut Grid<TileType>,
        door_sites: &mut HashMap<Direction, (i32, i32)>,
    ) {
        let mut dirs = [Direction::N, Direction::E, Direction::S, Direction::W];
        dirs.shuffle(&mut rand::rng()); // randomize direction order

        // ---- Pick a valid direction ----
        let mut chosen_dir = None;
        for &dir in &dirs {
            if let Some(&(x, y)) = door_sites.get(&dir) {
                let (dx, dy) = dir.offset().to_tuple();
                let test_x = x + dx * HORIZONTAL_CORRIDOR_MAX_LENGTH;
                let test_y = y + dy * VERTICAL_CORRIDOR_MAX_LENGTH;

                if grid.in_bounds(test_x, test_y) {
                    chosen_dir = Some(dir);
                    break;
                }
            }
        }

        // No valid direction found
        let dir = match chosen_dir {
            Some(d) => d,
            None => return,
        };

        // ---- Determine hallway length ----
        let length = match dir {
            Direction::N | Direction::S => {
                crate::rng::range(VERTICAL_CORRIDOR_MIN_LENGTH, VERTICAL_CORRIDOR_MAX_LENGTH)
            }
            Direction::E | Direction::W => crate::rng::range(
                HORIZONTAL_CORRIDOR_MIN_LENGTH,
                HORIZONTAL_CORRIDOR_MAX_LENGTH,
            ),
            _ => return,
        };

        // ---- Carve the hallway ----
        let mut x = door_sites[&dir].0;
        let mut y = door_sites[&dir].1;
        let (dx, dy) = dir.offset().to_tuple();

        for _ in 0..=length {
            if grid.in_bounds(x, y) {
                let idx = grid.xy_idx(x, y);
                grid.data[idx] = TileType::Floor;
            }
            x += dx;
            y += dy;
        }

        // Move back to the last valid cell inside the map
        x = (x - dx).clamp(0, self.width - 1);
        y = (y - dy).clamp(0, self.height - 1);

        // ---- Possibly allow oblique exits ----
        let allow_oblique_exit = crate::rng::percent(15);

        for dir2 in [Direction::N, Direction::E, Direction::S, Direction::W] {
            let (ndx, ndy) = dir2.offset().to_tuple();
            let new_x = x + ndx;
            let new_y = y + ndy;

            if ((dir2 != dir) && !allow_oblique_exit)
                || !grid.in_bounds(new_x, new_y)
                || *grid.at(new_x, new_y).unwrap() != TileType::Wall
            {
                door_sites.remove(&dir2);
                // door_sites.insert(dir2, (-1, -1));
            } else {
                door_sites.insert(dir2, (new_x, new_y));
            }
        }
    }

    fn direction_of_door_site(&self, grid: &Grid<TileType>, x: i32, y: i32) -> Direction {
        let mut solution_dir = Direction::NoDirection;
        if !matches!(grid.at(x, y), Some(TileType::Wall)) {
            return solution_dir;
        }
        for dir in [Direction::N, Direction::E, Direction::S, Direction::W] {
            let (ndx, ndy) = dir.offset().to_tuple();
            let new_x = x + ndx;
            let new_y = y + ndy;
            let opp_x = x - ndx;
            let opp_y = y - ndy;
            if grid.in_bounds(opp_x, opp_y)
                && grid.in_bounds(new_x, new_y)
                && *grid.at(opp_x, opp_y).unwrap() != TileType::Wall
            {
                // This grid cell would be a valid tile on which to place a door that, facing outward, points dir.
                if solution_dir != Direction::NoDirection {
                    // Already claimed by another direction; no doors here!
                    return Direction::NoDirection;
                }
                solution_dir = dir;
            }
        }
        solution_dir
    }

    fn choose_random_door_sites(&self, map: &Grid<TileType>) -> HashMap<Direction, (i32, i32)> {
        let grid = map.clone();
        let mut possible_door_sites: HashMap<Direction, Vec<(i32, i32)>> = HashMap::new();

        for i in 0..self.width {
            for j in 0..self.height {
                if grid[grid.xy_idx(i, j)] == TileType::Wall {
                    let dir = self.direction_of_door_site(map, i, j);
                    if dir != Direction::NoDirection {
                        // Trace a ray 10 spaces outward from the door site to make sure it doesn't intersect the room.
                        // If it does, it's not a valid door site.
                        let (dx, dy) = dir.offset().to_tuple();
                        let mut new_x = i + dx;
                        let mut new_y = j + dy;
                        let mut door_site_failed = false;
                        for _ in 0..=10 {
                            if !map.in_bounds(new_x, new_y) {
                                break;
                            }
                            if *map.at(new_x, new_y).unwrap() != TileType::Wall {
                                door_site_failed = true;
                                break;
                            }
                            new_x += dx;
                            new_y += dy;
                        }

                        if !door_site_failed {
                            possible_door_sites.entry(dir).or_default().push((i, j));
                        }
                    }
                }
            }
        }

        let mut selected_doors = HashMap::new();
        for dir in [Direction::N, Direction::E, Direction::S, Direction::W] {
            if let Some(coords) = possible_door_sites.get(&dir) {
                if let Some(choice) = coords.choose(&mut rand::rng()) {
                    selected_doors.insert(dir, *choice);
                }
            }
        }
        selected_doors
    }

    fn design_cross_room(&self, grid: &mut Grid<TileType>) {
        let room_width = crate::rng::range(3, 12);
        let room_x = crate::rng::range(
            max(0, self.width / 2 - (room_width - 1)),
            min(self.width, self.width / 2),
        );
        let room_width2 = crate::rng::range(4, 20);
        let room_x2 =
            (room_x + (room_width / 2) + crate::rng::range(0, 2) + crate::rng::range(0, 2) - 3)
                - (room_width2 / 2);

        let room_height = crate::rng::range(3, 7);
        let room_y = self.height / 2 - room_height;
        let room_height2 = crate::rng::range(2, 5);
        let room_y2 =
            self.height / 2 - room_height2 - (crate::rng::range(0, 2) + crate::rng::range(0, 1));

        self.draw_rectangle_on_grid(
            grid,
            room_x - 5,
            room_y + 5,
            room_width,
            room_height,
            TileType::Floor,
        );
        self.draw_rectangle_on_grid(
            grid,
            room_x2 - 5,
            room_y2 + 5,
            room_width2,
            room_height2,
            TileType::Floor,
        );
    }

    fn design_symmetrical_cross_room(&self, grid: &mut Grid<TileType>) {
        let major_width = crate::rng::range(4, 9);
        let major_height = crate::rng::range(4, 6);

        let mut minor_width = crate::rng::range(4, 6);
        if major_height % 2 == 0 {
            minor_width -= 1;
        }
        let mut minor_height = major_height - 1;
        if major_width % 2 == 0 {
            minor_height -= 1;
        }

        self.draw_rectangle_on_grid(
            grid,
            (self.width - major_width) / 2,
            (self.height - minor_height) / 2,
            major_width,
            minor_height,
            TileType::Floor,
        );
        self.draw_rectangle_on_grid(
            grid,
            (self.width - minor_width) / 2,
            (self.height - major_height) / 2,
            minor_width,
            major_height,
            TileType::Floor,
        );
    }

    fn design_small_room(&self, grid: &mut Grid<TileType>) {
        let width = crate::rng::range(3, 6);
        let height = crate::rng::range(2, 4);

        self.draw_rectangle_on_grid(
            grid,
            (self.width - width) / 2,
            (self.height - height) / 2,
            width,
            height,
            TileType::Floor,
        );
    }

    fn design_circular_room(&self, grid: &mut Grid<TileType>) {
        let radius = if crate::rng::percent(5) {
            crate::rng::range(4, 10)
        } else {
            crate::rng::range(2, 4)
        };

        self.draw_circle_on_grid(
            grid,
            self.width / 2,
            self.height / 2,
            radius,
            TileType::Floor,
        );

        if radius > 6 && crate::rng::percent(50) {
            self.draw_circle_on_grid(
                grid,
                self.width / 2,
                self.height / 2,
                crate::rng::range(3, radius - 3),
                TileType::Wall,
            );
        }
    }

    fn design_chunky_room(&self, grid: &mut Grid<TileType>) {
        let chunk_count = crate::rng::range(2, 8);
        self.draw_circle_on_grid(grid, self.width / 2, self.height / 2, 2, TileType::Floor);
        let mut min_x = self.width / 2 - 3;
        let mut max_x = self.width / 2 + 3;
        let mut min_y = self.height / 2 - 3;
        let mut max_y = self.height / 2 + 3;
        let mut i = 0;
        while i <= chunk_count {
            let x = crate::rng::range(min_x, max_x);
            let y = crate::rng::range(min_y, max_y);
            if !matches!(grid.at(x, y), None | Some(TileType::Wall)) {
                i += 1;
                self.draw_circle_on_grid(grid, x, y, 2, TileType::Floor);
                min_x = max(1, min(x - 3, min_x));
                max_x = min(self.width - 2, max(x + 3, max_x));
                min_y = max(1, min(y - 3, min_y));
                max_y = min(self.height - 2, max(y + 3, max_y));
            }
        }
    }

    fn design_entrance_room(&self, grid: &mut Grid<TileType>) {
        let room_width = 8;
        let room_height = 10;
        let room_width2 = 20;
        let room_height2 = 5;
        let room_x = self.width / 2 - room_width / 2 - 1;
        let room_y = self.height - room_height - 2;
        let room_x2 = self.width / 2 - room_width2 / 2 - 1;
        let room_y2 = self.height - room_height2 - 2;

        self.draw_rectangle_on_grid(
            grid,
            room_x,
            room_y,
            room_width,
            room_height,
            TileType::Floor,
        );
        self.draw_rectangle_on_grid(
            grid,
            room_x2,
            room_y2,
            room_width2,
            room_height2,
            TileType::Floor,
        );
    }

    fn draw_rectangle_on_grid(
        &self,
        grid: &mut Grid<TileType>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        tiletype: TileType,
    ) {
        for i in x..=x + width {
            for j in y..=y + height {
                let idx;
                {
                    idx = grid.xy_idx(i, j);
                }
                grid[idx] = tiletype;
            }
        }
    }

    fn draw_circle_on_grid(
        &self,
        grid: &mut Grid<TileType>,
        x: i32,
        y: i32,
        radius: i32,
        value: TileType,
    ) {
        let start_x = (x - radius - 1).max(0);
        let end_x = (x + radius).min(grid.width - 1);

        let start_y = (y - radius - 1).max(0);
        let end_y = (y + radius).min(grid.height - 1);

        for i in start_x..=end_x {
            for j in start_y..=end_y {
                let dx = i - x;
                let dy = j - y;
                if dx * dx + dy * dy < radius * radius + radius {
                    let idx = grid.xy_idx(i, j);
                    grid[idx] = value;
                }
            }
        }
    }

    fn attach_rooms(
        &self,
        build_data: &mut BuilderMap,
        dp: &DungeonProfile,
        attempts: usize,
        max_room_count: usize,
    ) {
        let total_cells = build_data.map.width() * build_data.map.height();
        let mut grid = Grid::from_cell_grid(&build_data.map.tiles);

        // shuffled list of all coordinates
        let mut s_coords: Vec<usize> = (0..total_cells as usize).collect();
        s_coords.shuffle(&mut rand::rng());

        let mut rooms_built = 0;
        let mut rooms_attempted = 0;

        while rooms_built < max_room_count && rooms_attempted < attempts {
            rooms_attempted += 1;

            let (room_map, door_sites) = self.design_random_room(
                true,
                rooms_attempted <= attempts - 5 && crate::rng::percent(dp.corridor_chance),
                &dp.room_frequencies,
            );

            // try each shuffled coordinate
            for &idx in &s_coords {
                let (x, y) = build_data.map.idx_to_xy(idx);

                let dir = self.direction_of_door_site(&grid, x, y);
                let opp_dir = dir.opposite();

                if dir != Direction::NoDirection {
                    if let Some(door_sites) = &door_sites {
                        if let Some(&(door_x, door_y)) = door_sites.get(&opp_dir) {
                            if self.room_fits_at(&grid, &room_map, x - door_x, y - door_y) {
                                {
                                    self.insert_room_at(
                                        &mut grid,
                                        &room_map,
                                        x - door_x,
                                        y - door_y,
                                        door_x,
                                        door_y,
                                    );
                                    grid.set(x, y, TileType::Door);
                                }
                                rooms_built += 1;
                                build_data.map.tiles = Grid::to_cell_grid(&grid);
                                build_data.take_snapshot();
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn room_fits_at(
        &self,
        dungeon: &Grid<TileType>,
        room: &Grid<TileType>,
        room_to_dungeon_x: i32,
        room_to_dungeon_y: i32,
    ) -> bool {
        for x_room in 0..dungeon.width {
            for y_room in 0..dungeon.height {
                if !matches!(room.at(x_room, y_room), Some(TileType::Wall)) {
                    let x_dungeon = x_room + room_to_dungeon_x;
                    let y_dungeon = y_room + room_to_dungeon_y;

                    // Check a 3x3 neighborhood around this tile
                    for i in (x_dungeon - 1)..=(x_dungeon + 1) {
                        for j in (y_dungeon - 1)..=(y_dungeon + 1) {
                            match dungeon.at(i, j) {
                                None => {
                                    return false;
                                }
                                Some(tile_type) => {
                                    if *tile_type != TileType::Wall {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        true
    }

    fn insert_room_at(
        &self,
        dungeon: &mut Grid<TileType>,
        room: &Grid<TileType>,
        room_to_dungeon_x: i32,
        room_to_dungeon_y: i32,
        x_room: i32,
        y_room: i32,
    ) {
        let mut stack = vec![(x_room, y_room)];

        let directions = [
            (0, -1), // North
            (0, 1),  // South
            (-1, 0), // West
            (1, 0),  // East
        ];

        while let Some((rx, ry)) = stack.pop() {
            let dx: i32 = rx + room_to_dungeon_x;
            let dy = ry + room_to_dungeon_y;

            if !dungeon.in_bounds(dx, dy) {
                continue;
            }
            // The carving must only occur on interior tiles (1 < x < width-2, etc.).
            if dx <= 0 || dx >= dungeon.width - 1 || dy <= 0 || dy >= dungeon.height - 1 {
                continue;
            }

            *dungeon.at_mut(dx, dy).unwrap() = TileType::Floor;

            // explore cardinal neighbors
            for (ox, oy) in directions {
                let nx = rx + ox;
                let ny = ry + oy;

                if !room.in_bounds(nx, ny) {
                    continue;
                }

                let room_tile = room.at(nx, ny);
                let dungeon_tile = dungeon.at(nx + room_to_dungeon_x, ny + room_to_dungeon_y);

                if !matches!(room_tile, None | Some(TileType::Wall))
                    && matches!(dungeon_tile, Some(TileType::Wall))
                {
                    stack.push((nx, ny));
                }
            }
        }
    }

    /// Generates a cavern in `map` using cellular automata and positions it in the center.
    pub fn design_cavern(
        &self,
        grid: &mut Grid<TileType>,
        min_width: usize,
        max_width: usize,
        min_height: usize,
        max_height: usize,
    ) {
        let mut blob_grid = Grid::new(grid.width, grid.height, TileType::Wall);

        // Clear the master map first
        grid.fill(TileType::Wall);

        // Generate a blob
        let (cave_x, cave_y, cave_width, cave_height) = create_blob_on_map(
            &mut blob_grid,
            5, // round_count
            min_width,
            min_height,
            max_width,
            max_height,
            55, // percent_seeded
            6,
            4,
        );

        // Find a flood fill insertion point in the blob (first non-wall tile)
        let mut found_fill_point = false;
        let mut fill_x = 0;
        let mut fill_y = 0;

        'outer: for y in 0..blob_grid.height {
            for x in 0..blob_grid.width {
                let idx = blob_grid.xy_idx(x as i32, y as i32);
                if blob_grid[idx] == TileType::Floor {
                    fill_x = x as i32;
                    fill_y = y as i32;
                    found_fill_point = true;
                    break 'outer;
                }
            }
        }

        if !found_fill_point {
            panic!("No floor tile found in blob map!");
        }

        // Position the cavern in the center of the master map
        let dest_x = (grid.width as i32 - cave_width) / 2;
        let dest_y = (grid.height as i32 - cave_height) / 2;

        // Copy the blob into the master map
        self.insert_room_at(
            grid,
            &blob_grid,
            dest_x - cave_x,
            dest_y - cave_y,
            fill_x,
            fill_y,
        );
    }

    /// Adds "loops" (extra doorways) between distant floor areas to make the dungeon less linear.
    pub fn add_loops(&self, grid: &mut Grid<TileType>, minimum_path_distance: i32) {
        let width = grid.width as usize;
        let height = grid.height as usize;
        let total_cells = width * height;

        // Shuffle all tile indices to randomize loop placement order
        let mut indices: Vec<usize> = (0..total_cells).collect();
        indices.shuffle(&mut rand::rng());

        let directions = [(1, 0), (0, 1)]; // Horizontal & vertical checks

        let mut map_for_dijkstra = Map::new(1, grid.width, grid.height, "tmp");
        map_for_dijkstra.tiles = Grid::to_cell_grid(&grid);

        for idx in indices {
            let (x, y) = grid.idx_to_xy(idx);

            // Only consider walls as potential new doors
            if grid[idx] != TileType::Wall {
                continue;
            }

            for &(dx, dy) in &directions {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                let ox = x as i32 - dx;
                let oy = y as i32 - dy;

                if !grid.in_bounds(nx, ny) || !grid.in_bounds(ox, oy) {
                    continue;
                }

                if matches!(grid.at(nx, ny), Some(TileType::Wall))
                    || matches!(grid.at(ox, oy), Some(TileType::Wall))
                {
                    continue;
                }

                // Compute Dijkstra distance between the two flanking floor tiles
                let start_idx = grid.xy_idx(nx as i32, ny as i32);
                let goal_idx = grid.xy_idx(ox as i32, oy as i32);
                let dijkstra =
                    DijkstraMap::new(width, height, &[start_idx], &map_for_dijkstra, 3000.0);
                if let Some(distance) = dijkstra.map.get(goal_idx) {
                    if *distance > minimum_path_distance as f32 {
                        // The two areas are far apart — add a connecting door here
                        grid.set(x, y, TileType::Door);
                        map_for_dijkstra.tiles[idx].dungeon = TileType::Door;

                        // Only add one door per wall tile
                        break;
                    }
                }
            }
        }
    }

    pub fn design_lakes(&self, lake_grid: &mut Grid<TileType>, game_map: &mut Map) {
        let mut temp_grid = lake_grid.clone();
        lake_grid.fill(TileType::Wall);

        let mut lake_max_height = 15;
        let mut lake_max_width = 30;

        // Loop through several generations of lakes, decreasing size each time
        while lake_max_height >= 10 {
            temp_grid.fill(TileType::Wall); // reset the temp grid

            // Generate a blob representing the current lake
            let (lake_x, lake_y, lake_width, lake_height) = create_blob_on_map(
                &mut temp_grid,
                5, // round_count
                4, // min_blob_width
                4, // min_blob_height
                lake_max_width as usize,
                lake_max_height as usize,
                55, // floor_percent
                5,  // birth_threshold
                4,  // survival_threshold
            );

            // Try to place the lake up to 20 times
            for _ in 0..20 {
                // Pick a random position for this lake
                let proposed_x = crate::rng::range(
                    1 - lake_x as i32,
                    lake_grid.width as i32 - lake_width as i32 - lake_x as i32 - 2,
                );
                let proposed_y = crate::rng::range(
                    1 - lake_y as i32,
                    lake_grid.height as i32 - lake_height as i32 - lake_y as i32 - 2,
                );

                // Only place the lake if it doesn't block pathing
                if !self.lake_disrupts_passability(
                    &game_map,
                    &temp_grid,
                    lake_grid,
                    -proposed_x,
                    -proposed_y,
                ) {
                    // Copy lake into the main lake map
                    for i in 0..lake_width {
                        for j in 0..lake_height {
                            let gx = i + lake_x as i32;
                            let gy = j + lake_y as i32;

                            if !matches!(temp_grid.at(gx, gy), Some(TileType::Wall)) {
                                let lx = gx + proposed_x;
                                let ly = gy + proposed_y;

                                if lake_grid.in_bounds(lx, ly) {
                                    lake_grid.set(lx, ly, TileType::Floor);
                                    game_map.tiles.at_mut(lx, ly).unwrap().dungeon =
                                        TileType::Floor;
                                }
                            }
                        }
                    }

                    // Done with this lake placement
                    break;
                }
            }

            lake_max_height -= 1;
            lake_max_width -= 2;
        }
    }

    pub fn lake_disrupts_passability(
        &self,
        game_map: &Map,
        grid: &Grid<TileType>,      // candidate lake shape
        lake_grid: &Grid<TileType>, // current lake layout
        dungeon_to_grid_x: i32,
        dungeon_to_grid_y: i32,
    ) -> bool {
        let mut start_x = -1;
        let mut start_y = -1;
        // --- 1. Find a valid flood fill start tile ---
        for i in 0..lake_grid.width {
            for j in 0..lake_grid.height {
                if !game_map.cell_has_terrain_flags(i, j, T_PATHING_BLOCKER)
                    && matches!(lake_grid.at(i, j), Some(TileType::Wall))
                    && (!game_map.in_bounds(i + dungeon_to_grid_x, j + dungeon_to_grid_y)
                        || matches!(
                            grid.at(i + dungeon_to_grid_x, j + dungeon_to_grid_y),
                            Some(TileType::Wall)
                        ))
                {
                    start_x = i as i32;
                    start_y = j as i32;
                    break;
                }
            }
            if start_x != -1 {
                break;
            }
        }

        // --- 2. Flood fill all reachable dry land ---
        let flood_map = self.lake_flood_fill(
            start_x,
            start_y,
            game_map,
            grid,
            lake_grid,
            dungeon_to_grid_x,
            dungeon_to_grid_y,
        );

        // --- 3. Check if any dry tiles were unreachable ---
        for i in 0..lake_grid.width {
            for j in 0..lake_grid.height {
                // No pathing blockers
                if !game_map.cell_has_terrain_flags(i, j, T_PATHING_BLOCKER)
                    // not part of a current lake
                    && matches!(lake_grid.at(i, j), Some(TileType::Wall))
                    // Tile was not reached as part of the flood fill
                    && matches!(flood_map.at(i, j), Some(TileType::Wall))
                    // C4/C5: Tile is NOT blocked by the candidate lake (`grid`).
                    // This confirms that the tile *should* have been walkable and is now an isolated dry patch,
                    // meaning the candidate lake is the cause of the break.
                    && (!game_map.in_bounds(i + dungeon_to_grid_x, j + dungeon_to_grid_y)
                        || matches!(
                            grid.at(i + dungeon_to_grid_x, j + dungeon_to_grid_y),
                            Some(TileType::Wall)
                        ))
                {
                    return true;
                }
            }
        }
        false
    }

    pub fn lake_flood_fill(
        &self,
        start_x: i32,
        start_y: i32,
        game_map: &Map,
        grid: &Grid<TileType>,
        lake_grid: &Grid<TileType>,
        dungeon_to_grid_x: i32,
        dungeon_to_grid_y: i32,
    ) -> Grid<TileType> {
        let mut flood_grid = lake_grid.clone();
        flood_grid.fill(TileType::Wall); // false/not visited

        let mut queue = VecDeque::new();
        queue.push_back((start_x, start_y));
        flood_grid.set(start_x, start_y, TileType::Floor);

        // 4-directional neighbors (N, S, E, W)
        let dirs = [Direction::N, Direction::S, Direction::E, Direction::W];

        while let Some((x, y)) = queue.pop_front() {
            for dir in dirs {
                let (dx, dy) = dir.offset().to_tuple();
                let nx = x + dx;
                let ny = y + dy;

                // in bounds
                if flood_grid.in_bounds(nx, ny)
                    // has not been flooded yet
                    && matches!(flood_grid.at(nx, ny), Some(TileType::Wall))
                    // there is no pathing blocker
                    && !game_map.cell_has_terrain_flags(nx, ny, T_PATHING_BLOCKER)
                    // not part of a current lake
                    && matches!(lake_grid.at(nx, ny), Some(TileType::Wall))
                    // not part of the candidate lake
                    && (!game_map.in_bounds(nx + dungeon_to_grid_x, ny + dungeon_to_grid_y)
                        || matches!(
                            grid.at(nx + dungeon_to_grid_x, ny + dungeon_to_grid_y),
                            Some(TileType::Wall)
                        ))
                {
                    // Mark flooded and enqueue for further spreading
                    flood_grid.set(nx, ny, TileType::Floor);
                    queue.push_back((nx, ny));
                }
            }
        }
        flood_grid
    }

    pub fn fill_lakes(&self, game_map: &mut Map, lake_grid: &mut Grid<TileType>) {
        let mut wreath_grid = lake_grid.clone();
        let (shallow_liquid, deep_liquid, shallow_liquid_width) = self.liquid_type(1);

        for i in 0..lake_grid.width {
            for j in 0..lake_grid.height {
                // Check if this tile is the start of an UNFILLED lake segment
                // The lake_map is used as the 'unfilledLakeMap' in the C version,
                // and its tiles are changed to Wall (false) by fill_lake once processed.
                if !matches!(lake_grid.at(i, j), Some(TileType::Wall)) {
                    wreath_grid.fill(TileType::Wall);
                    self.fill_lake(i, j, deep_liquid, 4, game_map, &mut wreath_grid, lake_grid);
                    self.create_wreath(
                        shallow_liquid,
                        shallow_liquid_width,
                        game_map,
                        &mut wreath_grid,
                    )
                }
            }
        }
    }

    pub fn create_wreath(
        &self,
        shallow_liquid: TileType,
        wreath_width: i32,
        game_map: &mut Map,
        wreath_grid: &mut Grid<TileType>,
    ) {
        for i in 0..game_map.width() {
            for j in 0..game_map.height() {
                if !matches!(wreath_grid.at(i, j), Some(TileType::Wall)) {
                    for k in (i - wreath_width)..=(i + wreath_width) {
                        for l in (j - wreath_width)..=(j + wreath_width) {
                            if game_map.in_bounds(k, l)
                                && matches!(
                                    game_map.tiles.at(k, l).unwrap().liquid,
                                    TileType::Empty
                                )
                                && (i - k) * (i - k) + (j - l) * (j - l)
                                    <= wreath_width * wreath_width
                            {
                                let cell = game_map.tiles.at_mut(k, l).unwrap();
                                cell.liquid = shallow_liquid;
                                cell.dungeon = TileType::Floor;
                                // if cell.dungeon == TileType::Door {
                                //     cell.dungeon = TileType::Floor;
                                // }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn clean_up_lake_boundaries(&self, game_map: &mut Map) {
        let width = game_map.width();
        let height = game_map.height();
        let mut reverse = true;
        let mut failsafe = 100;

        loop {
            let mut made_change = false;
            reverse = !reverse;
            failsafe -= 1;

            // Generate ranges for the sweep (1 to width-2, inclusive)
            let i_range = if reverse {
                (1..(width - 1)).rev().collect::<Vec<i32>>()
            } else {
                (1..(width - 1)).collect::<Vec<i32>>()
            };
            let j_range = if reverse {
                (1..(height - 1)).rev().collect::<Vec<i32>>()
            } else {
                (1..(height - 1)).collect::<Vec<i32>>()
            };

            // --- Sweep the map and attempt merges ---
            for i in i_range {
                for j in &j_range {
                    let j = *j;

                    // This variable will store the index of the cell we need to copy from,
                    // ONLY if a merge is required.
                    let mut source_idx: Option<usize> = None;

                    // --- 1. READ AND CHECK BLOCK: Immutable Borrows are Localized ---
                    // All immutable references fetched here will drop when this block ends.
                    {
                        // Fetch the center cell immutable reference
                        if let Some(c) = game_map.tiles.at(i, j) {
                            // Apply mandatory guards
                            if c.has_terrain_flags(T_LAKE_PATHING_BLOCKER | T_OBSTRUCTS_PASSABILITY)
                            {
                                let subject_flags = c.terrain_flags()
                                    & (T_LAKE_PATHING_BLOCKER | T_OBSTRUCTS_PASSABILITY);

                                // Fetch neighbor references
                                let left_cell = game_map.tiles.at(i - 1, j);
                                let right_cell = game_map.tiles.at(i + 1, j);
                                let top_cell = game_map.tiles.at(i, j - 1);
                                let bottom_cell = game_map.tiles.at(i, j + 1);

                                // --- Horizontal Merge Check ---
                                if let (Some(l), Some(r)) = (left_cell, right_cell) {
                                    let left_profile =
                                        l.terrain_flags() & T_LAKE_PATHING_BLOCKER & !subject_flags;
                                    let right_profile =
                                        r.terrain_flags() & T_LAKE_PATHING_BLOCKER & !subject_flags;

                                    if left_profile > 0 && left_profile == right_profile {
                                        // Store the index of the source cell (i+1, j)
                                        source_idx = Some(game_map.xy_idx(i + 1, j));
                                    }
                                }

                                // --- Vertical Merge Check (Only if Horizontal failed) ---
                                if source_idx.is_none() {
                                    if let (Some(t), Some(b)) = (top_cell, bottom_cell) {
                                        let top_profile = t.terrain_flags()
                                            & T_LAKE_PATHING_BLOCKER
                                            & !subject_flags;
                                        let bottom_profile = b.terrain_flags()
                                            & T_LAKE_PATHING_BLOCKER
                                            & !subject_flags;

                                        if top_profile > 0 && top_profile == bottom_profile {
                                            // Store the index of the source cell (i, j+1)
                                            source_idx = Some(game_map.xy_idx(i, j + 1));
                                        }
                                    }
                                }
                            }
                        }
                    } // ⬅️ ALL IMMUTABLE BORROWS ON `game_map.tiles` END HERE

                    // --- 2. WRITE BLOCK: Single Mutable Borrow ---
                    if let Some(s_idx) = source_idx {
                        made_change = true;
                        // Fetch the data we need to copy (immutable borrow)
                        // The center cell's reference (c_idx) is not needed yet, so this is safe.
                        let source_cell_data = game_map.tiles[s_idx]; // Since Cell is Copy, this copies the data.

                        // Fetch the mutable reference to the center cell
                        if let Some(center_cell_mut) = game_map.tiles.at_mut(i, j) {
                            // Perform the overwrite using the copied data
                            center_cell_mut.dungeon = source_cell_data.dungeon;
                            center_cell_mut.liquid = source_cell_data.liquid;
                        }
                    }
                }
            }

            // --- Exit Condition ---
            if !made_change || failsafe <= 0 {
                break;
            }
        }
    }

    pub fn fill_lake(
        &self,
        start_x: i32,
        start_y: i32,
        liquid: TileType,
        scan_width: i32,
        game_map: &mut Map,
        wreath_grid: &mut Grid<TileType>,
        lake_grid: &mut Grid<TileType>,
    ) {
        let mut stack = vec![(start_x, start_y)];

        while let Some((x, y)) = stack.pop() {
            // Mark the current tile as visited (Wall) and fill it with liquid
            if game_map.in_bounds(x, y) && !matches!(lake_grid.at(x, y), Some(TileType::Wall)) {
                lake_grid.set(x, y, TileType::Wall); // Mark as visited/filled
                game_map.tiles.at_mut(x, y).unwrap().liquid = liquid;
                wreath_grid.set(x, y, TileType::Floor); // Mark footprint
            } else {
                continue; // Skip processing if already visited or out of bounds/not part of lake
            }

            // Scan the box around the current tile to find unvisited lake tiles
            for i in (x - scan_width)..=(x + scan_width) {
                for j in (y - scan_width)..=(y + scan_width) {
                    if game_map.in_bounds(i, j)
                        // Only push to stack if it's an UNVISITED part of the lake
                        && !matches!(lake_grid.at(i, j), Some(TileType::Wall))
                    {
                        // Check if it's already in the stack to prevent massive duplication
                        // (While not strictly necessary for correctness, it's good practice)
                        if !stack.contains(&(i, j)) {
                            stack.push((i, j));
                        }
                    }
                }
            }
        }
    }

    pub fn liquid_type(&self, depth_level: i32) -> (TileType, TileType, i32) {
        (TileType::Obsidian, TileType::Lava, 2)
        // (TileType::ShallowWater, TileType::DeepWater, 2)
    }

    // --- GENERATION FUNCTIONS ---

    /// Finds a random location in the dungeon matching required cell types.
    /// Returns `Some((row, col))` if a match is found within 500 tries, otherwise `None`.
    pub fn random_matching_location(
        &self,
        dungeon: &Map,
        dungeon_types: &Vec<TileType>,
        liquid_types: &Vec<TileType>,
    ) -> Option<usize> {
        let width = dungeon.width();
        let height = dungeon.height();
        let total_cells = (width * height) as usize;
        let mut randomized_coords: Vec<usize> = (0..total_cells).collect();
        randomized_coords.shuffle(&mut rand::rng());

        for (i, cell_index) in randomized_coords.iter().enumerate() {
            if i >= 500 {
                break; // Stop after 500 attempts
            }
            let cell_type = dungeon.tiles[*cell_index];

            // Check against required dungeon types
            let passes_dungeon_check =
                dungeon_types.is_empty() || dungeon_types.contains(&cell_type.dungeon);

            // Check against required liquid types
            let passes_liquid_check =
                liquid_types.is_empty() || liquid_types.contains(&cell_type.liquid);

            // TODO - don't put terrain over top of itself

            if passes_dungeon_check && passes_liquid_check {
                return Some(*cell_index);
            }
        }

        None
    }

    /// Merges the temporary `spawn_map` onto the main `hyperspace` map based on tile priority.
    /// Update to match https://github.com/tsadok/brogue/blob/master/src/brogue/Architect.c#L3139
    pub fn fill_spawn_map(&self, hyperspace: &mut Map, spawn_map: &Grid<TileType>) {
        let mut tile_updates = Vec::new();
        for y in 0..hyperspace.height() {
            for x in 0..hyperspace.width() {
                let new_tile = spawn_map.at(x, y).unwrap();
                // Only consider tiles that were actually set by the generator (not CELL_TYPE_EMPTY)
                if !matches!(new_tile, TileType::Empty) {
                    // Check to see if any map features spawned before this already have priority
                    let current_tile = hyperspace.tiles.at(x, y).unwrap().features;
                    let fill_tile_priority = new_tile.data().priority;
                    let current_tile_priority = current_tile.data().priority;

                    // If the current cell is empty OR the new feature has STRICTLY higher priority
                    if current_tile == TileType::Empty || fill_tile_priority > current_tile_priority
                    {
                        tile_updates.push((x, y, new_tile));
                    }
                }
            }
        }

        for (x, y, tile) in tile_updates {
            hyperspace.tiles.at_mut(x, y).unwrap().features = *tile;
        }
    }

    /// The core propagation logic, equivalent to `spawnMapDF`.
    /// Returns the resulting spawn map (`Vec<Vec<CellType>>`) and whether any changes were successfully made.
    pub fn spawn_map_df(
        &self,
        start_idx: usize,
        hyperspace: &Map,
        propagation_terrains: Option<&Vec<TileType>>,
        start_probability: i32,
        probability_slope: i32,
        propagate: bool,
        tile: TileType,
    ) -> (Grid<TileType>, bool) {
        let width = hyperspace.width();
        let height = hyperspace.height();
        let map_size = (width * height) as usize;
        // `spawn_map` temporarily stores the propagation distance/step (1, 2, 3...)
        let mut spawn_map: Grid<i32> = Grid::new(width, height, 0);
        let mut frontier: Vec<usize> = Vec::with_capacity(map_size / 4); // Indices of cells spreading in the current step

        // Initial tile placement
        spawn_map[start_idx] = 1;

        let mut successful = !propagate;
        let require_propagation_terrain = propagation_terrains.is_some();
        let mut t: i32 = 1;
        let mut probability = start_probability;

        // If we need to propagate, start the frontier with the initial index
        if propagate && probability > 0 {
            frontier.push(start_idx);
            successful = false; // Will be set to true if successful spread occurs
        }

        // 2. High-Performance Propagation Loop (using Frontier)
        while !frontier.is_empty() && propagate && probability > 0 {
            let current_frontier = std::mem::take(&mut frontier); // Move old frontier out, creating a new empty one
            t += 1;

            // Iterate ONLY over the cells that propagated in the previous step
            for current_idx in current_frontier {
                let r = current_idx / width as usize;
                let c = current_idx % width as usize;

                // Use the requested 4-way cardinal directions
                for (dx, dy) in Direction::cardinals()
                    .into_iter()
                    .map(|p| p.offset().to_tuple())
                {
                    let r2 = r as i32 + dy;
                    let c2 = c as i32 + dx;

                    if !spawn_map.in_bounds(r2, c2) {
                        continue;
                    }

                    let offset_idx = spawn_map.xy_idx(c2, r2);
                    let neighbor_cell = hyperspace.tiles[offset_idx];

                    if neighbor_cell.has_terrain_flags(T_OBSTRUCTS_SURFACE_EFFECTS) {
                        continue;
                    }

                    // Check terrain requirement
                    let terrain_ok = if require_propagation_terrain {
                        if let Some(terrains) = propagation_terrains {
                            terrains.contains(&neighbor_cell.dungeon)
                                || terrains.contains(&neighbor_cell.liquid)
                                || terrains.contains(&neighbor_cell.features)
                        } else {
                            false
                        }
                    } else {
                        true
                    };

                    // Check if propagation is possible, random chance hits, and spot hasn't been hit yet (0)
                    if terrain_ok
                        && spawn_map[offset_idx] == 0
                        && crate::rng::range(0, 100) < probability
                    {
                        spawn_map[offset_idx] = t;
                        frontier.push(offset_idx); // Add to the next step's frontier
                        successful = true;
                    }
                }
            }

            // Decay the probability for the next step's propagation
            probability -= probability_slope;
        }

        let start_tile = hyperspace.tiles[start_idx];

        let passes_start_terrain_check = if require_propagation_terrain {
            let terrains = propagation_terrains.as_ref().unwrap(); // Safe unwrap due to require_propagation_terrain
            terrains.contains(&start_tile.dungeon)
                || terrains.contains(&start_tile.liquid)
                || terrains.contains(&start_tile.features)
        } else {
            true
        };

        if !passes_start_terrain_check {
            spawn_map[start_idx] = 0;
        }

        let mut final_spawn_map: Grid<TileType> =
            Grid::new(spawn_map.width, spawn_map.height, TileType::Empty);
        for (i, cell) in spawn_map.data.iter().enumerate() {
            if *cell > 0 {
                final_spawn_map.data[i] = tile;
            }
        }

        (final_spawn_map, successful)
    }

    /// Attempts to spawn a feature and recursively spawns any subsequent features.
    pub fn spawn_dungeon_feature(
        &self,
        idx: usize,
        hyperspace: &Map,
        feature: &DungeonFeatureConfig,
    ) -> Grid<TileType> {
        // 1. Initial spawning attempt
        let (mut spawn_map, successful) = self.spawn_map_df(
            idx,
            hyperspace,
            feature.propagation_terrains.as_ref(), // Pass Option<&Vec>
            feature.start.unwrap_or(0),
            feature.decr.unwrap_or(0),
            feature.propagate,
            feature.tile,
        );

        // 2. Handle subsequent features (e.g., Grass -> Foliage)
        if successful {
            if let Some(subsequent_feature_id) = feature.subsequent_feature {
                let feature_catalog = get_dungeon_feature_catalog();

                if let Some(subsequent_feature) = feature_catalog.get(&subsequent_feature_id) {
                    // Recursively spawn the subsequent feature using the same coordinates
                    let subsequent_spawn_map =
                        self.spawn_dungeon_feature(idx, hyperspace, subsequent_feature);

                    // Merge the subsequent feature map onto the primary spawn map
                    // Subsequent features should overwrite the base feature terrain.
                    draw_continuous_shape_on_grid(&subsequent_spawn_map, &mut spawn_map);
                }
            }
        }

        // If the spawn was not successful, return an empty map
        if !successful {
            return Grid::new(hyperspace.width(), hyperspace.height(), TileType::Empty);
        }

        spawn_map
    }

    /// The main generation loop, equivalent to `runAutogenerators`.
    pub fn run_autogenerators(&self, game_map: &mut Map, layer: i32, depth: i32) {
        let autogenerator_catalog = get_auto_generator_catalog();

        for autogenerator in autogenerator_catalog.iter() {
            if depth < autogenerator.min_depth || depth > autogenerator.max_depth {
                continue;
            }
            if autogenerator.layer != layer {
                continue;
            }

            // 1. Calculate the base number of feature spawns based on depth and slope
            let depth_scaling = autogenerator.min_intercept + depth * autogenerator.min_slope;
            let base_count = depth_scaling / 100;
            let mut count = base_count.min(autogenerator.max_number);

            // 2. Randomly increase count based on frequency
            while crate::rng::range(0, 100) < autogenerator.frequency
                && count < autogenerator.max_number
            {
                count += 1;
            }

            // 3. Spawn features
            for _i in 0..count {
                // Find a random location matching the requirements
                if let Some(idx) = self.random_matching_location(
                    &game_map,
                    &autogenerator.req_dungeon,
                    &autogenerator.req_liquid,
                ) {
                    // Get the specific feature configuration
                    let feature_catalog = get_dungeon_feature_catalog();
                    if let Some(feature_config) = feature_catalog.get(&autogenerator.feature) {
                        // Generate the temporary spawn map for this feature
                        let spawn_map = self.spawn_dungeon_feature(
                            idx,
                            &game_map, // Dungeon map serves as the existing terrain
                            feature_config,
                        );

                        // Merge the new feature map onto the hyperspace map
                        self.fill_spawn_map(game_map, &spawn_map);
                    }
                }
            }
        }
    }

    /// Translates raw Granite (unexposed rock) into Wall if it touches open space,
    /// and solidifies fully-enclosed Walls back into Granite.
    pub fn finish_walls(&self, hyperspace: &mut Map, including_diagonals: bool) {
        let width = hyperspace.width();
        let height = hyperspace.height();

        // Use a vector to collect changes to avoid modifying the map while iterating it.
        let mut changes: Vec<(usize, TileType)> = Vec::new();

        let directions = if including_diagonals {
            Direction::iter()
        } else {
            Direction::cardinals()
        };

        for r in 0..height {
            for c in 0..width {
                let current_idx = hyperspace.tiles.xy_idx(c, r);
                let current_tile_type = hyperspace.tiles[current_idx].dungeon;
                let mut found_exposure = false;

                // --- Helper function to check for exposure ---
                // Exposed means the neighbor is NOT fully obstructing.
                for (dx, dy) in directions.iter().map(|dir| dir.offset().to_tuple()) {
                    let c2 = c + dx;
                    let r2 = r + dy;

                    if hyperspace.tiles.in_bounds(c2, r2) {
                        let neighbor_idx = hyperspace.tiles.xy_idx(c2, r2);
                        let neighbor_cell = hyperspace.tiles[neighbor_idx];

                        if !neighbor_cell.has_terrain_flags(T_OBSTRUCTS_PASSABILITY)
                            || !neighbor_cell.has_terrain_flags(T_OBSTRUCTS_VISION)
                        {
                            found_exposure = true;
                            // Break the inner loop as soon as exposure is found
                            break;
                        }
                    }
                }
                if current_tile_type == TileType::Granite {
                    // Logic 1: GRANITE -> WALL
                    // If a GRANITE tile has any exposed neighbors, it becomes a WALL.
                    if found_exposure {
                        changes.push((current_idx, TileType::Wall));
                    }
                } else if current_tile_type == TileType::Wall {
                    // Logic 2: WALL -> GRANITE
                    // If a WALL tile has NO exposed neighbors (i.e., foundExposure is false), it becomes GRANITE.
                    if !found_exposure {
                        changes.push((current_idx, TileType::Granite));
                    }
                }
            }
        }

        // Apply all gathered changes to the map
        for (idx, new_tile) in changes {
            if let Some(cell) = hyperspace.tiles.data.get_mut(idx) {
                cell.dungeon = new_tile;
            }
        }
    }

    pub fn build_a_bridge(&self, map: &mut Map) -> bool {
        // bridgeRatioX = (short) (100 + (100 + 100 * rogue.depthLevel * gameConst->depthAccelerator / 9) * rand_range(10, 20) / 10);
        // bridgeRatioY = (short) (100 + (400 + 100 * rogue.depthLevel * gameConst->depthAccelerator / 18) * rand_range(10, 20) / 10);
        let bridge_ratio_x = 100 + 100 * crate::rng::range(10, 20) / 20;
        let bridge_ratio_y = 100 + 100 * crate::rng::range(10, 20) / 20;

        let mut n_rows: Vec<i32> = (0..self.height).collect();
        let mut n_cols: Vec<i32> = (0..self.width).collect();

        n_rows.shuffle(&mut rand::rng());
        n_cols.shuffle(&mut rand::rng());

        for i2 in 1..self.width {
            let i = n_cols[i2 as usize];
            for j2 in 1..self.height {
                let j = n_rows[j2 as usize];

                if !map.cell_has_terrain_flags(i, j, T_CAN_BE_BRIDGED | T_PATHING_BLOCKER) {
                    let mut found_exposure = false;
                    let mut k = i + 1; // k is the current x-coordinate of the bridge segment

                    // Try a horizontal bridge
                    while k < self.width 
                        && map.cell_has_terrain_flags(k, j, T_CAN_BE_BRIDGED) // Candidate tile must be spannable by a bridge
                        && !map.cell_has_terrain_flags(k, j, T_OBSTRUCTS_PASSABILITY)  // Candidate tile cannot be a wall
                        && map.cell_has_terrain_flags(
                            k,
                            j - 1,
                            T_CAN_BE_BRIDGED | T_OBSTRUCTS_PASSABILITY,
                        ) // // Only chasms or walls are permitted next to the length of the bridge
                        && map.cell_has_terrain_flags(
                            k,
                            j + 1,
                            T_CAN_BE_BRIDGED | T_OBSTRUCTS_PASSABILITY,
                        )
                    {

                        if !map.cell_has_terrain_flags(k, j - 1, T_OBSTRUCTS_PASSABILITY)
                            && !map.cell_has_terrain_flags(k, j + 1, T_OBSTRUCTS_PASSABILITY)
                        {
                            found_exposure = true;
                        }
                        k += 1;
                    }
                    if k < self.width 
                    && (k - i > 3) // Can't have bridges shorter than 3 spaces
                    && found_exposure
                    && !map.cell_has_terrain_flags(k, j, T_PATHING_BLOCKER | T_CAN_BE_BRIDGED) // Must end on an unobstructed land tile.
                    // && 100 * pathingDistance(i, j, k, j, T_PATHING_BLOCKER) / (k - i) > bridgeRatioX) { // Must shorten the pathing distance enough.
                    {
                        for l in i + 1..k {
                            map.tiles.at_mut(l, j).unwrap().features = TileType::Bridge;
                        }
                        map.tiles.at_mut(i, j).unwrap().features = TileType::Bridge;
                        map.tiles.at_mut(k, j).unwrap().features = TileType::Bridge;
                        return true;
                    }

                    // Try a vertical bridge
                    found_exposure = false;
                    k = j + 1; // reset
                    while k < self.height 
                        && map.cell_has_terrain_flags(i, k, T_CAN_BE_BRIDGED) // Candidate tile must be spannable by a bridge
                        && !map.cell_has_terrain_flags(i, k, T_OBSTRUCTS_PASSABILITY)  // Candidate tile cannot be a wall
                        && map.cell_has_terrain_flags(
                            i - 1,
                            k,
                            T_CAN_BE_BRIDGED | T_OBSTRUCTS_PASSABILITY,
                        ) // // Only chasms or walls are permitted next to the length of the bridge
                        && map.cell_has_terrain_flags(
                            i + 1,
                            k,
                            T_CAN_BE_BRIDGED | T_OBSTRUCTS_PASSABILITY,
                        )
                    {

                        if !map.cell_has_terrain_flags(i - 1, k, T_OBSTRUCTS_PASSABILITY)
                            && !map.cell_has_terrain_flags(i + 1, k, T_OBSTRUCTS_PASSABILITY)
                        {
                            found_exposure = true;
                        }
                        k += 1;
                    }
                    if k < self.height 
                    && (k - j > 3) // Can't have bridges shorter than 3 spaces
                    && found_exposure
                    && !map.cell_has_terrain_flags(i, k, T_PATHING_BLOCKER | T_CAN_BE_BRIDGED) // Must end on an unobstructed land tile.
                    // && 100 * pathingDistance(i, j, k, j, T_PATHING_BLOCKER) / (k - i) > bridgeRatioX) { // Must shorten the pathing distance enough.
                    {
                        for l in j + 1..k {
                            map.tiles.at_mut(i, l).unwrap().features = TileType::Bridge;
                        }
                        map.tiles.at_mut(i, j).unwrap().features = TileType::Bridge;
                        map.tiles.at_mut(i, k).unwrap().features = TileType::Bridge;
                        return true;
                    }
                }
            }
        }

        false
    }
}

impl InitialMapBuilder for BrogueLikeBuilder {
    fn build_map(&mut self, mut build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;
        let dungeon_profile = DungeonProfile::default(depth);
        let first_room_dungeon_profile = DungeonProfile::first_room(depth);

        // 1. Initial Room Placement
        let (mut grid, _) =
            self.design_random_room(false, false, &first_room_dungeon_profile.room_frequencies);
        build_data.map.tiles = Grid::to_cell_grid(&grid);
        build_data.take_snapshot();

        self.attach_rooms(&mut build_data, &dungeon_profile, 35, 35);
        build_data.take_snapshot();

        // Since we modified the map directly, we need to create a new grid copy from it
        grid = Grid::from_cell_grid(&build_data.map.tiles);
        self.add_loops(&mut grid, 20);
        build_data.map.tiles = Grid::to_cell_grid(&grid);
        build_data.take_snapshot();

        // Not every door will be generated
        for cell in build_data.map.tiles.iter_mut() {
            if cell.dungeon == TileType::Door {
                cell.dungeon = if crate::rng::percent(dungeon_profile.door_chance) {
                    TileType::Door
                } else {
                    TileType::Floor
                };
            }
        }
        build_data.take_snapshot();

        self.finish_walls(&mut build_data.map, false);
        build_data.take_snapshot();

        // Time to add lakes and chasms. Strategy is to generate a series of blob lakes of decreasing size. For each lake,
        // propose a position, and then check via a flood fill that the level would remain connected with that placement (i.e. that
        // each passable tile can still be reached). If not, make 9 more placement attempts before abandoning that lake
        // and proceeding to generate the next smaller one.
        // Canvas sizes start at 30x15 and decrease by 2x1 at a time down to a minimum of 20x10. Min generated size is always 4x4.

        let mut lake_grid: Grid<TileType> = Grid::new(self.width, self.height, TileType::Wall);
        self.design_lakes(&mut lake_grid, &mut build_data.map);
        self.fill_lakes(&mut build_data.map, &mut lake_grid);

        build_data.take_snapshot();

        self.clean_up_lake_boundaries(&mut build_data.map);
        build_data.take_snapshot();

        self.run_autogenerators(&mut build_data.map, 0, depth); // dungeon
        build_data.take_snapshot();
        // self.run_autogenerators(&mut build_data.map, 1, depth);  // liquid
        // build_data.take_snapshot();
        // self.run_autogenerators(&mut build_data.map, 2, depth);  // features
        // build_data.take_snapshot();

        // while self.build_a_bridge(&mut build_data.map) {}

        self.finish_walls(&mut build_data.map, true);
        build_data.take_snapshot();

        recalculate_static_lighting(&mut build_data.map);
        build_data.take_snapshot();
        build_data.rooms = Some(self.rooms.clone());
        build_data.take_snapshot();
    }

    fn clone_box(&self) -> Box<dyn InitialMapBuilder> {
        Box::new(self.clone())
    }
}

fn draw_continuous_shape_on_grid(room: &Grid<TileType>, grid: &mut Grid<TileType>) {
    for (idx, tile) in room.data.iter().enumerate() {
        if *tile != TileType::Empty {
            grid.data[idx] = *tile;
        }
    }
}
