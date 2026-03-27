# Phase 3: Loot & Growth — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make finding loot exciting. All items come from chests, weapon/armor variety covers all slots, rarity scales with floor depth, and mana potions exist.

**Architecture:** The item system is already mature — 9 equipment slots, equip/unequip with stat application, rarity enum, chest interaction, consumable stacking, ammo system. We're adding content (items in RON files) and two system changes (chest-based item spawning, mana potion effect).

**Tech Stack:** Rust, Bevy 0.17, RON assets

**Reference Docs:**
- `docs/design/ITEMS.md` — Weapon/armor/consumable tables, rarity weights per floor

**Current State:**
- 25 items defined (4 weapons, 2 armor, 2 potions, 1 ring, 1 ammo, 15 spellbooks)
- ItemSpawner places loose items on room floors (1 per room)
- Chests exist as props in prefabs, drop 1-3 random items on open
- Equipment slots all work (weapon, offhand, helm, chest, gloves, boots, 2 rings, amulet)
- Rarity enum exists but doesn't affect drop rates by floor
- Only HealHp effect for consumables (no mana restore)

---

## File Map

| File | Role | Change Type |
|------|------|-------------|
| `assets/items.ron` | Item definitions | Modify (add ~20 items) |
| `assets/item_spawns.ron` | Spawn table with floor ranges and weights | Modify |
| `src/map/builders/item_spawner.rs` | Spawns chests instead of loose items | Modify |
| `src/game/items.rs` | Add RestoreMana effect | Modify |
| `src/game/actions.rs` | Handle RestoreMana in use_item | Modify |

---

### Task 1: Add Mana Potion Effect

**Files:**
- Modify: `src/game/items.rs` (or wherever `Effect` enum is defined)
- Modify: `src/game/actions.rs` (or wherever `handle_use_item` processes effects)

Add a `RestoreMana(i32)` variant to the `Effect` enum so mana potions work.

- [ ] **Step 1: Find the Effect enum**

Search for `enum Effect` in `src/game/items.rs` or `src/game/effects.rs`. Add:
```rust
RestoreMana(i32),
```

- [ ] **Step 2: Handle RestoreMana in use_item**

Find `handle_use_item` or wherever `Effect::HealHp` is handled. Add a case for `RestoreMana`:
```rust
Effect::RestoreMana(amount) => {
    if let Some(mut mana) = mana_query.get_mut(user_entity) {
        mana.current = (mana.current + amount).min(mana.max);
    }
    log_writer.write(GameLogMessage(format!("You restore {} mana.", amount)));
}
```

Make sure the system has access to a `Mana` query. Check how `HealHp` accesses `Health` and follow the same pattern.

- [ ] **Step 3: Write a test for RestoreMana effect logic**

Add a test that verifies mana restoration caps at max:
```rust
#[test]
fn restore_mana_caps_at_max() {
    let current = 5;
    let max = 10;
    let amount = 15;
    let result = (current + amount).min(max);
    assert_eq!(result, 10);
}
```

- [ ] **Step 4: Run cargo check and tests**

Run: `cargo check && cargo test -p bevy_rpg`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(items): add RestoreMana effect for mana potions"
```

---

### Task 2: Add Weapon Variety

**Files:**
- Modify: `assets/items.ron`
- Modify: `assets/item_spawns.ron`

Add missing weapon types from ITEMS.md. The current items.ron has Iron Sword, Steel Sword, Short Bow, Long Bow. We need Dagger, Axe, Mace, Staff, Crossbow, and rarity variants.

- [ ] **Step 1: Add weapons to items.ron**

Follow the existing item format exactly. Add these weapons:

```
Dagger:        1d4, Common,   weapon_range: 0 (melee)
Long Sword:    1d8, Common    (already exists as "Steel Sword" — rename or keep)
Axe:           1d8, Common    (note: armor penetration is a future mechanic, skip for now)
Great Axe:     2d6, Uncommon  (note in description: two-handed)
Mace:          1d6, Common
Staff:         1d4, Common
Crossbow:      1d10, Uncommon, weapon_range: 10
```

For each, match the field format of existing weapons. Check existing entries for field names and sprite conventions.

- [ ] **Step 2: Add weapon spawn entries to item_spawns.ron**

Add entries with floor ranges and weights:
```
Dagger:      floors 1-6, weight 4
Axe:         floors 3-12, weight 3
Mace:        floors 3-12, weight 3
Staff:       floors 1-10, weight 2
Great Axe:   floors 6-15, weight 2
Crossbow:    floors 5-15, weight 2
```

Keep existing Iron Sword, Steel Sword, Short Bow, Long Bow entries.

- [ ] **Step 3: Run cargo check**

Run: `cargo check`
RON parse errors show at compile time or runtime — also run the game briefly to verify assets load.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(items): add dagger, axe, mace, staff, great axe, crossbow weapons"
```

---

### Task 3: Add Armor for All Slots

**Files:**
- Modify: `assets/items.ron`
- Modify: `assets/item_spawns.ron`

Currently only 2 chest armor pieces exist. Add armor for helm, gloves, boots, and off-hand (shields) at 3 tiers each.

- [ ] **Step 1: Add armor items to items.ron**

Each armor piece needs: `item_kind: Armor`, `armor_slot`, `defense` value, `rarity`.

**Helms:**
```
Leather Cap:    defense 1, Common,   armor_slot: Helm
Iron Helm:      defense 2, Uncommon, armor_slot: Helm
Full Helm:      defense 3, Rare,     armor_slot: Helm
```

**Chest (add heavy tier, keep existing two):**
```
Plate Armor:    defense 5, Rare,     armor_slot: Chest
```

**Gloves:**
```
Leather Gloves: defense 1, Common,   armor_slot: Gloves
Splint Gloves:  defense 2, Uncommon, armor_slot: Gloves
Gauntlets:      defense 3, Rare,     armor_slot: Gloves
```

**Boots:**
```
Soft Boots:     defense 0, Common,   armor_slot: Boots (future: speed bonus)
Iron Boots:     defense 2, Uncommon, armor_slot: Boots
Heavy Boots:    defense 3, Rare,     armor_slot: Boots
```

**Shields (Off-hand):**
```
Wooden Shield:  defense 2, Common,   armor_slot: OffHand
Iron Shield:    defense 3, Uncommon, armor_slot: OffHand
Tower Shield:   defense 5, Rare,     armor_slot: OffHand
```

- [ ] **Step 2: Add armor spawn entries to item_spawns.ron**

Common armor: floors 1-10, weight 3
Uncommon armor: floors 4-15, weight 2
Rare armor: floors 8-20, weight 1

- [ ] **Step 3: Verify shield equip works**

Shields use `armor_slot: OffHand`. The Equipment component has an `offhand` slot. Verify that equipping a shield applies its defense to the player's Armor component. This should work via the existing equip logic — check that `handle_equip_item` reads `defense` for OffHand items.

If the equip system only applies defense for Chest armor, it needs to apply defense for ALL armor slots.

- [ ] **Step 4: Run cargo check and test equip in-game**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(items): add armor for all slots (helm, gloves, boots, shields)"
```

---

### Task 4: Add Mana Potions and Swiftness Potion

**Files:**
- Modify: `assets/items.ron`
- Modify: `assets/item_spawns.ron`

- [ ] **Step 1: Add potion items to items.ron**

```
Mana Potion:         effect: RestoreMana(15), max_stack: 5, Common
Greater Mana Potion: effect: RestoreMana(35), max_stack: 3, Uncommon
```

Follow the format of existing Healing Potion entries exactly.

- [ ] **Step 2: Add potion spawn entries to item_spawns.ron**

```
Mana Potion:         floors 1-20, weight 3
Greater Mana Potion: floors 6-20, weight 2
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(items): add mana potions with RestoreMana effect"
```

---

### Task 5: Convert Item Spawner to Chest-Based

**Files:**
- Modify: `src/map/builders/item_spawner.rs`

The design says all items come from chests, not loose on the floor. Change the ItemSpawner to place **chest props** at item locations instead of loose items. When the player opens a chest, it rolls items from the spawn table (this already works via `handle_open_chest`).

- [ ] **Step 1: Read the current ItemSpawner**

Understand how it places items. It currently puts `(Point, String)` pairs into `build_data.item_spawn_list`. These become loose items on the floor.

- [ ] **Step 2: Change ItemSpawner to place chests**

Instead of adding to `item_spawn_list`, add chest props to `prop_spawn_list`:
```rust
build_data.prop_spawn_list.push((spawn_point, "chest".to_string()));
```

This places a chest prop that the player can open. The chest opening logic (`handle_open_chest`) already rolls random items from the spawn table.

- [ ] **Step 3: Verify the chest prop exists**

Check `assets/props.ron` for a "chest" entry. It should be a blocking prop with a sprite.

- [ ] **Step 4: Adjust chest density**

Currently ItemSpawner places 1 item per room. Converting to chests, we might want fewer chests (not every room should have one). Consider a spawn chance per room (e.g., 40-60% chance of a chest per room).

- [ ] **Step 5: Run cargo check and test in-game**

Run: `cargo check`
Play the game: verify chests appear in rooms and drop items when opened.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(items): items now spawn in chests, not loose on floor"
```

---

### Task 6: Floor-Scaled Rarity Weights

**Files:**
- Modify: `src/map/builders/item_spawner.rs` or `src/game/actions.rs` (chest open logic)

When a chest is opened, the rarity of dropped items should scale with floor depth per ITEMS.md:

| Floors | Common% | Uncommon% | Rare% | Legendary% |
|--------|---------|-----------|-------|-----------|
| 1-5    | 70      | 24        | 5     | 1         |
| 6-10   | 55      | 32        | 11    | 2         |
| 11-15  | 40      | 38        | 18    | 4         |
| 16-20  | 25      | 40        | 27    | 8         |

- [ ] **Step 1: Find where chest items are rolled**

In `handle_open_chest` (likely `src/game/actions.rs`), find where random items are selected from the spawn table. This is where rarity filtering should happen.

- [ ] **Step 2: Add floor-based rarity weighting**

Read the current floor depth (from `Floor` resource). Use the rarity table above to weight item selection. The simplest approach:
1. Roll a rarity tier based on floor weights
2. Filter the spawn table to items of that rarity
3. Pick a random item from the filtered list

If no items of the rolled rarity exist in the spawn table for this floor, fall back to the next lower rarity.

- [ ] **Step 3: Write tests for rarity weight calculation**

```rust
#[test]
fn floor_1_mostly_common() {
    let weights = rarity_weights_for_floor(1);
    assert_eq!(weights.common, 70);
    assert_eq!(weights.uncommon, 24);
    assert_eq!(weights.rare, 5);
    assert_eq!(weights.legendary, 1);
}

#[test]
fn floor_18_mostly_uncommon_and_rare() {
    let weights = rarity_weights_for_floor(18);
    assert_eq!(weights.common, 25);
    assert_eq!(weights.rare, 27);
}
```

- [ ] **Step 4: Run cargo check and tests**

Run: `cargo check && cargo test -p bevy_rpg`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(items): floor-scaled rarity weights for chest drops"
```

---

### Task 7: Smoke Test

- [ ] **Step 1: Run tests**

Run: `cargo test -p bevy_rpg`

- [ ] **Step 2: Play the game**

1. Open chests on floor 1 — mostly Common items
2. Descend to floor 5+ — Uncommon items appear more
3. Equip a weapon — damage changes
4. Equip armor in each slot — Armor stat increases
5. Equip a shield — Armor increases
6. Use a Healing Potion — HP restored
7. Use a Mana Potion — Mana restored
8. Check inventory — stacking works for potions and arrows
9. Find different weapon types (dagger, axe, etc.)

---

## Summary

| Task | What Changes | Risk |
|------|-------------|------|
| 1 | RestoreMana effect | Low — add enum variant + handler |
| 2 | Weapon variety | Low — RON data only |
| 3 | Armor for all slots | Medium — may need equip logic fix for non-chest slots |
| 4 | Mana potions | Low — RON data only |
| 5 | Chest-based item spawning | Medium — changes spawner behavior |
| 6 | Floor-scaled rarity | Medium — new logic in chest opening |
| 7 | Smoke test | None |

Task 3 has a risk: the equip system might only apply defense for chest armor, not for all armor slots. Verify and fix if needed.
