# bevy_rpg

A Brogue-inspired roguelike built with Bevy 0.17 and Rust.

## Build & Run

```bash
cargo run          # Run the game
cargo build        # Build without running
cargo check        # Fast type/borrow check (no codegen)
cargo clippy       # Lint
```

## Project Structure

```
src/
  main.rs              # App entry, plugin registration
  constants.rs         # Shared constants (tile size, Z-layers, action costs)
  components.rs        # Shared ECS components (Position, Viewshed, Monster, etc.)
  assets/              # Asset loading plugin, RON manifests, sprite handles
  game/
    mod.rs             # GamePlugin, AppState (Loading/Menu/InGame/GameOver), InGameState
    actions.rs         # Action enum, intent messages (Movement/Melee/Wait/Door)
    ai.rs              # MonsterAI component and logic
    camera.rs          # Camera follow and visibility toggle
    combat.rs          # Health, Damage, combat messages and systems
    level.rs           # XP, leveling up
    spawner.rs         # Entity spawning helpers
    stats.rs           # CombatStats, SpeedStats
    systems.rs         # FOV update, entity transform sync, monster visibility
    turns.rs           # TurnOrderPlugin, TurnManager, TurnState FSM
  map/
    mod.rs             # MapPlugin re-exports
    map.rs             # Map resource, tile visibility systems, GRID_SIZE (16x16), MAP_SIZE (80x60)
    tile.rs            # Tile struct (terrain + liquid layers), TerrainType, LiquidType, ECS tile spawning
    light.rs           # Lighting via bevy_light_2d
    dungeon.rs         # DungeonPlugin, Floor resource
    builders/
      mod.rs           # BuilderChain, BuilderMap, floor_builder pipeline
      brogelike.rs     # BrogueLikeBuilder — primary map generator (room types + corridor placement)
      algorithms.rs    # BlobGenConfig, Grid, cellular automata helpers
      choke_map.rs     # Topology analysis via petgraph
      lake_builder.rs  # Organic lake generation using blob algorithm
      diagonal_culler.rs  # Removes diagonally-unreachable wall tiles
      unseen_culler.rs    # Culls tiles unreachable from player start
      bsp_dungeon.rs      # Alternate BSP-based builder (unused in current pipeline)
      corridors.rs        # Corridor carving
      room_drawer.rs      # Room rendering onto BuilderMap
      start_point.rs      # Places player starting position
      exit_points.rs      # Places distant exit stairs
      monster_spawner.rs  # Populates spawn_list from spawn table
      candle_spawner.rs   # Places light source entities
  menu/                # Main menu plugin
  player/              # Player plugin, movement timer
  ui/
    mod.rs             # UiPlugin
    game_log.rs        # GameLog resource, GameLogMessage
    character_info.rs  # Character info screen (InGameState::CharacterInfo)
    cheat_menu.rs      # Debug cheat menu
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
- Current pipeline: `BrogueLikeBuilder → DiagonalCuller → StartPointBuilder → LakeBuilder → CandleSpawner → MonsterSpawner → UnseenCuller → DistantExit`

### Tile Layers
- **Terrain**: Wall, Floor, DownStairs, UpStairs, Empty, Door, OpenDoor
- **Liquid**: None, Water, ShallowWater, Lava
- `is_walkable()` requires both layers to be walkable
- `is_passable()` is used for connectivity (doors count, liquids are ignored)
- `is_opaque()` blocks FOV (walls, closed doors)

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
