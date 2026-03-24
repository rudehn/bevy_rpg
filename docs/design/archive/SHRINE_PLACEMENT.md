# Phase 6: Shrine Placement

## Overview

Place progression-relevant shrines in secluded dungeon nooks using chokepoint analysis.
Shrines provide meaningful exploration rewards — healing, essence (progression currency),
risk/reward buffs — and tie into the boss corruption system.

## How Shrines Are Found

Shrines use the same ChokeMap topology as machines, but target SMALLER gated regions
(nooks and alcoves rather than full rooms). This puts shrines in places the player
has to deliberately explore rather than stumble upon.

## Builder: ShrinePlacer

**File**: `src/map/builders/shrine_placer.rs` (NEW)

**Pipeline position**: After MachinePlacer, before second DecorationPropagator pass

### Algorithm

```
1. Retrieve ChokeMap from BuilderMap.chokepoints

2. Collect candidate sites:
   - Chokepoint tiles with choke_value in [8, 40]
   - NOT overlapping any machine_region or decoration_exclusion_zone
   - NOT within 10 tiles of player start (shrines shouldn't be trivially close)

3. Score each candidate:
   score = choke_value + dijkstra_distance_from_start * 0.3
   Higher score = more secluded and more distant = better shrine site

4. Select top 2-3 candidates (don't over-saturate)

5. For each selected site:
   a. Flood-fill the gated region (same as machine interior detection)
   b. Validate interior is 3-25 tiles (small nook, not a full room)
   c. Find the center-most floor tile in the interior
   d. Pick shrine type based on depth
   e. Place shrine structure at center
   f. Optionally place 0-1 guardian monster near the gate
   g. Add interior rect to decoration_exclusion_zones
```

### Shrine Type Selection

```
depth 1-5:   100% Healing Spring
depth 6-10:  60% Healing Spring, 40% Essence Shrine
depth 11-15: 30% Healing Spring, 30% Essence Shrine, 40% Corrupted Shrine
depth 16-20: 20% Healing Spring, 30% Essence Shrine, 50% Corrupted Shrine
```

### Shrine Types

| Shrine | Structure Name | Effect | Depth |
|--------|---------------|--------|-------|
| Healing Spring | `Healing Spring` | Restores HP to full (one-time use) | 1+ |
| Essence Shrine | `Essence Shrine` | Grants essence (progression currency) | 6+ |
| Corrupted Shrine | `Corrupted Fountain` | Buff + curse (risk/reward) | 11+ |

These use the existing structure system. `Healing Spring` and `Corrupted Fountain`
already exist in `structures.ron`. `Essence Shrine` would need to be added.

### Guardian Placement

- 30% chance to place a guardian monster near the shrine gate
- Guardian role: `melee_guard` with `Sentry` behavior
- Makes the shrine a mini-encounter rather than free loot

## Interaction with Other Systems

- **Prefabs**: Shrines don't overlap prefab regions (exclusion zones)
- **Machines**: Shrines don't overlap machine regions
- **Decorations**: Shrine nooks are excluded from the second decoration pass
- **Boss system**: Corrupted Shrines could interact with the Tyrant's corruption
  mechanic (future integration)

## Save/Load

Shrines are structures — they use the existing structure spawn/save system.
No additional persistence needed.
