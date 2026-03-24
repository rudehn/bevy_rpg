# Boss System — Design & Implementation Plan

## Overview

Floors 5, 10, 15, 20, and 26 feature boss encounters. Bosses live in large open rooms at the far end of each floor and use a **behavior tree AI** rather than the standard `MonsterAI` state machine. This lets each boss express complex conditional behavior — phase transitions, ability priorities, summon schedules — declaratively as a composable node tree.

---

## Architecture

```
behavior_tree.rs    — BtStatus, BehaviorNode enum, execute_node()
boss.rs             — BossAI component, Boss marker, boss_ai_dispatch, boss_death_system
builders/boss_room.rs  — BossRoomBuilder MetaMapBuilder
```

Bosses do NOT receive `MonsterAI`. Instead, `boss_ai_dispatch` runs alongside `monster_ai_dispatch` in the Processing chain and queries `With<BossAI>`.

---

## Behavior Tree Core (`behavior_tree.rs`)

```rust
pub enum BtStatus { Success, Failure }

pub enum BehaviorNode {
    // Composites
    Sequence(Vec<BehaviorNode>),         // AND — all children must succeed
    Selector(Vec<BehaviorNode>),         // OR  — first succeeding child wins

    // Condition guards (read-only, never write world state)
    PlayerVisible,
    HpBelow(f32),                        // health.current / health.max < threshold
    PhaseAtLeast(u8),                    // boss_ai.phase >= n
    PhaseBelow(u8),                      // boss_ai.phase < n  (used as "not yet phase N")
    AbilityReady { cooldown_id: usize }, // boss_ai.cooldowns[id] == 0

    // Action leaves (write intent messages to world)
    MeleeAttack,
    RangedAttack,
    MoveToPlayer,
    CastSpell { slot: usize },
    SummonMinion { monster_name: &'static str, cooldown_id: usize, reset_to: u32 },
    SetPhase(u8),   // advance boss.phase, reduce SpeedStats.delay, log message
    Wait,
}
```

`execute_node` takes `(node, entity, &mut World, &mut BossAI) -> BtStatus`. Action nodes write `world.write_message(...)` the same way `MonsterAI` does.

### Cooldown Tracking

`BossAI` holds `cooldowns: Vec<u32>` (one slot per timed ability). Each turn `boss_ai_dispatch` decrements all counters before running the BT. `SummonMinion` resets its counter to `reset_to` on success.

---

## `BossAI` Component

```rust
#[derive(Component)]
pub struct BossAI {
    pub phase: u8,
    pub phase_thresholds: Vec<f32>,
    pub cooldowns: Vec<u32>,
    pub root: BehaviorNode,
    pub last_known_player: Option<Point>,
}

#[derive(Component)]
pub struct Boss;  // marker for death system queries
```

`BossAI::for_monster(name: &str, thresholds: Vec<f32>) -> BossAI` dispatches to per-boss constructors.

---

## Boss Room

`BossRoomBuilder` is a `MetaMapBuilder` that:
1. Finds the map corner farthest from the player start position
2. Carves a **16×12 rectangular room** (wall border, floor interior)
3. Connects it with a short open corridor to the nearest existing walkable floor tile (no door — freely accessible)
4. Pushes `(room_center, boss_name)` onto `build_data.spawn_list`

Inserted into `floor_builder` after `DistantExit` for floors `[5, 10, 15, 20, 26]`.

---

## Boss Definitions

### Floor 5 — Goblin Warchief

*The biggest, meanest goblin in the dungeon — and he knows it.*

| Lvl | base_hp | STR | CON | AGI | INT | DEF | ATK | Eff. HP |
|-----|---------|-----|-----|-----|-----|-----|-----|---------|
| 3 | 28 | 15 | 14 | 12 | 10 | 3 | 1d10 | 37 |

**Phase trigger:** 40% HP → speed boost
**Abilities:**
- **Battle Cry (1/fight):** Summons 2 Goblins and 1 Goblin Archer at turn 1
- **Enrage (< 40% HP):** Gains +3 ATK and SPD drops to 0.83 for remainder of fight
- **Throwing Axe:** Ranged attack (1d8) used if player is > 4 tiles away

```
Selector
├── Sequence [Phase 2 transition]
│   ├── HpBelow(0.4)
│   ├── PhaseBelow(2)
│   └── SetPhase(2)               ← speed boost, log "enters a new phase!"
├── Sequence [Ranged axe throw]
│   ├── PlayerVisible
│   └── RangedAttack
└── Sequence [Close and melee]
    ├── PlayerVisible
    └── Selector
        ├── MeleeAttack
        └── MoveToPlayer
```

**Loot:** Guaranteed Uncommon weapon + small gold drop

---

### Floor 10 — Orc Warlord "Grak"

*Forged in the blood pits of the deep warrens. His warband fears him more than they fear you.*

| Lvl | base_hp | STR | CON | AGI | INT | DEF | ATK | Eff. HP |
|-----|---------|-----|-----|-----|-----|-----|-----|---------|
| 5 | 35 | 20 | 16 | 10 | 10 | 5 | 2d8 | 65 |

**Phase trigger:** 40% HP → speed boost + enrage
**Abilities:**
- **Cleave:** Melee hits all adjacent tiles (not just the target)
- **War Cry (1/fight):** Summons 2 Orc Warriors
- **Enrage (< 40% HP):** +4 STR, delay drops to 0.85

```
Selector
├── Sequence [Phase 2 transition — summon warriors]
│   ├── HpBelow(0.4)
│   ├── PhaseBelow(2)
│   └── Sequence
│       ├── SetPhase(2)
│       ├── SummonMinion { "orc_warrior", cooldown_id: 0, reset_to: 0 }
│       └── SummonMinion { "orc_warrior", cooldown_id: 0, reset_to: 0 }
└── Selector
    ├── MeleeAttack           ← Cleave hits all adjacent
    └── MoveToPlayer
```

**Loot:** Guaranteed Rare weapon

---

### Floor 15 — Bone Lord

*A towering skeleton warlord bound together by dark necromancy. Destroying it weakens the magic of the catacombs.*

| Lvl | base_hp | STR | CON | AGI | INT | DEF | ATK | Eff. HP |
|-----|---------|-----|-----|-----|-----|-----|-----|---------|
| 6 | 40 | 14 | 14 | 8 | 8 | 7 | 2d8 | 64 |

**Phase trigger:** 50% HP → speed boost + skeleton summons
**Abilities:**
- **Reassemble (2× fight):** When reduced to 0 HP for the first two times, regenerates to 30 HP instead of dying (third time is permanent)
- **Summon Minions:** Raises 1-2 Skeletons from the room's "bone piles" (3 total piles in the room)
- **Bone Shards:** AoE attack (2d4) hitting all adjacent tiles

```
Selector
├── Sequence [Phase 2 transition — summon 2 skeletons]
│   ├── HpBelow(0.5)
│   ├── PhaseBelow(2)
│   └── Sequence
│       ├── SetPhase(2)
│       ├── SummonMinion { "skeleton", cooldown_id: 0, reset_to: 0 }
│       └── SummonMinion { "skeleton", cooldown_id: 0, reset_to: 0 }
├── Sequence [Periodic skeleton (phase 2, every 5 turns)]
│   ├── PhaseAtLeast(2)
│   ├── AbilityReady { cooldown_id: 0 }
│   └── SummonMinion { "skeleton", cooldown_id: 0, reset_to: 5 }
└── Selector
    ├── MeleeAttack
    └── MoveToPlayer
```

**Loot:** Guaranteed Rare armor piece + spellbook

---

### Floor 20 — Vampire Lord

*Timeless. Patient. He has watched a thousand adventurers descend. None have returned.*

| Lvl | base_hp | STR | CON | AGI | INT | DEF | ATK | Eff. HP |
|-----|---------|-----|-----|-----|-----|-----|-----|---------|
| 7 | 40 | 16 | 16 | 14 | 16 | 3 | 1d12 | 82 |

**Phase trigger:** 50% HP → speed boost + Fear Aura
**Abilities:**
- **Vampiric Strike (spell):** Damage 2d6 + heals self 1d6. Uses existing multi-effect system.
- **Fear Aura (< 50% HP):** Player must pass a check or lose their next turn (every 4 turns)
- **Life Drain:** Melee hits heal the Vampire Lord for 50% of damage dealt
- **Bat Swarm (1/fight):** Summons 3 Giant Bats as distractions

```
Selector
├── Sequence [Phase 2 transition — fear aura + bat swarm]
│   ├── HpBelow(0.5)
│   ├── PhaseBelow(2)
│   └── Sequence
│       ├── SetPhase(2)
│       ├── SummonMinion { "giant_bat", cooldown_id: 0, reset_to: 0 }
│       ├── SummonMinion { "giant_bat", cooldown_id: 0, reset_to: 0 }
│       └── SummonMinion { "giant_bat", cooldown_id: 0, reset_to: 0 }
├── Sequence [Vampiric Strike spell — prioritize when hurt]
│   ├── HpBelow(0.75)
│   ├── PlayerVisible
│   └── CastSpell { slot: 0 }   ← vampiric_strike
└── Selector
    ├── MeleeAttack               ← Life Drain on hit
    └── MoveToPlayer
```

**Loot:** Guaranteed Rare/Legendary ring or amulet

---

### Floor 26 — Shadow Archon (Final Boss)

*The dungeon's true master. It does not speak. It does not negotiate. It simply unmakes.*

| Lvl | base_hp | STR | CON | AGI | INT | DEF | ATK | Eff. HP |
|-----|---------|-----|-----|-----|-----|-----|-----|---------|
| 8 | 60 | 18 | 18 | 14 | 20 | 10 | 3d10 | 108 |

**Phase trigger:** 50% HP → speed boost + shade summons
**Victory:** Amulet of Dominion spawns on death

**Phase 1 (100% → 50% HP):**
- **Shadow Strike:** Single target melee (3d10)
- **Void Tendrils:** Roots player in place for 2 turns (every 6 turns)
- **Shade Summon:** Summons 2 Shadow Fiends when below 75% HP

**Phase 2 (< 50% HP — transitions with a dramatic visual):**
- All Phase 1 abilities continue
- **Darkness Pulse:** AoE nova every 4 turns, 3d8 necrotic damage in 5-tile radius
- **Mana Void:** Drains 20 mana from player on hit
- **Desperate Shadows:** Summons 1 additional Shadow Fiend every 4 turns

```
Selector
├── Sequence [Phase 2 transition — summon 2 shades]
│   ├── HpBelow(0.5)
│   ├── PhaseBelow(2)
│   └── Sequence
│       ├── SetPhase(2)
│       ├── SummonMinion { "shadow_fiend", cooldown_id: 0, reset_to: 0 }
│       └── SummonMinion { "shadow_fiend", cooldown_id: 0, reset_to: 0 }
├── Sequence [Periodic shade — phase 2, cooldown 4]
│   ├── PhaseAtLeast(2)
│   ├── AbilityReady { cooldown_id: 0 }
│   └── SummonMinion { "shadow_fiend", cooldown_id: 0, reset_to: 4 }
├── Sequence [Shadow bolt spell]
│   ├── PlayerVisible
│   └── CastSpell { slot: 0 }
└── Selector
    ├── MeleeAttack
    └── MoveToPlayer
```

**Defeat:**
- Shadow Archon collapses. The room's darkness lifts.
- The **Amulet of Dominion** becomes accessible on its pedestal.
- Picking it up triggers the **victory screen**.

---

## Boss Death System

`boss_death_system` listens for `DeathEvent` on `Boss`-marked entities:
- Logs "The boss has been defeated!"
- If `floor.depth == 26`: spawns `"amulet_of_dominion"` at boss position via `spawn_item()`

The existing `AmuletOfBevy` pickup handler triggers the win condition — no additional changes needed.

---

## Amulet of Dominion (`assets/items.ron`)

```ron
"amulet_of_dominion": ItemAsset(
    name: "Amulet of Dominion",
    sprite: "items/amulets.png#0",
    item_kind: Amulet,
    rarity: Legendary,
    is_victory: true,
    tile_size: Some((16, 16)),
),
```

---

## `MonsterAsset` Extensions (`assets/monsters.ron`)

```ron
// Example entry
"goblin_warchief": MonsterAsset(
    name: "Goblin Warchief",
    sprite: "monsters/goblins.png#4",
    level: 3, base_hp: 28, damage: "1d10",
    strength: 15, dexterity: 12, constitution: 14,
    agility: 12, intelligence: 10, perception: 10,
    base_armor: 3,
    ranged_range: 5,
    is_boss: true,
    boss_phase_thresholds: [0.4],
    loot_table: [],
),
```

New fields added to `MonsterAsset` in `assets/mod.rs`:
- `is_boss: bool` (`#[serde(default)]`)
- `boss_phase_thresholds: Vec<f32>` (`#[serde(default)]`)

---

## Files to Create / Modify

| File | Change |
|------|--------|
| `src/game/behavior_tree.rs` | **NEW** — `BtStatus`, `BehaviorNode`, `execute_node` |
| `src/game/boss.rs` | **NEW** — `BossAI`, `Boss`, `BossPlugin`, dispatch + death systems, 5 BT constructors |
| `src/map/builders/boss_room.rs` | **NEW** — `BossRoomBuilder` |
| `src/map/builders/mod.rs` | Add `pub mod boss_room`; wire into `floor_builder` |
| `src/game/mod.rs` | Register `BossPlugin` |
| `src/game/turns.rs` | Add `boss_ai_dispatch` to Processing chain |
| `src/assets/mod.rs` | Add `is_boss`, `boss_phase_thresholds`, `base_armor` to `MonsterAsset` |
| `src/game/spawner.rs` | Insert `BossAI + Boss` when `is_boss`; skip `MonsterAI` |
| `assets/monsters.ron` | Add 5 boss entries + minion entries (skeleton, shadow_fiend, giant_bat) |
| `assets/items.ron` | Add `amulet_of_dominion` |

---

## Boss Summary

| Floor | Boss | Phase 2 Trigger | Phase 2 Behavior |
|-------|------|-----------------|------------------|
| 5 | Goblin Warchief | 40% HP | Speed boost, prefers ranged axe |
| 10 | Orc Warlord "Grak" | 40% HP | Speed boost + enrage (+4 STR) + summon 2 warriors |
| 15 | Bone Lord | 50% HP | Speed boost + periodic skeleton summons |
| 20 | Vampire Lord | 50% HP | Speed boost + Fear Aura + bat swarm |
| 26 | Shadow Archon | 50% HP | Speed boost + shade summons; drops Amulet of Dominion on death |

---

## Verification Checklist

1. `cargo check` — zero errors
2. Descend to floor 5 → large open boss room visible at far end of map
3. Fight Goblin Warchief → BT correctly uses ranged attack when in range, melee otherwise
4. Damage Warchief below 40% → "enters a new phase!" log, boss moves faster
5. Floor 10: Orc Warlord summons 2 Orc Warriors when enraged
6. Floor 15: Bone Lord reassembles twice, summons skeletons at 50% HP
7. Floor 20: Vampire Lord life-steals with vampiric_strike, summons bats at 50% HP
8. Floor 26: kill Shadow Archon → Amulet of Dominion item spawns at boss position
9. Pick up Amulet → victory screen triggers
