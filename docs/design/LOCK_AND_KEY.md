# Phase 7: Lock & Key Runtime

## Overview

Runtime systems for interacting with locked doors and key items. The map generation
side (key placement, locked door terrain) is handled by MachinePlacer in Phase 5.
This phase adds the player-facing interaction.

## Components

### KeyItem Component

```rust
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct KeyItem {
    pub key_id: u32,
}
```

Attached to key entities when spawned. The `key_id` matches a `LockedDoor`'s key_id.

### LockedDoorMarker Component

```rust
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct LockedDoorMarker {
    pub key_id: u32,
}
```

Attached to tile entities with `TerrainType::LockedDoor` terrain. The tile entity
needs this component so the bump handler knows which key_id to check.

## Player Interaction: Bumping a Locked Door

**File**: `src/game/actions.rs`

When the player attempts to move into a `LockedDoor` tile:

1. The movement system detects the target tile is not walkable
2. Check if target terrain is `LockedDoor`
3. If yes, emit an `UnlockDoorIntent` instead of a `MovementIntent`

### UnlockDoorIntent Handler

```rust
pub struct UnlockDoorIntent {
    pub entity: Entity,
    pub door_pos: Point,
}

fn handle_unlock_door(
    intents: MessageReader<UnlockDoorIntent>,
    // ... queries for inventory, keys, map, tiles
) {
    for intent in intents.read() {
        // Find the key_id of this locked door
        let door_key_id = /* read from LockedDoorMarker on the tile entity */;

        // Search player inventory for a Key with matching key_id
        let found_key = inventory.items.iter().find(|&item_entity| {
            key_query.get(item_entity).map(|k| k.key_id == door_key_id).unwrap_or(false)
        });

        if let Some(key_entity) = found_key {
            // Consume the key
            commands.entity(key_entity).despawn();
            inventory.items.retain(|&e| e != key_entity);

            // Convert LockedDoor → Door
            map.tiles[door_idx].terrain = TerrainType::Door;
            // Update tile entity TerrainType component
            // Update sprite to Door sprite
            // Remove LockedDoorMarker
            // Remove Collider (so player can walk through)
            // Mark viewsheds dirty

            log("You unlock the door with the key.");
        } else {
            log("This door is locked. You need a key.");
        }

        // Emit ActionFinishedEvent (attempting to unlock costs a turn)
        finish_writer.write(ActionFinishedEvent {
            entity: intent.entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}
```

## Key Item Spawning

Keys are spawned by the dungeon generation system (MachinePlacer places them via
`key_spawn_list`). The spawner in `dungeon.rs` reads `key_spawn_list` and creates
key item entities with:

- `Item` marker
- `Name("Iron Key")` or similar
- `KeyItem { key_id }`
- Standard item sprite/ASCII glyph
- `Position` at the designated location
- `FloorEntityMarker`

### Key Visuals

| Mode | Appearance |
|------|-----------|
| Sprite | Small golden key sprite (placeholder: reuse chest sprite tinted) |
| ASCII | `*` in gold `#FFD700` |

## Key in Inventory

Keys are regular inventory items. They can be:
- Picked up (automatic on walk-over, like other items)
- Dropped (standard drop action)
- Viewed in inventory (shows "Iron Key — unlocks a door on this floor")

They are NOT equippable, NOT consumable via the Use action.

## Save/Load

### Key Items

Keys are Items with a `KeyItem` component. The save system needs to:
1. Detect `KeyItem` on items during save
2. Store `key_id` in the item save data
3. Restore `KeyItem` component on load

### Locked Doors

- `LockedDoor` terrain type serializes automatically via `Tile`
- `LockedDoorMarker` component on tile entities needs save/load
- On load, when spawning tiles, check if terrain is `LockedDoor` and attach
  the `LockedDoorMarker` with the correct `key_id`
- The key_id mapping needs to be stored: `Vec<(i32, i32, u32)>` (x, y, key_id)
  in `GameSaveData`

### Checklist (per project save/load protocol)

1. `GameSaveData`: add `locked_doors: Vec<(i32, i32, u32)>`
2. `auto_save_system`: query tile entities with `LockedDoorMarker`, save positions + key_ids
3. Load path in `spawn_dungeon`: restore `LockedDoorMarker` on tile entities
4. Key items: add `key_id: Option<u32>` to `InventoryItemSave` or similar

## Integration Points

- **Movement system** (`handle_movement`): needs to check for LockedDoor before
  rejecting movement as a wall bump
- **Turn system**: UnlockDoorIntent goes through the standard intent → handler →
  ActionFinishedEvent pipeline
- **Inventory UI**: Keys show in inventory like any item
- **Game log**: Feedback messages for lock/unlock attempts
