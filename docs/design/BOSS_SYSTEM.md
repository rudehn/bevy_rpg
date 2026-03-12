# Boss System — M9 Design & Implementation Plan

## Overview

Floors 3, 6, 9, and 10 feature boss encounters. Bosses live in large open rooms at the far end of each floor and use a **behavior tree AI** rather than the standard `MonsterAI` state machine. This lets each boss express complex conditional behavior — phase transitions, ability priorities, summon schedules — declaratively as a composable node tree.

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

Inserted into `floor_builder` after `DistantExit` for floors `[3, 6, 9, 10]`.

---

## Boss Definitions

### Floor 3 — Goblin Warchief

**Stats:** Level 6, 80 HP, 2d6+3 damage, ranged_range 5
**Phase trigger:** 40% HP → speed boost

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

---

### Floor 6 — Bone Lord

**Stats:** Level 12, 120 HP, undead, regenerates slowly
**Phase trigger:** 50% HP → speed boost + skeleton summons

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

---

### Floor 9 — Pit Fiend

**Stats:** Level 18, 160 HP, 3d6+5 damage, fire damage type
**Phase trigger:** 50% HP → speed boost + imp summons

```
Selector
├── Sequence [Phase 2 transition]
│   ├── HpBelow(0.5)
│   ├── PhaseBelow(2)
│   └── SetPhase(2)
├── Sequence [Summon imp — phase 2, cooldown 5]
│   ├── PhaseAtLeast(2)
│   ├── AbilityReady { cooldown_id: 0 }
│   └── SummonMinion { "imp", cooldown_id: 0, reset_to: 5 }
├── Sequence [Fire breath spell]
│   ├── PlayerVisible
│   └── CastSpell { slot: 0 }
└── Selector
    ├── MeleeAttack
    └── MoveToPlayer
```

---

### Floor 10 — Shadow Archon

**Stats:** Level 22, 200 HP, magic damage, teleport ability
**Phase trigger:** 50% HP → speed boost + shade summons
**Victory:** Amulet of Dominion spawns on death

```
Selector
├── Sequence [Phase 2 transition — summon 2 shades]
│   ├── HpBelow(0.5)
│   ├── PhaseBelow(2)
│   └── Sequence
│       ├── SetPhase(2)
│       ├── SummonMinion { "shade", cooldown_id: 0, reset_to: 0 }
│       └── SummonMinion { "shade", cooldown_id: 0, reset_to: 0 }
├── Sequence [Periodic shade — phase 2, cooldown 6]
│   ├── PhaseAtLeast(2)
│   ├── AbilityReady { cooldown_id: 0 }
│   └── SummonMinion { "shade", cooldown_id: 0, reset_to: 6 }
├── Sequence [Shadow bolt spell]
│   ├── PlayerVisible
│   └── CastSpell { slot: 0 }
└── Selector
    ├── MeleeAttack
    └── MoveToPlayer
```

---

## Boss Death System

`boss_death_system` listens for `DeathEvent` on `Boss`-marked entities:
- Logs "The boss has been defeated!"
- If `floor.depth == 10`: spawns `"amulet_of_dominion"` at boss position via `spawn_item()`

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
    level: 6, base_hp: 80, damage: "2d6+3",
    strength: 14, dexterity: 12, constitution: 14,
    agility: 12, intelligence: 8, perception: 10,
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
| `src/game/boss.rs` | **NEW** — `BossAI`, `Boss`, `BossPlugin`, dispatch + death systems, 4 BT constructors |
| `src/map/builders/boss_room.rs` | **NEW** — `BossRoomBuilder` |
| `src/map/builders/mod.rs` | Add `pub mod boss_room`; wire into `floor_builder` |
| `src/game/mod.rs` | Register `BossPlugin` |
| `src/game/turns.rs` | Add `boss_ai_dispatch` to Processing chain |
| `src/assets/mod.rs` | Add `is_boss`, `boss_phase_thresholds` to `MonsterAsset` |
| `src/game/spawner.rs` | Insert `BossAI + Boss` when `is_boss`; skip `MonsterAI` |
| `assets/monsters.ron` | Add 4 boss entries + minion entries (skeleton, imp, shade) |
| `assets/items.ron` | Add `amulet_of_dominion` |

---

## Boss Summary

| Floor | Boss | Phase 2 Trigger | Phase 2 Behavior |
|-------|------|-----------------|------------------|
| 3 | Goblin Warchief | 40% HP | Speed boost, prefers ranged axe |
| 6 | Bone Lord | 50% HP | Speed boost + periodic skeleton summons |
| 9 | Pit Fiend | 50% HP | Speed boost + periodic imp summons |
| 10 | Shadow Archon | 50% HP | Speed boost + shade summons; drops Amulet of Dominion on death |

---

## Verification Checklist

1. `cargo check` — zero errors
2. Descend to floor 3 → large open boss room visible at far end of map
3. Fight Goblin Warchief → BT correctly uses ranged attack when in range, melee otherwise
4. Damage Warchief below 40% → "enters a new phase!" log, boss moves faster
5. Floor 6: at 50% HP, two skeletons spawn adjacent to Bone Lord
6. Floor 10: kill Shadow Archon → Amulet of Dominion item spawns at boss position
7. Pick up Amulet → victory screen triggers
