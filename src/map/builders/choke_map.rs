use bracket_lib::prelude::{Point, Algorithm2D};
use crate::map::map::Map;
use crate::map::tile::is_passable;
use std::collections::{VecDeque, HashSet};

pub struct ChokeMap {
    pub width: i32,
    pub height: i32,
    pub chokepoints: Vec<bool>,
    pub choke_values: Vec<i32>,
    pub in_loop: Vec<bool>,
}

impl ChokeMap {
    pub fn generate(map: &Map) -> Self {
        let width = map.width;
        let height = map.height;
        let size = (width * height) as usize;
        
        let mut in_loop = vec![false; size];
        let mut chokepoints = vec![false; size];
        let mut choke_values = vec![30000; size];

        // 1. Initial loop marking: all passable tiles are potentially in a loop
        for i in 0..size {
            if is_passable(map.tiles[i]) {
                in_loop[i] = true;
            }
        }

        // 2. Iterative loop pruning (equivalent to checkLoopiness in Architect.c)
        // We remove tiles that are "dead ends" until only loops remain.
        let mut changed = true;
        while changed {
            changed = false;
            for y in 1..height - 1 {
                for x in 1..width - 1 {
                    let idx = map.xy_idx(x, y);
                    if in_loop[idx] {
                        if !Self::is_part_of_loop(map, &in_loop, x, y) {
                            in_loop[idx] = false;
                            changed = true;
                        }
                    }
                }
            }
        }

        // 3. Identify chokepoints
        // A chokepoint is a passable tile NOT in a loop that has more than 2 passable arcs
        // OR is a narrow corridor connection.
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = map.xy_idx(x, y);
                if is_passable(map.tiles[idx]) && !in_loop[idx] {
                    let arcs = Self::passable_arc_count(map, x, y);
                    if arcs > 2 {
                        // In Brogue, it also checks if it's horizontal or vertical squeeze
                        let h_wall = !is_passable(map.tiles[map.xy_idx(x-1, y)]) && !is_passable(map.tiles[map.xy_idx(x+1, y)]);
                        let v_wall = !is_passable(map.tiles[map.xy_idx(x, y-1)]) && !is_passable(map.tiles[map.xy_idx(x, y+1)]);
                        if h_wall || v_wall {
                            chokepoints[idx] = true;
                        }
                    }
                }
            }
        }

        // 4. Calculate choke values (Flood fill from each side of a chokepoint)
        // This is expensive, so we only do it for chokepoints.
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = map.xy_idx(x, y);
                if chokepoints[idx] {
                    // Try to flood fill from neighbors
                    let mut min_region_size = 30000;
                    
                    // Orthogonal neighbors
                    let neighbors = [
                        (x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)
                    ];

                    for (nx, ny) in neighbors {
                        if map.in_bounds(Point::new(nx, ny)) && is_passable(map.tiles[map.xy_idx(nx, ny)]) {
                            // Run flood fill pretending the chokepoint is a wall
                            let region_size = Self::flood_fill_count_with_block(map, Point::new(nx, ny), Point::new(x, y));
                            if region_size < min_region_size {
                                min_region_size = region_size;
                            }
                        }
                    }
                    choke_values[idx] = min_region_size;
                }
            }
        }

        Self {
            width,
            height,
            chokepoints,
            choke_values,
            in_loop,
        }
    }

    fn is_part_of_loop(map: &Map, in_loop: &[bool], x: i32, y: i32) -> bool {
        // A tile is part of a loop if it has at least 2 neighbors that are also in a loop
        // and they are not "adjacent" to each other in a way that forms a dead end.
        // Simplified: Brogue counts "strings" of loopy neighbors.
        let mut num_strings = 0;
        let mut in_string = false;
        
        let neighbors = [
            (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)
        ];

        // Find an unloopy neighbor to start
        let mut start_dir = None;
        for i in 0..8 {
            let (dx, dy) = neighbors[i];
            let nx = x + dx;
            let ny = y + dy;
            if !map.in_bounds(Point::new(nx, ny)) || !in_loop[map.xy_idx(nx, ny)] {
                start_dir = Some(i);
                break;
            }
        }

        let s_dir = match start_dir {
            Some(d) => d,
            None => return true, // All neighbors are loopy
        };

        let mut max_string_len = 0;
        let mut current_string_len = 0;

        for i in 0..8 {
            let dir = (s_dir + i) % 8;
            let (dx, dy) = neighbors[dir];
            let nx = x + dx;
            let ny = y + dy;

            if map.in_bounds(Point::new(nx, ny)) && in_loop[map.xy_idx(nx, ny)] {
                current_string_len += 1;
                if !in_string {
                    num_strings += 1;
                    in_string = true;
                }
            } else {
                if in_string {
                    max_string_len = max_string_len.max(current_string_len);
                    current_string_len = 0;
                    in_string = false;
                }
            }
        }
        max_string_len = max_string_len.max(current_string_len);

        // Brogue logic: num_strings == 1 && max_string_len <= 4 means it's a dead end/not a loop
        !(num_strings == 1 && max_string_len <= 4)
    }

    fn passable_arc_count(map: &Map, x: i32, y: i32) -> i32 {
        let mut arc_count = 0;
        let neighbors = [
            (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)
        ];

        for i in 0..8 {
            let (dx1, dy1) = neighbors[i];
            let (dx2, dy2) = neighbors[(i + 7) % 8];
            
            let n1_passable = map.in_bounds(Point::new(x + dx1, y + dy1)) && is_passable(map.tiles[map.xy_idx(x + dx1, y + dy1)]);
            let n2_passable = map.in_bounds(Point::new(x + dx2, y + dy2)) && is_passable(map.tiles[map.xy_idx(x + dx2, y + dy2)]);
            
            if n1_passable != n2_passable {
                arc_count += 1;
            }
        }
        arc_count / 2
    }

    fn flood_fill_count_with_block(map: &Map, start: Point, block: Point) -> i32 {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut count = 0;

        queue.push_back(start);
        visited.insert(start);

        while let Some(current) = queue.pop_front() {
            count += 1;
            if count > 1000 { break; } // Optimization: Brogue caps at 10000/30000 but we'll cap smaller for now

            for dir in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let next = Point::new(current.x + dir.0, current.y + dir.1);
                if map.in_bounds(next) 
                    && is_passable(map.tiles[map.xy_idx(next.x, next.y)]) 
                    && next != block 
                    && !visited.contains(&next) 
                {
                    visited.insert(next);
                    queue.push_back(next);
                }
            }
        }
        count
    }
}
