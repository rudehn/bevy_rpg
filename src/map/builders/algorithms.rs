use std::collections::{VecDeque, HashSet};
use rand::Rng; // Add this for rng.gen_range

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlobType {
    #[default]
    Wall,
    Floor,
}

// --- Grid<T> Definition ---
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T>
where
    T: Copy + Clone + PartialEq + Default,
{
    pub data: Vec<T>,
    pub width: i32,
    pub height: i32,
}

impl<T> Grid<T>
where
    T: Copy + Clone + PartialEq + Default,
{
    pub fn new(width: i32, height: i32, default_value: T) -> Self {
        let size = (width * height) as usize;
        Grid {
            data: vec![default_value; size],
            width,
            height,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn xy_idx(&self, x: i32, y: i32) -> usize {
        (y as usize * self.width as usize) + x as usize
    }

    pub fn idx_to_xy(&self, idx: usize) -> (i32, i32) {
        let x = idx as i32 % self.width;
        let y = idx as i32 / self.width;
        (x, y)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width && y >= 0 && y < self.height
    }

    pub fn at(&self, x: i32, y: i32) -> Option<&T> {
        if self.in_bounds(x, y) {
            Some(&self.data[self.xy_idx(x, y)])
        } else {
            None
        }
    }

    pub fn at_mut(&mut self, x: i32, y: i32) -> Option<&mut T> {
        if self.in_bounds(x, y) {
            let idx = self.xy_idx(x, y);
            Some(&mut self.data[idx])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: i32, y: i32, value: T) {
        if let Some(tile) = self.at_mut(x, y) {
            *tile = value;
        }
    }

    pub fn fill(&mut self, fill_value: T) {
        for val in self.data.iter_mut() {
            *val = fill_value;
        }
    }
}

// --- FloodFillResult Struct ---
#[derive(Debug, Clone)]
pub struct FloodFillResult {
    pub size: usize,
    pub tiles: Vec<usize>,
}

// --- BlobGenConfig Struct ---
#[derive(Debug, Clone, Copy)]
pub struct BlobGenConfig {
    pub round_count: usize,
    pub min_blob_width: i32,
    pub min_blob_height: i32,
    pub max_blob_width: i32,
    pub max_blob_height: i32,
    pub initial_alive_percent: i32,
    pub birth_threshold: i32,
    pub survival_threshold: i32,
}

// --- Cellular Automata Helper Functions ---

pub fn count_neighbors<T>(grid: &Grid<T>, x: i32, y: i32, radius: i32, floor_val: T) -> i32 
where T: Copy + Clone + PartialEq + Default
{
    let mut count = 0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx == 0 && dy == 0 { continue; }
            let nx = x + dx;
            let ny = y + dy;
            if grid.in_bounds(nx, ny) {
                if *grid.at(nx, ny).unwrap() == floor_val {
                    count += 1;
                }
            }
        }
    }
    count
}

pub fn randomize_grid<T>(grid: &mut Grid<T>, alive_percent: i32, floor_val: T, wall_val: T) 
where T: Copy + Clone + PartialEq + Default
{
    let mut rng = rand::rng(); 
    for val in grid.data.iter_mut() {
        if rng.gen_range(0..100) < alive_percent {
            *val = floor_val;
        } else {
            *val = wall_val;
        }
    }
}

pub fn cellular_automata_iteration<T>(grid: &mut Grid<T>, birth_threshold: i32, survival_threshold: i32, floor_val: T, wall_val: T) 
where T: Copy + Clone + PartialEq + Default
{
    let old_grid = grid.clone(); 
    for y in 1..grid.height - 1 {
        for x in 1..grid.width - 1 {
            let idx = grid.xy_idx(x, y);
            let neighbors = count_neighbors(&old_grid, x, y, 1, floor_val); 

            if old_grid.data[idx] == wall_val {
                if neighbors >= birth_threshold {
                    grid.data[idx] = floor_val; 
                }
            } else {
                if neighbors < survival_threshold {
                    grid.data[idx] = wall_val; 
                }
            }
        }
    }
}

pub fn flood_fill_region<T>(
    grid: &Grid<T>,
    start_x: i32,
    start_y: i32,
    connectivity: u8,
    floor_val: T,
    wall_val: T,
) -> FloodFillResult 
where T: Copy + Clone + PartialEq + Default
{
    let mut visited: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut region_tiles: Vec<usize> = Vec::new();

    if !grid.in_bounds(start_x, start_y) || *grid.at(start_x, start_y).unwrap_or(&wall_val) == wall_val {
        return FloodFillResult { size: 0, tiles: vec![] };
    }

    let start_idx = grid.xy_idx(start_x, start_y);
    queue.push_back(start_idx);
    visited.insert(start_idx);

    let deltas: Vec<(i32, i32)> = if connectivity == 4 {
        vec![(0, -1), (0, 1), (-1, 0), (1, 0)]
    } else { // 8-connectivity
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| !(*dx == 0 && *dy == 0))
            .collect()
    };

    while let Some(current_idx) = queue.pop_front() {
        region_tiles.push(current_idx);
        let (cx, cy) = grid.idx_to_xy(current_idx);

        for (dx, dy) in &deltas {
            let nx = cx + dx;
            let ny = cy + dy;

            if grid.in_bounds(nx, ny) {
                let n_idx = grid.xy_idx(nx, ny);
                if !visited.contains(&n_idx) && *grid.at(nx, ny).unwrap() == floor_val {
                    visited.insert(n_idx);
                    queue.push_back(n_idx);
                }
            }
        }
    }

    FloodFillResult {
        size: region_tiles.len(),
        tiles: region_tiles,
    }
}

pub fn get_all_regions<T>(grid: &Grid<T>, floor_val: T, wall_val: T) -> Vec<FloodFillResult> 
where T: Copy + Clone + PartialEq + Default
{
    let mut regions: Vec<FloodFillResult> = Vec::new();
    let mut visited_all: HashSet<usize> = HashSet::new();

    for y in 0..grid.height {
        for x in 0..grid.width {
            let idx = grid.xy_idx(x, y);
            if *grid.at(x, y).unwrap() == floor_val && !visited_all.contains(&idx) {
                let result = flood_fill_region(grid, x, y, 8, floor_val, wall_val); 
                if result.size > 0 {
                    for &i in &result.tiles {
                        visited_all.insert(i);
                    }
                    regions.push(result);
                }
            }
        }
    }
    regions
}

pub fn retain_specific_region<T>(grid: &mut Grid<T>, region: &FloodFillResult, wall_val: T) 
where T: Copy + Clone + PartialEq + Default
{
    let region_set: HashSet<usize> = region.tiles.iter().cloned().collect();
    for idx in 0..grid.len() {
        if !region_set.contains(&idx) {
            grid.data[idx] = wall_val;
        }
    }
}

pub fn size_score(width: i32, height: i32, min_w: i32, min_h: i32, max_w: i32, max_h: i32) -> f32 {
    let clamped_w = width.clamp(min_w, max_w);
    let clamped_h = height.clamp(min_h, max_h);

    let width_score = (clamped_w - min_w) as f32 / (max_w - min_w).max(1) as f32;
    let height_score = (clamped_h - min_h) as f32 / (max_h - min_h).max(1) as f32;

    let overshoot_penalty = if width > max_w || height > max_h { 0.5 } else { 1.0 };

    ((width_score + height_score) / 2.0) * overshoot_penalty
}

pub fn create_blob<T>(
    initial_grid: &Grid<T>, 
    config: &BlobGenConfig,
    floor_val: T,
    wall_val: T,
) -> (Grid<T>, i32, i32, i32, i32) 
where T: Copy + Clone + PartialEq + Default
{
    let mut best_blob: Option<(Grid<T>, i32, i32, i32, i32, f32)> = None; 

    for _ in 0..50 { 
        let mut current_grid = Grid::new(initial_grid.width, initial_grid.height, wall_val);

        randomize_grid(&mut current_grid, config.initial_alive_percent, floor_val, wall_val);

        for _ in 0..config.round_count {
            cellular_automata_iteration(&mut current_grid, config.birth_threshold, config.survival_threshold, floor_val, wall_val);
        }

        let regions = get_all_regions(&current_grid, floor_val, wall_val);
        if regions.is_empty() {
            continue;
        }

        for region in regions {
            let mut blob_grid = current_grid.clone();
            retain_specific_region(&mut blob_grid, &region, wall_val);

            let (min_x, max_x, min_y, max_y) = region.tiles.iter().fold(
                (initial_grid.width, 0, initial_grid.height, 0),
                |(min_x, max_x, min_y, max_y), &idx| {
                    let (x, y) = initial_grid.idx_to_xy(idx);
                    (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y))
                },
            );

            let blob_width = max_x - min_x + 1;
            let blob_height = max_y - min_y + 1;

            let score = size_score(
                blob_width,
                blob_height,
                config.min_blob_width,
                config.min_blob_height,
                config.max_blob_width,
                config.max_blob_height,
            );

            match &best_blob {
                Some((_, _, _, _, _, best_score)) if *best_score >= score => {}
                _ => {
                    best_blob = Some((
                        blob_grid.clone(), 
                        min_x,
                        min_y,
                        blob_width,
                        blob_height,
                        score,
                    ));
                }
            }

            if blob_width >= config.min_blob_width
                && blob_height >= config.min_blob_height
                && blob_width <= config.max_blob_width
                && blob_height <= config.max_blob_height
            {
                return (blob_grid.clone(), min_x, min_y, blob_width, blob_height);
            }
        }
    }

    if let Some((grid, min_x, min_y, blob_width, blob_height, _)) = best_blob {
        (grid.clone(), min_x, min_y, blob_width, blob_height)
    } else {
        (
            Grid::new(initial_grid.width, initial_grid.height, wall_val),
            0,
            0,
            initial_grid.width,
            initial_grid.height,
        )
    }
}
