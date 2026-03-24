# Phase 4: Decoration Propagation System

## Overview

A Brogue-style system for spreading environmental decorations (grass, rubble, moss, fungus,
cobwebs) across the dungeon floor. Decorations are purely visual — they don't affect gameplay
mechanics. They make dungeons feel organic and lived-in.

Based on Brogue's `spawnMapDF` / autogenerator system and rogue-monster's propagation rules.

## Data Model

### Decoration Enum (already added in Phase 1)

```rust
pub enum Decoration {
    None, Grass, TallGrass, DeadGrass, Rubble, Moss, Fungus, Cobweb, Bloodstain, ScorchedEarth,
}
```

Lives as a field on `Tile` — not a separate ECS entity.

### DecorationRule (RON asset)

**File**: `assets/decorations.ron` (NEW)

```ron
DecorationCatalog(
    rules: [
        // --- ORGANIC ---
        ( name: "Grass Patch",
          min_floor: 1, max_floor: 10,
          min_seeds: 2, max_seeds: 5,
          decoration: Grass,
          requires_terrain: [Floor],
          propagation_chance: 0.60,
          propagation_decay: 0.85,
          max_propagation_depth: 12,
          chain: Some(( decoration: TallGrass, chance: 0.20 )),
        ),
        ( name: "Dead Grass",
          min_floor: 4, max_floor: 20,
          min_seeds: 1, max_seeds: 3,
          decoration: DeadGrass,
          requires_terrain: [Floor],
          propagation_chance: 0.55,
          propagation_decay: 0.80,
          max_propagation_depth: 8,
        ),

        // --- DEBRIS ---
        ( name: "Rubble",
          min_floor: 1, max_floor: 20,
          min_seeds: 1, max_seeds: 3,
          decoration: Rubble,
          requires_terrain: [Floor],
          propagation_chance: 0.40,
          propagation_decay: 0.90,
          max_propagation_depth: 6,
          wall_adjacent_only: true,
        ),

        // --- MOISTURE ---
        ( name: "Moss Growth",
          min_floor: 1, max_floor: 15,
          min_seeds: 1, max_seeds: 3,
          decoration: Moss,
          requires_terrain: [Floor],
          requires_nearby_liquid: true,
          propagation_chance: 0.50,
          propagation_decay: 0.80,
          max_propagation_depth: 8,
        ),

        // --- DEEP DUNGEON ---
        ( name: "Fungal Bloom",
          min_floor: 6, max_floor: 20,
          min_seeds: 1, max_seeds: 2,
          decoration: Fungus,
          requires_terrain: [Floor],
          propagation_chance: 0.35,
          propagation_decay: 0.75,
          max_propagation_depth: 10,
        ),

        // --- CORNERS ---
        ( name: "Cobweb",
          min_floor: 1, max_floor: 12,
          min_seeds: 2, max_seeds: 4,
          decoration: Cobweb,
          requires_terrain: [Floor],
          propagation_chance: 0.20,
          propagation_decay: 0.70,
          max_propagation_depth: 3,
          wall_adjacent_only: true,
          corner_only: true,
        ),

        // --- VIOLENCE ---
        ( name: "Bloodstain",
          min_floor: 5, max_floor: 20,
          min_seeds: 0, max_seeds: 2,
          decoration: Bloodstain,
          requires_terrain: [Floor],
          propagation_chance: 0.25,
          propagation_decay: 0.60,
          max_propagation_depth: 4,
        ),
    ]
)
```

### Rust Struct

```rust
#[derive(Deserialize, Debug, Clone)]
pub struct DecorationRule {
    pub name: String,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_seeds: i32,
    pub max_seeds: i32,
    pub decoration: Decoration,
    pub requires_terrain: Vec<TerrainType>,
    #[serde(default)]
    pub requires_nearby_liquid: bool,
    pub propagation_chance: f32,
    pub propagation_decay: f32,
    pub max_propagation_depth: i32,
    #[serde(default)]
    pub wall_adjacent_only: bool,
    #[serde(default)]
    pub corner_only: bool,
    #[serde(default)]
    pub chain: Option<DecorationChain>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DecorationChain {
    pub decoration: Decoration,
    pub chance: f32,
}
```

## Builder: DecorationPropagator

**File**: `src/map/builders/decoration_propagator.rs` (NEW)

**Pipeline position**: Runs TWICE — once before PrefabPlacer, once after MachinePlacer/ShrinePlacer.

### Algorithm

```
for each rule where min_floor <= depth <= max_floor:
    seed_count = rng.random_range(min_seeds..=max_seeds)
    seed_count = (seed_count as f32 * floor_profile.decoration_density) as i32

    for _ in 0..seed_count:
        // Find a valid seed position
        tile = random_tile_matching(requires_terrain, decoration == None)
        if wall_adjacent_only && !has_adjacent_wall(tile): skip
        if corner_only && !is_corner(tile): skip
        if requires_nearby_liquid && !has_nearby_liquid(tile, radius=3): skip

        // Place seed
        map.tiles[tile].decoration = rule.decoration

        // Propagate outward
        propagate(tile, rule.propagation_chance, depth=0)

fn propagate(pos, chance, depth):
    if depth >= max_propagation_depth: return
    for each cardinal neighbor:
        if neighbor.terrain in requires_terrain
           AND neighbor.decoration == None
           AND neighbor not in exclusion_zones:
            if rng < chance:
                if chain.is_some() && rng < chain.chance:
                    neighbor.decoration = chain.decoration
                else:
                    neighbor.decoration = rule.decoration
                propagate(neighbor, chance * propagation_decay, depth + 1)
```

### Exclusion Zones

`BuilderMap.decoration_exclusion_zones: Vec<Rect>` prevents decorations from overwriting:
- Prefab interiors (populated by PrefabPlacer)
- Machine interiors (populated by MachinePlacer)
- Shrine locations (populated by ShrinePlacer)

The first pass runs before prefabs/machines, so it fills the natural dungeon. The second pass
fills around the edges of placed encounters.

### Helper: is_corner

A floor tile is a "corner" if it has walls on two adjacent cardinal/diagonal sides forming
an L-shape. Used for cobweb placement.

### Helper: has_nearby_liquid

Returns true if any tile within `radius` has a non-None liquid type. Used for moss placement
near water/lava.

## Rendering

### Sprite Mode

Decorations spawn as a child sprite overlay on tile entities at z+0.05 (between terrain at z+0.0
and liquid at z+0.1). Each decoration type maps to a sprite entry in `tiles.ron`.

### ASCII Mode

In ASCII mode, decorations override the tile's foreground character and color:

| Decoration | Char | FG Color | Notes |
|-----------|------|----------|-------|
| Grass | `"` | `#5A9E3C` | Double-quote looks like grass blades |
| TallGrass | `"` | `#3CB43C` | Brighter green |
| DeadGrass | `"` | `#8C6432` | Brown |
| Rubble | `,` | `#808080` | Comma = small debris |
| Moss | `;` | `#3C8C50` | Semicolon = clinging growth |
| Fungus | `%` | `#9050A0` | Purple mushroom |
| Cobweb | `~` | `#C8C8C8` | Light grey wisp |
| Bloodstain | `.` | `#8C2020` | Dark red dot |
| ScorchedEarth | `.` | `#4A3020` | Dark brown |

The decoration ASCII overrides the tile's base character (`.` for floor) but the background
color stays from the terrain. This means grass on a floor tile shows `"` green on dark bg.

## Two-Pass Strategy

**Pass 1 (before PrefabPlacer)**: Fills natural dungeon spaces with organic features. Rooms
that later get a prefab stamped on them will have their decorations overwritten by the prefab's
tile data.

**Pass 2 (after MachinePlacer + ShrinePlacer)**: Fills spaces around placed encounters. The
exclusion zones prevent decorating inside encounter interiors, but the edges get natural growth.

## Save/Load

`Decoration` is a field on `Tile` which already serializes via serde. No additional save/load
work beyond what Phase 1 provided.
