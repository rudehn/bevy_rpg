# roguelike_engine

A reusable engine for turn-based grid roguelikes, built on [Bevy](https://bevyengine.org/) 0.17.

Extracted from [The Veiled Tyrant](https://github.com/rudehn/bevy_rpg), a Brogue-inspired roguelike. The engine provides the plumbing every roguelike needs while leaving content and balance to the game.

## What it ships

- **Map generation** — `Map` resource, `Tile`/`TerrainType`/`LiquidType`/`Decoration` types, `DecorationRule`, bracket-lib FOV/pathfinding integration, and a full `BuilderChain` framework with 14 pure builders (BrogueLike generator, BSP, cullers, corridors, lakes, doors, exit placement, decoration propagator)
- **Tile mutation pipeline** — `TileMutationMessage` / `DecorationMutationMessage` / `LiquidMutationMessage` + `MapMutationPlugin` / `MapMutationSet`. Apply systems sync `Map`, tile entities, `Viewshed.dirty`, `LightSources.dirty`, `Collider`, and apply universal physics (CrackedFloor → Floor, Fungus → fungal light)
- **Tile promotion** — `TilePromotionPlugin` runs Brogue-style timed promotions (door auto-close, grass regrow, cracked-floor → chasm) once per `TurnEndEvent`
- **Lighting** — `LightingPlugin` produces a per-tile `LightMap` (intensity + tint) via Bresenham LOS from `LightSources`. Headless: data only, games render with it
- **Combat math** — `DamageType`, `Resistances`, `Health`, armor reduction, resistance percentages, status-buff multipliers
- **Turn scheduling** — `TurnManager` with variable-speed actors, pure dequeue logic, reinsert-time computation, `TurnEndEvent`
- **AI primitives** — `MonsterAI` state machine (sleep/hunt/idle), pure decision helpers (flee, kite, chase leash, erratic movement), GOAP planner framework
- **Squad coordination** — `SquadPlugin` with alert propagation, shared morale, tactical blackboard
- **Factions** — `FactionMatrix` (string-keyed, symmetric, data-driven from RON)
- **Components** — `Position`, `Viewshed`, `Name`, `Inventory`, `Faction`, `MovementMode`, `Collider`, `PatrolRoute`
- **Geometry** — `Direction` (8-way), Manhattan/Chebyshev distance, adjacency, AoE tiles
- **Dice** — `roll_dice_string`, `avg_damage_from_dice` (wraps bracket-lib)
- **Save framework** — Platform-agnostic save I/O (native RON + WASM localStorage)

## What it does NOT ship

Monsters, items, spells, ability systems, asset manifests, UI, or rendering. The engine is headless — games own their sprites and content.

## Quick start

```toml
[dependencies]
roguelike_engine = { git = "https://github.com/rudehn/roguelike_engine" }
```

```rust
use roguelike_engine::prelude::*;

// Generate a dungeon
let ctx = EngineBuilderMap::with_seed(1, 80, 60, "Floor 1", 42);
let mut chain = BuilderChain::new(ctx);
// ... add builders ...
chain.build_map();
let finished = chain.finish();
// finished.map is a ready-to-play Map with tiles, pathfinding, and FOV support
```

## Extension points

Type enums (`TerrainType`, `Decoration`, `StatusEffectKind`, ...) are closed and named — new gameplay shapes add a variant directly rather than going through a runtime id. Bevy plugins expose empty `SystemSet` markers that games configure with `.after()` / `.before()` / `.run_if()`.

## Dependencies

- `bevy` 0.17
- `bracket-lib` (forked — FOV, pathfinding, geometry, RNG)
- `petgraph` (graph analysis for choke-point detection)
- `rand` (used by some map builders)
- `serde` (serialization for save/load types)

## License

Dual-licensed under MIT or Apache-2.0, at your option.
