# Horde Spawning System

## Context

M7 expanded the bestiary to 25 monsters across 8 factions with floor ranges 1-26. Currently, each room spawns at most 1 monster (50% chance). Difficulty is flat — a floor-20 room with one Rat feels the same as floor 1. The horde system scales difficulty via group size: deeper/weaker monsters spawn in packs, while elite monsters remain solo. This is data-driven via `monster_spawns.ron`.

---

## Design

### Data Model

Add `min_group` and `max_group` fields to `MonsterSpawnInfo` in `src/assets/mod.rs`. Both default to 1 (solo spawn) for backward compatibility via `#[serde(default)]`.

When a room rolls a spawn, the spawner picks a group size in `[min_group, max_group]` and places that many of the same monster in adjacent walkable tiles using BFS cluster placement.

### Cluster Placement Algorithm

`find_cluster_points(origin, count, map, occupied) -> Vec<Point>`:
- BFS outward from `origin` using **cardinal directions only** (tight clumps, no diagonal spread)
- Returns up to `count` walkable, unoccupied tiles
- **Graceful degradation**: if a small room can only fit 2 tiles, a horde of 4 places 2 and skips the rest
- An `occupied: HashSet<usize>` tracks all tiles already claimed by any spawn on the floor (including the player start), preventing monsters from stacking across rooms

### Spawn Flow

1. Initialize `occupied` with the player start position
2. For each room (50% chance, unchanged):
   - Pick a random monster from depth-filtered `possible_spawns`
   - Roll group size: `rng.range(min_group, max_group + 1)`
   - Find initial walkable point via existing `get_walkable_room_point()`
   - Call `find_cluster_points()` to get positions for the full group
   - Push all `(Point, monster_name)` pairs to `build_data.spawn_list`
   - Mark all placed tiles in `occupied`

### What Doesn't Change

- `spawn_list` format `Vec<(Point, String)>` is preserved — downstream systems (entity spawning, floor caching, save/load) are unaffected
- 50% per-room spawn chance stays flat — horde sizes already scale density naturally; spawn chance can be tuned later as a separate knob
- A horde of 4 Rats is just 4 `(Point, "Rat")` entries in the spawn list and floor cache

---

## Group Sizes Per Monster

| Category | Monster | min | max | Rationale |
|----------|---------|-----|-----|-----------|
| Vermin | Rat | 2 | 4 | Swarm creature, trivial alone |
| Vermin | Giant Bat | 1 | 3 | Flocks |
| Vermin | Giant Spider | 1 | 2 | Tough with on-hit poison, keep small |
| Vermin | Plague Rat | 2 | 5 | Defining horde monster |
| Goblinoid | Goblin | 1 | 3 | Small raiding bands |
| Goblinoid | Goblin Archer | 1 | 2 | Ranged support |
| Goblinoid | Goblin Shaman | 1 | 1 | Leader, solo |
| Goblinoid | Goblin Warchief | 1 | 1 | Leader, solo |
| Undead | Skeleton | 1 | 3 | Classic skeleton group |
| Undead | Bone Archer | 1 | 2 | Ranged pair |
| Undead | Zombie | 1 | 3 | Shambling horde |
| Undead | Wraith | 1 | 1 | Elite, solo |
| Undead | Lich Apprentice | 1 | 1 | Caster, solo |
| Orcish | Orc | 1 | 3 | Disciplined squad |
| Orcish | Orc Berserker | 1 | 2 | Dangerous pair |
| Orcish | Orc Shaman | 1 | 1 | Caster, solo |
| Orcish | Orc Warlord | 1 | 1 | Leader, solo |
| Demonic | Imp | 1 | 2 | Occasional pair |
| Demonic | Hell Hound | 1 | 2 | Flanking pair |
| Demonic | Shadow Fiend | 1 | 1 | Elite, solo |
| Giant | Ogre | 1 | 1 | Elite brute, solo |
| Giant | Ogre Mage | 1 | 1 | Elite caster, solo |
| Giant | Troll | 1 | 1 | Elite regen tank, solo |
| Dark | Vampire | 1 | 1 | Elite, solo |
| Dark | Dark Knight | 1 | 1 | Elite, solo |

**Design philosophy**: Weak/swarm creatures (vermin, basic goblinoids, basic undead) get groups of 2-5. Casters and leaders are always solo. Elite monsters (giants, dark, wraith) are solo — their difficulty comes from individual power, not numbers.

---

## Files to Modify

| File | Change |
|------|--------|
| `src/assets/mod.rs` | Add `min_group`, `max_group` to `MonsterSpawnInfo` with serde defaults |
| `src/map/builders/monster_spawner.rs` | Add `find_cluster_points()` BFS helper, rewrite `spawn_monsters()` with group logic + occupied tracking |
| `assets/monster_spawns.ron` | Add group sizes to all 25 entries |

---

## Related: Squad System

Horde-spawned groups are linked by the [squad system](SQUAD_SYSTEM.md), which
adds shared alerting, leader death effects (scatter/enrage), and collective flee
decisions. Squad behavior is configured per spawn entry in `monster_spawns.ron`.

---

## Future Tuning Knobs

- **Spawn chance scaling**: Currently flat 50% per room. Could scale with depth (e.g., `40 + depth * 2`, capped at 80%) if density is too low after horde testing.
- **Mixed hordes**: Currently each room spawns one monster type. Could add mixed-faction rooms (e.g., Goblin Warchief + Goblin escorts) as a future enhancement.
- **Spawn weight**: Currently uniform random selection from eligible monsters. Could add per-entry weights for rarity control.
