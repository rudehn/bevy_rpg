# CLAUDE.md — roguelike_engine

Bevy 0.17 headless engine for turn-based grid roguelikes. Vendored as a workspace crate inside [The Veiled Tyrant](../../).

## Build & Test

```bash
cargo check          # Fast type check
cargo test           # Run all tests (427+)
cargo clippy         # Lint
cargo bench          # Run benchmarks
cargo doc --no-deps  # Build docs
```

## Project Structure

All public items re-exported through `roguelike_engine::prelude::*`.

| Module | Purpose |
|--------|---------|
| `abilities/` | Ability/spell framework: `AbilityDef`, `TargetingRule`, `Abilities` component, cooldown/cost helpers |
| `ai/` | AI primitives: `MonsterAI` state machine, pure decision helpers, GOAP planner (`plan`, `WorldState`, `Goal`, `ActionDef`), pathfinding dispatch |
| `combat/` | Damage types, resistance math, `Health`, combat event pipeline (`DamageEvent`/`DeathEvent`/`HealEvent`) |
| `components/` | Shared ECS components (`Position`, `Viewshed`, `Collider`, `Inventory`, `Faction`, `PatrolRoute`...) + FOV system |
| `constants/` | Tile size, Z-layers, `BASE_ACTION_COST` |
| `dice/` | Dice notation wrappers: `roll_dice_string`, `avg_damage_from_dice` |
| `factions/` | Data-driven faction hostility matrix (RON-loadable `FactionMatrix`) |
| `geometry/` | Distance functions, `Direction` enum, AoE helpers |
| `lighting/` | Per-tile `LightMap` + `LightSources` resource, Bresenham LOS accumulation, `LightingPlugin` |
| `map/` | `Map` resource, 3-layer tile system (terrain/liquid/decoration), 14 procedural builders, `BuilderChain` pipeline, `DecorationRule`, `TileEntityIndex`, `MapMutationPlugin` (mutation messages + apply systems), `TilePromotionPlugin` + `PromotionCooldown` |
| `save/` | Platform-agnostic save I/O with schema versioning and migrations |
| `squad/` | Squad coordination: alerts, morale, blackboard, roles |
| `status/` | Status effect framework: `StatusEffects` component, tick system, DoT |
| `turn/` | Turn scheduling: `BinaryHeap`-based `TurnManager`, pure `dequeue_next_batch_pure`, `TurnEndEvent` |

## Key Architectural Patterns

- **Engine/game boundary** — Engine provides infrastructure + pure algorithms. Games own content, balance, UI, rendering, and integration.
- **Pure functions** — Combat math, geometry, AI decisions, turn scheduling are pure (no ECS, no Bevy World). Tested in isolation.
- **Closed enums** — Type enums (`DamageType`, `TerrainType`, `LiquidType`, `Decoration`, `StatusEffectKind`, `TargetingRule`, `WorldStateProp`) are exhaustive and named. New gameplay shapes add a named variant rather than going through a runtime id.
- **SystemSet markers** — Plugins expose empty `SystemSet`s (`SquadAlertSet`, `CombatEventSet`, `FovSet`, etc.) for games to configure with `.after()`/`.before()`/`.run_if()`.
- **Bevy 0.17 events** — Use `#[derive(Message)]`, `MessageWriter<T>`, `MessageReader<T>` (NOT old `EventWriter`/`EventReader`).
- **BuildContext trait** — Map builders are generic over `C: BuildContext`. Engine ships `EngineBuilderMap`; games wrap it with their own context.
- **Tile promotions** — Three-layer tiles with `on_step_promotion()` and `timed_promotion()` rules. The `TilePromotionPlugin` runs the per-turn tick; the `MapMutationPlugin` applies the resulting mutations.
- **Mutation = data sync only** — The engine's apply systems do universal data sync (Map ↔ tile entity ↔ viewshed ↔ lighting ↔ collider ↔ promotion cooldown) plus tile-data-driven physics (`Decoration::CrackedFloor` → `Floor`, `Decoration::Fungus` → fungal light). Game-specific reactions (e.g. chasm fall, lava kill) belong in game systems that read the same mutation messages and run `.after(MapMutationSet)`.
- **Headless lighting** — `LightingPlugin` produces a per-tile `LightMap`. The engine never renders; games consume the data for sprite tints, ASCII colors, etc.

## Conventions

- Snake_case files/modules/functions, PascalCase types
- Tests in `#[cfg(test)] mod tests` blocks within each file
- Deterministic RNG seeding in tests and builders (via bracket-lib `RandomNumberGenerator::seeded`)
- Map index arithmetic: `idx = y * width + x`; use `map.xy_idx(x, y)` / `map.idx_xy(idx)`
- All public items re-exported through `roguelike_engine::prelude::*`

## Dependencies

- **bevy 0.17** — ECS, plugins, scheduling
- **bracket-lib** (forked) — FOV, pathfinding, geometry, RNG
- **petgraph** — Graph algorithms (choke-point detection)
- **rand** — RNG in some builders
- **serde** — Serialization for save/load

## Related Project

Workspace member of The Veiled Tyrant (`bevy_rpg`). The game crate sits at the repo root; this engine crate lives at `crates/roguelike_engine/`. Migration notes tracking how the game crate adapts to engine changes are in `../../docs/ENGINE_MIGRATION.md`.
