# Spawn Weights, Poisson-Disc Placement, and Wander Clock

**Status:** Draft — design approved, awaiting implementation plan.
**Date:** 2026-05-13
**Touches:** `src/map/builders/monster_spawner.rs`, `src/assets/mod.rs`, `src/map/dungeon.rs`, `assets/monster_spawns.ron`, save schema, several design docs.

## Goal

Make floor-population feel intentional and put pressure on camping:

1. Give content authors a `weight` knob so rare/elite entries can actually be rare within their depth band.
2. Replace the per-room 50% roll with a floor-wide blue-noise sampler so density is even across both room-heavy and cave-heavy floors, and so density scales with depth.
3. Add a per-floor "wander clock" that periodically spawns monsters out of sight while the player is on the level, accelerating the longer the player stays.
4. Fix four latent bugs in the current spawner: silent stair acceptance in cluster BFS, lava-as-water for aquatic spawns, unseeded RNG, and the not-yet-existing wander-clock reset exploit.

## Non-goals

- No per-monster wander behavior (alerted-on-spawn, aggression toward player). Wandered monsters use their normal AI.
- No telegraphed pre-spawn warning turn. Game log is the only tell.
- No global cap on wandered monsters. Density throttle is implicit via origin rejection sampling.
- No Voronoi/Lloyd-relaxation centroid sampling. Bridson's Poisson-disc gives the same blue-noise feel at lower cost.

---

## 1. Schema changes

In [src/assets/mod.rs:509-533](../../../src/assets/mod.rs#L509-L533), `MonsterSpawnInfo` gains two fields:

```rust
pub struct MonsterSpawnInfo {
    // ... existing fields ...

    /// Relative selection weight within the depth-filtered set.
    /// Default 10 so existing entries without an explicit weight stay equal-weighted.
    #[serde(default = "default_weight")]
    pub weight: u32,

    /// Eligibility for periodic mid-floor "wander" spawns.
    /// Default true. Set false for bosses, large set-piece packs, or fragile
    /// aquatic-only groups that should only appear at floor-gen.
    #[serde(default = "default_true")]
    pub can_wander: bool,
}

fn default_weight() -> u32 { 10 }
fn default_true() -> bool { true }
```

Authoring tiers (added to `content-studio/references/balance-targets.md`):

| Rarity   | Weight |
|----------|--------|
| Common   | 20     |
| Uncommon | 10     |
| Rare     | 3      |
| Elite    | 1      |

Existing entries in `assets/monster_spawns.ron` default to weight 10 — no behavior change until authors retune.

## 2. Initial spawn — Bridson's Poisson-disc placement

Replace the per-room 50% gate in `MonsterSpawner::spawn_monsters` ([src/map/builders/monster_spawner.rs:30-141](../../../src/map/builders/monster_spawner.rs#L30-L141)).

**Target horde count per floor:**

```
target_hordes(depth) = 8 + floor(depth * 0.6)
```

| Depth | Hordes |
|-------|--------|
| 1     | 8      |
| 13    | 15     |
| 26    | 23     |

Tunable via a constant in `constants.rs` so the curve can be retuned without code churn.

**Bridson's algorithm, restricted to walkable tiles:**

1. Candidate set: all walkable, non-stair, non-portal tiles (both dry and liquid — Lava is excluded). Liquid classification of each accepted point is read at pick time, not used to restrict the sampler.
2. Compute minimum separation radius:
   ```
   r = sqrt(walkable_area / (target_hordes * π * 0.7))
   ```
   The 0.7 packing constant gives slack so the sampler reliably hits the target.
3. Pick a random initial seed from the candidate set. Add to the active list.
4. While the active list is non-empty:
   - Pick an active sample at random.
   - Generate up to `K = 20` candidate points in an annulus `[r, 2r]` around it.
   - Accept the first candidate that lies in the candidate set and is ≥ `r` from every existing sample.
   - If no candidate accepts, retire the active sample.
5. Each accepted sample is a **horde origin** carrying its liquid state (dry vs water).

**Per horde:**

- Restrict the depth-filtered spawn table to entries whose `spawn_on_liquid` matches the origin's liquid state.
- If the restricted set is empty (rare — happens only when a liquid origin lands on a floor with no aquatic entries), skip this origin.
- Weighted-pick a `MonsterSpawnInfo` from the restricted set.
- Roll group composition via existing branches (homogeneous `min_group/max_group` or heterogeneous `group`).
- Cluster-place via `find_cluster_points` with the §5 bug fixes.

**Why Poisson-disc over Lloyd-relaxed Voronoi:** Lloyd relaxation iterates centroids over the walkable grid each pass — overkill for ~20 sites on an 80×60 map. Bridson is O(N) and produces equivalent blue-noise spacing without iteration. Each horde implicitly owns its Voronoi cell via the minimum-separation guarantee.

**Why decouple from rooms:** Works identically on cave floors, mixed cave/room floors, and corridor-heavy floors. Avoids the current behavior of hordes concentrating in dense-room regions and leaving sparse-room regions empty.

## 3. Wander clock — per-floor ramping pressure

A wander clock that lives on the cached floor and accelerates the longer the player stays.

**State (lives on the cached floor, not as a standalone resource):**

```rust
pub struct WanderClock {
    /// Game turns until the next wander check fires. Decrements per turn.
    pub turns_until_check: i32,
    /// Number of wander events that have fired on this floor.
    pub checks_fired: u32,
}
```

`WanderClock` is initialized when a floor is first materialized and persists in the floor cache (and in the save) so it survives stair traversal and game reloads.

**Cadence:**

```
base_interval(depth) = max(60, 300 - depth * 8)
//   floor 1  → 292 turns
//   floor 13 → 196
//   floor 26 →  92

ramp_factor(checks_fired) = max(0.3, 1.0 - checks_fired * 0.08)
//   1st check  → 1.00
//   5th check  → 0.60
//   10th+      → 0.30 (floor)

next_interval = floor(base_interval(depth) * ramp_factor(checks_fired))
```

**Lifecycle:**

- First arrival on a floor → `turns_until_check = base_interval(depth)`, `checks_fired = 0`.
- Stair traversal → leaving floor freezes its clock; entering floor resumes from its persisted value (fresh if first visit).
- The clock does not tick on unloaded floors. Backtracking is not a free reset — accumulated wandered monsters are still standing where they were when the player left, and the clock resumes at the same `checks_fired`.

**Descend-and-return exploit:** explicitly resolved by per-floor persistence. Cost of bouncing two floors to "reset the clock" is two stair-use turns plus two floor crossings, and the clock isn't actually reset on return — strictly worse than just continuing to play.

**Tick system:**

- Runs in `ProcessingPhase`, gated on `AppState::InGame` + `TurnState::Processing`.
- Decrements `turns_until_check` by `1` per game turn (measured against `TurnManager`'s game-time delta — not per-actor action).
- When `turns_until_check <= 0`: fire one wander event (§4), bump `checks_fired`, set `turns_until_check = next_interval`.

**Pressure sanity check** at floor 13, `base_interval = 196`. After 5 fires, intervals shrink to ~118 turns. A player who spends 1500 turns on the floor sees ~9 wander events; a player who leaves at 800 turns sees ~4. Clearing efficiently is rewarded; dawdling has teeth.

## 4. The wander event

When the clock fires, run this procedure exactly once:

1. **Filter** the spawn table by `depth` (existing filter) and `can_wander == true`.
2. **Weighted-pick** one `MonsterSpawnInfo` from the filtered set using the new `weight` field.
3. **Find an origin tile** satisfying all of:
   - Walkable, with liquid state matching `spawn_on_liquid`.
   - Not in the player's current `Viewshed.visible_tiles`.
   - Not stairs, not portal.
   - Not currently occupied by any actor or item.
   - Chebyshev distance ≥ `MIN_WANDER_DISTANCE = 8` from the player.
   - Reachable from the player's tile (A* connectivity test) so the spawn isn't stranded in an isolated pocket.

   Rejection-sample up to 30 attempts. If no valid origin is found, **abort the event silently** — do not bump `checks_fired`, do not reset `turns_until_check` to a fresh interval. The next tick re-rolls at the same cadence. This is the implicit density throttle: a crowded floor or fully-visible floor naturally stops producing wanderers.

4. **Roll group composition** using the same homogeneous / heterogeneous logic the initial spawner uses.
5. **Cluster-place** via `find_cluster_points` (with §5 fixes). Additional constraint: every placed tile must remain outside the player's viewshed. If BFS expansion would land a member in viewshed, drop that member. At least one member must place or the event aborts.
6. **Materialize** through the same path the initial spawner uses (`SpawnEntry` → `add_monster_spawn` → floor materializer). Squad wiring, leader tagging, and faction assignment all reuse existing infrastructure.
7. **Soft-notify** via `GameLog`: if any placed member is within 15 Chebyshev tiles of the player, push *"You hear scuffling in the distance…"* (vary the string by faction/species later if desired). Out-of-range events stay silent.

## 5. Bug fixes, RNG seeding, save/load, tests

### 5a. BFS stair/portal guard

`find_cluster_points` ([src/map/builders/monster_spawner.rs:205-253](../../../src/map/builders/monster_spawner.rs#L205-L253)) rejects walls and lava in expansion but does not reject stairs or portal terrain in placement. Add a terrain filter at the result-push site:

```rust
let terrain_ok = !matches!(
    map.tiles[idx].terrain,
    TerrainType::UpStairs | TerrainType::DownStairs | TerrainType::Portal
);
```

Combine with the existing predicates: `is_walkable && liquid_ok && terrain_ok && !occupied`. Expansion remains permissive so clusters can route past stairs if necessary.

### 5b. Lava acceptance in `liquid_only` mode

Same function, line 224: `liquid != LiquidType::None` currently accepts Lava as valid for aquatic spawns. Tighten:

```rust
let liquid_ok = if liquid_only {
    matches!(
        map.tiles[idx].liquid,
        LiquidType::ShallowWater | LiquidType::Water
    )
} else {
    map.tiles[idx].liquid == LiquidType::None
};
```

A future fire-walker monster gets its own flag — do not conflate aquatic and pyrophilic placement.

### 5c. RNG seeding

`MonsterSpawner::spawn_monsters` ([src/map/builders/monster_spawner.rs:32](../../../src/map/builders/monster_spawner.rs#L32)) constructs `RandomNumberGenerator::new()` — unseeded. The builder pipeline already threads a map seed.

- Plumb the seed into `MonsterSpawner` (constructor argument or pulled from `BuilderMap`).
- Construct as `RandomNumberGenerator::seeded(map_seed.wrapping_add(MONSTER_SPAWN_SALT))`. The salt prevents different builder stages from sharing an RNG stream while keeping the full pipeline deterministic from a single seed.
- Apply the same pattern to the new Bridson sampler (different salt).
- `WanderClock` keeps an **unseeded** RNG per event. Wandering should feel emergent and respond to how the player camps; a fixed seed would make wander events pre-baked, which defeats the design.

### 5d. Save/load (per `.claude/rules/save-load-checklist.md`)

Save schema `v5 → v6`. Each cached floor's save record gains:

```rust
pub struct WanderClockSave {
    pub turns_until_check: i32,
    pub checks_fired: u32,
}
```

Migration: a v5 floor loads with a default `WanderClockSave` equivalent to a fresh first visit (`turns_until_check = base_interval(depth)`, `checks_fired = 0`). Add a round-trip test asserting non-default values survive save → load.

### 5e. Test coverage (per `.claude/rules/testing-requirements.md`)

**Pure-function tests in `monster_spawner.rs`:**

- `weighted_pick` distribution: 10,000 rolls against a weighted vec; each entry's selection rate within ±5% of its weight ratio.
- `weighted_pick` with `weight: 0` is never selected.
- `bridson_poisson_disc` on an 80×60 all-floor grid with `target = 20` returns 18–22 points (band, not exact); all pairwise distances ≥ `r`.
- `bridson_poisson_disc` on a fully walled grid returns empty.
- `find_cluster_points` stair-rejection: origin adjacent to `DownStairs`, request 5 members, assert no member lands on the stair tile.
- `find_cluster_points` lava-rejection-in-liquid-mode: 3×3 with center water, surrounded by lava, `liquid_only = true` returns only the center.

**Pure-function tests in new `wander.rs`:**

- `next_interval(depth, checks_fired)` — table covering floor 1 / 13 / 26 × checks 0 / 5 / 10 / 20. Verify the ramp floors at 0.3 and the base at `max(60, …)`.
- `target_hordes(depth)` — table covering floors 1, 13, 26.

**Integration test (gated on the existing Bevy app harness, if usable):**

- Build a 3-floor dungeon. Walk floor 1 → 2 → 1. Assert floor 1's `WanderClock` resumes at the value it had when the player left, not a fresh interval.

### 5f. Documentation impact

Per `.claude/rules/design-docs-required.md`:

- **New file** `docs/design/SPAWNING.md` — full writeup of weights, Bridson placement, wander clock, depth scaling, and the descend-and-return exploit resolution.
- **Update** `docs/design/DUNGEON.md` — replace the `MonsterSpawner` line in the pipeline writeup with a pointer to SPAWNING.md.
- **Update** `docs/design/ENEMIES.md` — refresh any section that currently describes initial-spawn behavior.
- **Update** `CLAUDE.md` — `WanderClock` in the architectural patterns section; bump save schema reference to v6.
- **Update** `.claude/skills/content-studio/references/ron-schemas.md` — document `weight` and `can_wander` on `MonsterSpawnInfo`.
- **Update** `.claude/skills/content-studio/references/balance-targets.md` — weight tiers table from §1.

## Open questions for plan phase

- Exact name and location of the cached-floor struct that hosts `WanderClock` ([src/map/dungeon.rs](../../../src/map/dungeon.rs) `Floor` resource vs. an inner cached-data struct).
- Whether the `MIN_WANDER_DISTANCE = 8` constant and the `target_hordes` curve should live in `constants.rs` or alongside their consumers.
- Whether `weighted_pick` becomes a free function in `monster_spawner.rs` or a helper in a new shared module (e.g., `rng_utils.rs`) — depends on whether item spawning gets the same treatment in a follow-up phase.
- Whether the soft-notify log message should vary by faction (defer until faction taglines exist).

## Out of scope (follow-ups)

- Weighted picks for `ItemSpawnInfo` — same shape; should be a separate phase.
- Wander-clock UI surfacing (some kind of "pressure" indicator on the HUD) — only if playtesting shows the system is opaque.
- Faction-flavored wander log messages.
- Per-monster `wander_only` flag (monsters that *only* appear via wandering) — not needed yet.
