use bracket_lib::prelude::{Algorithm2D, Point, RandomNumberGenerator, Rect};

use crate::map::tile::TerrainType;

use super::{BuilderMap, MetaMapBuilder};

pub struct CandleSpawner;

impl MetaMapBuilder for CandleSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl CandleSpawner {
    #[allow(dead_code)]
    pub fn new() -> Box<CandleSpawner> {
        Box::new(CandleSpawner {})
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        let Some(rooms) = build_data.rooms.clone() else {
            return;
        };

        let mut rng = RandomNumberGenerator::new();

        for room in &rooms {
            // Count actual floor tiles to gauge room size (works for all shapes).
            let floor_count = count_floor_tiles(room, build_data);

            if floor_count == 0 {
                continue;
            }

            // One candle per ~20 floor tiles, at least 1, at most 4.
            let candle_count = ((floor_count / 20).max(1)).min(4) as usize;

            // Valid positions: wall tiles adjacent to at least one floor tile.
            let candidates = wall_tiles_adjacent_to_floor(room, build_data);

            if candidates.is_empty() {
                continue;
            }

            // Spread candles as far apart as possible for even coverage.
            let chosen = pick_spread_positions(&candidates, candle_count, &mut rng);
            for pt in chosen {
                build_data.candle_spawn_points.push(pt);
            }
        }
    }
}

/// Counts floor tiles within the room's bounding box on the actual map.
fn count_floor_tiles(room: &Rect, build_data: &BuilderMap) -> i32 {
    let map = &build_data.map;
    let mut count = 0;
    for y in room.y1..=room.y2 {
        for x in room.x1..=room.x2 {
            let pt = Point::new(x, y);
            if map.in_bounds(pt) {
                let idx = map.xy_idx(x, y);
                if map.tiles[idx].terrain == TerrainType::Floor {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Collects all wall tiles within the room's bounding box (plus one-tile border)
/// that are directly adjacent (4-directional) to a floor tile.
/// This correctly handles non-rectangular rooms: circular, cavern, cross, etc.
fn wall_tiles_adjacent_to_floor(room: &Rect, build_data: &BuilderMap) -> Vec<Point> {
    let map = &build_data.map;
    let mut walls = Vec::new();

    // Expand search by 1 to capture walls just outside the room's floor area.
    let x_min = (room.x1 - 1).max(0);
    let x_max = (room.x2 + 1).min(map.width - 1);
    let y_min = (room.y1 - 1).max(0);
    let y_max = (room.y2 + 1).min(map.height - 1);

    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let pt = Point::new(x, y);
            if !map.in_bounds(pt) {
                continue;
            }
            let idx = map.xy_idx(x, y);
            if map.tiles[idx].terrain != TerrainType::Wall {
                continue;
            }

            // Must have at least one floor neighbor (4-directional).
            let adjacent_to_floor = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .any(|&(nx, ny)| {
                    let npt = Point::new(nx, ny);
                    map.in_bounds(npt) && {
                        let nidx = map.xy_idx(nx, ny);
                        map.tiles[nidx].terrain == TerrainType::Floor
                    }
                });

            if adjacent_to_floor {
                walls.push(pt);
            }
        }
    }

    walls
}

/// Picks `count` positions from `candidates` spread as far apart as possible.
/// Starts with a random seed, then greedily picks the point farthest from all chosen points.
fn pick_spread_positions(
    candidates: &[Point],
    count: usize,
    rng: &mut RandomNumberGenerator,
) -> Vec<Point> {
    if candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() <= count {
        return candidates.to_vec();
    }

    let mut remaining: Vec<Point> = candidates.to_vec();
    let mut chosen: Vec<Point> = Vec::with_capacity(count);

    // Seed with a random candidate.
    let seed = rng.range(0, remaining.len() as i32) as usize;
    chosen.push(remaining.remove(seed));

    while chosen.len() < count && !remaining.is_empty() {
        // Pick the candidate whose minimum Manhattan distance to any chosen point is greatest.
        let best_idx = remaining
            .iter()
            .enumerate()
            .max_by_key(|(_, pt)| {
                chosen
                    .iter()
                    .map(|c| (pt.x - c.x).abs() + (pt.y - c.y).abs())
                    .min()
                    .unwrap_or(0)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        chosen.push(remaining.remove(best_idx));
    }

    chosen
}
