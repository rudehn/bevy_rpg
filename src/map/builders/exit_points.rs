//! Exit placement — game-side wrapper over the engine's DistantExit.
//!
//! - Floor 1: delegates to the engine (DownStairs at the farthest tile),
//!   then stamps an Escape Portal at the player's starting position. The
//!   portal only opens once the player is carrying the Amulet of Yendor.
//! - Floors 2..MAX_FLOOR-1: delegates entirely to the engine builder
//!   (DownStairs at the farthest reachable tile).
//! - Final floor (MAX_FLOOR): places the Amulet of Yendor only — no
//!   DownStairs, no Portal. The player must climb back up to floor 1.

use bevy::log::{info, warn};
use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::constants::MAX_FLOOR;
use crate::map::{
    builders::{BuilderMap, BuilderPhase, MetaMapBuilder},
    tile::{LiquidType, TerrainType},
};

// Re-export the engine's pure builder so callers that just want
// DownStairs placement can use it directly.
pub use roguelike_engine::map::builders::exit_points::DistantExit as EngineDistantExit;

/// Game-aware exit builder. On non-final floors, delegates to the
/// engine's `DistantExit`. On the final floor, places the Amulet of
/// Yendor and an Escape Portal instead.
#[derive(Clone)]
pub struct DistantExit;

impl DistantExit {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for DistantExit {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        if build_data.map.depth as u32 >= MAX_FLOOR {
            self.final_floor(build_data);
        } else {
            // Delegate normal floors to the engine builder (places DownStairs).
            use roguelike_engine::map::builders::MapBuilder;
            let mut engine_exit = EngineDistantExit::new();
            engine_exit.build(build_data);

            // Floor 1 also hosts the Escape Portal at the player's start
            // position — the way the player entered the dungeon and the
            // only way out once the Amulet of Yendor is in hand.
            if build_data.map.depth == 1 {
                self.place_escape_portal(build_data);
            }
        }
    }

    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }
}

impl DistantExit {
    fn final_floor(&self, build_data: &mut BuilderMap) {
        let Some(starting_pos) = build_data.starting_position.clone() else {
            warn!("DistantExit: starting position not set — skipping");
            return;
        };

        let start_idx = build_data
            .map
            .point2d_to_index(Point::new(starting_pos.x, starting_pos.y));

        let amulet_pos = find_farthest_tile(&mut build_data.map, start_idx);
        let Some(amulet_pos) = amulet_pos else {
            warn!("DistantExit: no floor tile found for amulet on final floor!");
            return;
        };

        info!(
            "DistantExit: final floor — placing Amulet of Yendor at ({}, {})",
            amulet_pos.x, amulet_pos.y
        );
        build_data.add_item_spawn(amulet_pos, "Amulet of Yendor".to_string(), 1);

        // Deterministically place the Amulet Guardian adjacent to the
        // amulet so floor 26 always has a set-piece encounter rather
        // than leaving it to the spawn-table RNG. The Guardian stands
        // one tile offset on the first walkable cardinal neighbor.
        if let Some(guardian_pos) = nearest_walkable_neighbor(&build_data.map, amulet_pos) {
            info!(
                "DistantExit: placing Amulet Guardian at ({}, {}) guarding the amulet",
                guardian_pos.x, guardian_pos.y
            );
            build_data.add_monster_spawn(crate::map::builders::SpawnEntry::solo(
                guardian_pos,
                "Amulet Guardian".to_string(),
            ));
        } else {
            warn!("DistantExit: no walkable neighbor beside amulet for Amulet Guardian");
        }

        build_data.take_snapshot();
    }

    fn place_escape_portal(&self, build_data: &mut BuilderMap) {
        let Some(starting_pos) = build_data.starting_position.clone() else {
            warn!("DistantExit: starting position not set — cannot place portal");
            return;
        };
        let portal_pos = Point::new(starting_pos.x, starting_pos.y);
        info!(
            "DistantExit: placing Escape Portal at player start ({}, {})",
            portal_pos.x, portal_pos.y
        );
        build_data.map.set_tile(portal_pos, TerrainType::Portal);
        build_data.map.set_liquid(portal_pos, LiquidType::None);
        build_data.take_snapshot();
    }
}

/// Return the first walkable cardinal neighbor of `origin`, or `None` if
/// every neighbor is blocked. Used to place the Amulet Guardian next to
/// the amulet without overwriting the amulet's tile.
fn nearest_walkable_neighbor(map: &crate::map::Map, origin: Point) -> Option<Point> {
    use crate::map::tile::is_walkable;
    for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
        let nx = origin.x + dx;
        let ny = origin.y + dy;
        if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
            continue;
        }
        let idx = map.xy_idx(nx, ny);
        if is_walkable(map.tiles[idx]) {
            return Some(Point::new(nx, ny));
        }
    }
    None
}

/// Find the farthest reachable dry floor tile from `start_idx` using
/// Dijkstra distance. Returns `None` if no tile is reachable.
fn find_farthest_tile(
    map: &mut crate::map::Map,
    start_idx: usize,
) -> Option<Point> {
    let starts = vec![start_idx];

    // Temporarily flatten doors/stairs for traversal
    let original_tiles = map.tiles.clone();
    for tile in map.tiles.iter_mut() {
        match tile.terrain {
            TerrainType::Door | TerrainType::UpStairs | TerrainType::DownStairs => {
                tile.terrain = TerrainType::Floor;
            }
            _ => {}
        }
    }

    let dijkstra = DijkstraMap::new(
        map.width() as usize,
        map.height() as usize,
        &starts,
        map,
        3000.0,
    );

    map.tiles = original_tiles;

    let mut best: Option<(usize, f32)> = None;
    for y in 0..map.height() {
        for x in 0..map.width() {
            let pt = Point::new(x, y);
            let idx = map.point2d_to_index(pt);
            let tile = map.get_tile(pt);
            if tile.map(|t| t.terrain) == Some(TerrainType::Floor)
                && tile.map(|t| t.liquid) == Some(LiquidType::None)
            {
                let dist = dijkstra.map[idx];
                if dist != f32::MAX && dist > 0.0 {
                    let better = match best {
                        None => true,
                        Some((_, d)) => dist > d,
                    };
                    if better {
                        best = Some((idx, dist));
                    }
                }
            }
        }
    }

    best.map(|(idx, _)| map.index_to_point2d(idx))
}
