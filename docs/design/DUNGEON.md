# Dungeon Design

## Overview

The dungeon is 10 floors of procedurally generated maps. Each floor is built by
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
  -> HiddenDoorPlacer      (convert some doors to hidden doors)
  -> PrefabPlacer          (stamp hand-designed room layouts)
  -> DecorationPropagator  (pass 1: fill natural dungeon with decorations)
  -> MachinePlacer         (topology-gated encounters)
  -> ShrinePlacer          (progression shrines in secluded nooks)
  -> DecorationPropagator  (pass 2: fill edges around placed encounters)
  -> CandleSpawner         (place light sources on wall tiles)
  -> ItemSpawner           (place chests with loot)
  -> MonsterSpawner        (populate rooms with enemies)
  -> UnseenCuller          (remove tiles unreachable from player start)
  -> DistantExit           (place down-stairs far from player)
```

## Generation Style

The dungeon uses a mix of **room-based** and **cavernous** generation. Caverns
become more prominent on deeper floors.

| Floors | Style |
|--------|-------|
| 1-3 | Primarily room-based with small cave pockets |
| 4-6 | Hybrid — larger rooms, some cave corridors |
| 7-9 | More cavernous — organic open spaces mixed with rooms |
| 10 | Generated with constraints — includes Tyrant's throne room + normal dungeon content |

The `BrogueLikeBuilder` already supports room types and cave generation. The
ratio of rooms to caves shifts with depth via floor profile configuration.

### Floor 10

Floor 10 is generated with constraints, not hand-designed. It includes:
- The Tyrant's throne room (large, distinct area)
- Normal dungeon content: encounters, monsters, items, machines
- The player must explore and fight through the floor to reach the Tyrant

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
| HiddenDoor | No | Yes | Yes | Renders as wall until discovered |
| DownStairs | Yes | Yes | No | Descend to next floor |
| UpStairs | Yes | Yes | No | Return to previous floor |
| Empty | No | No | No | Void / off-map |

- `is_walkable()` requires both terrain and liquid to be walkable
- `is_passable()` is used for connectivity checks (doors count, liquids ignored)
- `is_opaque()` blocks FOV (walls, closed doors, hidden doors)

**Liquid types:**

| Liquid | Walkable | Notes |
|--------|----------|-------|
| None | Yes | Default |
| ShallowWater | Yes | Extinguishes burning status |
| DeepWater | Yes | Traversable but risky — items can be swept away |
| Lava | Yes | Instant death without fire resistance |

### Liquid Effects

**Shallow Water:**
- Extinguishes burning on any entity that steps in or starts a turn in it

**Deep Water:**
- **Traversable but risky.** Each step through deep water has a chance to sweep
  an item from the player's inventory. The item floats in a random direction
  until it lands on a non-deep-water tile (like Brogue's item-sweeping mechanic).
- Creates risk/reward decisions: shortcut through water and risk losing gear,
  or take the long way around.

**Lava:**
- **Instant death** for entities without fire resistance
- Entities with fire resistance: 15 HP/turn while standing in lava
- Exiting lava applies Burning (5 fire damage/turn for 5 turns) to
  non-fire-resistant entities
- Lava tiles provide **ambient light** but still require exploration to be
  visible. Once a lava tile has been seen, it remains lit on the map. Adjacent
  tiles benefit from the ambient glow.

## Lakes

Lakes are not randomly scattered — they are **thematic encounters**. Each lake
is a deliberate feature: a medium or large body of water or lava that the player
must navigate around, through, or across.

### Lake Placement

- Prefer **fewer, larger** lakes over many small disjoint puddles
- Lakes are placed as cohesive bodies using the organic blob algorithm
- Each floor gets 0-2 lakes depending on depth

### Water vs. Lava by Depth

| Floors | Lake Type |
|--------|-----------|
| 1-4 | Water only |
| 5-6 | Mostly water, lava begins appearing (20-40% lava chance) |
| 7-8 | Mixed (50/50 water and lava) |
| 9 | Predominantly lava (70-80% lava chance) |

Lava lakes on deeper floors create natural hazards that shape routing and
reward fire resistance gear.

## Hidden Doors

Some doors are converted to hidden doors that appear as walls until discovered.
Hidden doors create alternate paths and reward observant players.

### Placement Rules

- Only doors with high choke value (>= 20) are eligible — hiding a door that is
  the only connection to a region would make it inaccessible
- Depth-scaled conversion chance: **10% on floor 1, scaling up to 60%** on
  deeper floors
- Maximum 3 hidden doors per floor

### Discovery

- When the player is adjacent to a hidden door and it's in FOV, there's a 15%
  base chance per turn to discover it
- No stat or shrine boosts the discovery chance
- On discovery: converts to a normal Door, updates sprite, logs a message
- A future "Search" action could guarantee discovery of adjacent hidden doors

## Decorations

A Brogue-style system for spreading environmental decorations across the dungeon.
Decorations are **purely visual for now**. Future plans include some decorations
being burnable and some blocking visibility or movement.

### Decoration Types

| Decoration | Appears On | Placement Rule |
|-----------|-----------|---------------|
| Grass | Floor (floors 1-5) | BFS propagation from seeds |
| TallGrass | Floor (floors 1-5) | Chain from Grass (20% chance) |
| DeadGrass | Floor (floors 2-10) | BFS propagation |
| Rubble | Floor (all floors) | Wall-adjacent only |
| Moss | Floor (floors 1-8) | Near liquid only |
| Fungus | Floor (floors 3-10) | BFS propagation |
| Cobweb | Floor (floors 1-6) | Wall-adjacent corners only |
| Bloodstain | Floor (floors 3-10) | BFS propagation, small clusters |
| ScorchedEarth | Floor | Runtime only (fire damage aftermath) |

### Propagation Algorithm

1. For each decoration rule valid at this floor depth:
   - Roll seed count (scaled by floor profile density)
   - For each seed, find a valid tile matching terrain/adjacency requirements
   - BFS outward from seed with decaying probability
   - Each step: `next_chance = current_chance * propagation_decay`
   - Respects exclusion zones (prefab interiors, machine interiors, shrine nooks)

### Two-Pass Strategy

- **Pass 1** (before PrefabPlacer): Fills natural dungeon. Prefabs stamped later
  overwrite decorations in their footprint.
- **Pass 2** (after MachinePlacer + ShrinePlacer): Fills edges around placed
  encounters. Exclusion zones prevent decorating inside encounter interiors.

## Lighting

The game uses a custom lighting system (`src/map/light.rs`). Light sources are
placed by CandleSpawner in the builder pipeline.

### Design Goals

- **Consistent lighting across all floors.** Lighting does not scale difficulty.
  Every floor should feel well-lit enough to play comfortably.
- **Coverage depends on wall tiles.** Candles are placed relative to wall
  geometry, not as a fixed count. More walls = more candle placement
  opportunities = good coverage.
- **Most of the floor should be lit.** Dark pockets exist naturally in areas
  far from walls, but the majority of explorable space has light.

### Light Sources

- **Candles/torches** — placed on wall tiles by CandleSpawner
- **Lava** — provides ambient light on adjacent tiles (after exploration)
- **Watchfires** — placed in prefabs and machine encounters

## Floor Persistence

Floors persist in a **floor cache** when the player ascends or descends. Returning
to a previously visited floor restores its exact state (map, monsters, items,
decorations, door states). This enables backtracking for shrines, corruption
sites, and exploration.

The floor cache is serialized as part of the save file.

## Floor Structure

10 floors with escalating cave generation:
- Prefabs and machines appear on **all floors**
- Corruption Sites on floors 3-5, 5-7, and 7-9
- 3 shrines per floor (including spell shrines, which count against this budget)
- Floor 10 has normal content + Tyrant's throne room
- Difficulty scales via monster spawns, monster complexity, and liquid hazards

## Open Questions

1. **Floor visual identity** — Should different depth ranges have distinct
   color palettes, wall sprites, or ambient sound?
2. **Deep water item sweep chance** — What percentage per step? How far does
   the item float? Can it land in lava (destroyed)?
3. **Lake as encounter** — Should lakes have monsters associated with them
   (e.g., water monsters, lava-immune enemies guarding lava shores)?
4. **Decoration future mechanics** — Which decorations become burnable? Which
   block movement/visibility? (TallGrass blocks visibility? Fungus is burnable?)
5. **Cave generation tuning** — Exact room-to-cave ratio per floor tier for
   BrogueLikeBuilder configuration.
