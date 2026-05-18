# Spawning

How monster packs are placed on the map at build time.

## Design philosophy

- **One algorithm for every map type.** The forest is cellular-automata
  (no rooms). The town is hand-stamped (a few rooms but no
  representative-room data structure). A future dungeon could be either.
  Rather than ship a different spawner per builder family, the project
  uses one: a Voronoi-cell spawner that only needs walkable tiles.
- **Packs, not individuals.** A spawn-table entry's whole group goes
  into one Voronoi cell, BFS-bounded to the cell. The cell is the
  pack's "territory." Yields recognisable patrols / warrens instead of
  monsters scattered one-per-tile.
- **The player never arrives on top of a fight.** A Chebyshev-radius
  buffer around the builder's `starting_position` is scrubbed out of
  every cell before sampling, so the centre clearing the player lands
  on is always empty.
- **Predictable difficulty ramp.** Per-floor pack count is
  `BUDGET_BASE + depth`, not a per-cell random roll. Floor 1 gets 3
  packs, Forest 2 gets 4. Easy to balance against player power.
- **Future-proof for faction-coherent regions.** Today every cell rolls
  the same floor-wide spawn table. The structure supports per-cell
  table choices later — e.g., "this cell is a goblin patrol, that one
  is bandits, that one is wildlife."

## Where the code lives

| File | Responsibility |
|------|----------------|
| [src/map/builders/voronoi_spawner.rs](../../src/map/builders/voronoi_spawner.rs) | `VoronoiSpawner` MetaMapBuilder + pure helpers (`voronoi_regions`, `exclude_around`, `find_pack_cluster`). The pure helpers take plain `Map` + `HashSet` and can be unit-tested without an `App`. |
| [src/map/builders/mod.rs](../../src/map/builders/mod.rs) | `forest_builder()` inserts `VoronoiSpawner::new(spawn_table)` into the chain after `ForestStairsBuilder`. `floor_builder()` passes the spawn table through (previously `_spawn_table`). |
| [assets/monster_spawns.ron](../../assets/monster_spawns.ron) | The spawn table — `min_floor` / `max_floor` per entry, optional `min_group`/`max_group` or full `group: [(...)]` for mixed packs. |
| [src/map/builders/forest.rs](../../src/map/builders/forest.rs) | `ForestStairsBuilder` declared as `BuilderPhase::StructurePlacement` so stair tiles exist before the spawner runs. |

The old room-iterating `MonsterSpawner` was deleted when the Voronoi
spawner shipped; no surviving consumer required rooms.

## Algorithm

```text
1.  Read the spawn table; keep entries where depth ∈ [min_floor, max_floor].
    Bail with an info log if nothing is eligible.

2.  Build Voronoi cells:
      For every dry walkable non-stair tile, compute
        key = (FastNoise.get_noise(x, y) * 10240.0) as i32
      Bucket the tile index under `key` in a HashMap<i32, Vec<usize>>.
    Use NoiseType::Cellular with Manhattan distance and frequency 0.08
    (matches the rust-roguelike-tutorial value; gives ~15–25 usable
    cells on an 80×60 forest).

3.  Drop cells smaller than MIN_REGION_TILES (6). Too cramped to host
    a pack without making the cluster feel forced.

4.  Excise tiles within Chebyshev radius START_BUFFER (4) of the
    builder's `starting_position`. Drop any cell that empties below
    MIN_REGION_TILES afterward.

5.  Bail with a warn log if no cells survive.

6.  Compute the floor's pack budget:
        target = BUDGET_BASE + depth      (Forest 1 → 3, Forest 2 → 4)
        budget = min(target, surviving_cell_count)

7.  Weighted sample `budget` cells without replacement, weighted by
    each cell's tile count (bigger cell → more lottery tickets).

8.  For each chosen cell:
      a. Roll one spawn-table entry (uniform).
      b. Roll pack composition:
           - if `entry.group` is non-empty, materialise each `(monster,
             min_count, max_count)` member into the pack list;
           - else use `entry.monster` with `min_group..=max_group` copies.
      c. Pick a random in-cell origin, BFS outward through the cell's
         tile set collecting up to `pack.len()` walkable, unoccupied
         tiles (cardinal adjacency, region-bounded).
      d. If `pack.len() == 1`: emit a `SpawnEntry::solo`.
         Otherwise: allocate a new `SquadId`, emit one
         `SpawnEntry::squad` per pack member, the first as leader.
      e. Mark each placed tile as occupied so subsequent picks don't
         overlap.

9.  Push every emitted `SpawnEntry` into `build_data.spawn_list` —
    downstream `floor_materializer.rs` consumes the list unchanged.
```

## Pipeline ordering

`BuilderPhase` runs builders in this order:

```
Geometry → TerrainCleanup → StructurePlacement → ConnectivityCull
       → Spawning → Finalization
```

`VoronoiSpawner` declares `Spawning`. Other builders in the forest
pipeline:

| Builder | Phase | Why |
|--------|-------|-----|
| `ForestTerrainBuilder` | (Initial) | Lays the cellular-automata map. |
| `ForestStairsBuilder`  | `StructurePlacement` | **Stairs are terrain** — they must exist before `VoronoiSpawner` so the spawner skips them. (Previously `Finalization`; moved.) |
| `VoronoiSpawner`       | `Spawning` | Reads the stamped map; emits spawn entries. |
| `DecorationPropagator` | `Finalization` | Decoration overlay; doesn't interact with spawns. |

If you add a new MetaMapBuilder that stamps walkable terrain, declare
its phase as `StructurePlacement` (or earlier) so spawning sees it.

## Tunables

All live as `pub const` in [voronoi_spawner.rs](../../src/map/builders/voronoi_spawner.rs):

| Constant | Default | Effect |
|----------|---------|--------|
| `NOISE_FREQUENCY` | `0.08` | Higher → smaller cells (more, tighter regions). |
| `MIN_REGION_TILES` | `6` | Cells smaller than this are discarded. Raise to push spawns into bigger open areas; lower to allow more tucked-into-trees packs. |
| `START_BUFFER` | `4` | Chebyshev radius around `starting_position` that is scrubbed of spawn-eligible tiles. |
| `BUDGET_BASE` | `2` | `pack_count = BUDGET_BASE + depth`. Raise for a denser game. |

These are deliberately not yet wired through to RON or to per-floor
overrides — the world is small enough that one tuple is fine. When
that stops being true (per-floor density, per-biome cell size), promote
to per-builder fields in `VoronoiSpawner` and pass through
`forest_builder()`.

## Configuring spawns (data side)

Spawn entries live in [assets/monster_spawns.ron](../../assets/monster_spawns.ron).

Solo entry:

```ron
(monster: "Giant Rat", min_floor: 1, max_floor: 2, min_group: 1, max_group: 3),
```

Mixed-species pack (use `group:` instead of `monster:`):

```ron
(group: [
    (monster: "Giant Rat", min_count: 2, max_count: 3),
    (monster: "Giant Bat", min_count: 1, max_count: 2),
], min_floor: 1, max_floor: 3, flee_threshold: 0.4),
```

The spawner picks one entry per chosen cell. Eligibility is
`depth ∈ [min_floor, max_floor]` only — there's no weight column yet
(every eligible entry is equally likely). If we want common-vs-rare
spawns later, add `weight: u32` to `MonsterSpawnInfo` and switch the
uniform pick (step 8a) to a weighted draw.

Aquatic spawns (`spawn_on_liquid: true`) are not yet handled by the
Voronoi spawner — the cell-builder filters water tiles out, so any
`spawn_on_liquid` entry placed in the table will be silently ignored.
Re-enable by adding a parallel "water-cell" builder when an aquatic
monster (Eel) is reintroduced.

## Testing

Pure helpers have unit tests in
[voronoi_spawner.rs](../../src/map/builders/voronoi_spawner.rs#L284):

- `voronoi_regions_skip_walls_water_and_stairs` — every returned tile is
  a dry walkable non-stair Floor.
- `voronoi_regions_partition_walkable_tiles` — every eligible tile
  belongs to exactly one cell.
- `exclude_around_drops_tiles_within_chebyshev_radius` — buffer math is
  right.
- `find_pack_cluster_stays_inside_region` — BFS never escapes the cell.
- `find_pack_cluster_respects_occupied_tiles` — already-placed tiles
  are skipped.
- `weighted_sample_*` — distinct picks, clamped to available pool.

The spawner *system* itself isn't unit-tested (it wires together a
`BuilderMap` mutation), but `forest_builder()` integration is exercised
by the forest builder tests in `forest.rs`.

## Open questions / future work

- **Faction-coherent cells.** Tag spawn-table entries with a faction;
  per cell, pick a faction first, then a member of that faction. Lets a
  forest floor have a goblin patrol cell, a wildlife cell, and a
  bandit-camp cell rather than mixing the rolls per cell.
- **Per-floor density override.** RON-level `density_multiplier: f32`
  on a per-floor manifest so Forest 2 can be denser than Forest 1
  without touching code.
- **Spawn weights.** `weight: u32` on each `MonsterSpawnInfo` so we
  can model "common rats, rare bandits" within a single eligibility
  band.
- **Aquatic cells.** Optional second pass that builds cells only over
  water tiles. Re-enables `spawn_on_liquid` spawns (Eels) when water
  features return.
- **Out-of-depth picks.** A small chance (~5%) to lift an entry from
  the next-floor band into the current one — keeps the player on
  their toes. Easy to add after the per-floor budget step.

## Cross-references

- [ENEMIES.md](ENEMIES.md) — per-monster identity, faction roster,
  the source of truth for *what* spawns.
- [OVERWORLD.md](OVERWORLD.md) — the floor topology that the spawner
  runs on.
- [`docs/design/CHARACTER.md`](CHARACTER.md) §Level Progression — the
  XP grant on monster death depends on monster tier × player level,
  but the spawner has no opinion on this; XP is post-kill bookkeeping.
- [`.claude/rules/design-docs-required.md`](../../.claude/rules/design-docs-required.md) — why this doc exists.
