# Combat: Damage Types, Resistances & Critical Hits

## Overview

Expands the combat pipeline from a flat "roll dice → subtract armor → apply" system into a typed system with Physical and Fire damage, per-type resistances (including healing on extreme resistance), and critical hits that bypass all defenses.

---

## Critical Hits

- Triggered by a **natural 20** on the d20 attack roll
- Applies to both player and monsters
- Effect: **bypass all defenses** — armor and resistance are ignored; raw damage = final damage
- This is more interesting than "2× damage": feels especially rewarding against high-armor targets and is type-agnostic
- Log: uses `"critically hits"` / `"critically hit"` verb to distinguish from normal attacks

---

## Damage Types

Two types for now; enum is extendable:

| Type | Default? | Armor applied? | Resistance applied? |
|------|----------|---------------|---------------------|
| `Physical` | Yes | Yes (flat) | Yes (%) |
| `Fire` | No | No | Yes (%) |

**Physical** hits the full reduction chain: flat armor first, then percentage resistance.
**Fire** skips flat armor entirely — only fire resistance applies. This makes fire meaningfully distinct, not just a reskin.

---

## Resistances

`Resistances` component with `physical: i32` and `fire: i32` fields (percentage values):

| Value | Effect |
|-------|--------|
| 0 | No resistance (default) |
| 50 | 50% damage reduction |
| 100 | Immune — `final = raw × (1 - 1.0) = 0`, no damage and no heal |
| >100 | Heals — `final = raw × (1 - 1.5) = negative`, emit HealMessage |
| Negative | Vulnerability — takes extra damage |

Branch logic:
- `final < 0` → emit `HealMessage { entity: target, amount: final.abs() }`
- `final == 0` → silent immunity (no damage, no heal, no log message)
- `final > 0` → emit `ApplyDamageMessage { final_damage: final }`

Applies to both monsters and players. Players default to 0 in all types and can gain resistances through equipment or spells in future milestones.

---

## RON Asset Format

### `MonsterAsset` new fields (all optional, default to 0 / Physical)

```ron
"Fire Elemental": (
    name: "Fire Elemental",
    sprite: "...",
    damage: "2d6",
    damage_type: Fire,       // Optional — defaults to Physical
    fire_resist: 100,        // Immune to fire
    physical_resist: -25,    // Vulnerable to physical
    ...
),

"Ember Drake": (
    name: "Ember Drake",
    sprite: "...",
    damage: "1d8+2",
    damage_type: Fire,
    fire_resist: 150,        // Heals from fire
    ...
),
```

Existing monsters with no `damage_type` / `physical_resist` / `fire_resist` fields work unchanged — all default correctly.

### `ItemAsset` new field

```ron
"Flameblade": (
    name: "Flameblade",
    sprite: "...",
    item_kind: Weapon,
    damage: Some("1d8+2"),
    damage_type: Fire,       // Optional — defaults to Physical
    rarity: Rare,
),
```

---

## Combat Pipeline After Change

```
AttackIntentMessage
  → hit_check_system
      roll 1d20
      if roll == 20: is_critical = true, auto-hit (bypass hit_target)
      else: normal hit check (final_hit_score >= hit_target)
      → DamageRollMessage { is_critical, damage_type }

  → damage_roll_system
      roll damage.dice + damage_bonus
      → DamageReductionMessage { raw_damage, is_critical, damage_type }

  → damage_reduction_system
      if is_critical:
          final = raw_damage              // bypass all reductions
      else if Physical:
          after_armor = (raw - armor).max(1)
          final = round(after_armor × (1.0 - physical_resist / 100.0))
      else if Fire:
          final = round(raw × (1.0 - fire_resist / 100.0))

      if final < 0  → HealMessage (absorbed and healed)
      if final == 0 → return silently (immune)
      else          → ApplyDamageMessage { final_damage: final, is_critical }

  → damage_application_system
      log "critically hits" or "hits"
      apply health change; check death
```

---

## Files Changed

| File | Change |
|------|--------|
| `src/game/combat.rs` | Add `DamageType` enum, `Resistances` component; update `Damage`, all messages, `hit_check_system`, `damage_roll_system`; rename `armor_reduction_system` → `damage_reduction_system` with new logic; update log verb in `damage_application_system` |
| `src/assets/mod.rs` | Add `damage_type`, `physical_resist`, `fire_resist` to `MonsterAsset`; add `damage_type` to `ItemAsset` |
| `src/game/items.rs` | Add `damage_type: DamageType` to `ItemProperties` |
| `src/game/spawner.rs` | Wire `damage_type` and `Resistances` into monster/item spawn |
| `src/ui/mod.rs` | `damage.0` → `damage.dice` in tooltip |
| `src/ui/character_info.rs` | Same `.0` → `.dice` fix |
| `assets/monsters.ron` | No changes required (all default to Physical / 0 resist) |
| `assets/items.ron` | No changes required |

---

## Future Extensions

- **More damage types**: Cold, Lightning, Acid — add variants to `DamageType`, fields to `Resistances`
- **Status effects on crit**: Physical crit could apply a bleed; Fire crit could ignite (once status effect system lands)
- **Player resistances via equipment**: gear already has the `ItemProperties` structure; just expose `fire_resist` / `physical_resist` fields and sum them into the player's `Resistances` component during stat recalc
- **Penetration**: high-level weapons could have a `ResistancePenetration` component that ignores a portion of target resistance
