# Save / Load System

## Overview

Ironveil uses a hand-rolled RON (Rusty Object Notation) save system. The full game state is
serialized to a single file at `saves/ironveil_save.ron`. There is one save slot per run;
death deletes it (permadeath).

**Why not `bevy_save`?** `bevy_save` requires all saved types to implement Bevy's `Reflect` trait.
Third-party types like `bracket_lib::Point` cannot implement `Reflect`, making it impractical.
Our manual approach is simpler, fully explicit, and human-readable.

---

## File Location

```
saves/ironveil_save.ron      # relative to the working directory
```

Created automatically on first save. Deleted on player death.

---

## When Saves Happen

| Trigger | Mechanism |
|---|---|
| Entering a new floor (down stairs) | `spawn_dungeon` sets `AutoSavePending(true)` |
| Returning to a visited floor (up stairs) | `spawn_dungeon` sets `AutoSavePending(true)` |
| Loading a save file | `spawn_dungeon` sets `AutoSavePending(true)` after restore |
| Window X button / OS close | `save_on_exit_system` (Last schedule) reads `AppExit`, sets `AutoSavePending(true)` |
| Quit button in menu | Sends `AppExit::Success`; save-on-exit fires if still InGame |

`auto_save_system` runs in the **`Last` schedule** (after all `Update` systems) and writes the
file when `AutoSavePending` is true. Running in `Last` guarantees that Bevy's built-in
`close_when_requested` system (which sends `AppExit` in `Update`) has already fired before we
check for it.

---

## Data Serialized

### `GameSaveData` — top-level struct

| Field | Type | What it holds |
|---|---|---|
| `floor` | `u32` | Current floor number |
| `game_log` | `Vec<String>` | All game log entries |
| `map` | `MapSaveData` | Current floor tiles + explored state |
| `player` | `PlayerSaveData` | All player stats, inventory, equipment |
| `monsters` | `Vec<MonsterEntry>` | Live monsters on the current floor |
| `floor_items` | `Vec<ItemEntry>` | Items on the floor (not in inventory) |
| `candles` | `Vec<[i32; 2]>` | Light source positions |
| `floor_cache` | `HashMap<u32, CachedFloorSave>` | State of all previously visited floors |

### `PlayerSaveData`

Stores position, HP, level, XP, spell slots, all six attributes + modifiers (from equipment),
viewshed range, damage dice string, current mana, and the full inventory as
`Vec<InventoryItemSave>`.

Each `InventoryItemSave` holds the item name (manifest key), a full copy of `ItemProperties`
(preserves any per-item stat tweaks), and the equipped slot name if the item is equipped.

### `MapSaveData`

Width, height, depth, name, the full `Vec<Tile>` tile array, and `Vec<bool>` explored flags
(one per tile, index-matched). The explored state drives the fog-of-war dim overlay on load.

### `CachedFloorSave`

Mirrors `CachedFloor` but replaces `bracket_lib::Point` values with `[i32; 2]` arrays since
`Point` cannot implement `Serialize`/`Deserialize` directly.

---

## Load Flow

```
Menu "Continue"
  → load_save_file() reads RON from disk
  → PendingGameLoad(Some(Box<GameSaveData>)) set
  → AppState::InGame transition

spawn_dungeon  (triggered by SpawnDungeonMessage)
  → detects PendingGameLoad is Some → LOAD PATH:
      restore Map from MapSaveData
      spawn tile ECS entities  (all start Unexplored/Hidden)
      set NeedsExploredInit = true  → init_explored_tiles_system dims previously-seen tiles
      spawn monsters with SavedHp(hp) component
      spawn floor items
      spawn candles
      insert SavedFloorCache resource
      set PendingPlayerLoad(Some(PlayerSaveData))

apply_player_load_system  (Update, InGame, runs when PendingPlayerLoad is Some)
  → overrides all player components (pos, HP, level, XP, attrs, damage, mana, viewshed)
  → clears Inventory + Equipment, re-spawns inventory items with correct properties
  → restores FloorCache from SavedFloorCache
  → sets SaveExists = true

apply_saved_hp_system  (Update, InGame)
  → consumes SavedHp components on monsters, overrides HP after stat recalculation
```

---

## Permadeath

`delete_save()` is called from `death_system` in `combat.rs` when the player's HP reaches 0.
The save file is removed before the GameOver screen is shown. On the next launch, the Continue
button is disabled because `SaveExists` is false.

---

## Adding New Persistent State — Checklist

**Every time you add a new piece of game state that should survive a save/load, touch all of
the following. Missing any one of them will cause that state to silently reset on continue.**

### 1. Define the serializable type

Add `#[derive(Serialize, Deserialize)]` (and `Clone` if needed) to the component or struct.
Every field type must also be serializable — if it isn't (e.g. a foreign type), wrap it or
convert to a primitive (see how `Point` → `[i32; 2]`).

```rust
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct MyNewComponent {
    pub value: i32,
}
```

### 2. Add a field to `GameSaveData` (or the appropriate sub-struct)

```rust
// save/mod.rs
pub struct GameSaveData {
    // ...existing fields...
    pub my_new_thing: MyNewThingSave,
}
```

If the state is player-owned, add it to `PlayerSaveData`.
If it's per-floor entity state, add it to `MonsterEntry` / `ItemEntry` or a new entry type.
If it needs to persist across floor transitions, add it to `CachedFloorSave` too.

### 3. Populate the field in `auto_save_system`

Query or read the data and write it into the save struct:

```rust
// Inside auto_save_system, when building save_data:
my_new_thing: my_query.iter().map(|c| c.into_save()).collect(),
```

### 4. Restore in `spawn_dungeon` (load path)

In the `if let Some(save_data) = pending_game_load.0.take()` block in `dungeon.rs`:

```rust
for entry in &save_data.my_new_things {
    // spawn / restore entity with saved state
    // For deferred overrides (like monster HP), spawn a temporary component:
    commands.entity(e).insert(MyOverrideComponent(entry.value));
}
```

### 5. Restore player state in `apply_player_load_system`

If the state is player-owned, apply it here after the player entity exists:

```rust
my_component.value = player_data.my_new_field;
```

### 6. Add a consume system for deferred overrides (if needed)

If you used a temporary override component (like `SavedHp`), add a system that reads it and
applies it after stat recalculation has run, then removes the component:

```rust
pub fn apply_my_override_system(
    mut commands: Commands,
    mut query: Query<(Entity, &mut MyComponent, &MyOverrideComponent)>,
) {
    for (entity, mut comp, saved) in query.iter_mut() {
        comp.value = saved.0;
        commands.entity(entity).remove::<MyOverrideComponent>();
    }
}
```

Register it in `SavePlugin::build` alongside `apply_saved_hp_system`.

### 7. Update `CachedFloor` / `CachedFloorSave` if floor-transition persistence is needed

If the state must survive ascending/descending stairs (not just save/load), also update:
- `CachedFloor` in `dungeon.rs` — the in-memory cached floor struct
- `CachedFloorSave` in `save/mod.rs` — the serializable version
- `cached_floor_to_save()` — conversion to save format
- `save_to_cached_floor()` — conversion from save format
- `snapshot_floor()` in `dungeon.rs` — captures state before floor transition

---

## Explored Tile Initialization

When tiles are spawned on load or floor restore, they all start as `TileExplored::Unexplored`
(hidden). `NeedsExploredInit` is set to `true` by `spawn_dungeon` in these cases, which triggers
`init_explored_tiles_system` (in `Update`, `InGame`) to iterate all tile entities and dim
previously-explored tiles based on `map.explored_tiles`. This is handled globally — no
per-feature action needed when adding new tile types.

---

## Key Files

| File | Role |
|---|---|
| `src/save/mod.rs` | All save/load types, serialization, `auto_save_system`, `apply_player_load_system`, `apply_saved_hp_system`, `save_on_exit_system` |
| `src/map/dungeon.rs` | `spawn_dungeon` (load path + `AutoSavePending`), `PendingGameLoad`, `PendingPlayerLoad`, `AutoSavePending`, `NeedsExploredInit` |
| `src/map/map.rs` | `init_explored_tiles_system`, `NeedsExploredInit` resource |
| `src/game/combat.rs` | `death_system` → calls `delete_save()` |
| `src/menu/mod.rs` | `load_save_file()`, Continue button, Quit → `AppExit` |
