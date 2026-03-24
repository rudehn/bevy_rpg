use std::collections::VecDeque;
use rand::Rng;

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        if rng.random_range(0..100) < alive_percent {
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
    let size = grid.data.len();
    let mut visited = vec![false; size];
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut region_tiles: Vec<usize> = Vec::new();

    if !grid.in_bounds(start_x, start_y) || *grid.at(start_x, start_y).unwrap_or(&wall_val) == wall_val {
        return FloodFillResult { size: 0, tiles: vec![] };
    }

    let start_idx = grid.xy_idx(start_x, start_y);
    queue.push_back(start_idx);
    visited[start_idx] = true;

    let deltas: &[(i32, i32)] = if connectivity == 4 {
        &[(0, -1), (0, 1), (-1, 0), (1, 0)]
    } else {
        &[(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]
    };

    while let Some(current_idx) = queue.pop_front() {
        region_tiles.push(current_idx);
        let (cx, cy) = grid.idx_to_xy(current_idx);

        for &(dx, dy) in deltas {
            let nx = cx + dx;
            let ny = cy + dy;

            if grid.in_bounds(nx, ny) {
                let n_idx = grid.xy_idx(nx, ny);
                if !visited[n_idx] && grid.data[n_idx] == floor_val {
                    visited[n_idx] = true;
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
    let mut visited_all = vec![false; grid.data.len()];

    for y in 0..grid.height {
        for x in 0..grid.width {
            let idx = grid.xy_idx(x, y);
            if grid.data[idx] == floor_val && !visited_all[idx] {
                let result = flood_fill_region(grid, x, y, 8, floor_val, wall_val);
                if result.size > 0 {
                    for &i in &result.tiles {
                        visited_all[i] = true;
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
    let mut keep = vec![false; grid.data.len()];
    for &idx in &region.tiles {
        keep[idx] = true;
    }
    for idx in 0..grid.len() {
        if !keep[idx] {
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

            let perfect_fit = blob_width >= config.min_blob_width
                && blob_height >= config.min_blob_height
                && blob_width <= config.max_blob_width
                && blob_height <= config.max_blob_height;

            let dominated = match &best_blob {
                Some((_, _, _, _, _, best_score)) => *best_score >= score,
                None => false,
            };

            if perfect_fit || !dominated {
                let mut blob_grid = current_grid.clone();
                retain_specific_region(&mut blob_grid, &region, wall_val);

                if perfect_fit {
                    return (blob_grid, min_x, min_y, blob_width, blob_height);
                }

                best_blob = Some((blob_grid, min_x, min_y, blob_width, blob_height, score));
            }
        }
    }

    if let Some((grid, min_x, min_y, blob_width, blob_height, _)) = best_blob {
        (grid, min_x, min_y, blob_width, blob_height)
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- Grid basics ---

    #[test]
    fn grid_new_dimensions() {
        let g: Grid<i32> = Grid::new(5, 3, 0);
        assert_eq!(g.width, 5);
        assert_eq!(g.height, 3);
        assert_eq!(g.len(), 15);
    }

    #[test]
    fn grid_xy_idx_round_trip() {
        let g: Grid<i32> = Grid::new(10, 8, 0);
        for y in 0..g.height {
            for x in 0..g.width {
                let idx = g.xy_idx(x, y);
                let (rx, ry) = g.idx_to_xy(idx);
                assert_eq!((x, y), (rx, ry), "Round-trip failed for ({x}, {y})");
            }
        }
    }

    #[test]
    fn grid_in_bounds() {
        let g: Grid<i32> = Grid::new(5, 3, 0);
        assert!(g.in_bounds(0, 0));
        assert!(g.in_bounds(4, 2));
        assert!(!g.in_bounds(-1, 0));
        assert!(!g.in_bounds(5, 0));
        assert!(!g.in_bounds(0, 3));
    }

    #[test]
    fn grid_at_and_set() {
        let mut g: Grid<i32> = Grid::new(4, 4, 0);
        g.set(2, 1, 42);
        assert_eq!(*g.at(2, 1).unwrap(), 42);
        assert_eq!(*g.at(0, 0).unwrap(), 0);
        // Out-of-bounds returns None
        assert!(g.at(10, 10).is_none());
    }

    // --- count_neighbors ---

    #[test]
    fn count_neighbors_known_grid() {
        // 3x3 grid, center surrounded by 8 floor tiles
        let mut g = Grid::new(3, 3, BlobType::Wall);
        for y in 0..3 {
            for x in 0..3 {
                if !(x == 1 && y == 1) {
                    g.set(x, y, BlobType::Floor);
                }
            }
        }
        assert_eq!(count_neighbors(&g, 1, 1, 1, BlobType::Floor), 8);
        // Corner (0,0) has 2 floor neighbors at radius 1 (not counting itself)
        // Neighbors of (0,0): (1,0), (0,1), (1,1) — but (1,1) is Wall
        assert_eq!(count_neighbors(&g, 0, 0, 1, BlobType::Floor), 2);
    }

    // --- cellular_automata_iteration ---

    #[test]
    fn cellular_automata_birth_and_survival() {
        // 5x5 grid, all wall except center cross of floor
        let mut g = Grid::new(5, 5, BlobType::Wall);
        // Cross pattern: (2,1), (1,2), (2,2), (3,2), (2,3)
        g.set(2, 1, BlobType::Floor);
        g.set(1, 2, BlobType::Floor);
        g.set(2, 2, BlobType::Floor);
        g.set(3, 2, BlobType::Floor);
        g.set(2, 3, BlobType::Floor);

        // birth_threshold=3, survival_threshold=2
        // Center (2,2) has 4 floor neighbors → survives (4 >= 2)
        // Wall at (1,1) has 3 floor neighbors: (2,1), (1,2), (2,2) → births (3 >= 3)
        cellular_automata_iteration(&mut g, 3, 2, BlobType::Floor, BlobType::Wall);

        assert_eq!(*g.at(2, 2).unwrap(), BlobType::Floor, "center should survive");
        assert_eq!(*g.at(1, 1).unwrap(), BlobType::Floor, "(1,1) should birth");
    }

    // --- flood_fill_region ---

    #[test]
    fn flood_fill_4_connectivity() {
        // 5x5 grid with L-shaped floor region
        let mut g = Grid::new(5, 5, BlobType::Wall);
        // Horizontal: (0,0), (1,0), (2,0)
        // Vertical: (0,1), (0,2)
        g.set(0, 0, BlobType::Floor);
        g.set(1, 0, BlobType::Floor);
        g.set(2, 0, BlobType::Floor);
        g.set(0, 1, BlobType::Floor);
        g.set(0, 2, BlobType::Floor);

        let result = flood_fill_region(&g, 0, 0, 4, BlobType::Floor, BlobType::Wall);
        assert_eq!(result.size, 5);
    }

    #[test]
    fn flood_fill_8_connectivity_reaches_diagonal() {
        // Two floor tiles connected only diagonally
        let mut g = Grid::new(3, 3, BlobType::Wall);
        g.set(0, 0, BlobType::Floor);
        g.set(1, 1, BlobType::Floor);

        let result_4 = flood_fill_region(&g, 0, 0, 4, BlobType::Floor, BlobType::Wall);
        assert_eq!(result_4.size, 1, "4-conn should NOT reach diagonal");

        let result_8 = flood_fill_region(&g, 0, 0, 8, BlobType::Floor, BlobType::Wall);
        assert_eq!(result_8.size, 2, "8-conn should reach diagonal");
    }

    #[test]
    fn flood_fill_wall_start_returns_empty() {
        let g = Grid::new(3, 3, BlobType::Wall);
        let result = flood_fill_region(&g, 1, 1, 4, BlobType::Floor, BlobType::Wall);
        assert_eq!(result.size, 0);
    }

    // --- get_all_regions ---

    #[test]
    fn get_all_regions_two_islands() {
        // 7x3 grid with two disconnected floor patches
        let mut g = Grid::new(7, 3, BlobType::Wall);
        // Island 1: (0,1), (1,1)
        g.set(0, 1, BlobType::Floor);
        g.set(1, 1, BlobType::Floor);
        // Island 2: (5,1), (6,1)
        g.set(5, 1, BlobType::Floor);
        g.set(6, 1, BlobType::Floor);

        let regions = get_all_regions(&g, BlobType::Floor, BlobType::Wall);
        assert_eq!(regions.len(), 2);
        assert!(regions.iter().all(|r| r.size == 2));
    }

    // --- retain_specific_region ---

    #[test]
    fn retain_specific_region_walls_off_others() {
        let mut g = Grid::new(5, 3, BlobType::Wall);
        g.set(0, 1, BlobType::Floor);
        g.set(1, 1, BlobType::Floor);
        g.set(4, 1, BlobType::Floor);

        let regions = get_all_regions(&g, BlobType::Floor, BlobType::Wall);
        let largest = regions.iter().max_by_key(|r| r.size).unwrap();
        retain_specific_region(&mut g, largest, BlobType::Wall);

        // Only the largest region's tiles should be floor
        let floor_count = g.data.iter().filter(|&&v| v == BlobType::Floor).count();
        assert_eq!(floor_count, largest.size);
    }

    // --- create_blob ---

    #[test]
    fn create_blob_produces_connected_output() {
        let grid = Grid::new(20, 20, BlobType::Wall);
        let config = BlobGenConfig {
            round_count: 5,
            min_blob_width: 3,
            min_blob_height: 3,
            max_blob_width: 15,
            max_blob_height: 15,
            initial_alive_percent: 55,
            birth_threshold: 5,
            survival_threshold: 4,
        };

        let (result, min_x, min_y, blob_w, blob_h) =
            create_blob(&grid, &config, BlobType::Floor, BlobType::Wall);

        // Output should be within grid bounds
        assert!(min_x >= 0 && min_y >= 0);
        assert!(min_x + blob_w <= result.width);
        assert!(min_y + blob_h <= result.height);

        // All floor tiles should be in one connected region
        let regions = get_all_regions(&result, BlobType::Floor, BlobType::Wall);
        assert!(regions.len() <= 1, "blob should be a single connected region");
    }
}
