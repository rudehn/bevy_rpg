# Encounters

## Overview

Encounters are structured gameplay experiences placed in the dungeon by the
builder pipeline. A single unified **machine system** handles all encounter
types — from simple guarded rooms to multi-layered goblin forts with sub-machines.

Three decoupled layers drive the system:
1. **Hordes** — named groups of monsters with variable counts and a single tag
2. **Horde spawn table** — which hordes appear on which floors (shared with the
   open-floor monster spawner)
3. **Machines** — topology-gated encounters that reference hordes by tag

All encounters appear on **all floors**. The dungeon should feel alive and
worth exploring from floor 1.

---

## Layer 1: Hordes

A horde is a named, reusable group of monsters with variable counts. Each horde
has exactly **one tag** that describes its role. Hordes have no floor range —
that lives in the spawn table.

```
hordes.ron:
  "rat_pack":
    tag: swarm
    monsters: [giant_rat x2-4]

  "goblin_patrol":
    tag: patrol
    monsters: [goblin x2-3]

  "goblin_squad":
    tag: guard
    monsters: [goblin x2-3, goblin_brute x1]

  "goblin_archers":
    tag: ranged
    monsters: [goblin_archer x1-2]

  "goblin_casters":
    tag: support
    monsters: [goblin_shaman x1, goblin_totem x0-1]

  "goblin_war_party":
    tag: elite
    monsters: [goblin_warchief x1, goblin_brute x1-2, goblin_firebomber x0-1]

  "spider_nest":
    tag: ambush
    monsters: [giant_spider x1-2]

  "wolf_pack":
    tag: patrol
    monsters: [wolf x2-3]

  "bat_colony":
    tag: swarm
    monsters: [giant_bat x2-4]

  "salamander_pair":
    tag: threat
    monsters: [fire_salamander x1-2]

  "jelly_blob":
    tag: threat
    monsters: [jelly x1]

  "bloat_cluster":
    tag: hazard
    monsters: [bloat x1-2]

  "troll_den":
    tag: brute
    monsters: [cave_troll x1]

  "sentinel_post":
    tag: guardian
    monsters: [stone_sentinel x1]

  "dragon_guard":
    tag: elite
    monsters: [dragon_whelp x1]

  "young_dragon_lair":
    tag: apex
    monsters: [young_dragon x1]
```

## Layer 2: Horde Spawn Table

Controls which hordes can appear on which floors. Used by **both** the open-floor
monster spawner and the machine system. One source of truth for "what appears
where."

```
horde_spawns.ron:
  { horde: "rat_pack",          min_floor: 1,  max_floor: 4  }
  { horde: "bat_colony",        min_floor: 1,  max_floor: 3  }
  { horde: "goblin_patrol",     min_floor: 1,  max_floor: 5  }
  { horde: "goblin_archers",    min_floor: 2,  max_floor: 7  }
  { horde: "wolf_pack",         min_floor: 2,  max_floor: 5  }
  { horde: "salamander_pair",   min_floor: 2,  max_floor: 5  }
  { horde: "bloat_cluster",     min_floor: 1,  max_floor: 5  }
  { horde: "goblin_squad",      min_floor: 3,  max_floor: 7  }
  { horde: "spider_nest",       min_floor: 3,  max_floor: 6  }
  { horde: "jelly_blob",        min_floor: 2,  max_floor: 7  }
  { horde: "goblin_casters",    min_floor: 4,  max_floor: 9  }
  { horde: "troll_den",         min_floor: 4,  max_floor: 8  }
  { horde: "sentinel_post",     min_floor: 2,  max_floor: 10 }
  { horde: "dragon_guard",      min_floor: 5,  max_floor: 9  }
  { horde: "goblin_war_party",  min_floor: 7,  max_floor: 9  }
  { horde: "young_dragon_lair", min_floor: 8,  max_floor: 9  }
```

## Layer 3: Machines

Machines find chokepoints in the dungeon's topology, gate them, and populate
the interior using hordes resolved by **tag**. Machines can contain
**sub-machines** for nested encounters.

### Tag Resolution

At placement time, for each horde slot in a machine:
1. Read the slot's required tag (e.g., `guard`)
2. Filter horde_spawns for the current floor depth
3. Filter by tag match
4. Pick one randomly from eligible hordes
5. Log the resolution: `"Monster Den floor 8: slot 'guard' -> goblin_squad"`

**All slots are required.** If any slot has no eligible horde on this floor,
the machine does not place. The same horde can be picked for multiple slots
(doubling up is fine).

### Placement Constraints

- **Seclusion** — Dijkstra distance from player start to gate tile
- **Interior size** — Tile count of the region behind the gate

### Gate Types

| Gate | Description |
|------|-------------|
| Open | Normal door at the chokepoint |
| Locked | Locked door — key placed elsewhere on the floor |
| Hidden | Hidden door — discovered by search/proximity |
| Guardian | Monster placed at the gate (no door change) |

### Interior Preparation

| Prep | Description |
|------|-------------|
| None | Leave the region as-is |
| Purge | Clear all terrain to plain floor |
| Open | Remove isolated wall pillars (walls with 6+ floor neighbors) |

### Placement Hints

| Hint | Meaning |
|------|---------|
| `AtGate` | At or adjacent to the gate tile |
| `NearGate` | Within 3 tiles of the gate |
| `Center` | Room centroid |
| `DeepInterior` | Farthest walkable point from gate |
| `AlongWalls` | Adjacent to a wall tile |
| `Random` | Any interior floor tile |

Hordes with the same placement hint are clustered together.

### Sub-Machines

A machine can reference sub-machines that place **inside the parent's interior**.
The sub-machine finds a chokepoint within the parent region and does its own
gating + population.

- Sub-machines that can't place (no gatable sub-region, floor range doesn't
  match, interior too small) are **silently skipped**. The parent machine
  remains valid.
- Sub-machines enable nested encounters: a goblin fort with a locked treasury
  inside, or a monster den with a hidden back room.

### Machine Budget

**2-4 machines per floor** baseline. Deeper floors get more:

| Floors | Machine Budget |
|--------|---------------|
| 1-3 | 2-3 |
| 4-6 | 2-4 |
| 7-9 | 3-5 |
| 10 | Special (Tyrant floor) |

---

## Machine Blueprints

### Goblin Encounters

**Goblin Scuffle**
```
floors: 1-3, gate: Open, seclusion: 5-999, interior: 8-20
hordes:
  - tag: patrol (NearGate, guard)
items: [chest (DeepInterior)]
```

**Goblin Camp**
```
floors: 2-5, gate: Open, seclusion: 10-999, interior: 12-35
hordes:
  - tag: guard (NearGate, guard)
  - tag: ranged (AlongWalls, guard)
props: [watchfire (Center), barricade (NearGate)]
items: [chest (DeepInterior)]
```

**Goblin Outpost**
```
floors: 4-7, gate: Open, seclusion: 15-999, interior: 20-50, prep: Open
hordes:
  - tag: guard (NearGate, guard)
  - tag: ranged (AlongWalls, guard)
  - tag: support (Center, guard)
props: [watchfire (Center)]
items: [chest (DeepInterior)]
```

**Goblin Fort**
```
floors: 7-9, gate: Locked ("Fort Key"), seclusion: 20-999, interior: 35-70, prep: Open
hordes:
  - tag: elite (Center, guard)
  - tag: guard (NearGate, guard)
  - tag: ranged (AlongWalls, guard)
  - tag: support (DeepInterior, guard)
props: [watchfire (Center), barricade (NearGate), barricade (NearGate)]
items: [chest (DeepInterior), chest (DeepInterior)]
sub_machines: [Inner Treasury (DeepInterior)]
```

### Reward Machines

**Treasure Vault**
```
floors: 2-10, gate: Locked ("Vault Key"), seclusion: 30-999, interior: 8-40, prep: Purge
hordes:
  - tag: guard (Center, guard)
items: [chest (DeepInterior), chest (DeepInterior)]
```

**Hidden Armory**
```
floors: 2-10, gate: Hidden, seclusion: 5-999, interior: 5-20, prep: Purge
hordes: (none — no tag requirements, always places)
items: [chest (DeepInterior)]
```

**Inner Treasury** *(sub-machine only)*
```
floors: 5-10, gate: Locked ("Treasury Key"), seclusion: 0-999, interior: 4-15, prep: Purge
hordes: (none)
items: [chest (DeepInterior)]
```

**Guardian Corridor**
```
floors: 3-10, gate: Open, seclusion: 8-999, interior: 5-15
hordes:
  - tag: brute (AtGate, guard)
items: [chest (DeepInterior)]
```

### Environmental Machines

**Flooded Chamber**
```
floors: 3-8, gate: Open, seclusion: 15-999, interior: 10-50, prep: Purge
fill: ShallowWater
hordes: (none)
items: [chest (DeepInterior)]
```

**Fungal Grotto**
```
floors: 3-9, gate: Hidden, seclusion: 15-999, interior: 10-40
fill_decoration: Fungus
hordes:
  - tag: threat (Random, roam)
```

**Bone Crypt**
```
floors: 4-10, gate: Locked ("Crypt Key"), seclusion: 20-999, interior: 10-40, prep: Purge
fill_decoration: Bloodstain
hordes:
  - tag: guard (NearGate, guard)
items: [chest (DeepInterior)]
```

**Lava Vault**
```
floors: 5-10, gate: Locked ("Molten Key"), seclusion: 20-999, interior: 10-40, prep: Purge
fill: Lava
hordes:
  - tag: brute (NearGate, guard)
items: [chest (DeepInterior), chest (DeepInterior)]
```

### Hazard Machines

**Monster Den**
```
floors: 2-9, gate: Open, seclusion: 10-999, interior: 15-60
hordes:
  - tag: threat (Random, roam)
  - tag: swarm (Random, roam)
props: [watchfire (Center)]
items: [chest (DeepInterior)]
```

**Ambush Room**
```
floors: 3-8, gate: Hidden, seclusion: 10-999, interior: 8-25
hordes:
  - tag: ambush (AlongWalls, guard)
items: [chest (Center)]
```

---

## Props

| Prop | Light | Blocking | Notes |
|------|-------|----------|-------|
| Candle | Yes | No | Standard light source |
| Watchfire | Yes | No | Camp centerpiece, warm orange glow |
| Barricade | No | Yes | Tactical obstacle, blocks movement and projectiles. 10 HP, 0 armor. Destructible. |

## Guard AI

Guards are monsters with a **home position** that patrol near their post and
return after chasing the player.

- **Guarding mode:** Random walk within 3 tiles of home position
- **Hunting mode:** Chase player (standard AI). If player is lost, return to
  home position instead of wandering randomly.
- **Squad integration:** Guards wake from Guarding to Hunting via shared
  alerting. Each horde placed by a machine is squad-linked automatically.

## Lock & Key

Machines with `Locked` gates create a lock-and-key puzzle.

### Key Placement

1. Gate tile becomes `LockedDoor` terrain
2. Key placed via widening Dijkstra search:
   - First: 20-40 steps from gate
   - Widen: 10-60 steps
   - Fail: abandon machine, revert gate to normal door
3. Key is never placed inside another locked area

### Player Interaction

- Bumping a locked door checks inventory for a matching key
- Found: key consumed, door opens, costs one turn
- Not found: "This door is locked. You need a key."
- Keys are regular inventory items (pickable, droppable, not equippable)

## Shrines

Shrines use the machine system with small-region constraints:
- Targets gated regions of **3-25 tiles** (nooks and alcoves)
- Not within 10 tiles of player start
- **3 shrines per floor** (stat + spell shrines share budget)
- 30% chance for a guardian near the shrine gate

**Stat Shrines** — cost essence, grant permanent bonuses. See TYRANT.md.
**Spell Shrines** — cost essence, teach a visible spell. See SPELLS.md.

## Corruption Sites

Corruption Sites are deferred. They will be revisited once the core machine
system is implemented and tested. The current concept (3 per run, Aspect
Champions, Corruption Altars) is documented in TYRANT.md but not finalized.

## Design Notes

- **Three decoupled layers.** Hordes define monster groups. The spawn table
  controls floor eligibility. Machines compose hordes by tag. Each layer can
  be edited independently.
- **Tag resolution is logged.** Every machine placement logs which hordes were
  resolved for each slot. Debugging is: check the log, check horde_spawns for
  this floor, check which hordes have this tag.
- **Sub-machines enable depth.** A goblin fort can contain a locked treasury.
  A monster den can have a hidden back room. Composition replaces complexity.
- **All slots required.** If a machine can't fill all its tag slots on this
  floor, it doesn't place. This guarantees encounters are always complete —
  no half-populated rooms.
- **Coverage validation.** Use `tools/encounter_coverage.py` to verify all
  machines have eligible hordes across their floor ranges.

## Open Questions

1. **Sub-machine depth limit** — Should sub-machines be allowed to contain
   their own sub-machines? Or limit nesting to 1 level?
2. **Machine weight by floor** — Should some blueprints be more likely to
   appear on certain floors (weighted frequency)?
3. **Mixed-faction narrative** — When a machine spawns goblins + dragon,
   should there be any in-world justification, or just let it happen?
