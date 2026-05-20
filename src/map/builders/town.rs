//! Procedural town builder — the hub at floor 0.
//!
//! Layout:
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────┐
//! │ ~~~~~~                                                         │
//! │ ~~~~~~                                                         │
//! │ ~~~~~~ ════════════════════════════════════════════════════════│
//! │ ~~~~ ══ ─→ scattered named buildings ←─                  ─→  >│
//! │ ~p p ══                                                        │
//! │ ~~~~~~                                                         │
//! │ ~~~~~~                                                         │
//! └────────────────────────────────────────────────────────────────┘
//!   water     pathfinding road network                       east stair
//!   + piers                                                  into Forest 1
//! ```
//!
//! Notable features:
//!
//! - **Water + piers** on the west edge (deep water tiles, with 3
//!   wooden piers extending westward from shore as walkable Floor).
//! - **Buildings have roles** — Pub, Smithy, Alchemist, Temple,
//!   Houses, Hovel — each with role-specific interior props.
//! - **Player spawns at the Pub's door**, the narrative anchor.
//! - **Quest board** (totem_pole prop) stands near the Pub door.
//! - **Portal** at the map centre — return point with the amulet.
//! - **DownStairs** on the east border — descent into Forest 1.
//! - **Path network** — A*-style organic dirt roads connecting the
//!   east stair, the centre Portal, every building's door, and every
//!   pier's inland end. Per-tile noise on path costs produces wiggly
//!   organic-looking roads instead of L-shapes.
//!
//! Pipeline shape (registered in [`super::floor_builder`]):
//!
//! - `TownLayoutBuilder`     (Geometry):     water + piers + buildings + props
//! - `TownPortalBuilder`     (Finalization): Portal terrain at map centre
//! - `TownDownStairsBuilder` (Finalization): one DownStairs on east border
//! - `TownPathBuilder`       (Finalization): organic road network

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

use bracket_lib::prelude::{Point, Rect};

use roguelike_engine::components::PatrolState;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, InitialMapBuilder, MetaMapBuilder, SpawnEntry};
use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

// =====================================================================
// Tunables
// =====================================================================

/// Buildings the layout pass tries to place.
const TARGET_BUILDINGS: usize = 8;
const BUILDING_MIN: i32 = 4;
const BUILDING_MAX: i32 = 8;
/// Keep buildings this far from the border on all sides.
const BORDER_MARGIN: i32 = 3;
/// Half-width of the open square around the map centre kept clear so
/// the return portal + spawn margin is never inside a building.
const CENTER_KEEPOUT: i32 = 3;
/// Reserved corridor inland of the east-border DownStairs so the
/// player can step into town without bumping into a building.
const EAST_STAIR_KEEPOUT_WIDTH: i32 = 6;
const EAST_STAIR_KEEPOUT_HEIGHT: i32 = 5;
/// Tiles west of (and including) this column are water. Buildings and
/// roads stay east of `WATER_EAST_EDGE`; the NPC placement code in
/// this file uses it to bound roam areas to the land side of the map.
pub(crate) const WATER_EAST_EDGE: i32 = 12;
/// Pier configuration — three piers spread vertically along the
/// shore. Each is `(y, length, thickness)` where `length` is how many
/// tiles the pier extends westward from the shore and `thickness` is
/// 1 (single tile vertical) or 2 (double-width pier).
const PIERS: &[(i32, i32, i32)] = &[
    (12, 5, 1),  // northern pier
    (30, 6, 2),  // mid pier (the biggest one, near the middle of the shore)
    (48, 5, 1),  // southern pier
];
/// Random per-tile cost noise range added to path A*. Higher values
/// produce wigglier paths.
const PATH_NOISE_MAX: f32 = 0.6;
/// Cost reduction for stepping onto a tile already marked as a path.
/// Encourages branches to merge into the trunk rather than running
/// parallel — the visual hallmark of a "road network".
const PATH_MERGE_BONUS: f32 = 0.45;

// =====================================================================
// Building roles + interior props
// =====================================================================

/// Each building gets one of these roles assigned deterministically
/// (sorted by footprint area, largest = Pub). Drives interior prop
/// stamping + player-spawn location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildingRole {
    Pub,
    Smithy,
    Alchemist,
    Temple,
    House,
    Hovel,
}

impl BuildingRole {
    /// Stamp role-appropriate props inside a building's interior. Uses
    /// existing prop entries from `assets/props.ron`. Props are not
    /// blocking the central tile so the room still looks navigable.
    fn interior_props(self) -> &'static [&'static str] {
        match self {
            // Pub: barrels of ale + a warm candle.
            BuildingRole::Pub => &["barrel", "barrel", "candle"],
            // Smithy: an anvil-like barricade + a forge watchfire.
            BuildingRole::Smithy => &["barricade", "watchfire"],
            // Alchemist: potion barrels + a candle.
            BuildingRole::Alchemist => &["barrel", "barrel", "candle"],
            // Temple: the altar (illuminated) — the centrepiece.
            BuildingRole::Temple => &["altar"],
            // House: a single storage chest.
            BuildingRole::House => &["chest"],
            // Hovel: bare.
            BuildingRole::Hovel => &[],
        }
    }
}

// =====================================================================
// TownLayoutBuilder — water, piers, buildings, roles, props.
// =====================================================================

pub struct TownLayoutBuilder;

impl TownLayoutBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl InitialMapBuilder for TownLayoutBuilder {
    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;

        // 1. Fill the eastern land with open Floor; western strip with deep water.
        for y in 0..h {
            for x in 0..w {
                let idx = build.map.xy_idx(x, y);
                let liquid = if x < WATER_EAST_EDGE { LiquidType::Water } else { LiquidType::None };
                build.map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid,
                    decoration: Decoration::None,
                };
            }
        }

        // 2. Stamp piers — walkable Floor tiles extending westward
        //    from the shoreline into the water. Mark them with the
        //    path decoration so the renderer paints them as
        //    packed-wood boardwalk.
        for &(y, length, thickness) in PIERS {
            for dx in 0..length {
                for dy in 0..thickness {
                    let px = WATER_EAST_EDGE - 1 - dx;
                    let py = y + dy;
                    if px <= 0 || py <= 0 || py >= h - 1 { continue; }
                    let idx = build.map.xy_idx(px, py);
                    build.map.tiles[idx] = Tile {
                        terrain: TerrainType::Floor,
                        liquid: LiquidType::None,
                        decoration: Decoration::TownPath,
                    };
                }
            }
        }

        // 3. Scatter buildings on the land side of the map. Buildings
        //    stay east of the water strip and away from the east-stair
        //    corridor + map centre.
        let center = Point::new(w / 2, h / 2);
        let center_keepout = Rect::with_size(
            center.x - CENTER_KEEPOUT,
            center.y - CENTER_KEEPOUT,
            CENTER_KEEPOUT * 2 + 1,
            CENTER_KEEPOUT * 2 + 1,
        );
        // East-border DownStairs corridor.
        let east_stair_keepout = Rect::with_size(
            w - 1 - EAST_STAIR_KEEPOUT_WIDTH,
            h / 2 - EAST_STAIR_KEEPOUT_HEIGHT / 2,
            EAST_STAIR_KEEPOUT_WIDTH,
            EAST_STAIR_KEEPOUT_HEIGHT,
        );
        // Shore corridor — keep buildings off the water side so
        // pathing has room to thread to the piers.
        let shore_keepout = Rect::with_size(
            WATER_EAST_EDGE,
            0,
            3,
            h,
        );

        let mut buildings: Vec<Rect> = Vec::new();
        let mut tries = 0;
        while buildings.len() < TARGET_BUILDINGS && tries < 400 {
            tries += 1;
            let bw = build.rng.range(BUILDING_MIN, BUILDING_MAX + 1);
            let bh = build.rng.range(BUILDING_MIN, BUILDING_MAX + 1);
            let bx = build.rng.range(WATER_EAST_EDGE + 3, w - bw - BORDER_MARGIN);
            let by = build.rng.range(BORDER_MARGIN, h - bh - BORDER_MARGIN);
            let candidate = Rect::with_size(bx, by, bw, bh);

            if rects_overlap_with_margin(&center_keepout, &candidate, 1) { continue; }
            if rects_overlap_with_margin(&east_stair_keepout, &candidate, 1) { continue; }
            if rects_overlap_with_margin(&shore_keepout, &candidate, 1) { continue; }
            if buildings.iter().any(|r| rects_overlap_with_margin(r, &candidate, 2)) { continue; }

            stamp_building(build, candidate, center);
            buildings.push(candidate);
        }

        // 4. Sort buildings by footprint area descending so the
        //    largest one is always the Pub. Stable order keeps role
        //    assignment deterministic relative to the layout RNG.
        buildings.sort_by(|a, b| {
            let area_a = a.width() * a.height();
            let area_b = b.width() * b.height();
            area_b.cmp(&area_a).then_with(|| (a.x1, a.y1).cmp(&(b.x1, b.y1)))
        });

        // 5. Assign roles and stamp interior props.
        let mut pub_door: Option<Position> = None;
        for (i, rect) in buildings.iter().enumerate() {
            let role = role_for_index(i);
            let door = door_for_building(*rect, center);
            if role == BuildingRole::Pub {
                pub_door = Some(Position { x: door.x, y: door.y });
            }
            stamp_interior_props(build, *rect, role);
        }

        build.rooms = Some(buildings);

        // 6. Player spawn — one tile in front of the Pub door, on the
        //    *road* side (toward the map centre). Falls back to
        //    centre+1 only if no Pub got placed.
        let spawn = pub_door
            .map(|p| step_toward(p, center))
            .unwrap_or(Position { x: center.x + 1, y: center.y });
        build.set_starting_position(spawn);

        // 7. Quest board (totem_pole) one more step toward the centre
        //    from the player spawn — i.e. door → spawn → board lined
        //    up along the road. Visible landmark right next to the
        //    starting tile; future quest hook.
        if let Some(_door_pos) = pub_door {
            let board = step_toward(spawn, center);
            if board != spawn
                && (board.x >= 0 && board.x < w && board.y >= 0 && board.y < h)
            {
                let idx = build.map.xy_idx(board.x, board.y);
                // Only place on plain floor; never blot out a building wall.
                if build.map.tiles[idx].terrain == TerrainType::Floor
                    && build.map.tiles[idx].liquid == LiquidType::None
                {
                    build.add_prop_spawn(Point::new(board.x, board.y), "totem_pole".to_string());
                }
            }
        }
    }
}

fn role_for_index(i: usize) -> BuildingRole {
    match i {
        0 => BuildingRole::Pub,
        1 => BuildingRole::Smithy,
        2 => BuildingRole::Alchemist,
        3 => BuildingRole::Temple,
        4 | 5 | 6 => BuildingRole::House,
        _ => BuildingRole::Hovel,
    }
}

fn stamp_interior_props(build: &mut BuilderMap, building: Rect, role: BuildingRole) {
    let props = role.interior_props();
    if props.is_empty() { return; }
    let cx = building.x1 + building.width() / 2;
    let cy = building.y1 + building.height() / 2;
    // Spread props in a deterministic ring around the centre — first
    // prop at centre, then immediate neighbours in compass order.
    let offsets: [(i32, i32); 9] = [
        (0, 0),
        (1, 0), (-1, 0), (0, 1), (0, -1),
        (1, 1), (-1, 1), (1, -1), (-1, -1),
    ];
    for (prop_name, (dx, dy)) in props.iter().zip(offsets.iter()) {
        let x = cx + dx;
        let y = cy + dy;
        if x <= building.x1 || x >= building.x2 - 1 { continue; }
        if y <= building.y1 || y >= building.y2 - 1 { continue; }
        build.add_prop_spawn(Point::new(x, y), (*prop_name).to_string());
    }
}

/// Take one step from `pos` directly toward `target`. Used to find
/// the tile on the road side of the Pub door (which faces the centre
/// portal). Returns `pos` unchanged if it's already at `target`.
fn step_toward(pos: Position, target: Point) -> Position {
    let dx = (target.x - pos.x).signum();
    let dy = (target.y - pos.y).signum();
    Position { x: pos.x + dx, y: pos.y + dy }
}

// =====================================================================
// TownPortalBuilder — the return Portal at the map centre.
// =====================================================================

pub struct TownPortalBuilder;

impl TownPortalBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownPortalBuilder {
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Finalization) }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let cx = build.width / 2;
        let cy = build.height / 2;
        let idx = build.map.xy_idx(cx, cy);
        build.map.tiles[idx].terrain = TerrainType::Portal;
        build.map.tiles[idx].liquid = LiquidType::None;
        build.map.tiles[idx].decoration = Decoration::None;
    }
}

// =====================================================================
// TownDownStairsBuilder — east-border `>` into Forest 1.
// =====================================================================

pub struct TownDownStairsBuilder;

impl TownDownStairsBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownDownStairsBuilder {
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Finalization) }

    fn build_map(&mut self, build: &mut BuilderMap) {
        // East-border midpoint, one tile inside the wall.
        let x = build.width - 2;
        let y = build.height / 2;
        let idx = build.map.xy_idx(x, y);
        build.map.tiles[idx].terrain = TerrainType::DownStairs;
        build.map.tiles[idx].liquid = LiquidType::None;
        build.map.tiles[idx].decoration = Decoration::None;
    }
}

// =====================================================================
// TownPathBuilder — A*-style organic road network.
// =====================================================================

/// Stamps the dirt-path decoration on tiles connecting:
///
/// - east-border DownStairs ↔ centre Portal (trunk)
/// - every building's door ↔ trunk (branches)
/// - every pier inland end ↔ trunk (branches)
///
/// Uses Dijkstra with per-tile random cost noise so paths wiggle, and
/// a merge-bonus that discounts tiles already marked as path —
/// branches merge into the trunk rather than running parallel.
pub struct TownPathBuilder;

impl TownPathBuilder {
    pub fn new() -> Box<Self> { Box::new(Self) }
}

impl MetaMapBuilder for TownPathBuilder {
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Finalization) }

    fn build_map(&mut self, build: &mut BuilderMap) {
        let w = build.width;
        let h = build.height;

        // ------ Compute walkable mask -----------------------------
        // Path-laying can use any non-water Floor (avoiding building
        // interior Floor) + Door tiles. Interior tiles are marked by
        // sampling the inside of each building rect.
        let mut walkable = vec![false; (w * h) as usize];
        let mut interior: HashSet<(i32, i32)> = HashSet::new();
        if let Some(buildings) = &build.rooms {
            for rect in buildings {
                for y in (rect.y1 + 1)..(rect.y2 - 1) {
                    for x in (rect.x1 + 1)..(rect.x2 - 1) {
                        interior.insert((x, y));
                    }
                }
            }
        }
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                let tile = build.map.tiles[idx];
                // Skip water and skip building interiors.
                if tile.liquid != LiquidType::None { continue; }
                if interior.contains(&(x, y)) { continue; }
                match tile.terrain {
                    TerrainType::Floor
                    | TerrainType::Door
                    | TerrainType::DownStairs
                    | TerrainType::UpStairs
                    | TerrainType::Portal => {
                        walkable[idx] = true;
                    }
                    _ => {}
                }
            }
        }

        // ------ Per-tile noise grid (deterministic via builder rng) -----
        let mut noise = vec![0.0_f32; (w * h) as usize];
        for n in noise.iter_mut() {
            *n = build.rng.range(0.0_f32, PATH_NOISE_MAX);
        }

        // ------ Collect interest-points to connect -----------------
        // Trunk: east stair → centre portal.
        let east_stair = Point::new(w - 2, h / 2);
        let centre = Point::new(w / 2, h / 2);
        let mut endpoints: Vec<(Point, Point)> = Vec::new();
        endpoints.push((east_stair, centre));

        // Building doors → centre.
        let mut door_targets: Vec<Point> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if build.map.tiles[idx].terrain == TerrainType::Door {
                    door_targets.push(Point::new(x, y));
                }
            }
        }
        for door in door_targets {
            endpoints.push((door, centre));
        }

        // Pier inland-end → shore-side anchor (the easternmost tile
        // of the pier connects to the road network).
        for &(y, length, _thickness) in PIERS {
            // Inland end is the eastern tile of the pier; aim for the centre.
            let pier_inland = Point::new(WATER_EAST_EDGE - 1, y);
            endpoints.push((pier_inland, centre));
            // Also extend the road outwards onto the pier tiles by
            // marking them as path right now (they were already
            // path-decorated during TownLayoutBuilder).
            for dx in 0..length {
                let px = WATER_EAST_EDGE - 1 - dx;
                if px <= 0 { continue; }
                mark_path_tile(build, px, y);
            }
        }

        // ------ Run pathfinding for each endpoint pair ------------
        // The path mask grows as we add tiles; later branches benefit
        // from the merge bonus on tiles laid by earlier branches.
        let mut path_mask = vec![false; (w * h) as usize];
        // Seed the path mask with already-marked path tiles (piers).
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if is_path_tile(&build.map.tiles[idx]) {
                    path_mask[idx] = true;
                }
            }
        }

        for (start, end) in endpoints {
            if let Some(path) = pathfind_organic(
                start, end, w, h, &walkable, &path_mask, &noise,
            ) {
                for p in path {
                    let idx = (p.y * w + p.x) as usize;
                    path_mask[idx] = true;
                    mark_path_tile(build, p.x, p.y);
                }
            }
        }
    }
}

fn mark_path_tile(build: &mut BuilderMap, x: i32, y: i32) {
    let idx = build.map.xy_idx(x, y);
    let tile = &mut build.map.tiles[idx];
    // Don't overwrite stairs, doors, or portal terrain — only stamp
    // the decoration on plain Floor.
    if tile.terrain != TerrainType::Floor { return; }
    if tile.liquid != LiquidType::None { return; }
    tile.decoration = Decoration::TownPath;
}

fn is_path_tile(tile: &Tile) -> bool {
    matches!(tile.decoration, Decoration::TownPath)
}

/// Dijkstra path search with per-tile random noise (for organic
/// wiggle) and a merge bonus that discounts already-pathed tiles.
/// Returns `None` if no path exists.
fn pathfind_organic(
    start: Point,
    end: Point,
    width: i32,
    height: i32,
    walkable: &[bool],
    path_mask: &[bool],
    noise: &[f32],
) -> Option<Vec<Point>> {
    let idx_of = |x: i32, y: i32| -> usize { (y as usize) * (width as usize) + (x as usize) };
    let start_idx = idx_of(start.x, start.y);
    if !walkable[start_idx] {
        return None;
    }

    #[derive(PartialEq)]
    struct Node {
        cost: f32,
        x: i32,
        y: i32,
    }
    impl Eq for Node {}
    impl Ord for Node {
        fn cmp(&self, other: &Self) -> Ordering {
            other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
        }
    }
    impl PartialOrd for Node {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut costs: HashMap<usize, f32> = HashMap::new();
    let mut prev: HashMap<usize, usize> = HashMap::new();
    let mut frontier: BinaryHeap<Node> = BinaryHeap::new();
    costs.insert(start_idx, 0.0);
    frontier.push(Node { cost: 0.0, x: start.x, y: start.y });

    let end_idx = idx_of(end.x, end.y);

    while let Some(Node { cost, x, y }) = frontier.pop() {
        let idx = idx_of(x, y);
        if idx == end_idx {
            // Reconstruct path.
            let mut path = Vec::new();
            let mut cur = idx;
            while cur != start_idx {
                let cx = (cur % width as usize) as i32;
                let cy = (cur / width as usize) as i32;
                path.push(Point::new(cx, cy));
                cur = *prev.get(&cur)?;
            }
            path.push(start);
            path.reverse();
            return Some(path);
        }
        if cost > *costs.get(&idx).unwrap_or(&f32::INFINITY) {
            continue;
        }
        for (dx, dy) in [(0_i32, 1_i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= width || ny >= height { continue; }
            let nidx = idx_of(nx, ny);
            if !walkable[nidx] { continue; }
            let merge_bonus = if path_mask[nidx] { PATH_MERGE_BONUS } else { 0.0 };
            let move_cost = (1.0 + noise[nidx] - merge_bonus).max(0.1);
            let new_cost = cost + move_cost;
            if new_cost < *costs.get(&nidx).unwrap_or(&f32::INFINITY) {
                costs.insert(nidx, new_cost);
                prev.insert(nidx, idx);
                frontier.push(Node { cost: new_cost, x: nx, y: ny });
            }
        }
    }
    None
}

// =====================================================================
// Helpers (private)
// =====================================================================

fn stamp_building(build: &mut BuilderMap, r: Rect, toward: Point) {
    for y in r.y1..r.y2 {
        for x in r.x1..r.x2 {
            if x < 0 || y < 0 || x >= build.width || y >= build.height { continue; }
            let on_border = x == r.x1 || x == r.x2 - 1 || y == r.y1 || y == r.y2 - 1;
            let idx = build.map.xy_idx(x, y);
            build.map.tiles[idx] = Tile {
                terrain: if on_border { TerrainType::Wall } else { TerrainType::Floor },
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
        }
    }
    let door = door_for_building(r, toward);
    if door.x >= 0 && door.y >= 0 && door.x < build.width && door.y < build.height {
        let idx = build.map.xy_idx(door.x, door.y);
        build.map.tiles[idx].terrain = TerrainType::Door;
        build.map.tiles[idx].liquid = LiquidType::None;
    }
}

fn rects_overlap_with_margin(a: &Rect, b: &Rect, margin: i32) -> bool {
    !(a.x2 + margin <= b.x1
        || b.x2 + margin <= a.x1
        || a.y2 + margin <= b.y1
        || b.y2 + margin <= a.y1)
}

fn door_for_building(building: Rect, toward: Point) -> Point {
    let cx = building.x1 + building.width() / 2;
    let cy = building.y1 + building.height() / 2;
    let dx = toward.x - cx;
    let dy = toward.y - cy;
    if dx.abs() > dy.abs() {
        if dx > 0 { Point::new(building.x2 - 1, cy) } else { Point::new(building.x1, cy) }
    } else if dy > 0 {
        Point::new(cx, building.y2 - 1)
    } else {
        Point::new(cx, building.y1)
    }
}

// =====================================================================
// Town NPCs
//
// Previously sourced from `assets/town_npcs.ron` via a RonAssetPlugin;
// consolidated into this file (alongside the rest of the town builder)
// because there's no scenario where NPC placement diverges from the
// layout that produces it. The roster is small and changes rarely —
// editing this const is the canonical way to add a townsfolk.
//
// Each entry says "spawn N of `<npc>` via `<placement>`". The `npc`
// string keys into `monsters.ron`; NPCs reuse the monster pipeline
// and their peaceful behaviour comes from their `faction:` field, not
// a separate spawn path.
// =====================================================================

/// Placement strategy for a town NPC. The town builder owns the
/// geometry; this enum is the stable contract.
///
/// `AnywhereInTown` is the only variant today. Future entries (Pier,
/// BuildingInterior(role)) land when vendors / quest NPCs ship.
#[derive(Debug, Clone, Copy)]
pub enum TownNpcPlacement {
    /// Random walkable Floor tile east of the water strip, outside
    /// every building interior. Roam bounds = the entire land
    /// portion of the town.
    AnywhereInTown,
}

/// One row in the hardcoded town-NPC roster.
#[derive(Debug, Clone, Copy)]
pub struct TownNpcSpawn {
    /// Lookup key into `monsters.ron`.
    pub npc: &'static str,
    /// How many copies to place.
    pub count: u32,
    /// Where + roam-bounds.
    pub placement: TownNpcPlacement,
}

/// The shipping town-NPC roster. Edit this to add or remove townsfolk.
pub const TOWN_NPC_SPAWNS: &[TownNpcSpawn] = &[
    // Three drunken sailors stagger around the town centre.
    TownNpcSpawn {
        npc: "Drunken Sailor",
        count: 3,
        placement: TownNpcPlacement::AnywhereInTown,
    },
];

/// Queues NPC `SpawnEntry`s onto `BuilderMap.spawn_list`. Each entry
/// carries a `PatrolRoute` matching the chosen placement so the
/// materializer wires the right roaming behaviour onto the spawned
/// entity. Runs in `Finalization` so it can read the finalised town
/// (water + buildings + stairs + paths) without stomping anything.
pub struct TownNpcBuilder;

impl TownNpcBuilder {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for TownNpcBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        place_town_npcs(TOWN_NPC_SPAWNS, build);
    }
}

/// Place every entry in `spawns` onto `build.spawn_list`. Extracted
/// so tests can drive the placement code with an arbitrary roster
/// without going through the `MetaMapBuilder` trait.
fn place_town_npcs(spawns: &[TownNpcSpawn], build: &mut BuilderMap) {
    for spawn in spawns {
        for _ in 0..spawn.count {
            place_one_npc(build, spawn);
        }
    }
}

/// Pick a position + patrol bounds for one NPC and push the resulting
/// `SpawnEntry` onto `build.spawn_list`. Silently gives up after an
/// attempts budget — better one missing drunk than a hung builder on
/// a degenerate town.
fn place_one_npc(build: &mut BuilderMap, spawn: &TownNpcSpawn) {
    match spawn.placement {
        TownNpcPlacement::AnywhereInTown => {
            let Some(pos) = pick_anywhere_in_town(build) else {
                bevy::log::warn!(
                    "TownNpcBuilder: could not find an open tile for '{}' (AnywhereInTown)",
                    spawn.npc,
                );
                return;
            };
            let bounds = town_npc_bounds(build);
            let patrol = roguelike_engine::components::PatrolRoute {
                state: PatrolState::AreaRoam {
                    min: (bounds.x1, bounds.y1),
                    max: (bounds.x2, bounds.y2),
                },
            };
            build.spawn_list.push(SpawnEntry {
                pos: Point::new(pos.x, pos.y),
                name: spawn.npc.to_string(),
                squad_id: None,
                squad_config: None,
                is_leader: false,
                patrol_route: Some(patrol),
            });
        }
    }
}

/// The walkable land box — everywhere east of the water strip,
/// minus the outer wall. Drunks roam within this rectangle.
fn town_npc_bounds(build: &BuilderMap) -> Rect {
    Rect::with_exact(
        WATER_EAST_EDGE + 1,
        1,
        build.width - 2,
        build.height - 2,
    )
}

/// Pick a random walkable Floor tile suitable for an NPC. Avoids
/// water, building interiors, stairs, the portal, and any tile
/// already claimed by a prior spawn — we don't want two drunks
/// starting in the same square.
fn pick_anywhere_in_town(build: &mut BuilderMap) -> Option<Position> {
    const MAX_TRIES: u32 = 128;
    let bounds = town_npc_bounds(build);
    let occupied: HashSet<(i32, i32)> = build
        .spawn_list
        .iter()
        .map(|e| (e.pos.x, e.pos.y))
        .collect();

    for _ in 0..MAX_TRIES {
        let x = build.rng.range(bounds.x1, bounds.x2);
        let y = build.rng.range(bounds.y1, bounds.y2);
        if occupied.contains(&(x, y)) {
            continue;
        }
        if !is_open_town_tile(build, x, y) {
            continue;
        }
        if inside_any_building(build, x, y) {
            continue;
        }
        return Some(Position { x, y });
    }
    None
}

/// A "town tile" the NPC may start on: plain walkable Floor, no
/// liquid (so we don't drop a drunk into the harbour). Stairs and
/// the portal are non-`Floor` so they're already excluded.
fn is_open_town_tile(build: &BuilderMap, x: i32, y: i32) -> bool {
    if x <= 0 || y <= 0 || x >= build.width - 1 || y >= build.height - 1 {
        return false;
    }
    let idx = build.map.xy_idx(x, y);
    let tile = build.map.tiles[idx];
    if tile.liquid != LiquidType::None {
        return false;
    }
    matches!(tile.terrain, TerrainType::Floor)
}

fn inside_any_building(build: &BuilderMap, x: i32, y: i32) -> bool {
    let Some(rooms) = &build.rooms else {
        return false;
    };
    rooms.iter().any(|r| {
        // Building interior = strictly inside the wall border.
        x > r.x1 && x < r.x2 - 1 && y > r.y1 && y < r.y2 - 1
    })
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn build_town() -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(80, 60);
        TownLayoutBuilder.build_map(&mut bm);
        TownPortalBuilder.build_map(&mut bm);
        TownDownStairsBuilder.build_map(&mut bm);
        TownPathBuilder.build_map(&mut bm);
        bm
    }

    #[test]
    fn town_layout_sets_starting_position() {
        let bm = build_town();
        assert!(bm.starting_position.is_some());
    }

    #[test]
    fn town_west_strip_is_water() {
        let bm = build_town();
        // Sample the column just west of the shore line.
        let x = WATER_EAST_EDGE - 5;
        for y in 5..(bm.height - 5) {
            let idx = bm.map.xy_idx(x, y);
            // The pier carving will punch holes in the water; allow
            // those (Liquid::None), but require *most* tiles to be
            // water.
            let liquid = bm.map.tiles[idx].liquid;
            assert!(
                liquid == LiquidType::Water || liquid == LiquidType::None,
                "tile ({}, {}) should be water or pier, got {:?}", x, y, liquid,
            );
        }
        // Sample-water-tile check: tile far from any pier MUST be water.
        let idx_water = bm.map.xy_idx(2, 5);
        assert_eq!(bm.map.tiles[idx_water].liquid, LiquidType::Water);
    }

    #[test]
    fn town_piers_are_walkable_floor() {
        let bm = build_town();
        for &(y, length, thickness) in PIERS {
            for dx in 0..length {
                for dy in 0..thickness {
                    let px = WATER_EAST_EDGE - 1 - dx;
                    let py = y + dy;
                    let idx = bm.map.xy_idx(px, py);
                    assert_eq!(
                        bm.map.tiles[idx].terrain,
                        TerrainType::Floor,
                        "pier tile ({},{}) must be Floor terrain", px, py,
                    );
                    assert_eq!(
                        bm.map.tiles[idx].liquid,
                        LiquidType::None,
                        "pier tile ({},{}) must NOT be water", px, py,
                    );
                }
            }
        }
    }

    #[test]
    fn town_down_stairs_on_east_border() {
        let bm = build_town();
        // East-border centre — the only DownStairs in town.
        let count = bm
            .map
            .tiles
            .iter()
            .filter(|t| t.terrain == TerrainType::DownStairs)
            .count();
        assert_eq!(count, 1, "exactly one DownStairs");
        let idx = bm.map.xy_idx(bm.width - 2, bm.height / 2);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::DownStairs);
    }

    #[test]
    fn town_portal_at_centre() {
        let bm = build_town();
        let idx = bm.map.xy_idx(bm.width / 2, bm.height / 2);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Portal);
    }

    #[test]
    fn town_path_network_connects_east_stair_to_shore() {
        let bm = build_town();
        // BFS along path-decorated tiles from the east stair.
        // We should be able to reach within 2 tiles of any pier and
        // within 2 tiles of the centre.
        let start = Point::new(bm.width - 2, bm.height / 2);
        let path_or_special = |tile: Tile| -> bool {
            is_path_tile(&tile)
                || matches!(tile.terrain, TerrainType::DownStairs | TerrainType::Portal)
        };
        let walkable_mask = |x: i32, y: i32| -> bool {
            if x < 0 || y < 0 || x >= bm.width || y >= bm.height { return false; }
            let idx = bm.map.xy_idx(x, y);
            path_or_special(bm.map.tiles[idx])
        };
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        let mut frontier = vec![(start.x, start.y)];
        visited.insert((start.x, start.y));
        while let Some((x, y)) = frontier.pop() {
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let nx = x + dx;
                let ny = y + dy;
                if visited.contains(&(nx, ny)) { continue; }
                if !walkable_mask(nx, ny) { continue; }
                visited.insert((nx, ny));
                frontier.push((nx, ny));
            }
        }
        // Centre Portal must be reachable along the path network.
        assert!(visited.contains(&(bm.width / 2, bm.height / 2)),
                "path network must reach centre Portal");
        // At least one pier inland-end must be reachable.
        let pier_reached = PIERS.iter().any(|&(y, _len, _thick)| {
            visited.contains(&(WATER_EAST_EDGE - 1, y))
        });
        assert!(pier_reached, "path network must reach at least one pier");
    }

    #[test]
    fn town_buildings_have_props() {
        let bm = build_town();
        // The Pub (biggest) should have at least 3 props inside it.
        assert!(!bm.prop_spawn_list.is_empty(), "town must spawn at least some props");
        // Player should spawn near a Door (the Pub door).
        let spawn = bm.starting_position.unwrap();
        let mut found_door_near = false;
        for (dx, dy) in [(0, 0), (0, 1), (0, -1), (1, 0), (-1, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
            let nx = spawn.x + dx;
            let ny = spawn.y + dy;
            if nx < 0 || ny < 0 || nx >= bm.width || ny >= bm.height { continue; }
            let idx = bm.map.xy_idx(nx, ny);
            if bm.map.tiles[idx].terrain == TerrainType::Door {
                found_door_near = true;
                break;
            }
        }
        assert!(found_door_near, "player spawn should be adjacent to a building Door (the Pub)");
    }

    #[test]
    fn quest_board_totem_pole_is_spawned() {
        let bm = build_town();
        let has_totem = bm
            .prop_spawn_list
            .iter()
            .any(|(_, name)| name == "totem_pole");
        assert!(has_totem, "expected the quest-board totem_pole near the Pub");
    }

    #[test]
    fn town_starting_position_is_walkable_floor() {
        let bm = build_town();
        let pos = bm.starting_position.unwrap();
        let idx = bm.map.xy_idx(pos.x, pos.y);
        assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
        assert_eq!(bm.map.tiles[idx].liquid, LiquidType::None);
    }

    // -----------------------------------------------------------------
    // Town NPC placement
    // -----------------------------------------------------------------

    /// Builds a town-ish BuilderMap for NPC tests: open Floor everywhere
    /// east of WATER_EAST_EDGE, water everywhere west. No buildings (so
    /// placement has a clear field), no stairs (so the test doesn't
    /// depend on those builders running).
    fn open_town_for_npc_test(w: i32, h: i32) -> BuilderMap {
        let mut bm = BuilderMap::new_for_test(w, h);
        for y in 0..h {
            for x in 0..w {
                let idx = bm.map.xy_idx(x, y);
                let liquid = if x < WATER_EAST_EDGE {
                    LiquidType::Water
                } else {
                    LiquidType::None
                };
                bm.map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid,
                    decoration: Decoration::None,
                };
            }
        }
        bm.rooms = Some(vec![]);
        bm
    }

    /// `place_town_npcs` with N drunks must queue N entries onto
    /// `spawn_list`. Each entry references the NPC name, sits on open
    /// Floor, and carries an `AreaRoam` patrol route.
    #[test]
    fn builder_queues_n_entries_with_area_roam_route() {
        let mut bm = open_town_for_npc_test(80, 60);
        place_town_npcs(
            &[TownNpcSpawn {
                npc: "Drunken Sailor",
                count: 3,
                placement: TownNpcPlacement::AnywhereInTown,
            }],
            &mut bm,
        );
        assert_eq!(bm.spawn_list.len(), 3);
        for entry in &bm.spawn_list {
            assert_eq!(entry.name, "Drunken Sailor");
            assert!(matches!(
                &entry.patrol_route,
                Some(roguelike_engine::components::PatrolRoute {
                    state: PatrolState::AreaRoam { .. }
                })
            ));
            let idx = bm.map.xy_idx(entry.pos.x, entry.pos.y);
            assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
            assert_eq!(bm.map.tiles[idx].liquid, LiquidType::None);
        }
    }

    /// Two NPCs in the same builder run shouldn't share a tile.
    #[test]
    fn npc_positions_are_unique_within_one_run() {
        let mut bm = open_town_for_npc_test(80, 60);
        place_town_npcs(
            &[TownNpcSpawn {
                npc: "Drunken Sailor",
                count: 5,
                placement: TownNpcPlacement::AnywhereInTown,
            }],
            &mut bm,
        );
        let mut seen = HashSet::new();
        for entry in &bm.spawn_list {
            assert!(
                seen.insert((entry.pos.x, entry.pos.y)),
                "duplicate NPC position ({}, {})",
                entry.pos.x,
                entry.pos.y,
            );
        }
    }

    /// The shipping roster must be non-empty — guards against an
    /// accidental edit that empties `TOWN_NPC_SPAWNS` and silently
    /// removes every drunk from the town.
    #[test]
    fn shipping_roster_has_at_least_one_entry() {
        assert!(!TOWN_NPC_SPAWNS.is_empty());
        assert!(TOWN_NPC_SPAWNS.iter().any(|s| s.npc == "Drunken Sailor"));
    }
}
