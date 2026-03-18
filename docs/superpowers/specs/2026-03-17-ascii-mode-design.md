# ASCII Graphics Mode Design

## Goal

Add a Brogue-style ASCII rendering mode as an alternative to the existing sprite-based
graphics. The player can toggle between modes mid-game with a keybind. Each entity type
(tiles, monsters, items, props, player) gets an ASCII character and foreground color
defined in its existing RON manifest. Tile backgrounds provide the atmospheric coloring
(dark blue water, orange lava, grey stone).

## Motivation

Classic roguelike aesthetics are part of the genre's identity. An ASCII mode adds visual
variety, appeals to traditional roguelike players, and provides a fallback for players
who prefer clarity over art. The Brogue-style approach (rich fg+bg colors) keeps the mode
visually appealing rather than purely functional.

---

## Data Model

### New Fields on Existing RON Assets

All fields are `#[serde(default)]` — existing RON files work without changes until
ASCII values are populated.

**Color format:** `"#RRGGBB"` hex strings. A small serde deserializer parses these into
Bevy `Color` values. Defaults: `ascii_fg: "#FFFFFF"`, `ascii_bg: "#000000"`,
`ascii_char: ""` (empty = fallback to `"?"`).

#### TileAsset (tiles.ron)

Terrain and liquid types get `ascii_char`, `ascii_fg`, and `ascii_bg`:

```ron
"Floor":       ( sprite: "...", ascii_char: ".", ascii_fg: "#808080", ascii_bg: "#141414" ),
"Wall":        ( sprite: "...", ascii_char: "#", ascii_fg: "#A0A0A0", ascii_bg: "#282828" ),
"Door":        ( sprite: "...", ascii_char: "+", ascii_fg: "#B4783C", ascii_bg: "#141414" ),
"OpenDoor":    ( sprite: "...", ascii_char: "'", ascii_fg: "#B4783C", ascii_bg: "#141414" ),
"DownStairs":  ( sprite: "...", ascii_char: ">", ascii_fg: "#FFFFFF", ascii_bg: "#141414" ),
"UpStairs":    ( sprite: "...", ascii_char: "<", ascii_fg: "#FFFFFF", ascii_bg: "#141414" ),

// Liquids — ascii_bg replaces terrain bg when liquid present
"Water":        ( sprite: "...", ascii_char: "~", ascii_fg: "#508CDC", ascii_bg: "#0A143C" ),
"ShallowWater": ( sprite: "...", ascii_char: "~", ascii_fg: "#6496C8", ascii_bg: "#141E3C" ),
"Lava":         ( sprite: "...", ascii_char: "~", ascii_fg: "#FFA028", ascii_bg: "#501400" ),
```

#### MonsterAsset (monsters.ron)

Monsters get `ascii_char` and `ascii_fg` only — background comes from terrain:

```ron
"Goblin Scout":   ( ..., ascii_char: "g", ascii_fg: "#3CB43C" ),
"Orc Warrior":    ( ..., ascii_char: "o", ascii_fg: "#8CB450" ),
"Skeleton":       ( ..., ascii_char: "s", ascii_fg: "#C8C8C8" ),
```

#### ItemAsset (items.ron)

```ron
"Iron Sword":     ( ..., ascii_char: "/", ascii_fg: "#C8C8D2" ),
"Health Potion":  ( ..., ascii_char: "!", ascii_fg: "#DC3232" ),
"Wooden Arrow":   ( ..., ascii_char: "|", ascii_fg: "#B49664" ),
```

#### PropAsset (props.ron)

```ron
"barricade":  ( ..., ascii_char: "=", ascii_fg: "#A0783C" ),
"chest":      ( ..., ascii_char: "$", ascii_fg: "#DCB428" ),
"candle":     ( ..., ascii_char: "*", ascii_fg: "#FFDC64" ),
"watchfire":  ( ..., ascii_char: "*", ascii_fg: "#FF9632" ),
"barrel":     ( ..., ascii_char: "o", ascii_fg: "#96783C" ),
"totem_pole": ( ..., ascii_char: "T", ascii_fg: "#B46E3C" ),
```

#### PlayerAsset (player.ron)

```ron
( ..., ascii_char: "@", ascii_fg: "#FFFFFF" )
```

#### Structures (future)

The structure system (`structures.ron`, `StructureAsset`, `spawn_structure()`) does not
exist yet. When it is implemented, structure entities should follow the same pattern as
props: add `ascii_char` and `ascii_fg` fields to the asset type, and spawn an
`AsciiGlyph` child. Suggested defaults for reference:

```ron
"Goblin Totem":      ( ..., ascii_char: "T", ascii_fg: "#3CB43C" ),
"Explosive Barrel":  ( ..., ascii_char: "o", ascii_fg: "#FF5A28" ),
"Poison Mushroom":   ( ..., ascii_char: "m", ascii_fg: "#6E28B4" ),
"Healing Spring":    ( ..., ascii_char: "{", ascii_fg: "#50C878" ),
```

#### Empty Terrain

`TerrainType::Empty` renders as a blank space with black background:
```ron
"Empty": ( sprite: "...", ascii_char: " ", ascii_fg: "#000000", ascii_bg: "#000000" ),
```

### Serde Hex Color Deserializer

Add a helper to `src/assets/mod.rs` (alongside existing serde helpers):

```rust
fn deserialize_hex_color(s: &str) -> Color {
    // Parse "#RRGGBB" → Color::srgb(r/255, g/255, b/255)
    // Returns WHITE on parse failure
}
```

Used via `#[serde(default, deserialize_with = "...")]` on the new fields.

---

## Graphics Mode Resource

```rust
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum GraphicsMode {
    #[default]
    Sprites,
    Ascii,
}
```

- Inserted at app startup
- Toggled by `F5` keypress (or configurable)
- Persisted to save file (optional — could also just default to Sprites each launch)

---

## Rendering Architecture

### Entity Hierarchy

**Key constraint:** Currently, the terrain `Sprite` component lives directly on the
tile entity (not as a child). Similarly, monster/item/prop/player sprites are on the
entity itself. We do NOT restructure this — instead, in ASCII mode we make the parent
sprite invisible by setting `sprite.color = Color::NONE` (fully transparent). This
avoids `Visibility::Hidden` on the parent, which would cascade and hide children too.

**Tile entities:**
```
TileEntity (Position, TileMarker, Sprite ← terrain atlas on parent)
  ├── AsciiBackground (solid color sprite)  — visible in ASCII mode
  ├── AsciiGlyph (Text2d character)         — visible in ASCII mode
  └── LiquidOverlay (sprite overlay)        — visible in Sprites mode only
```

**Non-tile entities (monsters, items, props, player):**
```
MonsterEntity (Position, Monster, Sprite ← monster atlas on parent)
  └── AsciiGlyph (Text2d character)         — visible in ASCII mode
```

Non-tile entities do NOT get an `AsciiBackground` — the terrain tile beneath them
provides the background color. The character renders on top at its Z-layer.

**New marker: `LiquidOverlay`** — Added to liquid child sprites during tile spawning
so the mode-swap system can identify and hide them in ASCII mode. Currently liquid
children have no marker component.

### Marker Components

```rust
#[derive(Component)]
struct AsciiBackground;

#[derive(Component)]
struct AsciiGlyph;

#[derive(Component)]
struct LiquidOverlay;
```

`AsciiBackground` and `AsciiGlyph` mark ASCII rendering children. `LiquidOverlay`
is added to existing liquid child sprites (in `spawn_tile_entity()`) so the mode-swap
system can identify and hide them in ASCII mode.

### ASCII Background Rendering

A `Sprite` with a solid 1×1 white pixel texture, scaled to `GRID_SIZE` (16×16),
tinted to the tile's `ascii_bg` color. When a liquid overlay exists, use the liquid's
`ascii_bg` instead.

Z-ordering within a tile:
- `AsciiBackground` at z = 0.0 (same as tile sprite)
- `AsciiGlyph` at z = 0.05 (above background, below liquid)

### ASCII Glyph Rendering

A `Text2d` entity with:
- The `ascii_char` string
- `TextFont` using a monospace font sized to fill one grid cell
- `TextColor` set to `ascii_fg`
- `TextLayout` centered in the cell
- `RenderLayers::layer(1)` (game world camera)

### Font

A monospace font (e.g., `fonts/SourceCodePro-Regular.ttf`) loaded at startup. Font
size calculated to fill `GRID_SIZE` (16px). Stored in a resource for reuse:

```rust
#[derive(Resource)]
struct AsciiFont(Handle<Font>);
```

---

## Mode Toggle System

### Keybind

`F5` toggles `GraphicsMode` between `Sprites` and `Ascii`. System runs in `Update`,
gated on `in_state(AppState::InGame)`.

### Swap Logic

When `GraphicsMode` changes (detected via resource change detection):

**Tiles** (query entities with `TileMarker` + `&mut Sprite`):
- Sprites mode: parent `sprite.color = WHITE`, children `AsciiBackground` → `Visibility::Hidden`, `AsciiGlyph` → `Visibility::Hidden`, `LiquidOverlay` children → `Visibility::Inherited`
- ASCII mode: parent `sprite.color = Color::NONE` (transparent — keeps entity visible for FOV but hides the atlas sprite), children `AsciiBackground` → `Visibility::Inherited`, `AsciiGlyph` → `Visibility::Inherited`, `LiquidOverlay` children → `Visibility::Hidden`

**Non-tile entities** (monsters, items, props, player — query `&mut Sprite` + children with `AsciiGlyph`):
- Sprites mode: parent `sprite.color = WHITE`, `AsciiGlyph` child → `Visibility::Hidden`
- ASCII mode: parent `sprite.color = Color::NONE`, `AsciiGlyph` child → `Visibility::Inherited`

**Why `Color::NONE` instead of `Visibility::Hidden`:** The parent entity's `Visibility`
is controlled by the FOV system. Setting it to `Hidden` would cascade to all children.
Instead, we make the parent sprite transparent while keeping the entity itself visible,
so ASCII children can render when the FOV system says the entity is visible.

### Visibility & Dimming in ASCII Mode

The existing visibility systems (`update_tile_visibility`, `update_monster_visibility`,
`update_item_visibility`, `update_prop_visibility`) set `sprite.color` on the parent
entity for dimming. These systems need to branch on `GraphicsMode`:

**In Sprites mode** (no change): Set `sprite.color` on the parent entity as today.

**In ASCII mode**: Instead of modifying the parent `sprite.color`, modify the ASCII
children:
- Query children with `AsciiGlyph` → set `TextColor` to dimmed `ascii_fg`
- Query children with `AsciiBackground` → set `sprite.color` to dimmed `ascii_bg`

**Dimming states:**
- **Currently visible:** `AsciiGlyph` = full `ascii_fg`, `AsciiBackground` = full `ascii_bg`
- **Explored, not visible:** Both colors multiplied by 0.5
- **Unexplored:** Parent `Visibility::Hidden` → children hidden automatically

**LightMap interaction in ASCII mode:** The existing sprite mode applies a warm
light-map tint (`Color::srgb(light, light * 0.95, light * 0.8)`) to visible tiles.
In ASCII mode, skip the light-map tinting entirely — visible tiles render at full
`ascii_fg`/`ascii_bg` color. This is explicitly out of scope (see Out of Scope) and
keeps the implementation simple. The `update_tile_visibility` system branches: if
`GraphicsMode::Ascii`, set full color on ASCII children instead of light-tinted color
on parent sprite.

### Font Sizing Note

A 16pt monospace font does not produce exactly 16×16 pixel glyphs due to font metrics
(ascender/descender, advance width). The implementation should experiment with font
sizes (likely 12-14pt) and optionally apply a `Transform` scale on the `AsciiGlyph`
to force-fit the cell. The `AsciiGlyph` child's local transform can be adjusted
independently of the parent.

---

## Spawn Integration

### Tile Spawning (`tile.rs`)

`spawn_tile_entity()` currently creates the sprite and liquid child. Add after sprite
creation:

1. Create `AsciiBackground` child: solid color sprite at z=0.0, tinted to `ascii_bg`
2. Create `AsciiGlyph` child: `Text2d` with `ascii_char` at z=0.05
3. Both start `Visibility::Hidden` (since default mode is Sprites)

Lookup `ascii_char`/`ascii_fg`/`ascii_bg` from `TileManifest` using the terrain type
name, same as the existing sprite lookup.

### Entity Spawning (`spawner.rs`)

`spawn_monster()`, `spawn_item()`, `spawn_prop()` each add an `AsciiGlyph` child after
the sprite. Read `ascii_char`/`ascii_fg` from the respective manifest asset.

`player_spawn_or_move_system()` in `player/mod.rs` adds the `@` glyph child.

### Structure Spawning (future)

The structure system does not exist yet. When implemented, structure entities should
follow the same pattern: spawn an `AsciiGlyph` child using `ascii_char`/`ascii_fg`
from the structure asset.

---

## Files Affected

| File | Change |
|------|--------|
| `src/assets/mod.rs` | Add `ascii_char`, `ascii_fg`, `ascii_bg` fields to `TileAsset`, `MonsterAsset`, `ItemAsset`, `PropAsset`, `PlayerAsset`. Add hex color serde deserializer. Add `AsciiFont` resource. Add `GraphicsMode` resource. |
| `src/map/tile.rs` | Spawn `AsciiBackground` + `AsciiGlyph` children on tile entities. Add `LiquidOverlay` marker to liquid child sprites. |
| `src/game/spawner.rs` | Spawn `AsciiGlyph` children on monsters, items, props |
| `src/player/mod.rs` | Spawn `AsciiGlyph` child on player entity |
| `src/game/systems.rs` | Branch visibility systems on `GraphicsMode` — sprite mode sets parent `sprite.color`, ASCII mode sets `AsciiGlyph` `TextColor` and `AsciiBackground` `sprite.color`. Add mode-swap system (triggered by `Changed<GraphicsMode>`). Add `F5` toggle system. |
| `src/map/map.rs` | Branch `update_tile_visibility` on `GraphicsMode` — skip light-map tinting in ASCII mode, instead set full color on ASCII children or dimmed color for explored tiles. |
| `src/game/camera.rs` | No changes needed — `RenderLayers::layer(1)` already covers `Text2d` on that layer |
| `assets/tiles.ron` | Add `ascii_char`, `ascii_fg`, `ascii_bg` to all entries |
| `assets/monsters.ron` | Add `ascii_char`, `ascii_fg` to all entries |
| `assets/items.ron` | Add `ascii_char`, `ascii_fg` to all entries |
| `assets/props.ron` | Add `ascii_char`, `ascii_fg` to all entries |
| `assets/structures.ron` | Future — add `ascii_char`, `ascii_fg` when structure system exists |
| `assets/player.ron` | Add `ascii_char`, `ascii_fg` |

### New Files

| File | Purpose |
|------|---------|
| `assets/fonts/SourceCodePro-Regular.ttf` | Monospace font for ASCII glyphs (or similar open-source mono font) |

---

## Out of Scope

- **ASCII-specific lighting effects** — No per-tile brightness adjustment based on the
  light map in ASCII mode. Tiles are either visible (full color), explored (dimmed), or
  hidden. Candle glow radius could be a future enhancement.
- **Animated ASCII** — Candle flicker, water shimmer, etc. Characters are static in ASCII
  mode. Could add color cycling later.
- **ASCII-specific UI changes** — The UI (game log, inventory, character info) stays the
  same in both modes. Only the game world rendering changes.
- **Save/load persistence of GraphicsMode** — Default to Sprites on launch. Could persist
  later as a user preference.
- **Particle effects in ASCII mode** — Floating damage numbers and status particles use
  `Text2d` already and work in both modes. No changes needed.
