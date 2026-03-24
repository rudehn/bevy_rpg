use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point, RandomNumberGenerator, Rect};
use std::collections::VecDeque;

use crate::{
    assets::ShrineCategoryDef,
    game::shrines::{ShrineData, ShrineEffectInstance, ShrinesPurchased},
    map::{
        builders::{BuilderMap, MetaMapBuilder, ShrineSpawnEntry},
        map::Map,
        tile::{is_passable, is_walkable, LiquidType, TerrainType},
    },
};

use super::choke_map::ChokeMap;
use super::shrine_spawner::{pick_effect, rarity_slots_for_depth};

/// Target number of shrines per floor.
const SHRINE_BUDGET: usize = 3;

/// Minimum gated-interior tile count (inclusive).
const MIN_REGION_SIZE: usize = 3;

/// Maximum gated-interior tile count (inclusive).
const MAX_REGION_SIZE: usize = 25;

/// Minimum Dijkstra distance from player start for a shrine candidate.
const MIN_SECLUSION: f32 = 10.0;

/// A candidate location for a machine (currently shrine-only).
struct MachineCandidate {
    /// Index of the chokepoint tile that gates this region.
    choke_idx: usize,
    /// All tile indices belonging to the gated interior.
    region: Vec<usize>,
    /// Dijkstra distance from the player start to the chokepoint.
    seclusion: f32,
}

pub struct MachinePlacer {
    categories: Vec<ShrineCategoryDef>,
    purchased: Vec<String>,
}

impl MetaMapBuilder for MachinePlacer {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.place_machines(build_data);
    }
}

impl MachinePlacer {
    pub fn new(categories: Vec<ShrineCategoryDef>, purchased: &ShrinesPurchased) -> Box<Self> {
        Box::new(Self {
            categories,
            purchased: purchased.0.clone(),
        })
    }

    fn place_machines(&mut self, build_data: &mut BuilderMap) {
        if self.categories.is_empty() {
            return;
        }

        let starting_pos = build_data
            .require_starting_position("MachinePlacer")
            .clone();
        let start_pt = Point::new(starting_pos.x, starting_pos.y);

        // 1. Generate ChokeMap (expensive -- do it once).
        let choke_map = ChokeMap::generate(&build_data.map);

        // 2. Compute Dijkstra distance map from player start.
        //    Temporarily convert doors/stairs to floors so Dijkstra can traverse them.
        let original_tiles = build_data.map.tiles.clone();
        for tile in build_data.map.tiles.iter_mut() {
            match tile.terrain {
                TerrainType::Door
                | TerrainType::HiddenDoor
                | TerrainType::LockedDoor
                | TerrainType::UpStairs
                | TerrainType::DownStairs => {
                    tile.terrain = TerrainType::Floor;
                }
                _ => {}
            }
        }
        let start_idx = build_data.map.point2d_to_index(start_pt);
        let dijkstra = DijkstraMap::new(
            build_data.map.width() as usize,
            build_data.map.height() as usize,
            &[start_idx],
            &build_data.map,
            3000.0,
        );
        // Restore original tiles.
        build_data.map.tiles = original_tiles;

        // 3. Collect valid shrine candidates.
        let mut candidates: Vec<MachineCandidate> = Vec::new();

        let w = build_data.map.width;
        let h = build_data.map.height;
        let total = (w * h) as usize;

        for idx in 0..total {
            if !choke_map.chokepoints[idx] {
                continue;
            }

            // Quick-reject by choke_value range.
            let cv = choke_map.choke_values[idx] as usize;
            if cv < MIN_REGION_SIZE || cv > MAX_REGION_SIZE {
                continue;
            }

            // Seclusion check.
            let seclusion = dijkstra.map[idx];
            if seclusion == f32::MAX || seclusion < MIN_SECLUSION {
                continue;
            }

            let choke_pt = build_data.map.index_to_point2d(idx);

            // Flood-fill to find the smallest gated interior.
            let region = match find_gated_interior(&build_data.map, choke_pt) {
                Some(r) => r,
                None => continue,
            };

            // Validate actual region size (flood fill may differ from choke_value).
            if region.len() < MIN_REGION_SIZE || region.len() > MAX_REGION_SIZE {
                continue;
            }

            // Check overlap with existing exclusion zones.
            let bbox = region_bounding_rect(&region, &build_data.map);
            if overlaps_exclusion_zones(&bbox, build_data.exclusion_zones()) {
                continue;
            }

            candidates.push(MachineCandidate {
                choke_idx: idx,
                region,
                seclusion,
            });
        }

        // 4. Sort by seclusion descending (most remote first).
        candidates.sort_by(|a, b| b.seclusion.partial_cmp(&a.seclusion).unwrap_or(std::cmp::Ordering::Equal));

        // 5. Place shrines up to budget, skipping overlaps with already-placed regions.
        let mut rng = RandomNumberGenerator::new();
        let depth = build_data.map.depth;
        let mut placed = 0usize;
        let mut used_regions: Vec<Rect> = Vec::new();

        for candidate in &candidates {
            if placed >= SHRINE_BUDGET {
                break;
            }

            // Re-check overlap with regions placed in this pass.
            let bbox = region_bounding_rect(&candidate.region, &build_data.map);
            if used_regions.iter().any(|r| rects_overlap(r, &bbox)) {
                continue;
            }

            // Find the shrine placement tile (centroid or nearest walkable).
            let shrine_pt = match find_shrine_tile(&candidate.region, &build_data.map) {
                Some(pt) => pt,
                None => continue,
            };

            // Roll shrine data (category + effects).
            let cat_idx = rng.range(0, self.categories.len());
            let category = &self.categories[cat_idx];

            let rarity_slots = rarity_slots_for_depth(depth, &mut rng);
            let mut effects: Vec<ShrineEffectInstance> = Vec::new();

            for target_rarity in &rarity_slots {
                if let Some(effect) =
                    pick_effect(category, target_rarity, &self.purchased, &effects, &mut rng)
                {
                    effects.push(effect);
                }
            }

            if effects.is_empty() {
                continue;
            }

            let shrine_data = ShrineData {
                category_id: category.id.clone(),
                category_name: category.name.clone(),
                effects,
            };

            build_data.shrine_spawn_list.push(ShrineSpawnEntry {
                pos: shrine_pt,
                shrine_data,
                category_id: category.id.clone(),
            });

            // Place a door at the chokepoint.
            build_data.map.tiles[candidate.choke_idx].terrain = TerrainType::Door;

            // Place 1-2 candle props inside the region for atmosphere.
            let candle_tiles: Vec<usize> = candidate
                .region
                .iter()
                .copied()
                .filter(|&tidx| {
                    let pt = build_data.map.index_to_point2d(tidx);
                    pt != shrine_pt && is_valid_prop_tile(&build_data.map, tidx)
                })
                .collect();

            let num_candles = candle_tiles.len().min(2);
            for i in 0..num_candles {
                // Spread candles: pick first and last from the filtered list.
                let cidx = if i == 0 {
                    0
                } else {
                    candle_tiles.len() - 1
                };
                let cpt = build_data.map.index_to_point2d(candle_tiles[cidx]);
                build_data.add_prop_spawn(cpt, "candle".to_string());
            }

            // Register exclusion zone and mark region as used.
            build_data.add_exclusion_zone(bbox);
            used_regions.push(bbox);
            placed += 1;

            debug!(
                "MachinePlacer: placed shrine at ({}, {}) with chokepoint at ({}, {}), region size={}, seclusion={:.1}",
                shrine_pt.x, shrine_pt.y,
                build_data.map.index_to_point2d(candidate.choke_idx).x,
                build_data.map.index_to_point2d(candidate.choke_idx).y,
                candidate.region.len(),
                candidate.seclusion,
            );
        }

        debug!(
            "MachinePlacer: placed {placed}/{SHRINE_BUDGET} shrines from {} candidates",
            candidates.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Flood-fills from a start index while treating `block_idx` as impassable.
/// Returns the set of reachable tile indices.
fn flood_fill_region_with_block(map: &Map, start_idx: usize, block_idx: usize) -> Vec<usize> {
    let total = (map.width * map.height) as usize;
    let mut visited = vec![false; total];
    let mut result = Vec::new();
    let mut queue = VecDeque::new();

    if start_idx == block_idx || !is_passable(map.tiles[start_idx]) {
        return result;
    }

    visited[start_idx] = true;
    visited[block_idx] = true; // Treat chokepoint as blocked.
    queue.push_back(start_idx);
    result.push(start_idx);

    while let Some(idx) = queue.pop_front() {
        let (x, y) = map.idx_xy(idx);
        for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if !visited[nidx] && is_passable(map.tiles[nidx]) {
                visited[nidx] = true;
                queue.push_back(nidx);
                result.push(nidx);
            }
        }
    }

    result
}

/// For a chokepoint, find the smallest gated region by flood-filling from
/// each cardinal passable neighbor with the chokepoint blocked.
fn find_gated_interior(map: &Map, choke_pt: Point) -> Option<Vec<usize>> {
    let choke_idx = map.point2d_to_index(choke_pt);
    let mut smallest: Option<Vec<usize>> = None;

    for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
        let nx = choke_pt.x + dx;
        let ny = choke_pt.y + dy;
        if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
            continue;
        }
        let nidx = map.xy_idx(nx, ny);
        if !is_passable(map.tiles[nidx]) {
            continue;
        }

        let region = flood_fill_region_with_block(map, nidx, choke_idx);
        if region.is_empty() {
            continue;
        }

        match &smallest {
            None => smallest = Some(region),
            Some(prev) if region.len() < prev.len() => smallest = Some(region),
            _ => {}
        }
    }

    smallest
}

/// Compute the axis-aligned bounding rectangle of a set of tile indices.
fn region_bounding_rect(region: &[usize], map: &Map) -> Rect {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for &idx in region {
        let (x, y) = map.idx_xy(idx);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    Rect::with_exact(min_x, min_y, max_x, max_y)
}

/// Compute the centroid of a region and return the nearest walkable, non-stair,
/// non-liquid tile in the region as the shrine placement point.
fn find_shrine_tile(region: &[usize], map: &Map) -> Option<Point> {
    if region.is_empty() {
        return None;
    }

    // Compute centroid.
    let (sum_x, sum_y) = region.iter().fold((0i64, 0i64), |(sx, sy), &idx| {
        let (x, y) = map.idx_xy(idx);
        (sx + x as i64, sy + y as i64)
    });
    let n = region.len() as i64;
    let cx = (sum_x / n) as i32;
    let cy = (sum_y / n) as i32;

    // Sort region tiles by distance to centroid, pick first valid one.
    let mut sorted: Vec<usize> = region.to_vec();
    sorted.sort_by_key(|&idx| {
        let (x, y) = map.idx_xy(idx);
        (x - cx).abs() + (y - cy).abs()
    });

    for &idx in &sorted {
        if is_valid_shrine_tile(map, idx) {
            return Some(map.index_to_point2d(idx));
        }
    }

    None
}

/// A tile is valid for shrine placement if it is walkable, has no liquid,
/// and is not stairs.
fn is_valid_shrine_tile(map: &Map, idx: usize) -> bool {
    let tile = map.tiles[idx];
    is_walkable(tile)
        && tile.liquid == LiquidType::None
        && !matches!(
            tile.terrain,
            TerrainType::UpStairs | TerrainType::DownStairs
        )
}

/// A tile is valid for prop (candle) placement if it is walkable and not stairs/liquid.
fn is_valid_prop_tile(map: &Map, idx: usize) -> bool {
    is_valid_shrine_tile(map, idx)
}

/// Check if two bracket_lib Rects overlap.
fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x1 <= b.x2 && a.x2 >= b.x1 && a.y1 <= b.y2 && a.y2 >= b.y1
}

/// Check if a rect overlaps any existing exclusion zone.
fn overlaps_exclusion_zones(rect: &Rect, zones: &[Rect]) -> bool {
    zones.iter().any(|z| rects_overlap(z, rect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, Tile};

    /// Helper: create a small map with a gated nook behind a chokepoint.
    ///
    /// Layout (10x10, 0-indexed):
    ///   Walls everywhere except:
    ///   - Main area: (1,1)-(5,5) floor
    ///   - Chokepoint: (6,3) floor
    ///   - Nook: (7,2), (7,3), (7,4) floor
    fn make_test_map() -> Map {
        let w = 10;
        let h = 10;
        let mut map = Map::new(1, w, h, "test");

        // All walls by default. Carve the main area.
        for y in 1..=5 {
            for x in 1..=5 {
                let idx = map.xy_idx(x, y);
                map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }

        // Chokepoint corridor tile.
        let choke = map.xy_idx(6, 3);
        map.tiles[choke] = Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };

        // Nook behind the chokepoint.
        for y in 2..=4 {
            let idx = map.xy_idx(7, y);
            map.tiles[idx] = Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }

        map
    }

    #[test]
    fn test_flood_fill_with_block() {
        let map = make_test_map();

        let choke_idx = map.xy_idx(6, 3);
        let nook_start = map.xy_idx(7, 3);

        let region = flood_fill_region_with_block(&map, nook_start, choke_idx);
        assert_eq!(region.len(), 3, "Nook should have exactly 3 tiles");

        // All tiles should be in the nook column.
        for &idx in &region {
            let (x, _y) = map.idx_xy(idx);
            assert_eq!(x, 7);
        }
    }

    #[test]
    fn test_find_gated_interior() {
        let map = make_test_map();
        let choke_pt = Point::new(6, 3);

        let interior = find_gated_interior(&map, choke_pt);
        assert!(interior.is_some());
        let region = interior.unwrap();
        // The nook (3 tiles) should be the smallest side.
        assert_eq!(region.len(), 3);
    }

    #[test]
    fn test_region_bounding_rect() {
        let map = make_test_map();
        let region = vec![map.xy_idx(7, 2), map.xy_idx(7, 3), map.xy_idx(7, 4)];
        let bbox = region_bounding_rect(&region, &map);
        assert_eq!(bbox.x1, 7);
        assert_eq!(bbox.x2, 7);
        assert_eq!(bbox.y1, 2);
        assert_eq!(bbox.y2, 4);
    }

    #[test]
    fn test_find_shrine_tile() {
        let map = make_test_map();
        let region = vec![map.xy_idx(7, 2), map.xy_idx(7, 3), map.xy_idx(7, 4)];
        let pt = find_shrine_tile(&region, &map);
        assert!(pt.is_some());
        let p = pt.unwrap();
        assert_eq!(p.x, 7);
        // Centroid y is 3, so (7,3) should be picked.
        assert_eq!(p.y, 3);
    }
}
