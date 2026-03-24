# Environment & Map Generation Design

## Map Generation Progression

The dungeon's architecture shifts as the player descends — early floors feel like a constructed dungeon, deeper floors feel ancient and chaotic.

| Floors | Generation Style | Primary Hazard | Lighting |
|--------|-----------------|----------------|---------|
| 1-3 | Room-based (BSP) + small cave pockets | Water lakes | Candle-lit rooms, dark corridors |
| 4-6 | Hybrid — larger rooms, cave corridors | Water lakes (larger) | Dimmer; fewer candles |
| 7-9 | Predominantly cavernous (cellular automata) | Lava lakes | Dark; lava provides ambient glow |
| 10 | Boss floor — open cavernous arena | Boss chamber | Torchlit boss room; dark exterior |

### Builder Pipeline by Floor Tier

The existing `BuilderChain` in `src/map/builders/mod.rs` is already composable. Configure per floor tier:

**Floors 1-3 (Dungeon)**
```
BrogueLikeBuilder(room_weight: high, cave_weight: low)
→ DiagonalCuller
→ StartPointBuilder
→ LakeBuilder(liquid: Water, num_lakes: 1-2, size: small-medium)
→ CandleSpawner(density: medium)
→ MonsterSpawner
→ UnseenCuller
→ DistantExit
```

**Floors 4-6 (Catacombs)**
```
BrogueLikeBuilder(room_weight: medium, cave_weight: medium)
→ DiagonalCuller
→ StartPointBuilder
→ LakeBuilder(liquid: Water, num_lakes: 2-3, size: medium)
→ CandleSpawner(density: low)
→ MonsterSpawner
→ UnseenCuller
→ DistantExit
```

**Floors 7-9 (Infernal Depths)**
```
CellularAutomataBuilder(iterations: 5, alive_threshold: 45%)
→ DiagonalCuller
→ StartPointBuilder
→ LakeBuilder(liquid: Lava, num_lakes: 2-4, size: medium-large)
→ CandleSpawner(density: very_low)
→ MonsterSpawner
→ UnseenCuller
→ DistantExit
```

**Floor 10 (Amulet Chamber)**
```
CellularAutomataBuilder
→ DiagonalCuller
→ StartPointBuilder
→ LakeBuilder(liquid: Lava, num_lakes: 1-2)
→ BossRoomBuilder  (sealed rectangular room at map far end)
→ MonsterSpawner
→ UnseenCuller
```

### CellularAutomataBuilder

A new `InitialMapBuilder` implementation (can be based on the existing `BlobGenConfig` in `algorithms.rs`):
- Fills map with noise at a configured alive percentage (~45%)
- Runs N iterations of the standard cellular automata smoothing rule
- Produces organic cave-like open spaces with no predefined rooms
- Results in naturally winding passages and large open caverns

---

## Liquid Tile Effects

The existing `LiquidType` enum already has `Water`, `ShallowWater`, and `Lava`. These need active systems to interact with actors.

### Water

**Shallow Water** (`LiquidType::ShallowWater`) — walkable, applies effects:

| Effect | Details |
|--------|---------|
| **Lightning conductor** | When a lightning spell hits any entity standing in shallow water, the spell arcs to every other entity within the same connected water body (flood-fill connected `ShallowWater` tiles). Each arc deals 50% of original damage. Only one arc per original cast (not chain-of-chain). |
| **Extinguish burning** | Any entity with `StatusEffect::Burning` that steps into or starts a turn in shallow water immediately removes the Burning effect. |

**Deep Water** (`LiquidType::Water`) — impassable by default. Entering deals 5 HP/turn (drowning). Can be made passable if a future item grants swim ability.

### Lava

**Lava** (`LiquidType::Lava`) — impassable by default:

| Condition | Effect |
|-----------|--------|
| Entity enters lava | Takes **15 HP/turn** while standing in it |
| Entity exits lava | Gains `StatusEffect::Burning { dmg_per_turn: 5, turns: 5 }` |
| Entity with fire immunity | Takes 0 HP/turn; no burning on exit |
| Lava + water adjacent | No interaction (lava doesn't spread in this game) |

Lava provides ambient light — tiles adjacent to lava are always lit regardless of FOV or candles. This means lava lakes are visible even in dark cavernous floors, creating readable danger and dramatic atmosphere.

**Fire Immunity** — a component `FireImmune` (or status resistance flag). Applied to:
- Hellhounds, Pit Fiend, Pit Spawn (from Bestiary)
- Player can acquire via a Legendary ring/amulet (e.g., "Ring of the Inferno")

---

## Gas Clouds

Gas is a transient tile overlay — a gas entity that occupies a tile and disperses over time.

### Gas Types

| Gas | Color tint | Effect on entry | Duration |
|-----|-----------|-----------------|----------|
| Poison Gas | Green | Apply `Poison { dmg: 3/turn, turns: 6 }` | 8 turns then disperses |
| Sleep Gas | Purple | Apply `Stunned { turns: 4 }` if CON check fails | 6 turns |
| Confusion Gas | Yellow | Apply `Confused { turns: 5 }` — random movement | 7 turns |
| Smoke | Grey | Reduces FOV to 3 tiles while in cloud | 5 turns |

### Gas Spreading

Each gas entity has `intensity: u8`. Each turn:
1. Tick down `intensity` by 1
2. At intensity 0, despawn the gas entity
3. At spawn, optionally spread to adjacent walkable tiles at `intensity - 1` (creates a cloud that fans outward)

### Gas Sources

- **Room traps:** Some dungeon rooms contain sealed gas vents — triggered when the player enters the room (pressure plate variant)
- **Enemy death emissions:** Certain enemies release gas on death:
  - Zombie → small Poison Gas cloud (radius 1)
  - Lich Apprentice → small Confusion Gas cloud (radius 1)
- **Thrown flasks:** A consumable item type (stretch goal) — throw to create a gas cloud at target tile

### Gas & Wind

Gas clouds do not move unless a "Gust" spell is cast (stretch goal). They simply decay in place.

---

## Pressure Plate Traps

Traps are invisible tile entities detectable by high LCK or a Detect Traps scroll (if added). They trigger when any entity (player or monster) steps on them.

### Trap Types

| Trap | Trigger Effect | Notes |
|------|---------------|-------|
| **Dart Trap** | Fires a dart at the triggering entity (1d6 damage, chance of Poison) | Visible after triggered; resets after 20 turns |
| **Alarm Trap** | Wakes all sleeping monsters on the current floor; alerts hunting AI | One-time trigger; visible after |
| **Pit Trap** | Entity falls through to a random walkable tile on the same floor | Player takes 1d6 fall damage; marks tile as a pit permanently |
| **Gas Vent** | Releases a Poison or Sleep Gas cloud (3 tile radius) | |
| **Bear Trap** | Roots the triggering entity in place for 3 turns | Visible after trigger; doesn't reset |

### Trap Detection & Disarming

- LCK stat influences passive detection chance when moving adjacent to a trap
- A "Detect Traps" scroll reveals all traps on the floor (stretch)
- Player can disarm a visible trap by moving onto it without triggering it (pressing `D` on the tile) — requires DEX check

### Trap Placement

Traps are placed by the map builder:
- Prefer corridor intersections and room entrances
- Never place in starting room
- Density scales with floor depth (1-2 on floors 1-3; up to 4-6 on floors 7-9)
- Boss rooms have no traps (the boss is the hazard)

---

## Darkness & Lighting

The game already has a candle/light source system via `bevy_light_2d`. Environmental darkness builds on this:

- **Floors 1-3:** Candles frequent; most rooms lit; corridors dark
- **Floors 4-6:** Fewer candles; some rooms fully dark
- **Floors 7-9:** Almost no candles; lava glow is primary light source
- **Permanent darkness rooms:** Some rooms have no light source at all; player FOV drops to 3 tiles inside. Monsters gain a small stealth bonus in these rooms (harder to detect before they attack).

Player can carry a `Torch` consumable item that provides a temporary personal light radius for 30 turns (stretch goal).

---

## Implementation Notes

- Gas cloud entities should use `FloorEntityMarker` so they despawn on floor transition
- Lava glow can be implemented as a `PointLight2d` component on lava tile entities (already using `bevy_light_2d`)
- Lightning arc targeting needs a flood-fill of connected `ShallowWater` tile indices — reuse or adapt the existing connectivity code in `choke_map.rs`
- Trap entities are invisible by default (no sprite rendered until detected or triggered); use the existing `Hidden` component
- `CellularAutomataBuilder` can reuse `BlobGenConfig` and the `Grid` utilities already in `src/map/builders/algorithms.rs`
