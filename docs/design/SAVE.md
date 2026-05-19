# Save/Load System Design

## 1. Save File Location

**Key:** `"ironveil_save"` — `GAME_SAVE_KEY` at `src/save/mod.rs:60`
**Storage:** Platform-agnostic via `roguelike_engine::save::platform` (native `.ron` on desktop, `localStorage` on WASM)
**Envelope:** Versioned `SaveEnvelope` from `roguelike_engine::save` wraps the RON payload with `{ schema_version, payload }`
**Access:** Read via `read_save_data()` (`src/save/mod.rs:272`), write via `save_with_version()` (`:265`), delete via `delete_save()` (`:317`)

---

## 2. Schema Version Timeline

All versions are defined with migration stubs in `src/save/mod.rs`. Migrations are zero-copy RON string ops where needed.

| Version | Added | Migration Notes |
|---------|-------|-----------------|
| **v0** | Initial | Pre-envelope raw RON — no versioning |
| **v1** | Envelope wrapper | Same payload as v0; wraps in versioned [`SaveEnvelope`](../../../roguelike_engine/src/save/mod.rs) |
| **v2** | Status effect format | Engine refactored status kinds (`Burning`/`Poisoned` flat + `magnitude`; game kinds initially went through `Custom { id }`). Migration drops all active statuses (transient by design, max ~20 turns). |
| **v3** | Character phase 1 | Added `PlayerSaveData`: `race`, `class`, `attributes`, `hit_bonus`, `damage_bonus`. Backward-compat via `#[serde(default)]`. |
| **v4** | Character phase 2 | Dropped `CON` from `Attributes`; removed `Halfling` from `Race`; added `level: u32` (default 1 via `default_save_level`) and `experience: u32` (default 0) to `PlayerSaveData`. **Breaking** w.r.t. attribute math: the modifier anchor moved from 10 → 16, so v3 attribute scores load with off-by-six combat math. The migration stub (`MigrateV3ToV4` at `src/save/mod.rs:209`) is a no-op — acceptable in permadeath dev because there's no save to migrate after a death. |
| **v5** | Skills phase 3 | Added `PlayerSaveData`: `skills`, `skill_xp`, `skill_training`, `skill_xp_pool`. All `#[serde(default)]` — v4 saves load with empty maps and 0 pool. No-op migration. |
| **v6** | Overworld | Added `GameSaveData.overworld: OverworldSave` and `SavedFloorData.exit_tiles: Vec<SavedExitTile>`. Both `#[serde(default)]` — v5 saves load with empty overworld and no edge transitions. No-op migration. |
| **v7** | Stealth phase I | Per-monster `SavedMonster.awareness` (degraded Hidden/Searching shape). `#[serde(default)]` — v6 saves default to Hidden. No-op migration. |
| **v8** | Named status kinds | `StatusEffectKind::Custom { id }` replaced by named variants (`Entangled`, `Enraged`, `FireResistance`, `PoisonResistance`). **Pre-v8 saves containing the old `Custom { id }` shape are unrecoverable** — the bincode representation changed. No migration provided; acceptable in permadeath dev. |

Current version: **8** (`src/save/mod.rs:89`)

---

## 3. Persisted Data Shape

### Root: `GameSaveData` (`src/save/mod.rs:375`)

| Field | Type | Purpose |
|-------|------|---------|
| `floor` | `u32` | Current depth (0=town, 1..=8 forest, 9..=11 temple) |
| `game_log` | `Vec<String>` | Full turn history |
| `map` | `MapSaveData` | Current floor tile/entity grid |
| `player` | `PlayerSaveData` | Player state (position, HP, equipment, character) |
| `monsters` | `Vec<SavedMonster>` | Enemies on current floor |
| `floor_items` | `Vec<SavedItem>` | Items on current floor |
| `props` | `Vec<SavedProp>` | Destructibles/terrain (v6+, default empty) |
| `floor_cache` | `HashMap<u32, SavedFloorData>` | Visited floors (depth → state) |
| `squad_id_counter` | `u64` | Next unique monster squad ID (v6+, default 0) |
| `fallen_monsters` | `HashMap<u32, Vec<SavedMonster>>` | Monsters in transit through chasms (v6+, default empty) |
| `fallen_items` | `HashMap<u32, Vec<SavedItem>>` | Items in transit through chasms (v6+, default empty) |
| `overworld` | `OverworldSave` | Temple entrance location (v6+, default empty) |

### Player: `PlayerSaveData` (`src/save/mod.rs:542`)

| Field | Type | Purpose |
|-------|------|---------|
| `x`, `y` | `i32` | Position on floor |
| `hp` | `i32` | Current hit points |
| `armor` | `i32` | Armor defense |
| `block` | `i32` | Shield SH bonus (v5+, default 0) |
| `max_shield_blocks` | `u32` | Shield attempts/turn cap (v5+, default 0) |
| `dodge` | `i32` | Dodge defense |
| `viewshed_range` | `i32` | Vision distance |
| `damage` | `String` | Damage dice (e.g., `"1d8+2"`) |
| `status_effects` | `StatusEffects` | Active buffs/debuffs (always empty on load per v2 migration) |
| `inventory` | `Vec<InventoryItemSave>` | Carried items + state |
| `race` | `Race` | Ancestry (v3+, default Human) |
| `class` | `Class` | Class (v3+, default Warrior) |
| `attributes` | `Attributes` | STR/DEX/INT final scores (v3+, default all-16 via `default_attributes_baseline`). Phase 2 anchors the modifier at 16, so the default yields mod 0 across the board. |
| `hit_bonus` | `i32` | Post-equipment hit modifier (v3+, default 0) |
| `damage_bonus` | `i32` | Post-equipment damage modifier (v3+, default 0) |
| `level` | `u32` | XP level (v4+, default 1) |
| `experience` | `u32` | XP toward next level (v4+, default 0) |
| `skills` | `Skills` | Per-skill float levels (v5+, default empty) |
| `skill_xp` | `SkillXp` | Per-skill cumulative XP (v5+, default empty) |
| `skill_training` | `SkillTraining` | Per-skill training time spent (v5+, default empty) |
| `skill_xp_pool` | `u32` | Unallocated XP pool (v5+, default 0) |

### Monster: `SavedMonster` (`src/save/mod.rs:431`)

| Field | Type | Purpose |
|-------|------|---------|
| `x`, `y` | `i32` | Position |
| `name` | `String` | Manifest ID (e.g., `"goblin"`) |
| `hp_current` | `i32` | Restored HP (0 = fresh spawn, let manifest decide) |
| `squad_id` | `Option<u64>` | Group ID for cohesion (v6+, default None) |
| `is_leader` | `bool` | Squad leader flag (v6+, default false) |
| `squad_config` | `Option<SquadConfig>` | Composition override (v6+, default None) |
| `patrol_route` | `Option<PatrolRoute>` | Waypoint path (v6+, default None) |
| `submerged` | `bool` | Underwater flag (v6+, default false) |

### Item: `SavedItem` (`src/save/mod.rs:485`)

| Field | Type | Purpose |
|-------|------|---------|
| `x`, `y` | `i32` | Position |
| `name` | `String` | Manifest ID |
| `count` | `u32` | Stack size (default 1) |
| `state` | `ItemMutableState` | Enchantment, runics, staff state (flattened) |
| `drifting` | `bool` | Airborne/in-motion flag (v6+, default false) |

**ItemMutableState** (`src/save/mod.rs:457`): `enchantment`, `weapon_runic`, `armor_runic`, `runic_identified`, `staff_effect`, `base_recharge`, `staff_charges`, `staff_max_charges`, `staff_recharge_timer`, `staff_recharge_rate` — all with `#[serde(default)]` for safe field addition.

### Floor Cache: `SavedFloorData` (`src/save/mod.rs:517`)

| Field | Type | Purpose |
|-------|------|---------|
| `map` | `MapSaveData` | Tile grid |
| `monsters` | `Vec<SavedMonster>` | Dormant enemies |
| `items` | `Vec<SavedItem>` | Dormant items |
| `props` | `Vec<SavedProp>` | Dormant destructibles (v6+, default empty) |
| `down_stairs_pos` | `[i32; 2]` | Stair location |
| `up_stairs_pos` | `[i32; 2]` | Return stair location |
| `exit_tiles` | `Vec<SavedExitTile>` | Overworld edges/temple stairs (v6+, default empty) |

### Map: `MapSaveData` (`src/save/mod.rs:532`)

| Field | Type | Purpose |
|-------|------|---------|
| `tiles` | `Vec<Tile>` | 80×50 grid serialized flat |
| `explored` | `HashSet<usize>` | Revealed tile indices |

---

## 4. Save Trigger

**Entry point:** `auto_save_system()` (`src/save/mod.rs:813`)

**Scheduled:** In `SavePlugin.build()` as a system that polls `AutoSavePending` resource

**When it fires:**
- **Floor transition:** `save_on_exit_system()` sets `AutoSavePending` flag on `AppExit` event
- **Demand:** Direct call to `save_with_version()` from `platform` module (not auto; menu/quit events)

**What it captures:**
1. Player position, HP, armor, block, dodge, viewshed, damage, inventory, equipment
2. Character (race, class, attributes, hit bonus, damage bonus, level, XP)
3. Skills (per-skill levels, XP, training, pool)
4. Game log (full turn history)
5. Current floor's monsters, items, props
6. Floor cache (all visited depths with their state)
7. Fallen units in chasm transit
8. Overworld state (temple entrance location)

**Note:** Status effects are NOT saved (transient, auto-cleared per v2 migration).

---

## 5. Load Trigger

**Entry point:** `read_save_data()` (`src/save/mod.rs:272`)

**When called:**
- **"Continue" menu:** `src/ui/menu.rs` calls `read_save_data()` to populate the continue button
- **Game load:** After "Continue" is selected, `spawn_dungeon()` (`src/game/dungeon.rs`) restores `PendingGameLoad` and `PendingPlayerLoad`

**What happens:**
1. `load_with_version()` fetches the envelope and version
2. If version < current, `apply_migrations()` chains v→v+1 migrations forward
3. RON payload is deserialized into `GameSaveData`
4. `spawn_dungeon()` instantiates floor entities + player from the save
5. `apply_player_load_system()` applies character/skill/equipment state
6. `apply_saved_hp_system()` overrides stat-recalc'd monster HP with saved values

**Failure modes:**
- **Missing file:** `load_with_version()` returns `SaveLoadError::NotFound` → `None` (no save to load)
- **Corrupt envelope:** `SaveLoadError::CorruptedEnvelope` → `None` + warn log
- **Migration failure:** `apply_migrations()` fails → `None` + warn log (e.g., v3→v4 with Halfling race)
- **Bad RON:** Deserialization fails → `None` + error log

---

## 6. Serde Compatibility Contract

**Policy:** Fields added to any persisted struct MUST use `#[serde(default)]` or have a custom default function.

**Additive strategy:** Old saves load with missing fields filled by defaults. No destructive migrations unless absolutely required (see v3→v4).

**Example:** v5 added `skills` to `PlayerSaveData` with `#[serde(default)]`. v4 saves load with empty `Skills` map (equivalent to level-0 in all skills).

**7-step checklist** (canonical: `.claude/rules/save-load-checklist.md`):
1. **`GameSaveData`** — add the new field(s) with `#[serde(Serialize, Deserialize)]` (or `#[serde(default)]` if loaders need the safety net)
2. **`auto_save_system`** — query/read the new data and populate the new field
3. **Load path in `spawn_dungeon`** (`dungeon.rs`) — restore the new state from `save_data`. For entity state this usually means spawning with a temporary override component, like `SavedHp`.
4. **`apply_player_load_system`** (if player-owned) — apply the new field to the player entity
5. **`CachedFloor` / `CachedFloorSave`** — if the state also needs to persist across floor transitions, update the mirror structs
6. **Serde derives** — any new component/resource type stored in the save must derive `Serialize` and `Deserialize` (recursive for all fields)
7. **Explored tile init** (`NeedsExploredInit`) — handled globally; no per-feature action needed

---

## 7. Permadeath Flow

**Death event:** Player HP drops to 0

**Handler:** `src/game/combat/mod.rs:771` in the main health-damage system

**Action sequence:**
1. `RunSummary` is populated with floor reached, cause of death, and enemy name
2. **`crate::save::delete_save()`** is called immediately
3. `AppState` transitions to `GameOver`
4. Game-over screen displays `RunSummary` (final statistics)

**Code reference:** `src/game/combat/mod.rs:765–772`

---

## 8. What's NOT Persisted

The following are recreated at load and NOT saved:

- **Status effects:** All buffs/debuffs (v2 migration deliberately drops them; transient by design, max ~20 turns). Player loads with empty `StatusEffects`.
- **Particle effects:** Cosmetic animations (light, fire, water splashes) — rebuilt fresh per-frame.
- **Spatial indices:** ECS spatial queries (broadphase colliders, visibility maps) — rebuilt on load via systems.
- **Light maps:** Pre-computed illumination — regenerated per dungeon-load.
- **RNG state:** Random number generator — seeded fresh per dungeon generation (or consistent via floor seed if determinism required).
- **Turn queue:** Entity turn order — reconstructed from priorities on spawn.
- **UI state:** Menu selections, log scroll position, focus — reset per app state transition.

---

## 9. Recent Commits Touching Save

```
29aca1e  refactor(map): unify FloorPlan schema with SavedMonster/Item/Prop
92dbe69  feat(overworld): town + 3x3 forest + temple replace 26-floor descent  [→ v6]
341250a  feat(combat): Block stat + Shields skill (9th skill)                    [→ v5+]
6ec571e  feat(skills): persist Skills/SkillXp/SkillTraining/pool (save v4 → v5)  [→ v5]
520d173  feat(character): Phase 2 — XP, levels, racial stat-gain, ASI, character info
25b2e02  refactor(character): Phase 1.5 — drop CON, anchor mod at 16, remove Halfling [→ v4]
7f2bcc8  feat(save): persist Race/Class/Attributes/HitBonus/DamageBonus (v3 schema)    [→ v3]
1eee1eb  feat: save/load submerged state + water-aware chase AI (Phase 2G + 3A)
```

---

## 10. Known Fragility & TODOs

- **v3→v4 unrecoverable:** Saves containing CON or Halfling fail to load. No remapping possible (attribute-score scale changed). Acceptable during active dev; would require explicit player versioning in production.
- **Status effects always dropped:** v1→v2 migration is destructive but acceptable (transient by design). Any future spell/curse that spans floor transitions would require a new field and migration.
- **ItemMutableState flattening:** Item state uses `#[serde(flatten)]` for backward compat. Adding new item fields is low-friction but requires care with serde attribute ordering.
- **Floor cache unbounded:** `floor_cache: HashMap<u32, SavedFloorData>` grows without limit. Hypothetical 100-floor dungeon would accumulate all prior floor states. Consider pruning strategy if dungeon depth expands significantly.

No active FIXMEs in `src/save/mod.rs` as of last check.

---

**File structure:** All save code lives in `/Users/nathanrude/Development/bevy_rpg/src/save/mod.rs` (~2700 lines). Related: `src/game/dungeon.rs` (load orchestration), `src/game/combat/mod.rs` (death trigger), `src/ui/menu.rs` (continue button).
