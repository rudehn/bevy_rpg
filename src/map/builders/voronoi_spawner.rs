//! Voronoi-cell monster spawner.
//!
//! Replaces the room-based `MonsterSpawner` with a region-based one that
//! works on **any** walkable terrain — cellular-automata forests, room
//! dungeons, or arbitrary hand-stamped maps. The algorithm follows the
//! "Voronoi spawning" pattern from Herbert Wolverson's *Roguelike
//! Tutorial in Rust* (chapter 27), then layers on a few project-specific
//! improvements:
//!
//! - **Pack per region**, not entity per region: a single spawn-table
//!   entry's whole group is dropped into one Voronoi cell, BFS-bounded
//!   to that cell. Yields recognisable "this is the rat warren, that
//!   is the bandit camp" territories instead of sprinkled individuals.
//! - **Player-start exclusion buffer**: any cell tile within
//!   [`START_BUFFER`] (Chebyshev) of the builder's `starting_position`
//!   is removed before sampling — the player never arrives on top of
//!   a fresh pack.
//! - **Per-floor budget**: target `BUDGET_BASE + depth` packs,
//!   distributed across cells weighted by tile count. Bigger cells get
//!   more "host pack" lottery tickets. Predictable difficulty ramp.
//!
//! The pure helpers (`voronoi_regions`, `exclude_around`,
//! `find_pack_cluster`) take plain data, no Bevy — so they can be unit-
//! tested without spinning up an `App`.
//!
//! See [docs/design/SPAWNING.md](../../../docs/design/SPAWNING.md) for
//! the canonical writeup.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use bracket_lib::prelude::{
    CellularDistanceFunction, FastNoise, NoiseType, Point, RandomNumberGenerator,
};

use crate::{
    assets::MonsterSpawnInfo,
    game::squad::SquadConfig,
    map::{
        builders::{BuilderMap, BuilderPhase, MetaMapBuilder, SpawnEntry},
        map::Map,
        tile::{LiquidType, TerrainType, is_walkable},
    },
};

/// Cellular-noise frequency. Higher = smaller cells. 0.08 matches the
/// rust-roguelike-tutorial value and produces ~15–25 usable cells on an
/// 80×60 forest after the dry-walkable cull.
pub const NOISE_FREQUENCY: f32 = 0.08;
/// Voronoi cells smaller than this many tiles are dropped — too cramped
/// to host a pack without making the spawn cluster visibly cramped.
pub const MIN_REGION_TILES: usize = 6;
/// No spawn pack will be placed within this Chebyshev radius of the
/// player's starting tile. Prevents floor-1 "open eyes, eat a rat."
pub const START_BUFFER: i32 = 4;
/// Per-floor spawn-pack budget = `BUDGET_BASE + depth`. Forest 1 → 3,
/// Forest 2 → 4. Tunable here; balance docs in SPAWNING.md.
pub const BUDGET_BASE: usize = 2;

pub struct VoronoiSpawner {
    spawn_table: Vec<MonsterSpawnInfo>,
}

impl MetaMapBuilder for VoronoiSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.spawn(build_data);
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Spawning)
    }
}

impl VoronoiSpawner {
    pub fn new(spawn_table: &[MonsterSpawnInfo]) -> Box<VoronoiSpawner> {
        Box::new(VoronoiSpawner {
            spawn_table: spawn_table.to_vec(),
        })
    }

    fn spawn(&mut self, build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        let possible: Vec<MonsterSpawnInfo> = self
            .spawn_table
            .iter()
            .filter(|s| depth >= s.min_floor && depth <= s.max_floor)
            .cloned()
            .collect();
        if possible.is_empty() {
            info!("VoronoiSpawner: no eligible spawns for depth {depth}");
            return;
        }

        // 1. Build Voronoi cells over dry walkable non-stair tiles.
        let mut regions = voronoi_regions(&build_data.map, NOISE_FREQUENCY);
        regions.retain(|_, tiles| tiles.len() >= MIN_REGION_TILES);

        // 2. Excise the player's arrival clearing.
        if let Some(start) = &build_data.starting_position {
            exclude_around(
                &mut regions,
                &build_data.map,
                start.x,
                start.y,
                START_BUFFER,
            );
            regions.retain(|_, tiles| tiles.len() >= MIN_REGION_TILES);
        }
        if regions.is_empty() {
            warn!("VoronoiSpawner: no usable Voronoi cells on floor {depth}");
            return;
        }

        // 3. Decide budget; never exceed the number of usable cells.
        let budget = (BUDGET_BASE + depth as usize).min(regions.len());

        // 4. Weighted-sample cells by tile count (bigger cell = more
        //    likely to host a pack).
        let region_keys: Vec<i32> = regions.keys().copied().collect();
        let weights: Vec<usize> = region_keys.iter().map(|k| regions[k].len()).collect();
        let chosen = weighted_sample_without_replacement(
            &mut rng,
            &region_keys,
            &weights,
            budget,
        );

        let mut occupied: HashSet<usize> = HashSet::new();
        let mut new_spawns: Vec<SpawnEntry> = Vec::new();

        // Precompute spawn-entry weights once. Entries default to weight
        // 100; rare entries (e.g. Treant) author lower numbers and so are
        // selected proportionally less often. A `total == 0` falls back
        // to uniform selection to keep a misconfigured table playable.
        let entry_weights: Vec<u32> = possible.iter().map(|s| s.weight).collect();
        let total_weight: u32 = entry_weights.iter().sum();

        for region_key in chosen {
            let tiles = &regions[&region_key];
            let entry_idx = if total_weight == 0 {
                rng.range(0, possible.len())
            } else {
                let mut roll = rng.range(0, total_weight as usize);
                let mut picked = 0usize;
                for (i, &w) in entry_weights.iter().enumerate() {
                    let w = w as usize;
                    if roll < w {
                        picked = i;
                        break;
                    }
                    roll -= w;
                }
                picked
            };
            let info = &possible[entry_idx];

            // Pack composition.
            let members = roll_pack_members(info, &mut rng);
            if members.is_empty() {
                continue;
            }

            // Pick an in-cell origin, BFS-bounded to the cell.
            let origin_idx = tiles[rng.range(0, tiles.len())];
            let origin = idx_to_point(&build_data.map, origin_idx);
            let region_set: HashSet<usize> = tiles.iter().copied().collect();
            let points = find_pack_cluster(
                origin,
                members.len(),
                &build_data.map,
                &region_set,
                &occupied,
            );
            if points.is_empty() {
                continue;
            }

            let squad_cfg = SquadConfig {
                flee_threshold: info.flee_threshold,
            };
            if points.len() > 1 {
                let squad_id = build_data.squad_counter.next();
                for (i, (pt, name)) in points.iter().zip(members.iter()).enumerate() {
                    occupied.insert(build_data.map.xy_idx(pt.x, pt.y));
                    new_spawns.push(SpawnEntry::squad(
                        *pt,
                        name.clone(),
                        squad_id,
                        squad_cfg.clone(),
                        i == 0,
                    ));
                }
            } else {
                let pt = points[0];
                let name = members[0].clone();
                occupied.insert(build_data.map.xy_idx(pt.x, pt.y));
                new_spawns.push(SpawnEntry::solo(pt, name));
            }
        }

        for entry in new_spawns {
            build_data.add_monster_spawn(entry);
        }
    }
}

// =====================================================================
// Pure helpers (no Bevy — unit-testable)
// =====================================================================

/// Build Voronoi cells over dry walkable non-stair tiles using the
/// `bracket-noise` Cellular noise (Manhattan distance). Returns a map
/// of `region_id -> tile_indices`. The same `frequency` and the default
/// `FastNoise` seed (1337) produce stable cell boundaries across runs
/// — variability comes from the map layout, not the noise field.
pub fn voronoi_regions(map: &Map, frequency: f32) -> HashMap<i32, Vec<usize>> {
    let mut noise = FastNoise::new();
    noise.set_noise_type(NoiseType::Cellular);
    noise.set_frequency(frequency);
    noise.set_cellular_distance_function(CellularDistanceFunction::Manhattan);

    let mut regions: HashMap<i32, Vec<usize>> = HashMap::new();
    for y in 0..map.height {
        for x in 0..map.width {
            let idx = map.xy_idx(x, y);
            let tile = map.tiles[idx];
            if !is_walkable(tile) {
                continue;
            }
            if tile.liquid != LiquidType::None {
                continue;
            }
            if matches!(
                tile.terrain,
                TerrainType::UpStairs | TerrainType::DownStairs
            ) {
                continue;
            }
            let v = noise.get_noise(x as f32, y as f32) * 10240.0;
            let key = v as i32;
            regions.entry(key).or_default().push(idx);
        }
    }
    regions
}

/// Drop every tile within Chebyshev radius `radius` of `(cx, cy)` from
/// every region. Used to keep the player's starting clearing free of
/// spawns.
pub fn exclude_around(
    regions: &mut HashMap<i32, Vec<usize>>,
    map: &Map,
    cx: i32,
    cy: i32,
    radius: i32,
) {
    for tiles in regions.values_mut() {
        tiles.retain(|&idx| {
            let x = (idx as i32) % map.width;
            let y = (idx as i32) / map.width;
            (x - cx).abs() > radius || (y - cy).abs() > radius
        });
    }
}

/// BFS from `origin` collecting up to `count` walkable, unoccupied tiles
/// that all belong to `region`. Cardinal-only adjacency. The returned
/// points form a tight cluster bounded to the supplied Voronoi cell.
pub fn find_pack_cluster(
    origin: Point,
    count: usize,
    map: &Map,
    region: &HashSet<usize>,
    occupied: &HashSet<usize>,
) -> Vec<Point> {
    let mut result = Vec::new();
    if count == 0 {
        return result;
    }
    let mut visited: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<Point> = VecDeque::new();

    let origin_idx = map.xy_idx(origin.x, origin.y);
    if !region.contains(&origin_idx) {
        return result;
    }
    queue.push_back(origin);
    visited.insert(origin_idx);

    let deltas: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
    while let Some(pt) = queue.pop_front() {
        let idx = map.xy_idx(pt.x, pt.y);
        if region.contains(&idx) && !occupied.contains(&idx) {
            result.push(pt);
            if result.len() >= count {
                break;
            }
        }
        for (dx, dy) in &deltas {
            let nx = pt.x + dx;
            let ny = pt.y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if !visited.contains(&nidx) && region.contains(&nidx) {
                visited.insert(nidx);
                queue.push_back(Point::new(nx, ny));
            }
        }
    }
    result
}

// =====================================================================
// Internal helpers
// =====================================================================

fn idx_to_point(map: &Map, idx: usize) -> Point {
    Point::new((idx as i32) % map.width, (idx as i32) / map.width)
}

fn roll_pack_members(info: &MonsterSpawnInfo, rng: &mut RandomNumberGenerator) -> Vec<String> {
    if !info.group.is_empty() {
        let mut v = Vec::new();
        for gm in &info.group {
            let n_i32 = if gm.max_count > gm.min_count {
                rng.range(gm.min_count, gm.max_count + 1)
            } else {
                gm.min_count
            };
            let n = n_i32.max(0) as usize;
            for _ in 0..n {
                v.push(gm.monster.clone());
            }
        }
        v
    } else {
        let n_i32 = if info.max_group > info.min_group {
            rng.range(info.min_group, info.max_group + 1)
        } else {
            info.min_group
        };
        let n = n_i32.max(1) as usize;
        std::iter::repeat(info.monster.clone()).take(n).collect()
    }
}

/// Sample `take` distinct keys from `keys` weighted by `weights` without
/// replacement. Linear-time per draw; fine for our cell counts (~15–25).
fn weighted_sample_without_replacement(
    rng: &mut RandomNumberGenerator,
    keys: &[i32],
    weights: &[usize],
    take: usize,
) -> Vec<i32> {
    debug_assert_eq!(keys.len(), weights.len());
    let limit = take.min(keys.len());
    let mut remaining_keys: Vec<i32> = keys.to_vec();
    let mut remaining_weights: Vec<usize> = weights.to_vec();
    let mut out = Vec::with_capacity(limit);
    for _ in 0..limit {
        let total: usize = remaining_weights.iter().sum();
        if total == 0 {
            break;
        }
        let mut pick = rng.range(0, total);
        let mut chosen_at = 0;
        for (i, &w) in remaining_weights.iter().enumerate() {
            if pick < w {
                chosen_at = i;
                break;
            }
            pick -= w;
        }
        out.push(remaining_keys.swap_remove(chosen_at));
        remaining_weights.swap_remove(chosen_at);
    }
    out
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, Tile};

    fn floor() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }
    fn wall() -> Tile {
        Tile {
            terrain: TerrainType::Wall,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }
    fn water() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::Water,
            decoration: Decoration::None,
        }
    }
    fn upstairs() -> Tile {
        Tile {
            terrain: TerrainType::UpStairs,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }

    fn make_map(width: i32, height: i32, tiles: Vec<Tile>) -> Map {
        let count = (width * height) as usize;
        assert_eq!(tiles.len(), count);
        Map {
            name: "test".to_string(),
            tiles,
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth: 1,
        }
    }

    #[test]
    fn voronoi_regions_skip_walls_water_and_stairs() {
        // 10×10: walls border, water cross, one upstair, rest floor.
        let mut tiles = vec![floor(); 100];
        for x in 0..10 {
            tiles[x] = wall();
            tiles[90 + x] = wall();
        }
        for y in 0..10 {
            tiles[y * 10] = wall();
            tiles[y * 10 + 9] = wall();
        }
        // Water at column 5.
        for y in 1..9 {
            tiles[y * 10 + 5] = water();
        }
        // Upstairs at (2, 2).
        tiles[2 * 10 + 2] = upstairs();
        let map = make_map(10, 10, tiles);

        let regions = voronoi_regions(&map, NOISE_FREQUENCY);
        for (_, tile_idxs) in &regions {
            for &idx in tile_idxs {
                assert!(
                    is_walkable(map.tiles[idx]),
                    "region tile {idx} is not walkable"
                );
                assert_eq!(
                    map.tiles[idx].liquid,
                    LiquidType::None,
                    "region tile {idx} has liquid"
                );
                assert!(
                    !matches!(
                        map.tiles[idx].terrain,
                        TerrainType::UpStairs | TerrainType::DownStairs
                    ),
                    "region tile {idx} is a stair"
                );
            }
        }
    }

    #[test]
    fn voronoi_regions_partition_walkable_tiles() {
        // 20×20, all floor — every walkable tile should belong to
        // exactly one region.
        let map = make_map(20, 20, vec![floor(); 400]);
        let regions = voronoi_regions(&map, NOISE_FREQUENCY);
        let total_in_regions: usize = regions.values().map(|v| v.len()).sum();
        // Border + interior, all floor here, all walkable. 400 tiles total.
        assert_eq!(
            total_in_regions, 400,
            "all floor tiles must be assigned to some region"
        );
        // Sanity: no tile appears in two regions.
        let mut seen: HashSet<usize> = HashSet::new();
        for tiles in regions.values() {
            for &idx in tiles {
                assert!(seen.insert(idx), "tile {idx} appears in multiple regions");
            }
        }
    }

    #[test]
    fn exclude_around_drops_tiles_within_chebyshev_radius() {
        // 10×10 all floor; build a single region with all tiles, then
        // exclude around (5, 5) with radius 2.
        let map = make_map(10, 10, vec![floor(); 100]);
        let all_tiles: Vec<usize> = (0..100).collect();
        let mut regions: HashMap<i32, Vec<usize>> = HashMap::new();
        regions.insert(42, all_tiles);
        exclude_around(&mut regions, &map, 5, 5, 2);

        let remaining = &regions[&42];
        for &idx in remaining {
            let x = (idx as i32) % map.width;
            let y = (idx as i32) / map.width;
            assert!(
                (x - 5).abs() > 2 || (y - 5).abs() > 2,
                "tile ({x}, {y}) should have been excluded (Chebyshev radius 2 of (5,5))"
            );
        }
        // 5×5 = 25 tiles should have been removed (Chebyshev radius 2
        // around (5,5)).
        assert_eq!(remaining.len(), 100 - 25);
    }

    #[test]
    fn find_pack_cluster_stays_inside_region() {
        // 5×5 all floor. Region = a 2×2 block at top-left: indices 0, 1, 5, 6.
        let map = make_map(5, 5, vec![floor(); 25]);
        let region: HashSet<usize> = [0, 1, 5, 6].into_iter().collect();
        let occupied: HashSet<usize> = HashSet::new();
        let pts = find_pack_cluster(Point::new(0, 0), 6, &map, &region, &occupied);
        // Can return at most 4 — the region has 4 tiles.
        assert_eq!(pts.len(), 4);
        for pt in &pts {
            let idx = map.xy_idx(pt.x, pt.y);
            assert!(region.contains(&idx), "({}, {}) is outside the region", pt.x, pt.y);
        }
    }

    #[test]
    fn find_pack_cluster_respects_occupied_tiles() {
        // Whole map is one region; occupy two tiles, ask for 5.
        let map = make_map(5, 5, vec![floor(); 25]);
        let region: HashSet<usize> = (0..25usize).collect();
        let occupied: HashSet<usize> = [0, 6].into_iter().collect();
        let pts = find_pack_cluster(Point::new(2, 2), 5, &map, &region, &occupied);
        assert_eq!(pts.len(), 5);
        for pt in &pts {
            let idx = map.xy_idx(pt.x, pt.y);
            assert!(
                !occupied.contains(&idx),
                "({}, {}) is occupied — should have been skipped",
                pt.x,
                pt.y
            );
        }
    }

    #[test]
    fn weighted_sample_returns_up_to_take_items_no_duplicates() {
        let mut rng = RandomNumberGenerator::seeded(42);
        let keys = vec![1_i32, 2, 3, 4, 5];
        let weights = vec![10_usize, 1, 1, 1, 1];
        let picked = weighted_sample_without_replacement(&mut rng, &keys, &weights, 3);
        assert_eq!(picked.len(), 3);
        let unique: HashSet<i32> = picked.iter().copied().collect();
        assert_eq!(unique.len(), 3, "sample must be without replacement");
    }

    #[test]
    fn weighted_sample_clamps_at_keys_length() {
        let mut rng = RandomNumberGenerator::seeded(7);
        let keys = vec![1_i32, 2];
        let weights = vec![5_usize, 5];
        let picked = weighted_sample_without_replacement(&mut rng, &keys, &weights, 99);
        assert_eq!(picked.len(), 2);
    }
}
