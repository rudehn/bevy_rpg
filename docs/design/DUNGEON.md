# Dungeon Design

> **Status update (overworld milestone):** the game's current shape is an
> **overworld + 3-floor temple**, not a 26-floor descent. See
> [OVERWORLD.md](OVERWORLD.md) for the canonical writeup of the new structure
> (town hub, 8-tile forest ring, temple). The 26-floor pipeline described in
> the rest of this file is preserved for floors `>= 12`, but is not currently
> reached in a fresh run. Spawners (monsters, items, chests) are temporarily
> disabled in the overworld pipelines — content returns in a later phase.

## Overview

The dungeon is 26 floors of procedurally generated maps. Each floor is built by
a composable **builder pipeline** that layers terrain, liquids, doors, decorations,
and encounters. The map is 80x60 tiles on a 16x16 pixel grid.

Two parallel representations exist:
1. **Map resource** — pure data (tiles, width, height, depth). Drives game logic
   and pathfinding.
2. **ECS tile entities** — handle rendering, visibility, sprites.

## Builder Pipeline

The `BuilderChain` composes one `InitialMapBuilder` + N `MetaMapBuilder`s. The
pipeline runs once per floor when the player descends.

### Current Pipeline

```
BrogueLikeBuilder          (initial map: rooms, corridors, doors)
  -> DiagonalCuller        (remove diagonally-unreachable walls)
  -> StartPointBuilder     (place player start position)
  -> LakeBuilder           (thematic water/lava lake encounters)
  -> PrefabPlacer          (stamp hand-designed room layouts)
  -> DecorationPropagator  (pass 1: fill natural dungeon with decorations)
  -> MachinePlacer         (topology-gated encounters)
  -> DecorationPropagator  (pass 2: fill edges around placed encounters)
  -> CandleSpawner         (place light sources on wall tiles)
  -> ItemSpawner           (place chests with loot)
  -> VoronoiSpawner        (populate the map with enemy packs — see SPAWNING.md)
  -> UnseenCuller          (remove tiles unreachable from player start)
  -> DistantExit           (place down-stairs far from player)
```

On floor 26, an additional step places the **Amulet of Ascension** and the
**Escape Portal** at distant points from each other and from the player start.

## Generation Style

The dungeon uses a mix of **room-based** and **cavernous** generation. Caverns
become more prominent on deeper floors.

| Floors | Style |
|--------|-------|
| 1-3 | Primarily room-based with small cave pockets |
| 4-6 | Hybrid — larger rooms, some cave corridors |
| 7-9 | More cavernous — organic open spaces mixed with rooms |
| 10-25 | Increasingly cavernous — deep dungeon with diverse hazards |
| 26 | Hybrid — normal dungeon content with amulet + portal placement |

The `BrogueLikeBuilder` already supports room types and cave generation. The
ratio of rooms to caves shifts with depth via floor profile configuration.

## Tile System

### Tile Layers

Each tile has two layers: terrain + liquid.

**Terrain types:**

| Terrain | Walkable | Passable | Opaque | Notes |
|---------|----------|----------|--------|-------|
| Wall | No | No | Yes | Standard wall |
| Floor | Yes | Yes | No | Open ground |
| Door | Yes | Yes | No | Opens when walked through |
| OpenDoor | Yes | Yes | No | Already opened |
| LockedDoor | No | No | Yes | Requires matching key |
| DownStairs | Yes | Yes | No | Descend to next floor |
| UpStairs | Yes | Yes | No | Return to previous floor |
| Empty | No | No | No | Void / off-map |

- `is_walkable()` requires both terrain and liquid to be walkable
- `is_passable()` is used for connectivity checks (doors count, liquids ignored)
- `is_opaque()` blocks FOV (walls, closed doors)

### Door Placement & Validation

Doors are placed by three distinct passes inside `BrogueLikeBuilder`:

1. **Room-attach doors** — each newly attached room gets one door at the
   junction where its door-site aligns with the existing dungeon wall.
   Sites are filtered by `direction_of_door_site`: the wall must have
   **exactly one** floor neighbor among N/E/S/W, so it's a proper
   one-tile separator.
2. **Reward-room door** — same `direction_of_door_site` filter, but the
   candidate is selected via the choke-map (high choke value =
   topologically isolated wall slot).
3. **Loop doors (`add_loops`)** — adds extra doors to shorten long
   detours between two regions. Each candidate must satisfy
   `loop_door_axis` (floor on opposite sides of one axis AND walls on
   both perpendicular sides — a true one-tile separator), must have no
   orthogonal door neighbor, and the BFS detour around it must exceed
   the minimum-path threshold. Newly placed loop doors are treated as
   walkable for subsequent BFS so neighbors don't pile up into chains.

The `FinishDoors` cleanup pass then runs in the `TerrainCleanup` phase
and iterates until stable, demoting any door that:

- Is passable on both cardinal axes (sits in an opening, not a wall
  separator)
- Has 3+ blocking neighbors (dead end)
- Has any orthogonally-adjacent door (scan-order dedup keeps the first,
  demotes the rest)

> **Future work:** the `add_loops` BFS-detour heuristic could be
> replaced with Brogue's exact loop algorithm (validated detour at
> proven door-site candidates with topology-aware selection). Item D in
> the door-placement review — kept as a future TODO; current A+B+C
> changes resolve the visible bugs (mid-floor doors, adjacent
> clusters).

**Liquid types:**

| Liquid | Walkable | Notes |
|--------|----------|-------|
| None | Yes | Default |
| ShallowWater | Yes | Extinguishes burning status |
| DeepWater | Conditional | Player can traverse; land monsters cannot path through |
| Lava | Yes | Instant death without fire resistance |
| Chasm | No | Impassable void; player can voluntarily fall (2d6 damage, descend a floor) |

### Chasms

Chasms are impassable voids placed during map generation as lake bodies. They
can also be **created at runtime** by Pit Bloat explosions: the bloat's
`ExplodeOnHit` ability places `CrackedFloor` decorations, which collapse into
chasms after ~3 turns via the tile promotion system.

**Entities on collapsing tiles fall to the floor below**, maintaining their
(x,y) position (adjusted to the nearest walkable tile via BFS). Monsters are
snapshotted to `FallenEntities` and spawned when that floor is materialized.
The player takes 2d6 fall damage and transitions to the next floor. On
floor 26, entities fall into the void and are destroyed.

### Water Mechanics

**Shallow Water:**
- Extinguishes burning on any entity that steps in or starts a turn in it

**Deep Water:**
- **Player traversable but risky.** Each step through deep water:
  - Costs extra movement (1.5x action delay)
  - Has a chance to sweep an item from the player's inventory. The item floats
    in a random direction until it lands on a non-deep-water tile (Brogue-style).
- **Land monsters cannot path through deep water.** Their pathfinding treats
  deep water tiles as impassable. This creates natural barriers — the player can
  use water to escape or funnel enemies.
- **Aquatic monsters** (if any) can traverse deep water freely.
- Creates risk/reward decisions: shortcut through water and risk losing gear,
  or take the long way around.

**Water Shimmer (Visual):**
- Water tiles animate with a Brogue-style "color dancing" shimmer effect
- Each tile has a deterministic phase offset based on position, creating organic ripple patterns
- Two overlapping sine waves at different frequencies produce irregular, natural-looking shimmer
- Deep water: darker blue base (`0.5, 0.55, 0.85`) with wider color variation (±0.12)
- Shallow water: lighter cyan base (`0.6, 0.75, 0.95`) with subtler variation (±0.06)
- Shimmer integrates with the lighting system — light level and color tint are applied
- Only visible tiles animate; explored-but-not-visible tiles remain dim gray
- Runs every frame (trivial cost for ~200 tiles) to stay in sync with visibility updates
- Implementation: `animate_water_shimmer` system in `src/game/water.rs`

**Lava:**
- **Instant death** for entities without fire resistance
- Entities with fire resistance: 15 HP/turn while standing in lava
- Exiting lava applies Burning (5 fire damage/turn for 5 turns) to
  non-fire-resistant entities
- Lava tiles provide **ambient light** but still require exploration to be
  visible. Once a lava tile has been seen, it remains lit on the map.

## Lakes

Lakes are not randomly scattered — they are **thematic encounters**. Each lake
is a deliberate feature: a medium or large body of water that the player
must navigate around, through, or across.

### Lake Placement

- Prefer **fewer, larger** lakes over many small disjoint puddles
- Lakes are placed as cohesive bodies using the organic blob algorithm
- Each floor gets 0-2 lakes depending on depth

### Liquid by Depth

`LakeBuilder::pick_liquid_type` decides which liquid fills each lake.
Lava is gated to the mid-and-deep dungeon — early floors stay
"natural" (water and chasms only) so the player meets fire hazards
*after* descending into volcanic strata.

| Floors | Water | Lava | Chasm |
|--------|-------|------|-------|
| 1–9    | 70%   | —    | 30%   |
| 10–17  | 40%   | 35%  | 25%   |
| 18–26  | 20%   | 50%  | 30%   |

The "Lava Vault" machine encounter is gated by the same threshold
(`min_floor: 10`). Bumping the lake threshold without moving the
machine would let lava reappear via a back-door spawn.

## Decorations

A Brogue-style system for spreading environmental decorations across the dungeon.
Decorations are **purely visual for now**.

### Decoration Types

| Decoration | Appears On | Placement Rule |
|-----------|-----------|---------------|
| Grass | Floor (floors 1-5) | BFS propagation from seeds |
| TallGrass | Floor (floors 1-5) | Chain from Grass (20% chance) |
| DeadGrass | Floor (floors 2-26) | BFS propagation |
| Rubble | Floor (all floors) | Wall-adjacent only |
| Moss | Floor (floors 1-8) | Near liquid only |
| Fungus | Floor (floors 3-26) | BFS propagation |
| Cobweb | Floor (floors 1-26) | Wall-adjacent corners only |
| Bloodstain | Floor (floors 3-26) | BFS propagation, small clusters |

### Propagation Algorithm

1. For each decoration rule valid at this floor depth:
   - Roll seed count (scaled by floor profile density)
   - For each seed, find a valid tile matching terrain/adjacency requirements
   - BFS outward from seed with decaying probability
   - Each step: `next_chance = current_chance * propagation_decay`
   - Respects exclusion zones (prefab interiors, machine interiors)

### Two-Pass Strategy

- **Pass 1** (before PrefabPlacer): Fills natural dungeon. Prefabs stamped later
  overwrite decorations in their footprint.
- **Pass 2** (after MachinePlacer): Fills edges around placed encounters.
  Exclusion zones prevent decorating inside encounter interiors.

## Lighting

The game uses a custom lighting system. Light sources are placed by
CandleSpawner in the builder pipeline.

### Design Goals

- **Consistent lighting across all floors.** Every floor should feel well-lit
  enough to play comfortably.
- **Coverage depends on wall tiles.** Candles are placed relative to wall
  geometry, not as a fixed count.
- **Most of the floor should be lit.** Dark pockets exist naturally in areas
  far from walls, but the majority of explorable space has light.

### Light Sources

- **Candles/torches** — placed on wall tiles by CandleSpawner
- **Lava** — provides ambient light on adjacent tiles (after exploration)
- **Watchfires** — placed in prefabs and machine encounters

## Floor Persistence

Floors persist in a **floor cache** when the player ascends or descends. Returning
to a previously visited floor restores its exact state (map, monsters, items,
decorations, door states). This enables backtracking for exploration.

The floor cache is serialized as part of the save file.

## Floor 26: The Amulet

Floor 26 is a full dungeon floor, not a boss arena. It contains:
- Normal machines, encounters, monsters, and chests
- The **Amulet of Ascension** — placed in a secluded, dangerous location
- The **Escape Portal** — placed far from the amulet
- The player must explore and fight through the floor to retrieve the amulet
  and reach the portal

The amulet is a pickup item. The portal activates only when the player carries it.
