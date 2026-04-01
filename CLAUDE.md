# bevy_rpg — The Veiled Tyrant

A Brogue-inspired roguelike built with Bevy 0.17 and Rust. Descend 10 floors,
find the Amulet of Ascension, escape through the portal. Permadeath.

## Build & Run

```bash
cargo run          # Run the game
cargo build        # Build without running
cargo check        # Fast type/borrow check (no codegen)
cargo clippy       # Lint
```

## Design Documentation

Design docs live in `docs/design/`. Read these before making gameplay changes:

| Doc | Covers |
|-----|--------|
| [GAME.md](docs/design/GAME.md) | Vision, core loop, win/lose, combat system, damage types, player stats, progression |
| [DUNGEON.md](docs/design/DUNGEON.md) | Map generation pipeline, terrain/liquid layers, water mechanics, decorations, lighting, floor structure |
| [ENCOUNTERS.md](docs/design/ENCOUNTERS.md) | Machine system (hordes → spawn table → machines), blueprints, trapped chests, lock & key |
| [ENEMIES.md](docs/design/ENEMIES.md) | Monster roster, cooldown-based abilities, factions, squad system, morale, out-of-depth |
| [ITEMS.md](docs/design/ITEMS.md) | Weapons (active abilities), staves (charges), armor, rings/amulets, potions, enchanting, runics |

**Key design constraints:**
- No spells/mana — player uses staves (Brogue-style charges), monsters use cooldown abilities
- All loot comes from chests — no floor drops
- 4 damage types: Physical, Poison, Fire, Lightning
- Win condition: Find amulet on floor 10, reach escape portal

## Project Structure

```
src/
  main.rs                # App entry, plugin registration
  constants.rs           # Shared constants (tile size, Z-layers, action costs)
  components.rs          # Shared ECS components (Position, Viewshed, Monster, etc.)
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
  map/
    mod.rs               # MapPlugin re-exports
    map.rs               # Map resource, tile visibility systems, GRID_SIZE (16x16), MAP_SIZE (80x60)
    tile.rs              # Tile struct (terrain + liquid layers), TerrainType, LiquidType
    light.rs             # Lighting via bevy_light_2d
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
      decoration_propagator.rs # Brogue-style BFS decoration spreading
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
    cheat_menu.rs        # Debug cheat menu
```

## Key Architectural Patterns

### ECS & Bevy Conventions
- All shared component types live in `components.rs`; game-specific components in their domain module
- Systems run in `Update` gated by `run_if(in_state(...))` — always scope systems to the correct `AppState`/`TurnState`
- Messages (events) use Bevy's `Message` / `MessageWriter` / `MessageReader` pattern (not the old `EventWriter`/`EventReader`)
- Use `Query::single()` not `.iter().next()` when expecting exactly one entity

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

### Tile Layers
- **Terrain**: Wall, Floor, DownStairs, UpStairs, Empty, Door, OpenDoor, LockedDoor
- **Liquid**: None, ShallowWater, DeepWater, Lava
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
- Weapons have unique active abilities per type (sword=Riposte, dagger=Backstab)
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
- `bevy_light_2d 0.8` — 2D lighting
- `petgraph 0.8` — graph analysis for choke map
- `rand 0.9` — random generation in map builders
- `bevy_common_assets 0.14` + `serde` — RON asset loading
- `bevy_save 0.17` — save/load support

## Conventions
- Snake_case for files, modules, functions, variables; PascalCase for types
- Prefer `bracket-lib` RNG (`RandomNumberGenerator`) in builder code; `rand` crate directly in `brogelike.rs`
- Map index arithmetic: `idx = y * width + x`; use `map.xy_idx(x, y)` / `map.idx_xy(idx)`
- `GameEntityMarker` — tag all in-game entities that should be despawned on game over
- `FloorEntityMarker` — tag entities that belong to the current floor only
- Item handlers live in the `TurnState::Processing` chain in `turns.rs`, not registered independently
- Every player action sets `turn_manager.player_action_pending` → `TurnState::Processing` → `player_ai_bridge` dispatches intent
- Free UI actions (open inventory, navigate) do NOT emit `ActionFinishedEvent`
