# Overworld (Linear Floors)

## Overview

The game is a traditional descend-stairs roguelike. The player starts in
a **town hub** (floor 0), descends through **four forest floors**
(floors 1–4), and reaches the **cult temple** at the bottom (floor 5).
The Amulet of Yendor sits inside the temple sanctum; carry it back to
the town's central **Portal** to win.

> **Status:** linear-floor pivot. The earlier "3×3 overworld grid"
> design from the original milestone is fully removed. Monster spawns
> are re-enabled on forest floors via the Voronoi-cell spawner (see
> [SPAWNING.md](SPAWNING.md)); item spawns remain disabled — the amulet
> is the only item in the world.

## Floor index scheme

`Floor(u32)` indexes a single linear descent. Closed at `MAX_FLOOR`
(currently 5).

| Index | Map      | Theme                                 |
|------:|----------|---------------------------------------|
| 0     | Town     | Open hub with buildings + waterside piers |
| 1     | Forest 1 | Sparse outer woods, introductory      |
| 2     | Forest 2 | Slightly denser, packs begin          |
| 3     | Forest 3 | Deeper canopy, full pack tier         |
| 4     | Forest 4 | Densest old-growth; temple entrance hides here |
| 5     | Temple   | Cold-stone corridor + sanctum holding the Amulet |

Helpers live in [src/map/world.rs](../../src/map/world.rs):

- `FloorKind::{Town, Forest{depth}, Temple}` — classification
- `floor_kind(floor: u32) -> FloorKind` — closed at `MAX_FLOOR`
- `FloorTheme::{Dungeon, Town, Forest, Temple}` — renderer overrides

## Transitions

Every floor change uses a regular `>` or `<` stair tile. A single
system, `player_transition_system`
([src/map/dungeon.rs](../../src/map/dungeon.rs)), handles every move.
Resolution order on each player position change:

1. The tile has a `MapExitTile` component → fire `MapTransitionMessage`
   with the component's destination floor + position. (Currently unused
   in the linear-floor world — kept available for future fast-travel /
   scripted teleporters.)
2. Bare `DownStairs` terrain → `MapTransitionMessage { floor + 1, None }`.
3. Bare `UpStairs` terrain → `MapTransitionMessage { floor - 1, None }`.
4. Terrain is `Portal` + the player has a `QuestItem` (the amulet) →
   Victory.

The default destination position is decided by the materializer:
descending → land on the destination's `UpStairs`, ascending → land on
its `DownStairs`.

## Builder pipelines

`floor_builder()` in [src/map/builders/mod.rs](../../src/map/builders/mod.rs)
dispatches on `FloorKind`:

```
FloorKind::Town          → town_builder()    (floor 0)
FloorKind::Forest { .. } → forest_builder()  (floors 1..=MAX_FLOOR-1)
FloorKind::Temple        → temple_builder()  (floor MAX_FLOOR)
```

### Town (floor 0)

See the **Town** section of [CLAUDE.md](../../CLAUDE.md) for the full
builder list. The town hub has:

- Open Floor with a few themed buildings (Pub, Smithy, Alchemist, etc.)
- A waterside on the west edge with piers
- A central `Portal` tile (win-condition return)
- A `DownStairs` on the east border → Forest 1
- An organic A*-style dirt road network connecting everything
- Townsfolk NPCs placed per `town_npcs.ron` (drunken sailors today)

### Forest (floors 1..=`MAX_FLOOR - 1`)

Each forest floor uses the same builder chain:

```
ForestTerrainBuilder   → cellular automata + east-west spine + end-clearings
ForestStairsBuilder    → UpStairs west, DownStairs east (or off-spine on Forest 4)
VoronoiSpawner         → drops monster packs per Voronoi cell
DecorationPropagator   → grass, foliage, rubble — density scales with depth
```

`ForestTerrainBuilder::profile_for_depth` tunes the CA per depth:

| Depth | Initial alive % | CA rounds | Feel                |
|------:|----------------:|----------:|---------------------|
| 1     | 50              | 4         | Sparse, open paths  |
| 2     | 54              | 4         | Scrubbier underbrush |
| 3     | 58              | 5         | Thicker canopy      |
| 4     | 62              | 5         | Gnarly old-growth   |

Decoration density follows the same ramp (`0.20 → 0.40` across the four
floors).

**Stair placement** in `ForestStairsBuilder`:

- **Forest 1–3**: standard — `UpStairs` at the west clearing,
  `DownStairs` at the east clearing. The east-west corridor (the
  "spine") connects them through the map centre.
- **Forest 4** (the deepest forest floor, `MAX_FLOOR - 1`): the
  east clearing has **no** stair. Instead, the `DownStairs` to the
  temple is placed at a random walkable Floor tile that's at least
  `TEMPLE_ENTRANCE_MIN_DY` (6 tiles) off the spine. The player must
  wander off the corridor to find it — the temple is "discoverable",
  not handed to them. Fallback: if no off-spine tile qualifies, the
  entrance lands at the east clearing.

### Temple (floor `MAX_FLOOR`)

A sealed stone interior. The whole map starts as solid wall; the
builder carves out:

- A **3-tile-tall east-west corridor** running from `CORRIDOR_INSET`
  (8 tiles from the west border) to the mirror inset on the east.
- A **7×7 sanctum chamber** centred on the east end of the corridor.

[`TempleLayoutBuilder`](../../src/map/builders/temple.rs) handles the
carve. [`TempleStairsBuilder`](../../src/map/builders/temple.rs) stamps:

- `UpStairs` at the corridor's west end → returns to Forest 4.
- The **Amulet of Yendor** at the sanctum centre (queued onto
  `item_spawn_list`, not a terrain tile, so normal pickup applies).

No monster spawn table yet — cultists arrive in a future pass. The
shape is also designed to descend further: future content can drop a
`DownStairs` in the sanctum to add temple sub-levels without
restructuring the floor.

## Floor theming

`FloorTheme::{Dungeon, Town, Forest, Temple}` is a resource read by the
ASCII renderer ([src/map/ascii_renderer.rs](../../src/map/ascii_renderer.rs))
to override the base Wall/Floor glyph + colour without introducing new
`TerrainType` variants. Stairs, doors, portals, and decorations stay
manifest-driven.

`spawn_dungeon` writes the theme on every floor materialisation based on
`floor_kind(floor.0)`. Forest walls render as `♣` (green), town walls as
`▓` (brown), temple walls as `▒` (cold grey).

## Quest item

Only one item ships in the world today: the **Amulet of Yendor** in the
temple sanctum. Definition lives in [assets/items.ron](../../assets/items.ron)
with `is_quest_item: true`; the `QuestItem` marker component is what the
town portal's victory check tests for.

## Save / restore

Each visited floor is snapshotted into a floor cache when the player
leaves, and restored verbatim when they return. The save file persists
the cache (see [SAVE.md](SAVE.md)). Schema is unchanged by the temple
expansion — the temple is just one more floor index in the same scheme.

## Out of scope (for now)

- Cultists on the temple floor — the spawn table will gain entries
  once the cult roster is designed.
- A real temple visual + prop pass (altars, braziers, statuary).
- A "deeper temple" — the current floor is one map; descending further
  is a deliberate future hook, not currently reachable.
- Item drops other than the amulet — item spawning remains disabled
  pending the loot pass.
