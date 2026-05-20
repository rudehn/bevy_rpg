# bevy_rpg — The Veiled Tyrant

A Brogue-inspired roguelike built with Bevy 0.17 and Rust.

**Current shape (linear-floor milestone):** traditional descend-stairs
roguelike. Player starts in a procedural **town** (floor 0), descends
through **four forest floors** (1–4), and reaches the **cult temple**
at the bottom (floor 5 = `MAX_FLOOR`). The temple is a linear stone
corridor opening into a sanctum chamber holding the Amulet of Yendor.
Pick it up and return to the town's central Portal to win. Permadeath.

The temple entrance on **Forest 4** is intentionally hidden — instead
of a `DownStairs` at the east clearing, it's placed at a random
walkable tile well off the east-west spine. The player has to wander
off the corridor to find it; the temple is "discoverable" rather than
handed to them.

**Monster spawns are re-enabled** on forest floors via the Voronoi-cell
spawner (see [SPAWNING.md](docs/design/SPAWNING.md)). Forest 1 is solo-
only (no packs); packs form starting on Forest 2 and full pack tiers
ship on Forest 3–4. The temple has no spawn entries yet — cultists
arrive in a future pass. Item spawns remain disabled — the amulet is
the only item in the world. See [OVERWORLD.md](docs/design/OVERWORLD.md)
for the canonical writeup. `MAX_FLOOR` lives in
[src/constants.rs](src/constants.rs); raising it is content work — add
more `FloorKind::Forest` floors, new variants, or expand the temple.

## Build & Run

```bash
cargo run          # Run the game
cargo build        # Build without running
cargo check        # Fast type/borrow check (no codegen)
cargo clippy       # Lint
```

## Design Documentation

Design docs live in `docs/design/`. Read these before making gameplay changes.
**Every game system has a corresponding design doc here** — see
`.claude/rules/design-docs-required.md` for the rule that enforces this.

### High-level overviews

| Doc | Covers |
|-----|--------|
| [GAME.md](docs/design/GAME.md) | Vision, core loop, win/lose, combat system, damage types, player stats, progression |
| [PLAYER.md](docs/design/PLAYER.md) | Player stats, starting kit, equipment slots |
| [CHARACTER.md](docs/design/CHARACTER.md) | Race / class / attribute system, character creation, HP-from-CON, attribute → combat math |
| [OVERWORLD.md](docs/design/OVERWORLD.md) | **Current map structure** — town hub, forest ring, temple, transitions, save layout |
| [ENCOUNTERS.md](docs/design/ENCOUNTERS.md) | Prop trigger system (hordes → spawn table → interactive props), blueprints, trapped chests, lock & key. See also [RFC 0002](docs/rfcs/0002-prop-machine-decoration-unification.md) for the props+machines unification. |
| [ENEMIES.md](docs/design/ENEMIES.md) | Monster roster, factions, species, tier structure, per-monster identities |
| [ITEMS.md](docs/design/ITEMS.md) | Weapons (active abilities), staves (charges), armor, rings/amulets, potions, enchanting, runics |

### Per-system docs

| Doc | Covers |
|-----|--------|
| [TURNS.md](docs/design/TURNS.md) | Turn queue, TurnState FSM, SpeedStats delay model, ActionFinishedEvent contract, processing phases |
| [ABILITIES.md](docs/design/ABILITIES.md) | Monster ability triggers (on-hit/on-being-hit/on-death/passive), cooldown family, ExplodeEffect variants |
| [STATUS_EFFECTS.md](docs/design/STATUS_EFFECTS.md) | Burning, Poisoned, Slowed, Stunned, Hasted, Enraged, Entangled, Fire/Poison Resistance, tick model, refresh policy |
| [FACTIONS.md](docs/design/FACTIONS.md) | Faction component, FactionMatrix, hostility lookup, cross-faction combat, default-Hostile gotcha |
| [NPCS.md](docs/design/NPCS.md) | Peaceful Townsfolk reuse the monster pipeline; faction-gated Idle→Hunting; `TOWN_NPC_SPAWNS` placement roster; AreaRoam / Sentry patrol routes |
| [RANGED.md](docs/design/RANGED.md) | Ranged attack pipeline, F-key targeting, weapon range, ammo, LOS gating |
| [SQUAD_AI.md](docs/design/SQUAD_AI.md) | Squad system, shared alerting, leader mechanics, morale-based fleeing |
| [TACTICS.md](docs/design/TACTICS.md) | Per-monster tactic registry — pure resolver + Bevy adapter, ordered tactic list per monster, sticky `Fleeing` mode overlay, `IdleMove` with `PathToRandomTile`/`Patrol`/`Roam`/`Stationary` variants. Single AI path (replaced FSM + GOAP) |
| [STEALTH.md](docs/design/STEALTH.md) | Per-perceiver awareness model, opposed d20 detection, Stealth skill, Backstab gate, noise map V2 hook |
| [FIRE.md](docs/design/FIRE.md) | Fire entities, spread, ignition chance, burn duration, water/gas interactions |
| [GAS.md](docs/design/GAS.md) | Gas types (Poison, Steam), volume, diffusion, decay, FOV blocking, ignition |
| [WATER.md](docs/design/WATER.md) | Shallow/deep water, movement cost, Submerged state, item drift, fire-water steam |
| [CHASMS.md](docs/design/CHASMS.md) | Chasm tile mechanics, fall damage, fallen-entity propagation across floors |
| [TILE_PROMOTION.md](docs/design/TILE_PROMOTION.md) | Cracked floor → chasm, grass regrowth, embers → ash, promotion cooldown |
| [LIGHT.md](docs/design/LIGHT.md) | Per-tile light intensity + color, Bresenham LOS, resource vs. entity-driven sources, dirty propagation |
| [ASCII_RENDERER.md](docs/design/ASCII_RENDERER.md) | Per-tile glyph variation, animated effects, lighting, color palettes |
| [SAVE.md](docs/design/SAVE.md) | Save/load architecture — schema versions v0–v6, persisted shape, save/load triggers, permadeath flow, serde compatibility contract |
| [SPAWNING.md](docs/design/SPAWNING.md) | Voronoi-cell monster spawner: pack-per-cell placement, FastNoise tunables, per-floor budget, exclusion buffer, pipeline ordering, tuning knobs |

**Key design constraints:**
- All loot comes from chests — no floor drops
- 4 damage types: Physical, Poison, Fire, Lightning
- Win condition: Find Amulet of Yendor on floor `MAX_FLOOR` (currently 2), climb back up to the Town Portal on floor 0
- **Character system (Phase 2, see [CHARACTER.md](docs/design/CHARACTER.md)):** the game is mid-pivot from the original Brogue-style "no chargen, no attributes" model toward a D&D-flavored RPG layer. Players pick 1 of 3 races (Human/Dwarf/Elf) and 1 of 4 classes (Warrior/Rogue/Mage/Ranger) at character creation; attribute scores are fully race + class sum (no allocation step). Three attributes: STR/DEX/INT (CON removed). Modifier formula `(score - 16) / 2` — anchored at 16 so chargen mods are typically negative and players grow into them. HP scales from race + level via `floor(race_hp_mod × (8 + 11 × XL / 2))`.
- **XP and levels.** Player gains XP from kills (anti-farming dropoff: monsters 5+ levels below give 0 XP); level cap 27. Level-up recomputes HP, heals to full, fires a particle, and may queue ASI prompts (racial schedule every 4 levels — `Race.gain_schedule`; player-choice at L3/9/15/21/27 → +2 free points). ASI prompts route through `InGameState::AsiSelect` (DCSS-style inline modal).
- **Symmetric combat is partially broken:** the player now has `Race`, `Class`, `Attributes`, `Level`, `Experience` components; monsters have `MonsterTier` but no attributes. Monster-side parity (save bonuses, skills) lands in later phases. Don't write code that *requires* monsters to have a `Race` or `Attributes` component.
- **Saves are deferred.** No saving throws on player or monsters yet.
- **Skills are deferred.** No use-trained weapon/spell skill tiers yet. The HP formula's missing Fighting term lands when the Skills phase ships.
- **Mana is deferred.** Player magic still uses staves (Brogue-style charges); INT_mod adds to staff zap damage as a hook for the future mana pool.
- **Monster combat-stat rebalancing is deferred.** Phase 2 introduced a much wider chargen-mod range; monster HP/damage values designed against the Phase 1 power curve will feel off until they're tuned.

## Project Structure

```
src/
  main.rs                # App entry, plugin registration
  constants.rs           # Shared constants (tile size, Z-layers, action costs)
  components.rs          # Shared ECS components (Position, Viewshed, Monster, etc.)
  character/
    mod.rs               # CharacterPlugin, CharacterChoice resource, public exports
    race.rs              # Race enum component + RaceTrait passive enum
    class.rs             # Class enum component + Attribute enum (STR/DEX/CON/INT)
    attributes.rs        # Attributes component + ability_mod + compose / derive helpers
    asset.rs             # RaceManifest / ClassManifest RON schemas + handle resources
  assets/
    mod.rs               # Asset loading plugin, RON manifests, sprite handles
  game/
    mod.rs               # GamePlugin, AppState (Loading/Menu/InGame/GameOver), InGameState
    actions.rs           # Action enum, intent messages (Movement/Melee/Wait/Door)
    abilities.rs         # Monster ability definitions and cooldown system
    ai.rs                # MonsterAI re-exports + refresh_monster_modes_system (mode FSM tracking — chase, leash, waypoint snapback)
    ascii_mode.rs        # ASCII rendering mode toggle
    camera.rs            # Camera follow and visibility toggle
    combat/
      mod.rs             # Bevy adapter: attack_resolution_system, combat_trigger_system, combat_log_system, death_system, regen, GodMode, plugin
      resolve.rs         # Pure attack resolver (no Bevy, no ECS). Owns hit math, damage math, shield block, armor roll
    effects.rs           # Effect application (item use, on-hit effects)
    enchantment.rs       # Enchant scroll system (+1 to any item)
    factions.rs          # Faction definitions and hostility matrix
    fleeing.rs           # Sticky `Fleeing` overlay component + damage_triggers_flee + maybe_exit_fleeing (entry/exit transitions for the new FSM state)
    gas.rs               # Gas layer system (poison clouds, spread, decay, FOV blocking)
    items.rs             # Item components, equip/unequip/drop handlers
    magic.rs             # Magical effect processing
    particles.rs         # Visual particle effects
    prop_effects.rs      # Prop trigger vocabulary (TileEffect, PropTrigger, EverFired) + bump/step/decoration dispatchers — RFC 0002
    ranged.rs            # Ranged attack handling
    spawner.rs           # Entity spawning helpers
    squad.rs             # Squad system, shared alerting, leader mechanics
    staves.rs            # Staff charge system, staff usage
    stats.rs             # CombatStats, SpeedStats
    systems.rs           # FOV update, entity transform sync, monster visibility
    targeting.rs         # Target selection for abilities and staves
    turns.rs             # TurnOrderPlugin, TurnManager, TurnState FSM
    water.rs             # Water effects (item sweep, movement cost, extinguish)
    xp.rs                # Level / Experience / MonsterTier / XP curve / level-up handler
    skills.rs            # Skill / WeaponSkill / Skills / SkillXp / SkillTraining / training-mode allocator
    stealth.rs           # Stealth system (Phase 4): compute_*_mod, perception_tick_system, squad propagation, Backstab gate, use-counter, StealthPlugin
    tactics/
      mod.rs             # TacticsPlugin re-exports
      resolve.rs         # Pure tactic resolver (no Bevy, no ECS). TurnSnapshot / TacticAction / TacticStateDelta / Tactic trait / resolve_turn entry point. Mirrors combat/resolve.rs
      dispatch.rs        # Bevy adapter: TacticBrain component, MapPathContext, BracketRngAdapter, build_snapshot, apply_state_delta, write_intent, tactic_dispatch_system, validate_tactic_names_system, TacticsPlugin
      library/
        mod.rs           # TACTIC_REGISTRY: name → &'static dyn Tactic lookup, ALL_TACTIC_NAMES, TERMINAL_TACTIC_NAME constant
        wait.rs          # WaitTactic (unconditional fallback)
        combat.rs        # MeleeAdjacent
        flee.rs          # FleeAtLowHp, FleePanicked (sticky), KiteRetreat
        movement.rs      # HuntVisibleTarget, PursueLastKnownPosition
        ranged.rs        # RangedAttack, UseAbility (defers to try_use_ability_world)
        squad.rs         # SquadLeash
        aquatic.rs       # SubmergeOrSurface
        idle.rs          # IdleMove — dispatches on IdleMovement enum (PathToRandomTile / Patrol / Roam / Stationary)

  map/
    mod.rs               # MapPlugin re-exports
    map.rs               # Map resource, tile visibility systems, GRID_SIZE (16x16), MAP_SIZE (80x60)
    tile.rs              # Tile re-exports + TileVisibility/TileExplored components + sprite spawning + chasm_fall_reaction_system
    light.rs             # Game adapter for engine's lighting (re-exports + candle sprite animation + LightPlugin scheduling)
    dungeon.rs           # DungeonPlugin, Floor resource, floor cache, unified player_transition_system
    floor_materializer.rs # Converts BuilderMap data into ECS entities (incl. MapExitTile stamping)
    ascii_renderer.rs    # Themed glyph + colour resolution (reads FloorTheme)
    world.rs             # Overworld topology — FloorKind, GridDir, neighbor, edge/arrival positions, FloorTheme, MapExitTile, OverworldState
    builders/
      mod.rs             # BuilderChain, BuilderMap, floor_builder dispatch (town | forest | temple)
      town.rs            # All town building: TownLayoutBuilder (water + piers + buildings + Pub-door spawn + quest-board) + TownPortalBuilder + TownDownStairsBuilder (east border) + TownPathBuilder (A* organic road network) + TownNpcBuilder (reads hardcoded `TOWN_NPC_SPAWNS` const, queues SpawnEntry with PatrolRoute)
      forest.rs          # ForestTerrainBuilder (depth-tuned CA, depths 1-4 + west/east end-clearings + spine corridor) + ForestStairsBuilder (west `<`, east `>` for Forest 1-3; off-spine random `>` on Forest 4 — the hidden temple entrance)
      temple.rs          # TempleLayoutBuilder (sealed stone interior — east-west corridor + 7×7 sanctum chamber) + TempleStairsBuilder (UpStairs at entry, Amulet at sanctum centre)
      algorithms.rs      # Re-export of `roguelike_engine::map::builders::algorithms` (BlobGenConfig, Grid, CA helpers)
      decoration_propagator.rs # Game adapter — DecorationPropagator lives in engine
      voronoi_spawner.rs # Voronoi-cell pack spawner; works on any walkable map (forest, future dungeon). See docs/design/SPAWNING.md
      prefab_placer.rs   # Hand-designed room layout stamping (currently unused; preserved for future authored rooms)
      item_spawner.rs    # Places chests with loot (currently unused by overworld pipelines)
      candle_spawner.rs  # Places light source entities (currently unused by overworld pipelines)
      exit_points.rs     # Engine `DistantExit` wrapper + final-floor amulet placement (currently unused by overworld pipelines)
  save/
    mod.rs               # Save/load system (RON format, permadeath deletion)
  menu/                  # Main menu plugin
  player/
    mod.rs               # Player plugin, input handling, movement
  ui/
    mod.rs               # UiPlugin, InGameState substates for UI screens
    registry.rs          # UiScreen trait, ScreenRegistry, central hotkey dispatcher, key-collision detector
    game_log.rs          # GameLog resource, GameLogMessage
    inventory.rs         # Inventory screen (InGameState::Inventory)
    character_info.rs    # Character info screen (InGameState::CharacterInfo)
    monster_info.rs      # Monster inspection overlay
    nearby.rs            # Nearby entities sidebar
    hover_info.rs        # Mouse hover tooltips
    enchant_select.rs    # Enchant scroll target selection UI
    staff_select.rs      # Staff targeting UI
    log_history.rs       # Scrollable game log history
    menu.rs              # In-game pause/options menu
    modal.rs             # Reusable modal dialog component
    asi_modal.rs         # DCSS-style ASI prompt (InGameState::AsiSelect)
    skill_screen.rs      # Skill training screen (InGameState::SkillScreen, key M)
    cheat_menu.rs        # Debug cheat menu
```

## Key Architectural Patterns

### ECS & Bevy Conventions
- All shared component types live in `components.rs`; game-specific components in their domain module
- Systems run in `Update` gated by `run_if(in_state(...))` — always scope systems to the correct `AppState`/`TurnState`
- Messages (events) use Bevy's `Message` / `MessageWriter` / `MessageReader` pattern (not the old `EventWriter`/`EventReader`)
- Use `Query::single()` not `.iter().next()` when expecting exactly one entity

### Character System (Phase 2)
- `AppState`: `Loading → Menu → CharacterCreation → InGame` (with `GameOver`/`Victory` as terminal states). The character creation screen is its own top-level state — see `src/ui/character_creation.rs`.
- `CharacterChoice { race, class }` resource (Phase 2 — no free_points; chargen no longer has an allocation step). The character creation UI writes it on "Begin Descent"; the save-load path overwrites it from `PlayerSaveData` before player spawn (`spawn_dungeon`'s load arm, see `SpawnDungeonExtras::character_choice`).
- The player spawner ([src/player/mod.rs](src/player/mod.rs)) reads `CharacterChoice` plus `RaceManifest` / `ClassManifest` to:
  1. `compose_attributes(race, class)` → final `Attributes` (just race + class sum; no allocation)
  2. `derive_stats(race, attrs, 1)` → initial `Dodge` (DEX_mod) and `Health.max` (race × level HP formula). `HitBonus` and `DamageBonus` are baked at 0 — attribute mods are added **dynamically** at hit-check/damage-roll time, branching on `AttackIntentMessage.source` *and* the equipped weapon's `weapon_skill` tag: DEX for ranged or finesse melee (Short/Long Blades); STR for any other melee (axes, fists, staff bash); 0 otherwise. The pure helper is `attack_attribute_bonus(source, finesse, attrs)`; the `finesse: bool` is computed at the call site in [src/game/combat.rs](src/game/combat.rs) from the weapon-skill tag.
  3. Race-specific spawn effects: **Stoneblood** (Dwarf 50% poison resist), **Keen Senses** (Elf +2 vision range). **Adaptive** (Human's "any stat at racial schedule") is informational; the schedule itself drives the gain.
  4. Player spawns at `Level(1)`, `Experience(0)`.
- Equipment continues to bump `HitBonus`/`Dodge`/`DamageBonus` incrementally on equip/unequip.
- INT contributes to staff zap damage (clamped at 0) via `handle_zap_staff` in [src/game/staves.rs](src/game/staves.rs).
- Modifier formula: `(score - 16) / 2` (anchored at 16 — chargen mods typically negative, players climb into positive).

### XP / Levels (Phase 2, [src/game/xp.rs](src/game/xp.rs))
- `Level(u32)` and `Experience(u32)` on the player. `MonsterTier(u32)` on every monster (from `MonsterAsset.tier`, default 1).
- XP curve: `100·(L-1)² + 50·(L-1) + (10·(L-1)³)/8`. Level cap 27.
- XP grant: `award_xp_on_death` reads `DeathEvent` where `killer == player`, computes `xp_reward(monster_tier, player_level)` (anti-farming: 0 XP if monster ≥5 levels below).
- `process_level_thresholds` increments `Level` and fires `LevelUpEvent` for each threshold crossed.
- `handle_level_up` recomputes HP from the race-level formula (heals to full), spawns a gold "LEVEL UP!" floating-text particle, and queues `PendingAsi` for stat-gain prompts:
  - Racial schedule (every `Race.gain_schedule.interval` levels)
  - Player-choice ASI (L3, 9, 15, 21, 27 → +2 free points)
  - If both fire on the same level, the second is held in `QueuedAsi` and drains after the first ASI resolves.
- ASI prompt UX: `InGameState::AsiSelect` (DCSS-style inline modal). Player presses S / D / I to spend a point. Disallowed letters greyed out.
- Save schema v4 persists `level` and `experience`.
- **Per-monster tier values are not authored yet** — every monster ships at tier 1, so XP rewards are uniform until a balancing pass. The anti-farming dropoff still works against player level.
- See [docs/design/CHARACTER.md](docs/design/CHARACTER.md) §Level Progression for the canonical writeup. Race/class tables are test-enforced to match `races.ron` / `classes.ron` — see `.claude/rules/character-writeup-required.md`.

### Skills (Phase 3, [src/game/skills.rs](src/game/skills.rs))
- 9 skills: Fighting, Axes, ShortBlades, LongBlades, RangedWeapons, Armor, Dodging, Shields, Evocations. Float levels `[0.0, 27.0]`; effects unlock at integer breakpoints via `floor(skill/4)`.
- ECS data on the player: `Skills` (per-skill level), `SkillXp` (cumulative per-skill XP), `SkillTraining` (per-skill `Normal`/`Focused`/`Disabled`). Resources: `SkillXpPool` (unallocated), `SkillUseCounters` (Auto-mode weights), `TrainingMode` (global `Auto` | `Manual`).
- XP flow: `award_xp_on_death` adds half the character XP reward to `SkillXpPool`. `allocate_skill_xp` drains the pool every frame per training settings — Auto mode weights by use counters, Manual mode splits evenly (×2 for Focused). Per-skill XP is divided by `aptitude_multiplier(race_apt)` before being added — positive aptitude = faster training. `update_skill_levels` recomputes `Skills` levels from `SkillXp` via the DCSS XP table (50 → 24,325 points across 27 levels).
- Combat: `weapon_skill_bonus(weapon, source, skills)` and `fighting_melee_bonus(source, skills)` are added dynamically in `hit_check_system` and `damage_roll_system` alongside `attack_attribute_bonus`. Armor is a **random roll** (`0..=armor_max + floor(Armor/4)`) applied to Physical damage only. **Shield blocks** use a per-attack check: `d20 + floor(Shields/4) + Block(SH) >= 17` (DC). On pass the hit is fully negated for **any** damage type — shields are the sole defense vs. magical damage. Each shield (Buckler/Kite/Tower) has a `max_blocks` budget (1/2/3) capping successful blocks per turn, tracked in `ShieldBlocksUsed` and reset on the wearer's own `ActionFinishedEvent`. Shields also impose a `dodge_bonus: -1/-3/-5` encumbrance penalty. Use counters bump on every successful melee/ranged hit (Fighting + weapon skill), on damage taken while armored (Armor), on miss (Dodging), and on each **successful** shield block (Shields).
- HP formula gains the DCSS Fighting term: `+ Fighting × XL/14 + (1 + Fighting × 3)/2` inside the `race_hp_mod` multiplier. Recomputed at every `handle_level_up`.
- Staves: `handle_zap_staff` adds `floor(Evocations/4)` to staff damage alongside `INT_mod`. The combined sum is clamped at 0. Use counter bumps on every fired zap.
- Skill screen UI: `InGameState::SkillScreen` (key `M`). DCSS-style listing with state badges (`+`/`*`/`-`), aptitude column, pool counter, mode toggle (`/`).
- Save schema v5 persists `skills`, `skill_xp`, `skill_training`, `skill_xp_pool`.
- See [docs/design/SKILLS.md](docs/design/SKILLS.md) for the canonical writeup. Class `starting_skills` and race `aptitudes` are test-enforced — see `.claude/rules/skill-writeup-required.md` (next phase) and the maintenance contracts in `src/character/asset.rs`.

### Stealth & Awareness ([STEALTH.md](docs/design/STEALTH.md))
- Per-perceiver `Awareness` component (engine-side, `roguelike_engine::stealth::Awareness`) maps `target_entity → AwarenessState`.
- 3 states: `Hidden | Searching | Aware`. **Aware is sticky** — no rolls fire against an Aware target until LOS is lost; then it drops to Searching with a ~20-turn timer.
- Opposed d20 roll fires on each perceiver's turn against non-Aware visible targets in [src/game/stealth.rs](src/game/stealth.rs).
- `MonsterAIMode` is driven by `Awareness`: the engine's `MonsterAI::update_mode_from_awareness` maps `Aware → Hunting`, `Searching → Idle` (wakes Asleep), `Hidden → preserve`. The game-side `update_mode` in [src/game/ai.rs](src/game/ai.rs) adds a viewshed fast path on top: any monster with current LOS to a hostile player is forced to `Aware + Hunting` in the same turn.
- Backstab triple-damage gates on `AwarenessState::Hidden` only (combat.rs). Asleep monsters resolve as Hidden by default.
- `NoiseMap` resource ships in V1 with decay-by-1 tick but no producer. V2 noise phase plugs in Dijkstra populator.
- Save: degraded persistence (`Aware → Searching{last_known_pos}` on save, `Searching` keeps its timer offset). See [SAVE.md](docs/design/SAVE.md) for schema version history.

### Tactic Registry ([TACTICS.md](docs/design/TACTICS.md))
- **All monster AI runs through the tactic registry.** The legacy FSM mega-dispatcher (`execute_monster_ai`) and the GOAP planner were deleted; their behaviors live as discrete `Tactic` impls in `src/game/tactics/library/`.
- **Three-layer architecture.** Layer 1: state lives in `MonsterAI` (mode, last-known position, tuning knobs) + game-side `Fleeing` overlay component. Layer 2: small transition systems (`refresh_monster_modes_system`, `damage_triggers_flee`, `maybe_exit_fleeing`) update the mode each turn. Layer 3: per-monster `TacticBrain.tactics` (a `&'static [&'static dyn Tactic]` slice) is evaluated top-to-bottom each turn; the first non-`None` wins.
- **Pure resolver + Bevy adapter pattern** mirroring [src/game/combat/resolve.rs](src/game/combat/resolve.rs). `resolve.rs` is pure Rust (no Bevy, no ECS) — `TurnSnapshot` in, `TurnOutcome` (action + state delta) out. `dispatch.rs` is the adapter — builds the snapshot from ECS components, calls `resolve_turn`, writes the matching intent message, applies the state delta to live components. Snapshots have no `Default` to catch wiring bugs at compile time.
- **Sticky Fleeing mode** (game-side). Inserted by `damage_triggers_flee` when an actor with `flee_at_hp_percent > 0` drops below threshold (fires from any reactive state, including Idle — wandering creatures struck from stealth panic without first transitioning through Hunting). Exit via `maybe_exit_fleeing` requires 10+ elapsed turns + no visible threat + HP recovered by 0.15 hysteresis margin. Synthesized as `AiMode::Fleeing` in the resolver's snapshot; the `FleePanicked` tactic gates on it and ignores HP (entry/exit transitions own the mode lifecycle).
- **`IdleMove` tactic** handles non-combat movement via the per-monster `idle_movement: IdleMovement` knob (default `PathToRandomTile`). Variants: `PathToRandomTile` (pick random walkable tile, walk there, repeat), `Patrol` (waypoint loop from `PatrolRoute::Waypoint`), `Roam` (bounded random walk from `PatrolRoute::AreaRoam`), `Stationary` (never move when idle). The asset declares what kind of wander; the spawn-time builder attaches the `PatrolRoute` with the bounds/waypoints (separation of content-time and placement-time concerns).
- **Startup name validation.** `validate_tactic_names_system` runs once after `MonsterManifest` loads. Panics if any `monsters.ron` entry references an unknown tactic name in `ALL_TACTIC_NAMES` or fails to terminate with `"Wait"`. Catches typos at boot rather than at first spawn — same loud-failure pattern as `detect_screen_key_collisions`.
- **Run order each turn** (Brain phase, chained): `perception_tick_system` → `refresh_monster_modes_system` → `tactic_dispatch_system` → `marker_dispatch`. Damage-triggered flee fires in `ResolveActions` after combat application. Tactics always see a fresh mode.
- **Adding a new tactic.** One new file in `library/` + one `const` reference + one `lookup_tactic` arm + one row in `ALL_TACTIC_NAMES`. The RON references it by name. Each tactic is a zero-sized struct implementing `Tactic { fn name, fn evaluate }`. Tests use hand-built snapshots via `test_support::test_actor` + `ToyPaths` / `BlockedPaths` — no Bevy `App` required.

### Turn System
- `TurnState`: `Waiting → NextTurn → PlayerInput → Processing → NextTurn`
- `TurnManager` resource holds a sorted `Vec<(Entity, u32)>` turn queue keyed by game time
- Actors emit intent messages (`MovementIntent`, `MeleeIntent`, etc.); execution systems handle them
- `ActionFinishedEvent { base_cost }` re-inserts actors into the queue — every actor **must** emit this or the turn loop stalls
- `SpeedStats::delay` multiplies the base cost (lower = faster)

### Map System
- Two parallel representations:
  1. `Map` resource — pure data (tiles, width, height, depth); drives game logic and pathfinding
  2. ECS tile entities (`TileMarker`) — handle rendering, visibility, sprites
- `Tile` is a value type with two layers: `TerrainType` + `LiquidType`
- `BuilderChain` composes one `InitialMapBuilder` + N `MetaMapBuilder`s; call `build_map()` to run the pipeline
- `floor_builder()` in [src/map/builders/mod.rs](src/map/builders/mod.rs) dispatches on `FloorKind` from [src/map/world.rs](src/map/world.rs):
  - **Town** (floor 0) → `town_builder`: `TownLayoutBuilder → TownPortalBuilder → TownDownStairsBuilder → TownPathBuilder` — water + piers on the west edge, scattered themed buildings (Pub, Smithy, Alchemist, Temple, Houses, Hovel — each with role-specific interior props), a centre `Portal` (win-condition return), an east-border `DownStairs` into Forest 1, and an **organic A*-style dirt road network** connecting the east stair, the centre Portal, every building door, and every pier inland end. Path A* uses per-tile random noise + a merge bonus so branches join the trunk instead of running parallel — paths wiggle and look hand-laid. Player spawns one tile in front of the Pub door; a `totem_pole` quest-board prop stands one more step toward centre.
  - **Forest** (floors 1..=`MAX_FLOOR - 1`) → `forest_builder`: `ForestTerrainBuilder → ForestStairsBuilder → VoronoiSpawner → DecorationPropagator` — cellular-automata trees with **two end-clearings** (west = `UpStairs` where the player arrives, east = `DownStairs` to the next floor) connected by a 1-tile corridor through the centre. CA parameters are tuned per depth across all four forest floors (50→62% alive, 4–5 rounds — deeper floors get denser/gnarlier). Forest 4 is a special case: the `DownStairs` is **not** at the east clearing — it's placed at a random walkable tile at least 6 tiles off the spine, hiding the temple entrance. Decoration density also ramps with depth (0.20 → 0.40).
  - **Temple** (floor `MAX_FLOOR`) → `temple_builder`: `TempleLayoutBuilder → TempleStairsBuilder` — a sealed stone interior carved from solid wall, with a 3-tile-tall east-west corridor opening into a 7×7 sanctum chamber at the east end. `UpStairs` at the corridor's west end (returns to Forest 4); Amulet of Yendor at the sanctum centre. No monster spawn entries yet — cultists ship in a future pass. The shape is designed to extend downward (sub-levels) without restructuring.
- **Map-to-map transitions** unified in `player_transition_system`: terrain-based stairs (`>` → `floor + 1`, `<` → `floor - 1`) are the standard path. A tile may carry an optional `MapExitTile` component for **explicit** destinations (currently unused but retained for future warps / fast-travel / scripted teleporters). Walking onto the `Portal` in town triggers Victory if the player carries the Amulet of Yendor; otherwise it just logs an atmospheric line. One `MapTransitionMessage { destination_floor, destination_pos: Option<Position> }` flows through `apply_map_transition` to swap floors, with `PendingArrival` honoured by `spawn_dungeon` when a destination position is set.
- **Transition-frame invariant: `spawn_dungeon` must set `NeedsExploredInit.0 = true` on every code path (load / restore / generate).** The flag gates `update_tile_visibility` off for one frame. Without it, the player's leftover `Viewshed.visible_tiles` from the OLD floor gets re-applied to the NEW floor's tile entities at the same `(x, y)` and stamps `map.explored_tiles[idx] = true` for tiles the player could never have seen on the new floor. Symptom: dim "memory" trees deep inside forest clusters that the player legitimately could never reach. Regression test: `update_tile_visibility_skips_when_needs_explored_init_is_true` in [src/map/map.rs](src/map/map.rs). A `debug_assert!` at the end of `spawn_dungeon` catches the same regression in dev builds.
- **Town paths** are marked with `Decoration::Custom { id: TOWN_PATH_DECO_ID }` (defined in [src/map/world.rs](src/map/world.rs)); `themed_tile_display` / `themed_tile_bg` substitute a packed-dirt glyph + colour. The underlying terrain stays `Floor` so movement is unaffected.
- **`FloorTheme` resource** (`Dungeon | Town | Forest | Temple`) is set by `spawn_dungeon` on every materialisation and read by the ASCII renderer ([src/map/ascii_renderer.rs](src/map/ascii_renderer.rs)) to override Wall/Floor glyphs and colours per biome (forest walls `♣` green, town walls `▓` brown, temple walls `▒` cold grey). No new `TerrainType` variants needed.
- **`OverworldState` resource** is currently an empty struct — kept as a home for future per-run state (faction influence, NPC progress, quest flags). The 3×3 overworld grid (`GridDir`, `CardinalDir`, `temple_entrance_*`, border stair clusters) was removed when the game pivoted back to traditional linear floors.

### NPCs (Phase 1, see [NPCS.md](docs/design/NPCS.md))
- **Reuse, don't fork.** NPCs are monsters with a non-hostile faction — same `MonsterAsset` shape in `monsters.ron`, same `MonsterAI` / `TacticBrain` / `Position` / `Faction` / save snapshot path. The only thing distinguishing them is `faction: "Townsfolk"`.
- **`Townsfolk` faction** is Allied to Player + Neutral to all monster factions. Player bumps fall through `resolve_bump`'s hostility check to `BumpResult::BlockedByCollider` — no melee swing.
- **Faction-filtered visibility.** The tactic snapshot builder filters `visible_enemies` through the `FactionMatrix`; non-hostile actors never appear in an NPC's enemy list, so `HuntVisibleTarget` / `MeleeAdjacent` / etc. all return `None` against the player. Allied NPCs see the player but stay non-hostile.
- **`Asleep → Idle` wake gate.** Every monster spawns with `MonsterAI::default().mode == Asleep`. The `is_player_hostile_target` gate in [src/game/ai.rs](src/game/ai.rs) `update_mode` early-returns for non-hostile actors so they never escalate to `Hunting`, but it now also calls `non_hostile_mode_adjustment` to promote `Asleep → Idle` on the first mode-refresh tick. Without this promotion the `IdleMove` tactic — gated on `AiMode::Idle` — would never fire and NPCs would freeze at their spawn point forever.
- **Placement is separated from NPC identity.** The shipping roster lives in the [`TOWN_NPC_SPAWNS`](src/map/builders/town.rs) const inside `town.rs` — a `&[TownNpcSpawn]` with `(npc, count, placement)` triples. `TownNpcBuilder` reads the const directly and queues `SpawnEntry`s with the appropriate `PatrolRoute` (`AreaRoam` for drunks, future `Sentry` for vendors). The NPC asset has no notion of "pier" or "building interior" — placement lives in the town builder. Previously hot-loaded from `assets/town_npcs.ron`; consolidated when it became clear the roster never diverged from the layout that produced it.
- **Roaming** uses the `IdleMove` tactic's `Roam` variant (`idle_movement: Roam` on the asset) plus the `PatrolRoute::AreaRoam { min, max }` component the builder attaches at spawn. The asset declares "I roam"; the builder declares "where". `IdleMove::Patrol` follows the same pattern for waypoint-based NPCs.
- Adding a new NPC = one stat block in `monsters.ron` + one entry in `TOWN_NPC_SPAWNS` (in [src/map/builders/town.rs](src/map/builders/town.rs)). Zero code change unless the placement strategy is new (a new `TownNpcPlacement` variant + placement helper).

### Tile Mutation Pipeline (engine-owned)
- Mutation messages (`TileMutationMessage`, `DecorationMutationMessage`, `LiquidMutationMessage`) and their apply systems live in `roguelike_engine::map::mutation`. The engine plugin `MapMutationPlugin` registers them; game configures `MapMutationSet` ordering inside `ProcessingPhase::Cleanup`.
- The engine apply systems do **universal data sync only**: write `Map`, sync the tile entity's terrain/liquid component, mark `Viewshed.dirty` + `LightSources.dirty`, toggle `Collider`, insert into `PromotionCooldown`, and apply universal physics (`Decoration::CrackedFloor` → `TerrainType::Floor`). The `fungal_light` helper is retained but no shipping decoration emits light — rewire `apply_decoration_mutations` if a future plant variant needs it.
- **Game-specific reactions** to a mutation belong in a system that reads the same message and runs `.after(MapMutationSet)`. Current reaction: `chasm_fall_reaction_system` in [src/map/tile.rs](src/map/tile.rs) (player/monster fall, lava-kill, forced floor transition on player fall).
- `TilePromotionPlugin` (engine, in `roguelike_engine::map::promotion`) ships the per-turn promotion tick. Game configures `TilePromotionSet` to run inside `ProcessingPhase::Cleanup` before `MapMutationSet`. See `docs/design/TILE_PROMOTION.md`.

### Lighting (engine-owned)
- `LightMap`, `LightSources`, `LightSource` component, Bresenham accumulation, and `LightingPlugin` live in `roguelike_engine::lighting`. The game's [src/map/light.rs](src/map/light.rs) is a thin adapter that re-exports + adds candle sprite animation + configures `LightingSet` ordering relative to `SpawnDungeonSet` and `AppState::InGame`.
- Engine apply-systems write `LightSources.dirty = true` on opacity-flipping terrain mutations, so light recomputes automatically when doors open/close, walls collapse, etc.

### Tile Layers
- **Terrain**: Wall, Floor, DownStairs, UpStairs, Empty, Door, OpenDoor, LockedDoor, Portal
- **Liquid**: None, ShallowWater, Water (deep), Lava, Chasm
- `is_walkable()` requires both layers to be walkable
- `is_passable()` is used for connectivity (doors count, liquids are ignored)
- `is_opaque()` blocks FOV (walls, closed doors)

### Combat System
- d20 hit check: `d20 + hit_bonus >= 4 + target_dodge_bonus`
- Damage types: Physical (shield check + armor roll + resistance), Poison/Fire/Lightning (shield check + resistance only — no armor)
- Player attacks via weapons (active abilities on cooldown) and staves (charges)
- Monster attacks via melee + cooldown abilities
- See GAME.md for full damage pipeline
- **All damage paths flow through the pure resolver in [src/game/combat/resolve.rs](src/game/combat/resolve.rs).** The Bevy adapter `attack_resolution_system` in [src/game/combat/mod.rs](src/game/combat/mod.rs) reads `AttackIntentMessage`, builds `AttackerSnapshot` / `DefenderSnapshot` / `WeaponSnapshot` from ECS components, calls `resolve::resolve_attack`, and writes `DamageEvent` / `MissMessage` / log lines from the `AttackOutcome`. The resolver owns the bonus stack (attribute → weapon-skill → Fighting → Enraged/Terrified → Backstab via `damage_multiplier_bp`), shield block, and the armor *roll*. The engine's `damage_application_system` still owns armor *subtraction* and resistance percentage — the adapter writes the resolver's `armor_roll` value into `DamageEvent.armor`, and the engine reads the defender's `Resistances` component downstream. **Cleave splash** uses `resolve::apply_damage` per neighbor — each splash target rolls armor and may shield-block independently. **Staff zaps (Lightning chain, Fire AoE)** route through `resolve::roll_damage` + `resolve::apply_damage` per target. Staff damage curves convert from `(low, high)` ranges to dice expressions via `range_to_dice` in `src/game/staves.rs`; INT_mod + floor(Evocations/4) is pre-baked into `AttackerSnapshot.damage_bonus`. Shield blocks now apply to staff zaps and the adapter writes a `"{} blocks the {} blast!"` log line on a successful block. The `DefenderQueries` SystemParam in [src/game/combat/mod.rs](src/game/combat/mod.rs) bundles the per-defender component reads + the shield-budget write-back. See [docs/rfcs/0001-combat-resolver.md](docs/rfcs/0001-combat-resolver.md) for the migration RFC.

### Item System
- All items found in chests (placed by builder pipeline)
- **`ItemAsset` uses a tagged-union `kind:` field** (`Weapon(WeaponData)` / `Armor(ArmorData)` / `Staff(StaffData)` / `Consumable(ConsumableData)` / `Ring` / `Amulet`). Kind-specific fields live inside the variant; universal equip bonuses (`hit_bonus`, `damage_bonus`, `dodge_bonus`, `regen_bonus`, `max_hp_bonus`, `delay_modifier`, `vision_bonus`, `resistances`) stay flat because they apply across rings, amulets, armor, and weapons. The runtime `ItemProperties` component remains flat — the spawner unpacks the asset's variant into the flat shape, keeping every downstream reader unchanged.
- **`OnHitEffect`** (`src/game/items.rs`): wielder-agnostic on-hit procs declared on the weapon variant's `on_hit_effects: Vec<OnHitEffect>`. Variants: `PoisonStrike`, `BurningStrike`, `StunningBlow`, `SlowStrike`, `LifeDrain`. Applied by `handle_weapon_on_hit_effects` in `CombatReactionSet`, reading the attacker's `Equipment.weapon`. Works for player and any monster with `equipped: [...]` on its asset.
- **Monster equipment slots**: `MonsterAsset.equipped: Vec<String>` lists item names a monster spawns wielding. The spawner places an `UnequippedLoadout` marker on the monster; `process_monster_loadout_system` (in `src/game/items.rs`, runs every `Update` in `InGame`) looks up each item in `ItemManifest`, spawns a minimal item entity (no Position / no rendering, but with `Equipped` + `GameEntityMarker`), attaches it to the monster's `Equipment` slot, and applies stat overrides — equipped weapon's `damage` dice replace the monster's intrinsic `damage:`; armor `defense` adds to base Armor (or Block for shields); armor `dodge_bonus` adds to Dodge. Once the loadout is processed the marker is removed. Honors the symmetric-combat pillar — every weapon proc the player gets is available to any monster wielding the same item.
- **Equipment drops on death**: `drop_equipment_on_death` (in `src/game/mod.rs`, alongside `loot_drop_system` and `drop_inventory_on_death`) reads dead entities with `Health.current <= 0 + Equipment`. For each occupied slot it despawns the bare equipped item entity (which lacks rendering / Position) and re-`spawn_item`s a fresh floor item at the death tile, so the player can pick up everything the monster was wielding. This is the deterministic "guaranteed drop" pathway — `loot_table` rolls run independently for any *additional* random drops. Loadout items are sterile (no enchantment / runic), so re-spawning loses nothing; if monster equipment ever gains state, this is the system to revisit.
- Weapons differentiate via active abilities: Sword is the no-ability balance baseline; Dagger has Backstab (3× damage vs unaware); Axe has Cleave (lower damage but splashes the rolled damage to all 8 tiles around the attacker); Bow uses ranged targeting via `F`
- Staves use Brogue-style charges (enchanting adds charges + power)
- Armor provides either dodge bonus or flat armor (light vs heavy)
- Enchant scrolls: +1 to any item (the core strategic decision)
- Item actions (equip, unequip, drop) cost a turn via `player_action_pending`

### Rendering
- Grid cells are 16×16 pixels (`GRID_SIZE`)
- Map is 80×60 tiles (`MAP_SIZE`)
- Z-layers: Player=3, Monster=2, Item=1, Tiles=0
- Tile sprites are atlas-based, looked up via `TileManifest` RON asset
- Liquids spawn as child entities overlaid at z+0.1

## Dependencies
- `bevy 0.17` — game engine
- `bracket-lib` (forked) — FOV, pathfinding, geometry, RNG
- `roguelike_engine` (path: `crates/roguelike_engine`, workspace member) — shared roguelike infrastructure (turns, combat, status, abilities, AI, factions, squad, FOV, save, **map builders incl. decoration propagator**, **lighting**, **tile mutation messages + apply systems**, **tile promotion**)
- `petgraph 0.8` — graph analysis for choke map
- `rand 0.9` — random generation in map builders
- `bevy_common_assets 0.14` + `serde` — RON asset loading
- `bevy_save 0.17` — save/load support

## UI Architecture
- Game world is suspended while any UI screen is open — `handle_player_input` (movement) is gated on `InGameState::Running`
- Inventory and Character Info screens must never let keystrokes bleed through to the game world
- Every new UI substate must be added to this gate
- Inventory can only be opened when it is the player's turn (`TurnState::PlayerInput`)
- **Modal screens go through [src/ui/registry.rs](src/ui/registry.rs).** Each screen implements `UiScreen` with const associated items (`STATE`, `OPEN_KEY`, optional `OPEN_MODIFIERS` / `OPEN_GATE` / `HELP`) and a `build(app)` method that registers the screen's own OnEnter / OnExit / Update systems. `App::register_screen::<T>()` in `UiPlugin::build` wires the systems and records the hotkey + help entry in the `ScreenRegistry` resource. A single exclusive `dispatch_screen_hotkeys` system (gated on `InGameState::Running`) reads the registry and transitions to the matching state when a hotkey + modifier combination is pressed and the optional gate predicate passes. `detect_screen_key_collisions` (Startup) panics if two screens share the same `(key, modifiers)`. Use `close_on_toggle_or_escape::<Self>` as the standard close handler; the `open_gate_player_turn` helper is the canonical `OPEN_GATE` for screens that should only open during the player's turn (Inventory, CharacterInfo, SkillScreen, LogHistory). Event-driven screens (AsiSelect, EnchantSelect, ChasmConfirm) set `OPEN_KEY: None` and `HELP: None`; gameplay code transitions them via `NextState`. **Adding a new modal screen is one file (the screen plugin with its `UiScreen` impl) plus one `.register_screen::<T>()` call in `UiPlugin::build` plus one `InGameState` variant.** The help screen's "Screens" section is derived from `ScreenRegistry::help_entries` automatically — no manual edits to `help.rs` when adding a screen.

## Conventions
- Snake_case for files, modules, functions, variables; PascalCase for types
- Prefer `bracket-lib` RNG (`RandomNumberGenerator`) in builder code
- Map index arithmetic: `idx = y * width + x`; use `map.xy_idx(x, y)` / `map.idx_xy(idx)`
- `GameEntityMarker` — tag all in-game entities that should be despawned on game over
- `FloorEntityMarker` — tag entities that belong to the current floor only
- Item handlers live in the `TurnState::Processing` chain in `turns.rs`, not registered independently
- Every player action sets `turn_manager.player_action_pending` → `TurnState::Processing` → `player_ai_bridge` dispatches intent
- Free UI actions (open inventory, navigate) do NOT emit `ActionFinishedEvent`
