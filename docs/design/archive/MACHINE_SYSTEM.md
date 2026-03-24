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

## Seclusion and Interior Size

Each blueprint has two independent placement axes:

- **Seclusion** (`min_seclusion`/`max_seclusion`) — Dijkstra distance from the player
  start position to the gate tile. How deep in the dungeon the machine appears. A Treasure
  Vault requires `min_seclusion: 30` so it's never placed right next to the player start.

- **Interior size** (`min_interior`/`max_interior`) — tile count of the region behind the
  gate. How large the machine's playspace is. Checked after flood-fill to ensure the region
  is worth populating.

Both checks must pass for a gate candidate to qualify.

## Multi-Exit Regions

Machines gate the **primary chokepoint**, not all access to a region. A machine's interior
can have secondary exits — this is intentional. The BrogueLikeBuilder deliberately creates
loops in the dungeon graph, so a region may be reachable by two paths. This creates a
"find the key vs. find the other way" routing decision, consistent with Brogue's design.

The MachinePlacer does not reject multi-exit regions. The lock creates routing tension even
when an alternate route exists.

## Data Model

### MachineBlueprint (RON asset)

**File**: `assets/machines.ron`

```ron
MachineCatalog(
    blueprints: [
        // --- REWARD MACHINES ---
        ( name: "Treasure Vault",
          min_floor: 4, max_floor: 20,
          min_seclusion: 30, max_seclusion: 999,
          min_interior: 8, max_interior: 40,
          gate_type: Locked,
          key_name: "Vault Key",
          interior_prep: Purge,
          monsters: [ (role: "melee_guard", behavior: Sentry) ],
          props: [ "chest", "chest" ],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 10,
        ),

        // --- MONSTER ENCOUNTERS ---
        ( name: "Monster Den",
          min_floor: 3, max_floor: 18,
          min_seclusion: 10, max_seclusion: 999,
          min_interior: 15, max_interior: 60,
          gate_type: Open,
          key_name: "",
          interior_prep: Open,
          monsters: [
            (role: "brute", behavior: Sentry),
            (role: "any", behavior: Wander),
          ],
          props: [ "watchfire" ],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 15,
        ),

        // --- SECRET REWARDS ---
        ( name: "Hidden Armory",
          min_floor: 3, max_floor: 20,
          min_seclusion: 5, max_seclusion: 999,
          min_interior: 5, max_interior: 20,
          gate_type: Hidden,
          key_name: "",
          interior_prep: Purge,
          monsters: [],
          props: [ "chest" ],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 8,
        ),

        // --- ENVIRONMENTAL ---
        ( name: "Flooded Chamber",
          min_floor: 5, max_floor: 15,
          min_seclusion: 15, max_seclusion: 999,
          min_interior: 10, max_interior: 50,
          gate_type: Open,
          key_name: "",
          interior_prep: Purge,
          monsters: [],
          props: [ "chest" ],
          fill_liquid: Some(ShallowWater),
          fill_decoration: None,
          frequency: 8,
        ),

        // --- THEMED ---
        ( name: "Fungal Grotto",
          min_floor: 6, max_floor: 18,
          min_seclusion: 15, max_seclusion: 999,
          min_interior: 10, max_interior: 40,
          gate_type: Hidden,
          key_name: "",
          interior_prep: None,
          monsters: [
            (role: "any", behavior: Roam),
            (role: "any", behavior: Roam),
          ],
          props: [],
          fill_liquid: None,
          fill_decoration: Some(Fungus),
          frequency: 6,
        ),

        ( name: "Bone Crypt",
          min_floor: 8, max_floor: 20,
          min_seclusion: 20, max_seclusion: 999,
          min_interior: 10, max_interior: 40,
          gate_type: Locked,
          key_name: "Crypt Key",
          interior_prep: Purge,
          monsters: [
            (role: "melee_guard", behavior: Sentry),
            (role: "melee_guard", behavior: Sentry),
          ],
          props: [ "chest" ],
          fill_liquid: None,
          fill_decoration: Some(Bloodstain),
          frequency: 8,
        ),

        // --- CHALLENGE ---
        ( name: "Guardian Corridor",
          min_floor: 6, max_floor: 20,
          min_seclusion: 8, max_seclusion: 999,
          min_interior: 5, max_interior: 15,
          gate_type: Open,
          key_name: "",
          interior_prep: None,
          monsters: [ (role: "brute", behavior: Sentry) ],
          props: [ "chest" ],
          fill_liquid: None,
          fill_decoration: None,
          frequency: 12,
        ),

        // --- LATE GAME ---
        ( name: "Lava Vault",
          min_floor: 10, max_floor: 20,
          min_seclusion: 20, max_seclusion: 999,
          min_interior: 10, max_interior: 40,
          gate_type: Locked,
          key_name: "Molten Key",
          interior_prep: Purge,
          monsters: [ (role: "melee_guard", behavior: Sentry) ],
          props: [ "chest", "chest" ],
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
pub enum MachineBehavior {
    Sentry,     // Stands at spawn point
    Wander,     // Free random walk (may leave the machine)
    Roam,       // Bounded walk — clamped to machine interior bounding box at spawn time
}

#[derive(Deserialize, Debug, Clone)]
pub struct MachineBlueprint {
    pub name: String,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_seclusion: i32,     // min Dijkstra distance from player start to gate
    pub max_seclusion: i32,     // max Dijkstra distance from player start to gate
    pub min_interior: i32,      // minimum interior tile count
    pub max_interior: i32,      // maximum interior tile count
    pub gate_type: GateType,
    pub key_name: String,       // display name for the key item; ignored if gate_type != Locked
    pub interior_prep: InteriorPrep,
    pub monsters: Vec<MachineMonster>,
    pub props: Vec<String>,
    pub fill_liquid: Option<LiquidType>,
    pub fill_decoration: Option<Decoration>,
    pub frequency: i32,         // raffle weight
}

#[derive(Deserialize, Debug, Clone)]
pub struct MachineMonster {
    pub role: String,
    pub behavior: MachineBehavior,
}
```

Note: `MachineBehavior::Roam` has no coordinates in the blueprint. The MachinePlacer
computes the interior bounding box and sets the Roam bounds at spawn time.

## Builder: MachinePlacer

**File**: `src/map/builders/machine_placer.rs` (NEW)

**Pipeline position**: After PrefabPlacer, before CandleSpawner

### Algorithm

```
1. Retrieve or compute ChokeMap:
   - If BuilderMap.chokepoints is None, call ChokeMap::generate(&map) and store it.

2. Compute Dijkstra distance map from BuilderMap.starting_position.
   (Used for seclusion filtering.)

3. Collect eligible blueprints for this depth.
   Weighted raffle by frequency field.

4. Collect candidate gate sites:
   - All chokepoint tiles from ChokeMap
   - Filter: dijkstra_dist[gate] in [blueprint.min_seclusion, blueprint.max_seclusion]
   - Filter: not in any existing machine_region or prefab exclusion zone
   - Shuffle candidates

5. For each selected blueprint (machine_budget per floor = 1-2, scaled by depth):
   a. Find a matching gate site from candidates
   b. Flood-fill from each passable neighbor of the gate (treating gate as blocked)
      to find the interior — return the SMALLEST resulting Vec<usize>
   c. Validate: min_interior <= interior.len() <= max_interior
   d. If no valid gate found, skip this blueprint

6. Apply interior_prep:
   - Purge: set all interior tiles to Floor terrain, remove liquid/decoration
   - Open: for each interior wall tile with 6+ floor neighbors, convert to Floor
   - None: leave as-is

7. Apply fill_liquid: set all interior Floor tiles to the specified liquid

8. Apply fill_decoration: set all interior Floor tiles' decoration

9. Place gate:
   - GateType::Open — leave existing door (or place Door if none exists)
   - GateType::Locked — set terrain to LockedDoor, allocate key_id,
     place Key item using widening search: try Dijkstra distance 20-40 first,
     then 10-60, then hard skip (abandon this machine)
   - GateType::Hidden — set terrain to HiddenDoor
   - GateType::Guardian — place a monster at the gate tile

10. Place monsters inside interior:
    - Use existing MonsterRoleTable for faction-based resolution
    - Place at random interior floor positions
    - Assign behavior from blueprint:
      - Sentry: no PatrolRoute (stands at spawn)
      - Wander: no PatrolRoute (free random walk)
      - Roam: PatrolRoute::area_roam(interior_bbox.min, interior_bbox.max)
        (stays within the machine's interior bounding box)

11. Place props inside interior:
    - Random interior floor positions
    - Prefer positions far from gate (rewards at the back)

12. Record interior bounding rect in BuilderMap.machine_regions
    and BuilderMap.decoration_exclusion_zones
```

### Interior Detection

```rust
fn find_gated_interior(map: &Map, gate: Point) -> Vec<usize> {
    // Flood-fill from each passable neighbor of the gate,
    // treating the gate as impassable.
    // Return the SMALLEST connected region found.

    let mut smallest_region: Option<Vec<usize>> = None;

    for each cardinal neighbor of gate:
        if neighbor is passable:
            let tiles = flood_fill_tiles_with_block(map, neighbor, gate)
            if smallest_region.is_none() || tiles.len() < smallest_region.len():
                smallest_region = Some(tiles)

    smallest_region.unwrap_or_default()
}
```

## Lock & Key

### Map Generation

When `gate_type == Locked`:
1. Increment `BuilderMap.next_key_id`
2. Set gate tile terrain to `TerrainType::LockedDoor`
3. Compute Dijkstra distance from gate tile
4. Key placement uses widening search:
   - First attempt: find a floor tile 20–40 Dijkstra steps from gate,
     NOT behind any existing locked door or machine gate
   - If none found: widen to 10–60 steps
   - If still none: hard skip — abandon the machine, revert gate to Door
5. Store `(gate_point, key_id)` in `BuilderMap.locked_doors`
6. Store `(key_point, key_id)` in `BuilderMap.key_spawn_list`

Key item name comes from `blueprint.key_name` (e.g., "Vault Key", "Crypt Key").

### Runtime (Phase 7)

- Player bumps LockedDoor → check inventory for Key with matching `key_id`
- If found: consume key, convert LockedDoor → Door (OpenDoor on pass-through), log message
- If not found: log "This door is locked."

### Key Item

```rust
#[derive(Component)]
pub struct KeyItem {
    pub key_id: u32,
}
```

Key items appear as a golden `*` in ASCII mode.

## Save/Load

- `LockedDoor` terrain type already serializes
- Key items serialize via standard item save/load
- `key_id` on `KeyItem` component needs save/load support (add to save checklist)
- Machine regions don't need persistence (map-gen artifacts, rebuilt on floor restore)

## BuilderMap Extensions

```rust
pub struct BuilderMap {
    // ... existing ...
    pub chokepoints: Option<ChokeMap>,
    pub machine_regions: Vec<Rect>,
    pub locked_doors: Vec<(Point, u32)>,
    pub key_spawn_list: Vec<(Point, u32)>,
    pub next_key_id: u32,
}
```

## Open Questions

- Should `GateType::Guardian` place the monster before or after interior population?
  (Currently: at the gate tile, separately from interior monsters)
- Should the machine budget scale with floor depth or stay flat at 1–2?
- Should Flooded Chamber require the player to wade through water to reach the chest,
  or should the chest be placed on an island? (Current: random interior position, likely wet)
