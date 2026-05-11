# Engine Migration Notes

Upcoming changes to `roguelike_engine` and what `bevy_rpg` needs to adapt. Organized by
implementation phase — earlier phases land first.

---

## Phase 1

### Combat Event Pipeline (engine adds `DamageEvent`, `DeathEvent`, `HealEvent`)

The engine will add a `DamageApplicationSystem` that reads `DamageEvent`, calls
`compute_after_armor` + `apply_resistance`, mutates `Health`, and emits `DeathEvent`.

**What changes in bevy_rpg:**

- `src/game/combat.rs` — The 4-stage pipeline (`hit_check_system` -> `damage_roll_system`
  -> `armor_reduction_system` -> `damage_application_system`) can be simplified. The game
  currently calls `compute_after_armor` (line 303), `apply_resistance` (line 312), and
  `apply_damage_multipliers` directly. Once the engine owns the application system:
  - `damage_roll_system` emits `DamageEvent` instead of `ApplyDamageMessage`
  - `armor_reduction_system` and `damage_application_system` can be removed — the engine
    handles armor + resistance + HP mutation
  - The game keeps hit checking (d20 roll) and damage rolling (dice + crits) since those
    are game-specific
  - Wire game reactions (combat log, screen shake, XP) into `CombatEventSet` via `.after()`
- `src/game/effects.rs` — Effect application that writes `ApplyDamageMessage` should emit
  `DamageEvent` instead
- `src/game/magic.rs` — `apply_dot_damage_system` (line 279) emits `ApplyDamageMessage` for
  Burning/Poisoned DoT — switch to `DamageEvent`
- `src/game/abilities.rs` — On-hit triggers (`BurningStrike`, `LifeDrain`, etc.) that deal
  damage should emit `DamageEvent`
- `src/game/squad.rs` — Squad damage alerting currently reads `Changed<Health>`. With engine
  events, it can read `DamageEvent` directly for more context (attacker, damage type)

### FOV Computation System (engine adds `fov_update_system`)

The engine will add a system that updates `Viewshed.visible_tiles` from bracket-lib's
`field_of_view_set`, plus a `FovRevealsMap` marker component for explored tile tracking.

**What changes in bevy_rpg:**

- `src/game/systems.rs` — `fov_update_system` (line 24) can be removed entirely. The
  engine's version does the same thing: queries `(&Position, &mut Viewshed)`, checks
  `dirty`, calls `field_of_view`, updates `visible_tiles`, clears `dirty`
- Add `FovRevealsMap` component to the player entity (in `src/player/mod.rs` or spawner)
  so the engine updates `map.explored_tiles` for the player
- `src/game/ai.rs` — The on-demand FOV computation for monsters (line 283-287) should
  also be covered by the engine system if monster viewsheds are marked dirty correctly.
  Verify that the engine system runs before the AI system in the schedule
- Schedule the engine's `FovSet` to run before `SquadAlertSet` (squad alerts read
  `viewshed.visible_tiles`)

### Turn Queue: BinaryHeap (engine internal change)

`TurnManager` switches from `Vec<(Entity, u32)>` to `BinaryHeap` internally. The public
API (`add_entity`, `insert_at`, `dequeue_next_batch_pure`, `compute_reinsert_time`) is
unchanged.

**What changes in bevy_rpg:**

- `src/game/turns.rs` — **No code changes needed**. All calls go through the public API.
  However, `turn_queue` is currently `pub` and the game may access it directly. If so,
  the field type changes and any direct `Vec` access (iteration, indexing) breaks.
  - Check for direct `turn_queue` access in `turns.rs` and `combat.rs`
  - If found, switch to using `TurnManager` methods instead

### Builder Phase Enforcement (debug_assert -> assert)

**What changes in bevy_rpg:**

- `src/map/builders/mod.rs` — If the builder pipeline has out-of-order phases, it will
  now panic in release builds too. The current pipeline order (BrogueLike -> StartPoint ->
  LakeBuilder -> cullers -> doors -> PrefabPlacer -> MachineBuilder -> cullers -> spawners
  -> DistantExit) should already be phase-correct, but verify after the engine change

---

## Phase 2

### Status Effect Framework (engine adds `StatusEffects` component + tick system)

The engine will add `StatusEffectKind`, `StatusEffects` component, tick system, and DoT
event emission. This directly overlaps with the game's existing status effect system.

**What changes in bevy_rpg:**

- `src/game/magic.rs` — This is the biggest migration. The game's `StatusEffects` component
  (line 160), `StatusEffectKind` enum (lines 33-46), `ActiveStatusEffect` struct,
  `tick_status_durations_system` (line 312), and `apply_dot_damage_system` (line 279) all
  have engine equivalents. Migration path:
  - Replace game's `StatusEffectKind` with engine's (Burning, Poisoned, Stunned, Hasted,
    Slowed are shared; game-specific kinds like Entangled, Enraged, FireResistance,
    PoisonResistance use `Custom { id }`)
  - Replace game's `StatusEffects` component with engine's
  - Remove `tick_status_durations_system` — engine's `StatusEffectTickSystem` handles this
  - Remove `apply_dot_damage_system` — engine emits `DamageEvent` for Burning/Poisoned
  - Keep game-specific effect logic (Entangled movement restriction, resistance auras)
    as `.after(StatusEffectSet)` systems
- `src/game/combat.rs` — `apply_damage_multipliers` call currently passes `is_enraged` /
  `is_terrified` booleans. The engine's `compute_damage_modifier(&StatusEffects)` replaces
  this. Switch to querying `StatusEffects` component and calling the engine helper
- `src/game/turns.rs` — Speed delay calculation (Hasted halves, Slowed 1.5x) should use
  the engine's `compute_speed_modifier(&StatusEffects)` instead of manual checks
- `src/save/mod.rs` — `PlayerSaveData.status_effects` serialization needs to match the
  engine's `StatusEffects` format. Map game-specific kinds to `Custom { id }` values.
  Assign stable IDs:
  - `Entangled = Custom { id: 1 }`
  - `Enraged = Custom { id: 2 }`
  - `FireResistance = Custom { id: 3 }`
  - `PoisonResistance = Custom { id: 4 }`
  (Or whatever mapping scheme the game adopts — just keep it stable for save compat)

### Pathfinding Dispatch (engine adds `next_step_toward` pure function)

**What changes in bevy_rpg:**

- `src/game/ai.rs` — The 4 pathfinding call sites (normal, guard return, waypoint, chase)
  all call `a_star_search` directly. They can switch to the engine's
  `next_step_toward(map, from, to, mode)` which wraps A* and returns the first step.
  This simplifies each call site from ~8 lines to ~2 lines
- The `MapWithMode` wrapper construction can also be removed if the engine function
  handles it internally

### GOAP Planner Improvements (duplicate pruning + `plan_full`)

**What changes in bevy_rpg:**

- `src/game/goap.rs` — No breaking changes. The `plan()` function signature is unchanged.
  Duplicate-state pruning improves performance for complex action sets (the game defines
  ~10 actions per monster type). `plan_full()` is a new function useful for debugging
  AI decisions — consider adding it to the cheat menu or debug overlay

### Generalized Combat Modifiers

**What changes in bevy_rpg:**

- `src/game/combat.rs` — `apply_damage_multipliers(base, is_enraged, is_terrified)` still
  works (backward compatible). But the game can switch to
  `apply_damage_multipliers_from_modifiers(base, &modifiers)` to support additional
  multiplier sources (weapon enchantments, terrain bonuses, etc.) without expanding the
  function signature each time

---

## Phase 3

### Ability Framework (engine adds `AbilityDef`, `TargetingRule`, `AbilityUseEvent`)

The engine will add data-driven ability descriptors and a resolution system. This overlaps
with parts of the game's ability and targeting systems.

**What changes in bevy_rpg:**

- `src/game/abilities.rs` — The game's component-per-ability pattern (BurningStrike,
  PoisonStrike, etc.) is different from the engine's data-driven `AbilityDef` descriptors.
  Two migration options:
  1. **Gradual**: Keep game components but use `AbilityUseEvent` for resolution. Game
     components become "source of truth" for ability stats; the engine handles targeting
     validation and event emission
  2. **Full**: Convert each ability to an `AbilityDef` loaded from RON. On-hit/on-death
     effects remain as game-side components since they're triggered by events, not actively
     cast. Only active abilities (melee attacks, ranged attacks, war cry) become `AbilityDef`s
- `src/game/targeting.rs` — The game's `TargetingContext` (line 47) and targeting modes
  (Spell, SpellAlly, Tile, RangedAttack, Staff) partially overlap with the engine's
  `TargetingRule` enum. The engine provides `targets_in_range(origin, rule, map)` which
  replaces manual range/AoE validation. The game's UI-driven target selection (cursor,
  highlights) remains game-owned
- `src/game/staves.rs` — Staff usage could become an `AbilityUseEvent` with a custom
  `TargetingRule`, tying charges into the `AbilitySlot` cooldown system. Or staves can
  remain game-owned since they have unique charge mechanics (Brogue-style)
- `src/game/goap.rs` — The engine's `CanCastUsefulSpell` GOAP prop can now be evaluated
  by checking `Abilities` component cooldowns, replacing game-side ad-hoc checks

### Save System: Schema Versioning

**What changes in bevy_rpg:**

- `src/save/mod.rs` — Switch from `platform::write_bytes` / `platform::read_bytes` to
  `save_with_version` / `load_with_version`. Add a `schema_version` constant to the game.
  When changing the save format in future updates, implement `SaveMigration` trait to
  upgrade old saves
- Add `schema_version` to `SaveFrameworkConfig` initialization (currently just sets
  `save_key: "ironveil_save"`)
- Old saves (pre-versioning) are detected as version 0 — add a migration from v0 to v1
  that wraps the raw payload in the new envelope format

### AI Decision Helper Expansion

**What changes in bevy_rpg:**

- `src/game/ai.rs` — The game likely has inline logic for patrol target selection and
  threat prioritization. Replace with engine's `pick_patrol_target(patrol, current_pos)`
  and `threat_priority(threats)`. Check the 4 pathfinding call sites — the guard patrol
  and waypoint patrol paths (lines 733, 781) are candidates for using the engine helper

---

## Phase 4

### Example Game, Benchmarks, CLAUDE.md

**No changes needed in bevy_rpg.** These are engine-only additions. The example game
validates that the engine API works end-to-end. Benchmarks establish performance baselines
for engine operations the game depends on (map gen, pathfinding, FOV, turn scheduling).

---

## General Migration Notes

- **Import paths**: As systems move from game to engine, update `use` statements. The
  engine re-exports everything through `roguelike_engine::prelude::*`
- **SystemSet scheduling**: New engine SystemSet markers (`CombatEventSet`,
  `StatusEffectSet`, `FovSet`) need to be wired into the game's schedule. The game already
  configures `SquadAlertSet` and `SquadReactionSet` — follow the same pattern
- **Save compatibility**: Any component that moves from game-defined to engine-defined
  changes its type path for Bevy reflection. Test save/load round-trips after each
  migration phase
- **Custom variants**: Game-specific enum values (Entangled status, custom terrain, etc.)
  must use `Custom { id }` variants. Assign stable IDs and document them
