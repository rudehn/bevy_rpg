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
                let faction_tag = asset.faction.to_lowercase();
                let role = crate::assets::infer_role(asset).to_string();
                if !faction_tag.is_empty() && !role.is_empty() {
                    entries.push(MonsterRoleEntry {
                        name: asset.name.clone(),
                        faction_tag,
                        role,
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
        let behavior = transform_behavior(&m.behavior, transform);
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
        let behavior = transform_behavior(&m.behavior, transform);
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

    /// Try orientations of a prefab, recording the footprint on success.
    /// Limits to MAX_ORIENTATIONS_PER_PREFAB to bound total work.
    fn try_place_oriented(
        &self,
        build_data: &mut BuilderMap,
        prefab: &PrefabTemplate,
        rng: &mut RandomNumberGenerator,
        occupied: &mut Vec<Rect>,
    ) -> bool {
        const MAX_ORIENTATIONS_PER_PREFAB: usize = 3;

        let mut orientations = generate_orientations(prefab);
        let n = orientations.len() as i32;
        for i in (1..n).rev() {
            let j = rng.range(0, i + 1);
            orientations.swap(i as usize, j as usize);
        }
        orientations.truncate(MAX_ORIENTATIONS_PER_PREFAB);

        for oriented in &orientations {
            let try_room = oriented.placement != "wall";
            let try_wall = oriented.placement != "room";

            if try_room
                && let Some(rect) = self.try_room_placement_with_overlap(
                    build_data, oriented, rng, occupied,
                ) {
                    occupied.push(rect);
                    build_data.add_exclusion_zone(rect);
                    return true;
                }
            if try_wall
                && let Some(rect) = self.try_wall_carve_placement_with_overlap(
                    build_data, oriented, rng, occupied,
                ) {
                    occupied.push(rect);
                    build_data.add_exclusion_zone(rect);
                    return true;
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
        let rooms = build_data.rooms()?;

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

        // Limit connectivity check attempts to avoid expensive repeated flood fills.
        let max_stamp_attempts = 5;
        for (offset_x, offset_y) in candidate_offsets.iter().take(max_stamp_attempts) {
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

        let max_attempts = 100;
        let mut candidates: Vec<(i32, i32)> = Vec::new();

        for _ in 0..max_attempts {
            let ox = rng.range(1, max_x);
            let oy = rng.range(1, max_y);
            if self.wall_carve_fits(build_data, prefab, ox, oy)
                && !overlaps_placed(occupied, ox, oy, prefab.width, prefab.height)
            {
                candidates.push((ox, oy));
                if candidates.len() >= 5 { break; } // Enough candidates
            }
        }

        if candidates.is_empty() {
            return None;
        }

        for (ox, oy) in candidates {
            if let Some(door_pt) = self.find_connection_point(build_data, prefab, ox, oy) {
                // Pre-compute walkable count before stamping.
                let walkable_before = count_walkable(&build_data.map);
                let mut walkable_delta: i32 = 0;
                let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();

                for (py, row_str) in prefab.tiles.iter().enumerate() {
                    for (px, ch) in row_str.chars().enumerate() {
                        let wx = ox + px as i32;
                        let wy = oy + py as i32;
                        let pt = Point::new(wx, wy);
                        if !build_data.map.in_bounds(pt) { continue; }
                        let idx = build_data.map.xy_idx(wx, wy);
                        let old_terrain = build_data.map.tiles[idx].terrain;
                        snapshot.push((idx, build_data.map.tiles[idx]));

                        let new_terrain = match ch {
                            '#' => Some(TerrainType::Wall),
                            '.' => Some(TerrainType::Floor),
                            '+' => Some(TerrainType::Door),
                            _ => None,
                        };

                        if let Some(nt) = new_terrain {
                            let was_walkable = is_walkable_terrain(old_terrain);
                            let now_walkable = is_walkable_terrain(nt);
                            if was_walkable && !now_walkable {
                                walkable_delta -= 1;
                            } else if !was_walkable && now_walkable {
                                walkable_delta += 1;
                            }
                            build_data.map.tiles[idx].terrain = nt;
                        }
                    }
                }

                let door_idx = build_data.map.xy_idx(door_pt.x, door_pt.y);
                let old_door_terrain = build_data.map.tiles[door_idx].terrain;
                snapshot.push((door_idx, build_data.map.tiles[door_idx]));
                if !is_walkable_terrain(old_door_terrain) {
                    walkable_delta += 1; // Door is walkable
                }
                build_data.map.tiles[door_idx].terrain = TerrainType::Door;

                let total_walkable = (walkable_before as i32 + walkable_delta) as usize;

                let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
                if let Some(start) = start
                    && !check_connectivity_fast(&build_data.map, start, total_walkable) {
                        for (idx, tile) in &snapshot {
                            build_data.map.tiles[*idx] = *tile;
                        }
                        continue;
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
        // Pre-compute walkable count before stamping so we can adjust incrementally.
        let walkable_before = count_walkable(&build_data.map);

        let mut snapshot: Vec<(usize, crate::map::tile::Tile)> = Vec::new();
        let mut walkable_delta: i32 = 0;

        for (py, row_str) in prefab.tiles.iter().enumerate() {
            for (px, ch) in row_str.chars().enumerate() {
                let wx = offset_x + px as i32;
                let wy = offset_y + py as i32;
                let pt = Point::new(wx, wy);
                if !build_data.map.in_bounds(pt) { continue; }
                let idx = build_data.map.xy_idx(wx, wy);
                let old_terrain = build_data.map.tiles[idx].terrain;
                snapshot.push((idx, build_data.map.tiles[idx]));

                let new_terrain = match ch {
                    '#' => Some(TerrainType::Wall),
                    '.' => Some(TerrainType::Floor),
                    '+' => Some(TerrainType::Door),
                    _ => None,
                };

                if let Some(nt) = new_terrain {
                    let was_walkable = is_walkable_terrain(old_terrain);
                    let now_walkable = is_walkable_terrain(nt);
                    if was_walkable && !now_walkable {
                        walkable_delta -= 1;
                    } else if !was_walkable && now_walkable {
                        walkable_delta += 1;
                    }
                    build_data.map.tiles[idx].terrain = nt;
                }
            }
        }

        let total_walkable = (walkable_before as i32 + walkable_delta) as usize;

        // Connectivity check
        let start = build_data.starting_position.as_ref().map(|p| Point::new(p.x, p.y));
        if let Some(start) = start
            && !check_connectivity_fast(&build_data.map, start, total_walkable) {
                for (idx, tile) in &snapshot {
                    build_data.map.tiles[*idx] = *tile;
                }
                return false;
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

/// Count walkable tiles in the map.
fn count_walkable(map: &crate::map::Map) -> usize {
    map.tiles
        .iter()
        .filter(|t| is_walkable_terrain(t.terrain))
        .count()
}

#[inline]
fn is_walkable_terrain(terrain: TerrainType) -> bool {
    matches!(
        terrain,
        TerrainType::Floor
            | TerrainType::DownStairs
            | TerrainType::UpStairs
            | TerrainType::OpenDoor
            | TerrainType::Door
    )
}

/// Flood fill from `start` — returns true if all floor tiles are reachable.
/// Accepts a pre-computed walkable count to avoid redundant full-map scans.
fn check_connectivity_fast(
    map: &crate::map::Map,
    start: Point,
    total_walkable: usize,
) -> bool {
    if total_walkable == 0 {
        return true;
    }

    let total = map.tiles.len();
    let mut visited = vec![false; total];
    let mut queue = VecDeque::new();
    let mut visited_count = 0usize;
    let w = map.width;
    let h = map.height;

    if map.in_bounds(start) {
        let idx = map.point2d_to_index(start);
        queue.push_back(idx);
        visited[idx] = true;
    }

    while let Some(current) = queue.pop_front() {
        visited_count += 1;
        // Early exit: already reached all walkable tiles.
        if visited_count >= total_walkable {
            return true;
        }
        let (cx, cy) = map.idx_xy(current);
        for (dx, dy) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let idx = (ny * w + nx) as usize;
            if idx >= total || visited[idx] {
                continue;
            }
            if is_walkable_terrain(map.tiles[idx].terrain) {
                visited[idx] = true;
                queue.push_back(idx);
            }
        }
    }

    visited_count >= total_walkable
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bracket_lib::prelude::Rect;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Helper constructors
    // -----------------------------------------------------------------------

    /// Build a minimal PrefabTemplate from a tile grid. Defaults: floor 1-26,
    /// allow_rotate = true, allow_flip = true, placement = "any".
    fn make_prefab(name: &str, tiles: &[&str]) -> PrefabTemplate {
        let height = tiles.len() as i32;
        let width = if tiles.is_empty() { 0 } else { tiles[0].len() as i32 };
        PrefabTemplate {
            name: name.to_string(),
            width,
            height,
            min_floor: 1,
            max_floor: 26,
            tiles: tiles.iter().map(|s| s.to_string()).collect(),
            props: Vec::new(),
            monster_spawns: Vec::new(),
            item_spawns: Vec::new(),
            on_leader_death: String::new(),
            flee_threshold: 0.5,
            placement: "any".to_string(),
            allow_rotate: true,
            allow_flip: true,
        }
    }

    /// Build a PrefabTemplate with explicit rotation/flip flags.
    fn make_prefab_with_flags(
        name: &str,
        tiles: &[&str],
        allow_rotate: bool,
        allow_flip: bool,
    ) -> PrefabTemplate {
        let mut p = make_prefab(name, tiles);
        p.allow_rotate = allow_rotate;
        p.allow_flip = allow_flip;
        p
    }

    /// Build a minimal MonsterRoleTable from a list of
    /// (name, faction, role, min_floor, max_floor) tuples.
    fn make_role_table(entries: &[(&str, &str, &str, i32, i32)]) -> MonsterRoleTable {
        MonsterRoleTable {
            entries: entries
                .iter()
                .map(|(name, faction, role, min_f, max_f)| MonsterRoleEntry {
                    name: name.to_string(),
                    faction_tag: faction.to_string(),
                    role: role.to_string(),
                    min_floor: *min_f,
                    max_floor: *max_f,
                })
                .collect(),
        }
    }

    // =======================================================================
    // overlaps_placed
    // =======================================================================

    #[test]
    fn overlaps_placed_empty_list_returns_false() {
        let occupied: Vec<Rect> = Vec::new();
        assert!(!overlaps_placed(&occupied, 10, 10, 5, 5));
    }

    #[test]
    fn overlaps_placed_non_overlapping_rects() {
        // Place one rect at (0,0) size 5x5.
        let occupied = vec![Rect::with_size(0, 0, 5, 5)];
        // Another rect far away.
        assert!(!overlaps_placed(&occupied, 50, 50, 5, 5));
    }

    #[test]
    fn overlaps_placed_directly_overlapping() {
        let occupied = vec![Rect::with_size(10, 10, 5, 5)];
        // Exact same position.
        assert!(overlaps_placed(&occupied, 10, 10, 5, 5));
    }

    #[test]
    fn overlaps_placed_partial_overlap() {
        let occupied = vec![Rect::with_size(10, 10, 5, 5)];
        // Shifted slightly to overlap.
        assert!(overlaps_placed(&occupied, 12, 12, 5, 5));
    }

    #[test]
    fn overlaps_placed_respects_padding_close_but_within_pad() {
        // First rect at (10, 10) size 5x5 → occupies x 10..14, y 10..14.
        let occupied = vec![Rect::with_size(10, 10, 5, 5)];
        // Place a rect adjacent (1 tile gap) — padding is 2, so this should overlap.
        // New rect at (16, 10) → with padding it expands to (14, 8) size (9, 9),
        // which overlaps the existing rect ending at x=14.
        assert!(overlaps_placed(&occupied, 16, 10, 5, 5));
    }

    #[test]
    fn overlaps_placed_respects_padding_far_enough() {
        // First rect at (10, 10) size 5x5 → Rect x1=10 x2=14, y1=10 y2=14.
        let occupied = vec![Rect::with_size(10, 10, 5, 5)];
        // Place rect far enough away that even with PREFAB_PADDING=2 there is no overlap.
        // New rect at (20, 10) → with padding expands to (18, 8) size (9, 9) → x1=18..x2=26.
        // Existing rect x2=14. 18 > 14, so no overlap.
        assert!(!overlaps_placed(&occupied, 20, 10, 5, 5));
    }

    #[test]
    fn overlaps_placed_multiple_occupied_regions() {
        let occupied = vec![
            Rect::with_size(0, 0, 5, 5),
            Rect::with_size(30, 30, 5, 5),
        ];
        // Overlaps second region.
        assert!(overlaps_placed(&occupied, 30, 30, 3, 3));
        // Overlaps neither.
        assert!(!overlaps_placed(&occupied, 15, 15, 3, 3));
    }

    // =======================================================================
    // generate_orientations
    // =======================================================================

    #[test]
    fn orientations_symmetric_square_deduplicates() {
        // A fully symmetric 2x2 block of the same character should produce
        // fewer unique orientations since rotations/flips are identical.
        let prefab = make_prefab("sym", &["##", "##"]);
        let orientations = generate_orientations(&prefab);
        // All rotations and flips of a uniform grid are identical.
        assert_eq!(orientations.len(), 1);
    }

    #[test]
    fn orientations_asymmetric_prefab_produces_multiple() {
        // An L-shaped prefab (asymmetric) should produce up to 8 variants.
        let prefab = make_prefab("asym", &["#.", "##"]);
        let orientations = generate_orientations(&prefab);
        // With allow_rotate=true and allow_flip=true, an asymmetric shape
        // should yield more than 1 unique orientation.
        assert!(
            orientations.len() > 1,
            "expected multiple orientations, got {}",
            orientations.len()
        );
        // Maximum possible is 8 (4 rotations * 2 flip states).
        assert!(orientations.len() <= 8);
    }

    #[test]
    fn orientations_no_rotate_returns_original_and_flip() {
        let prefab = make_prefab_with_flags("no_rot", &["#.", "##"], false, true);
        let orientations = generate_orientations(&prefab);
        // Without rotation: identity + flip = at most 2.
        assert!(orientations.len() <= 2);
        assert!(orientations.len() >= 1);
        // First orientation is always the original.
        assert_eq!(orientations[0].tiles, prefab.tiles);
    }

    #[test]
    fn orientations_no_rotate_no_flip_returns_only_original() {
        let prefab = make_prefab_with_flags("static", &["#.", "##"], false, false);
        let orientations = generate_orientations(&prefab);
        assert_eq!(orientations.len(), 1);
        assert_eq!(orientations[0].tiles, prefab.tiles);
    }

    #[test]
    fn orientations_rotate_no_flip() {
        let prefab = make_prefab_with_flags("rot_only", &["#.", "##"], true, false);
        let orientations = generate_orientations(&prefab);
        // Up to 4 rotations, deduplicated.
        assert!(orientations.len() >= 1 && orientations.len() <= 4);
    }

    #[test]
    fn orientations_preserves_dimensions_on_rotation() {
        // A 3x2 prefab rotated 90 degrees becomes 2x3.
        let prefab = make_prefab("rect", &["#.#", "##."]);
        let orientations = generate_orientations(&prefab);
        assert_eq!(orientations[0].width, 3);
        assert_eq!(orientations[0].height, 2);

        // Find a rotated variant (should have swapped dimensions).
        let has_swapped = orientations.iter().any(|o| o.width == 2 && o.height == 3);
        assert!(has_swapped, "expected a rotated orientation with swapped dimensions");
    }

    #[test]
    fn orientations_identity_always_first() {
        let prefab = make_prefab("first", &["#.", ".#", "##"]);
        let orientations = generate_orientations(&prefab);
        assert_eq!(orientations[0].tiles, prefab.tiles);
        assert_eq!(orientations[0].width, prefab.width);
        assert_eq!(orientations[0].height, prefab.height);
    }

    // =======================================================================
    // rotate_90_cw
    // =======================================================================

    #[test]
    fn rotate_90_cw_2x2() {
        let prefab = make_prefab("r", &["AB", "CD"]);
        let rotated = rotate_90_cw(&prefab);
        // 90 CW: new_x = H-1-old_y, new_y = old_x
        // Original: row0="AB", row1="CD"
        // (0,0)=A → new(1,0), (1,0)=B → new(1,1), (0,1)=C → new(0,0), (1,1)=D → new(0,1)
        // New row0: C A, New row1: D B
        assert_eq!(rotated.tiles, vec!["CA", "DB"]);
        assert_eq!(rotated.width, 2);
        assert_eq!(rotated.height, 2);
    }

    #[test]
    fn rotate_90_cw_rectangular() {
        // 3 wide x 2 tall
        let prefab = make_prefab("r", &["ABC", "DEF"]);
        let rotated = rotate_90_cw(&prefab);
        // Original: W=3, H=2 → New: W=2, H=3
        // (0,0)=A→new(1,0), (1,0)=B→new(1,1), (2,0)=C→new(1,2)
        // (0,1)=D→new(0,0), (1,1)=E→new(0,1), (2,1)=F→new(0,2)
        // New row0: DA, row1: EB, row2: FC
        assert_eq!(rotated.tiles, vec!["DA", "EB", "FC"]);
        assert_eq!(rotated.width, 2);
        assert_eq!(rotated.height, 3);
    }

    #[test]
    fn rotate_180_is_double_rotation() {
        let prefab = make_prefab("r", &["AB", "CD"]);
        let rotated = rotate_prefab(&prefab, 2);
        // 180: (0,0)=A→(1,1), (1,0)=B→(0,1), (0,1)=C→(1,0), (1,1)=D→(0,0)
        // New row0: DC, row1: BA
        assert_eq!(rotated.tiles, vec!["DC", "BA"]);
    }

    #[test]
    fn rotate_360_returns_to_original() {
        let prefab = make_prefab("r", &["ABC", "DEF"]);
        let rotated = rotate_prefab(&prefab, 4);
        assert_eq!(rotated.tiles, prefab.tiles);
        assert_eq!(rotated.width, prefab.width);
        assert_eq!(rotated.height, prefab.height);
    }

    // =======================================================================
    // flip_prefab_h
    // =======================================================================

    #[test]
    fn flip_horizontal_2x2() {
        let prefab = make_prefab("f", &["AB", "CD"]);
        let flipped = flip_prefab_h(&prefab);
        assert_eq!(flipped.tiles, vec!["BA", "DC"]);
        assert_eq!(flipped.width, prefab.width);
        assert_eq!(flipped.height, prefab.height);
    }

    #[test]
    fn flip_horizontal_rectangular() {
        let prefab = make_prefab("f", &["ABC", "DEF"]);
        let flipped = flip_prefab_h(&prefab);
        assert_eq!(flipped.tiles, vec!["CBA", "FED"]);
    }

    #[test]
    fn double_flip_returns_original() {
        let prefab = make_prefab("f", &["#.", ".#", "##"]);
        let flipped_twice = flip_prefab_h(&flip_prefab_h(&prefab));
        assert_eq!(flipped_twice.tiles, prefab.tiles);
    }

    // =======================================================================
    // MonsterRoleTable::eligible_factions
    // =======================================================================

    #[test]
    fn eligible_factions_empty_table() {
        let table = make_role_table(&[]);
        let factions = table.eligible_factions(&["melee_guard"], 1);
        assert!(factions.is_empty());
    }

    #[test]
    fn eligible_factions_single_faction_all_roles() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
            ("Goblin Archer", "goblin", "ranged", 1, 5),
        ]);
        let factions = table.eligible_factions(&["melee_guard", "ranged"], 3);
        assert_eq!(factions.len(), 1);
        assert_eq!(factions[0], "goblin");
    }

    #[test]
    fn eligible_factions_excludes_faction_missing_role() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
            // Goblins have no ranged — should be excluded when ranged is required.
            ("Skeleton Archer", "undead", "ranged", 1, 5),
            ("Skeleton Guard", "undead", "melee_guard", 1, 5),
        ]);
        let factions = table.eligible_factions(&["melee_guard", "ranged"], 3);
        assert_eq!(factions.len(), 1);
        assert_eq!(factions[0], "undead");
    }

    #[test]
    fn eligible_factions_depth_filtering() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 3),
            ("Goblin Archer", "goblin", "ranged", 1, 3),
        ]);
        // At depth 3, goblins are eligible.
        assert_eq!(table.eligible_factions(&["melee_guard", "ranged"], 3).len(), 1);
        // At depth 4, goblins are out of range.
        assert!(table.eligible_factions(&["melee_guard", "ranged"], 4).is_empty());
    }

    #[test]
    fn eligible_factions_multiple_factions_qualify() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
            ("Goblin Archer", "goblin", "ranged", 1, 5),
            ("Skeleton Guard", "undead", "melee_guard", 1, 5),
            ("Skeleton Archer", "undead", "ranged", 1, 5),
        ]);
        let mut factions = table.eligible_factions(&["melee_guard", "ranged"], 3);
        factions.sort();
        assert_eq!(factions, vec!["goblin", "undead"]);
    }

    #[test]
    fn eligible_factions_no_roles_required() {
        // With no roles required, every faction that has any entry in range qualifies.
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
        ]);
        let factions = table.eligible_factions(&[], 3);
        // Empty needed set means all factions pass the "all needed" check.
        assert!(!factions.is_empty());
    }

    #[test]
    fn eligible_factions_duplicate_roles_treated_as_one() {
        // Prefab might request ["melee_guard", "melee_guard"] — unique roles = {"melee_guard"}.
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
        ]);
        let factions = table.eligible_factions(&["melee_guard", "melee_guard"], 3);
        assert_eq!(factions.len(), 1);
    }

    // =======================================================================
    // MonsterRoleTable::resolve_role
    // =======================================================================

    #[test]
    fn resolve_role_returns_valid_name() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
            ("Goblin Brute", "goblin", "melee_guard", 1, 5),
        ]);
        let mut rng = RandomNumberGenerator::new();
        let result = table.resolve_role("goblin", "melee_guard", 3, &mut rng);
        assert!(result.is_some());
        let name = result.unwrap();
        assert!(
            name == "Goblin Warrior" || name == "Goblin Brute",
            "unexpected name: {name}"
        );
    }

    #[test]
    fn resolve_role_wrong_faction_returns_none() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
        ]);
        let mut rng = RandomNumberGenerator::new();
        assert!(table.resolve_role("undead", "melee_guard", 3, &mut rng).is_none());
    }

    #[test]
    fn resolve_role_wrong_role_returns_none() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 5),
        ]);
        let mut rng = RandomNumberGenerator::new();
        assert!(table.resolve_role("goblin", "caster", 3, &mut rng).is_none());
    }

    #[test]
    fn resolve_role_out_of_depth_returns_none() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 1, 3),
        ]);
        let mut rng = RandomNumberGenerator::new();
        assert!(table.resolve_role("goblin", "melee_guard", 5, &mut rng).is_none());
    }

    #[test]
    fn resolve_role_at_exact_depth_boundaries() {
        let table = make_role_table(&[
            ("Goblin Warrior", "goblin", "melee_guard", 3, 7),
        ]);
        let mut rng = RandomNumberGenerator::new();
        // At min_floor boundary.
        assert!(table.resolve_role("goblin", "melee_guard", 3, &mut rng).is_some());
        // At max_floor boundary.
        assert!(table.resolve_role("goblin", "melee_guard", 7, &mut rng).is_some());
        // Below min.
        assert!(table.resolve_role("goblin", "melee_guard", 2, &mut rng).is_none());
        // Above max.
        assert!(table.resolve_role("goblin", "melee_guard", 8, &mut rng).is_none());
    }

    // =======================================================================
    // is_walkable_terrain
    // =======================================================================

    #[test]
    fn walkable_terrain_classification() {
        assert!(is_walkable_terrain(TerrainType::Floor));
        assert!(is_walkable_terrain(TerrainType::DownStairs));
        assert!(is_walkable_terrain(TerrainType::UpStairs));
        assert!(is_walkable_terrain(TerrainType::Door));
        assert!(is_walkable_terrain(TerrainType::OpenDoor));
        assert!(!is_walkable_terrain(TerrainType::Wall));
        assert!(!is_walkable_terrain(TerrainType::Empty));
    }

    // =======================================================================
    // count_walkable
    // =======================================================================

    #[test]
    fn count_walkable_all_walls() {
        let map = crate::map::Map::new(1, 10, 10, "test");
        assert_eq!(count_walkable(&map), 0);
    }

    #[test]
    fn count_walkable_mixed_tiles() {
        let mut map = crate::map::Map::new(1, 10, 10, "test");
        // Carve 3 walkable tiles at known indices.
        map.tiles[11].terrain = TerrainType::Floor;  // (1, 1)
        map.tiles[12].terrain = TerrainType::Floor;  // (2, 1)
        map.tiles[13].terrain = TerrainType::Door;   // (3, 1)
        assert_eq!(count_walkable(&map), 3);
    }

    // =======================================================================
    // check_connectivity_fast
    // =======================================================================

    #[test]
    fn connectivity_all_walls_zero_walkable() {
        let map = crate::map::Map::new(1, 10, 10, "test");
        // 0 walkable tiles ⇒ trivially connected.
        assert!(check_connectivity_fast(&map, Point::new(5, 5), 0));
    }

    #[test]
    fn connectivity_single_floor_tile() {
        let mut map = crate::map::Map::new(1, 10, 10, "test");
        let idx = map.xy_idx(5, 5);
        map.tiles[idx].terrain = TerrainType::Floor;
        assert!(check_connectivity_fast(&map, Point::new(5, 5), 1));
    }

    #[test]
    fn connectivity_connected_corridor() {
        let mut map = crate::map::Map::new(1, 10, 10, "test");
        // Carve a horizontal corridor at y=5.
        for x in 1..9 {
            let idx = map.xy_idx(x, 5);
            map.tiles[idx].terrain = TerrainType::Floor;
        }
        assert!(check_connectivity_fast(&map, Point::new(1, 5), 8));
    }

    #[test]
    fn connectivity_disconnected_regions() {
        let mut map = crate::map::Map::new(1, 20, 20, "test");
        // Region A.
        let idx_a1 = map.xy_idx(1, 1);
        let idx_a2 = map.xy_idx(2, 1);
        let idx_b = map.xy_idx(18, 18);
        map.tiles[idx_a1].terrain = TerrainType::Floor;
        map.tiles[idx_a2].terrain = TerrainType::Floor;
        // Region B (disconnected).
        map.tiles[idx_b].terrain = TerrainType::Floor;
        // Total walkable = 3, but only 2 reachable from start.
        assert!(!check_connectivity_fast(&map, Point::new(1, 1), 3));
    }

    // =======================================================================
    // transform_behavior
    // =======================================================================

    #[test]
    fn transform_behavior_sentry_unchanged() {
        use crate::assets::MonsterBehavior;
        let result = transform_behavior(&MonsterBehavior::Sentry, |x, y| (y, x));
        assert!(matches!(result, MonsterBehavior::Sentry));
    }

    #[test]
    fn transform_behavior_wander_unchanged() {
        use crate::assets::MonsterBehavior;
        let result = transform_behavior(&MonsterBehavior::Wander, |x, y| (y, x));
        assert!(matches!(result, MonsterBehavior::Wander));
    }

    #[test]
    fn transform_behavior_patrol_transforms_points() {
        use crate::assets::MonsterBehavior;
        let patrol = MonsterBehavior::Patrol(vec![(1, 2), (3, 4)]);
        // Simple swap transform.
        let result = transform_behavior(&patrol, |x, y| (y, x));
        match result {
            MonsterBehavior::Patrol(pts) => {
                assert_eq!(pts, vec![(2, 1), (4, 3)]);
            }
            _ => panic!("expected Patrol"),
        }
    }

    #[test]
    fn transform_behavior_roam_normalizes_min_max() {
        use crate::assets::MonsterBehavior;
        let roam = MonsterBehavior::Roam {
            min: (0, 0),
            max: (5, 5),
        };
        // A flip transform: new_x = 10-x, new_y = y → (0,0)→(10,0), (5,5)→(5,5)
        let result = transform_behavior(&roam, |x, y| (10 - x, y));
        match result {
            MonsterBehavior::Roam { min, max } => {
                // min should be the component-wise minimum.
                assert_eq!(min, (5, 0));
                assert_eq!(max, (10, 5));
            }
            _ => panic!("expected Roam"),
        }
    }

    // =======================================================================
    // Budget and bounds (integration-level)
    // =======================================================================

    #[test]
    fn base_prefab_budget_is_positive() {
        assert!(BASE_PREFAB_BUDGET > 0);
    }

    #[test]
    fn prefab_padding_is_positive() {
        assert!(PREFAB_PADDING > 0);
    }

    #[test]
    fn medium_threshold_is_reasonable() {
        // Must be > 0 and less than a full map.
        assert!(MEDIUM_THRESHOLD > 0);
        assert!(MEDIUM_THRESHOLD < 80 * 60);
    }

    // =======================================================================
    // MonsterRoleTable::from_manifest
    // =======================================================================

    #[test]
    fn from_manifest_skips_group_spawns() {
        use crate::assets::{GroupMember, MonsterSpawnInfo};
        let mut monsters = HashMap::new();
        monsters.insert("rat".to_string(), make_test_monster("rat", "vermin"));

        let spawn_table = vec![MonsterSpawnInfo {
            monster: String::new(),
            min_floor: 1,
            max_floor: 5,
            min_group: 1,
            max_group: 2,
            group: vec![GroupMember {
                monster: "rat".to_string(),
                min_count: 2,
                max_count: 2,
            }],
            on_leader_death: String::new(),
            flee_threshold: 0.5,
            spawn_on_liquid: false,
        }];

        let table = MonsterRoleTable::from_manifest(&monsters, &spawn_table);
        assert!(table.entries.is_empty(), "group spawns should be skipped");
    }

    #[test]
    fn from_manifest_populates_entries() {
        use crate::assets::MonsterSpawnInfo;
        let mut monsters = HashMap::new();
        monsters.insert("rat".to_string(), make_test_monster("rat", "vermin"));

        let spawn_table = vec![MonsterSpawnInfo {
            monster: "rat".to_string(),
            min_floor: 1,
            max_floor: 3,
            min_group: 1,
            max_group: 1,
            group: Vec::new(),
            on_leader_death: String::new(),
            flee_threshold: 0.5,
            spawn_on_liquid: false,
        }];

        let table = MonsterRoleTable::from_manifest(&monsters, &spawn_table);
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[0].name, "rat");
        assert_eq!(table.entries[0].faction_tag, "vermin");
        assert_eq!(table.entries[0].min_floor, 1);
        assert_eq!(table.entries[0].max_floor, 3);
    }

    /// Build a minimal MonsterAsset for testing from_manifest.
    fn make_test_monster(name: &str, faction: &str) -> crate::assets::MonsterAsset {
        use bevy::prelude::Color;
        crate::assets::MonsterAsset {
            name: name.to_string(),
            vision: 6,
            sprite: String::new(),
            grid_size: None,
            tile_size: None,
            base_hp: 10,
            damage: "1d4".to_string(),
            regen: None,
            loot_table: Vec::new(),
            damage_type: "physical".to_string(),
            resistances: HashMap::new(),
            base_armor: 0,
            faction: faction.to_string(),
            abilities: Vec::new(),
            monster_abilities: Vec::new(),
            ascii_char: String::new(),
            ascii_fg: Color::WHITE,
            ai: crate::assets::AiConfig::default(),
            base_dodge: 0,
            movement_delay: 1.0,
            attack_delay: 1.0,
            movement_mode: crate::components::MovementMode::default(),
            stationary: false,
            species: crate::components::Species::default(),
        }
    }

    // =======================================================================
    // Orientation transforms with monster_spawns / props / items
    // =======================================================================

    #[test]
    fn rotate_transforms_monster_spawn_coordinates() {
        let mut prefab = make_prefab("ms", &["#.", "##"]);
        prefab.monster_spawns.push(crate::assets::PrefabMonsterSpawn {
            x: 1,
            y: 0,
            role: "melee_guard".to_string(),
            behavior: crate::assets::MonsterBehavior::Sentry,
        });
        // Original: W=2, H=2. Spawn at (1,0).
        // 90 CW: new_x = H-1-old_y = 2-1-0 = 1, new_y = old_x = 1.
        let rotated = rotate_90_cw(&prefab);
        assert_eq!(rotated.monster_spawns.len(), 1);
        assert_eq!(rotated.monster_spawns[0].x, 1);
        assert_eq!(rotated.monster_spawns[0].y, 1);
    }

    #[test]
    fn flip_transforms_item_spawn_coordinates() {
        let mut prefab = make_prefab("is", &["#.", "##"]);
        prefab.item_spawns.push(crate::assets::PrefabItemSpawn {
            x: 0,
            y: 1,
            item: Some("sword".to_string()),
        });
        // Flip: new_x = W-1-old_x = 2-1-0 = 1, new_y = old_y = 1.
        let flipped = flip_prefab_h(&prefab);
        assert_eq!(flipped.item_spawns.len(), 1);
        assert_eq!(flipped.item_spawns[0].x, 1);
        assert_eq!(flipped.item_spawns[0].y, 1);
    }

    #[test]
    fn flip_transforms_prop_coordinates() {
        let mut prefab = make_prefab("ps", &["#.", "##"]);
        prefab.props.push(crate::assets::PrefabPropEntry {
            x: 1,
            y: 0,
            prop: "candle".to_string(),
        });
        // Flip: new_x = W-1-old_x = 2-1-1 = 0, new_y = old_y = 0.
        let flipped = flip_prefab_h(&prefab);
        assert_eq!(flipped.props.len(), 1);
        assert_eq!(flipped.props[0].x, 0);
        assert_eq!(flipped.props[0].y, 0);
    }
}
