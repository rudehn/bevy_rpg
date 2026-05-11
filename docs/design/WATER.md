# Water

Water is a tactical terrain layer that reshapes movement, combat, and item
control. It is *passable but expensive* for the land-bound, and *home
territory* for aquatic monsters. It interacts with fire (producing
steam), with the player's status effects (extinguishing Burning), and
with loose items (drift). Water is generated alongside lava and chasms by
`LakeBuilder` (see `DUNGEON.md`).

## Design Philosophy

- **Slow but safe for land creatures.** No drowning timer, no swim skill —
  the cost is doubled action time and the risk of losing inventory to
  currents.
- **Aquatic monsters can ONLY occupy water.** Eels do not flop on land;
  they do not chase the player onto dry tiles. Water is both a barrier
  and an enemy reservoir.
- **Antithesis of fire.** Any fire-water interaction produces steam.
  Water is never consumed by fire.
- **Water moves things.** Items dropped or swept into deep water drift on
  the current and may be lost into a chasm before they can be retrieved.

## Data Model

### `LiquidType` (engine, `roguelike_engine/src/map/tile.rs:91-103`)

Tiles carry a `liquid` overlay independent of their `terrain`:

| Variant | Walkable (`is_walkable`) | Pathing blocker | Notes |
|---|---|---|---|
| `None` | yes | no | Default, dry tile |
| `ShallowWater` | yes | no | Flavor liquid; extinguishes Burning |
| `Water` (deep) | yes | **yes** | The "water" of this doc; 2x move cost |
| `Lava` | no | yes | Instant death without fire resistance |
| `Chasm` | no | yes | Impassable void (see `CHASMS.md`) |

The codebase calls deep water `LiquidType::Water` (not `DeepWater`).
Shallow water is a separate variant, and only `Water` triggers the
deep-water cost and item-sweep logic.

### Components

- **`Submerged`** (`src/components.rs:79`) — marker for aquatic monsters
  hiding beneath the surface. Removed before any attack.
- **`Drifting`** (`src/components.rs:68`) — marker for an item floating
  in deep water. Advances one tile per turn.
- **`MovementMode`** (engine, `roguelike_engine/src/components/movement_mode.rs`):
  - `Land` (default) — pays deep-water tax, can leave water
  - `ImmuneToWater` — ignores deep-water cost
  - `RestrictedToLiquid` — can ONLY enter tiles with non-`None` liquid

### Resources & Plugin

- **`WaterTiles`** (`src/game/water.rs:21`) — `HashMap<(i32, i32), LiquidType>`
  rebuilt on floor load (`src/map/dungeon.rs:661-680`); drives the
  shimmer animation.
- **`WaterPlugin`** (`src/game/water.rs:24`) registers three turn-driven
  systems gated to `AppState::InGame`:
  `deep_water_item_sweep_system` → `item_drift_system` →
  `water_extinguish_system`. Each drains `TurnEndEvent` once per frame
  so multiple queued turns collapse into a single tick (prevents items
  teleporting).

## Movement Model

### Cost Table

Movement cost is computed in `handle_movement` in
`src/game/actions.rs:588-610`. `BASE_ACTION_COST` is 100
(`roguelike_engine/src/constants.rs:39`); deep water doubles it for Land
actors and emits "The deep water slows your movement." to the player log.

| Actor | Tile | Cost | Comment |
|---|---|---|---|
| Land | Floor | 100 | Baseline |
| Land | ShallowWater | 100 | No tax — flavor only |
| Land | Water (deep) | **200** | 2x; logs "slows your movement" |
| Land | Lava | n/a | `is_walkable` false; bump rejected |
| Land | Chasm | n/a | Confirmation dialog → fall |
| ImmuneToWater | Water | 100 | No tax |
| RestrictedToLiquid | Floor | n/a | Cannot enter; pathfinding rejects |
| RestrictedToLiquid | Water/ShallowWater | 100 | Their normal terrain |

### Tile Entry (`can_entity_enter_tile`)

Mode-aware predicate (`roguelike_engine/src/map/tile.rs:304-312`):

```rust
pub fn can_entity_enter_tile(tile: Tile, mode: MovementMode) -> bool {
    match mode {
        MovementMode::Land | MovementMode::ImmuneToWater => is_walkable(tile),
        MovementMode::RestrictedToLiquid => {
            tile.liquid != LiquidType::None && is_walkable(tile)
        }
        _ => is_walkable(tile),
    }
}
```

Eels (canonical aquatic monster, `assets/monsters.ron:95-106`) carry
`movement_mode: RestrictedToLiquid` and are constrained to liquid for
both bump-movement and pathfinding (`pathfind_toward` in
`src/game/ai.rs:695-704`).

`update_mode` in `src/game/ai.rs:215-225` adds a chase-give-up bonus:
when a Land monster's last-known player position is on deep water,
`chase_distance` increments by `+3` per turn instead of `+1`, so they
drop the chase faster — pursuit through water is unrealistic.

## Submerged State

Aquatic monsters in their home tile hide until they break surface to
attack. Recomputed each AI tick in `execute_monster_ai`
(`src/game/ai.rs:67-80`):

```rust
if movement_mode == MovementMode::RestrictedToLiquid {
    let on_liquid = map.tiles[idx].liquid != LiquidType::None;
    if on_liquid && !has_adjacent_enemy(entity, ctx.monster_pos, world) {
        commands.entity(entity).insert(Submerged);
    } else {
        commands.entity(entity).remove::<Submerged>();
    }
}
```

A `surface(...)` closure (`ai.rs:83-85`) is called immediately before any
attack action — ability use, ranged, melee, flee, kite — so the player
sees the eel before it bites.

### Rendering Hooks

`update_monster_visibility` (`src/game/systems.rs:29-53`) sets
`Visibility::Hidden` for any monster carrying `Submerged`, regardless of
FOV. The ASCII renderer (`src/map/ascii_renderer.rs:220`) excludes them
from the glyph layer; ranged attacks (`src/game/ranged.rs:82`) and staff
zaps (`src/game/staves.rs:351`) reject submerged targets. The save
system (`src/save/mod.rs:618`, `src/map/floor_materializer.rs:534`)
persists `Submerged` so a saved eel reloads still hidden.

## Item Drift

### Sweep (`deep_water_item_sweep_system`)

`src/game/water.rs:42-90`. For every actor standing in a
`LiquidType::Water` tile, each item in their inventory has a 50% chance
per turn to be ejected (`InInventory` removed; `Position`,
`FloorEntityMarker`, `Drifting` inserted; logged "Your {item} is swept
away by the current!"). The sweep runs against any actor — `ImmuneToWater`
is not exempt by code, only by avoiding water.

### Drift (`item_drift_system`)

`src/game/water.rs:96-157`. Each turn, every `Drifting` item picks a
random walkable adjacent tile from the 8-neighborhood:

- **Chosen tile is a chasm:** log "The {item} falls into the chasm!" and
  despawn (matches `CHASMS.md` rule).
- **Chosen tile is non-water:** drop `Drifting`, item rests on the new
  tile (washed ashore).
- **Chosen tile is more deep water:** keep `Drifting`, move there.
- **No candidates (landlocked):** drop `Drifting` in place.

8-directional offsets allow items to round corners through wider lakes.
Combined with chasm destruction this yields the Brogue-style threat: an
item swept into a deep-water lake adjacent to a chasm has finite turns
to be retrieved.

## Fire Interaction

Direction is **fire → water = steam, water wins**. Water is never
consumed; fire is suppressed. The conversion appears in three places,
all one-way:

1. **Fire spreading onto water** (`fire.rs:91-95` in `fire_tick_system`):
   adjacent fire produces a 500-volume Steam puff instead of igniting;
   the water tile is unchanged.
2. **Direct ignition on water** (`fire.rs:230-239` in `ignite_tile_at`):
   a fire ability or runic targeting water spawns 500-volume steam and
   returns `false`.
3. **Burning creature stepping into water** (`water.rs:160-191` in
   `water_extinguish_system`): both `Water` and `ShallowWater` count.
   `StatusEffectKind::Burning` is removed, log "The water extinguishes
   the flames!", and a 300-volume Steam burst spawns at the entity.

Steam itself is documented in `GAS.md`. Mechanically it is a thin gas
that scalds non-fire-resistant entities
(`StatusEffectKind::Burning`, duration 3, magnitude 2;
`src/game/gas.rs:47`).

## Player Extinguish

Stepping into either `Water` or `ShallowWater` strips Burning via the
same `water_extinguish_system`. It runs every turn end on every entity
with `Position` + `StatusEffects`, not just the player. A burning player
who drops into a shallow puddle loses Burning at no movement cost
(puddles do not levy the deep-water tax).

## Lighting, Shimmer, and Generation

Water tiles animate with a Brogue-style "color dancing" shimmer (see
`DUNGEON.md` "Water Shimmer") — a per-tile color tint applied during
rendering with **no gameplay effect**. `WaterTiles` is the spatial
index, rebuilt from the live `Map` on every floor load
(`src/map/dungeon.rs:673-678`).

Water/lava/chasm placement is the job of `LakeBuilder` — see
`DUNGEON.md` "Lakes". Terrain definitions live in
`assets/tiles.ron:73-112`; `Water`, `ShallowWater`, `Lava`, and `Chasm`
are separate manifest entries each with their own ASCII glyph and color.
`MonsterSpawner` honors `MovementMode`: aquatic monsters cluster into
liquid-only candidate cells, so eels never spawn on dry floor.

## Edge Cases & Resolved Decisions

- **No drowning / no swim timer.** Player can stand in deep water
  indefinitely; cost is move tax + inventory loss, not HP loss.
- **No "flopping fish" gameplay.** `RestrictedToLiquid` is hard. An eel
  knocked or teleported onto dry land cannot path back
  (`can_entity_enter_tile` rejects the tile) and sits still until
  re-watered externally. No out-of-water timer or auto-death.
- **Land monster teleported into deep water.** Tile is `is_walkable`, so
  the monster pays 2x move cost. Deep water is a pathing blocker, so
  its AI will try to walk back to dry land.
- **Fire spread suppressed on water.** Adjacent fire never converts the
  tile or removes the liquid; the fire fizzles into steam.
- **Items in water survive the floor cache.** Drift only runs in
  `AppState::InGame`. Items on a cached floor freeze in place —
  `Drifting` persists but no ticks fire until re-materialization.
- **Steam is one-way.** Steam never condenses back to water; gas
  decays via the standard gas system.
- **Sweep ignores `ImmuneToWater`.** Sweep checks tile liquid, not
  actor mode. No player class currently has `ImmuneToWater`, so this
  is benign.

## Cross-Links

- `DUNGEON.md` — terrain layer, lake placement, shimmer rendering, lava
- `CHASMS.md` — drifting items destroyed in chasm tiles
- `GAS.md` — Steam gas mechanics, scalding damage
- `STATUS_EFFECTS.md` — Burning extinguish behavior
- `ENEMIES.md` — Eel and other aquatic monster definitions
- `TURNS.md` — `TurnEndEvent` cadence used by water systems

## Key Files

- `src/game/water.rs` — sweep, drift, extinguish; `WaterTiles` resource
- `src/game/fire.rs` — fire→steam conversion (lines 91-95, 230-239)
- `src/game/actions.rs` — deep-water move cost (lines 602-610)
- `src/game/ai.rs` — submerge/surface logic (lines 67-85)
- `src/game/systems.rs` — `update_monster_visibility` for `Submerged`
- `src/components.rs` — `Submerged`, `Drifting` markers
- `src/map/dungeon.rs` — `WaterTiles` rebuild on floor load
- `roguelike_engine/src/map/tile.rs` — `LiquidType`, `can_entity_enter_tile`
- `roguelike_engine/src/components/movement_mode.rs` — `MovementMode` enum
- `assets/tiles.ron` — Water / ShallowWater / Lava / Chasm manifest entries
