# Phase 5: Machine System (Topology-Gated Encounters)

## Overview

Machines are procedurally-shaped encounters that use the dungeon's existing topology.
Unlike prefabs (which stamp fixed ASCII layouts), machines find chokepoints that gate
small regions and populate those regions with monsters, props, and loot. The same
blueprint produces different shapes on every floor because the dungeon layout is different.

Simplified from Brogue's full machine system — no sub-machines, no trigger mechanics,
no multi-step key chains. Each machine is a self-contained gated encounter.

## How Machines Differ From Prefabs

| Feature | Prefabs | Machines |
|---------|---------|----------|
| Shape | Fixed ASCII layout | Adapts to dungeon topology |
| Placement | Overlay on rooms or carve into walls | Gates existing chokepoints |
| Gating | None (open access) | Locked doors, hidden doors, guardian |
| Lock & key | No | Yes — key placed elsewhere on floor |
| Interior | Fully designed by hand | Populated procedurally from blueprint |
| Reusability | Each prefab is unique geometry | Same blueprint, different shapes |

## Data Model

### MachineBlueprint (RON asset)

**File**: `assets/machines.ron` (NEW)

```ron
MachineCatalog(
    blueprints: [
        // --- REWARD MACHINES ---
        ( name: "Treasure Vault",
          min_floor: 4, max_floor: 20,
          min_seclusion: 15, max_seclusion: 50,
          min_interior: 8, max_interior: 40,
          gate_type: Locked,
          interior_prep: Purge,
          monsters: [ (role: "melee_guard", behavior: Sentry) ],
          props: [ "chest", "chest" ],
          structures: [],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 10,
        ),

        // --- MONSTER ENCOUNTERS ---
        ( name: "Monster Den",
          min_floor: 3, max_floor: 18,
          min_seclusion: 20, max_seclusion: 80,
          min_interior: 15, max_interior: 60,
          gate_type: Open,
          interior_prep: Open,
          monsters: [
            (role: "brute", behavior: Sentry),
            (role: "any", behavior: Wander),
          ],
          props: [ "watchfire" ],
          structures: [],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 15,
        ),

        // --- SECRET REWARDS ---
        ( name: "Hidden Armory",
          min_floor: 3, max_floor: 20,
          min_seclusion: 10, max_seclusion: 30,
          min_interior: 5, max_interior: 20,
          gate_type: Hidden,
          interior_prep: Purge,
          monsters: [],
          props: [ "chest" ],
          structures: [],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 8,
        ),

        // --- ENVIRONMENTAL ---
        ( name: "Flooded Chamber",
          min_floor: 5, max_floor: 15,
          min_seclusion: 15, max_seclusion: 40,
          min_interior: 10, max_interior: 50,
          gate_type: Open,
          interior_prep: Purge,
          monsters: [],
          props: [ "chest" ],
          structures: [],
          fill_liquid: Some(ShallowWater),
          fill_decoration: None,
          frequency: 8,
        ),

        // --- THEMED ---
        ( name: "Fungal Grotto",
          min_floor: 6, max_floor: 18,
          min_seclusion: 15, max_seclusion: 50,
          min_interior: 10, max_interior: 40,
          gate_type: Open,
          interior_prep: None,
          monsters: [
            (role: "any", behavior: Wander),
            (role: "any", behavior: Roam(min: (0,0), max: (99,99))),
          ],
          props: [],
          structures: [],
          fill_liquid: None,
          fill_decoration: Some(Fungus),
          frequency: 6,
        ),

        ( name: "Bone Crypt",
          min_floor: 8, max_floor: 20,
          min_seclusion: 15, max_seclusion: 50,
          min_interior: 10, max_interior: 40,
          gate_type: Locked,
          interior_prep: Purge,
          monsters: [
            (role: "melee_guard", behavior: Sentry),
            (role: "melee_guard", behavior: Sentry),
          ],
          props: [ "chest" ],
          structures: [],
          fill_liquid: None,
          fill_decoration: Some(Bloodstain),
          frequency: 8,
        ),

        // --- CHALLENGE ---
        ( name: "Guardian Corridor",
          min_floor: 6, max_floor: 20,
          min_seclusion: 8, max_seclusion: 25,
          min_interior: 5, max_interior: 15,
          gate_type: Open,
          interior_prep: None,
          monsters: [ (role: "brute", behavior: Sentry) ],
          props: [ "chest" ],
          structures: [],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 12,
        ),

        // --- LATE GAME ---
        ( name: "Lava Vault",
          min_floor: 10, max_floor: 20,
          min_seclusion: 15, max_seclusion: 50,
          min_interior: 10, max_interior: 40,
          gate_type: Locked,
          interior_prep: Purge,
          monsters: [ (role: "melee_guard", behavior: Sentry) ],
          props: [ "chest", "chest" ],
          structures: [],
          fill_liquid: Some(Lava),
          fill_decoration: None,
          frequency: 5,
        ),
    ]
)
```

### Rust Types

```rust
#[derive(Deserialize, Debug, Clone)]
pub enum GateType {
    Open,       // Normal door at chokepoint
    Locked,     // LockedDoor — key placed elsewhere
    Hidden,     // HiddenDoor — discovered by search/perception
    Guardian,   // Monster placed at gate (no door change)
}

#[derive(Deserialize, Debug, Clone)]
pub enum InteriorPrep {
    None,       // Leave as-is
    Purge,      // Clear all terrain to plain floor
    Open,       // Remove isolated wall pillars (walls with 6+ floor neighbors)
}

#[derive(Deserialize, Debug, Clone)]
pub struct MachineBlueprint {
    pub name: String,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_seclusion: i32,
    pub max_seclusion: i32,
    pub min_interior: i32,      // minimum interior tile count
    pub max_interior: i32,      // maximum interior tile count
    pub gate_type: GateType,
    pub interior_prep: InteriorPrep,
    pub monsters: Vec<MachineMonster>,
    pub props: Vec<String>,
    pub structures: Vec<String>,
    pub fill_liquid: Option<LiquidType>,
    pub fill_decoration: Option<Decoration>,
    pub frequency: i32,         // raffle weight
}

#[derive(Deserialize, Debug, Clone)]
pub struct MachineMonster {
    pub role: String,
    pub behavior: MonsterBehavior,
}
```

## Builder: MachinePlacer

**File**: `src/map/builders/machine_placer.rs` (NEW)

**Pipeline position**: After PrefabPlacer, before ShrinePlacer

### Algorithm

```
1. Retrieve ChokeMap from BuilderMap.chokepoints (computed by HiddenDoorPlacer)
   If missing, compute it now.

2. Collect eligible blueprints for this depth.
   Weighted raffle by frequency field.

3. Collect candidate gate sites:
   - All chokepoint tiles from ChokeMap
   - Filter: choke_value in [blueprint.min_seclusion, blueprint.max_seclusion]
   - Filter: not in any existing machine_region or prefab exclusion zone
   - Shuffle candidates

4. For each selected blueprint (max machine_budget per floor, typically 1-2):
   a. Find a matching gate site from candidates
   b. Flood-fill from the gate (blocking the gate tile itself) to find the interior
      - Only fill passable tiles
      - Stop at map boundaries
   c. Validate interior size: min_interior <= interior.len() <= max_interior
   d. If no valid gate found, skip this blueprint

5. Apply interior_prep:
   - Purge: set all interior tiles to Floor terrain, remove liquid/decoration
   - Open: for each interior wall tile with 6+ floor neighbors, convert to Floor

6. Apply fill_liquid: set all interior Floor tiles to the specified liquid

7. Apply fill_decoration: set all interior Floor tiles' decoration

8. Place gate:
   - GateType::Open — leave existing door (or place one if none exists)
   - GateType::Locked — set terrain to LockedDoor, allocate key_id,
     place Key item 20-40 Dijkstra tiles away (never behind another lock)
   - GateType::Hidden — set terrain to HiddenDoor
   - GateType::Guardian — place a monster at the gate tile

9. Place monsters inside interior:
   - Use existing MonsterRoleTable for faction-based resolution
   - Place at random interior floor positions
   - Assign behavior from blueprint

10. Place props and structures inside interior:
    - Random interior floor positions
    - Prefer positions far from gate (rewards at the back)

11. Record interior bounding rect in machine_regions and decoration_exclusion_zones
```

### Flood-Fill Interior Detection

```rust
fn find_gated_interior(map: &Map, gate: Point) -> Vec<usize> {
    // Flood-fill from each neighbor of the gate that is passable,
    // treating the gate as impassable (blocking it).
    // Return the SMALLEST connected region found.
    // This is the "gated" region — the area behind the chokepoint.

    let mut smallest_region: Option<Vec<usize>> = None;

    for each cardinal neighbor of gate:
        if neighbor is passable:
            let region = flood_fill(map, neighbor, blocking: gate)
            if smallest_region.is_none() || region.len() < smallest_region.len():
                smallest_region = Some(region)

    smallest_region.unwrap_or_default()
}
```

## Lock & Key

### Map Generation

When `gate_type == Locked`:
1. Increment `BuilderMap.next_key_id`
2. Set gate tile terrain to `TerrainType::LockedDoor`
3. Store `(gate_point, key_id)` in `BuilderMap.locked_doors`
4. Compute Dijkstra distance from gate tile
5. Find a floor tile 20-40 distance away that is NOT behind any locked door
6. Store `(key_point, key_id)` in `BuilderMap.key_spawn_list`

### Runtime (Phase 7)

- Player bumps LockedDoor → check inventory for Key with matching key_id
- If found: consume key, convert LockedDoor → Door, log "You unlock the door"
- If not found: log "This door is locked. You need a key."

### Key Item

A new item kind or component:
```rust
#[derive(Component)]
pub struct KeyItem {
    pub key_id: u32,
}
```

Key items appear as a golden `*` in ASCII mode, with a name like "Iron Key" or "Vault Key".

## Save/Load

- `LockedDoor` terrain type already serializes
- Key items serialize via standard item save/load
- `key_id` stored on the `KeyItem` component — needs save/load support
- Machine regions don't need persistence (they're map-gen artifacts)

## BuilderMap Extensions

```rust
pub struct BuilderMap {
    // ... existing ...
    pub chokepoints: Option<ChokeMap>,
    pub decoration_exclusion_zones: Vec<Rect>,
    pub machine_regions: Vec<Rect>,
    pub locked_doors: Vec<(Point, u32)>,
    pub key_spawn_list: Vec<(Point, u32)>,
    pub next_key_id: u32,
}
```
