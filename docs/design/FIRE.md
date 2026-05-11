# Fire

Fire is a dynamic terrain hazard, not a damage source attached to an actor. It spreads, decays, ignites flammable terrain and decorations, lights tiles, converts adjacent water into steam, and detonates poison gas. The player exploits fire as much as they fear it: a Staff of Fire dropped into a corridor of tall grass can clear a room.

---

## Design Philosophy

Fire is **stateful terrain**, not an effect. It exists as ECS entities with positions, lifetimes, and light. Treating fire as a tile-state hazard (rather than a creature or projectile) means:

- The same fire that burns a goblin will burn the player who walks into it.
- Lighting, FOV, and gameplay damage all read from the same source of truth (`FireTiles`).
- Fire propagates with simple rules — no separate "fire AI."
- Fire eventually **goes out**. There is no permanent fire on a floor; embers cool to ash.

The player should learn:

- **Tall grass and cobwebs are kindling.** A fire bolt into TallGrass becomes a fire-storm.
- **Water makes steam.** Steam burns. Fighting on a wet floor is dangerous in a different way.
- **Poison gas plus flame is a bomb.** A Staff of Fire shot into a fungus-emitted gas cloud detonates the whole room.
- **Fire passes.** Wait it out — but it leaves embers and possibly ash, which can change pathing decoration.

Fire is **cosmetically lit** but the lighting is not load-bearing — fire visibility comes from the gameplay state, not from the renderer.

---

## Data Model

### Entities & Components

| Type | Kind | Purpose |
|------|------|---------|
| `FireMarker` | Component | Tag for fire entities. |
| `Position` | Component | Tile coordinates of the fire. |
| `FloorEntityMarker` / `GameEntityMarker` | Component | Cleanup tags on floor change / game over. |
| `FireTiles(HashSet<(i32, i32)>)` | Resource | Spatial index — `O(1)` "is this tile on fire?" without iterating fire entities. |

A fire is a real entity (`spawn_fire`, `src/game/fire.rs:185`). It has a `Position` but **no sprite of its own** — fire rendering is handled by an animation system reading `FireTiles`, and ASCII rendering reads the same set (`src/map/ascii_renderer.rs:154`). The entity exists primarily to:

1. Tie a `FloorEntityMarker` lifetime to the burning tile.
2. Register and unregister a `LightSource` cleanly.
3. Allow the `fire_tick_system` to query fire positions deterministically.

### Tunable Constants (`src/game/fire.rs:23-28`)

| Constant | Value | Meaning |
|----------|-------|---------|
| `FIRE_DECAY_CHANCE` | 20 | Per-tile chance out of 100 of going out each turn. ~5 turn average lifetime. |
| `BURN_DURATION` | 5 | Turns of `Burning` status applied to a creature standing in fire. |
| `BURN_DAMAGE` | 3 | Per-turn damage of the `Burning` status applied by fire. |
| `FIRE_LIGHT_RADIUS` | 15.0 | Light radius in tiles. |
| `FIRE_LIGHT_INTENSITY` | 1.0 | Light intensity (input to `bevy_light_2d`). |
| `FIRE_LIGHT_COLOR` | `[1.0, 0.4, 0.1]` | Warm orange. |

---

## Spread System

`fire_tick_system` (`src/game/fire.rs:44`) runs once per `TurnEndEvent`. It is structured in four passes so that one fire's decay cannot influence its own spread within the same turn:

```
Pass 1: For each burning tile:
          if rand(0,100) < FIRE_DECAY_CHANCE -> mark for decay, skip spread
          else                              -> attempt spread to 4 cardinal neighbors
Pass 2: Apply decay (despawn entity, remove light, place Embers)
Pass 3: Spawn new fires (consumes decoration, normalizes terrain to Floor)
Pass 4: Apply Burning status to creatures standing in fire (refresh-only)
```

### Spread Probability — Read From the *Target* Tile

Spread chance is the **target tile's** flammability — the tile being ignited, not the burning tile. The fire system reads from the engine's `Decoration::flammability()` and `TerrainType::flammability()` and takes the **max** of the two (`src/game/fire.rs:99-103`):

```rust
let flammability = ntile.decoration.flammability()
    .max(ntile.terrain.flammability());
if flammability > 0 && rng.range(0, 100) < flammability as i32 {
    new_fires.push((nx, ny));
}
```

### Flammability Table (engine: `roguelike_engine/src/map/tile.rs`)

**Decoration** (`tile.rs:172-185`):

| Decoration | Flammability (0-100) |
|------------|----------------------|
| Cobweb | 100 |
| TallGrass | 75 |
| DeadGrass | 60 |
| Grass | 50 |
| Fungus | 40 |
| TrampledGrass | 40 |
| Moss | 30 |
| TrampledFungus | 30 |
| Embers / Ash / others | 0 |

**Terrain** (`tile.rs:62-67`):

| Terrain | Flammability |
|---------|--------------|
| Door / OpenDoor | 20 |
| All other | 0 |

A Cobweb tile **always** ignites if a fire is adjacent. A Door has a 20% chance per turn per adjacent burning tile. Plain stone floor never spreads fire — fire chains are limited to vegetative decoration corridors (the tactical signal to the player).

### Spread Direction

Cardinal-only (4-neighbor). No diagonal spread — keeps fire chains visually predictable along corridors and rooms.

---

## Ignition Helper — `ignite_tile_at`

Used by the Staff of Fire and similar effects (`src/game/staves.rs:523`). Given a target tile:

1. If already burning → no-op (returns `false`).
2. If liquid is `Water` or `ShallowWater` → spawn `Steam` gas (vol 500), no fire (returns `false`).
3. If liquid is anything else non-`None` (Lava, Chasm) → no-op.
4. Compute `flammability = max(decoration, terrain)`. If 0 → no-op.
5. Otherwise: spawn fire entity, register light, consume flammable decoration (set to `None`), and if terrain was flammable (a Door) demote terrain to `Floor`.

This is the single chokepoint for "make this tile catch fire." All staves, abilities, and on-hit effects route through `ignite_tile_at` so the rules above apply uniformly.

---

## Creature Ignition

Fire never directly damages creatures. Instead, fire applies the **`Burning` status effect** to any creature whose `Position` is in `FireTiles`:

- Duration: `BURN_DURATION = 5` turns
- Magnitude: `BURN_DAMAGE = 3` per tick
- Resistance: respected via the standard status-effect pipeline (`is_fire_resistant`)

Application is **refresh-only**, not stacking (`src/game/fire.rs:153-163`):

```rust
if fire_tiles.0.contains(&(pos.x, pos.y)) && !effects.is_burning() {
    effects.add_effect_with_magnitude(Burning, BURN_DURATION, BURN_DAMAGE, None);
}
```

A creature already on fire who steps onto another fire tile does *not* re-roll the duration. This avoids "fire treadmill" infinite ticks when an actor is panicking around a burning room.

---

## Decay & Embers

Each burning tile rolls `FIRE_DECAY_CHANCE` (20%) every turn before considering spread. On decay (`src/game/fire.rs:120-128`):

1. Remove `(x, y)` from `FireTiles`.
2. Remove the light source at that tile.
3. Emit a `DecorationMutationMessage` setting decoration to `Decoration::Embers`.
4. Despawn the fire entity.

**Embers have flammability 0**, so a tile that just burned cannot re-ignite from a neighbor on the same cycle — fire chains are guaranteed to terminate. Embers themselves carry an engine-side timed promotion to `Decoration::Ash` (`tile.rs:218-220`, `chance_per_turn: 1000`, ~10% per turn), so the visual scar fades over time.

Average fire lifetime: with a 20% decay chance per turn, expected lifetime is 5 turns (geometric distribution). Long enough to spread one or two tiles down a corridor, short enough that the player can wait it out.

---

## Light Source

Every fire entity registers a `LightSourceData` in the global `LightSources` resource on spawn (`src/game/fire.rs:199-205`):

- Radius 15 tiles, intensity 1.0, color warm orange `[1.0, 0.4, 0.1]`.
- The light is removed in the decay pass — light and gameplay state are kept consistent through the same spawn/despawn pair.

Fire light is **cosmetic + readability only**. It does not extend FOV, does not reveal hidden enemies, and is not used for stealth checks. The renderer reads `LightSources` for atmospheric lighting; gameplay reads `FireTiles` for damage and spread.

---

## Water Interaction — Steam Conversion

Two paths produce steam from fire-water contact:

### 1. Fire spreading into water (`fire.rs:91-95`)

When the spread loop encounters a target tile with `LiquidType::Water` or `ShallowWater`, the fire **does not** spawn there. Instead, the water tile is recorded in `steam_spawns` and a `GasType::Steam` cloud (volume 500) is spawned on that tile via `gas::spawn_gas`. The water remains — fire does not dry up the lake. The fire that *would have* spread is simply not created. This is the dominant case during open combat over a wet floor.

### 2. Direct ignition on water (`fire.rs:231-238`)

`ignite_tile_at` short-circuits to a Steam spawn (volume 500) on a water/shallow-water tile and returns `false`. The Staff of Fire bolt into a pool produces a steam cloud rather than a fire.

### 3. Burning creature standing in water (`src/game/water.rs:water_extinguish_system`)

A creature with the `Burning` status on a water tile has the status removed and a Steam cloud (volume 300) spawned at their feet. The creature *and* the water both contribute — water never disappears, fire/burning always loses.

**Steam itself is hot**: standing in steam applies `Burning(3 turns, 2 magnitude)` to non-fire-resistant creatures (`src/game/gas.rs:46-47`). Steam is non-flammable, so it does not chain into a fire-gas explosion.

### Lava and chasm (`fire.rs:96-98`)

Fire does not spread into Lava (already hotter, redundant) or Chasm (no surface to burn). No steam, no fire — the spread attempt is silently dropped.

---

## Gas Interaction — Poison Detonation

Poison gas is **flammable** (`src/game/gas.rs:60-66`). The check happens in `gas_tick_system`, not in the fire system itself, but the fire system supplies the `FireTiles` resource it consumes (`gas.rs:255`). Once per turn, after gas redistribution:

```rust
let ignited: Vec<...> = gas_tiles.0.iter()
    .filter(|((x, y), data)| data.gas_type.flammable()
                          && fire_tiles.0.contains(&(*x, *y)))
    ...
```

For each ignited gas tile (`src/game/gas.rs:362-385`):

1. Compute AoE damage: `concentration / 50`, clamped to `[1, 10]`.
2. All creatures within the **3×3 area** (Chebyshev distance ≤ 1) take that much `DamageType::Fire`.
3. The gas is despawned (consumed in the explosion).
4. The fire that triggered it is unaffected — it continues its normal decay/spread roll the next turn.

Steam is non-flammable (`flammable() == false`), so the cycle "fire → water → steam → fire" cannot self-perpetuate. Poison gas, however, can chain: a Staff of Fire bolt into a Fungus-saturated room can detonate the whole gas cloud at once.

---

## Edge Cases & Resolved Decisions

| Case | Behavior | Source |
|------|----------|--------|
| Walking into a burning tile | Burning status applied that turn (Pass 4 of `fire_tick_system`) | `fire.rs:153-163` |
| Already-Burning creature steps onto new fire | Status **not refreshed** — duration runs out as normal | `fire.rs:154` (`!effects.is_burning()` guard) |
| Fire on a chasm tile | Impossible — chasm is non-walkable, spread loop rejects, ignite helper rejects | `fire.rs:96-98`, `fire.rs:240-242` |
| Fire on a lava tile | No spread, no steam — silently dropped | `fire.rs:96-98` |
| Two fires igniting the same tile | Second attempt is a no-op (`fire_tiles.0.contains` guard) | `fire.rs:131-133` |
| Decoration tile catching fire | Decoration cleared to `None`; visible "ground burned through" | `fire.rs:138-143` |
| Door catching fire | Door terrain demoted to `Floor` (door destroyed) | `fire.rs:144-149`, terrain flammability for Door = 20 |
| Fire visible only out of FOV | "Fire spreads!" message suppressed — only logged if any new fire is in player viewshed | `fire.rs:166-175` |
| Floor change | `FloorEntityMarker` despawns all fire entities; `FireTiles` is cleared on floor load | (standard floor lifecycle) |

### Resolved Decisions

- **Terrain-based, not entity-based.** Fire damages because of *where you are*, not because of a hostile entity. There is no "fire elemental" hidden in a fire tile; the spatial index *is* the source of truth.
- **Fire decays.** No permanent fire. A Staff of Fire that would loop forever is impossible by construction.
- **Light is cosmetic.** Fire light extends `bevy_light_2d` warmth and helps the player read the screen, but does not modify FOV, stealth, or vision range. Removing it would be an aesthetic regression, not a balance change.
- **Refresh-only Burning.** Walking through fire tiles repeatedly cannot stack the status. The player can disengage from fire by leaving once.
- **Cardinal spread only.** Diagonals do not spread. Tactical fire chains follow corridor topology, not raw distance.

---

## Cross-Links

- [STATUS_EFFECTS.md](STATUS_EFFECTS.md) (TBD) — `Burning` (5 turns, 3 dmg/tick) is the status fire applies; standard resistance pipeline.
- [WATER.md](WATER.md) (TBD) — water extinguishes burning creatures and converts adjacent fire to steam.
- [GAS.md](GAS.md) (TBD) — poison gas detonation when fire-adjacent; steam emission when fire meets water; steam itself applies Burning.
- [DUNGEON.md](DUNGEON.md) — terrain and decoration generation, including TallGrass / Cobweb / Fungus distribution that determines where fire can chain.
- [ITEMS.md](ITEMS.md) — Staff of Fire (`src/game/staves.rs:523`) routes through `ignite_tile_at`.
