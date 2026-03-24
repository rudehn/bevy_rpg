use std::collections::VecDeque;

use bevy::log::{info, warn};
use bracket_lib::prelude::{Algorithm2D, DijkstraMap, Point};

use crate::constants::MAX_FLOOR;
use crate::map::{
    builders::{BuilderMap, MetaMapBuilder, SpawnEntry},
    tile::{is_passable, Decoration, LiquidType, TerrainType, Tile},
};

const ARENA_W: i32 = 16;
const ARENA_H: i32 = 13;

pub struct BossRoomBuilder;

impl BossRoomBuilder {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for BossRoomBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        if build_data.map.depth != MAX_FLOOR {
            return; // Only runs on the final floor
        }

        let starting_pos = build_data.require_starting_position("BossRoomBuilder").clone();
        let start = Point::new(starting_pos.x, starting_pos.y);

        // --- Find farthest reachable point via Dijkstra ---

        // Temporarily swap doors/stairs to floors so Dijkstra can traverse them
        let original_tiles = build_data.map.tiles.clone();
        for tile in build_data.map.tiles.iter_mut() {
            match tile.terrain {
                TerrainType::Door | TerrainType::UpStairs | TerrainType::DownStairs => {
                    tile.terrain = TerrainType::Floor;
                }
                _ => {}
            }
        }

        let map_starts = vec![build_data.map.point2d_to_index(start)];
        let dijkstra = DijkstraMap::new(
            build_data.map.width() as usize,
            build_data.map.height() as usize,
            &map_starts,
            &build_data.map,
            3000.0,
        );

        // Restore original tiles
        build_data.map.tiles = original_tiles;

        let mut best_idx = 0;
        let mut best_dist = 0.0f32;
        for (idx, &dist) in dijkstra.map.iter().enumerate() {
            if dist < 3000.0 && dist > best_dist {
                let tile = build_data.map.tiles[idx];
                if tile.terrain == TerrainType::Floor {
                    best_dist = dist;
                    best_idx = idx;
                }
            }
        }

        if best_dist <= 0.0 {
            warn!("BossRoomBuilder: No reachable floor tile found for boss arena");
            fallback_spawn(build_data);
            return;
        }

        let far_pt = build_data.map.index_to_point2d(best_idx);

        // Clamp arena center to fit within map bounds (leave 2-tile border)
        let half_w = ARENA_W / 2;
        let half_h = ARENA_H / 2;
        let cx = far_pt.x.clamp(half_w + 2, build_data.map.width() - half_w - 3);
        let cy = far_pt.y.clamp(half_h + 2, build_data.map.height() - half_h - 3);
        let arena_left = cx - half_w;
        let arena_top = cy - half_h;

        // Snapshot tiles before carving (for revert on connectivity failure)
        let snapshot = build_data.map.tiles.clone();

        // --- Carve the arena ---
        for dy in 0..ARENA_H {
            for dx in 0..ARENA_W {
                let x = arena_left + dx;
                let y = arena_top + dy;
                let idx = build_data.map.xy_idx(x, y);
                if dx == 0 || dx == ARENA_W - 1 || dy == 0 || dy == ARENA_H - 1 {
                    build_data.map.tiles[idx] = Tile {
                        terrain: TerrainType::Wall,
                        liquid: LiquidType::None,
                        decoration: Decoration::None,
                    };
                } else {
                    build_data.map.tiles[idx] = Tile {
                        terrain: TerrainType::Floor,
                        liquid: LiquidType::None,
                        decoration: Decoration::None,
                    };
                }
            }
        }

        // --- Find connection point on arena border adjacent to existing dungeon ---
        let door_pos = find_door_position(build_data, &snapshot, arena_left, arena_top, cx, cy);

        if let Some(dp) = door_pos {
            let didx = build_data.map.xy_idx(dp.x, dp.y);
            build_data.map.tiles[didx].terrain = TerrainType::Door;
        } else {
            // No adjacent floor found — carve a corridor downward from bottom-center
            let door_x = cx;
            let door_y = arena_top + ARENA_H - 1;
            let didx = build_data.map.xy_idx(door_x, door_y);
            build_data.map.tiles[didx].terrain = TerrainType::Door;
            // Carve downward until we hit existing passable terrain
            let mut carve_y = door_y + 1;
            while carve_y < build_data.map.height() - 1 {
                let cidx = build_data.map.xy_idx(door_x, carve_y);
                if is_passable(snapshot[cidx]) {
                    break;
                }
                build_data.map.tiles[cidx].terrain = TerrainType::Floor;
                carve_y += 1;
            }
        }

        // --- Connectivity check: BFS from player start to arena center ---
        let start_idx = build_data.map.point2d_to_index(start);
        let center_idx = build_data.map.xy_idx(cx, cy);

        if !is_connected(build_data, start_idx, center_idx) {
            warn!("BossRoomBuilder: Arena not connected to dungeon, reverting to fallback spawn");
            build_data.map.tiles = snapshot;
            fallback_spawn(build_data);
            return;
        }

        info!(
            "BossRoomBuilder: Boss arena carved at ({}, {}), size {}x{}",
            cx, cy, ARENA_W, ARENA_H
        );

        // --- Spawn the Tyrant at center ---
        let center = Point::new(cx, cy);
        build_data.add_monster_spawn(SpawnEntry::solo(center, "The Veiled Tyrant".to_string()));

        // --- Spawn Wraith guards ---
        for (dx, dy) in [(-3, -2), (3, -2), (0, 3)] {
            let guard_pt = Point::new(cx + dx, cy + dy);
            let gidx = build_data.map.xy_idx(guard_pt.x, guard_pt.y);
            if build_data.map.tiles[gidx].terrain == TerrainType::Floor {
                build_data.add_monster_spawn(SpawnEntry::solo(
                    guard_pt,
                    "Wraith".to_string(),
                ));
            }
        }

        // --- Watchfires at inner corners ---
        for (dx, dy) in [(-5, -4), (5, -4), (-5, 4), (5, 4)] {
            let prop_pt = Point::new(cx + dx, cy + dy);
            if prop_pt.x > arena_left
                && prop_pt.x < arena_left + ARENA_W - 1
                && prop_pt.y > arena_top
                && prop_pt.y < arena_top + ARENA_H - 1
            {
                build_data.add_prop_spawn(prop_pt, "watchfire".to_string());
            }
        }

        // Mark the arena as a decoration exclusion zone
        build_data.add_exclusion_zone(bracket_lib::prelude::Rect::with_size(
            arena_left,
            arena_top,
            ARENA_W,
            ARENA_H,
        ));

        build_data.take_snapshot();
    }
}

/// Scan arena border walls for adjacency to existing dungeon floor tiles (from snapshot).
/// Tries bottom wall first (most natural entrance), then left, right, top.
fn find_door_position(
    build_data: &BuilderMap,
    snapshot: &[Tile],
    arena_left: i32,
    arena_top: i32,
    _cx: i32,
    _cy: i32,
) -> Option<Point> {
    let w = build_data.map.width();
    let h = build_data.map.height();

    // Check bottom wall
    let bottom_y = arena_top + ARENA_H - 1;
    for dx in 1..ARENA_W - 1 {
        let x = arena_left + dx;
        let below_y = bottom_y + 1;
        if below_y < h {
            let below_idx = build_data.map.xy_idx(x, below_y);
            if is_passable(snapshot[below_idx]) {
                return Some(Point::new(x, bottom_y));
            }
        }
    }

    // Check top wall
    let top_y = arena_top;
    for dx in 1..ARENA_W - 1 {
        let x = arena_left + dx;
        let above_y = top_y - 1;
        if above_y >= 0 {
            let above_idx = build_data.map.xy_idx(x, above_y);
            if is_passable(snapshot[above_idx]) {
                return Some(Point::new(x, top_y));
            }
        }
    }

    // Check left wall
    let left_x = arena_left;
    for dy in 1..ARENA_H - 1 {
        let y = arena_top + dy;
        let check_x = left_x - 1;
        if check_x >= 0 {
            let check_idx = build_data.map.xy_idx(check_x, y);
            if is_passable(snapshot[check_idx]) {
                return Some(Point::new(left_x, y));
            }
        }
    }

    // Check right wall
    let right_x = arena_left + ARENA_W - 1;
    for dy in 1..ARENA_H - 1 {
        let y = arena_top + dy;
        let check_x = right_x + 1;
        if check_x < w {
            let check_idx = build_data.map.xy_idx(check_x, y);
            if is_passable(snapshot[check_idx]) {
                return Some(Point::new(right_x, y));
            }
        }
    }

    None
}

/// BFS flood fill from `start_idx` to check if `target_idx` is reachable.
fn is_connected(build_data: &BuilderMap, start_idx: usize, target_idx: usize) -> bool {
    let total = (build_data.map.width() * build_data.map.height()) as usize;
    let mut visited = vec![false; total];
    let mut queue = VecDeque::new();
    visited[start_idx] = true;
    queue.push_back(start_idx);

    while let Some(idx) = queue.pop_front() {
        if idx == target_idx {
            return true;
        }
        let (x, y) = build_data.map.idx_xy(idx);
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= build_data.map.width() || ny >= build_data.map.height() {
                continue;
            }
            let nidx = build_data.map.xy_idx(nx, ny);
            if !visited[nidx] && is_passable(build_data.map.tiles[nidx]) {
                visited[nidx] = true;
                queue.push_back(nidx);
            }
        }
    }

    false
}

/// Fallback: spawn boss at the farthest walkable tile (no arena carving).
fn fallback_spawn(build_data: &mut BuilderMap) {
    warn!("BossRoomBuilder: Using fallback boss spawn (no arena)");
    for idx in (0..build_data.map.tiles.len()).rev() {
        if build_data.map.tiles[idx].terrain == TerrainType::Floor {
            let pt = build_data.map.index_to_point2d(idx);
            build_data
                .add_monster_spawn(SpawnEntry::solo(pt, "The Veiled Tyrant".to_string()));
            return;
        }
    }
}
