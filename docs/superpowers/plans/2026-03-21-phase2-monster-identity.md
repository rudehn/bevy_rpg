# Phase 2: Monster Identity — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every fight feel different by adding 4 AI behaviors (fleeing, kiting, erratic movement, leash/give-up) and aligning 8 core monsters to the new design docs.

**Architecture:** The monster AI system in `src/game/ai.rs` already handles Asleep→Idle→Hunting transitions, A* pathfinding, ranged attacks, and squad leashing. We're adding new decision branches inside the existing AI execution path, driven by per-monster behavior flags on `MonsterAsset`. Pure decision functions are extracted for testing.

**Tech Stack:** Rust, Bevy 0.17, bracket-lib (A*, FOV, RNG)

**Reference Docs:**
- `docs/design/ENEMIES.md` — Monster stats, behaviors, group sizes
- `docs/design/PLAYER.md` — Combat formulas (symmetric combat)

**Current State:**
- 40+ monsters in `assets/monsters.ron` (from old design — many don't match new ENEMIES.md)
- Group spawning with BFS cluster placement: working
- Squad shared alerting: working
- BurningStrike on-hit: working
- Ranged combat: working
- Fleeing: threshold defined in SquadConfig but NOT implemented in AI
- Kiting: NOT implemented
- Erratic movement: NOT implemented
- Leash/give-up: NOT implemented (separate from squad leash)

---

## File Map

| File | Role | Change Type |
|------|------|-------------|
| `src/game/ai.rs` | Monster AI execution — add flee, kite, erratic, leash | Modify |
| `src/game/ai_behaviors.rs` | NEW — pure decision functions for testability | Create |
| `src/assets/mod.rs` | MonsterAsset — add behavior flag fields | Modify |
| `assets/monsters.ron` | Update 8 monster definitions to match ENEMIES.md | Modify |
| `assets/monster_spawns.ron` | Update spawn entries for new floor ranges/group sizes | Modify |

---

### Task 1: Add Behavior Flags to MonsterAsset

**Files:**
- Modify: `src/assets/mod.rs`
- Modify: `src/game/spawner.rs`
- Modify: `src/game/ai.rs` (MonsterAI component)

Add per-monster behavior configuration fields to `MonsterAsset` so that each
monster can have distinct AI personality. All fields default to sensible values
so existing monsters don't break.

- [ ] **Step 1: Add behavior fields to MonsterAsset**

In `src/assets/mod.rs`, add these fields to `MonsterAsset` (all with `#[serde(default)]`):

```rust
/// HP percentage (0.0-1.0) at which this monster flees. 0.0 = never flees.
#[serde(default)]
pub flee_at_hp_percent: f32,

/// Chance (0.0-1.0) per turn to move in a random direction instead of toward target.
#[serde(default)]
pub erratic_chance: f32,

/// Max tiles this monster will chase before giving up and returning to idle. 0 = no limit.
#[serde(default)]
pub chase_leash: u32,

/// If true, ranged monsters try to maintain distance from the player (retreat if too close).
#[serde(default)]
pub kites: bool,

/// Minimum distance a kiting monster tries to maintain from the player.
#[serde(default = "default_kite_distance")]
pub kite_distance: u32,

// Add the default function:
fn default_kite_distance() -> u32 { 3 }
```

- [ ] **Step 2: Add behavior fields to MonsterAI component**

In `src/game/ai.rs`, add runtime state to `MonsterAI` for tracking chase distance:

```rust
/// How many tiles this monster has chased since last seeing the player.
pub chase_distance: u32,
/// Spawn position — used as return point when chase_leash exceeded.
pub spawn_position: Option<Point>,
```

- [ ] **Step 3: Wire behavior flags from asset to AI at spawn time**

In `src/game/spawner.rs`, when creating `MonsterAI`, read the behavior flags
from `MonsterAsset` and store them. The `MonsterAI` component needs fields for:
`flee_at_hp_percent`, `erratic_chance`, `chase_leash`, `kites`, `kite_distance`.

Copy these from the asset at spawn time. Also store the spawn position.

- [ ] **Step 4: Run cargo check**

Run: `cargo check`
Expected: Clean compilation. Existing monsters use defaults (0.0/false/0).

- [ ] **Step 5: Commit**

```bash
git add src/assets/mod.rs src/game/ai.rs src/game/spawner.rs
git commit -m "feat(ai): add per-monster behavior flags (flee, erratic, kite, leash)"
```

---

### Task 2: Create AI Behavior Decision Functions (Testable)

**Files:**
- Create: `src/game/ai_behaviors.rs`
- Modify: `src/game/mod.rs` (add module)

Extract pure decision functions so AI behaviors are testable without ECS.

- [ ] **Step 1: Create ai_behaviors.rs with pure functions**

```rust
//! Pure decision functions for monster AI behaviors.
//! These are extracted from the AI system for testability.

/// Should this monster flee? Checks individual HP ratio against threshold.
pub fn should_flee(current_hp: i32, max_hp: i32, flee_threshold: f32) -> bool {
    if flee_threshold <= 0.0 || max_hp <= 0 {
        return false;
    }
    (current_hp as f32 / max_hp as f32) < flee_threshold
}

/// Should this monster move erratically this turn?
/// Returns true with probability `erratic_chance`.
pub fn should_move_erratically(erratic_chance: f32, roll: f32) -> bool {
    erratic_chance > 0.0 && roll < erratic_chance
}

/// Should this monster give up chasing and return to idle?
pub fn should_give_up_chase(chase_distance: u32, chase_leash: u32) -> bool {
    chase_leash > 0 && chase_distance >= chase_leash
}

/// Should a kiting monster retreat from the player?
/// Returns true if the monster is closer than kite_distance.
pub fn should_kite_retreat(
    monster_x: i32, monster_y: i32,
    player_x: i32, player_y: i32,
    kite_distance: u32,
) -> bool {
    let dx = (monster_x - player_x).abs();
    let dy = (monster_y - player_y).abs();
    let dist_sq = dx * dx + dy * dy;
    let kite_sq = (kite_distance as i32) * (kite_distance as i32);
    dist_sq < kite_sq
}

/// Pick a random cardinal direction to flee AWAY from the given position.
/// Returns the best flee direction as a (dx, dy) offset.
pub fn flee_direction(
    monster_x: i32, monster_y: i32,
    threat_x: i32, threat_y: i32,
) -> (i32, i32) {
    let dx = monster_x - threat_x;
    let dy = monster_y - threat_y;
    // Move in the direction with the greatest distance component
    if dx.abs() >= dy.abs() {
        (dx.signum(), 0)
    } else {
        (0, dy.signum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- should_flee ---

    #[test]
    fn flee_when_below_threshold() {
        assert!(should_flee(3, 10, 0.3)); // 30% HP, threshold 30%
    }

    #[test]
    fn no_flee_when_above_threshold() {
        assert!(!should_flee(5, 10, 0.3)); // 50% HP, threshold 30%
    }

    #[test]
    fn no_flee_when_threshold_zero() {
        assert!(!should_flee(1, 10, 0.0)); // threshold 0 = never flee
    }

    #[test]
    fn no_flee_at_exact_threshold() {
        assert!(!should_flee(3, 10, 0.3)); // 30% == 30%, not below
        assert!(should_flee(2, 10, 0.3));  // 20% < 30%, flees
    }

    // --- should_move_erratically ---

    #[test]
    fn erratic_with_low_roll() {
        assert!(should_move_erratically(0.3, 0.1)); // 10% < 30%
    }

    #[test]
    fn not_erratic_with_high_roll() {
        assert!(!should_move_erratically(0.3, 0.5)); // 50% > 30%
    }

    #[test]
    fn never_erratic_when_chance_zero() {
        assert!(!should_move_erratically(0.0, 0.0));
    }

    // --- should_give_up_chase ---

    #[test]
    fn give_up_when_leash_exceeded() {
        assert!(should_give_up_chase(10, 8)); // chased 10, leash 8
    }

    #[test]
    fn keep_chasing_within_leash() {
        assert!(!should_give_up_chase(5, 8));
    }

    #[test]
    fn never_give_up_when_leash_zero() {
        assert!(!should_give_up_chase(100, 0)); // leash 0 = unlimited
    }

    // --- should_kite_retreat ---

    #[test]
    fn kite_when_too_close() {
        assert!(should_kite_retreat(5, 5, 6, 5, 3)); // 1 tile away, wants 3
    }

    #[test]
    fn no_kite_when_far_enough() {
        assert!(!should_kite_retreat(5, 5, 10, 5, 3)); // 5 tiles away, wants 3
    }

    // --- flee_direction ---

    #[test]
    fn flee_away_from_threat() {
        assert_eq!(flee_direction(5, 5, 8, 5), (-1, 0)); // threat east, flee west
        assert_eq!(flee_direction(5, 5, 5, 8), (0, -1)); // threat south, flee north
        assert_eq!(flee_direction(5, 5, 2, 5), (1, 0));  // threat west, flee east
    }
}
```

- [ ] **Step 2: Register module**

In `src/game/mod.rs`, add `pub mod ai_behaviors;`

- [ ] **Step 3: Run tests**

Run: `cargo test -p bevy_rpg -- ai_behaviors`
Expected: All 10+ tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/game/ai_behaviors.rs src/game/mod.rs
git commit -m "feat(ai): add testable AI decision functions with tests"
```

---

### Task 3: Implement Fleeing AI

**Files:**
- Modify: `src/game/ai.rs`

Wire `should_flee` into the AI execution path. When a monster's HP falls below
its `flee_at_hp_percent`, it pathfinds away from the player instead of toward.

- [ ] **Step 1: Add flee check to AI execution**

In the monster AI execution (likely in the Hunting mode logic), BEFORE the
pathfind-toward-player step, add:

```rust
// Check flee condition
if ai.flee_at_hp_percent > 0.0 {
    if ai_behaviors::should_flee(health.current, health.max, ai.flee_at_hp_percent) {
        // Flee: move away from player
        let (dx, dy) = ai_behaviors::flee_direction(
            monster_pos.x, monster_pos.y,
            player_pos.x, player_pos.y,
        );
        // Try to move in flee direction, emit MovementIntent
        // ... (use existing movement intent emission pattern)
        return; // Skip normal hunting pathfind
    }
}
```

The flee direction function returns the best cardinal direction away from the
player. Try that direction first; if blocked, try perpendicular directions.

- [ ] **Step 2: Import ai_behaviors module**

Add `use crate::game::ai_behaviors;` at the top of `ai.rs`.

- [ ] **Step 3: Run cargo check**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/game/ai.rs
git commit -m "feat(ai): implement fleeing behavior when HP below threshold"
```

---

### Task 4: Implement Kiting AI

**Files:**
- Modify: `src/game/ai.rs`

Ranged monsters with `kites: true` retreat from the player when within
`kite_distance` tiles, instead of standing still or advancing.

- [ ] **Step 1: Add kite check to ranged AI logic**

In the AI execution, when a ranged monster is deciding whether to attack or move,
add a kite check BEFORE the ranged attack decision:

```rust
if ai.kites {
    if ai_behaviors::should_kite_retreat(
        monster_pos.x, monster_pos.y,
        player_pos.x, player_pos.y,
        ai.kite_distance,
    ) {
        // Move away from player (same flee_direction logic)
        let (dx, dy) = ai_behaviors::flee_direction(
            monster_pos.x, monster_pos.y,
            player_pos.x, player_pos.y,
        );
        // Emit movement intent away
        // ...
        return; // Don't attack this turn, retreat instead
    }
}
```

Kiting monsters still attack when at proper range — they only retreat when
the player closes distance below `kite_distance`.

- [ ] **Step 2: Run cargo check**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/game/ai.rs
git commit -m "feat(ai): implement kiting behavior for ranged monsters"
```

---

### Task 5: Implement Erratic Movement and Chase Leash

**Files:**
- Modify: `src/game/ai.rs`

Two small behaviors:
1. **Erratic movement:** With probability `erratic_chance`, move in a random
   direction instead of toward the target.
2. **Chase leash:** Track how far the monster has chased. If it exceeds
   `chase_leash`, give up and return to idle.

- [ ] **Step 1: Add erratic movement check**

In the movement/pathfinding section of AI execution, before computing the A*
path, check for erratic movement:

```rust
if ai.erratic_chance > 0.0 {
    let roll = rng.rand::<f32>(); // or equivalent RNG call
    if ai_behaviors::should_move_erratically(ai.erratic_chance, roll) {
        // Pick a random cardinal direction
        // Emit movement intent in that direction
        return;
    }
}
```

- [ ] **Step 2: Add chase distance tracking and leash check**

When a monster is in Hunting mode and moves toward the player, increment
`ai.chase_distance`. When the monster sees the player again (enters FOV),
reset `chase_distance` to 0.

Before pathfinding:
```rust
if ai_behaviors::should_give_up_chase(ai.chase_distance, ai.chase_leash) {
    // Give up: transition to Idle, reset chase_distance
    ai.mode = MonsterAIMode::Idle;
    ai.chase_distance = 0;
    return;
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check`

- [ ] **Step 4: Run tests**

Run: `cargo test -p bevy_rpg`
Expected: All tests pass (including ai_behaviors tests from Task 2).

- [ ] **Step 5: Commit**

```bash
git add src/game/ai.rs
git commit -m "feat(ai): implement erratic movement and chase leash/give-up"
```

---

### Task 6: Update Monster Definitions

**Files:**
- Modify: `assets/monsters.ron`

Update the 8 Phase 2 monsters to match `docs/design/ENEMIES.md` stats and
assign behavior flags. Don't delete other monsters — just ensure these 8 are
correct.

- [ ] **Step 1: Update Giant Rat**

```ron
base_hp: 5, damage: "1d3", base_armor: 0, vision: 6,
// behavior: flee_at_hp_percent: 0.0, erratic_chance: 0.0, chase_leash: 0, kites: false
```

Group size handled in monster_spawns.ron (Task 7).

- [ ] **Step 2: Update Giant Bat**

```ron
base_hp: 4, damage: "1d3", base_armor: 0, vision: 8,
// Key: erratic_chance: 0.3, dodge: 2
```
The Dodge component is set via a `dodge` field on MonsterAsset (check if this
field exists; if not, add it alongside `base_armor`).

- [ ] **Step 3: Update Wolf**

```ron
base_hp: 10, damage: "1d6", base_armor: 0, vision: 12,
// No special behavior flags — relies on squad alerting
```

- [ ] **Step 4: Update Fire Salamander**

If "Fire Salamander" doesn't exist, find the closest match (Venomous Snake?)
and rename/update it:
```ron
base_hp: 8, damage: "1d4", base_armor: 0, vision: 8,
damage_type: "fire",
resistances: { "fire": 50 },
abilities: [ BurningStrike(damage_per_turn: 2, duration: 3, chance: 100) ],
```

- [ ] **Step 5: Update Goblin**

```ron
base_hp: 5, damage: "1d4", base_armor: 0, vision: 8,
flee_at_hp_percent: 0.3,
```

- [ ] **Step 6: Update Goblin Archer**

```ron
base_hp: 5, damage: "1d6", base_armor: 0, vision: 10,
ranged_range: 8, kites: true, kite_distance: 3,
```

- [ ] **Step 7: Update Goblin Brute**

```ron
base_hp: 14, damage: "1d8", base_armor: 2, vision: 7,
// No special behavior — just tanky
```

- [ ] **Step 8: Update Cave Bear**

```ron
base_hp: 25, damage: "2d6", base_armor: 2, vision: 6,
chase_leash: 8,
// SpeedStats delay should be 1.15
```
Check if delay is configurable per monster in MonsterAsset. If not, it defaults
to 1.0 — add a `delay` field if needed.

- [ ] **Step 9: Run cargo check**

Run: `cargo check`
Expected: Clean compilation with updated assets.

- [ ] **Step 10: Commit**

```bash
git add assets/monsters.ron src/assets/mod.rs
git commit -m "feat(monsters): update 8 core monsters to match ENEMIES.md design"
```

---

### Task 7: Update Monster Spawn Table

**Files:**
- Modify: `assets/monster_spawns.ron`

Update floor ranges and group sizes for the 8 Phase 2 monsters to match
ENEMIES.md. Don't delete other spawn entries — just ensure these 8 are correct.

- [ ] **Step 1: Update spawn entries**

Match these to ENEMIES.md:

| Monster | Floors | Group Size |
|---------|--------|-----------|
| Giant Rat | 1-8 | 1-2 (floors 1-3), 2-3 (4-5), 3-4 (6-8) |
| Giant Bat | 1-6 | 1 (1-2), 1-2 (3-4), 1-3 (5-6) |
| Wolf | 3-10 | 1-2 (3-5), 2-3 (6-8), 3-4 (9-10) |
| Fire Salamander | 3-9 | 1 (3-5), 1-2 (6-9) |
| Goblin | 1-10 | 1-3 |
| Goblin Archer | 2-12 | 1-2 |
| Goblin Brute | 5-14 | 1 |
| Cave Bear | 6-12 | 1 |

For monsters with floor-scaled group sizes, create multiple spawn entries with
different floor ranges and group sizes (the spawner already supports this).

- [ ] **Step 2: Run cargo check**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add assets/monster_spawns.ron
git commit -m "feat(spawns): update spawn table for 8 core monsters per ENEMIES.md"
```

---

### Task 8: Smoke Test

No code changes. Verify behaviors in-game.

- [ ] **Step 1: Run tests**

Run: `cargo test -p bevy_rpg`
Expected: All tests pass.

- [ ] **Step 2: Run the game**

Run: `cargo run`

- [ ] **Step 3: Verify behaviors**

1. Find goblins — do they flee when wounded?
2. Find goblin archers — do they retreat when you close distance?
3. Find giant bats — do they move erratically (not straight toward you)?
4. Find a cave bear — does it give up chasing after ~8 tiles?
5. Find fire salamanders — do they apply burning on hit?
6. Find wolf packs — does alerting one alert the whole pack?
7. Kill monsters — does essence equal their HP?

- [ ] **Step 4: Document any issues**

---

## Summary

| Task | What Changes | Risk |
|------|-------------|------|
| 1 | Behavior flags on MonsterAsset + MonsterAI | Low — additive, defaults safe |
| 2 | Pure AI decision functions + 10+ tests | Low — new file, no breakage |
| 3 | Fleeing AI in hunting loop | Medium — modifies AI execution path |
| 4 | Kiting AI for ranged monsters | Medium — modifies AI execution path |
| 5 | Erratic movement + chase leash | Medium — modifies AI execution path |
| 6 | Update 8 monster definitions | Low — data changes only |
| 7 | Update spawn table | Low — data changes only |
| 8 | Smoke test | None — verification only |

Tasks 3-5 all modify `ai.rs` and should be done sequentially.
Tasks 6-7 are data-only and can run in parallel with earlier tasks.
