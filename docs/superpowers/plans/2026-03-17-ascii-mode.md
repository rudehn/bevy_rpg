# ASCII Graphics Mode Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Brogue-style ASCII rendering mode toggleable mid-game with F5, using hex-color definitions in existing RON manifests.

**Architecture:** Each game entity gets `AsciiGlyph` (Text2d) and optionally `AsciiBackground` (solid sprite) children at spawn time. `GraphicsMode` resource controls which children are visible. Parent sprites become transparent (`Color::NONE`) in ASCII mode instead of hidden, so FOV visibility propagation still works. Visibility systems branch on `GraphicsMode` to dim the correct children.

**Tech Stack:** Bevy 0.17, serde/RON, bracket-lib

**Spec:** `docs/superpowers/specs/2026-03-17-ascii-mode-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/game/ascii_mode.rs` | New file: `GraphicsMode` resource, `AsciiBackground`/`AsciiGlyph`/`LiquidOverlay` marker components, `AsciiFont` resource, mode toggle system, mode swap system |
| `src/assets/mod.rs` | Add `ascii_char`, `ascii_fg`, `ascii_bg` fields to asset structs. Add hex color serde deserializer. |
| `src/map/tile.rs` | Spawn ASCII children on tile entities. Add `LiquidOverlay` marker. |
| `src/game/spawner.rs` | Spawn `AsciiGlyph` children on monsters, items, props. |
| `src/player/mod.rs` | Spawn `AsciiGlyph` child on player entity. |
| `src/map/map.rs` | Branch `update_tile_visibility` on `GraphicsMode`. |
| `src/game/systems.rs` | Branch item/prop/monster visibility + status visuals on `GraphicsMode`. |
| `src/game/mod.rs` | Register `AsciiModePlugin`. |
| RON files | Add ASCII fields to all entity definitions. |

---

### Task 1: Create `ascii_mode.rs` — resources and marker components

**Files:**
- Create: `src/game/ascii_mode.rs`
- Modify: `src/game/mod.rs` (register plugin)

- [ ] **Step 1: Create the module with core types**

Create `src/game/ascii_mode.rs`:

```rust
use bevy::prelude::*;

/// Controls whether the game renders sprites or ASCII characters.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum GraphicsMode {
    #[default]
    Sprites,
    Ascii,
}

/// Marker for the solid-color background quad on tile entities (ASCII mode).
#[derive(Component)]
pub struct AsciiBackground;

/// Marker for the Text2d character glyph on any entity (ASCII mode).
#[derive(Component)]
pub struct AsciiGlyph;

/// Marker for liquid overlay sprite children on tile entities.
#[derive(Component)]
pub struct LiquidOverlay;

/// Monospace font handle for ASCII glyphs.
#[derive(Resource)]
pub struct AsciiFont(pub Handle<Font>);

pub struct AsciiModePlugin;

impl Plugin for AsciiModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GraphicsMode>()
            .add_systems(Startup, load_ascii_font)
            .add_systems(
                Update,
                (toggle_graphics_mode, apply_graphics_mode_swap)
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

fn load_ascii_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/SourceCodePro-Regular.ttf");
    commands.insert_resource(AsciiFont(font));
}

/// F5 toggles between Sprites and ASCII mode.
fn toggle_graphics_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<GraphicsMode>,
) {
    if keys.just_pressed(KeyCode::F5) {
        *mode = match *mode {
            GraphicsMode::Sprites => GraphicsMode::Ascii,
            GraphicsMode::Ascii => GraphicsMode::Sprites,
        };
    }
}

/// When GraphicsMode changes, swap visibility of sprite vs ASCII children.
fn apply_graphics_mode_swap(
    mode: Res<GraphicsMode>,
    mut tile_query: Query<&mut Sprite, With<crate::map::tile::TileMarker>>,
    mut ascii_bg_query: Query<&mut Visibility, (With<AsciiBackground>, Without<AsciiGlyph>, Without<LiquidOverlay>)>,
    mut ascii_glyph_query: Query<&mut Visibility, (With<AsciiGlyph>, Without<AsciiBackground>, Without<LiquidOverlay>)>,
    mut liquid_query: Query<&mut Visibility, (With<LiquidOverlay>, Without<AsciiBackground>, Without<AsciiGlyph>)>,
    mut entity_sprite_query: Query<&mut Sprite, (Without<crate::map::tile::TileMarker>, Without<AsciiBackground>)>,
) {
    if !mode.is_changed() {
        return;
    }

    let is_ascii = *mode == GraphicsMode::Ascii;

    // Tile sprites: transparent in ASCII mode
    for mut sprite in tile_query.iter_mut() {
        sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
    }

    // ASCII backgrounds: visible in ASCII mode
    for mut vis in ascii_bg_query.iter_mut() {
        *vis = if is_ascii { Visibility::Inherited } else { Visibility::Hidden };
    }

    // ASCII glyphs: visible in ASCII mode
    for mut vis in ascii_glyph_query.iter_mut() {
        *vis = if is_ascii { Visibility::Inherited } else { Visibility::Hidden };
    }

    // Liquid overlays: hidden in ASCII mode
    for mut vis in liquid_query.iter_mut() {
        *vis = if is_ascii { Visibility::Hidden } else { Visibility::Inherited };
    }

    // Non-tile entity sprites (monsters, items, props, player): transparent in ASCII mode
    for mut sprite in entity_sprite_query.iter_mut() {
        sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
    }
}
```

- [ ] **Step 2: Register the plugin in `src/game/mod.rs`**

Add `pub mod ascii_mode;` to the module declarations and add `ascii_mode::AsciiModePlugin` to the game plugin registration.

- [ ] **Step 3: Add a placeholder monospace font**

Download or add `assets/fonts/SourceCodePro-Regular.ttf` (SIL Open Font License). If unavailable, any monospace TTF works.

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: Compiles. The swap system won't do anything yet since no ASCII children exist.

- [ ] **Step 5: Commit**

```bash
git add src/game/ascii_mode.rs src/game/mod.rs assets/fonts/
git commit -m "feat(ascii): add GraphicsMode resource, markers, toggle and swap systems"
```

---

### Task 2: Add hex color deserializer and ASCII fields to asset structs

**Files:**
- Modify: `src/assets/mod.rs` (serde_helpers, TileAsset, MonsterAsset, ItemAsset, PropAsset, PlayerAsset)

- [ ] **Step 1: Add hex color deserializer to `serde_helpers`**

In `src/assets/mod.rs`, inside the `serde_helpers` module, add:

```rust
    /// Deserialize a "#RRGGBB" hex string into a Bevy Color.
    pub fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<bevy::prelude::Color, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(parse_hex_color(&s))
    }

    /// Deserialize an optional "#RRGGBB" hex string.
    pub fn deserialize_hex_color_option<'de, D>(deserializer: D) -> Result<Option<bevy::prelude::Color>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        Ok(s.map(|s| parse_hex_color(&s)))
    }

    fn parse_hex_color(s: &str) -> bevy::prelude::Color {
        let s = s.trim_start_matches('#');
        if s.len() != 6 {
            return bevy::prelude::Color::WHITE;
        }
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
        bevy::prelude::Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }
```

- [ ] **Step 2: Add ASCII fields to `TileAsset`**

```rust
pub struct TileAsset {
    pub sprite: String,
    #[serde(default)]
    pub grid_size: Option<UVec2>,
    #[serde(default)]
    pub tile_size: Option<UVec2>,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
    #[serde(default = "default_black", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_bg: Color,
}
```

Add default functions:
```rust
fn default_white() -> Color { Color::WHITE }
fn default_black() -> Color { Color::BLACK }
```

- [ ] **Step 3: Add ASCII fields to `MonsterAsset`**

Add to `MonsterAsset`:
```rust
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
```

- [ ] **Step 4: Add ASCII fields to `ItemAsset`, `PropAsset`, `PlayerAsset`**

Same pattern: `ascii_char: String` + `ascii_fg: Color` on each.

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Expected: Compiles. Existing RON files work unchanged (all fields have defaults).

- [ ] **Step 6: Commit**

```bash
git add src/assets/mod.rs
git commit -m "feat(ascii): add hex color deserializer and ascii fields to all asset structs"
```

---

### Task 3: Spawn ASCII children on tile entities

**Files:**
- Modify: `src/map/tile.rs` (spawn_tile_entity)

- [ ] **Step 1: Add `LiquidOverlay` marker to liquid children**

In `spawn_tile_entity()`, where the liquid child is spawned (the `.spawn((...))` block around line 206), add `crate::game::ascii_mode::LiquidOverlay` to the component tuple.

- [ ] **Step 2: Spawn `AsciiBackground` and `AsciiGlyph` children**

After the liquid child spawning block and before returning `tile_entity`, add:

```rust
    // ASCII mode children
    if let Some(ascii_font) = world_or_commands.get_resource::<crate::game::ascii_mode::AsciiFont>() {
        // ... lookup ascii_char/fg/bg from tile manifest
    }
```

The function takes `commands: &mut Commands` and various resources. Add `ascii_font: &Res<AsciiFont>` as a parameter (or access from commands context). The function signature needs the font resource.

**Approach:** Since `spawn_tile_entity` is a plain function (not a system), pass the `AsciiFont` handle and `GraphicsMode` through. Or, spawn the ASCII children unconditionally as `Visibility::Hidden` — the swap system handles visibility.

Spawn two children:

1. **AsciiBackground**: A `Sprite` with solid white texture scaled to GRID_SIZE, tinted to `ascii_bg`:

```rust
let bg_child = commands.spawn((
    Sprite {
        color: tile_asset.ascii_bg,
        custom_size: Some(GRID_SIZE),
        ..default()
    },
    Transform::from_translation(Vec3::ZERO),
    Visibility::Hidden,
    crate::game::ascii_mode::AsciiBackground,
    RenderLayers::layer(1),
)).id();
commands.entity(tile_entity).add_child(bg_child);
```

2. **AsciiGlyph**: A `Text2d` with the character:

```rust
let ascii_char = if tile_asset.ascii_char.is_empty() { "?".to_string() } else { tile_asset.ascii_char.clone() };
let glyph_child = commands.spawn((
    Text2d::new(ascii_char),
    TextFont {
        font: ascii_font.0.clone(),
        font_size: 14.0,
        ..default()
    },
    TextColor(tile_asset.ascii_fg),
    TextLayout::new_with_justify(JustifyText::Center),
    Transform::from_translation(Vec3::new(0.0, 0.0, 0.05)),
    Visibility::Hidden,
    crate::game::ascii_mode::AsciiGlyph,
    RenderLayers::layer(1),
)).id();
commands.entity(tile_entity).add_child(glyph_child);
```

If a liquid exists, override the background color with the liquid's `ascii_bg`.

- [ ] **Step 3: Verify compilation and test**

Run: `cargo check`
Run the game, press F5 — tiles should swap to ASCII characters.

- [ ] **Step 4: Commit**

```bash
git add src/map/tile.rs
git commit -m "feat(ascii): spawn AsciiBackground and AsciiGlyph children on tiles"
```

---

### Task 4: Spawn ASCII children on monsters, items, props, player

**Files:**
- Modify: `src/game/spawner.rs` (spawn_monster, spawn_item, spawn_prop)
- Modify: `src/player/mod.rs` (player spawn)

- [ ] **Step 1: Add AsciiGlyph child to `spawn_monster()`**

After the entity is spawned with its `Sprite`, add a child:

```rust
let ascii_char = if monster_asset.ascii_char.is_empty() { "?" } else { &monster_asset.ascii_char };
let glyph = commands.spawn((
    Text2d::new(ascii_char.to_string()),
    TextFont { font: ascii_font.0.clone(), font_size: 14.0, ..default() },
    TextColor(monster_asset.ascii_fg),
    TextLayout::new_with_justify(JustifyText::Center),
    Transform::from_translation(Vec3::ZERO),
    Visibility::Hidden,
    crate::game::ascii_mode::AsciiGlyph,
    RenderLayers::layer(1),
)).id();
commands.entity(monster_entity).add_child(glyph);
```

The `AsciiFont` resource needs to be passed to `spawn_monster()`. Add it as a parameter.

- [ ] **Step 2: Same pattern for `spawn_item()` and `spawn_prop()`**

Each spawner creates an `AsciiGlyph` child using the asset's `ascii_char`/`ascii_fg`.

- [ ] **Step 3: Add `@` glyph to player entity**

In `src/player/mod.rs`, after the player entity is spawned, add an `AsciiGlyph` child with `"@"` in white.

- [ ] **Step 4: Verify — F5 should now toggle ALL entity types**

Run: `cargo run`, press F5.
Expected: All tiles, monsters, items, props, and player swap between sprites and ASCII.

- [ ] **Step 5: Commit**

```bash
git add src/game/spawner.rs src/player/mod.rs
git commit -m "feat(ascii): spawn AsciiGlyph children on monsters, items, props, player"
```

---

### Task 5: Branch visibility systems on GraphicsMode

**Files:**
- Modify: `src/map/map.rs` (update_tile_visibility)
- Modify: `src/game/systems.rs` (update_item_visibility, update_prop_visibility, update_monster_visibility, update_status_visuals)

- [ ] **Step 1: Branch `update_tile_visibility` on GraphicsMode**

Add `mode: Res<GraphicsMode>` to the system parameters. Add `Children` and child queries for `AsciiBackground` and `AsciiGlyph`.

In the FOV-visible branch:
- Sprites mode: set `sprite.color` with light tint as today
- ASCII mode: set parent `sprite.color = Color::NONE`, set `AsciiBackground` child sprite color to full `ascii_bg`, set `AsciiGlyph` child `TextColor` to full `ascii_fg`

In the explored-not-visible branch:
- Sprites mode: `sprite.color = srgb(0.5, 0.5, 0.5)` as today
- ASCII mode: parent `sprite.color = Color::NONE`, `AsciiBackground` color = dimmed bg (multiply by 0.5), `AsciiGlyph` `TextColor` = dimmed fg

In the unexplored branch: `Visibility::Hidden` on parent (hides all children automatically).

- [ ] **Step 2: Branch `update_item_visibility` and `update_prop_visibility`**

Add `mode: Res<GraphicsMode>` parameter. In ASCII mode, set parent `sprite.color = Color::NONE` and modify the `AsciiGlyph` child's `TextColor` for dimming. Query children via `Children` component.

- [ ] **Step 3: Branch `update_monster_visibility`**

Monsters are binary visible/hidden (no dimming). In ASCII mode, when visible: parent `sprite.color = Color::NONE`. When hidden: `Visibility::Hidden` (cascades to glyph child).

- [ ] **Step 4: Branch `update_status_visuals`**

In ASCII mode, status tinting applies to the `AsciiGlyph` `TextColor` instead of `sprite.color`. Stunned = yellow tint on the character.

- [ ] **Step 5: Verify full visibility behavior**

Run: `cargo run` in ASCII mode. Walk around. Verify:
1. Tiles in FOV show full color characters
2. Explored tiles show dimmed characters
3. Unexplored tiles are hidden
4. Monsters appear/disappear with FOV
5. Items show dimmed when explored but not visible

- [ ] **Step 6: Commit**

```bash
git add src/map/map.rs src/game/systems.rs
git commit -m "feat(ascii): branch all visibility systems on GraphicsMode"
```

---

### Task 6: Populate ASCII data in RON files

**Files:**
- Modify: `assets/tiles.ron`
- Modify: `assets/monsters.ron`
- Modify: `assets/items.ron`
- Modify: `assets/props.ron`
- Modify: `assets/player.ron`

- [ ] **Step 1: Add ASCII data to `tiles.ron`**

For each terrain and liquid type, add `ascii_char`, `ascii_fg`, `ascii_bg`:

| Type | Char | FG | BG |
|------|------|----|----|
| Floor | `.` | `#808080` | `#141414` |
| Wall | `#` | `#A0A0A0` | `#282828` |
| Door | `+` | `#B4783C` | `#141414` |
| OpenDoor | `'` | `#B4783C` | `#141414` |
| DownStairs | `>` | `#FFFFFF` | `#141414` |
| UpStairs | `<` | `#FFFFFF` | `#141414` |
| Empty | ` ` | `#000000` | `#000000` |
| HiddenDoor | `#` | `#A0A0A0` | `#282828` |
| Water | `~` | `#508CDC` | `#0A143C` |
| ShallowWater | `~` | `#6496C8` | `#141E3C` |
| Lava | `~` | `#FFA028` | `#501400` |

- [ ] **Step 2: Add ASCII data to `monsters.ron`**

Each monster gets a character (typically first letter of species, lowercase) and a faction-themed color. Read `assets/monsters.ron` to get the full list and assign characters. Examples:

| Monster | Char | Color Rationale |
|---------|------|----------------|
| Goblin Scout | `g` | Green (goblin faction) |
| Goblin Archer | `g` | Green |
| Orc Warrior | `o` | Olive/brown |
| Skeleton | `s` | Light grey |
| Zombie | `z` | Dark green |
| Spider | `S` | Brown (capital = larger) |
| Boss | `D` | Red/purple (capital = unique) |

Use lowercase for common monsters, uppercase for elite/boss/large variants.

- [ ] **Step 3: Add ASCII data to `items.ron`**

Standard roguelike conventions:
- Weapons: `/` (swords), `|` (polearms), `)` (axes)
- Armor: `[` (body), `]` (shield)
- Potions: `!`
- Scrolls/tomes: `?`
- Arrows: `|`
- Gold/gems: `$`

- [ ] **Step 4: Add ASCII data to `props.ron`**

| Prop | Char | Color |
|------|------|-------|
| barricade | `=` | Brown |
| barrel | `o` | Brown |
| chest | `$` | Gold |
| small_chest | `$` | Dark gold |
| candle | `*` | Yellow |
| watchfire | `*` | Orange |
| totem_pole | `T` | Brown |
| fountain | `{` | Cyan |

- [ ] **Step 5: Add `@` to `player.ron`**

```ron
( ..., ascii_char: "@", ascii_fg: "#FFFFFF" )
```

- [ ] **Step 6: Run game and verify all entities render in ASCII mode**

Run: `cargo run`, press F5.
Expected: Every entity type shows a colored character. No `?` fallbacks (meaning all entries have ascii_char defined).

- [ ] **Step 7: Commit**

```bash
git add assets/tiles.ron assets/monsters.ron assets/items.ron assets/props.ron assets/player.ron
git commit -m "feat(ascii): populate ASCII characters and colors in all RON manifests"
```

---

### Task 7: Smoke test and polish

- [ ] **Step 1: Run `cargo clippy`**

Fix any warnings in changed files.

- [ ] **Step 2: Test mode toggle mid-combat**

Start a game, find monsters, toggle F5 during combat. Verify no panics, no visual glitches, smooth swap.

- [ ] **Step 3: Test explored tile dimming**

Walk through rooms, walk away, check that explored tiles show dimmed ASCII in the fog.

- [ ] **Step 4: Test save/load across modes**

Save in sprite mode, load in ASCII mode (and vice versa). Verify no crashes.

- [ ] **Step 5: Tune font size**

Adjust `font_size` in the `AsciiGlyph` spawn code until characters fill the 16x16 grid cell cleanly. Try 12-16pt, or add a small `Transform` scale. The character should be clearly visible and centered.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "fix(ascii): polish and clippy fixes for ASCII mode"
```
