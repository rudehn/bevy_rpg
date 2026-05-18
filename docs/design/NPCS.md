# NPCs

Peaceful townsfolk who wander the hub map.

## Design philosophy

NPCs are mechanically just **monsters with a non-hostile faction**.
They reuse the entire existing monster pipeline — `MonsterAsset`,
`MonsterAI`, `Faction`, `Health`, `Position`, save snapshotting, ASCII
rendering, turn slot, `idle_movement` patrol routes. The only things
that distinguish them from monsters:

- A new `Townsfolk` faction with **Allied** relation to `Player` and
  **Neutral** to every monster faction. The faction matrix is the
  single source of truth for "will this thing attack me?".
- A small guard in `update_mode` ([src/game/ai.rs](../../src/game/ai.rs))
  gating the `Asleep`/`Idle → Hunting` transition on the faction
  relation being **Hostile**. Non-Hostile actors stay where they are
  even when the player walks past them.
- A separate placement RON ([assets/town_npcs.ron](../../assets/town_npcs.ron))
  parsed by the town-side `TownNpcBuilder` that decides *where* on
  the map each NPC goes and what their roaming bounds are. The NPC
  asset itself has no notion of "pier" or "building interior" — it
  just declares stats, glyph, AI tuning.

This deliberately mixes NPCs into `monsters.ron` rather than creating
a parallel `npcs.ron` asset type. Pro: zero new spawn code path,
faction-matrix-aware bump behaviour, save/load works for free.
Con: the line between "monster" and "NPC" is encoded only in the
`faction:` field. The boundary feels right while NPCs are
non-interactive — when dialogue / vendor systems ship, we may want
to split.

## Data model

### Authoring

```ron
// assets/monsters.ron
"Drunken Sailor": (
    name: "Drunken Sailor",
    vision: 6,
    damage: "1d2",        // they CAN fight back if attacked
    base_hp: 12,
    ai: Fsm(
        flee_at_hp_percent: 0.0,
        erratic_chance: 0.8,
        chase_leash: 0,
        kites: false,
    ),
    faction: "Townsfolk", // <-- this is what makes them peaceful
    species: Humanoid,
    movement_delay: 1.2,
    ascii_char: "d",
    ascii_fg: "#A88040",
),
```

```ron
// assets/town_npcs.ron
(
    spawns: [
        ( npc: "Drunken Sailor", count: 3, placement: AnywhereInTown ),
    ],
)
```

```ron
// assets/factions.ron — Townsfolk relations
( a: "Player",   b: "Townsfolk", relation: Allied ),
( a: "Townsfolk", b: "Monster",  relation: Neutral ),
// ... + Neutral against each named monster faction
```

### Runtime

- Each NPC is an ECS entity carrying the same components a monster
  carries — `Monster`, `MonsterAI`, `Position`, `Faction`,
  `Collider`, `Health`, `StatusEffects`, `MovementMode`, `SpeedStats`,
  plus a `PatrolRoute` matching their placement strategy.
- `idle_movement` ([src/game/ai.rs](../../src/game/ai.rs)) reads
  `PatrolRoute` every turn an NPC is in `MonsterAIMode::Idle` and
  picks a random adjacent walkable tile within the patrol bounds.

### Placement strategies

Implemented in [src/map/builders/town_npcs.rs](../../src/map/builders/town_npcs.rs):

| Strategy | Roam bounds | Notes |
|---|---|---|
| `AnywhereInTown` | Full town land box (east of `WATER_EAST_EDGE`, minus border wall) | Drunks |
| `Pier { index: Option<u8> }` | Tight box around the chosen pier tile | **Planned** — fishermen |
| `BuildingInterior(role)` | Sentry on a tile inside that building | **Planned** — vendors |

## Faction model

`resolve_bump` in [src/game/actions.rs](../../src/game/actions.rs)
already consults `FactionMatrix.is_hostile_to`:

- Player walks into a Townsfolk tile → relation is `Allied` → not
  hostile → falls through to `target_has_collider` check →
  `BumpResult::BlockedByCollider`. **No melee swing, no damage.** The
  player just can't walk through them.
- A Townsfolk NPC could in principle attack the player back if
  attacked, since they carry `damage: "1d2"`. **Phase 1 ships them
  killable** — bash them down and they die. They don't get aggro on
  the player automatically (faction is `Allied`), so if the player
  swings via an explicit attack intent, the NPC takes the hit and
  may die without retaliating. This is the simplest behaviour;
  Phase ≥2 could add a "Townsfolk turn hostile on attack" rule.

## AI loop

```text
Asleep / Idle → Hunting transition (src/game/ai.rs::update_mode):
  is_player_visible && faction_matrix.is_hostile_to(npc, "Player")
                                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^
                                       NPCs are Allied → false
                                       → stay in Idle

Idle (idle_movement reads PatrolRoute):
  PatrolState::AreaRoam { min, max }
    → pick a random direction that stays in [min..=max] + walkable
  PatrolState::Sentry { home }
    → jitter back toward home if drifted too far
```

The `erratic_chance` field on `MonsterAI` is currently only consulted
in the **Hunting** state (random walk while pursuing). For Idle
NPCs, the patrol route is the sole driver of motion — they move
every turn (subject to their `movement_delay`).

## File surface

| File | Role |
|---|---|
| [src/map/builders/town_npcs.rs](../../src/map/builders/town_npcs.rs) | Module — `TownNpcManifest`, `TownNpcSpawn`, `TownNpcPlacement`, `TownNpcBuilder`, loader system. Single concept, single file. |
| [assets/town_npcs.ron](../../assets/town_npcs.ron) | Placement RON. Edit to add NPCs to town. |
| [assets/monsters.ron](../../assets/monsters.ron) | NPC stat blocks live here alongside hostile monsters (Drunken Sailor today). |
| [assets/factions.ron](../../assets/factions.ron) | Townsfolk relations. |
| [src/game/ai.rs](../../src/game/ai.rs) | `is_player_hostile_target` gate on Asleep/Idle → Hunting transitions. |
| [src/map/builders/town.rs](../../src/map/builders/town.rs) | Exposes `WATER_EAST_EDGE` so the NPC builder can avoid the harbour. |

## Adding a new NPC type

1. Add the stat block to `assets/monsters.ron` with `faction: "Townsfolk"`
   and an FSM AI config (peaceful behaviour comes from faction relation).
2. Add a placement entry to `assets/town_npcs.ron`.
3. Done. No code change unless the placement strategy is new
   (`Pier`, `BuildingInterior(role)`, etc.) — those land later as
   variants on `TownNpcPlacement`.

## Edge cases + resolved decisions

- **Killable**: NPCs ship with `base_hp: 12` and `damage: "1d2"`,
  same as a weak monster. Attacking them via an explicit intent
  (not bump) will kill them. They don't retaliate automatically —
  the AI never transitions Allied NPCs into Hunting.
- **Bump = no attack**: enforced by faction matrix + `resolve_bump`'s
  existing `is_hostile_to` check. No new code.
- **NPC vs NPC**: Townsfolk are Neutral to monster factions. A goblin
  invasion wouldn't ignore the locals, but the town has no monster
  spawns yet. Tighten when content lands.
- **Save/load**: NPCs persist via the existing `SavedMonster` path —
  their `MonsterAI`, `Faction`, `Position`, `PatrolRoute` all
  round-trip with no schema changes.
- **Glyph uniqueness**: drunks use `d`. Player uses `@`. Other NPCs
  will get unique letters (e.g. `f` for fishermen, named-letter for
  vendors).

## Cross-links

- [FACTIONS.md](FACTIONS.md) — Faction component, matrix, hostility lookup
- [SQUAD_AI.md](SQUAD_AI.md) — Monster AI patterns NPCs reuse
- [OVERWORLD.md](OVERWORLD.md) — Town layout that the NPC builder
  populates (water, piers, building roles, road network)
- [src/game/ai.rs](../../src/game/ai.rs) — `idle_movement`,
  `update_mode`, `PatrolRoute` consumer
