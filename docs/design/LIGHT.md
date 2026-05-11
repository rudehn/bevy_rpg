# Lighting

Per-tile light intensity + color, used by sprite rendering and ASCII tinting to give the dungeon a varied, atmospheric look. The lighting system is **engine-owned** (`roguelike_engine::lighting`) — the game crate ships only a thin adapter for sprite animation and Bevy schedule wiring.

## Design philosophy

Brogue's "lighting" gives flat dungeons character: a candle at the end of a corridor, a fungal patch glowing green, fire spreading and dying. The implementation deliberately does **not** use a graphics-API lighting solution. Instead, it computes a per-tile intensity + tint into a `LightMap` resource, and the renderer (sprite tints, ASCII colors) consumes that data. This keeps lighting headless and platform-agnostic — the engine produces the data, the game applies it however it wants.

## Data model

| Item | Where | Notes |
|------|-------|-------|
| `LightMap` resource | `roguelike_engine::lighting` | `{ values: Vec<f32>, colors: Vec<[f32; 3]> }` parallel to `Map.tiles`. |
| `LightSources` resource | `roguelike_engine::lighting` | `{ sources: Vec<LightSourceData>, dirty: bool }` — authoritative list. |
| `LightSourceData` | `roguelike_engine::lighting` | `{ x, y, radius, intensity, color, on_wall }`. |
| `LightSource` component | `roguelike_engine::lighting` | Entity-driven light (candle, prop). Synced into `LightSources` by `sync_entity_lights_system`. |
| `fungal_light(x, y)` | `roguelike_engine::lighting` | Helper that builds a `LightSourceData` with the fungal-glow constants. |
| `LightingPlugin` / `LightingSet` | `roguelike_engine::lighting` | Engine plugin + set marker for ordering hookup. |
| `AnimationTimer` + `animate_light_sources` | [src/map/light.rs](src/map/light.rs) | Game-side. Cycles candle sprite frames. |
| `LightPlugin` | [src/map/light.rs](src/map/light.rs) | Game adapter. Adds `LightingPlugin` and configures `LightingSet.run_if(in_state(AppState::InGame)).after(SpawnDungeonSet)`. Adds the candle animation system. |

## Tick pipeline

Every frame in `Update`, the engine's `LightingSet` runs (gated by the game's `AppState::InGame` configuration):

1. `sync_entity_lights_system` mirrors `LightSource` components into the `LightSources` resource. Resource-driven sources (e.g. fire, fungal glow) are managed directly via `LightSources::add` / `remove_at`.
2. `rebuild_light_map_system` short-circuits if `LightSources.dirty` is false. Otherwise it clears the dirty flag, iterates all sources, and accumulates per-tile intensity + tint into `LightMap` using Bresenham line-of-sight (with diagonal-corner blocking). After rebuilding, it marks every `Viewshed` dirty so visibility consumers pick up the new lighting on the same frame.

## Light source flavours

- **Resource-driven** (preferred for transient/algorithmic sources): code calls `light_sources.add(...)` directly. Used by fire ([src/game/fire.rs](src/game/fire.rs)) and by the engine's `apply_decoration_mutations` for fungal glow on `Decoration::Fungus`.
- **Entity-driven**: an entity carries a `LightSource` component. Candles use this so the candle entity owns its light's lifecycle. `sync_entity_lights_system` does a full resync of `on_wall: true` sources whenever any change is detected.

## Dirty propagation

`LightSources.dirty` is the trigger for a rebuild. It is set by:
- `LightSources::add`, `remove_at`, `remove_floor_sources` (any direct mutation).
- The engine's `apply_tile_mutations` whenever a terrain change flips `is_opaque()` (door open/close, wall→floor) — light needs to recompute through newly opened or blocked corridors.
- `sync_entity_lights_system` after a candle/prop change.

This keeps the rebuild cost amortised — a turn that touches no lighting state runs only the trivial early-return.

## Bresenham LOS notes

`has_los` walks Bresenham steps from the source to each tile in range. Two non-obvious behaviours:

- The **start cell itself** is never treated as opaque: a torch on a wall would be self-occluded otherwise.
- **Diagonal steps** check both adjacent cardinals — if both are opaque, light cannot squeeze through the corner. This stops light leaking through diagonal cracks in walls.

`floor_neighbor` lets wall-mounted lights (`on_wall: true`) sample LOS from a neighbouring floor tile, so a torch on a wall illuminates the room rather than itself.

## Edge cases and resolved decisions

- **Headless engine** — the engine does not depend on `bevy_light_2d` or any rendering crate. It only produces `LightMap`. The game uses the values to tint sprites; an ASCII frontend would tint glyphs. (CLAUDE.md previously referenced `bevy_light_2d` as a dependency — that has been corrected; the lighting is a custom Bresenham implementation.)
- **Why a resource, not entity queries** — the original design used entity queries; with deferred spawn/despawn semantics in Bevy, one frame's spawned light wouldn't appear in the next system's query. A resource sidesteps this: any system that adds a light sets `dirty` and the rebuild always sees the up-to-date list.
- **Why mark every viewshed dirty after rebuild** — visibility shading depends on light values. Forcing all viewsheds dirty on rebuild guarantees the same-frame visibility update (`Changed<Viewshed>` in downstream systems will fire).
- **Save/load** — `LightSources` and `LightMap` are not persisted. Lighting is derivable from current entity state; on load, candles re-add themselves on spawn and the dirty flag triggers a rebuild.

## Cross-links

- [TILE_PROMOTION.md](TILE_PROMOTION.md) — opacity changes via promotion automatically dirty the light map (handled in the engine apply systems).
- [DUNGEON.md](DUNGEON.md) — `Map`, `Viewshed`, and tile entities; the light pipeline reads `Map`, marks `Viewshed.dirty`.
- [ASCII_RENDERER.md](ASCII_RENDERER.md) — one of the consumers of `LightMap` (intensity + color tinting per cell).
- [FIRE.md](FIRE.md) — fire entities register their own light via `LightSources::add` and clean up on decay.
