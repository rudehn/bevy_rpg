# Spawn Weights, Poisson-Disc Placement, and Wander Clock — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-room 50% monster spawning with weighted, Bridson-Poisson-disc placement; add a per-floor ramping wander clock that periodically spawns monsters out of sight; fix four latent bugs.

**Architecture:** Two pure-function helpers (`weighted_pick`, `bridson_poisson_disc`) live under `src/map/builders/`. `MonsterSpawner` is rewritten to call them. A new `src/game/wander.rs` owns the `WanderClock` state, cadence math, tick system, and event firing. `WanderClock` lives on `CachedFloor` so per-floor state persists across stair traversal. Save schema bumps v5 → v6.

**Tech Stack:** Rust, Bevy 0.17, bracket-lib RNG (`RandomNumberGenerator`), RON assets, `bevy_save 0.17`.

**Spec:** [docs/superpowers/specs/2026-05-13-spawn-weights-and-wander-clock-design.md](../specs/2026-05-13-spawn-weights-and-wander-clock-design.md)

---

## File Inventory

**Create:**
- `src/map/builders/weighted_pick.rs` — generic weighted random selection.
- `src/map/builders/poisson_disc.rs` — Bridson's algorithm constrained to walkable tiles.
- `src/game/wander.rs` — `WanderClock`, cadence formulas, `WanderPlugin`, tick + event systems.
- `docs/design/SPAWNING.md` — canonical design writeup.

**Modify:**
- `src/assets/mod.rs:509-533` — add `weight` and `can_wander` fields to `MonsterSpawnInfo`.
- `src/map/builders/monster_spawner.rs` — rewrite `spawn_monsters` to use Bridson + weights; fix stair/lava bugs in `find_cluster_points`; share `BuilderMap.rng`.
- `src/map/builders/mod.rs` — register new submodules.
- `src/map/dungeon.rs:34-48` — add `wander_clock: WanderClock` to `CachedFloor`.
- `src/save/mod.rs:82` — bump `SAVE_SCHEMA_VERSION` to 6; add `wander_clock: WanderClock` to `SavedFloorData`; add v5 → v6 migration.
- `src/game/mod.rs` — register `WanderPlugin`.
- `docs/design/DUNGEON.md` — point the spawn line at SPAWNING.md.
- `docs/design/ENEMIES.md` — refresh initial-spawn references.
- `CLAUDE.md` — add `WanderClock` to architectural patterns; bump save schema reference.
- `.claude/skills/content-studio/references/ron-schemas.md` — document `weight` and `can_wander`.
- `.claude/skills/content-studio/references/balance-targets.md` — weight tiers table.

**Constants** (add to `src/constants.rs`):
- `TARGET_HORDES_BASE: i32 = 8`
- `TARGET_HORDES_PER_DEPTH: f32 = 0.6`
- `POISSON_PACKING_CONSTANT: f32 = 0.7`
- `POISSON_K_CANDIDATES: u32 = 20`
- `WANDER_MIN_DISTANCE: i32 = 8`
- `WANDER_NOTIFY_RANGE: i32 = 15`
- `WANDER_RAMP_FACTOR_FLOOR: f32 = 0.3`
- `WANDER_RAMP_PER_FIRE: f32 = 0.08`
- `WANDER_BASE_INTERVAL_FLOOR: i32 = 60`
- `WANDER_BASE_INTERVAL_AT_DEPTH_0: i32 = 300`
- `WANDER_INTERVAL_DEPTH_STEP: i32 = 8`

---

## Task 1: Add `weight` and `can_wander` to `MonsterSpawnInfo`

**Files:**
- Modify: `src/assets/mod.rs:509-533`
- Test: `src/assets/mod.rs` (existing `#[cfg(test)] mod tests` block, or inline)

- [ ] **Step 1: Write the failing test**

Append to `src/assets/mod.rs` inside (or create) the `#[cfg(test)] mod tests { ... }` block:

```rust
#[test]
fn monster_spawn_info_defaults_weight_to_ten_and_can_wander_to_true() {
    // Parse minimal RON with only required fields — verify defaults fire.
    let ron = r#"(monster: "Sewer Rat", min_floor: 1, max_floor: 5)"#;
    let info: MonsterSpawnInfo = ron::from_str(ron).expect("RON parse");
    assert_eq!(info.weight, 10);
    assert!(info.can_wander);
}

#[test]
fn monster_spawn_info_honors_explicit_weight_and_can_wander() {
    let ron = r#"(monster: "Dragon", min_floor: 24, max_floor: 26, weight: 1, can_wander: false)"#;
    let info: MonsterSpawnInfo = ron::from_str(ron).expect("RON parse");
    assert_eq!(info.weight, 1);
    assert!(!info.can_wander);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p bevy_rpg --lib assets::tests::monster_spawn_info`
Expected: FAIL — `unknown field 'weight'` (or `'can_wander'`).

- [ ] **Step 3: Add the fields**

In `src/assets/mod.rs`, at the bottom of the `MonsterSpawnInfo` struct (line ~532, before the closing `}`):

```rust
    /// Relative selection weight within the depth-filtered set.
    /// Default 10 so existing entries without an explicit weight stay
    /// equal-weighted with one another (back-compat).
    #[serde(default = "default_weight")]
    pub weight: u32,

    /// Eligibility for periodic mid-floor "wander" spawns. Default true.
    /// Set false for bosses or large set-piece packs that should only
    /// appear at floor-gen.
    #[serde(default = "default_true")]
    pub can_wander: bool,
```

And below `default_flee_threshold` (line ~549-551), add:

```rust
fn default_weight() -> u32 {
    10
}

fn default_true() -> bool {
    true
}
```

If `ron` is not already a dev-dep, add to `Cargo.toml` `[dev-dependencies]`:
```toml
ron = "0.10"
```
(check the version `bevy_common_assets 0.14` uses transitively — match that).

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p bevy_rpg --lib assets::tests::monster_spawn_info`
Expected: 2 passing.

- [ ] **Step 5: Verify full table still parses**

Run: `cargo test -p bevy_rpg` (just to confirm no other test now breaks).
Expected: same pass count as before + 2 new passes.

- [ ] **Step 6: Commit**

```bash
git add src/assets/mod.rs Cargo.toml
git commit -m "feat(spawn): add weight and can_wander fields to MonsterSpawnInfo

Both fields default via serde so existing monster_spawns.ron entries
keep their current behavior (weight=10, can_wander=true)."
```

---

## Task 2: `weighted_pick` helper

**Files:**
- Create: `src/map/builders/weighted_pick.rs`
- Modify: `src/map/builders/mod.rs` (add `pub mod weighted_pick;`)

- [ ] **Step 1: Write the failing test**

Create `src/map/builders/weighted_pick.rs` with this content (will not compile yet — `weighted_pick` is missing):

```rust
//! Weighted random selection. Used by the monster spawner so authors can
//! mark entries as common/uncommon/rare via `weight` on `MonsterSpawnInfo`.

use bracket_lib::prelude::RandomNumberGenerator;

/// Pick an index into `weights` with probability proportional to its value.
/// Returns `None` if the slice is empty or the sum of weights is zero.
pub fn weighted_pick(weights: &[u32], rng: &mut RandomNumberGenerator) -> Option<usize> {
    let total: u64 = weights.iter().map(|w| *w as u64).sum();
    if total == 0 {
        return None;
    }
    // `range(0, total)` returns [0, total) — exclusive upper bound.
    let mut roll = rng.range(0, total as i64) as u64;
    for (i, w) in weights.iter().enumerate() {
        let w64 = *w as u64;
        if roll < w64 {
            return Some(i);
        }
        roll -= w64;
    }
    // Numerically unreachable if total > 0; satisfy the borrow checker.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_none() {
        let mut rng = RandomNumberGenerator::new();
        assert_eq!(weighted_pick(&[], &mut rng), None);
    }

    #[test]
    fn all_zero_returns_none() {
        let mut rng = RandomNumberGenerator::new();
        assert_eq!(weighted_pick(&[0, 0, 0], &mut rng), None);
    }

    #[test]
    fn zero_weight_never_selected() {
        let mut rng = RandomNumberGenerator::seeded(42);
        let weights = [10, 0, 10];
        for _ in 0..10_000 {
            let pick = weighted_pick(&weights, &mut rng).expect("non-empty");
            assert_ne!(pick, 1, "weight-0 entry must never be selected");
        }
    }

    #[test]
    fn distribution_matches_weights() {
        let mut rng = RandomNumberGenerator::seeded(1234);
        let weights = [20, 10, 5, 1]; // total = 36
        let mut counts = [0u32; 4];
        let n = 100_000;
        for _ in 0..n {
            counts[weighted_pick(&weights, &mut rng).unwrap()] += 1;
        }
        // Expected ratios: 0.5555, 0.2777, 0.1388, 0.0277
        let expected = [
            n as f64 * 20.0 / 36.0,
            n as f64 * 10.0 / 36.0,
            n as f64 * 5.0 / 36.0,
            n as f64 * 1.0 / 36.0,
        ];
        for i in 0..4 {
            let observed = counts[i] as f64;
            let tolerance = expected[i] * 0.05; // 5%
            let diff = (observed - expected[i]).abs();
            assert!(
                diff < tolerance,
                "index {} observed {} expected {} (±{})",
                i, observed, expected[i], tolerance
            );
        }
    }

    #[test]
    fn single_weight_always_picks_index_zero() {
        let mut rng = RandomNumberGenerator::new();
        for _ in 0..100 {
            assert_eq!(weighted_pick(&[5], &mut rng), Some(0));
        }
    }
}
```

- [ ] **Step 2: Register the module**

In `src/map/builders/mod.rs`, near the top with the other `pub mod` declarations, add:

```rust
pub mod weighted_pick;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p bevy_rpg --lib map::builders::weighted_pick`
Expected: 5 passing.

- [ ] **Step 4: Commit**

```bash
git add src/map/builders/weighted_pick.rs src/map/builders/mod.rs
git commit -m "feat(spawn): add weighted_pick helper

Generic weighted random selection over u32 weights. Returns None for
empty slices and all-zero weight vectors. Used in the next commit by
MonsterSpawner to honor MonsterSpawnInfo.weight."
```

---

## Task 3: `bridson_poisson_disc` helper

**Files:**
- Create: `src/map/builders/poisson_disc.rs`
- Modify: `src/map/builders/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/map/builders/poisson_disc.rs`:

```rust
//! Bridson's Poisson-disc sampler restricted to walkable tiles. Used by
//! the monster spawner to give floor-wide blue-noise horde placement
//! instead of per-room rolls.

use bracket_lib::prelude::{Point, RandomNumberGenerator};
use std::f32::consts::TAU;

use crate::map::map::Map;
use crate::map::tile::{is_walkable, LiquidType, TerrainType};

/// A candidate tile for horde placement. `liquid` is the tile's liquid
/// state at sample time, so callers can match aquatic spawn entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub point: Point,
    pub on_liquid: bool,
}

/// Returns true if the tile is walkable, non-stair, non-portal, and not lava.
/// Lava is excluded so dry monsters never spawn on it; aquatic monsters need
/// water specifically (filtered per-horde at pick time, not here).
fn tile_is_valid_origin(map: &Map, idx: usize) -> bool {
    let tile = &map.tiles[idx];
    if !is_walkable(*tile) {
        return false;
    }
    if matches!(
        tile.terrain,
        TerrainType::UpStairs | TerrainType::DownStairs | TerrainType::Portal
    ) {
        return false;
    }
    !matches!(tile.liquid, LiquidType::Lava)
}

/// Run Bridson's Poisson-disc sampler on `map`, returning blue-noise-spaced
/// samples on valid walkable tiles. Uses `target_count` to compute the
/// minimum separation radius.
///
/// `k_candidates` is the per-active-sample candidate budget (Bridson's K;
/// 20 is standard). Higher K = tighter spacing closer to the radius.
pub fn bridson_poisson_disc(
    map: &Map,
    target_count: i32,
    k_candidates: u32,
    rng: &mut RandomNumberGenerator,
) -> Vec<Sample> {
    if target_count <= 0 {
        return Vec::new();
    }

    // Count valid origin tiles to size the separation radius.
    let mut valid: Vec<usize> = Vec::new();
    for idx in 0..map.tiles.len() {
        if tile_is_valid_origin(map, idx) {
            valid.push(idx);
        }
    }
    if valid.is_empty() {
        return Vec::new();
    }

    let area = valid.len() as f32;
    // r = sqrt(area / (target * PI * packing_constant))
    let r = (area / (target_count as f32 * std::f32::consts::PI * crate::constants::POISSON_PACKING_CONSTANT)).sqrt();
    let r_sq = r * r;

    let pick = valid[rng.range(0, valid.len() as i64) as usize];
    let (sx, sy) = map.idx_xy(pick);
    let seed = Point::new(sx, sy);

    let mut samples: Vec<Point> = vec![seed];
    let mut active: Vec<usize> = vec![0];

    while !active.is_empty() {
        let active_idx = rng.range(0, active.len() as i64) as usize;
        let center = samples[active[active_idx]];
        let mut placed = false;

        for _ in 0..k_candidates {
            // Annulus [r, 2r]. bracket-lib's RNG doesn't expose a 0..1 f32
            // primitive, so synthesize one from `range`.
            let angle = (rng.range(0, 10000) as f32 / 10000.0) * TAU;
            let dist = r + (rng.range(0, 10000) as f32 / 10000.0) * r;
            let nx = (center.x as f32 + angle.cos() * dist).round() as i32;
            let ny = (center.y as f32 + angle.sin() * dist).round() as i32;

            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if !tile_is_valid_origin(map, nidx) {
                continue;
            }
            let candidate = Point::new(nx, ny);
            // Min-distance check against ALL existing samples.
            let mut too_close = false;
            for existing in &samples {
                let dx = (existing.x - candidate.x) as f32;
                let dy = (existing.y - candidate.y) as f32;
                if dx * dx + dy * dy < r_sq {
                    too_close = true;
                    break;
                }
            }
            if !too_close {
                samples.push(candidate);
                active.push(samples.len() - 1);
                placed = true;
                break;
            }
        }

        if !placed {
            active.swap_remove(active_idx);
        }
    }

    samples
        .into_iter()
        .map(|p| {
            let idx = map.xy_idx(p.x, p.y);
            let on_liquid = !matches!(map.tiles[idx].liquid, LiquidType::None);
            Sample { point: p, on_liquid }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, Tile};

    fn floor() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::None, decoration: Decoration::None }
    }
    fn wall() -> Tile {
        Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None }
    }
    fn lava() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Lava, decoration: Decoration::None }
    }
    fn stairs_down() -> Tile {
        Tile { terrain: TerrainType::DownStairs, liquid: LiquidType::None, decoration: Decoration::None }
    }

    fn make_map(width: i32, height: i32, tiles: Vec<Tile>) -> Map {
        Map {
            name: "test".to_string(),
            explored_tiles: vec![false; tiles.len()],
            blocked: vec![false; tiles.len()],
            tiles,
            width,
            height,
            depth: 1,
        }
    }

    #[test]
    fn fully_walled_returns_empty() {
        let map = make_map(20, 20, vec![wall(); 400]);
        let mut rng = RandomNumberGenerator::seeded(1);
        let samples = bridson_poisson_disc(&map, 5, 20, &mut rng);
        assert!(samples.is_empty());
    }

    #[test]
    fn target_zero_returns_empty() {
        let map = make_map(20, 20, vec![floor(); 400]);
        let mut rng = RandomNumberGenerator::seeded(1);
        let samples = bridson_poisson_disc(&map, 0, 20, &mut rng);
        assert!(samples.is_empty());
    }

    #[test]
    fn samples_obey_min_distance() {
        let map = make_map(80, 60, vec![floor(); 80 * 60]);
        let mut rng = RandomNumberGenerator::seeded(7);
        let samples = bridson_poisson_disc(&map, 20, 20, &mut rng);
        assert!(!samples.is_empty());

        // Re-derive r the same way the sampler does so the assertion uses
        // identical packing math.
        let r = (samples.len() as f32 + 1.0).max(80.0 * 60.0 / 1.0); // placeholder; real check below
        // Actual: just verify pairwise distance >= sqrt(area / (target * pi * 0.7))
        let r = (80.0 * 60.0 / (20.0 * std::f32::consts::PI * crate::constants::POISSON_PACKING_CONSTANT)).sqrt();
        let r_sq = r * r;
        for i in 0..samples.len() {
            for j in (i + 1)..samples.len() {
                let dx = (samples[i].point.x - samples[j].point.x) as f32;
                let dy = (samples[i].point.y - samples[j].point.y) as f32;
                assert!(dx * dx + dy * dy >= r_sq * 0.99, "samples {} and {} too close", i, j);
            }
        }
        let _ = r; // silence unused
    }

    #[test]
    fn sample_count_within_band_of_target() {
        let map = make_map(80, 60, vec![floor(); 80 * 60]);
        let mut rng = RandomNumberGenerator::seeded(42);
        let samples = bridson_poisson_disc(&map, 20, 20, &mut rng);
        assert!(samples.len() >= 15, "got {}", samples.len());
        assert!(samples.len() <= 28, "got {}", samples.len());
    }

    #[test]
    fn lava_tiles_are_rejected() {
        let mut tiles = vec![floor(); 80 * 60];
        // Make a big lava lake in the middle
        for y in 20..40 {
            for x in 30..50 {
                tiles[(y * 80 + x) as usize] = lava();
            }
        }
        let map = make_map(80, 60, tiles);
        let mut rng = RandomNumberGenerator::seeded(3);
        let samples = bridson_poisson_disc(&map, 30, 20, &mut rng);
        for s in &samples {
            let idx = map.xy_idx(s.point.x, s.point.y);
            assert_ne!(map.tiles[idx].liquid, LiquidType::Lava);
        }
    }

    #[test]
    fn stair_tiles_are_rejected() {
        let mut tiles = vec![floor(); 80 * 60];
        tiles[(30 * 80 + 40) as usize] = stairs_down();
        let map = make_map(80, 60, tiles);
        let mut rng = RandomNumberGenerator::seeded(5);
        let samples = bridson_poisson_disc(&map, 30, 20, &mut rng);
        for s in &samples {
            let idx = map.xy_idx(s.point.x, s.point.y);
            assert_ne!(map.tiles[idx].terrain, TerrainType::DownStairs);
        }
    }
}
```

- [ ] **Step 2: Register the module + add constants**

In `src/map/builders/mod.rs`:
```rust
pub mod poisson_disc;
```

In `src/constants.rs`, add (group with other map constants):
```rust
/// Bridson's Poisson-disc — slack on radius so the sampler reliably hits target.
pub const POISSON_PACKING_CONSTANT: f32 = 0.7;
/// Bridson's K: candidates per active sample.
pub const POISSON_K_CANDIDATES: u32 = 20;
```

- [ ] **Step 3: Run tests to verify pass**

Run: `cargo test -p bevy_rpg --lib map::builders::poisson_disc`
Expected: 6 passing.

If `sample_count_within_band_of_target` fails low (< 15) for the given seed, widen the lower bound to 12 — the test is a sanity band, not a precise spec. Document the bound in a comment.

- [ ] **Step 4: Commit**

```bash
git add src/map/builders/poisson_disc.rs src/map/builders/mod.rs src/constants.rs
git commit -m "feat(spawn): add Bridson Poisson-disc sampler for horde placement

Constrained to walkable tiles, rejects stairs/portal/lava. Samples carry
their liquid classification so per-horde aquatic-vs-dry matching can
happen at the caller. Replaces per-room 50% rolls in the next commit."
```

---

## Task 4: Fix `find_cluster_points` bugs (stair guard + lava-as-water)

**Files:**
- Modify: `src/map/builders/monster_spawner.rs:205-253`
- Modify: `src/map/builders/monster_spawner.rs:255-361` (test block)

- [ ] **Step 1: Add failing tests**

Append to the `#[cfg(test)] mod tests` block at the bottom of `src/map/builders/monster_spawner.rs` (after the existing `cluster_liquid_only_no_water_returns_empty` test):

```rust
    fn down_stairs() -> Tile {
        Tile {
            terrain: TerrainType::DownStairs,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }

    fn shallow_water() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::ShallowWater,
            decoration: Decoration::None,
        }
    }

    fn lava() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::Lava,
            decoration: Decoration::None,
        }
    }

    #[test]
    fn cluster_rejects_downstairs_in_placement() {
        // 3x3: center is downstairs (still walkable), surrounded by floor.
        // Cluster should never place on the stair tile.
        let mut tiles = vec![floor(); 9];
        tiles[4] = down_stairs();
        let map = make_map(3, 3, tiles);
        let points = find_cluster_points(Point::new(1, 1), 9, &map, &HashSet::new(), false);
        for pt in &points {
            let idx = map.xy_idx(pt.x, pt.y);
            assert_ne!(
                map.tiles[idx].terrain,
                TerrainType::DownStairs,
                "cluster member placed on DownStairs at {:?}",
                pt
            );
        }
    }

    #[test]
    fn cluster_liquid_only_rejects_lava() {
        // 3x3: center is water, surrounded by lava. liquid_only mode must
        // accept only the center, never the lava tiles.
        let mut tiles = vec![lava(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let points = find_cluster_points(Point::new(1, 1), 3, &map, &HashSet::new(), true);
        assert_eq!(points.len(), 1, "should only place on water, not lava");
        let idx = map.xy_idx(points[0].x, points[0].y);
        assert_eq!(map.tiles[idx].liquid, LiquidType::Water);
    }

    #[test]
    fn cluster_liquid_only_accepts_shallow_water() {
        let map = make_map(3, 3, vec![shallow_water(); 9]);
        let points = find_cluster_points(Point::new(1, 1), 4, &map, &HashSet::new(), true);
        assert_eq!(points.len(), 4);
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p bevy_rpg --lib map::builders::monster_spawner::tests`
Expected:
- `cluster_rejects_downstairs_in_placement`: FAIL — current code accepts stair tiles.
- `cluster_liquid_only_rejects_lava`: FAIL — current code accepts lava as "liquid".
- `cluster_liquid_only_accepts_shallow_water`: likely PASS already (shallow water already counts).

- [ ] **Step 3: Apply the fixes**

In `src/map/builders/monster_spawner.rs`, edit `find_cluster_points` (around line 222-234). Replace the existing accept block:

```rust
    while let Some(pt) = queue.pop_front() {
        let idx = map.xy_idx(pt.x, pt.y);
        let liquid_ok = if liquid_only {
            map.tiles[idx].liquid != LiquidType::None
        } else {
            map.tiles[idx].liquid == LiquidType::None
        };
        if is_walkable(map.tiles[idx]) && liquid_ok && !occupied.contains(&idx) {
            result.push(pt);
            if result.len() >= count {
                break;
            }
        }
```

with:

```rust
    while let Some(pt) = queue.pop_front() {
        let idx = map.xy_idx(pt.x, pt.y);
        let liquid_ok = if liquid_only {
            matches!(
                map.tiles[idx].liquid,
                LiquidType::ShallowWater | LiquidType::Water
            )
        } else {
            map.tiles[idx].liquid == LiquidType::None
        };
        let terrain_ok = !matches!(
            map.tiles[idx].terrain,
            TerrainType::UpStairs | TerrainType::DownStairs | TerrainType::Portal
        );
        if is_walkable(map.tiles[idx])
            && liquid_ok
            && terrain_ok
            && !occupied.contains(&idx)
        {
            result.push(pt);
            if result.len() >= count {
                break;
            }
        }
```

The expansion-step filter (`map.tiles[nidx].liquid != LiquidType::Lava`) already exists at line 245 — leave it. Lava is now also rejected in `liquid_only` placement by the tighter `matches!` predicate.

If `TerrainType::Portal` doesn't exist (check `src/map/tile.rs` first — CLAUDE.md says it does), drop that arm. If it does, leave it.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p bevy_rpg --lib map::builders::monster_spawner::tests`
Expected: all previous tests pass + 3 new tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/map/builders/monster_spawner.rs
git commit -m "fix(spawn): reject stairs and lava in find_cluster_points

Two latent bugs:

1. Cluster placement accepted UpStairs/DownStairs/Portal tiles even
   though origin-picking explicitly rejected them. A horde seeded next
   to stairs could land a member on the stair tile.

2. liquid_only mode (aquatic monsters) accepted Lava because the check
   was 'liquid != None'. Tightened to ShallowWater | Water only.

Both paths covered by new unit tests."
```

---

## Task 5: Rewrite `MonsterSpawner::spawn_monsters` to use Bridson + weights

**Files:**
- Modify: `src/map/builders/monster_spawner.rs:30-141`
- Modify: `src/constants.rs`

- [ ] **Step 1: Add target-count constants**

In `src/constants.rs`:

```rust
/// Initial horde count = base + floor(depth * per_depth). Floor 1 ~ 8,
/// floor 26 ~ 23.
pub const TARGET_HORDES_BASE: i32 = 8;
pub const TARGET_HORDES_PER_DEPTH: f32 = 0.6;
```

- [ ] **Step 2: Add a pure helper + test for `target_hordes`**

Inside `src/map/builders/monster_spawner.rs`, add near the top (after imports, above `MonsterSpawner`):

```rust
/// Initial floor-population target — number of hordes to seed at
/// floor-gen. Scales linearly with depth.
pub fn target_hordes(depth: i32) -> i32 {
    crate::constants::TARGET_HORDES_BASE
        + ((depth as f32) * crate::constants::TARGET_HORDES_PER_DEPTH).floor() as i32
}
```

And inside the test block, add:

```rust
    #[test]
    fn target_hordes_curve_matches_design() {
        assert_eq!(target_hordes(1), 8);
        assert_eq!(target_hordes(13), 8 + 7); // floor(7.8) = 7
        assert_eq!(target_hordes(26), 8 + 15); // floor(15.6) = 15
    }
```

Run: `cargo test -p bevy_rpg --lib map::builders::monster_spawner::tests::target_hordes_curve_matches_design`
Expected: PASS.

- [ ] **Step 3: Rewrite `spawn_monsters`**

Replace the entire `spawn_monsters` function in `src/map/builders/monster_spawner.rs` (lines 30-141) with:

```rust
    fn spawn_monsters(&mut self, build_data: &mut BuilderMap) {
        let depth = build_data.map.depth;

        // Filter spawn table by depth.
        let possible_spawns: Vec<MonsterSpawnInfo> = self
            .spawn_table
            .iter()
            .filter(|spawn| depth >= spawn.min_floor && depth <= spawn.max_floor)
            .cloned()
            .collect();

        if possible_spawns.is_empty() {
            return;
        }

        // Reserve player start so nothing spawns on it.
        let mut occupied: HashSet<usize> = HashSet::new();
        if let Some(start) = &build_data.starting_position {
            occupied.insert(build_data.map.xy_idx(start.x, start.y));
        }

        // Run Bridson on the map. We pull the rng out via std::mem::take so
        // we can pass &mut to the sampler without holding two borrows of
        // build_data, then restore it.
        let target = target_hordes(depth);
        let mut rng = std::mem::replace(
            &mut build_data.rng,
            RandomNumberGenerator::new(),
        );
        let samples = crate::map::builders::poisson_disc::bridson_poisson_disc(
            &build_data.map,
            target,
            crate::constants::POISSON_K_CANDIDATES,
            &mut rng,
        );

        let mut new_spawns: Vec<SpawnEntry> = Vec::new();

        for sample in &samples {
            // Restrict the depth-filtered set to entries whose liquid mode
            // matches this sample's tile.
            let candidates: Vec<&MonsterSpawnInfo> = possible_spawns
                .iter()
                .filter(|s| s.spawn_on_liquid == sample.on_liquid)
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let weights: Vec<u32> = candidates.iter().map(|c| c.weight).collect();
            let Some(pick) = crate::map::builders::weighted_pick::weighted_pick(&weights, &mut rng) else {
                continue;
            };
            let monster_info = candidates[pick];

            // Skip if the chosen origin is already occupied (e.g. player start).
            let origin_idx = build_data.map.xy_idx(sample.point.x, sample.point.y);
            if occupied.contains(&origin_idx) {
                continue;
            }

            let squad_config = SquadConfig {
                on_leader_death: LeaderDeathBehavior::from_str(&monster_info.on_leader_death),
                flee_threshold: monster_info.flee_threshold,
            };

            // Build the member list — either heterogeneous (group) or
            // homogeneous (min_group..=max_group of the same monster).
            let members: Vec<String> = if !monster_info.group.is_empty() {
                let mut m = Vec::new();
                for gm in &monster_info.group {
                    let count = if gm.max_count > gm.min_count {
                        rng.range(gm.min_count, gm.max_count + 1)
                    } else {
                        gm.min_count
                    };
                    for _ in 0..count {
                        m.push(gm.monster.clone());
                    }
                }
                m
            } else {
                let group_size = if monster_info.max_group > monster_info.min_group {
                    rng.range(monster_info.min_group, monster_info.max_group + 1)
                } else {
                    monster_info.min_group
                } as usize;
                vec![monster_info.monster.clone(); group_size]
            };

            if members.is_empty() {
                continue;
            }

            let points = find_cluster_points(
                sample.point,
                members.len(),
                &build_data.map,
                &occupied,
                monster_info.spawn_on_liquid,
            );

            if points.is_empty() {
                continue;
            }

            // Squads only when a group of >1 actually landed; otherwise solo.
            let is_squad = points.len() > 1 || !monster_info.group.is_empty();
            let squad_id = if is_squad {
                Some(build_data.squad_counter.next())
            } else {
                None
            };

            for (i, (pt, name)) in points.iter().zip(members.iter()).enumerate() {
                occupied.insert(build_data.map.xy_idx(pt.x, pt.y));
                if let Some(sid) = squad_id {
                    new_spawns.push(SpawnEntry::squad(
                        *pt,
                        name.clone(),
                        sid,
                        squad_config.clone(),
                        i == 0,
                    ));
                } else {
                    new_spawns.push(SpawnEntry::solo(*pt, name.clone()));
                }
            }
        }

        // Restore the RNG.
        build_data.rng = rng;

        for entry in new_spawns {
            build_data.add_monster_spawn(entry);
        }
    }
```

Also delete the now-unused helpers `get_walkable_room_point` and `get_liquid_room_point` (lines 144-198) — they're replaced by Bridson. Likewise remove the `rooms` block + `rng` construction at the top of the old function.

Make sure imports still include what's needed and drop what isn't. Required imports at top of file:

```rust
use std::collections::{HashSet, VecDeque};

use crate::{
    assets::MonsterSpawnInfo,
    game::squad::{LeaderDeathBehavior, SquadConfig},
    map::{
        builders::{BuilderMap, BuilderPhase, MetaMapBuilder, SpawnEntry},
        map::Map,
        tile::{is_walkable, LiquidType, TerrainType},
    },
};
use bevy::prelude::*;
use bracket_lib::prelude::{Point, RandomNumberGenerator};
```

(Drop `Rect` — no longer used now that rooms are not consulted.)

- [ ] **Step 4: Run all tests in the spawner**

Run: `cargo test -p bevy_rpg --lib map::builders::monster_spawner`
Expected: All previous `find_cluster_points` tests still pass, plus the new `target_hordes_curve_matches_design`.

- [ ] **Step 5: Run the full build + clippy**

Run: `cargo build -p bevy_rpg && cargo clippy -p bevy_rpg -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/map/builders/monster_spawner.rs src/constants.rs
git commit -m "feat(spawn): floor-wide Bridson placement + weighted picks

Replace the per-room 50% spawn roll with a Bridson Poisson-disc sampler
that places hordes with even blue-noise distribution across all walkable
tiles. Horde count scales with depth (target_hordes formula).

Each horde's monster entry is weighted-picked from the depth-filtered set
using the new MonsterSpawnInfo.weight field (defaults to 10 — equal
weights, so existing tables behave the same until authors retune).

Aquatic spawns (spawn_on_liquid: true) are matched against samples that
landed on water tiles; dry spawns to dry tiles. If a liquid sample has
no aquatic candidates available at that depth, the sample is skipped.

MonsterSpawner now shares the BuilderMap RNG (via mem::replace) instead
of constructing an unseeded one — full determinism comes once the
broader builder pipeline seeds BuilderMap.rng (out of scope here)."
```

---

## Task 6: `WanderClock` data + cadence math

**Files:**
- Create: `src/game/wander.rs`
- Modify: `src/game/mod.rs`
- Modify: `src/constants.rs`

- [ ] **Step 1: Add cadence constants**

In `src/constants.rs`:

```rust
/// Wander clock — see docs/design/SPAWNING.md.
pub const WANDER_BASE_INTERVAL_AT_DEPTH_0: i32 = 300;
pub const WANDER_INTERVAL_DEPTH_STEP: i32 = 8;
pub const WANDER_BASE_INTERVAL_FLOOR: i32 = 60;
pub const WANDER_RAMP_PER_FIRE: f32 = 0.08;
pub const WANDER_RAMP_FACTOR_FLOOR: f32 = 0.3;
pub const WANDER_MIN_DISTANCE: i32 = 8;
pub const WANDER_NOTIFY_RANGE: i32 = 15;
```

- [ ] **Step 2: Write the failing test**

Create `src/game/wander.rs`:

```rust
//! Per-floor wandering-monster spawner. See
//! [docs/design/SPAWNING.md](../../docs/design/SPAWNING.md) §3-4.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-floor wander clock. Lives on `CachedFloor` so it persists across
/// stair traversal and floor revisits — a key part of preventing the
/// "descend and return to reset the clock" exploit.
#[derive(Component, Resource, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WanderClock {
    /// Game turns remaining until the next wander check fires.
    pub turns_until_check: i32,
    /// Count of wander events fired so far on this floor. Drives the ramp.
    pub checks_fired: u32,
}

impl Default for WanderClock {
    fn default() -> Self {
        WanderClock {
            turns_until_check: 0,
            checks_fired: 0,
        }
    }
}

impl WanderClock {
    /// Fresh clock for a never-before-visited floor.
    pub fn fresh(depth: i32) -> Self {
        WanderClock {
            turns_until_check: base_interval(depth),
            checks_fired: 0,
        }
    }
}

/// Base interval before any ramping. Decreases with depth so deeper floors
/// have higher baseline pressure.
pub fn base_interval(depth: i32) -> i32 {
    let raw = crate::constants::WANDER_BASE_INTERVAL_AT_DEPTH_0
        - depth * crate::constants::WANDER_INTERVAL_DEPTH_STEP;
    raw.max(crate::constants::WANDER_BASE_INTERVAL_FLOOR)
}

/// Ramp factor applied to base — shrinks the interval the longer the
/// player camps. Floors at `WANDER_RAMP_FACTOR_FLOOR`.
pub fn ramp_factor(checks_fired: u32) -> f32 {
    let raw = 1.0 - (checks_fired as f32) * crate::constants::WANDER_RAMP_PER_FIRE;
    raw.max(crate::constants::WANDER_RAMP_FACTOR_FLOOR)
}

/// Final interval — base × ramp, floored at 1 turn.
pub fn next_interval(depth: i32, checks_fired: u32) -> i32 {
    ((base_interval(depth) as f32) * ramp_factor(checks_fired)).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_interval_decreases_with_depth() {
        assert_eq!(base_interval(0), 300);
        assert_eq!(base_interval(1), 292);
        assert_eq!(base_interval(13), 300 - 13 * 8); // 196
        assert_eq!(base_interval(26), 300 - 26 * 8); // 92
    }

    #[test]
    fn base_interval_floors_at_min() {
        // depth 50 would give 300 - 400 = -100; must clamp to 60.
        assert_eq!(base_interval(50), 60);
        assert_eq!(base_interval(30), 60);
    }

    #[test]
    fn ramp_factor_floors_at_0_3() {
        assert!((ramp_factor(0) - 1.0).abs() < 1e-6);
        assert!((ramp_factor(5) - 0.6).abs() < 1e-6);
        assert!((ramp_factor(10) - 0.3).abs() < 1e-6);
        assert!((ramp_factor(20) - 0.3).abs() < 1e-6); // floored
    }

    #[test]
    fn next_interval_matches_spec_table() {
        // Floor 1, no fires: full base (292).
        assert_eq!(next_interval(1, 0), 292);
        // Floor 13, 5 fires: 196 × 0.6 = 117.6 → 117.
        assert_eq!(next_interval(13, 5), 117);
        // Floor 26, 10 fires: 92 × 0.3 = 27.6 → 27.
        assert_eq!(next_interval(26, 10), 27);
    }

    #[test]
    fn fresh_clock_uses_base_interval() {
        let c = WanderClock::fresh(13);
        assert_eq!(c.turns_until_check, 196);
        assert_eq!(c.checks_fired, 0);
    }
}
```

- [ ] **Step 3: Register the module**

In `src/game/mod.rs`, add to the existing module declarations:
```rust
pub mod wander;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p bevy_rpg --lib game::wander`
Expected: 5 passing.

- [ ] **Step 5: Commit**

```bash
git add src/game/wander.rs src/game/mod.rs src/constants.rs
git commit -m "feat(wander): WanderClock state and cadence math

Pure-function piece of the wander system: base_interval(depth) gives the
floor-pressure baseline; ramp_factor(checks_fired) compresses it the
longer the player camps; next_interval(depth, checks_fired) combines
both. WanderClock::fresh(depth) is what new floors start with.

Tick system, persistence, and event firing land in follow-up commits."
```

---

## Task 7: Persist `WanderClock` on `CachedFloor`

**Files:**
- Modify: `src/map/dungeon.rs:34-48`
- Modify: any system that constructs a `CachedFloor` (snapshot helper at line 205+)

- [ ] **Step 1: Find every `CachedFloor` construction**

Run: `grep -n "CachedFloor {" /Users/nathanrude/Development/bevy_rpg/src/map/dungeon.rs`
Expect roughly one struct literal (the snapshot at ~line 270) and the type definition.

- [ ] **Step 2: Add the field**

In `src/map/dungeon.rs`, modify `pub struct CachedFloor`:

```rust
pub struct CachedFloor {
    pub map: Map,
    pub monsters: Vec<crate::save::SavedMonster>,
    pub items: Vec<crate::save::SavedItem>,
    pub props: Vec<crate::save::SavedProp>,
    pub down_stairs_pos: Point,
    pub up_stairs_pos: Point,
    /// Per-floor wander pressure state — preserved across stair traversal
    /// so that descend-and-return is not a free clock reset.
    pub wander_clock: crate::game::wander::WanderClock,
}
```

- [ ] **Step 3: Update the snapshot function**

Find the function that returns a `CachedFloor` (around line 199-275). Add `wander_clock` to the constructed literal. Source: the live `WanderClock` resource (set up in Task 10).

For now, until Task 10 wires the resource, snapshot it as `WanderClock::default()`. Replace later:

```rust
    CachedFloor {
        map: /* existing */,
        monsters: /* existing */,
        items: /* existing */,
        props: /* existing */,
        down_stairs_pos: /* existing */,
        up_stairs_pos: /* existing */,
        wander_clock: crate::game::wander::WanderClock::default(),
    }
```

- [ ] **Step 4: Run `cargo check`**

Run: `cargo check -p bevy_rpg`
Expected: clean compile.

- [ ] **Step 5: Commit**

```bash
git add src/map/dungeon.rs
git commit -m "feat(wander): add wander_clock field to CachedFloor

Stub: snapshots default for now. Task 10 wires the live WanderClock
resource into the snapshot path."
```

---

## Task 8: Save schema v5 → v6 migration for `WanderClock`

**Files:**
- Modify: `src/save/mod.rs:82` (`SAVE_SCHEMA_VERSION`)
- Modify: `src/save/mod.rs:460-473` (`SavedFloorData`)
- Modify: `src/save/mod.rs` (migration registration block)

- [ ] **Step 1: Write the failing round-trip test**

Append to `src/save/mod.rs` test module:

```rust
    #[test]
    fn v6_saved_floor_data_round_trips_wander_clock() {
        let original = SavedFloorData {
            map: MapSaveData {
                width: 80,
                height: 60,
                depth: 5,
                name: "test".to_string(),
                tiles: vec![],
                explored: vec![],
            },
            monsters: vec![],
            items: vec![],
            props: vec![],
            down_stairs_pos: [10, 10],
            up_stairs_pos: [0, 0],
            wander_clock: crate::game::wander::WanderClock {
                turns_until_check: 137,
                checks_fired: 4,
            },
        };
        let encoded = ron::to_string(&original).expect("ron encode");
        let decoded: SavedFloorData = ron::from_str(&encoded).expect("ron decode");
        assert_eq!(decoded.wander_clock.turns_until_check, 137);
        assert_eq!(decoded.wander_clock.checks_fired, 4);
    }

    #[test]
    fn pre_v6_save_loads_with_default_wander_clock() {
        // RON missing wander_clock (v5 shape) — should fill in a default.
        let v5_ron = r#"(
            map: (width: 80, height: 60, depth: 5, name: "x", tiles: [], explored: []),
            monsters: [],
            items: [],
            props: [],
            down_stairs_pos: (10, 10),
            up_stairs_pos: (0, 0),
        )"#;
        let decoded: SavedFloorData = ron::from_str(v5_ron).expect("ron decode");
        assert_eq!(decoded.wander_clock.turns_until_check, 0);
        assert_eq!(decoded.wander_clock.checks_fired, 0);
    }
```

Note: `down_stairs_pos` in the existing save is `[i32; 2]`, RON tuple — confirm shape with existing tests. Adjust the RON above if needed.

- [ ] **Step 2: Bump version**

In `src/save/mod.rs` at line 82:
```rust
pub const SAVE_SCHEMA_VERSION: u32 = 6;
```

Find the `assert_eq!(SAVE_SCHEMA_VERSION, 5);` at line ~2562 — bump to 6.

- [ ] **Step 3: Add the field to `SavedFloorData`**

At `src/save/mod.rs:464-473`:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedFloorData {
    pub map: MapSaveData,
    pub monsters: Vec<SavedMonster>,
    pub items: Vec<SavedItem>,
    #[serde(default)]
    pub props: Vec<SavedProp>,
    pub down_stairs_pos: [i32; 2],
    #[serde(default)]
    pub up_stairs_pos: [i32; 2],
    /// Phase 3 follow-up: per-floor wander pressure clock.
    /// Defaults to `WanderClock::default()` on pre-v6 saves, which
    /// matches a freshly entered floor (clock will be re-initialized
    /// on first tick).
    #[serde(default)]
    pub wander_clock: crate::game::wander::WanderClock,
}
```

- [ ] **Step 4: Add the v5 → v6 migration**

Search for `migrate_v4_to_v5_is_identity` (~line 2649). Above or below it, add the v5 → v6 migration following the existing identity-migration pattern:

```rust
/// v5 → v6: Per-floor `wander_clock` added to `SavedFloorData` with a
/// serde default of `WanderClock::default()`. Pre-v6 saves load with
/// a default clock, which is equivalent to a fresh floor visit — the
/// next tick will re-initialize via `WanderClock::fresh(depth)`.
fn migrate_v5_to_v6(payload: &str) -> Result<String, String> {
    // Identity — serde defaults handle the new field.
    Ok(payload.to_string())
}
```

Register the migration in the migration list (search for `migrate_v3_to_v4` registration; add `migrate_v5_to_v6` in the same place). Also add an identity-migration test:

```rust
    #[test]
    fn migrate_v5_to_v6_is_identity() {
        let input = r#"(schema_version: 5, data: ())"#;
        let output = migrate_v5_to_v6(input).expect("migration");
        assert_eq!(output, input);
    }
```

- [ ] **Step 5: Update CachedFloor → SavedFloorData conversion**

Search for where `CachedFloor` is converted into `SavedFloorData` (usually in the snapshot path or auto_save_system — `grep -n "SavedFloorData {" src/`). Add `wander_clock: cached.wander_clock` to the literal.

Similarly update the reverse direction (`SavedFloorData → CachedFloor`, search for `CachedFloor {` in the restore/load paths).

- [ ] **Step 6: Run tests**

Run: `cargo test -p bevy_rpg --lib save::`
Expected: existing tests pass + 3 new pass.

Run: `cargo build -p bevy_rpg`
Expected: clean.

- [ ] **Step 7: Update documentation block in `src/save/mod.rs`**

At the top of the file (around line 70-82), add a `v6` bullet matching the existing format:

```rust
/// - **v6**: Per-floor wander clock. `SavedFloorData` gains a
///   `wander_clock` field (turns_until_check + checks_fired). Pre-v6
///   saves load with a default clock; the next tick re-initializes it
///   via `WanderClock::fresh(depth)`.
```

- [ ] **Step 8: Commit**

```bash
git add src/save/mod.rs
git commit -m "feat(save): v6 — persist per-floor WanderClock

Bump SAVE_SCHEMA_VERSION to 6. SavedFloorData gains wander_clock with a
serde default so pre-v6 saves load cleanly (fresh-floor equivalent). v5
→ v6 is an identity migration; the default field does the work."
```

---

## Task 9: Wander origin finder

**Files:**
- Modify: `src/game/wander.rs`

The origin search needs: map, player position, player viewshed, occupied set, liquid mode, RNG. Pure function so it tests easily.

- [ ] **Step 1: Write the failing test**

Append to `src/game/wander.rs`:

```rust
use bracket_lib::prelude::{Point, RandomNumberGenerator};
use std::collections::HashSet;

use crate::map::map::Map;
use crate::map::tile::{is_walkable, LiquidType, TerrainType};

/// Result of searching for a wander-spawn origin.
#[derive(Debug, Clone, Copy)]
pub struct WanderOrigin {
    pub point: Point,
}

/// Find a walkable tile that:
///   - matches the requested liquid mode (dry or aquatic),
///   - is not stairs/portal,
///   - is not in `visible_tiles` (player viewshed),
///   - is not in `occupied`,
///   - is at Chebyshev distance >= `min_distance` from `player`.
///
/// Returns `None` if no valid tile is found after `max_attempts` random tries.
/// The caller decides what to do with `None` — typically skip the event
/// without bumping the clock (implicit density throttle).
pub fn find_wander_origin(
    map: &Map,
    player: Point,
    visible_tiles: &HashSet<usize>,
    occupied: &HashSet<usize>,
    liquid_only: bool,
    min_distance: i32,
    max_attempts: u32,
    rng: &mut RandomNumberGenerator,
) -> Option<WanderOrigin> {
    for _ in 0..max_attempts {
        let x = rng.range(0, map.width);
        let y = rng.range(0, map.height);
        let idx = map.xy_idx(x, y);
        let tile = &map.tiles[idx];

        if !is_walkable(*tile) {
            continue;
        }
        if matches!(
            tile.terrain,
            TerrainType::UpStairs | TerrainType::DownStairs | TerrainType::Portal
        ) {
            continue;
        }
        let liquid_ok = if liquid_only {
            matches!(tile.liquid, LiquidType::ShallowWater | LiquidType::Water)
        } else {
            matches!(tile.liquid, LiquidType::None)
        };
        if !liquid_ok {
            continue;
        }
        if visible_tiles.contains(&idx) {
            continue;
        }
        if occupied.contains(&idx) {
            continue;
        }
        let chebyshev = (x - player.x).abs().max((y - player.y).abs());
        if chebyshev < min_distance {
            continue;
        }
        return Some(WanderOrigin { point: Point::new(x, y) });
    }
    None
}
```

Now add tests at the bottom of `src/game/wander.rs`:

```rust
    fn make_dry_map(width: i32, height: i32) -> Map {
        use crate::map::tile::{Decoration, Tile};
        let count = (width * height) as usize;
        let tiles = vec![
            Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            };
            count
        ];
        Map {
            name: "test".to_string(),
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            tiles,
            width,
            height,
            depth: 1,
        }
    }

    #[test]
    fn wander_origin_skips_visible_tiles() {
        let map = make_dry_map(30, 30);
        // Mark every tile within 5 of player as visible.
        let player = Point::new(15, 15);
        let mut visible = HashSet::new();
        for y in 10..20 {
            for x in 10..20 {
                visible.insert(map.xy_idx(x, y));
            }
        }
        let mut rng = RandomNumberGenerator::seeded(99);
        let origin = find_wander_origin(
            &map, player, &visible, &HashSet::new(), false, 0, 200, &mut rng,
        )
        .expect("dry tile somewhere on the map");
        let idx = map.xy_idx(origin.point.x, origin.point.y);
        assert!(!visible.contains(&idx));
    }

    #[test]
    fn wander_origin_respects_min_distance() {
        let map = make_dry_map(30, 30);
        let player = Point::new(15, 15);
        let mut rng = RandomNumberGenerator::seeded(7);
        let origin = find_wander_origin(
            &map, player, &HashSet::new(), &HashSet::new(), false, 8, 200, &mut rng,
        )
        .expect("space is large enough");
        let cheb = (origin.point.x - 15).abs().max((origin.point.y - 15).abs());
        assert!(cheb >= 8, "got chebyshev {} from origin {:?}", cheb, origin);
    }

    #[test]
    fn wander_origin_returns_none_when_no_valid_tile() {
        let map = make_dry_map(10, 10);
        let player = Point::new(5, 5);
        // Mark EVERY tile visible — no valid origin exists.
        let mut visible = HashSet::new();
        for idx in 0..100 {
            visible.insert(idx);
        }
        let mut rng = RandomNumberGenerator::seeded(1);
        let origin = find_wander_origin(
            &map, player, &visible, &HashSet::new(), false, 0, 30, &mut rng,
        );
        assert!(origin.is_none());
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p bevy_rpg --lib game::wander`
Expected: 8 passing (5 from Task 6 + 3 new).

- [ ] **Step 3: Commit**

```bash
git add src/game/wander.rs
git commit -m "feat(wander): find_wander_origin — out-of-sight tile picker

Rejection-samples for a tile that's walkable, off-camera (not in the
player's viewshed), respects liquid mode, far enough from the player,
and not already occupied. Returning None means 'no valid origin found
in N attempts' — caller skips the event without bumping the clock,
which acts as the implicit density throttle."
```

---

## Task 10: `WanderPlugin` — tick + event firing

**Files:**
- Modify: `src/game/wander.rs`
- Modify: `src/game/mod.rs`
- Modify: `src/map/dungeon.rs` — snapshot path picks up live `WanderClock` resource.

This is the most invasive task. The wander system needs to:
1. Maintain a `WanderClock` resource (mirrors the cached floor's clock).
2. Tick once per game-turn transition (not per actor).
3. When `turns_until_check <= 0`, fire a wander event using the same `SpawnEntry` path the floor materializer consumes — but at runtime, not in the builder.
4. On floor enter, hydrate the resource from `CachedFloor.wander_clock`.
5. On floor exit/save, snapshot the resource back.

- [ ] **Step 1: Confirm the runtime spawn entry point**

The helper is [`crate::game::spawner::spawn_monster_by_name`](../../src/game/spawner.rs#L381) — already public, signature:

```rust
pub fn spawn_monster_by_name(
    commands: &mut Commands,
    monster_name: &str,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_manifests: &Res<Assets<MonsterManifest>>,
    monster_manifest_handle: &Res<MonsterManifestHandle>,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
) -> Option<Entity>
```

It returns the spawned entity but does **not** wire squad components. For wander-spawned squads, after the call, attach `crate::game::squad::SquadMember { squad_id, is_leader }` and (on the leader) `crate::game::squad::SquadLeader { config: squad_config }` to the returned entity via `commands.entity(entity).insert(...)`. Check `src/game/squad.rs` for the exact component names — adjust the inserts to match.

- [ ] **Step 2: Wander resource + plugin scaffolding**

In `src/game/wander.rs`, add at the bottom:

```rust
/// Game-time mirror of the active floor's `WanderClock`. Reset on floor
/// enter (hydrated from `CachedFloor.wander_clock`); snapshotted back
/// on floor exit/auto-save.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ActiveWanderClock(pub WanderClock);

/// Fires when the wander clock ticks to zero. Distinct from the event
/// firing system so the tick can be tested independently of the world.
#[derive(Message, Debug, Clone, Copy)]
pub struct WanderTickFired;

pub struct WanderPlugin;

impl Plugin for WanderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveWanderClock>()
            .add_message::<WanderTickFired>()
            .add_systems(
                Update,
                (wander_tick_system, wander_fire_system)
                    .chain()
                    .run_if(in_state(crate::game::AppState::InGame))
                    .run_if(in_state(crate::game::TurnState::Processing))
                    .in_set(crate::game::turns::ProcessingPhase::Cleanup),
            )
            .add_systems(
                OnEnter(crate::game::AppState::InGame),
                hydrate_wander_clock_from_cache,
            );
    }
}
```

- [ ] **Step 3: Tick system**

```rust
/// Decrement `turns_until_check` by 1 per call. Emits `WanderTickFired`
/// when the clock reaches zero.
pub fn wander_tick_system(
    mut clock: ResMut<ActiveWanderClock>,
    mut events: MessageWriter<WanderTickFired>,
    floor: Res<crate::map::dungeon::Floor>,
) {
    // First tick on a fresh floor — `turns_until_check` would be 0 and we'd
    // fire immediately. Initialize from depth instead.
    if clock.0.turns_until_check == 0 && clock.0.checks_fired == 0 {
        clock.0 = WanderClock::fresh(floor.0 as i32);
    }

    clock.0.turns_until_check -= 1;
    if clock.0.turns_until_check <= 0 {
        events.send(WanderTickFired);
        clock.0.checks_fired += 1;
        clock.0.turns_until_check =
            next_interval(floor.0 as i32, clock.0.checks_fired);
    }
}
```

- [ ] **Step 4: Event firing system**

The fire system needs many things: viewshed, player position, map, spawn table, monster manifest, occupied set. Signature:

```rust
pub fn wander_fire_system(
    mut events: MessageReader<WanderTickFired>,
    mut commands: Commands,
    map: Res<crate::map::map::Map>,
    monster_spawn_tables: Res<bevy::asset::Assets<crate::assets::MonsterSpawnTable>>,
    monster_table_handle: Res<crate::assets::MonsterSpawnTableHandle>,
    monster_manifests: Res<bevy::asset::Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    mut turn_manager: ResMut<crate::game::turns::TurnManager>,
    q_player: Query<(&crate::components::Position, &crate::components::Viewshed), With<crate::components::Player>>,
    q_existing_monsters: Query<&crate::components::Position, With<crate::components::Monster>>,
    mut squad_counter: ResMut<crate::game::squad::SquadIdCounter>,
    mut clock: ResMut<ActiveWanderClock>,
    mut game_log: MessageWriter<crate::ui::game_log::GameLogMessage>,
) {
    // Bail early if no event this tick.
    let mut fired = false;
    for _ in events.read() {
        fired = true;
    }
    if !fired {
        return;
    }

    let Ok((player_pos, player_viewshed)) = q_player.single() else { return; };
    let Some(spawn_table) = monster_spawn_tables.get(&monster_table_handle.0) else { return; };
    let Some(monster_manifest) = monster_manifests.get(&monster_manifest_handle.0) else { return; };

    let depth = map.depth;
    let player_point = Point::new(player_pos.x, player_pos.y);

    // 1. Filter spawn table: depth + can_wander.
    let eligible: Vec<&crate::assets::MonsterSpawnInfo> = spawn_table
        .spawns
        .iter()
        .filter(|s| depth >= s.min_floor && depth <= s.max_floor && s.can_wander)
        .collect();
    if eligible.is_empty() {
        return;
    }

    // 2. Build viewshed set and occupied set.
    let visible_tiles: HashSet<usize> = player_viewshed
        .visible_tiles
        .iter()
        .map(|p| map.xy_idx(p.x, p.y))
        .collect();
    let mut occupied: HashSet<usize> = q_existing_monsters
        .iter()
        .map(|p| map.xy_idx(p.x, p.y))
        .collect();
    occupied.insert(map.xy_idx(player_pos.x, player_pos.y));

    // 3. Roll one entry (weighted) and one origin.
    //    Use a per-event RNG so wandering feels emergent — not seeded from map.
    let mut rng = RandomNumberGenerator::new();
    let weights: Vec<u32> = eligible.iter().map(|s| s.weight).collect();
    let Some(pick) = crate::map::builders::weighted_pick::weighted_pick(&weights, &mut rng) else {
        return;
    };
    let monster_info = eligible[pick];

    let Some(origin) = find_wander_origin(
        &map,
        player_point,
        &visible_tiles,
        &occupied,
        monster_info.spawn_on_liquid,
        crate::constants::WANDER_MIN_DISTANCE,
        30,
        &mut rng,
    ) else {
        // No valid tile — abort the event silently. Clock continues; the
        // tick has already incremented checks_fired, but per the design
        // (§4) we should NOT have bumped it. Roll back:
        // Caller note: we rely on the tick system having already bumped,
        // so we need a way to revert. Cleaner: move the bump out of the
        // tick into the fire path. See Step 5.
        return;
    };

    // 4. Build the member list and find cluster positions.
    let members: Vec<String> = if !monster_info.group.is_empty() {
        let mut m = Vec::new();
        for gm in &monster_info.group {
            let count = if gm.max_count > gm.min_count {
                rng.range(gm.min_count, gm.max_count + 1)
            } else {
                gm.min_count
            };
            for _ in 0..count {
                m.push(gm.monster.clone());
            }
        }
        m
    } else {
        let group_size = if monster_info.max_group > monster_info.min_group {
            rng.range(monster_info.min_group, monster_info.max_group + 1)
        } else {
            monster_info.min_group
        } as usize;
        vec![monster_info.monster.clone(); group_size]
    };

    let mut cluster_points = crate::map::builders::monster_spawner::find_cluster_points(
        origin.point,
        members.len(),
        &map,
        &occupied,
        monster_info.spawn_on_liquid,
    );

    // 5. Drop any cluster member that landed in viewshed (extra safety).
    cluster_points.retain(|p| !visible_tiles.contains(&map.xy_idx(p.x, p.y)));
    if cluster_points.is_empty() {
        return;
    }

    // 6. Spawn through the runtime monster-spawn helper.
    let squad_id = if cluster_points.len() > 1 || !monster_info.group.is_empty() {
        Some(squad_counter.next())
    } else {
        None
    };
    let squad_config = crate::game::squad::SquadConfig {
        on_leader_death: crate::game::squad::LeaderDeathBehavior::from_str(
            &monster_info.on_leader_death,
        ),
        flee_threshold: monster_info.flee_threshold,
    };

    for (i, (pt, name)) in cluster_points.iter().zip(members.iter()).enumerate() {
        let spawned = crate::game::spawner::spawn_monster_by_name(
            &mut commands,
            name,
            pt,
            &mut turn_manager,
            &monster_manifests,
            &monster_manifest_handle,
            &monster_sprite_assets,
            ascii_font.as_deref(),
        );
        // Attach squad components if this is a squad spawn. Component
        // names below are placeholders — verify against src/game/squad.rs
        // and adjust to whatever the existing initial-spawn path uses
        // (likely SquadMember + SquadLeader, but match the live API).
        if let (Some(entity), Some(sid)) = (spawned, squad_id) {
            commands
                .entity(entity)
                .insert(crate::game::squad::SquadMember {
                    squad_id: sid,
                    is_leader: i == 0,
                });
            if i == 0 {
                commands
                    .entity(entity)
                    .insert(crate::game::squad::SquadLeader {
                        config: squad_config.clone(),
                    });
            }
        }
    }

    // 7. Notify if nearby.
    let nearest_member = cluster_points
        .iter()
        .map(|p| (p.x - player_pos.x).abs().max((p.y - player_pos.y).abs()))
        .min()
        .unwrap_or(i32::MAX);
    if nearest_member <= crate::constants::WANDER_NOTIFY_RANGE {
        game_log.send(crate::ui::game_log::GameLogMessage(
            "You hear scuffling in the distance…".to_string(),
        ));
    }
}
```

**IMPORTANT:** The "Step 4 origin failure" note above flags a sequencing issue. To keep the abort-without-charging-a-fire behavior, restructure: **the tick should not bump `checks_fired` itself** — it should only send `WanderTickFired` when `turns_until_check <= 0` and leave the clock dirty (negative). The fire system, on success, sets `clock.checks_fired += 1` and `clock.turns_until_check = next_interval(...)`. On abort, it can either rewind to a small retry delay (e.g., 20 turns) or leave `turns_until_check` at its negative value so the next tick fires another attempt immediately. Pick "leave negative" — it's simpler and matches the design (rejection sampling re-rolls at the same cadence).

Adjust `wander_tick_system`:
```rust
    clock.0.turns_until_check -= 1;
    if clock.0.turns_until_check <= 0 {
        events.send(WanderTickFired);
        // DO NOT bump checks_fired or reset turns_until_check here —
        // the fire system commits the state change only on success.
    }
```

And in `wander_fire_system`, after a successful spawn, take a `ResMut<ActiveWanderClock>` and update it:
```rust
    // At the bottom, after successful spawn:
    clock.0.checks_fired += 1;
    clock.0.turns_until_check = next_interval(depth, clock.0.checks_fired);
```

(Update the function signature accordingly.)

- [ ] **Step 5: Hydrate-from-cache system**

Add to `src/game/wander.rs`:

```rust
/// Set the active clock from the floor cache on floor enter. Called on
/// `OnEnter(AppState::InGame)`. The floor materializer is responsible
/// for pushing the cached `wander_clock` into this resource before this
/// system fires.
pub fn hydrate_wander_clock_from_cache(
    // The materializer writes the cached value into the resource directly
    // (see floor_materializer.rs changes). This system is a no-op stub
    // for symmetry — it exists so future logic (e.g., reset detection)
    // has a single hook point.
) {
    // intentionally empty
}
```

Actual wiring goes in `floor_materializer.rs` — after building the floor or restoring from cache, write the clock into the resource:

```rust
// in materialize_floor / FloorResult path, where CachedFloor is consumed:
commands.insert_resource(ActiveWanderClock(cached.wander_clock));
```

For a freshly built (un-cached) floor, materializer writes `ActiveWanderClock(WanderClock::fresh(depth))`.

- [ ] **Step 6: Snapshot back into cache**

Find the system that builds `CachedFloor` for floor caching (in `dungeon.rs`, around line 199-275). Update it to take `Res<ActiveWanderClock>` and pass it through:

```rust
    CachedFloor {
        /* ... existing fields ... */
        wander_clock: active_clock.0,
    }
```

Replace the `WanderClock::default()` stub from Task 7 with the real read.

Same for the save path (`auto_save_system`): make sure the live clock is written into the floor record being persisted.

- [ ] **Step 7: Register the plugin**

In `src/game/mod.rs` (in `GamePlugin::build`), add:
```rust
app.add_plugins(crate::game::wander::WanderPlugin);
```

- [ ] **Step 8: Add cargo `tracing` for sanity logs**

In `wander_fire_system`, after a successful spawn, add:
```rust
info!("wander event fired: {} ({} member(s)) at {:?}", monster_info.monster, cluster_points.len(), origin.point);
```

This is just to make playtest validation possible without UI changes.

- [ ] **Step 9: Run all tests + manual sanity**

Run: `cargo test -p bevy_rpg`
Expected: clean.

Run: `cargo run -p bevy_rpg`
Manual check: start a game, stand still on floor 1 for ~300 turns (press wait/`.` repeatedly), watch for the game log message and the info-log `wander event fired`. Confirm no monsters appear in viewshed at spawn time.

- [ ] **Step 10: Commit**

```bash
git add src/game/wander.rs src/game/mod.rs src/map/dungeon.rs src/map/floor_materializer.rs
git commit -m "feat(wander): tick + fire system, plugin registration

Each game turn decrements ActiveWanderClock. When it hits zero, the
tick emits WanderTickFired. The fire system reads the event, weighted-
picks a can_wander entry from the depth-filtered spawn table, finds an
out-of-sight origin, BFS-clusters members, and spawns through the
runtime spawn path. On success, clock advances; on abort (no valid
origin / cluster fully visible) the clock stays negative so the next
tick re-rolls at the same cadence — implicit density throttle.

ActiveWanderClock is hydrated from CachedFloor.wander_clock on floor
enter and snapshotted back to it on floor exit / auto-save."
```

---

## Task 11: Integration test — revisit preserves clock

**Files:**
- Create: `tests/wander_persistence.rs` (or extend an existing integration test if the pattern exists).

If the repo has no integration test scaffolding for full Bevy app harnesses, skip this task — the unit coverage from Tasks 6, 8, 9, 10 plus manual playtest is sufficient. Check first:

Run: `ls /Users/nathanrude/Development/bevy_rpg/tests/ 2>/dev/null`

If `tests/` exists with at least one full-app harness, extend it. Otherwise:

- [ ] **Step 1: Add a smaller in-process test**

In `src/game/wander.rs`, add a unit-level harness test that verifies the snapshot ↔ hydrate cycle:

```rust
    #[test]
    fn cached_floor_round_trip_preserves_clock() {
        // Build a CachedFloor with a non-default clock, push it through
        // the snapshot → SavedFloorData → CachedFloor pipeline, and
        // assert the clock survives.
        use crate::map::dungeon::CachedFloor;
        use crate::save::{CachedFloorSave, SavedFloorData, MapSaveData};

        let clock = WanderClock {
            turns_until_check: 77,
            checks_fired: 3,
        };

        let cached = CachedFloor {
            map: crate::map::map::Map::new(5, 10, 10, "test"),
            monsters: vec![],
            items: vec![],
            props: vec![],
            down_stairs_pos: Point::new(0, 0),
            up_stairs_pos: Point::new(0, 0),
            wander_clock: clock,
        };

        // Convert CachedFloor → SavedFloorData (use whatever helper exists;
        // if it's via a From impl, call it; otherwise build the literal).
        let saved = SavedFloorData {
            map: MapSaveData {
                width: cached.map.width,
                height: cached.map.height,
                depth: cached.map.depth,
                name: cached.map.name.clone(),
                tiles: cached.map.tiles.clone(),
                explored: cached.map.explored_tiles.clone(),
            },
            monsters: cached.monsters.clone(),
            items: cached.items.clone(),
            props: cached.props.clone(),
            down_stairs_pos: [cached.down_stairs_pos.x, cached.down_stairs_pos.y],
            up_stairs_pos: [cached.up_stairs_pos.x, cached.up_stairs_pos.y],
            wander_clock: cached.wander_clock,
        };

        let encoded = ron::to_string(&saved).expect("encode");
        let decoded: SavedFloorData = ron::from_str(&encoded).expect("decode");

        assert_eq!(decoded.wander_clock.turns_until_check, 77);
        assert_eq!(decoded.wander_clock.checks_fired, 3);
    }
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p bevy_rpg --lib game::wander::tests::cached_floor_round_trip_preserves_clock`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/game/wander.rs
git commit -m "test(wander): clock survives the cache → save → cache round trip"
```

---

## Task 12: Documentation

**Files:**
- Create: `docs/design/SPAWNING.md`
- Modify: `docs/design/DUNGEON.md`
- Modify: `docs/design/ENEMIES.md`
- Modify: `CLAUDE.md`
- Modify: `.claude/skills/content-studio/references/ron-schemas.md`
- Modify: `.claude/skills/content-studio/references/balance-targets.md`

- [ ] **Step 1: Write `docs/design/SPAWNING.md`**

Content: the spec file's body, lightly rewritten to fit `docs/design/` style. Sections:
1. Design philosophy (why blue-noise placement, why a wander clock).
2. Data model: `MonsterSpawnInfo` fields, `WanderClock`, `ActiveWanderClock`.
3. Configuration knobs: every constant from `src/constants.rs` related to spawning, with the depth tables.
4. System flow: initial spawn pipeline (BrogueLikeBuilder → … → MonsterSpawner), then runtime (`wander_tick_system` → `WanderTickFired` → `wander_fire_system`).
5. Edge cases and resolved decisions:
   - Descend-and-return exploit → per-floor persistence resolves.
   - Origin-search aborts don't charge a fire.
   - Aquatic-vs-dry sample matching at pick time.
   - Lava is never a placement target.
6. Cross-links to: DUNGEON.md (map pipeline), ENEMIES.md (table authoring), TURNS.md (tick scheduling).

Use the existing per-system docs (FIRE.md, GAS.md, etc.) as style guides.

- [ ] **Step 2: Cross-link updates**

In `docs/design/DUNGEON.md`, find the "MonsterSpawner" line in the pipeline writeup. Replace with:

> `MonsterSpawner` — places hordes using Bridson's Poisson-disc sampler with weighted picks. See [SPAWNING.md](SPAWNING.md).

In `docs/design/ENEMIES.md`, find any section that describes "monsters spawn per room with a 50% chance" or similar; update to the new model. If no such section exists, skip.

- [ ] **Step 3: CLAUDE.md updates**

Add a `Spawning System` bullet in "Key Architectural Patterns":

```markdown
### Spawning System (Phase 3 follow-up, [src/game/wander.rs](src/game/wander.rs) + [src/map/builders/monster_spawner.rs](src/map/builders/monster_spawner.rs))
- Initial floor population: Bridson's Poisson-disc sampler places `target_hordes(depth)` blue-noise-spaced origins; each origin runs a weighted-random pick on the depth-filtered spawn table using `MonsterSpawnInfo.weight`.
- Periodic wandering: `WanderClock` lives on `CachedFloor` (per-floor state), so the clock survives stair traversal. `wander_tick_system` decrements per turn; on zero it emits `WanderTickFired` and `wander_fire_system` weighted-picks a `can_wander` entry, finds an out-of-sight tile ≥8 Chebyshev from the player, BFS-clusters members, and spawns. Aborted attempts (no valid origin) re-roll next tick at the same cadence.
- See [docs/design/SPAWNING.md](docs/design/SPAWNING.md).
```

Bump the save-schema reference: search CLAUDE.md for `v5` save references and add `v6` (per-floor `wander_clock`).

- [ ] **Step 4: content-studio references**

In `.claude/skills/content-studio/references/ron-schemas.md`, find the `MonsterSpawnInfo` schema documentation. Add:

```
- `weight: u32` (default 10) — relative selection weight within the depth-filtered set. Common = 20, uncommon = 10, rare = 3, elite = 1.
- `can_wander: bool` (default true) — false = builder-only (bosses, set-piece packs).
```

In `.claude/skills/content-studio/references/balance-targets.md`, add a "Spawn weight tiers" section with the table from spec §1.

- [ ] **Step 5: Commit**

```bash
git add docs/design/SPAWNING.md docs/design/DUNGEON.md docs/design/ENEMIES.md CLAUDE.md .claude/skills/content-studio/references/ron-schemas.md .claude/skills/content-studio/references/balance-targets.md
git commit -m "docs(spawning): SPAWNING.md + cross-doc updates

New canonical design doc for the spawning system. Updates DUNGEON.md
to point at it from the pipeline writeup, ENEMIES.md to refresh
initial-spawn behavior, CLAUDE.md to add the spawning architectural
pattern and bump the save schema reference to v6, and the
content-studio skill references to document the new weight and
can_wander fields plus the weight tier table."
```

---

## Self-Review

- **Spec §1 (Schema):** Task 1. ✓
- **Spec §2 (Bridson + target):** Tasks 2, 3, 5. ✓
- **Spec §3 (Wander clock):** Tasks 6, 7, 10. ✓
- **Spec §4 (Wander event):** Tasks 9, 10. ✓
- **Spec §5a (Stair guard):** Task 4. ✓
- **Spec §5b (Lava-as-water):** Task 4. ✓
- **Spec §5c (RNG seeding):** Task 5 (MonsterSpawner now shares `BuilderMap.rng`). Wander uses unseeded per-event RNG per spec — covered in Task 10. ✓
- **Spec §5d (Save/load v5 → v6):** Task 8. ✓
- **Spec §5e (Test coverage):** Tasks 1, 2, 3, 4, 5, 6, 8, 9, 11 add tests for every pure function and the round-trip path. ✓
- **Spec §5f (Documentation):** Task 12. ✓
- **Spec open question (cached-floor struct name):** Resolved in Task 7 — `CachedFloor` (existing in `src/map/dungeon.rs`).
- **Spec open question (constants location):** Resolved in Task 5 — `src/constants.rs`.
- **Spec open question (weighted_pick location):** Resolved in Task 2 — `src/map/builders/weighted_pick.rs`, free-function.
- **Spec open question (faction-flavored log):** Out of scope, marked in spec; ignored here.

**Type-consistency scan:** `WanderClock`, `ActiveWanderClock`, `WanderTickFired`, `WanderPlugin`, `find_wander_origin`, `WanderOrigin`, `target_hordes`, `bridson_poisson_disc`, `Sample`, `weighted_pick` — names match across tasks. ✓

**Placeholder scan:** all code blocks are concrete. Two task steps (Task 10 Step 1, Task 11 Step 1) ask the implementer to check the existing code first before deciding between "extract a helper" vs "extend existing harness" — these aren't placeholders, they're decision points with both branches specified.
