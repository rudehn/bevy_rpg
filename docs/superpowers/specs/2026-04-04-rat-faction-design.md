# Rat Faction Design

## Context

The dungeon currently has only 3 active monsters (Giant Rat, Giant Bat, Wolf) and needs a
fully realized rat faction that spans all 10 floors. Rats serve as **background fauna** — an
ever-present environmental threat that adds texture to every floor without competing with
primary factions (goblins, undead, etc.) for the spotlight. The Rat Broodmother introduces a
summoner mechanic as the faction's signature tactical puzzle.

## Design Principles

- Rats are **background threat**, not a rival faction — they're dungeon wildlife
- Danger comes from **volume**, not individual power — no PackTactics, no special damage
- Packs use **swarm + flee** behavior: aggressive when together, scatter when thinned
- Scaling is via **group size**, not new monster types — keep the roster small (3 types)
- The Broodmother is the faction's one **tactical puzzle** — a summoner with a swarm cap

## Faction

Create a new `Rat` faction in `assets/factions.ron`:

| Faction Pair | Relationship |
|-------------|-------------|
| Rat ↔ Player | Hostile |
| Rat ↔ all others | Neutral |

Rats are hostile only to the player. Neutral to all monster factions — rats coexist as
scavengers and ignore other dungeon inhabitants.

---

## Monster Roster

### Sewer Rat

The baseline swarm unit. Individually trivial, dangerous in numbers. Replaces the current
"Giant Rat" asset.

| Stat | Value |
|------|-------|
| HP | 5 |
| Damage | 1d3 (Physical) |
| Armor | 0 |
| Dodge | 0 |
| Vision | 12 |
| Movement delay | 0.9 |
| Attack delay | 1.0 |
| Faction | Rat |
| AI | FSM |

**AI Config:**
- `erratic_chance: 0` — rats move with purpose
- `flee_at_hp_percent: 0.25`
- `chase_leash: 8`
- `kites: false`
- `stationary: false`

**Floors:** 1-10

**Group sizes by floor:**

| Floors | Group Size |
|--------|-----------|
| 1 | 1-3 |
| 2 | 2-4 |
| 3-5 | 4-6 |
| 6-8 | 5-8 |
| 9-10 | 6-9 |

**Squad config:**
- `flee_threshold: 0.5` — pack scatters when half are dead
- `on_leader_death: Scatter`
- No designated leader in ambient packs (leaderless squads)

---

### Plague Rat

Poison variant that punishes sustained contact. Same fragility as sewer rat but trades base
damage for guaranteed poison DoT. Teaches: "don't let these hit you repeatedly."

| Stat | Value |
|------|-------|
| HP | 5 |
| Damage | 1d2 (Poison) |
| Armor | 0 |
| Dodge | 0 |
| Vision | 12 |
| Movement delay | 0.9 |
| Attack delay | 1.0 |
| Faction | Rat |
| Damage type | Poison |
| On-hit | BurningStrike (poison): 1 dmg/turn, 3 turns, 100% chance |
| AI | FSM (same config as Sewer Rat) |

**Floors:** 3-10

**Spawn patterns:**
- **Mixed into sewer rat packs:** Implemented via the spawn system's `group` field — sewer
  rat spawn entries on floors 3+ include 1-2 plague rats in their group composition (same
  approach as wolf packs). The spawn entry defines a mixed group, not a post-spawn pass.
  - Floors 3-5: sewer rat group includes 1 plague rat
  - Floors 6-10: sewer rat group includes 1-2 plague rats
- **Pure plague rat groups:** 2-4 plague rats (independent spawn entry)

**Squad config:** Same as sewer rat.

---

### Rat Broodmother

Mobile summoner that maintains a fixed swarm. The tactical puzzle: push through her swarm to
kill her, or fight an endless stream of replacements.

| Stat | Value |
|------|-------|
| HP | 20 |
| Damage | 1d4 (Physical) |
| Armor | 1 |
| Dodge | 0 |
| Vision | 14 |
| Movement delay | 1.2 |
| Attack delay | 1.2 |
| Faction | Rat |
| AI | GOAP |
| Squad role | Leader |

**AI Config (GOAP):**
- Traits: `Cowardly`, `Support`
- `base_morale: 0.7`

**GOAP Priority Behavior:**
1. Player adjacent → flee (move away from player)
2. Swarm count < cap and player not adjacent → summon rat
3. No summon needed, player visible → retreat (maintain distance behind swarm)
4. Player not visible → wander slowly

**Summon Swarm Ability:**
- Type: `MonsterAbility` (cooldown-based)
- Cooldown: 2 turns
- Effect: Summon 1 rat at a random adjacent walkable tile
- Summon selection: 70% sewer rat, 30% plague rat (weighted random)
- **Swarm cap: 6** — she will not summon when 6 of her summoned rats are alive
- Summoned rats join her squad (inherit her `SquadId`)

**Starting Escort:**
- Spawns with 3-4 sewer rats + 1 plague rat
- Escort rats count toward the swarm cap
- So she enters the floor with 4-5/6 cap filled, and will summon 1-2 more

**Floors:** 5-10

**Spawn:** Group of 1 Broodmother + escort. Normal spawn table entry (not machine-only).

**On Broodmother Death:**
- -0.3 morale hit to squad (standard leader death)
- `on_leader_death: Scatter` — remaining rats flee/rout
- Summoned rats that survive become leaderless wanderers

---

## New System: Summon Cap

The Broodmother's summon mechanic requires a new system that does not currently exist.

### Components

```
SummonCap { max: u32 }        — on the summoner, defines maximum active summons
SummonedBy { summoner: Entity } — on each summoned creature, points back to summoner
```

The current count is derived by querying all entities with `SummonedBy` pointing to the
summoner. No mutable counter needed — the ECS query is the source of truth.

### Ability Definition

Add a new `MonsterAbilityDef` variant:

```
SummonCreature {
    cooldown: u32,          // turns between summons (2)
    creature_name: String,  // "sewer_rat" or "plague_rat"
    weights: Vec<(String, u32)>,  // [("sewer_rat", 70), ("plague_rat", 30)]
    max_summons: u32,       // 6
}
```

### Execution Flow

1. Broodmother's turn arrives
2. GOAP planner checks: is summon action available?
   - Query count of alive entities with `SummonedBy { summoner: broodmother_entity }`
   - If count < `max_summons` and ability off cooldown → summon action is available
3. If summon action chosen:
   - Pick creature from weighted list
   - Find random adjacent walkable tile
   - Spawn creature with `SummonedBy` component
   - Add to Broodmother's squad
   - Put ability on cooldown
4. When a summoned rat dies:
   - Entity despawns naturally (existing death system)
   - Next time Broodmother checks, query returns count - 1
   - She can summon again

### Edge Cases

- **No adjacent walkable tile:** Summon fails, ability goes on cooldown anyway (she tried)
- **Broodmother dies:** Summoned rats lose their leader (squad scatter). `SummonedBy`
  components remain but are inert — no cleanup needed.
- **Summoned rat changes floor:** Should not happen (rats don't use stairs), but if it did,
  they'd still count in the query. Not a concern for rats.

---

## Pack Behavior Summary

### Ambient Packs (Leaderless)
- Rats share a `SquadId` but have no `SquadLeader`
- Base morale is lower without leader bonus (+0.0 instead of +0.2)
- Squad alerting: one rat sees player → all rats within 12 tiles converge
- Damage alerting: hitting a rat wakes nearby sleeping packmates
- `flee_threshold: 0.5` — kill half the pack and survivors scatter
- Scattered rats flee for ~5 turns, then wander aimlessly
- Without a leader, scattered rats do **not** re-form — they stay broken

### Broodmother Packs
- Broodmother is `SquadLeader`, granting +0.2 morale to all squad rats
- Higher effective morale makes the pack harder to scatter
- Broodmother stays behind her swarm (Cowardly + Support GOAP traits)
- Rats act as a living shield; player must push through to reach her
- On Broodmother death: -0.3 morale + scatter → immediate rout

### Morale Recovery
- Rats in combat do not recover morale
- Out of combat (no player visible, no damage for several turns): slow morale recovery
- Without a leader, recovery is very slow and rats rarely re-form into a threat
- With Broodmother alive: faster recovery, but she'd need to survive and break contact

---

## Spawn Table Configuration

```ron
// Sewer Rat — ambient packs, all floors
(
    monster: "sewer_rat",
    min_floor: 1, max_floor: 10,
    min_group: 1, max_group: 3,  // Floor 1 override via floor-scaled groups
    // Group size scales with floor depth (see group size table above)
)

// Plague Rat — mixed and pure groups, floors 3+
(
    monster: "plague_rat",
    min_floor: 3, max_floor: 10,
    min_group: 2, max_group: 4,
)

// Rat Broodmother — with escort, floors 5+
(
    monster: "rat_broodmother",
    min_floor: 5, max_floor: 10,
    min_group: 1, max_group: 1,
    // Escort spawned as part of her squad (3-4 sewer + 1 plague)
)
```

**Note:** The current spawn system uses flat `min_group`/`max_group` per entry. Floor-scaled
group sizes (e.g., 1-3 on floor 1, 6-9 on floor 10) may require extending the spawn config
to support per-floor group size overrides, or using multiple spawn entries for different floor
ranges.

---

## Verification Plan

### Unit Tests
- Summon cap: verify `SummonCap` query returns correct count after spawn/despawn
- Summon ability: verify it checks cap before activating
- Weighted summon selection: verify distribution matches weights
- Squad scatter: verify leaderless packs rout at flee_threshold

### Integration Testing (Manual)
1. **Floor 1:** Verify sewer rat packs of 1-3 spawn, fight correctly, scatter when thinned
2. **Floor 3:** Verify plague rats appear mixed into sewer rat packs; poison DoT applies on hit
3. **Floor 5:** Verify Broodmother spawns with escort, summons replacements, respects cap
4. **Floor 5 Broodmother kill:** Verify remaining rats scatter/rout on her death
5. **Floor 10:** Verify large rat packs (6-9) spawn; Broodmother encounter still works
6. **Faction check:** Verify rats are hostile to player, neutral to other monsters, hostile to fungal
