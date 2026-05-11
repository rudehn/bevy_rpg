# Unified ASCII Tile Renderer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 6 independent ASCII tile color-writing systems with a single `render_tile_ascii` system that resolves glyph, foreground, and background per tile each frame using a priority cascade (fire > gas > water > base + lighting).

**Architecture:** New file `src/map/ascii_renderer.rs` contains the unified system and helper functions. Existing animation systems (`animate_fire_backgrounds`, `animate_gas_backgrounds`) are deleted. Water shimmer's ASCII branch is removed (sprite branch stays). ASCII color-writing code is removed from `update_tile_visibility`, `apply_tile_mutations`, and `apply_decoration_mutations`.

**Tech Stack:** Bevy 0.17 ECS, existing `TileEntityIndex`, `FireTiles`, `GasTiles`, `WaterTiles` resources.

**Spec:** [docs/design/ASCII_RENDERER.md](../design/ASCII_RENDERER.md)

---

### Task 1: Create `ascii_renderer.rs` with helper functions and tests

**Files:**
- Create: `src/map/ascii_renderer.rs`
- Modify: `src/map/mod.rs` (add module declaration)

- [ ] **Step 1: Create the module file with helper functions**

Create `src/map/ascii_renderer.rs` with the fire, gas, and lighting helper functions extracted from existing systems. These are pure functions, easy to test.

```rust
//! Unified ASCII tile renderer — the sole writer to tile ASCII components.
//!
//! Replaces: animate_fire_backgrounds, animate_gas_backgrounds,
//! animate_water_shimmer (ASCII branch), update_tile_visibility (ASCII path),
//! apply_tile_mutations glyph updates, apply_decoration_mutations glyph/bg updates.

use bevy::prelude::*;

/// Compute fire glyph foreground and background colors from sine-wave animation.
/// Returns `(fg_color, bg_color)`. Fire is self-luminous — no lighting tint needed.
pub fn compute_fire_colors(t: f32, phase: f32) -> (Color, Color) {
    let wave1 = (t * 1.2 + phase * 6.28).sin() * 0.5 + 0.5;
    let wave2 = (t * 0.7 + phase * 3.14).sin() * 0.5 + 0.5;
    let blend = wave1 * 0.6 + wave2 * 0.4;

    let fg = Color::srgb(
        0.85 + blend * 0.15,
        0.35 + blend * 0.25,
        0.05 + blend * 0.05,
    );
    let bg = Color::srgb(
        0.35 + blend * 0.25,
        0.08 + blend * 0.12,
        0.02,
    );
    (fg, bg)
}

/// Compute gas background color from type, concentration, and sine-wave animation.
/// Gas backgrounds are self-luminous — no lighting tint needed.
pub fn compute_gas_bg(
    gas_type: crate::game::gas::GasType,
    concentration: u8,
    t: f32,
    phase: f32,
) -> Color {
    let wave = (t * 0.6 + phase * 6.28).sin() * 0.5 + 0.5;
    let intensity = match concentration {
        3 => 0.6 + wave * 0.15,
        2 => 0.35 + wave * 0.15,
        _ => 0.15 + wave * 0.10,
    };
    let [br, bg, bb] = gas_type.ascii_bg_color();
    Color::srgb(br * intensity, bg * intensity, bb * intensity)
}

/// Add light tint to a base color. `light_amount` is 0.0 (ambient) to 1.0 (bright).
pub fn apply_light_to_color(base: Color, light_amount: f32, tint: [f32; 3]) -> Color {
    let srgba = base.to_srgba();
    let r = (srgba.red + light_amount * tint[0] * 0.2).min(1.0);
    let g = (srgba.green + light_amount * tint[1] * 0.2).min(1.0);
    let b = (srgba.blue + light_amount * tint[2] * 0.2).min(1.0);
    Color::srgba(r, g, b, srgba.alpha)
}

/// Dim a color by a multiplicative factor (0.0 = black, 1.0 = unchanged).
pub fn dim_color(base: Color, factor: f32) -> Color {
    let srgba = base.to_srgba();
    Color::srgba(
        srgba.red * factor,
        srgba.green * factor,
        srgba.blue * factor,
        srgba.alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_colors_in_range() {
        for i in 0..100 {
            let t = i as f32 * 0.1;
            let phase = (i as f32 * 0.37).fract();
            let (fg, bg) = compute_fire_colors(t, phase);
            let fg_s = fg.to_srgba();
            let bg_s = bg.to_srgba();
            assert!(fg_s.red >= 0.0 && fg_s.red <= 1.0);
            assert!(fg_s.green >= 0.0 && fg_s.green <= 1.0);
            assert!(fg_s.blue >= 0.0 && fg_s.blue <= 1.0);
            assert!(bg_s.red >= 0.0 && bg_s.red <= 1.0);
            assert!(bg_s.green >= 0.0 && bg_s.green <= 1.0);
            assert!(bg_s.blue >= 0.0 && bg_s.blue <= 1.0);
        }
    }

    #[test]
    fn fire_colors_vary_over_time() {
        let (fg1, _) = compute_fire_colors(0.0, 0.5);
        let (fg2, _) = compute_fire_colors(2.0, 0.5);
        let s1 = fg1.to_srgba();
        let s2 = fg2.to_srgba();
        assert!(
            (s1.red - s2.red).abs() > 0.001
                || (s1.green - s2.green).abs() > 0.001
                || (s1.blue - s2.blue).abs() > 0.001,
        );
    }

    #[test]
    fn gas_bg_scales_with_concentration() {
        let low = compute_gas_bg(crate::game::gas::GasType::Poison, 1, 0.0, 0.0);
        let high = compute_gas_bg(crate::game::gas::GasType::Poison, 3, 0.0, 0.0);
        let low_s = low.to_srgba();
        let high_s = high.to_srgba();
        assert!(high_s.green > low_s.green, "higher concentration should be brighter");
    }

    #[test]
    fn gas_bg_in_range() {
        for conc in 1..=3 {
            for gas in [crate::game::gas::GasType::Poison, crate::game::gas::GasType::Steam] {
                let c = compute_gas_bg(gas, conc, 3.14, 0.7);
                let s = c.to_srgba();
                assert!(s.red >= 0.0 && s.red <= 1.0);
                assert!(s.green >= 0.0 && s.green <= 1.0);
                assert!(s.blue >= 0.0 && s.blue <= 1.0);
            }
        }
    }

    #[test]
    fn dim_color_reduces_brightness() {
        let base = Color::srgb(0.8, 0.6, 0.4);
        let dimmed = dim_color(base, 0.5);
        let s = dimmed.to_srgba();
        assert!((s.red - 0.4).abs() < 0.01);
        assert!((s.green - 0.3).abs() < 0.01);
        assert!((s.blue - 0.2).abs() < 0.01);
    }

    #[test]
    fn apply_light_brightens() {
        let base = Color::srgb(0.3, 0.3, 0.3);
        let lit = apply_light_to_color(base, 1.0, [1.0, 1.0, 1.0]);
        let s = lit.to_srgba();
        assert!(s.red > 0.3);
        assert!(s.green > 0.3);
        assert!(s.blue > 0.3);
    }
}
```

- [ ] **Step 2: Add module declaration to `src/map/mod.rs`**

Add `pub mod ascii_renderer;` to `src/map/mod.rs` alongside the other module declarations.

- [ ] **Step 3: Run tests to verify helpers**

Run: `cargo test --bin bevy_rpg map::ascii_renderer::tests -v`
Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/map/ascii_renderer.rs src/map/mod.rs
git commit -m "feat: add ascii_renderer module with helper functions and tests"
```

---

### Task 2: Implement the `render_tile_ascii` system

**Files:**
- Modify: `src/map/ascii_renderer.rs`

- [ ] **Step 1: Add the unified render system**

Add this system to `src/map/ascii_renderer.rs`, below the helper functions and above the tests module:

```rust
use bracket_lib::prelude::Point;

use crate::components::{Position, Viewshed};
use crate::game::ascii_mode::{
    AsciiBackground, AsciiGlyph, AsciiGlyphColor, GraphicsMode, LiquidOverlay,
};
use crate::game::fire::FireTiles;
use crate::game::gas::GasTiles;
use crate::game::water::{WaterTiles, compute_shimmer_color};
use crate::map::light::LightMap;
use crate::map::map::Map;
use crate::map::tile::{
    LiquidType, TileEntityIndex, TileExplored, TileManifest, TileManifestHandle, TileMarker,
    TileVisibility, resolve_tile_bg, resolve_tile_display,
};
use crate::player::Player;

/// Ambient light floor — must match the constant in map.rs.
const AMBIENT: f32 = 0.55;

/// Unified ASCII tile renderer. The sole writer to tile ASCII components
/// (TextColor, Text2d, AsciiGlyphColor, AsciiBackground).
///
/// Runs every frame in ASCII mode. Resolves glyph, fg, and bg per tile
/// using a priority cascade: fire > gas > water > base + lighting.
pub fn render_tile_ascii(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    light_map: Res<LightMap>,
    time: Res<Time>,
    fire_tiles: Res<FireTiles>,
    gas_tiles: Res<GasTiles>,
    water_tiles: Res<WaterTiles>,
    omniscient: Res<crate::game::systems::Omniscient>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_query: Query<
        (
            &Position,
            &TileVisibility,
            &TileExplored,
            Option<&Children>,
        ),
        With<TileMarker>,
    >,
    mut glyph_query: Query<
        (&mut Text2d, &mut TextColor, &mut AsciiGlyphColor),
        With<AsciiGlyph>,
    >,
    mut bg_query: Query<(&mut Sprite, &mut AsciiBackground), Without<LiquidOverlay>>,
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };
    let Some(manifest) = tile_manifests.get(&tile_manifest_handle.0) else {
        return;
    };

    let fov_tiles = &player_viewshed.visible_tiles;
    let omni = omniscient.0;
    let t = time.elapsed_secs();

    for (tile_pos, tile_vis, tile_explored, children) in tile_query.iter() {
        let Some(children) = children else { continue };
        let idx = map.xy_idx(tile_pos.x, tile_pos.y);
        if idx >= map.tiles.len() {
            continue;
        }

        let tile = map.tiles[idx];
        let in_fov = omni || fov_tiles.contains(&Point::new(tile_pos.x, tile_pos.y));

        // --- Determine glyph character, fg color, bg color ---
        let (glyph_char, fg_color, bg_color): (String, Color, Color);

        if *tile_vis == TileVisibility::Visible && in_fov {
            // Visible tile — apply effect overlay cascade
            let phase = (tile_pos.x as f32 * 1.7 + tile_pos.y as f32 * 2.3).fract();

            if fire_tiles.0.contains(&(tile_pos.x, tile_pos.y)) {
                // FIRE — self-luminous, no lighting
                let (fg, bg) = compute_fire_colors(t, phase);
                glyph_char = "^".to_string();
                fg_color = fg;
                bg_color = bg;
            } else if let Some(gas_data) = gas_tiles.0.get(&(tile_pos.x, tile_pos.y)) {
                // GAS — base glyph with gas-tinted background
                let (ch, base_fg, _) = resolve_tile_display(tile, manifest);
                let light = light_map.values.get(idx).copied().unwrap_or(0.0).max(AMBIENT);
                let light_color = light_map.colors.get(idx).copied().unwrap_or([1.0, 1.0, 1.0]);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);

                glyph_char = ch;
                fg_color = apply_light_to_color(base_fg, light_amount, light_color);
                bg_color = compute_gas_bg(
                    gas_data.gas_type,
                    gas_data.concentration,
                    t,
                    phase,
                );
            } else if water_tiles.0.contains_key(&(tile_pos.x, tile_pos.y)) {
                // WATER — shimmer colors
                let liquid = water_tiles.0.get(&(tile_pos.x, tile_pos.y)).copied()
                    .unwrap_or(LiquidType::ShallowWater);
                let light = light_map.values.get(idx).copied().unwrap_or(0.0).max(AMBIENT);

                let variation = if liquid == LiquidType::Water { 0.10_f32 } else { 0.05 };
                let r_wave = (t * 2.0 + phase * std::f32::consts::TAU).sin();
                let g_wave = (t * 1.7 + phase * std::f32::consts::TAU + 1.0).sin();
                let b_wave = (t * 1.3 + phase * std::f32::consts::TAU + 2.0).sin();

                let (fg_base, bg_base_arr) = match liquid {
                    LiquidType::Water => ([0.50_f32, 0.50, 1.0], [0.37_f32, 0.37, 0.79]),
                    _ => ([0.63_f32, 0.75, 1.0], [0.44_f32, 0.63, 0.93]),
                };

                let (ch, _, _) = resolve_tile_display(tile, manifest);
                glyph_char = ch;
                fg_color = Color::srgb(
                    (fg_base[0] * light * (1.0 + r_wave * variation)).clamp(0.0, 1.0),
                    (fg_base[1] * light * (1.0 + g_wave * variation)).clamp(0.0, 1.0),
                    (fg_base[2] * light * (1.0 + b_wave * variation)).clamp(0.0, 1.0),
                );
                bg_color = Color::srgb(
                    (bg_base_arr[0] * light * (1.0 + r_wave * variation)).clamp(0.0, 1.0),
                    (bg_base_arr[1] * light * (1.0 + g_wave * variation)).clamp(0.0, 1.0),
                    (bg_base_arr[2] * light * (1.0 + b_wave * variation)).clamp(0.0, 1.0),
                );
            } else {
                // BASE — normal tile with lighting
                let (ch, base_fg, _) = resolve_tile_display(tile, manifest);
                let base_bg = resolve_tile_bg(tile, manifest);
                let light = light_map.values.get(idx).copied().unwrap_or(0.0).max(AMBIENT);
                let light_color = light_map.colors.get(idx).copied().unwrap_or([1.0, 1.0, 1.0]);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);

                glyph_char = ch;
                fg_color = apply_light_to_color(base_fg, light_amount, light_color);
                bg_color = apply_light_to_color(base_bg, light_amount, light_color);
            }
        } else if *tile_explored == TileExplored::Explored {
            // Explored but not visible — dimmed base colors, no effects
            let (ch, base_fg, _) = resolve_tile_display(tile, manifest);
            let base_bg = resolve_tile_bg(tile, manifest);
            glyph_char = ch;
            fg_color = dim_color(base_fg, 0.45);
            bg_color = dim_color(base_bg, 0.35);
        } else {
            // Unexplored — skip (Visibility::Hidden handled by update_tile_visibility)
            continue;
        }

        // --- Write to components once ---
        for child in children.iter() {
            if let Ok((mut text, mut text_color, mut glyph_base)) = glyph_query.get_mut(child) {
                **text = glyph_char.clone();
                text_color.0 = fg_color;
                glyph_base.0 = fg_color;
            }
            if let Ok((mut sprite, mut bg)) = bg_query.get_mut(child) {
                sprite.color = bg_color;
                bg.base_color = bg_color;
            }
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/map/ascii_renderer.rs
git commit -m "feat: implement render_tile_ascii unified system"
```

---

### Task 3: Register the new system and remove old animation systems

**Files:**
- Modify: `src/game/mod.rs` (system registration)
- Modify: `src/map/map.rs` (add `render_tile_ascii` registration)

- [ ] **Step 1: Register `render_tile_ascii` in `src/map/map.rs`**

In the `MapPlugin::build` method (around line 48), add the new system alongside `update_tile_visibility`:

```rust
.add_systems(
    Update,
    (
        init_explored_tiles_system,
        update_tile_visibility
            .after(crate::map::light::rebuild_light_map_system)
            .run_if(|init: Res<NeedsExploredInit>| !init.0)
            .after(init_explored_tiles_system),
        crate::map::ascii_renderer::render_tile_ascii
            .run_if(|mode: Res<crate::game::ascii_mode::GraphicsMode>|
                *mode == crate::game::ascii_mode::GraphicsMode::Ascii),
        handle_reveal_map_system.run_if(on_message::<RevealMapMessage>),
    ).run_if(in_state(AppState::InGame)),
);
```

- [ ] **Step 2: Remove old animation systems from `src/game/mod.rs`**

In `src/game/mod.rs`, change the animation systems block (around line 183) to remove `fire::animate_fire_backgrounds` and `gas::animate_gas_backgrounds`. Keep only the water sprite-mode shimmer:

```rust
.add_systems(
    Update,
    (
        water::animate_water_shimmer,
    )
        .run_if(in_state(AppState::InGame)),
);
```

Also remove the timer resource registrations:

Remove these lines:
```rust
.init_resource::<fire::FireAnimationTimer>()
.init_resource::<gas::GasAnimationTimer>()
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Warnings about unused `FireAnimationTimer` and `GasAnimationTimer` types (we'll remove those in Task 5), but no errors.

- [ ] **Step 4: Commit**

```bash
git add src/game/mod.rs src/map/map.rs
git commit -m "feat: register render_tile_ascii, remove old animation system registrations"
```

---

### Task 4: Remove ASCII color-writing from `update_tile_visibility`

**Files:**
- Modify: `src/map/map.rs`

- [ ] **Step 1: Remove `ascii_child_updates` vec, all push sites, and the apply loop**

In `update_tile_visibility` (line 137+):

1. Remove the `ascii_child_updates` vector declaration (line ~182).
2. Remove the ASCII branch in the visible-tile section that pushes to `ascii_child_updates` (lines ~218-244 — the `if is_ascii { ... }` block that includes the water skip check). Replace with just `sprite.color = Color::NONE;` when `is_ascii`.
3. Remove the ASCII branch in the explored-but-hidden section that pushes to `ascii_child_updates` (lines ~273-279). Replace with just `sprite.color = Color::NONE;`.
4. Remove the apply loop for `ascii_child_updates` (lines ~302-323 — the `bg_q` section and the `ascii_glyph_query` loop).
5. Remove the `ascii_glyph_query` parameter from the function signature.
6. Remove the `apply_light_to_color` and `dim_color` private functions (lines ~123-135) — they've been moved to `ascii_renderer.rs`.

The ASCII branches should become:
```rust
if is_ascii {
    sprite.color = Color::NONE;
    // ASCII child colors handled by render_tile_ascii
} else {
    // existing sprite-mode code unchanged
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles. May have warnings about unused ParamSet entry if `p1()` (bg query) is no longer used — that's fine, we'll clean up in Task 5.

- [ ] **Step 3: Commit**

```bash
git add src/map/map.rs
git commit -m "refactor: remove ASCII color-writing from update_tile_visibility"
```

---

### Task 5: Remove ASCII glyph/bg writes from tile mutation systems

**Files:**
- Modify: `src/map/tile.rs`

- [ ] **Step 1: Remove glyph update from `apply_tile_mutations`**

In `apply_tile_mutations` (line ~654), remove the ASCII glyph update block (lines ~723-735) that writes to `Text2d` children. Remove the `glyph_query` parameter from the function signature if it's only used for this.

- [ ] **Step 2: Remove glyph/bg update from `apply_decoration_mutations`**

In `apply_decoration_mutations` (line ~767), remove the calls to `update_tile_glyph` and `update_tile_bg` (lines ~814-818). Remove the glyph/bg query parameters from the function signature if they're only used for this.

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles. Warnings about now-unused `update_tile_glyph` and `update_tile_bg` functions — we'll assess whether to keep them as utilities or remove them.

- [ ] **Step 4: Run all tests**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass (including the new ascii_renderer tests from Task 1).

- [ ] **Step 5: Commit**

```bash
git add src/map/tile.rs
git commit -m "refactor: remove ASCII writes from tile/decoration mutation systems"
```

---

### Task 6: Delete old animation functions and clean up dead code

**Files:**
- Modify: `src/game/fire.rs`
- Modify: `src/game/gas.rs`
- Modify: `src/game/water.rs`

- [ ] **Step 1: Delete `animate_fire_backgrounds` and `FireAnimationTimer` from `src/game/fire.rs`**

Remove the `FireAnimationTimer` struct + `Default` impl (lines ~42-49) and the entire `animate_fire_backgrounds` function (lines ~284-371).

- [ ] **Step 2: Delete `animate_gas_backgrounds` and `GasAnimationTimer` from `src/game/gas.rs`**

Remove the `GasAnimationTimer` struct + `Default` impl (lines ~151-157) and the entire `animate_gas_backgrounds` function (lines ~452-501).

- [ ] **Step 3: Remove ASCII branch from `animate_water_shimmer` in `src/game/water.rs`**

In `animate_water_shimmer`, remove the `if is_ascii { ... }` branch (the water ASCII color code). Keep only the `else` (sprite mode) branch. Remove the `glyph_query`, `bg_query`, and `mode` parameters since they're only used by the ASCII branch. Also remove the `fire_tiles` and `gas_tiles` parameters since the fire/gas skip check was only needed for the ASCII flicker bug. The sprite branch doesn't need those checks.

- [ ] **Step 4: Remove unused imports and query parameters**

Clean up any unused imports in the modified files. If `update_tile_glyph` and `update_tile_bg` in `tile.rs` are now unused, remove them.

- [ ] **Step 5: Verify clean compilation**

Run: `cargo check`
Expected: No errors, minimal warnings (only pre-existing ones).

- [ ] **Step 6: Run all tests**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/game/fire.rs src/game/gas.rs src/game/water.rs src/map/tile.rs
git commit -m "refactor: delete old animation systems and dead ASCII code"
```

---

### Task 7: Final verification and doc update

**Files:**
- Modify: `docs/design/ASCII_RENDERER.md` (mark as implemented)
- Modify: `CLAUDE.md` (update project structure if needed)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No new warnings from our changes.

- [ ] **Step 3: Manual verification**

Run the game (`cargo run`) and verify:
- ASCII mode (F5): tiles render correctly with lighting
- Fire appears as `^` with orange flicker animation
- Gas clouds show colored backgrounds
- Water tiles shimmer with blue colors
- Fire + water → steam displays correctly (no flicker)
- Explored-but-not-visible tiles are dimmed gray
- Toggle sprite/ASCII mode → no visual glitches
- Descend stairs → no stale fire/gas on new floor

- [ ] **Step 4: Update design doc**

Add "Status: Implemented" to the top of `docs/design/ASCII_RENDERER.md`.

- [ ] **Step 5: Commit**

```bash
git add docs/design/ASCII_RENDERER.md
git commit -m "docs: mark ASCII renderer refactor as implemented"
```
