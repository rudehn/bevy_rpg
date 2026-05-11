# Unified ASCII Tile Renderer

**Status: Implemented**

## Problem

Tile ASCII rendering is spread across 6 independent systems, each with ad-hoc skip
logic to avoid overwriting the others. This causes flicker (fire→embers, water+gas),
ordering bugs, and makes adding new visual effects fragile.

**Current writers**: `update_tile_visibility` (lighting), `animate_fire_backgrounds`,
`animate_gas_backgrounds`, `animate_water_shimmer`, `apply_tile_mutations`,
`apply_decoration_mutations`.

## Solution

A single system — `render_tile_ascii` — is the **sole writer** to tile ASCII
components (`TextColor`, `Text2d`, `AsciiGlyphColor`, `Sprite.color` on backgrounds,
`AsciiBackground.base_color`). Runs every frame in ASCII mode. All other systems
stop writing to these components.

### File: `src/map/ascii_renderer.rs`

### Priority Cascade

For each grid cell, the system resolves what to display:

```
1. Build entity spatial lookup: Player > Monster > Item > Prop
2. Visibility gate:
   - Unexplored → skip
   - Explored, not in FOV → dim base glyph, skip effects
   - In FOV → continue to cascade
3. Priority cascade (highest wins):
   - Entity at position → entity glyph + color, bg from tile effect cascade
   - Fire (FireTiles) → glyph '^', sine-wave orange/amber fg, ember bg
   - Gas (GasTiles) → keep glyph, blend gas bg over lit base
   - Water (WaterTiles) → keep glyph '~', shimmer blue fg/bg
   - Base → terrain/decoration glyph with lighting
4. Apply lighting (LightMap values + colors) — fire is self-luminous, skips this
5. Write to components once per tile
```

### Inputs (read-only)

- `Map` — tile data (terrain, liquid, decoration)
- `TileManifest` — glyph chars, base colors
- `LightMap` — per-tile light level and color
- `FireTiles` — fire positions
- `GasTiles` — gas positions, type, concentration
- `WaterTiles` — water positions, liquid type
- `TileEntityIndex` — position → entity lookup
- `Viewshed` (player) — FOV tiles
- `Time` — elapsed seconds for sine wave animation

### Visual Rules

**Fire** (in `FireTiles`):
- Glyph: `'^'`
- FG: `(0.85 + blend*0.15, 0.35 + blend*0.25, 0.05 + blend*0.05)` — yellow-orange
- BG: `(0.35 + blend*0.25, 0.08 + blend*0.12, 0.02)` — ember red
- `blend` from dual sine waves: `wave1 * 0.6 + wave2 * 0.4`
- Self-luminous — no lighting tint applied

**Gas** (in `GasTiles`, not fire):
- Glyph: base tile glyph unchanged
- FG: base fg with lighting applied
- BG: `gas_type.ascii_bg_color()` modulated by concentration alpha + sine wave
- BG is self-luminous — no lighting on bg

**Water** (in `WaterTiles`, not fire/gas):
- Glyph: base (`~` from manifest)
- FG: blue shimmer — per-channel sine waves at different frequencies
  - Deep: base `(0.50, 0.50, 1.0)`, variation ±10%
  - Shallow: base `(0.63, 0.75, 1.0)`, variation ±5%
- BG: darker version of fg shimmer
- Lighting baked into shimmer (reads LightMap directly)

**Base** (no effects):
- Glyph + colors from `resolve_tile_display()` / `resolve_tile_bg()`
- Lighting: `apply_light_to_color(base, light_amount, light_color)`

**Explored-but-not-visible** (any type):
- Glyph: base from manifest (no effect overlay)
- FG/BG: dimmed to ~45% brightness
- No animation

### Systems Removed

| System | File | What happens |
|--------|------|-------------|
| `animate_fire_backgrounds` | fire.rs | Deleted entirely |
| `animate_gas_backgrounds` | gas.rs | Deleted entirely |
| `animate_water_shimmer` ASCII branch | water.rs | Removed; sprite branch stays |
| `update_tile_visibility` ASCII child loop | map.rs | `ascii_child_updates` vec + apply section removed |
| `apply_decoration_mutations` glyph/bg writes | tile.rs | `update_tile_glyph`/`update_tile_bg` calls removed |
| `apply_tile_mutations` glyph writes | tile.rs | Glyph update code removed |

### Systems Unchanged

- `spawn_tile_entity` — still creates initial ASCII children
- `apply_graphics_mode_swap` — still toggles ASCII child visibility on F5
- `update_tile_visibility` — still manages `TileVisibility`, `TileExplored`, sprite-mode colors, liquid overlay tinting
- `animate_water_shimmer` sprite branch — still runs for liquid overlay sprite tinting
- Tile/decoration mutation systems — still update `Map` resource, FOV dirty flags

### Registration

```rust
// In map/mod.rs or game/mod.rs
render_tile_ascii
    .run_if(in_state(AppState::InGame))
    .run_if(|mode: Res<GraphicsMode>| *mode == GraphicsMode::Ascii)
```

No ordering constraints — it is the only ASCII tile color writer.

### Helper Functions

Extracted into `ascii_renderer.rs` as pure functions for testability:

- `compute_fire_colors(t: f32, phase: f32) -> (Color, Color)` — fg + bg
- `compute_gas_bg(gas_type, concentration, t, phase) -> Color`
- `compute_water_ascii_colors` reuses existing `compute_shimmer_color` from water.rs

### Removed Resources

- `FireAnimationTimer` — no longer needed (runs every frame)
- `GasAnimationTimer` — no longer needed

### Performance

~500 visible tiles × (1 map lookup + 3 hash lookups + a few sine calls + 2 component writes) per frame. Measured cost of water shimmer (200 tiles, every frame) was negligible. Full 500-tile pass should be under 0.5ms.
