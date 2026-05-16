# Overworld

## Overview

The game now opens in a **town hub**, the center of a 3×3 overworld grid. The
8 surrounding tiles are forests; one of them (chosen randomly at the start of
each run) holds the **Temple entrance** — a 3-floor mini-dungeon that ends with
the **Amulet of Yendor**. Returning to the town's plaza portal with the amulet
wins the run.

```
   NW   N    NE
    1   2    3
   W           E
    4  [Town] 5
   SW   S    SE
    6   7    8

   Town (floor 0)  →  Forest ring (floors 1..=8)  →  Temple (floors 9..=11)
```

> **Status:** Phase 1 — terrain, theming, transitions, and quest item ship in
> this milestone. **No monsters, items, chests, or NPCs** spawn in the
> overworld or the temple yet. The amulet is the only item in the world.
> Content returns in a later milestone.

## Floor index scheme

`Floor(u32)` is reused — no enum, no schema split. The scheme is closed at 12
entries; anything outside `0..=11` is treated as a legacy dungeon floor.

| Index | Map        |
|------:|------------|
| 0     | Town       |
| 1     | Forest NW  |
| 2     | Forest N   |
| 3     | Forest NE  |
| 4     | Forest W   |
| 5     | Forest E   |
| 6     | Forest SW  |
| 7     | Forest S   |
| 8     | Forest SE  |
| 9     | Temple 1   |
| 10    | Temple 2   |
| 11    | Temple 3 (Amulet) |

Helpers live in [src/map/world.rs](../../src/map/world.rs):

- `FloorKind::{Town, Forest(GridDir), Temple(u8)}` — classification
- `GridDir` + `mirror_dir` + `neighbor` — 3×3 grid topology
- `edge_position(GridDir)` / `arrival_position(GridDir)` — deterministic
  per-edge coordinates so transitions don't need a routing resource

## Transitions

Every map change is a **regular `>` or `<` stair tile** — no edge walking,
no special map-border tiles. The 3×3 topology is encoded by placing
multiple stairs on the town map (one per direction) and tagging each
with a `MapExitTile` component carrying the explicit destination floor.

A single system, `player_transition_system`
([src/map/dungeon.rs](../../src/map/dungeon.rs)), handles every move.
Resolution order on each player position change:

1. The tile at the player's new position has a `MapExitTile` component →
   fire `MapTransitionMessage` with the component's destination floor +
   position. Used by all overworld + temple stair tiles.
2. Bare `DownStairs` terrain (no `MapExitTile`) → `MapTransitionMessage
   { floor + 1, None }`. Used by temple-1 → temple-2 and temple-2 → temple-3.
3. Bare `UpStairs` terrain (no `MapExitTile`) → `MapTransitionMessage
   { floor - 1, None }`. Used by temple-3 → temple-2 and temple-2 → temple-1.
4. Terrain is `Portal` + the player has a `QuestItem` → Victory.

`MapExitTile { destination_floor: u32, destination_pos: Option<Position> }`
sits on the 8 town `DownStairs` (one per direction), each forest's single
`UpStairs` (back to the town stair the player came in on), the
temple-entrance forest's `DownStairs` (to temple 1), and temple-1's
`UpStairs` (back to the forest entrance). `destination_pos: None` defers
to the materializer's default arrival (UpStairs when descending, DownStairs
when ascending); `Some(pos)` lands the player exactly there. The forest
→ town arrow needs `Some(town_stair_pos)` because town has 8 DownStairs
and the default heuristic can't pick which one.

## Builder pipelines

`floor_builder()` in [src/map/builders/mod.rs](../../src/map/builders/mod.rs)
dispatches on `FloorKind`:

```
FloorKind::Town       → town_builder()        (floor 0)
FloorKind::Forest(_)  → forest_builder()      (floors 1..=8)
FloorKind::Temple(_)  → temple_builder()      (floors 9..=11)
floor >= 12           → legacy dungeon pipeline (kept for backwards-compat,
                        not currently reached in a fresh run)
```

### Transition placement (all overworld maps)

Every overworld map carries 4 stair tiles per cardinal exit, clustered
side-by-side at the centre of the corresponding border. The K-th stair
on a map's N border pairs with the K-th stair on its destination's S
border (and likewise E↔W) — walking off one map lands the player at
the matching slot in the destination so the world reads as continuous.

Cluster positions (80×60 map):

| Border | X coords     | Y coord |
|--------|--------------|---------|
| N      | 38, 39, 40, 41 | 1     |
| S      | 38, 39, 40, 41 | 58    |
| E      | 78           | 28, 29, 30, 31 |
| W      | 1            | 28, 29, 30, 31 |

Town stairs always render as `>` (DownStairs). Forest stairs render as
`<` (UpStairs) on the border facing town, and `>` on borders facing
lateral forest neighbours. Temple stairs are normal `>` / `<` with no
`MapExitTile` (they use `floor ± 1`).

### Town

- `TownLayoutBuilder` — fills the entire map with open Floor, then
  scatters up to 8 small buildings (walls + interior + one door facing
  the centre). Reserves keep-out zones around each of the 16 border
  stair positions and an inland corridor so no building blocks them.
- `TownBorderStairsBuilder` — stamps 16 `>` stair tiles (4 per
  N/S/E/W border) clustered side-by-side. Each is tagged with a
  `MapExitTile { destination_floor: <neighbour>, destination_pos:
  Some(<mirror K-th>) }`.
- `TownPathBuilder` — paints a dirt-path network: a 4-wide cross
  through the centre joining the N/S clusters and the E/W clusters,
  plus an L-shaped connector from each building's door to the cross.
  Paths are `Decoration::Custom { id: TOWN_PATH_DECO_ID }`; the
  renderer in [`themed_tile_display`](../../src/map/world.rs)
  substitutes a packed-dirt glyph + colour. Underlying terrain stays
  `Floor` so movement and other systems work normally.
- `TownPortalBuilder` — places the win portal at the map centre.

### Forest

- `ForestTerrainBuilder` — cellular automata using the **open-cave rule**
  (60% fill, 4 rounds, birth=4, survive=3). Hardened against degenerate
  RNG seeds: retries up to 8 times until the largest connected region
  covers ≥25% of the map; always carves a 5×5 walkable clearing at the
  centre; tunnels a 1-tile corridor if the clearing ends up isolated;
  finishes with a BFS-from-centre cull so the player can never step
  into a dead pocket.
- `ForestBorderStairsBuilder` — for each valid cardinal exit (2 for
  corner forests, 3 for cardinal forests), stamps 4 border stairs
  clustered side-by-side. Stairs heading back to town are `<`;
  stairs heading to lateral forest neighbours are `>`. Each is paired
  with the K-th destination stair via `MapExitTile.destination_pos`.
  Tunnels a 1-tile-wide path from each stair into the forest centre
  so the player can walk from any stair into the interior.
- `TempleEntranceBuilder` (conditional) — runs only on the chosen
  entrance forest tile. Picks the walkable tile most distant from
  the centre that is at least 4 tiles inside the border and not
  already on a stair, stamps `DownStairs` + `MapExitTile {
  destination_floor: 9 }` on it.

The chosen entrance forest is `OverworldState.temple_entrance_floor`, seeded
once per run on `OnEnter(AppState::InGame)` (preserved across reloads from
save schema v6+).

### Temple

- `BrogueLikeBuilder` — reused with a slightly looser profile (cavernous
  ruins).
- `StartPointBuilder` — places `UpStairs` at the player start when depth > 1.
- `DistantExit` — places `DownStairs` on temples 1 and 2.
- `AmuletPlacerBuilder` (temple 3 only) — pushes the Amulet of Yendor onto
  `item_spawn_list` at the most distant walkable tile. Replaces `DistantExit`
  on the deepest floor.
- `TempleUpstairsLinker` (temple 1 only) — stamps a `MapExitTile` on the
  UpStairs pointing back to the forest entrance position from
  `OverworldState.temple_entrance_pos`.

## Floor theming

`FloorTheme::{Dungeon, Town, Forest, Temple}` is a resource read by the ASCII
renderer ([src/map/ascii_renderer.rs](../../src/map/ascii_renderer.rs)) to
override the base Wall/Floor glyph + colour without introducing new
`TerrainType` variants. Stairs, doors, portals, and decorations stay
manifest-driven.

`spawn_dungeon` writes the theme on every floor materialisation based on
`floor_kind(floor.0)`. Forest walls render as `♣` (green), town walls as `▓`
(brown), temple walls as `#` (mossy grey). Dungeon (the legacy fall-through)
keeps the existing palette.

## Quest item

Only one item ships in the world right now: the **Amulet of Yendor** on
temple 3. Definition lives in [assets/items.ron](../../assets/items.ron) with
`is_quest_item: true`; the `QuestItem` marker component is what the town
portal's victory check tests for.

## Save / restore

Schema bumped to v6 (additive). New persistent state:

- `GameSaveData.overworld: OverworldSave` — `temple_entrance_floor` plus
  `temple_entrance_pos`. The `OnEnter(AppState::InGame)` reseed system skips
  rerolling when a save load is pending, so a reloaded run keeps its temple
  in the same forest tile.
- `SavedFloorData.exit_tiles: Vec<SavedExitTile>` — `MapExitTile` markers per
  floor. Restored by the materializer when a cached floor is revived.

Older saves (v5 and earlier) load fine — both fields are `#[serde(default)]`
and the v5→v6 migration is a no-op.

## Out of scope (for now)

- NPCs in town (quest giver, shopkeepers)
- Monster / item spawns in the overworld or temple
- Visual sprites for tile rendering — themed glyphs only (ASCII)
- World-edge guard rails — corner forest tiles simply have no exits in the
  outward directions, so the player can never walk off the world
