---
paths:
  - "src/save/**"
  - "src/map/dungeon.rs"
---

# Save / Load Checklist — New Features

Any time new persistent game state is added, ALL of the following must be updated:

1. **`GameSaveData`** (`save/mod.rs`): add the new field(s) with `#[derive(Serialize, Deserialize)]`
2. **`auto_save_system`**: query/read the new data and populate the new field
3. **Load path in `spawn_dungeon`** (`dungeon.rs`): restore the new state from `save_data`
   (for entity state this usually means spawning with a temporary override component, like `SavedHp`)
4. **`apply_player_load_system`** (if player-owned): apply the new field to the player entity
5. **`CachedFloor` / `CachedFloorSave`** if the state also needs to persist across floor transitions
6. **Serde derives**: any new component/resource type stored in the save must derive
   `Serialize` and `Deserialize` (and its fields must too)
7. **Explored tile init** (`NeedsExploredInit`): already handled globally — no per-feature action needed
