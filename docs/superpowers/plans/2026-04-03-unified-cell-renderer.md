# Unified Cell Renderer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `render_tile_ascii` to render one glyph per cell including entities (player > monster > item > prop > tile effects), fixing items invisible on water and eliminating 4 separate entity visibility systems.

**Architecture:** Build a per-cell `HashMap<(i32, i32), CellEntity>` from entity queries each frame, then merge it into the existing tile cascade. Entity glyphs override tile glyphs at matching positions. Entity visibility systems (`update_monster_visibility`, `update_item_visibility`, `update_prop_visibility`, `update_status_visuals`) are deleted — their logic folds into the renderer. In sprite mode, entity visibility is handled by a new lightweight `update_entity_sprite_visibility` system.

**Tech Stack:** Bevy 0.17 ECS, existing `TileEntityIndex`, `FireTiles`, `GasTiles`, `WaterTiles` resources.

---

### Task 1: Add entity queries and spatial lookup to render_tile_ascii

**Files:**
- Modify: `src/map/ascii_renderer.rs`

This task adds the entity queries, builds a spatial lookup, and integrates entities into the priority cascade. The entity priority is: Player > Monster > Item > Prop > (existing tile effects).

- [ ] **Step 1: Add entity imports and CellEntity struct**

At the top of `src/map/ascii_renderer.rs`, add these imports alongside the existing ones:

```rust
use std::collections::HashMap;
use crate::components::{Position, Monster, Item, Prop, Submerged, InInventory};
use crate::game::magic::StatusEffects;
```

Add this struct above the `render_tile_ascii` function:

```rust
/// What entity (if any) occupies a cell for ASCII rendering.
struct CellEntity {
    glyph: String,
    color: Color,
}
```

- [ ] **Step 2: Add entity queries to the system signature**

Add these parameters to `render_tile_ascii`:

```rust
    // Entity queries for the spatial lookup
    player_pos_query: Query<(&Position, &Children), With<Player>>,
    monster_query: Query<(&Position, Option<&StatusEffects>, &Children, Has<Submerged>), With<Monster>>,
    item_query: Query<(&Position, &Children), (With<Item>, Without<InInventory>)>,
    prop_query: Query<(&Position, &Children), With<Prop>>,
    entity_glyph_query: Query<(&Text2d, &AsciiGlyphColor), With<AsciiGlyph>>,
```

- [ ] **Step 3: Build the entity spatial lookup**

After the `let fov_changed = ...` block and before the positions loop, add:

```rust
    // Build per-cell entity lookup. Priority: Player > Monster > Item > Prop.
    // Only include entities in FOV (or omniscient).
    let mut cell_entities: HashMap<(i32, i32), CellEntity> = HashMap::new();

    // Props (lowest priority — inserted first, overwritten by higher)
    for (pos, children, ) in prop_query.iter() {
        let pt = bracket_lib::prelude::Point::new(pos.x, pos.y);
        if !(omni || fov_tiles.contains(&pt)) { continue; }
        if let Some(ce) = entity_glyph_from_children(children, &entity_glyph_query, None) {
            cell_entities.insert((pos.x, pos.y), ce);
        }
    }

    // Items (above props)
    for (pos, children) in item_query.iter() {
        let pt = bracket_lib::prelude::Point::new(pos.x, pos.y);
        if !(omni || fov_tiles.contains(&pt)) { continue; }
        if let Some(ce) = entity_glyph_from_children(children, &entity_glyph_query, None) {
            cell_entities.insert((pos.x, pos.y), ce);
        }
    }

    // Monsters (above items) — stunned monsters get yellow tint, submerged are hidden
    for (pos, effects, children, is_submerged) in monster_query.iter() {
        if is_submerged { continue; }
        let pt = bracket_lib::prelude::Point::new(pos.x, pos.y);
        if !(omni || fov_tiles.contains(&pt)) { continue; }
        let status_tint = if effects.map(|e| e.is_stunned()).unwrap_or(false) {
            Some(Color::srgba(1.0, 1.0, 0.3, 1.0))
        } else {
            None
        };
        if let Some(ce) = entity_glyph_from_children(children, &entity_glyph_query, status_tint) {
            cell_entities.insert((pos.x, pos.y), ce);
        }
    }

    // Player (highest priority)
    if let Ok((pos, children)) = player_pos_query.single() {
        if let Some(ce) = entity_glyph_from_children(children, &entity_glyph_query, None) {
            cell_entities.insert((pos.x, pos.y), ce);
        }
    }
```

- [ ] **Step 4: Add the entity_glyph_from_children helper**

Add this function above `render_tile_ascii`:

```rust
/// Extract ASCII glyph data from an entity's children.
/// If `tint` is Some, use that color instead of the base glyph color (for status effects).
fn entity_glyph_from_children(
    children: &Children,
    glyph_query: &Query<(&Text2d, &AsciiGlyphColor), With<AsciiGlyph>>,
    tint: Option<Color>,
) -> Option<CellEntity> {
    for child in children.iter() {
        if let Ok((text, base_color)) = glyph_query.get(child) {
            let ch = text.as_str().to_string();
            if ch.is_empty() { continue; }
            return Some(CellEntity {
                glyph: ch,
                color: tint.unwrap_or(base_color.0),
            });
        }
    }
    None
}
```

- [ ] **Step 5: Integrate entity lookup into the cascade**

In the visible-tile cascade (after `if in_fov && *tile_vis == TileVisibility::Visible`), add entity check as the FIRST branch, before the fire check:

```rust
        if in_fov && *tile_vis == TileVisibility::Visible {
            // --- Priority cascade for visible tiles ---

            // 0. Entity at this position (highest priority)
            if let Some(entity_cell) = cell_entities.get(&(x, y)) {
                glyph_char = entity_cell.glyph.clone();
                fg_color = entity_cell.color;
                // Background: use the tile's normal bg (with lighting) so entities
                // appear "on top of" the tile terrain
                let base_bg = resolve_tile_bg(tile, manifest);
                let (light, light_color) = get_light(idx, &light_map);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                bg_color = apply_light_to_color(base_bg, light_amount, light_color);
            } else if fire_tiles.0.contains(&(x, y)) {
                // ... existing fire cascade
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check`
Expected: Compiles. Entity visibility systems still exist but the renderer now also draws entities.

- [ ] **Step 7: Run tests**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/map/ascii_renderer.rs
git commit -m "feat: render entities in unified ASCII cell renderer"
```

---

### Task 2: Remove entity ASCII visibility systems

**Files:**
- Modify: `src/game/systems.rs` (delete 4 functions)
- Modify: `src/game/mod.rs` (remove registrations + imports)

Now that `render_tile_ascii` handles entity display in ASCII mode, the entity visibility systems need to be split: their sprite-mode logic stays (simplified), their ASCII-mode logic is deleted.

- [ ] **Step 1: Replace entity visibility systems with sprite-only versions**

In `src/game/systems.rs`, replace `update_monster_visibility`, `update_item_visibility`, `update_prop_visibility`, and `update_status_visuals` with sprite-only versions that skip work in ASCII mode:

Replace `update_monster_visibility` with:
```rust
pub fn update_monster_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    omniscient: Res<Omniscient>,
    mut monster_query: Query<(&Position, &mut Visibility, &mut Sprite, Has<Submerged>), With<Monster>>,
) {
    // In ASCII mode, render_tile_ascii handles all visibility.
    if *mode == crate::game::ascii_mode::GraphicsMode::Ascii { return; }
    let Ok(player_viewshed) = player_query.single() else { return; };

    for (monster_pos, mut monster_vis, mut sprite, is_submerged) in monster_query.iter_mut() {
        if is_submerged {
            *monster_vis = Visibility::Hidden;
            continue;
        }
        let monster_point = Point::new(monster_pos.x, monster_pos.y);
        let is_visible = omniscient.0 || player_viewshed.visible_tiles.contains(&monster_point);
        if is_visible {
            *monster_vis = Visibility::Visible;
            sprite.color = Color::WHITE;
        } else {
            *monster_vis = Visibility::Hidden;
        }
    }
}
```

Replace `update_item_visibility` with:
```rust
pub fn update_item_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    omniscient: Res<Omniscient>,
    mut item_query: Query<(&Position, &mut Visibility, &mut Sprite), (With<Item>, Without<InInventory>)>,
) {
    if *mode == crate::game::ascii_mode::GraphicsMode::Ascii { return; }
    let Ok(viewshed) = player_query.single() else { return; };

    for (pos, mut vis, mut sprite) in item_query.iter_mut() {
        if !map.in_bounds(Point::new(pos.x, pos.y)) { continue; }
        let idx = map.xy_idx(pos.x, pos.y);
        let pt = Point::new(pos.x, pos.y);

        if omniscient.0 || viewshed.visible_tiles.contains(&pt) {
            *vis = Visibility::Visible;
            sprite.color = Color::WHITE;
        } else if map.explored_tiles[idx] {
            *vis = Visibility::Visible;
            sprite.color = Color::srgb(0.5, 0.5, 0.5);
        } else {
            *vis = Visibility::Hidden;
        }
    }
}
```

Replace `update_prop_visibility` with:
```rust
pub fn update_prop_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    omniscient: Res<Omniscient>,
    mut prop_query: Query<(&Position, &mut Visibility, &mut Sprite), With<Prop>>,
) {
    if *mode == crate::game::ascii_mode::GraphicsMode::Ascii { return; }
    let Ok(viewshed) = player_query.single() else { return; };

    for (pos, mut vis, mut sprite) in prop_query.iter_mut() {
        if !map.in_bounds(Point::new(pos.x, pos.y)) { continue; }
        let idx = map.xy_idx(pos.x, pos.y);
        let pt = Point::new(pos.x, pos.y);

        if omniscient.0 || viewshed.visible_tiles.contains(&pt) {
            *vis = Visibility::Visible;
            sprite.color = Color::WHITE;
        } else if map.explored_tiles[idx] {
            *vis = Visibility::Visible;
            sprite.color = Color::srgb(0.5, 0.5, 0.5);
        } else {
            *vis = Visibility::Hidden;
        }
    }
}
```

Replace `update_status_visuals` with:
```rust
pub fn update_status_visuals(
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    mut query: Query<(Option<&StatusEffects>, &mut Sprite), With<Monster>>,
) {
    // In ASCII mode, render_tile_ascii handles status tinting.
    if *mode == crate::game::ascii_mode::GraphicsMode::Ascii { return; }

    for (effects, mut sprite) in &mut query {
        let tint = if effects.map(|e| e.is_stunned()).unwrap_or(false) {
            Color::srgba(1.0, 1.0, 0.3, 1.0)
        } else {
            Color::WHITE
        };
        sprite.color = tint;
    }
}
```

- [ ] **Step 2: Clean up unused imports in systems.rs**

Remove any now-unused imports (e.g., `AsciiGlyph` if no longer referenced, `Children` query usage, etc.).

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 4: Run tests**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/game/systems.rs src/game/mod.rs
git commit -m "refactor: simplify entity visibility systems to sprite-only"
```

---

### Task 3: Handle entity glyphs in ASCII mode toggle

**Files:**
- Modify: `src/game/ascii_mode.rs`

When toggling to ASCII mode, entity sprite visibility needs to be managed. Since `render_tile_ascii` now owns entity display in ASCII mode, entity sprites should be hidden and their AsciiGlyph children should also be hidden (the tile renderer writes the entity glyph directly to the tile's ASCII children, not the entity's own glyph child).

- [ ] **Step 1: Hide entity AsciiGlyph children in ASCII mode**

In `apply_graphics_mode_swap` in `src/game/ascii_mode.rs`, find where entity sprites are handled. Ensure that in ASCII mode:
- Entity parent sprites have `color = Color::NONE` (already the case)
- Entity `AsciiGlyph` children are `Visibility::Hidden` (the tile renderer draws entity glyphs on tile children, so entity glyph children must not also render)

If there's an `init_new_ascii_glyphs` system that sets entity glyph children to `Visibility::Inherited`, modify it to set them to `Visibility::Hidden` in ASCII mode for entities (but NOT for tile children, which are still needed).

Actually, the simpler approach: in `render_tile_ascii`, entity glyph data is READ from entity children (via `entity_glyph_query`) but WRITTEN to tile children. The entity's own AsciiGlyph child should be hidden. Add a system that hides all entity ASCII glyphs in ASCII mode.

Add to `ascii_mode.rs`, in the `apply_graphics_mode_swap` function, after the existing entity handling:

```rust
// In ASCII mode, hide entity AsciiGlyph children — render_tile_ascii
// draws entity glyphs on tile children instead.
if is_ascii {
    for (_, mut vis) in entity_glyph_vis_query.iter_mut() {
        *vis = Visibility::Hidden;
    }
}
```

This needs a query parameter for entity glyphs (on non-tile entities). The key filter: `With<AsciiGlyph>, Without<TileMarker>` on the parent, or just hide ALL entity glyph children and let the tile glyph children be managed by `render_tile_ascii`.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 3: Run tests**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/game/ascii_mode.rs
git commit -m "fix: hide entity ASCII glyphs in ASCII mode (tile renderer owns display)"
```

---

### Task 4: Entity background blending for water/gas/fire tiles

**Files:**
- Modify: `src/map/ascii_renderer.rs`

When an entity sits on a special tile (water, gas, fire), the background should reflect the tile effect, not just plain lit terrain. For example, an item on water should show blue shimmer background.

- [ ] **Step 1: Use tile effect background for entity cells**

In the entity branch of the cascade (the `if let Some(entity_cell) = cell_entities.get(...)` block), replace the simple base_bg + lighting with the full tile effect cascade for the background:

```rust
            if let Some(entity_cell) = cell_entities.get(&(x, y)) {
                glyph_char = entity_cell.glyph.clone();
                fg_color = entity_cell.color;
                // Background uses tile effect cascade (water shimmer, gas, fire glow)
                let phase = (x as f32 * 1.7 + y as f32 * 2.3).fract();
                if fire_tiles.0.contains(&(x, y)) {
                    let (_, fire_bg) = compute_fire_colors(t, phase);
                    bg_color = fire_bg;
                } else if let Some(gas) = gas_tiles.0.get(&(x, y)) {
                    let base_bg = resolve_tile_bg(tile, manifest);
                    let (light, light_color) = get_light(idx, &light_map);
                    let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                    let lit_bg = apply_light_to_color(base_bg, light_amount, light_color);
                    let gas_bg = compute_gas_bg(gas.gas_type, gas.concentration, t, phase);
                    let alpha = match gas.concentration { 3 => 0.85, 2 => 0.6, _ => 0.35 };
                    let g = gas_bg.to_srgba();
                    let b = lit_bg.to_srgba();
                    bg_color = Color::srgb(
                        g.red * alpha + b.red * (1.0 - alpha),
                        g.green * alpha + b.green * (1.0 - alpha),
                        g.blue * alpha + b.blue * (1.0 - alpha),
                    );
                } else if water_tiles.0.contains_key(&(x, y)) {
                    let liquid = water_tiles.0.get(&(x, y)).copied().unwrap_or(LiquidType::ShallowWater);
                    let (light, _) = get_light(idx, &light_map);
                    let variation = if liquid == LiquidType::Water { 0.10 } else { 0.05 };
                    let bg_base = match liquid {
                        LiquidType::Water => [0.37_f32, 0.37, 0.79],
                        _ => [0.44_f32, 0.63, 0.93],
                    };
                    let r_wave = (t * 2.0 + phase * TAU).sin();
                    let g_wave = (t * 1.7 + phase * TAU + 1.0).sin();
                    let b_wave = (t * 1.3 + phase * TAU + 2.0).sin();
                    bg_color = Color::srgb(
                        (bg_base[0] * light * (1.0 + r_wave * variation)).clamp(0.0, 1.0),
                        (bg_base[1] * light * (1.0 + g_wave * variation)).clamp(0.0, 1.0),
                        (bg_base[2] * light * (1.0 + b_wave * variation)).clamp(0.0, 1.0),
                    );
                } else {
                    let base_bg = resolve_tile_bg(tile, manifest);
                    let (light, light_color) = get_light(idx, &light_map);
                    let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                    bg_color = apply_light_to_color(base_bg, light_amount, light_color);
                }
            } else if fire_tiles.0.contains(&(x, y)) {
```

- [ ] **Step 2: Extract a `resolve_tile_bg_color` helper to avoid duplication**

The tile-effect background logic is now duplicated (entity branch + tile-only branches). Extract a helper:

```rust
/// Resolve the background color for a cell given its tile state and effects.
fn resolve_cell_bg(
    tile: crate::map::tile::Tile,
    manifest: &TileManifest,
    idx: usize,
    light_map: &LightMap,
    fire_tiles: &FireTiles,
    gas_tiles: &GasTiles,
    water_tiles: &WaterTiles,
    x: i32, y: i32,
    t: f32,
    phase: f32,
) -> Color {
    // ... extracted logic
}
```

Use this helper in both the entity branch and the tile-only branches.

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 4: Run tests**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/map/ascii_renderer.rs
git commit -m "feat: entity backgrounds reflect tile effects (water/gas/fire)"
```

---

### Task 5: Final verification and doc update

**Files:**
- Modify: `docs/design/ASCII_RENDERER.md`

- [ ] **Step 1: Run full test suite**

Run: `cargo test --bin bevy_rpg`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No new warnings from our changes.

- [ ] **Step 3: Manual verification**

Run the game (`cargo run`) and verify in ASCII mode:
- Player `@` displays correctly on all tile types
- Monsters display with correct glyphs, stunned monsters are yellow
- Items on water tiles are VISIBLE (the original bug)
- Items drifting in water show item glyph over blue water background
- Fire `^` still animates with orange flicker
- Steam/gas shows on water tiles correctly
- Props (chests, etc.) display correctly
- Toggle sprite/ASCII mode (F5) — no visual glitches
- Submerged monsters are hidden
- Explored-but-not-visible tiles show dimmed terrain (no entities)

- [ ] **Step 4: Update design doc**

Update `docs/design/ASCII_RENDERER.md` to document the entity integration:
- Add entity priority to the cascade documentation
- Note that entity visibility systems are sprite-only in ASCII mode
- Document the `CellEntity` spatial lookup approach

- [ ] **Step 5: Commit**

```bash
git add docs/design/ASCII_RENDERER.md
git commit -m "docs: update ASCII renderer spec with entity integration"
```
