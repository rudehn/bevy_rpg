# Phase 1: Core Combat Loop — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the combat system to match the design docs — d20+bonus hit formula, double-dice crits, percentage-based resistances, non-physical damage skips armor, and essence = monster base_hp.

**Architecture:** The combat pipeline already exists as a 4-stage message chain (hit_check → damage_roll → armor_reduction → damage_application). We're modifying the formulas inside each stage, not restructuring the pipeline. Pure helper functions get updated first with tests, then the systems that call them.

**Tech Stack:** Rust, Bevy 0.17, bracket-lib RNG

**Reference Docs:**
- `docs/design/PLAYER.md` — Hit formula, crit rules, damage pipeline, damage types, resistances
- `docs/design/ENEMIES.md` — Monster stats, essence drops
- `docs/design/SPELLS.md` — Status effect stacking rules

---

## File Map

| File | Role | Change Type |
|------|------|-------------|
| `src/game/combat.rs` | Hit check, damage roll, armor reduction, resistance, helpers, tests | Modify |
| `src/game/stats.rs` | Armor, Dodge components + new HitBonus component | Modify |
| `src/game/essence.rs` | Essence award formula | Modify |
| `src/game/magic.rs` | Burning status effect tick + reapplication | Modify |
| `src/game/abilities.rs` | BurningStrike applies Burning | Verify |
| `src/game/spawner.rs` | Wire HitBonus onto player and monsters | Modify |
| `src/player/mod.rs` | Player spawn: add HitBonus, set starting mana to 10 | Modify |
| `assets/player.ron` | Player starting stats (verify/update) | Verify |
| `assets/monsters.ron` | Monster resistance format (enum → percentage) | Modify |
| `src/assets/mod.rs` | MonsterAsset resistance field type | Modify |
| `src/save/mod.rs` | Resistance serialization (if format changes) | Verify |

---

### Task 1: Add HitBonus Component

**Files:**
- Modify: `src/game/stats.rs`
- Modify: `src/game/spawner.rs`
- Modify: `src/player/mod.rs`

- [ ] **Step 1: Add HitBonus component to stats.rs**

In `src/game/stats.rs`, add alongside existing `Armor` and `Dodge`:

```rust
/// Flat bonus added to the d20 attack roll.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct HitBonus(pub i32);
```

Ensure it has the same derives as `Armor` and `Dodge`.

- [ ] **Step 2: Add HitBonus to player spawn**

In `src/player/mod.rs`, in the player entity spawn chain, add `HitBonus(0)` alongside the existing `Armor` and `Dodge` inserts.

- [ ] **Step 3: Set player starting mana to 10**

In `src/player/mod.rs`, change the player's `Mana` component from `Mana { current: 0, max: 0 }` to `Mana { current: 10, max: 10 }`.

- [ ] **Step 4: Add HitBonus to monster spawn**

In `src/game/spawner.rs`, in `spawn_monster()`, add `HitBonus(0)` to the monster entity. Monsters currently have no hit bonus field in the asset — all monsters start at 0. A `hit_bonus` field on `MonsterAsset` can be added later when monsters need non-zero values.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add src/game/stats.rs src/game/spawner.rs src/player/mod.rs
git commit -m "feat(combat): add HitBonus component to player and monsters"
```

---

### Task 2: Update Hit Formula

**Files:**
- Modify: `src/game/combat.rs`

The current formula is `1d20 >= 2 + dodge`. The new formula is `d20 + hit_bonus >= 4 + dodge_bonus`. The crit (natural 20) always hits regardless.

- [ ] **Step 1: Update hit_check_system**

In `src/game/combat.rs`, modify `hit_check_system`:

1. Add `Option<&HitBonus>` to the attacker query
2. Change the formula:
   - Old: `let hit_target = 2 + dodge_val;` then `if hit_roll >= hit_target`
   - New: `let hit_bonus = attacker_hit_bonus.map(|h| h.0).unwrap_or(0);`
          `let dodge_target = 4 + dodge_val;`
          `let is_natural_20 = hit_roll == 20;`
          `if is_natural_20 || (hit_roll + hit_bonus >= dodge_target)`
3. Pass `is_natural_20` to the `DamageRollMessage` so the damage system knows it's a crit

- [ ] **Step 2: Add is_crit field to DamageRollMessage**

Add `pub is_crit: bool` to `DamageRollMessage`. Update the `hit_check_system` to set `is_crit: is_natural_20`. Update any other code that creates `DamageRollMessage` (spell damage, etc.) to set `is_crit: false`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/game/combat.rs src/game/stats.rs
git commit -m "feat(combat): update hit formula to d20 + hit_bonus >= 4 + dodge_bonus"
```

---

### Task 3: Update Crit to Double Damage Dice

**Files:**
- Modify: `src/game/combat.rs`

Currently crit is a separate 5% roll in `damage_roll_system` that applies 150% damage. Change to: crit is detected in hit_check (nat 20), passed via message, and doubles the damage dice roll.

- [ ] **Step 1: Update damage_roll_system to use is_crit from message**

In `damage_roll_system`:
1. Read `message.is_crit` instead of rolling a separate crit check
2. If `is_crit`: roll damage dice **twice** and sum them, then add any flat bonuses once
3. Remove the old `game_rng.0.roll_dice(1, 20) == 20` crit roll

```rust
let base_roll = roll_dice(&damage_dice.0, &mut game_rng.0);
let rolled_damage = if message.is_crit {
    base_roll + roll_dice(&damage_dice.0, &mut game_rng.0)
} else {
    base_roll
};
```

- [ ] **Step 2: Update apply_damage_multipliers to remove crit parameter**

Remove the `is_crit` parameter from `apply_damage_multipliers`. Crit is now handled by double dice, not a multiplier. Update all callers.

- [ ] **Step 3: Update existing tests**

In the `tests` module of `combat.rs`, update or remove tests that reference the old crit multiplier in `apply_damage_multipliers`. The function signature changed.

- [ ] **Step 4: Verify tests pass**

Run: `cargo test -p bevy_rpg`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/game/combat.rs
git commit -m "feat(combat): crits now double damage dice instead of 150% multiplier"
```

---

### Task 4: Rework Resistance System to Percentage-Based

**Files:**
- Modify: `src/game/combat.rs` (ResistanceLevel → percentage, apply_resistance, armor_reduction_system)
- Modify: `src/assets/mod.rs` (MonsterAsset resistance field type)
- Modify: `assets/monsters.ron` (resistance values)
- Modify: `src/game/spawner.rs` (resistance wiring)

This is the largest task. The resistance system changes from an enum (`Weak/Normal/Resistant/Immune/Absorb`) to `i32` percentages (`-50 = vulnerable, 0 = normal, 50 = resistant, 100 = immune, 150 = absorb`).

- [ ] **Step 1: Update Resistances component**

In `src/game/combat.rs`, replace:

```rust
// OLD
pub enum ResistanceLevel { Weak, Normal, Resistant, Immune, Absorb }
pub struct Resistances(pub HashMap<DamageType, ResistanceLevel>);
```

With:

```rust
// NEW
/// Per-entity resistance map. Values are percentages.
/// 0 = normal, 50 = 50% reduction, 100 = immune, >100 = heals.
/// Negative = vulnerability (takes extra damage).
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Resistances(pub HashMap<DamageType, i32>);

impl Resistances {
    pub fn get(&self, damage_type: &DamageType) -> i32 {
        self.0.get(damage_type).copied().unwrap_or(0)
    }
}
```

- [ ] **Step 2: Update apply_resistance helper**

Replace the old `apply_resistance(after_armor: i32, resistance: ResistanceLevel) -> i32` with:

```rust
/// Apply percentage-based resistance. Returns final damage.
/// Negative result means healing (absorb). Zero means immune.
pub fn apply_resistance(damage: i32, resist_percent: i32) -> i32 {
    let multiplier = 1.0 - (resist_percent as f32 / 100.0);
    (damage as f32 * multiplier).round() as i32
}
```

- [ ] **Step 3: Update armor_reduction_system for damage type routing**

In `armor_reduction_system`, change the logic so **only Physical damage** applies armor. Fire, Lightning, and Necrotic skip armor entirely:

```rust
let after_armor = if message.damage_type == DamageType::Physical {
    let armor_val = armor.map(|a| a.0).unwrap_or(0)
        + rally_buff.map(|r| r.armor_bonus).unwrap_or(0);
    compute_after_armor(message.raw_damage, armor_val)
} else {
    message.raw_damage // Non-physical skips armor
};

let resist_percent = resistances
    .map(|r| r.get(&message.damage_type))
    .unwrap_or(0);
let final_damage = apply_resistance(after_armor, resist_percent);
```

- [ ] **Step 4: Update resistance log messages**

Replace the old enum-based match with percentage-based logic:

```rust
if resist_percent >= 100 {
    // Immune
    log_writer.write(GameLogMessage(format!(
        "{} is immune to {} damage!", target_name.0, message.damage_type.name()
    )));
} else if resist_percent > 0 {
    // Resistant
    log_writer.write(GameLogMessage(format!(
        "{} resists the {} damage.", target_name.0, message.damage_type.name()
    )));
} else if resist_percent < 0 {
    // Vulnerable
    log_writer.write(GameLogMessage(format!(
        "{} is weak to {}!", target_name.0, message.damage_type.name()
    )));
}
// final_damage < 0 means absorb (heal) — handled in damage_application_system
```

- [ ] **Step 5: Update compute_after_armor to .max(0)**

Change `compute_after_armor`:
```rust
pub fn compute_after_armor(raw_damage: i32, armor: i32) -> i32 {
    (raw_damage - armor).max(0)  // Was .max(1)
}
```

- [ ] **Step 6: Update damage_application_system for heal-on-absorb**

If `final_damage < 0`, emit a `HealMessage` instead of applying damage. If `final_damage == 0`, skip silently (immune).

- [ ] **Step 7: Update MonsterAsset resistance format**

In `src/assets/mod.rs`, change the resistance field on `MonsterAsset` from `HashMap<String, String>` (enum names) to `HashMap<String, i32>` (percentages).

In `src/game/spawner.rs`, update the monster spawn to convert the new format into `Resistances(HashMap<DamageType, i32>)`.

- [ ] **Step 8: Update monsters.ron**

Change all resistance values in `assets/monsters.ron` from string enum names to integer percentages. For example:
- `"fire": "immune"` → `"fire": 100`
- `"fire": "resistant"` → `"fire": 50`
- `"fire": "absorb"` → `"fire": 150`
- `"physical": "weak"` → `"physical": -50`

- [ ] **Step 9: Update all tests**

In the `tests` module of `combat.rs`:
1. Update `compute_after_armor` tests: `armor_cannot_reduce_below_one` → `armor_can_reduce_to_zero`
2. Replace all `apply_resistance` tests with percentage-based versions:

```rust
#[test]
fn resistance_zero_is_normal() {
    assert_eq!(apply_resistance(10, 0), 10);
}

#[test]
fn resistance_50_halves_damage() {
    assert_eq!(apply_resistance(10, 50), 5);
}

#[test]
fn resistance_100_is_immune() {
    assert_eq!(apply_resistance(10, 100), 0);
}

#[test]
fn resistance_150_heals() {
    assert_eq!(apply_resistance(10, 150), -5);
}

#[test]
fn resistance_negative_50_is_vulnerable() {
    assert_eq!(apply_resistance(10, -50), 15);
}

#[test]
fn armor_reduces_damage() {
    assert_eq!(compute_after_armor(10, 3), 7);
}

#[test]
fn armor_can_reduce_to_zero() {
    assert_eq!(compute_after_armor(5, 100), 0);
}

#[test]
fn zero_armor_passes_through() {
    assert_eq!(compute_after_armor(8, 0), 8);
}
```

- [ ] **Step 10: Fix any remaining compilation errors**

Any code referencing `ResistanceLevel` enum variants needs updating. Search the codebase:
- `src/game/abilities.rs` may reference resistances
- `src/save/mod.rs` may serialize resistances
- Spell effects that apply resistance changes

Run: `cargo check`
Fix all errors.

- [ ] **Step 11: Run all tests**

Run: `cargo test -p bevy_rpg`
Expected: All tests pass.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "feat(combat): rework resistance to percentage-based, non-physical skips armor"
```

---

### Task 5: Update Essence Drop Formula

**Files:**
- Modify: `src/game/essence.rs`

Change essence awarded on kill from `max_hp / 2 + 5` to `base_hp` (which equals `max_hp` for monsters since they have no HP modifiers).

- [ ] **Step 1: Find and update the essence award formula**

In `src/game/essence.rs`, find `xp_for_kill` or the equivalent function and change:
- Old: `max_hp / 2 + 5`
- New: `max_hp` (the monster's max HP IS its base HP)

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/game/essence.rs
git commit -m "feat(progression): essence drops now equal monster base_hp"
```

---

### Task 6: Status Effect Stacking — Refresh Duration

**Files:**
- Modify: `src/game/magic.rs` or wherever Burning/Slowed/Stunned are applied

When a status effect is reapplied, it should refresh to whichever duration is longer. It should NOT stack intensity.

- [ ] **Step 1: Find where Burning is applied**

Search for code that inserts the `Burning` component on an entity. This is likely in `src/game/abilities.rs` (BurningStrike on-hit) and possibly in spell effect handlers.

- [ ] **Step 2: Update Burning application to refresh duration**

Instead of blindly inserting `Burning { damage_per_turn, turns_remaining }`, check if the entity already has `Burning`. If so, set `turns_remaining` to the max of existing and new:

```rust
if let Some(mut existing) = existing_burning {
    existing.turns_remaining = existing.turns_remaining.max(new_turns);
    // damage_per_turn stays the same (no stacking)
} else {
    commands.entity(target).insert(Burning { damage_per_turn, turns_remaining: new_turns });
}
```

- [ ] **Step 3: Apply same pattern to Slowed and Stunned**

Find where `Slowed` and `Stunned` are applied and use the same refresh-duration logic.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/game/abilities.rs src/game/magic.rs
git commit -m "feat(combat): status effects refresh duration on reapply, no stacking"
```

---

### Task 7: Smoke Test — Play the Game

No code changes. Verify the full loop works.

- [ ] **Step 1: Run the game**

Run: `cargo run`

- [ ] **Step 2: Verify combat**

1. Start a new game
2. Attack a monster — verify the hit message makes sense (d20 + bonus vs dodge target)
3. Check that nat 20 crits show "critically hits" and deal visibly more damage
4. Check that armor reduces physical damage (can go to 0)
5. Kill a monster — verify essence drops equal the monster's HP
6. Check that fire damage (if available via starting spells) skips armor

- [ ] **Step 3: Verify resistances**

If any monster has fire/physical resistances defined, attack it and verify the log messages show the correct resistance behavior.

- [ ] **Step 4: Document any issues found**

If anything is broken, fix it before moving to Phase 2.

---

## Summary

| Task | What Changes | Risk |
|------|-------------|------|
| 1 | Add HitBonus component | Low — additive, no breakage |
| 2 | Hit formula d20 + bonus | Low — isolated to hit_check_system |
| 3 | Crit = double dice | Low — change in damage_roll_system |
| 4 | Resistance rework + armor routing | **High** — touches many files, serialization |
| 5 | Essence = base_hp | Low — one formula change |
| 6 | Status effect refresh | Low — small logic addition |
| 7 | Smoke test | None — verification only |

Task 4 is the riskiest. Do a `cargo check` after every sub-step, not just at the end.

## Testing Gap (Retrospective)

Phase 1 updated existing tests for `compute_after_armor`, `apply_resistance`,
`apply_damage_multipliers`, and `xp_for_kill` but did NOT add tests for:

- Hit formula logic (d20 + hit_bonus >= 4 + dodge_bonus, nat 20 always hits)
- Crit double-dice behavior (hard to unit test since it involves RNG rolls)
- Non-physical damage skipping armor (tested via existing `armor_reduction_system` integration)
- Status effect refresh-on-reapply (no unit test for max-duration logic)
- Burning tick behavior

These gaps should be addressed in a follow-up testing task.
