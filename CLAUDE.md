# bevy_rpg — The Veiled Tyrant

A Brogue-inspired roguelike built with Bevy 0.17 and Rust. Descend 26 floors,
find the Amulet of Ascension, escape through the portal. Permadeath.

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
| [DUNGEON.md](docs/design/DUNGEON.md) | Map generation pipeline, terrain/liquid layers, decorations, lighting, floor structure |
| [ENCOUNTERS.md](docs/design/ENCOUNTERS.md) | Machine system (hordes → spawn table → machines), blueprints, trapped chests, lock & key |
| [ENEMIES.md](docs/design/ENEMIES.md) | Monster roster, factions, species, tier structure, per-monster identities |
| [ITEMS.md](docs/design/ITEMS.md) | Weapons (active abilities), staves (charges), armor, rings/amulets, potions, enchanting, runics |

### Per-system docs

| Doc | Covers |
|-----|--------|
| [TURNS.md](docs/design/TURNS.md) | Turn queue, TurnState FSM, SpeedStats delay model, ActionFinishedEvent contract, processing phases |
| [ABILITIES.md](docs/design/ABILITIES.md) | Monster ability triggers (on-hit/on-being-hit/on-death/passive), cooldown family, ExplodeEffect variants |
| [STATUS_EFFECTS.md](docs/design/STATUS_EFFECTS.md) | Burning, Poisoned, Slowed, Stunned, Hasted, Enraged, Entangled, Custom IDs, tick model, refresh policy |
| [FACTIONS.md](docs/design/FACTIONS.md) | Faction component, FactionMatrix, hostility lookup, cross-faction combat, default-Hostile gotcha |
| [RANGED.md](docs/design/RANGED.md) | Ranged attack pipeline, F-key targeting, weapon range, ammo, LOS gating |
| [SQUAD_AI.md](docs/design/SQUAD_AI.md) | Squad system, shared alerting, leader mechanics, morale-based fleeing |
| [FIRE.md](docs/design/FIRE.md) | Fire entities, spread, ignition chance, burn duration, water/gas interactions |
| [GAS.md](docs/design/GAS.md) | Gas types (Poison, Steam), volume, diffusion, decay, FOV blocking, ignition |
| [WATER.md](docs/design/WATER.md) | Shallow/deep water, movement cost, Submerged state, item drift, fire-water steam |
| [CHASMS.md](docs/design/CHASMS.md) | Chasm tile mechanics, fall damage, fallen-entity propagation across floors |
| [TILE_PROMOTION.md](docs/design/TILE_PROMOTION.md) | Cracked floor → chasm, grass regrowth, embers → ash, promotion cooldown |
| [LIGHT.md](docs/design/LIGHT.md) | Per-tile light intensity + color, Bresenham LOS, resource vs. entity-driven sources, dirty propagation |
| [ASCII_RENDERER.md](docs/design/ASCII_RENDERER.md) | Per-tile glyph variation, animated effects, lighting, color palettes |

**Key design constraints:**
- All loot comes from chests — no floor drops
- 4 damage types: Physical, Poison, Fire, Lightning
- Win condition: Find Amulet of Yendor on floor 26, climb back up to the Escape Portal on floor 1
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
    dice.rs              # roll_d20_with_race helper (thin wrapper after Halfling Lucky removed in Phase 2)
  assets/
    mod.rs               # Asset loading plugin, RON manifests, sprite handles
  game/
    mod.rs               # GamePlugin, AppState (Loading/Menu/InGame/GameOver), InGameState
    actions.rs           # Action enum, intent messages (Movement/Melee/Wait/Door)
    abilities.rs         # Monster ability definitions and cooldown system
    ai.rs                # MonsterAI component and logic
    ascii_mode.rs        # ASCII rendering mode toggle
    camera.rs            # Camera follow and visibility toggle
    combat.rs            # Health, Damage, combat messages and systems
    effects.rs           # Effect application (item use, on-hit effects)
    enchantment.rs       # Enchant scroll system (+1 to any item)
    factions.rs          # Faction definitions and hostility matrix
    gas.rs               # Gas layer system (poison clouds, spread, decay, FOV blocking)
    goap.rs              # Goal-Oriented Action Planning AI
    items.rs             # Item components, equip/unequip/drop handlers
    machines.rs          # Machine encounter runtime logic
    magic.rs             # Magical effect processing
    particles.rs         # Visual particle effects
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

  map/
    mod.rs               # MapPlugin re-exports
    map.rs               # Map resource, tile visibility systems, GRID_SIZE (16x16), MAP_SIZE (80x60)
    tile.rs              # Tile re-exports + TileVisibility/TileExplored components + sprite spawning + chasm_fall_reaction_system
    light.rs             # Game adapter for engine's lighting (re-exports + candle sprite animation + LightPlugin scheduling)
    dungeon.rs           # DungeonPlugin, Floor resource, floor cache
    floor_materializer.rs # Converts BuilderMap data into ECS entities
    builders/
      mod.rs             # BuilderChain, BuilderMap, floor_builder pipeline
      brogelike.rs       # BrogueLikeBuilder — primary map generator (room types + corridors)
      algorithms.rs      # BlobGenConfig, Grid, cellular automata helpers
      choke_map.rs       # Topology analysis via petgraph (chokepoints for machines)
      lake_builder.rs    # Organic lake generation using blob algorithm
      machine_builder.rs # Machine encounter placement in builder pipeline
      prefab_placer.rs   # Hand-designed room layout stamping
      decoration_propagator.rs # Game adapter — DecorationPropagator lives in engine
      diagonal_culler.rs # Removes diagonally-unreachable wall tiles
      unseen_culler.rs   # Culls tiles unreachable from player start
      isolated_area_culler.rs # Removes disconnected map regions
      pillar_culler.rs   # Removes isolated wall pillars
      cave_eroder.rs     # Cave wall erosion for organic shapes
      finish_doors.rs    # Final door placement/cleanup pass
      item_spawner.rs    # Places chests with loot
      monster_spawner.rs # Populates spawn_list from spawn table
      candle_spawner.rs  # Places light source entities
      start_point.rs     # Places player starting position
      exit_points.rs     # Places distant exit stairs
      corridors.rs       # Corridor carving
      room_drawer.rs     # Room rendering onto BuilderMap
      bsp_dungeon.rs     # Alternate BSP-based builder (unused)
  save/
    mod.rs               # Save/load system (RON format, permadeath deletion)
  menu/                  # Main menu plugin
  player/
    mod.rs               # Player plugin, input handling, movement
  ui/
    mod.rs               # UiPlugin, InGameState substates for UI screens
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
  2. `derive_stats(race, attrs, 1)` → initial `Dodge` (DEX_mod) and `Health.max` (race × level HP formula). `HitBonus` and `DamageBonus` are baked at 0 — attribute mods are added **dynamically** at hit-check/damage-roll time, branching on `AttackIntentMessage.source` (STR melee, DEX ranged). The pure helper is `attack_attribute_bonus(source, attrs)`.
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
- Current pipeline: `BrogueLikeBuilder → StartPointBuilder → LakeBuilder → DiagonalCuller → PillarCuller → FinishDoors → PrefabPlacer → MachineBuilder → IsolatedAreaCuller → CandleSpawner → MonsterSpawner → DecorationPropagator → DistantExit`

### Tile Mutation Pipeline (engine-owned)
- Mutation messages (`TileMutationMessage`, `DecorationMutationMessage`, `LiquidMutationMessage`) and their apply systems live in `roguelike_engine::map::mutation`. The engine plugin `MapMutationPlugin` registers them; game configures `MapMutationSet` ordering inside `ProcessingPhase::Cleanup`.
- The engine apply systems do **universal data sync only**: write `Map`, sync the tile entity's terrain/liquid component, mark `Viewshed.dirty` + `LightSources.dirty`, toggle `Collider`, insert into `PromotionCooldown`, and apply universal physics (`Decoration::CrackedFloor` → `TerrainType::Floor`, `Decoration::Fungus` ↔ `fungal_light` add/remove).
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
- Damage types: Physical (armor + resistance), Poison/Fire/Lightning (resistance only)
- Player attacks via weapons (active abilities on cooldown) and staves (charges)
- Monster attacks via melee + cooldown abilities
- See GAME.md for full damage pipeline

### Item System
- All items found in chests (placed by builder pipeline)
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
- `roguelike_engine` (path: `../roguelike_engine`) — shared roguelike infrastructure (turns, combat, status, abilities, AI, factions, squad, FOV, save, **map builders incl. decoration propagator**, **lighting**, **tile mutation messages + apply systems**, **tile promotion**)
- `petgraph 0.8` — graph analysis for choke map
- `rand 0.9` — random generation in map builders
- `bevy_common_assets 0.14` + `serde` — RON asset loading
- `bevy_save 0.17` — save/load support

## UI Architecture
- Game world is suspended while any UI screen is open — `handle_player_input` (movement) is gated on `InGameState::Running`
- Inventory and Character Info screens must never let keystrokes bleed through to the game world
- Every new UI substate must be added to this gate
- Inventory can only be opened when it is the player's turn (`TurnState::PlayerInput`)

## Conventions
- Snake_case for files, modules, functions, variables; PascalCase for types
- Prefer `bracket-lib` RNG (`RandomNumberGenerator`) in builder code; `rand` crate directly in `brogelike.rs`
- Map index arithmetic: `idx = y * width + x`; use `map.xy_idx(x, y)` / `map.idx_xy(idx)`
- `GameEntityMarker` — tag all in-game entities that should be despawned on game over
- `FloorEntityMarker` — tag entities that belong to the current floor only
- Item handlers live in the `TurnState::Processing` chain in `turns.rs`, not registered independently
- Every player action sets `turn_manager.player_action_pending` → `TurnState::Processing` → `player_ai_bridge` dispatches intent
- Free UI actions (open inventory, navigate) do NOT emit `ActionFinishedEvent`
