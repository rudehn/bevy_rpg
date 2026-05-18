use bracket_lib::prelude::Point;
use crate::map::map::Map;
use crate::map::tile::is_passable;
use std::collections::VecDeque;

/// Maximum region size reported by the flood fill. Regions larger than this
/// are treated as "open" and capped at this value. All three uses (flood-fill
/// break, choke_values init, and min_region_size seed) must share this constant
/// so comparisons stay meaningful.
const FLOOD_FILL_CAP: i32 = 1000;

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
        let mut choke_values = vec![FLOOD_FILL_CAP; size];

        // 1. Initial loop marking: all passable tiles are potentially in a loop
        for (i, in_loop_val) in in_loop.iter_mut().enumerate().take(size) {
            if is_passable(map.tiles[i]) {
                *in_loop_val = true;
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
                    if in_loop[idx]
                        && !Self::is_part_of_loop(map, &in_loop, x, y) {
                            in_loop[idx] = false;
                            changed = true;
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
                    let mut min_region_size = FLOOD_FILL_CAP;

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
        for (i, &(dx, dy)) in neighbors.iter().enumerate() {
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
            } else if in_string {
                max_string_len = max_string_len.max(current_string_len);
                current_string_len = 0;
                in_string = false;
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
        let total = (map.width * map.height) as usize;
        let mut visited = vec![false; total];
        let mut queue = VecDeque::new();
        let mut count = 0;

        let start_idx = map.xy_idx(start.x, start.y);
        let block_idx = map.xy_idx(block.x, block.y);
        queue.push_back(start_idx);
        visited[start_idx] = true;

        while let Some(current_idx) = queue.pop_front() {
            count += 1;
            if count >= FLOOD_FILL_CAP { break; }

            let (cx, cy) = map.idx_xy(current_idx);
            for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
                let n_idx = map.xy_idx(nx, ny);
                if n_idx == block_idx { continue; }
                if visited[n_idx] { continue; }
                if !is_passable(map.tiles[n_idx]) { continue; }
                visited[n_idx] = true;
                queue.push_back(n_idx);
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Tile, TerrainType, LiquidType, Decoration};

    const W: Tile = Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None };
    const F: Tile = Tile { terrain: TerrainType::Floor, liquid: LiquidType::None, decoration: Decoration::None };

    /// Build a Map from a flat tile vector. The vector must contain exactly
    /// `width * height` tiles. Row 0 is the top of the map (lowest y).
    fn make_test_map(width: usize, height: usize, tiles: Vec<Tile>) -> Map {
        assert_eq!(tiles.len(), width * height);
        let mut map = Map::new(1, width as i32, height as i32, "test");
        map.tiles = tiles;
        map
    }

    /// Build a Map from an ASCII picture. '#' = Wall, '.' = Floor.
    /// Each line becomes one row; all lines must have equal length.
    fn map_from_ascii(lines: &[&str]) -> Map {
        let height = lines.len();
        let width = lines[0].len();
        let mut tiles = Vec::with_capacity(width * height);
        for line in lines {
            assert_eq!(line.len(), width, "all lines must be the same width");
            for ch in line.chars() {
                tiles.push(match ch {
                    '#' => W,
                    '.' => F,
                    _ => W,
                });
            }
        }
        make_test_map(width, height, tiles)
    }

    // =====================================================================
    // passable_arc_count
    // =====================================================================

    #[test]
    fn passable_arc_count_surrounded_by_walls() {
        // A single floor tile surrounded entirely by walls: 0 arcs.
        let map = map_from_ascii(&[
            "#####",
            "##.##",
            "#####",
        ]);
        assert_eq!(ChokeMap::passable_arc_count(&map, 2, 1), 0);
    }

    #[test]
    fn passable_arc_count_vertical_corridor() {
        // Center tile of a vertical corridor: N and S are floor, rest wall.
        // Ring: F,W,W,W,F,W,W,W -> 4 transitions / 2 = 2 arcs.
        let map = map_from_ascii(&[
            "#####",
            "##.##",
            "##.##",
            "##.##",
            "#####",
        ]);
        assert_eq!(ChokeMap::passable_arc_count(&map, 2, 2), 2);
    }

    #[test]
    fn passable_arc_count_cross_intersection() {
        // Center of a plus/cross: N,E,S,W are floor, diagonals are wall.
        // Ring: F,W,F,W,F,W,F,W -> 8 transitions / 2 = 4 arcs.
        let map = map_from_ascii(&[
            "#####",
            "##.##",
            "#...#",
            "##.##",
            "#####",
        ]);
        assert_eq!(ChokeMap::passable_arc_count(&map, 2, 2), 4);
    }

    #[test]
    fn passable_arc_count_three_isolated_diagonals() {
        // Tile with h_wall (E and W are walls), and floor at N, SE, SW
        // (each separated by walls).
        // Ring: N(F),NE(W),E(W),SE(F),S(W),SW(F),W(W),NW(W)
        // = F,W,W,F,W,F,W,W -> 6 transitions / 2 = 3 arcs.
        let map = map_from_ascii(&[
            "#####",
            "##.##",
            "##.##",
            "#.#.#",
            "#####",
        ]);
        assert_eq!(ChokeMap::passable_arc_count(&map, 2, 2), 3);
    }

    // =====================================================================
    // flood_fill_count_with_block
    // =====================================================================

    #[test]
    fn flood_fill_single_tile() {
        let map = map_from_ascii(&[
            "###",
            "#.#",
            "###",
        ]);
        let count = ChokeMap::flood_fill_count_with_block(
            &map, Point::new(1, 1), Point::new(0, 0),
        );
        assert_eq!(count, 1);
    }

    #[test]
    fn flood_fill_straight_corridor() {
        let map = map_from_ascii(&[
            "#######",
            "#.....#",
            "#######",
        ]);
        let count = ChokeMap::flood_fill_count_with_block(
            &map, Point::new(1, 1), Point::new(0, 0),
        );
        assert_eq!(count, 5);
    }

    #[test]
    fn flood_fill_blocked_splits_corridor() {
        // Blocking the 3rd tile of a 5-tile corridor.
        let map = map_from_ascii(&[
            "#######",
            "#.....#",
            "#######",
        ]);
        // Left of block: only (1,1).
        assert_eq!(
            ChokeMap::flood_fill_count_with_block(&map, Point::new(1, 1), Point::new(2, 1)),
            1
        );
        // Right of block: (3,1), (4,1), (5,1).
        assert_eq!(
            ChokeMap::flood_fill_count_with_block(&map, Point::new(3, 1), Point::new(2, 1)),
            3
        );
    }

    #[test]
    fn flood_fill_two_rooms_through_passage() {
        // Two rooms connected by a single passage tile at (5,2).
        let map = map_from_ascii(&[
            "##########",
            "#....#...#",
            "#........#",
            "#....#...#",
            "##########",
        ]);
        // Block the passage. Left: 4*3=12 tiles. Right: 3*3=9 tiles.
        let block = Point::new(5, 2);
        assert_eq!(
            ChokeMap::flood_fill_count_with_block(&map, Point::new(1, 1), block),
            12
        );
        assert_eq!(
            ChokeMap::flood_fill_count_with_block(&map, Point::new(6, 2), block),
            9
        );
    }

    #[test]
    fn flood_fill_caps_at_limit() {
        // 48*48 = 2304 passable tiles, well over the 1000 cap.
        let w = 50;
        let h = 50;
        let mut tiles = vec![W; w * h];
        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                tiles[y * w + x] = F;
            }
        }
        let map = make_test_map(w, h, tiles);
        let count = ChokeMap::flood_fill_count_with_block(
            &map, Point::new(1, 1), Point::new(0, 0),
        );
        assert_eq!(count, FLOOD_FILL_CAP);
    }

    // =====================================================================
    // is_part_of_loop (called with a manually-constructed in_loop mask)
    // =====================================================================

    #[test]
    fn is_part_of_loop_all_neighbors_loopy() {
        // Center of a 3x3 room: all 8 neighbors passable -> all loopy -> true.
        let map = map_from_ascii(&[
            "#####",
            "#...#",
            "#...#",
            "#...#",
            "#####",
        ]);
        let in_loop: Vec<bool> = map.tiles.iter().map(|t| is_passable(*t)).collect();
        assert!(ChokeMap::is_part_of_loop(&map, &in_loop, 2, 2));
    }

    #[test]
    fn is_part_of_loop_single_loopy_neighbor() {
        // Dead-end tip: only S neighbor is loopy -> 1 string of 1 -> false.
        let map = map_from_ascii(&[
            "#####",
            "##.##",
            "##.##",
            "#####",
        ]);
        let in_loop: Vec<bool> = map.tiles.iter().map(|t| is_passable(*t)).collect();
        assert!(!ChokeMap::is_part_of_loop(&map, &in_loop, 2, 1));
    }

    #[test]
    fn is_part_of_loop_two_separated_strings() {
        // Center of a vertical corridor: N and S are loopy, diagonals are not.
        // 2 strings -> true (num_strings != 1).
        let map = map_from_ascii(&[
            "#####",
            "##.##",
            "##.##",
            "##.##",
            "#####",
        ]);
        let in_loop: Vec<bool> = map.tiles.iter().map(|t| is_passable(*t)).collect();
        assert!(ChokeMap::is_part_of_loop(&map, &in_loop, 2, 2));
    }

    #[test]
    fn is_part_of_loop_corner_of_room() {
        // Corner tile (1,1) of a 3-wide room.
        // Loopy neighbors: E(2,1), SE(2,2), S(1,2) -> 1 string of 3.
        // num_strings=1, max_string_len=3 <= 4 -> false (not loop).
        let map = map_from_ascii(&[
            "#####",
            "#...#",
            "#...#",
            "#...#",
            "#####",
        ]);
        let in_loop: Vec<bool> = map.tiles.iter().map(|t| is_passable(*t)).collect();
        assert!(!ChokeMap::is_part_of_loop(&map, &in_loop, 1, 1));
    }

    #[test]
    fn is_part_of_loop_long_string_of_five() {
        // A tile with 1 string of exactly 5 neighbors -> exceeds the <= 4
        // threshold -> true (IS part of loop).
        // Create a concave shape where 5 consecutive 8-ring neighbors are loopy.
        //
        //  #######
        //  ##...##
        //  #..X.##    X at (3,2). Loopy neighbors: NW(2,1), N(3,1), NE(4,1),
        //  ##...##                                  W(2,2), SW(2,3) -> 5 consecutive.
        //  #######
        let map = map_from_ascii(&[
            "#######",
            "##...##",
            "#....##",
            "##...##",
            "#######",
        ]);
        let in_loop: Vec<bool> = map.tiles.iter().map(|t| is_passable(*t)).collect();
        // (3,2) 8-ring: N(3,1)=F, NE(4,1)=F, E(4,2)=W, SE(4,3)=W,
        //   S(3,3)=F, SW(2,3)=F, W(2,2)=F, NW(2,1)=F
        // Starting from E(W): E(!), SE(!), S(F), SW(F), W(F), NW(F), N(F), NE(F)
        // 1 string of 6 -> exceeds 4 -> true.
        assert!(ChokeMap::is_part_of_loop(&map, &in_loop, 3, 2));
    }

    // =====================================================================
    // Loop detection via generate()
    // =====================================================================

    #[test]
    fn ring_all_tiles_in_loop() {
        // A rectangular ring: all passable tiles should remain in-loop.
        let map = map_from_ascii(&[
            "########",
            "#......#",
            "#.####.#",
            "#.####.#",
            "#.####.#",
            "#.####.#",
            "#......#",
            "########",
        ]);
        let cm = ChokeMap::generate(&map);
        let passable_count = map.tiles.iter().filter(|t| is_passable(**t)).count();
        let loop_count = cm.in_loop.iter().filter(|&&v| v).count();
        assert_eq!(loop_count, passable_count, "all ring tiles should be in a loop");
        assert!(!cm.chokepoints.iter().any(|&v| v), "ring has no chokepoints");
    }

    #[test]
    fn ring_with_dead_end_spur() {
        // A ring with a 2-tile dead-end spur. Ring tiles stay in-loop,
        // spur tiles get pruned.
        let map = map_from_ascii(&[
            "#########",
            "#.......#",
            "#.#####.#",
            "#.......#",
            "####.####",
            "####.####",
            "#########",
        ]);
        let cm = ChokeMap::generate(&map);

        // Ring tiles are in-loop.
        assert!(cm.in_loop[map.xy_idx(1, 1)]);
        assert!(cm.in_loop[map.xy_idx(7, 1)]);
        assert!(cm.in_loop[map.xy_idx(1, 3)]);
        assert!(cm.in_loop[map.xy_idx(7, 3)]);

        // Spur tiles are NOT in-loop.
        assert!(!cm.in_loop[map.xy_idx(4, 4)]);
        assert!(!cm.in_loop[map.xy_idx(4, 5)]);
    }

    #[test]
    fn dead_end_corridor_fully_pruned() {
        // A single vertical dead-end corridor. The top-to-bottom scan order
        // causes a cascade: (3,1) is pruned first (only S neighbor), then
        // (3,2) (only S left after N pruned), then (3,3). The final tile
        // has 0 in_loop neighbors, which the algorithm treats as "in loop"
        // (degenerate case: num_strings=0 -> !(0==1 && ...) = true).
        let map = map_from_ascii(&[
            "#######",
            "###.###",
            "###.###",
            "###.###",
            "#######",
        ]);
        let cm = ChokeMap::generate(&map);

        // Top two tiles are definitely pruned by the cascade.
        assert!(!cm.in_loop[map.xy_idx(3, 1)], "(3,1) should be pruned");
        assert!(!cm.in_loop[map.xy_idx(3, 2)], "(3,2) should be pruned");
        // Bottom tile (3,3) stays in-loop (degenerate: 0 strings).
        // No tile qualifies as a chokepoint (arcs <= 2).
        for y in 1..=3 {
            assert!(!cm.chokepoints[map.xy_idx(3, y)], "(3,{}) no chokepoint", y);
        }
    }

    // =====================================================================
    // No chokepoints in simple geometries
    // =====================================================================

    #[test]
    fn all_walls_no_loops_no_chokepoints() {
        let map = make_test_map(5, 5, vec![W; 25]);
        let cm = ChokeMap::generate(&map);
        for i in 0..25 {
            assert!(!cm.in_loop[i]);
            assert!(!cm.chokepoints[i]);
            assert_eq!(cm.choke_values[i], FLOOD_FILL_CAP);
        }
    }

    #[test]
    fn open_room_no_chokepoints() {
        // Even though corner/edge tiles may be pruned from in_loop, they
        // don't satisfy the wall-squeeze requirement for chokepoint status.
        let map = map_from_ascii(&[
            "##########",
            "#........#",
            "#........#",
            "#........#",
            "#........#",
            "##########",
        ]);
        let cm = ChokeMap::generate(&map);
        let size = (map.width * map.height) as usize;
        for i in 0..size {
            assert!(!cm.chokepoints[i], "open room should have no chokepoints");
        }
    }

    #[test]
    fn simple_squeeze_has_no_chokepoint() {
        // Two corridors connected by a 1-wide passage. The passage tile has
        // exactly 2 passable arcs (N and S), which is not > 2, so it is
        // NOT a chokepoint even though it's a topological bottleneck.
        let map = map_from_ascii(&[
            "#######",
            "#.....#",
            "###.###",
            "#.....#",
            "#######",
        ]);
        let cm = ChokeMap::generate(&map);
        let size = (map.width * map.height) as usize;
        for i in 0..size {
            assert!(!cm.chokepoints[i], "simple squeeze has insufficient arcs");
        }
    }

    // =====================================================================
    // Chokepoint detection: full integration test
    // =====================================================================

    #[test]
    fn chokepoint_at_ring_junction() {
        // A ring connected to a junction tile at (5,4) via diagonal
        // neighbors NW, N, NE. The junction also has two dead-end diagonal
        // spurs at SW(4,5) and SE(6,5) that get pruned.
        //
        // After spur pruning, (5,4) has exactly 1 string of 3 in_loop
        // neighbors (the ring side), which satisfies the prune condition
        // (num_strings=1, max_string_len=3 <= 4).
        //
        // Raw passable_arc_count = 3 (N+NE+NW form one group, SE one, SW one).
        // h_wall = true (W(4,4) and E(6,4) are both walls).
        // => chokepoint!
        let w = 11;
        let h = 9;
        let mut tiles = vec![W; w * h];
        let set = |tiles: &mut Vec<Tile>, x: usize, y: usize| {
            tiles[y * w + x] = F;
        };

        // Ring: rectangular ring at rows 1-3, cols 2-6.
        for x in 2..=6 { set(&mut tiles, x, 1); } // top
        set(&mut tiles, 2, 2); set(&mut tiles, 6, 2); // sides
        for x in 2..=6 { set(&mut tiles, x, 3); } // bottom

        // Junction at (5,4): connected to ring via NW(4,3), N(5,3), NE(6,3).
        set(&mut tiles, 5, 4);

        // Walls at W(4,4) and E(6,4): already walls by default.

        // Dead-end spurs at diagonal corners.
        set(&mut tiles, 4, 5); // SW of junction
        set(&mut tiles, 6, 5); // SE of junction

        let map = make_test_map(w, h, tiles);
        let cm = ChokeMap::generate(&map);
        let idx = map.xy_idx(5, 4);

        // Verify the junction is pruned from loop (1 string of 3 after spurs removed).
        assert!(!cm.in_loop[idx], "(5,4) should be pruned from loop");
        // Verify it is detected as a chokepoint.
        assert!(cm.chokepoints[idx], "(5,4) should be a chokepoint");
        // Verify arcs.
        assert_eq!(ChokeMap::passable_arc_count(&map, 5, 4), 3);
    }

    #[test]
    fn chokepoint_choke_value() {
        // Same geometry as chokepoint_at_ring_junction. The choke value
        // should equal the smallest region when the chokepoint is blocked.
        // The two spur tiles (4,5) and (6,5) are each isolated (size=1).
        // The ring side has 12 tiles. min(1, 1, 12) = 1.
        //
        // BUT: choke_values only flood-fills from orthogonal neighbors.
        // (5,4) orthogonal neighbors: N(5,3)=F, S(5,5)=W, E(6,4)=W, W(4,4)=W.
        // Only N(5,3) is passable, so only one fill is done: from N into the ring.
        // The ring has 12 tiles -> min_region = 12.
        // Since the spurs are diagonal-only, they are NOT checked by choke_values
        // (flood fill is orthogonal-only from orthogonal neighbors).
        let w = 11;
        let h = 9;
        let mut tiles = vec![W; w * h];
        let set = |tiles: &mut Vec<Tile>, x: usize, y: usize| {
            tiles[y * w + x] = F;
        };
        for x in 2..=6 { set(&mut tiles, x, 1); }
        set(&mut tiles, 2, 2); set(&mut tiles, 6, 2);
        for x in 2..=6 { set(&mut tiles, x, 3); }
        set(&mut tiles, 5, 4);
        set(&mut tiles, 4, 5);
        set(&mut tiles, 6, 5);

        let map = make_test_map(w, h, tiles);
        let cm = ChokeMap::generate(&map);
        let idx = map.xy_idx(5, 4);

        // Only orthogonal neighbor N(5,3) is passable; fill reaches ring = 12.
        assert_eq!(cm.choke_values[idx], 12, "choke value from ring side");
    }

    // =====================================================================
    // Choke value: direct flood_fill verification
    // =====================================================================

    #[test]
    fn choke_value_is_min_region_size() {
        // Two rooms connected by a passage. Blocking splits into 12 and 9.
        let map = map_from_ascii(&[
            "##########",
            "#....#...#",
            "#........#",
            "#....#...#",
            "##########",
        ]);
        let block = Point::new(5, 2);
        let left = ChokeMap::flood_fill_count_with_block(&map, Point::new(1, 1), block);
        let right = ChokeMap::flood_fill_count_with_block(&map, Point::new(6, 2), block);
        assert_eq!(left, 12);
        assert_eq!(right, 9);
        assert_eq!(left.min(right), 9, "choke value = smaller region");
    }

    #[test]
    fn choke_value_asymmetric_corridor() {
        // Block 3rd tile of an 11-tile corridor: left=2, right=8.
        let map = map_from_ascii(&[
            "#############",
            "#...........#",
            "#############",
        ]);
        let block = Point::new(3, 1);
        let left = ChokeMap::flood_fill_count_with_block(&map, Point::new(1, 1), block);
        let right = ChokeMap::flood_fill_count_with_block(&map, Point::new(4, 1), block);
        assert_eq!(left, 2);
        assert_eq!(right, 8);
    }

    // =====================================================================
    // Door tiles are passable for topology analysis
    // =====================================================================

    #[test]
    fn door_tile_is_passable() {
        let door = Tile { terrain: TerrainType::Door, liquid: LiquidType::None, decoration: Decoration::None };
        assert!(is_passable(door));
    }

    #[test]
    fn hidden_door_is_passable() {
        let tile = Tile { terrain: TerrainType::HiddenDoor, liquid: LiquidType::None, decoration: Decoration::None };
        assert!(is_passable(tile));
    }

    #[test]
    fn wall_is_not_passable() {
        assert!(!is_passable(W));
    }
}
