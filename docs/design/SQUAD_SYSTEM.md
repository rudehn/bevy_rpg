# Squad System — Coordinated Group AI

## Context

The [horde spawning system](HORDE_SYSTEM.md) places monsters in groups, but those
groups had no coordination — each member acted independently. The squad system
links group members so they **feel** like a coordinated unit: shared awareness,
morale effects when the leader falls, and collective flee decisions.

---

## Design Philosophy

The squad system optimizes for **emergent-feeling behavior** with minimal
complexity. Four simple rules (leader leashing, shared alerting, leader death
effects, collective flee) produce tactics that feel intelligent without requiring
formation AI, communication ranges, or centralized planners.

### What squads DO

1. **Leader leashing** — Non-leader squad members stay near their leader. If a
   follower is more than 4 tiles from the leader, it pathfinds toward the leader
   instead of its normal target (player or random wander). This keeps squads
   moving through corridors as a group. When they're within leash range, each
   member pathfinds independently — so they still fan out naturally in open rooms.

2. **Shared alerting** — When any squad member spots the player or takes damage,
   the entire squad transitions from Asleep to Hunting. You cannot silently pick
   off a sentry without alerting the whole group.

3. **Leader death effects** — Each squad has one leader (first spawned member).
   When the leader dies:
   - **Scatter**: Remaining members lose their target and wander aimlessly.
   - **Enrage**: Remaining members gain a temporary damage bonus (+50% for 10 turns).
   - **Nothing**: No special effect.
   A new leader is promoted from survivors.

4. **Collective flee** — Cowardly squad members check the *group's* total HP
   ratio (not just their own) against the squad's `flee_threshold`. When the
   group is hurt enough, they all flee at once.

### What squads DON'T do (deliberate omissions)

- **No dynamic joining** — Solo monsters never join an existing squad. Groups are
  defined at spawn time only.
- **No group merging** — Two rat packs in adjacent rooms stay distinct. This
  creates better tactics (alert pack A without alerting pack B).
- **No communication range** — All squad members alert regardless of distance on
  the same floor. Simpler and prevents the exploit of picking off distant sentries.

---

## Data Model

All squad state is stored as ECS components on individual entities. There is no
centralized squad resource — all information is derived by querying entities with
matching `SquadId`. This avoids sync bugs with despawned entities and keeps
save/load trivial.

### Components (`src/game/squad.rs`)

| Component | Description |
|-----------|-------------|
| `SquadId(u64)` | Links all members of a squad. Solo monsters have no `SquadId`. |
| `SquadLeader` | Marker on the current leader. Triggers death effects when killed. |
| `SquadConfig` | Per-entity config: `on_leader_death` and `flee_threshold`. All members of a squad carry the same config. |

### Resource

| Resource | Description |
|----------|-------------|
| `SquadIdCounter(u64)` | Global counter for generating unique `SquadId` values. Persisted across save/load and floor transitions. |

### Enums

```rust
enum LeaderDeathBehavior {
    Nothing,   // No special effect (default)
    Scatter,   // Members lose target, wander
    Enrage,    // Members gain temporary damage bonus
}
```

---

## Configuration

Squad behavior is configured **per spawn entry** in `monster_spawns.ron`, not
per monster definition. The same monster type might be solo in one entry and part
of a squad in another.

### Spawn entry fields

| Field | Default | Description |
|-------|---------|-------------|
| `on_leader_death` | `""` (nothing) | `"scatter"` or `"enrage"` |
| `flee_threshold` | `0.5` | Collective HP ratio below which cowardly members flee (0.0–1.0) |

### Examples

```ron
// Solo rat on floor 1 — no squad
(monster: "Rat", min_floor: 1, max_floor: 1, min_group: 1, max_group: 1),

// Rat pack that scatters when leader dies
(monster: "Rat", min_floor: 2, max_floor: 5, min_group: 2, max_group: 4,
 on_leader_death: "scatter", flee_threshold: 0.4),

// Goblin war party (mixed group) that enrages
(group: [
    (monster: "Goblin",        min_count: 2, max_count: 2),
    (monster: "Goblin Archer", min_count: 2, max_count: 2),
], min_floor: 4, max_floor: 8, on_leader_death: "enrage"),
```

### Squad assignment rules

- Groups with effective size > 1 (either `min_group > 1` or a `group` list) get
  a `SquadId` assigned at spawn time.
- Solo monsters (`min_group = max_group = 1`, no `group` list) get no squad
  components.
- Mixed groups (via `group` list) always get a squad, even if only one member
  rolls.

---

## Systems

### System ordering

```
fov_update_system
  → squad_alert_system          (wakes squads when any member sees player)
    → monster_ai_dispatch       (individual AI runs with updated modes)

CombatDamageSet
  → squad_damage_alert_system   (wakes squads when any member takes damage)
  → squad_leader_death_system   (scatter/enrage on leader kill)
    → death_system
```

### `squad_alert_system`
- **When**: After `fov_update_system`, during `InGameState::Running`
- **What**: Two-pass approach. Pass 1: collect which squads have a member with
  the player in their viewshed. Pass 2: transition all Asleep members of alerted
  squads to Hunting.
- **Scope**: Only triggers Asleep→Hunting. Ongoing position tracking remains
  per-individual.

### `squad_damage_alert_system`
- **When**: After `CombatDamageSet`, during `InGameState::Running`
- **What**: When a squad member takes damage (`Changed<Health>`), alert the
  entire squad.

### `squad_leader_death_system`
- **When**: After `CombatDamageSet`, before `death_system`
- **What**: Checks if any `SquadLeader` entity has HP ≤ 0. Applies
  `on_leader_death` effect (scatter or enrage) to remaining members. Promotes a
  new leader.

### Leader leashing (in `MonsterAI::execute()`)
- **Where**: `src/game/ai.rs`, step 2.8 (before pathfinding)
- **What**: Non-leader squad members check their distance to the leader. If
  farther than 4 tiles (`SQUAD_LEASH_RANGE`), the movement target is overridden
  to the leader's position instead of the player (Hunting) or random direction
  (Wandering). Once within range, normal AI resumes.
- **Edge cases**: If the leader is dead or missing, no leash target is found and
  the follower behaves independently.

### Collective flee (in `MonsterAI::execute()`)
- **Where**: `src/game/ai.rs`, cowardly flee block
- **What**: For cowardly monsters with a `SquadId`, sums all squad members'
  current/max HP via `compute_squad_hp()`. If the collective ratio is below
  `flee_threshold`, the monster flees. Solo cowardly monsters still use the
  individual 50% HP check.

---

## Persistence

Squad data is fully persisted across save/load and floor transitions.

### Save file (`GameSaveData`)
- `squad_id_counter: u64` — restored as `SquadIdCounter` resource
- `MonsterEntry` includes `squad_id: Option<u64>`, `is_leader: bool`,
  `squad_config: Option<SquadConfig>`

### Floor cache (`CachedFloor` / `CachedFloorSave`)
- `CachedMonster` / `CachedMonsterSave` structs include the same squad fields
- Squad membership is preserved when ascending/descending between floors

---

## Edge Cases

- **Squad of one**: If all but one member die, the survivor keeps its `SquadId`
  and `SquadConfig`. The collective flee threshold applies to just their HP.
- **Leader dies with no survivors**: The death effect fires but finds no members
  to affect. No new leader is promoted. No error.
- **Floor transitions**: `snapshot_floor` captures squad components.
  `CachedMonster` preserves `squad_id`, `is_leader`, and `squad_config`. When
  the floor is restored, squad components are reattached.
- **Save compatibility**: All squad fields use `#[serde(default)]`, so saves
  from before the squad system load without error (monsters have no squads).
