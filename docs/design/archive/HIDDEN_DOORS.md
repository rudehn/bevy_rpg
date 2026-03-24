# Phase 3: Hidden Doors

## Overview

Convert some existing doors to hidden doors that appear as walls until discovered.
Hidden doors create alternate paths and reward observant players.

## Tile Behavior

`HiddenDoor` terrain type (already added in Phase 1):
- **Renders as**: Wall sprite/ASCII (indistinguishable from wall)
- **Walkable**: No (until discovered)
- **Passable**: Yes (connectivity checkers treat it as a connection)
- **Opaque**: Yes (blocks FOV)
- **On discovery**: Converts to `Door` terrain type (standard door behavior)

## Builder: HiddenDoorPlacer

**File**: `src/map/builders/hidden_door_placer.rs` (NEW)

**Pipeline position**: After LakeBuilder, before DecorationPropagator

### Algorithm

1. Compute ChokeMap and store on `BuilderMap.chokepoints` for reuse by later builders
2. Iterate all `Door` tiles on the map
3. For each door, look up its choke_value from the ChokeMap
4. **Skip** if choke_value < 20 (this door is the only connection — hiding it would make the gated region inaccessible without searching)
5. Roll depth-scaled conversion chance:
   - Depth 1-3: 0%
   - Depth 4-10: `(depth - 3) * 3`% (3-21%)
   - Depth 11+: 25% cap
6. Cap at 3 hidden doors per floor
7. Convert selected doors: `TerrainType::Door → TerrainType::HiddenDoor`

### ChokeMap Sharing

The ChokeMap is expensive to compute. HiddenDoorPlacer computes it once and stores it on `BuilderMap.chokepoints: Option<ChokeMap>`. Later builders (MachinePlacer, ShrinePlacer) reuse this cached result.

## Runtime Discovery System

**File**: `src/game/systems.rs` (new system)

### Discovery Mechanic

- System runs when player's `Viewshed` changes (same trigger as tile visibility)
- For each tile adjacent to the player (8 directions):
  - If tile terrain is `HiddenDoor` AND tile is in player's FOV:
    - Roll discovery chance: 15% base + perception bonus
    - On success: convert `HiddenDoor → Door`, mark viewshed dirty, log "You notice a hidden door!"
- A dedicated "Search" action (future) could guarantee discovery of all adjacent hidden doors

### ECS Components

No new components needed — `HiddenDoor` is a terrain type, not a component. Discovery modifies the `Map` resource and the tile entity's `TerrainType` component directly.

### Sprite/ASCII Update on Discovery

When terrain changes from `HiddenDoor` to `Door`:
- The tile visibility system already reads terrain type for sprite lookup
- Need to update the tile entity's `Sprite` atlas to the Door sprite
- Need to update `AsciiGlyph` text from `#` to `+` and `AsciiBackground` color
- Need to remove `Collider` component (walls have it, doors don't when open)

## Save/Load

`HiddenDoor` is already a `TerrainType` variant with serde derives. Tiles serialize/deserialize automatically. No additional save/load work needed.

## Interaction with Lakes

Lakes run before HiddenDoorPlacer. If a lake absorbed a door, that door no longer exists for hidden door conversion. This is correct — hidden doors should only exist in the structural dungeon, not in lake areas.
