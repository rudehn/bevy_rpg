# Prefab RON Schema Reference

Complete reference for the `PrefabTemplate` RON format used in `assets/prefabs.ron`.

Source: `src/assets/mod.rs:243-271`

## PrefabTemplate Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | String | required | Unique identifier for this prefab |
| `width` | i32 | required | Tile width of the layout |
| `height` | i32 | required | Tile height of the layout |
| `min_floor` | i32 | required | Earliest floor this prefab can appear |
| `max_floor` | i32 | required | Latest floor this prefab can appear |
| `tiles` | Vec\<String\> | required | Row-major ASCII layout, one string per row |
| `props` | Vec\<PrefabPropEntry\> | `[]` | Decorative/interactive world objects |
| `structures` | Vec\<PrefabStructureEntry\> | `[]` | Special map structures with AI behavior |
| `monster_spawns` | Vec\<PrefabMonsterSpawn\> | `[]` | Monster positions with role assignments |
| `item_spawns` | Vec\<PrefabItemSpawn\> | `[]` | Item positions (specific or random) |
| `on_leader_death` | String | `""` | Squad behavior when leader dies |
| `flee_threshold` | f32 | `0.5` | HP ratio at which squad members attempt to flee |
| `placement` | String | `"any"` | Placement strategy |
| `allow_rotate` | bool | `true` | Enable 90/180/270 degree rotation |
| `allow_flip` | bool | `true` | Enable horizontal flip |

## Sub-Entry Formats

```ron
// PrefabPropEntry — place a prop at (x, y)
(x: 2, y: 1, prop: "barricade")

// PrefabStructureEntry — place a structure at (x, y)
(x: 3, y: 3, structure: "Goblin Totem")

// PrefabMonsterSpawn — spawn a monster role at (x, y)
(x: 1, y: 2, role: "melee_guard", behavior: Sentry)
(x: 3, y: 4, role: "ranged")                          // behavior defaults to Wander
(x: 5, y: 2, role: "melee_guard", behavior: Patrol([(5,2),(5,8),(10,8),(10,2)]))
(x: 7, y: 3, role: "melee_guard", behavior: Roam(( min: (0,0), max: (10,6) )))

// PrefabItemSpawn — spawn a specific item or random item at (x, y)
(x: 4, y: 1, item: Some("Iron Sword"))
(x: 4, y: 1, item: None)
```

**Coordinate system:** `(0, 0)` is the top-left corner of the tile grid. `x` increases rightward, `y` increases downward.

## Tile Characters

| Char | Meaning |
|------|---------|
| `#` | Wall |
| `.` | Floor |
| `+` | Door |
| ` ` | Unchanged (passthrough — keeps whatever the map already has) |

**Void space (` `)** preserves whatever the map already has at that
position. Use void to create non-rectangular shapes: L-shapes, crosses,
protrusions, campsites, compound structures with gaps between buildings.
For `wall` placement, void stays as rock. For `room` placement, void
keeps the existing room floor. Never place spawns on void tiles.

**Rules:**
- Tile row count must equal `height`
- Each row length must equal `width`
- Spawn coordinates (props, structures, monsters, items) must land on floor (`.`) or door (`+`) tiles, never on void or wall

## Valid Enum Values

### Monster Roles

Used in `PrefabMonsterSpawn.role`:

| Role | Description |
|------|-------------|
| `melee_guard` | Holds position, blocks approaches |
| `ranged` | Attacks from distance, stays behind cover |
| `brute` | High damage/HP, anchors the encounter |
| `caster` | Spell-based attacks, high priority target |
| `leader` | Squad leader, triggers on_leader_death behavior |
| `any` | Flexible slot, filled by whatever the faction provides |

### on_leader_death Values

| Value | Description |
|-------|-------------|
| `scatter` | Squad members scatter and act independently |
| `enrage` | Squad members become enraged (damage boost) |
| `fight_on` | Squad continues fighting normally |
| `flee` | Squad members attempt to flee |

> **Note:** `fight_on` and `flee` are defined in prefab data but currently fall through to `Nothing` in `squad.rs`. Document as intended vocabulary pending code fix.

### Monster Behavior (`behavior` field, default: Wander)

| Variant | RON Syntax | Description |
|---------|-----------|-------------|
| `Sentry` | `behavior: Sentry` | Hold spawn position, jitter within 3 tiles. Returns home after chasing. |
| `Patrol` | `behavior: Patrol([(x1,y1),(x2,y2),...])` | Walk waypoints in order, loop. Resumes from nearest waypoint after chase. |
| `Roam` | `behavior: Roam(( min: (x,y), max: (x,y) ))` | Random walk within bounding rectangle. |
| `Wander` | `behavior: Wander` (or omit field) | Random walk, no constraints. Default if `behavior` is absent. |

Coordinates are relative to prefab `(0, 0)` top-left. Automatically transformed on rotation/flip.

### Placement Values

| Value | Description |
|-------|-------------|
| `room` | Overlay into existing rooms |
| `wall` | Carve into solid walls, adds a door at the border |
| `chokepoint` | Place at corridor bottlenecks (max 1 per floor) |
| `landmark` | Large set-piece, stamped before room generation |
| `any` | Try both room and wall placement (default) |

## Valid Prop Names

Source: `assets/props.ron`

`candle`, `watchfire`, `totem_pole`, `barricade`, `barrel`, `small_chest`, `chest`, `small_red_chest`, `red_chest`, `fountain`, `corrupted_fountain`, `tyrants_offering`

**Blocking props** (impassable): `barricade`, `barrel`, `totem_pole`, `small_chest`, `chest`, `small_red_chest`, `red_chest`, `fountain`, `corrupted_fountain`, `tyrants_offering`

**Non-blocking props:** `candle`, `watchfire`

**Light-emitting props:** `candle`, `watchfire`

## Valid Structure Names

Source: `assets/structures.ron`

`Goblin Totem`, `Tyrant's Altar`, `Orc War Drum`, `Necromancer's Pillar`, `Spider Egg Sac`, `Explosive Barrel`, `Poison Mushroom`, `Healing Spring`, `Soul Anchor`, `Necrotic Obelisk`, `Void Rift`, `Warding Stone`, `Tyrant's Eye`

Structures have AI behavior (cast spells, faction-aware) and can be destroyed by the player.

## Size Categories & Budget System

**Budget:** 350 tiles per floor (`width * height` consumed per prefab placed)

**Padding:** 2 tiles minimum between prefabs

| Category | Tile Area | Placement Pass | Notes |
|----------|-----------|---------------|-------|
| Small | < 31 | Pass 2 | Fills remaining budget, randomly selected |
| Medium/Large | >= 31 | Pass 1 | Tactical landmarks, shuffled, one attempt each |
| Landmark | Large set-pieces | Before room generation | Major encounters |
| Chokepoint | Any size | Pass 0 | Max 1 per floor, corridor bottlenecks |

**Placement pass order:**
1. **Pass 0:** Chokepoint prefabs (max 1). Floor tiles must match existing floor/door, walls must match existing walls.
2. **Pass 1:** Medium/large prefabs (shuffled, one attempt each across all orientations).
3. **Pass 2:** Small prefabs (random selection until budget exhausted or 3 consecutive failures).

**Connectivity:** After every placement attempt, a flood-fill from the player start verifies all walkable tiles remain reachable. Failed connectivity → revert and try next candidate.

**Orientation:** Up to 8 unique orientations generated via rotation (0/90/180/270) and horizontal flip. Symmetric duplicates are removed. All coordinates (tiles, props, monsters, items, structures) are transformed.

## Faction Note

The `faction_tag` field appears in some prefab RON entries (e.g., Goblin Shrine uses `faction_tag: "goblin"`) but is NOT currently in the `PrefabTemplate` Rust struct — serde silently ignores it. The monster role resolution system picks factions from `monsters.ron` based on which factions can fill all required roles at the current depth. Faction-locking requires adding the field to the struct (tracked separately).

## Annotated Example

```ron
(
    name: "Sentry Post",
    width: 7,
    height: 6,
    min_floor: 1,
    max_floor: 8,
    placement: "room",
    tiles: [
        //  0123456    (x coordinates)
        "       ",  // y=0: unchanged border
        " ..... ",  // y=1: floor interior
        " ..... ",  // y=2: floor interior
        " ..... ",  // y=3: floor interior
        " ..... ",  // y=4: floor interior
        "       ",  // y=5: unchanged border
    ],
    props: [
        (x: 1, y: 1, prop: "barricade"),   // top-left cover
        (x: 2, y: 1, prop: "barricade"),   // extends barricade line
        (x: 1, y: 2, prop: "barricade"),   // L-shaped cover
        (x: 5, y: 1, prop: "chest"),       // reward at edge
    ],
    monster_spawns: [
        (x: 3, y: 3, role: "melee_guard", behavior: Sentry),  // holds position behind barricade
    ],
    on_leader_death: "scatter",
    flee_threshold: 0.4,
    allow_rotate: true,
    allow_flip: true,
)
```
