use bracket_lib::prelude::{Algorithm2D, Point, RandomNumberGenerator, Rect};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    assets::{MonsterAsset, MonsterSpawnInfo, PrefabTemplate},
    game::squad::SquadConfig,
    map::tile::TerrainType,
};

use super::{BuilderMap, MetaMapBuilder, SpawnEntry};

// ---------------------------------------------------------------------------
// MonsterRoleTable — faction-first role resolution for prefab spawns
// ---------------------------------------------------------------------------

pub struct MonsterRoleEntry {
    pub name: String,
    pub faction_tag: String,
    pub role: String,
    pub min_floor: i32,
    pub max_floor: i32,
}

pub struct MonsterRoleTable {
    entries: Vec<MonsterRoleEntry>,
}

impl MonsterRoleTable {
    /// Build from the monster manifest and spawn table. Each spawn table entry
    /// contributes a floor range; the faction/role come from the monster asset.
    pub fn from_manifest(
        monsters: &HashMap<String, MonsterAsset>,
        spawn_table: &[MonsterSpawnInfo],
    ) -> Self {
        let mut entries = Vec::new();
        for spawn in spawn_table {
            // Skip mixed groups — they don't map cleanly to a single monster.
            if !spawn.group.is_empty() {
                continue;
            }
            if let Some(asset) = monsters.get(&spawn.monster) {
                if !asset.faction_tag.is_empty() && !asset.role.is_empty() {
                    entries.push(MonsterRoleEntry {
                        name: asset.name.clone(),
                        faction_tag: asset.faction_tag.clone(),
                        role: asset.role.clone(),
                        min_floor: spawn.min_floor,
                        max_floor: spawn.max_floor,
                    });
                }
            }
        }
        Self { entries }
    }

    /// Get all factions that have at least one monster for every required role at this depth.
    pub fn eligible_factions(&self, roles: &[&str], depth: i32) -> Vec<String> {
        // Unique roles needed.
        let needed: HashSet<&str> = roles.iter().copied().collect();

        // Build faction → set of available roles at this depth.
        let mut faction_roles: HashMap<&str, HashSet<&str>> = HashMap::new();
        for entry in &self.entries {
            if depth >= entry.min_floor && depth <= entry.max_floor {
                faction_roles
                    .entry(entry.faction_tag.as_str())
                    .or_default()
                    .insert(entry.role.as_str());
            }
        }

        faction_roles
            .into_iter()
            .filter(|(_, available)| needed.iter().all(|r| available.contains(r)))
            .map(|(faction, _)| faction.to_string())
            .collect()
    }

    /// Resolve a role to a random monster name within the given faction at this depth.
    pub fn resolve_role(
        &self,
        faction: &str,
        role: &str,
        depth: i32,
        rng: &mut RandomNumberGenerator,
    ) -> Option<String> {
        let candidates: Vec<&MonsterRoleEntry> = self
            .entries
            .iter()
            .filter(|e| {
                e.faction_tag == faction
                    && e.role == role
                    && depth >= e.min_floor
                    && depth <= e.max_floor
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }
        let idx = rng.range(0, candidates.len() as i32) as usize;
        Some(candidates[idx].name.clone())
    }
}

/// Total tile budget for prefabs per floor. Each placed prefab consumes
/// width × height from this budget.
const BASE_PREFAB_BUDGET: i32 = 200;

/// Minimum area for a prefab to be considered "medium" size.
const MEDIUM_THRESHOLD: i32 = 31;

/// Padding between placed prefabs to prevent them from touching.
const PREFAB_PADDING: i32 = 2;

/// Returns true if a rect at (x, y, w, h) overlaps any already-placed region
/// (with padding).
fn overlaps_placed(occupied: &[Rect], x: i32, y: i32, w: i32, h: i32) -> bool {
    let new_rect = Rect::with_size(
        x - PREFAB_PADDING,
        y - PREFAB_PADDING,
        w + PREFAB_PADDING * 2,
        h + PREFAB_PADDING * 2,
    );
    occupied.iter().any(|r| {
        // bracket_lib Rect intersection check
        !(new_rect.x2 < r.x1 || new_rect.x1 > r.x2 || new_rect.y2 < r.y1 || new_rect.y1 > r.y2)
    })
}

// ---------------------------------------------------------------------------
// Orientation transforms
// ---------------------------------------------------------------------------

/// Generate all unique orientations (rotations + optional horizontal flip)
/// of a prefab template. Returns cloned templates with transformed geometry.
fn generate_orientations(prefab: &PrefabTemplate) -> Vec<PrefabTemplate> {
    let mut orientations = Vec::with_capacity(8);
    let mut seen_tiles: HashSet<Vec<String>> = HashSet::new();

    // Identity is always included.
    seen_tiles.insert(prefab.tiles.clone());
    orientations.push(prefab.clone());

    // Build candidate transforms.
    let mut candidates: Vec<PrefabTemplate> = Vec::new();

    if prefab.allow_rotate {
        candidates.push(rotate_prefab(prefab, 1)); // 90° CW
        candidates.push(rotate_prefab(prefab, 2)); // 180°
        candidates.push(rotate_prefab(prefab, 3)); // 270° CW
    }

    if prefab.allow_flip {
        let flipped = flip_prefab_h(prefab);
        candidates.push(flipped.clone());
        if prefab.allow_rotate {
            candidates.push(rotate_prefab(&flipped, 1));
            candidates.push(rotate_prefab(&flipped, 2));
            candidates.push(rotate_prefab(&flipped, 3));
        }
    }

    for c in candidates {
        if seen_tiles.insert(c.tiles.clone()) {
            orientations.push(c);
        }
    }

    orientations
}

/// Rotate a prefab 90° clockwise `times` times (1=90°, 2=180°, 3=270°).
fn rotate_prefab(prefab: &PrefabTemplate, times: u32) -> PrefabTemplate {
    let mut p = prefab.clone();
    for _ in 0..times {
        p = rotate_90_cw(&p);
    }
    p
}

/// Single 90° clockwise rotation.
/// Original (W×H) → New (H×W).
/// new_x = H-1-old_y, new_y = old_x
fn rotate_90_cw(prefab: &PrefabTemplate) -> PrefabTemplate {
    let w = prefab.width;
    let h = prefab.height;

    // Build rotated tile grid: new dimensions are (h, w).
    // New grid has `w` rows of length `h`.
    let mut new_tiles: Vec<Vec<char>> = vec![vec![' '; h as usize]; w as usize];
    for (old_y, row) in prefab.tiles.iter().enumerate() {
        for (old_x, ch) in row.chars().enumerate() {
            let new_x = (h - 1 - old_y as i32) as usize;
            let new_y = old_x;
            new_tiles[new_y][new_x] = ch;
        }
    }

    let tiles: Vec<String> = new_tiles.iter().map(|row| row.iter().collect()).collect();

    let transform = |x: i32, y: i32| -> (i32, i32) {
        (h - 1 - y, x)
    };

    let props = prefab.props.iter().map(|p| {
        let (nx, ny) = transform(p.x, p.y);
        crate::assets::PrefabPropEntry { x: nx, y: ny, prop: p.prop.clone() }
    }).collect();

    let monster_spawns = prefab.monster_spawns.iter().map(|m| {
        let (nx, ny) = transform(m.x, m.y);
        let behavior = transform_behavior(&m.behavior, &transform);
        crate::assets::PrefabMonsterSpawn { x: nx, y: ny, role: m.role.clone(), behavior }
    }).collect();

    let item_spawns = prefab.item_spawns.iter().map(|i| {
        let (nx, ny) = transform(i.x, i.y);
        crate::assets::PrefabItemSpawn { x: nx, y: ny, item: i.item.clone() }
    }).collect();

    PrefabTemplate {
        name: prefab.name.clone(),
        width: h,
        height: w,
        min_floor: prefab.min_floor,
        max_floor: prefab.max_floor,
        tiles,
        props,
        monster_spawns,
        item_spawns,
        on_leader_death: prefab.on_leader_death.clone(),
        flee_threshold: prefab.flee_threshold,
        placement: prefab.placement.clone(),
        allow_rotate: prefab.allow_rotate,
        allow_flip: prefab.allow_flip,
    }
}

/// Horizontal flip: new_x = W-1-old_x, new_y = old_y.
fn flip_prefab_h(prefab: &PrefabTemplate) -> PrefabTemplate {
    let w = prefab.width;

    let tiles: Vec<String> = prefab.tiles.iter()
        .map(|row| row.chars().rev().collect())
        .collect();

    let transform = |x: i32, y: i32| -> (i32, i32) {
        (w - 1 - x, y)
    };

    let props = prefab.props.iter().map(|p| {
        let (nx, ny) = transform(p.x, p.y);
        crate::assets::PrefabPropEntry { x: nx, y: ny, prop: p.prop.clone() }
    }).collect();

    let monster_spawns = prefab.monster_spawns.iter().map(|m| {
        let (nx, ny) = transform(m.x, m.y);
        let behavior = transform_behavior(&m.behavior, &transform);
        crate::assets::PrefabMonsterSpawn { x: nx, y: ny, role: m.role.clone(), behavior }
    }).collect();

    let item_spawns = prefab.item_spawns.iter().map(|i| {
        let (nx, ny) = transform(i.x, i.y);
        crate::assets::PrefabItemSpawn { x: nx, y: ny, item: i.item.clone() }
    }).collect();

    PrefabTemplate {
        name: prefab.name.clone(),
        width: w,
        height: prefab.height,
        min_floor: prefab.min_floor,
        max_floor: prefab.max_floor,
        tiles,
        props,
        monster_spawns,
        item_spawns,
        on_leader_death: prefab.on_leader_death.clone(),
        flee_threshold: prefab.flee_threshold,
        placement: prefab.placement.clone(),
        allow_rotate: prefab.allow_rotate,
        allow_flip: prefab.allow_flip,
    }
}

/// Transform behavior coordinates through the same rotation/flip applied to tiles.
fn transform_behavior(
    behavior: &crate::assets::MonsterBehavior,
    transform: impl Fn(i32, i32) -> (i32, i32),
) -> crate::assets::MonsterBehavior {
    use crate::assets::MonsterBehavior;
    match behavior {
        MonsterBehavior::Sentry | MonsterBehavior::Wander => behavior.clone(),
        MonsterBehavior::Patrol(points) => {
            MonsterBehavior::Patrol(points.iter().map(|(x, y)| transform(*x, *y)).collect())
        }
        MonsterBehavior::Roam { min, max } => {
            let (x1, y1) = transform(min.0, min.1);
            let (x2, y2) = transform(max.0, max.1);
            MonsterBehavior::Roam {
                min: (x1.min(x2), y1.min(y2)),
                max: (x1.max(x2), y1.max(y2)),
            }
        }
    }
}

pub struct PrefabPlacer {
    prefabs: Vec<PrefabTemplate>,
    role_table: MonsterRoleTable,
}

impl MetaMapBuilder for PrefabPlacer {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.place_prefabs(build_data);
    }
}

impl PrefabPlacer {
    pub fn new(prefabs: Vec<PrefabTemplate>, role_table: MonsterRoleTable) -> Box<Self> {
        Box::new(Self { prefabs, role_table })
    }

    fn place_prefabs(&mut self, build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        let mut budget = BASE_PREFAB_BUDGET;
        let mut occupied_regions: Vec<Rect> = Vec::new();
        let mut placed_names: HashSet<String> = HashSet::new();

        // Filter prefabs eligible for this floor depth.
        let eligible: Vec<PrefabTemplate> = self
            .prefabs
            .iter()
            .filter(|p| depth >= p.min_floor && depth <= p.max_floor)
            .cloned()
            .collect();

        if eligible.is_empty() {
            return;
        }

        // Pass 1: medium + large prefabs (tactical landmarks). Each gets one attempt.
        let mut big_prefabs: Vec<&PrefabTemplate> = eligible.iter()
            .filter(|p| p.width * p.height >= MEDIUM_THRESHOLD)
            .collect();
        // Shuffle.
        let n = big_prefabs.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            big_prefabs.swap(i as usize, j as usize);
        }

        for prefab in &big_prefabs {
            if budget <= 0 { break; }
            if placed_names.contains(&prefab.name) { continue; }

            if self.try_place_oriented(build_data, prefab, &mut rng, &mut occupied_regions) {
                let area = prefab.width * prefab.height;
                budget -= area;
                placed_names.insert(prefab.name.clone());
            }
        }

        // Pass 2: small prefabs — fill remaining budget.
        let mut consecutive_failures = 0;
        while budget > 0 && consecutive_failures < 3 {
            let small: Vec<&PrefabTemplate> = eligible.iter()
                .filter(|p| p.width * p.height < MEDIUM_THRESHOLD)
                .filter(|p| !placed_names.contains(&p.name))
                .collect();

            if small.is_empty() { break; }

            let prefab = small[rng.range(0, small.len() as i32) as usize];

            if self.try_place_oriented(build_data, prefab, &mut rng, &mut occupied_regions) {
                let area = prefab.width * prefab.height;
                budget -= area;
                placed_names.insert(prefab.name.clone());
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
            }
        }
    }

    /// Try all orientations of a prefab, recording the footprint on success.
    fn try_place_oriented(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        rng: &mut RandomNumberGenerator,
        occupied: &mut Vec<Rect>,
    ) -> bool {
        let mut orientations = generate_orientations(prefab);
        let n = orientations.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            orientations.swap(i as usize, j as usize);
        }

        for oriented in &orientations {
            let try_room = oriented.placement != "wall";
            let try_wall = oriented.placement != "room";

            if try_room {
                if let Some(rect) = self.try_room_placement_with_overlap(
                    build_data, oriented, rng, occupied,
                ) {
                    occupied.push(rect);
                    build_data.add_exclusion_zone(rect);
                    return true;
                }
            }
            if try_wall {
                if let Some(rect) = self.try_wall_carve_placement_with_overlap(
                    build_data, oriented, rng, occupied,
                ) {
                    occupied.push(rect);
                    build_data.add_exclusion_zone(rect);
                    return true;
                }
            }
        }
        false
    }

    /// Room-overlay placement with overlap checking against already-placed prefabs.
    fn try_room_placement_with_overlap(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        rng: &mut RandomNumberGenerator,
        occupied: &[Rect],
    ) -> Option<Rect> {
        let Some(rooms) = build_data.rooms() else {
            return None;
        };

        let mut candidate_offsets: Vec<(i32, i32)> = rooms
            .iter()
            .filter_map(|r| {
                let rw = r.x2 - r.x1 + 1;
                let rh = r.y2 - r.y1 + 1;
                if rw >= prefab.width && rh >= prefab.height {
                    let ox = r.x1 + (rw - prefab.width) / 2;
                    let oy = r.y1 + (rh - prefab.height) / 2;
                    if !overlaps_placed(occupied, ox, oy, prefab.width, prefab.height) {
                        Some((ox, oy))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        if candidate_offsets.is_empty() {
            return None;
        }

        let n = candidate_offsets.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            candidate_offsets.swap(i as usize, j as usize);
        }

        for (offset_x, offset_y) in &candidate_offsets {
            if self.try_stamp_prefab(build_data, prefab, *offset_x, *offset_y) {
                return Some(Rect::with_size(*offset_x, *offset_y, prefab.width, prefab.height));
            }
        }

        None
    }

    /// Wall-carve placement with overlap checking against already-placed prefabs.
    fn try_wall_carve_placement_with_overlap(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        rng: &mut RandomNumberGenerator,
        occupied: &[Rect],
    ) -> Option<Rect> {
        let map_w = build_data.map.width;
        let map_h = build_data.map.height;

        // Instead of exhaustively scanning every position, use random sampling
        // with a reasonable attempt limit. This avoids O(map_w * map_h) per prefab.
        let max_x = map_w - prefab.width - 1;
        let max_y = map_h - prefab.height - 1;
        if max_x < 2 || max_y < 2 { return None; }

        let max_attempts = 200;
        let mut candidates: Vec<(i32, i32)> = Vec::new();

        for _ in 0..max_attempts {
            let ox = rng.range(1, max_x);
            let oy = rng.range(1, max_y);
            if self.wall_carve_fits(build_data, prefab, ox, oy)
                && !overlaps_placed(occupied, ox, oy, prefab.width, prefab.height)
            {
                candidates.push((ox, oy));
                if candidates.len() >= 10 { break; } // Enough candidates
            }
        }

        if candidates.is_empty() {
            return None;
        }

        for (ox, oy) in candidates {
            if let Some(door_pt) = self.find_connection_point(build_data, prefab, ox, oy) {
                let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();

                for (py, row_str) in prefab.tiles.iter().enumerate() {
                    for (px, ch) in row_str.chars().enumerate() {
                        let wx = ox + px as i32;
                        let wy = oy + py as i32;
                        let pt = Point::new(wx, wy);
                        if !build_data.map.in_bounds(pt) { continue; }
                        let idx = build_data.map.xy_idx(wx, wy);
                        snapshot.push((idx, build_data.map.tiles[idx]));

                        match ch {
                            '#' => build_data.map.tiles[idx].terrain = TerrainType::Wall,
                            '.' => build_data.map.tiles[idx].terrain = TerrainType::Floor,
                            '+' => build_data.map.tiles[idx].terrain = TerrainType::Door,
                            _ => {}
                        }
                    }
                }

                let door_idx = build_data.map.xy_idx(door_pt.x, door_pt.y);
                snapshot.push((door_idx, build_data.map.tiles[door_idx]));
                build_data.map.tiles[door_idx].terrain = TerrainType::Door;

                let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
                if let Some(start) = start {
                    if !check_connectivity(&build_data.map, start) {
                        for (idx, tile) in &snapshot {
                            build_data.map.tiles[*idx] = *tile;
                        }
                        continue;
                    }
                }

                self.add_prefab_spawns(build_data, prefab, ox, oy);
                return Some(Rect::with_size(ox, oy, prefab.width, prefab.height));
            }
        }

        None
    }

    /// Check if the prefab footprint is entirely wall tiles (suitable for carving).
    fn wall_carve_fits(&self, build_data: &BuilderMap, prefab: &PrefabTemplate, ox: i32, oy: i32) -> bool {
        for py in 0..prefab.height {
            for px in 0..prefab.width {
                let wx = ox + px;
                let wy = oy + py;
                let pt = Point::new(wx, wy);
                if !build_data.map.in_bounds(pt) { return false; }
                let idx = build_data.map.xy_idx(wx, wy);
                if build_data.map.tiles[idx].terrain != TerrainType::Wall {
                    return false;
                }
            }
        }
        true
    }

    /// Find a tile on the prefab border that is adjacent to existing floor in the dungeon.
    /// Returns the wall tile that should become a door.
    fn find_connection_point(&self, build_data: &BuilderMap, prefab: &PrefabTemplate, ox: i32, oy: i32) -> Option<Point> {
        let deltas = [(0, -1), (0, 1), (-1, 0), (1, 0)];

        // Check all border tiles of the prefab footprint
        for py in 0..prefab.height {
            for px in 0..prefab.width {
                // Only border tiles
                if px > 0 && px < prefab.width - 1 && py > 0 && py < prefab.height - 1 {
                    continue;
                }

                let wx = ox + px;
                let wy = oy + py;

                // Check if any neighbor outside the prefab is floor
                for (dx, dy) in &deltas {
                    let nx = wx + dx;
                    let ny = wy + dy;
                    // Must be outside the prefab footprint
                    if nx >= ox && nx < ox + prefab.width && ny >= oy && ny < oy + prefab.height {
                        continue;
                    }
                    let pt = Point::new(nx, ny);
                    if !build_data.map.in_bounds(pt) { continue; }
                    let idx = build_data.map.xy_idx(nx, ny);
                    if build_data.map.tiles[idx].terrain == TerrainType::Floor {
                        return Some(Point::new(wx, wy));
                    }
                }
            }
        }
        None
    }

    /// Stamp a prefab at the given offset with connectivity check and snapshot-revert.
    fn try_stamp_prefab(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        offset_x: i32,
        offset_y: i32,
    ) -> bool {
        let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();

        for (py, row_str) in prefab.tiles.iter().enumerate() {
            for (px, ch) in row_str.chars().enumerate() {
                let wx = offset_x + px as i32;
                let wy = offset_y + py as i32;
                let pt = Point::new(wx, wy);
                if !build_data.map.in_bounds(pt) { continue; }
                let idx = build_data.map.xy_idx(wx, wy);
                snapshot.push((idx, build_data.map.tiles[idx]));

                match ch {
                    '#' => build_data.map.tiles[idx].terrain = TerrainType::Wall,
                    '.' => build_data.map.tiles[idx].terrain = TerrainType::Floor,
                    '+' => build_data.map.tiles[idx].terrain = TerrainType::Door,
                    ' ' => {}
                    _ => {}
                }
            }
        }

        // Connectivity check
        let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
        if let Some(start) = start {
            if !check_connectivity(&build_data.map, start) {
                for (idx, tile) in &snapshot {
                    build_data.map.tiles[*idx] = *tile;
                }
                return false;
            }
        }

        // Success — add spawns.
        self.add_prefab_spawns(build_data, prefab, offset_x, offset_y);
        true
    }

    /// Add monster, prop, and item spawns for a successfully placed prefab.
    /// Uses faction-first role resolution: pick one faction that can fill all
    /// required roles at this depth, then resolve each role within that faction.
    fn add_prefab_spawns(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        offset_x: i32,
        offset_y: i32,
    ) {
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        // Collect all roles needed by this prefab.
        let needed_roles: Vec<&str> = prefab
            .monster_spawns
            .iter()
            .map(|ms| ms.role.as_str())
            .collect();

        // Pick a faction that can fill all roles at this depth.
        let eligible = self.role_table.eligible_factions(&needed_roles, depth);
        if eligible.is_empty() {
            // No faction can fill all roles — skip monster spawns entirely.
            // Still place props and items below.
        } else {
            let faction = &eligible[rng.range(0, eligible.len() as i32) as usize];

            // Resolve each monster spawn within the chosen faction.
            let monster_count = prefab.monster_spawns.len();
            let is_squad = monster_count >= 2;

            let squad_id = if is_squad {
                Some(build_data.squad_counter.next())
            } else {
                None
            };

            let squad_config = if is_squad {
                let behavior =
                    crate::game::squad::LeaderDeathBehavior::from_str(&prefab.on_leader_death);
                Some(SquadConfig {
                    on_leader_death: behavior,
                    flee_threshold: prefab.flee_threshold,
                })
            } else {
                None
            };

            let mut is_first_squad_member = true;
            for ms in &prefab.monster_spawns {
                let monster_name = match self.role_table.resolve_role(
                    faction,
                    &ms.role,
                    depth,
                    &mut rng,
                ) {
                    Some(name) => name,
                    None => continue,
                };

                let wx = offset_x + ms.x;
                let wy = offset_y + ms.y;
                let pos = Point::new(wx, wy);

                let mut entry = if let (Some(sid), Some(cfg)) = (squad_id, squad_config.clone()) {
                    let leader = is_first_squad_member;
                    is_first_squad_member = false;
                    SpawnEntry::squad(pos, monster_name, sid, cfg, leader)
                } else {
                    SpawnEntry::solo(pos, monster_name)
                };

                entry.patrol_route = match &ms.behavior {
                    crate::assets::MonsterBehavior::Sentry => {
                        Some(crate::game::ai::PatrolRoute {
                            state: crate::game::ai::PatrolState::sentry(pos),
                        })
                    }
                    crate::assets::MonsterBehavior::Patrol(waypoints) => {
                        let abs_points: Vec<Point> = waypoints.iter()
                            .map(|(wpx, wpy)| Point::new(offset_x + wpx, offset_y + wpy))
                            .collect();
                        Some(crate::game::ai::PatrolRoute {
                            state: crate::game::ai::PatrolState::waypoint(&abs_points),
                        })
                    }
                    crate::assets::MonsterBehavior::Roam { min, max } => {
                        Some(crate::game::ai::PatrolRoute {
                            state: crate::game::ai::PatrolState::area_roam(
                                Point::new(offset_x + min.0, offset_y + min.1),
                                Point::new(offset_x + max.0, offset_y + max.1),
                            ),
                        })
                    }
                    crate::assets::MonsterBehavior::Wander => None,
                };
                build_data.add_monster_spawn(entry);
            }
        }

        for pe in &prefab.props {
            let wx = offset_x + pe.x;
            let wy = offset_y + pe.y;
            build_data.add_prop_spawn(Point::new(wx, wy), pe.prop.clone());
        }

        for ie in &prefab.item_spawns {
            let wx = offset_x + ie.x;
            let wy = offset_y + ie.y;
            if let Some(ref item_name) = ie.item {
                build_data.add_item_spawn(Point::new(wx, wy), item_name.clone(), 1);
            }
        }
    }
}

/// Flood fill from `start` — returns true if all floor tiles are reachable.
fn check_connectivity(map: &crate::map::Map, start: Point) -> bool {
    let total = map.tiles.len();
    let total_walkable = map.tiles.iter().filter(|t| {
        matches!(
            t.terrain,
            TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::OpenDoor | TerrainType::Door
        )
    }).count();

    let mut visited = vec![false; total];
    let mut queue = VecDeque::new();
    let mut visited_count = 0usize;

    if map.in_bounds(start) {
        let idx = map.point2d_to_index(start);
        queue.push_back(idx);
        visited[idx] = true;
    }

    while let Some(current) = queue.pop_front() {
        visited_count += 1;
        let (cx, cy) = map.idx_xy(current);
        for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height { continue; }
            let idx = map.xy_idx(nx, ny);
            if visited[idx] { continue; }
            let terrain = map.tiles[idx].terrain;
            if matches!(
                terrain,
                TerrainType::Floor | TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::OpenDoor | TerrainType::Door
            ) {
                visited[idx] = true;
                queue.push_back(idx);
            }
        }
    }

    visited_count >= total_walkable
}
