# Player Design

## Overview

The player has no attribute stats (no STR, DEX, CON, etc). All character growth
comes from **equipment** and **shrines**. The player starts as a blank slate with
base values and becomes powerful through what they find and how they spend essence.

## Base Stats

| Stat | Starting Value | Increased By |
|------|---------------|--------------|
| HP | 25 | Equipment, shrines |
| Mana | 10 | Equipment, shrines |
| Hit Bonus | 0 | Equipment, shrines |
| Dodge Bonus | 0 | Equipment, shrines |
| Armor | 0 | Equipment, shrines |
| Damage | 1d2 (unarmed) | Weapon equipped |
| Action Delay | 1.0x (baseline) | Equipment, shrines |
| Vision Range | 8 tiles (min 4) | Equipment, shrines |
| Spell Slots | 1 | Shrines |

## Equipment Slots

9 slots total:

| Slot | Examples |
|------|---------|
| Weapon | Sword, Dagger, Staff, Bow |
| Off-hand | Shield, Quiver, Focus Orb |
| Helm | Iron Helm, Leather Cap |
| Chest | Plate Armor, Robe, Chainmail |
| Gloves | Gauntlets, Leather Gloves |
| Boots | Iron Boots, Soft Boots |
| Ring (x2) | Two ring slots |
| Amulet | One amulet slot |

See ITEMS.md for full equipment details.

## Combat

### Hit Check (d20)

```
Attacker rolls: d20 + hit_bonus
Target number:  4 + target_dodge_bonus

If roll >= target: hit
If roll < target:  miss (turn still consumed)
If natural 20:     critical hit (always hits)
```

Both player and monsters use this formula (symmetric combat).

### Critical Hits

- Triggered by a **natural 20** on the d20 attack roll
- Always hits regardless of target's dodge
- **Double damage dice** — roll damage twice, then apply armor and resistances
  normally. Resistances are never bypassed.
- Future: skills/abilities may expand crit range to 18-20

### Melee Attack

1. Player bumps into enemy — triggers melee intent
2. Hit check: `d20 + hit_bonus >= 4 + target_dodge_bonus`
3. On hit: `damage = weapon_dice + damage_bonus`, reduced by target armor and
   resistance
4. On miss: 0 damage, turn consumed

### Ranged Attack

- Requires bow equipped and arrows in off-hand quiver
- Arrows are consumed on each shot
- Range limited (default: 8 tiles)
- Uses the same d20 hit formula

### Damage Pipeline

```
AttackIntentMessage
  -> hit_check_system
      roll d20
      if roll == 20: is_critical = true, auto-hit
      else: d20 + hit_bonus >= 4 + target_dodge_bonus
      -> DamageRollMessage { is_critical, damage_type }

  -> damage_roll_system
      if is_critical: roll damage dice x2 + damage_bonus
      else: roll damage dice + damage_bonus
      -> DamageReductionMessage { raw_damage, damage_type }

  -> damage_reduction_system
      if Physical:
          after_armor = (raw - armor).max(0)
          final = round(after_armor * (1.0 - physical_resist / 100.0))
      else if Fire/Lightning/Necrotic:
          final = round(raw * (1.0 - type_resist / 100.0))

      if final < 0  -> HealMessage (absorbed and healed)
      if final == 0 -> silent (immune)
      else          -> ApplyDamageMessage { final_damage, is_critical }

  -> damage_application_system
      apply HP change, check death
```

### Damage Types

| Type | Armor Applied? | Resistance Applied? | Unique Property |
|------|---------------|---------------------|-----------------|
| Physical | Yes (flat subtraction) | Yes (%) | Standard; most melee and basic spells |
| Fire | No | Yes (%) | Destroys wooden doors; all DoT (burning) is fire-based |
| Lightning | No | Yes (%) | Can chain jump to additional enemies |

Physical hits the full reduction chain: flat armor first, then percentage
resistance. Fire, Lightning, and Necrotic skip flat armor — only their
respective resistance applies.

### Resistances

Percentage-based, stored per damage type (default 0 for all):

| Value | Effect |
|-------|--------|
| 0 | No resistance (default) |
| 50 | 50% damage reduction |
| 100 | Immune (0 damage, no heal) |
| >100 | Heals (damage absorbed and converted to healing) |
| Negative | Vulnerability (takes extra damage) |

Applies symmetrically to player and monsters.

## Health & Regen

- **Starting HP:** 25
- **Regen:** HP regenerates slowly over time
- **Regen suppression:** Regen is suppressed for 5 turns after taking damage.
  This creates a "recover between fights" pacing — the player heals up in
  corridors, not mid-combat.

## Mana

- **Starting mana:** 10
- **Mana regen:** 1 per turn (not suppressed by damage)
- **Spell slots:** Start with 1. Additional slots unlocked via shrines
  (max 6 total).

See SPELLS.md for the spell system.

## Speed & Turn Order

```
action_delay = base_cost * delay_multiplier
delay_multiplier starts at 1.0
```

The player's delay multiplier is fixed at 1.0 unless modified by equipment or
shrines. Lower delay = more turns per cycle. Feeds into the TurnManager queue
where all actors (player and monsters) are sorted by game time.

## Symmetric Combat

Player and monsters share the same components and combat formulas. The damage
pipeline, hit checks, resistances, and speed system work identically for all
entities. The only structural differences:

- **HP source:** Player has a flat base (25); monsters have per-type base HP
- **Mana:** Only player has a mana pool
- **Spell slots:** Player-only feature
- **Equipment:** Only player equips gear; monsters have stats baked in

Status effects, debuffs, and damage types work the same regardless of target.

## Death

On death, show:
- Floor reached
- Essence collected
- Equipment carried
- Shrines purchased
- Cause of death
- Enemies killed

## Resolved Decisions

- **Armor can fully negate damage** — `(raw - armor).max(0)`, not `.max(1)`
- **Mana regen is modifiable** — 1/turn baseline, increased by shrines/equipment
- **Vision minimum is 4 tiles** — no effects currently lower vision
- **Single hit bonus** — same hit_bonus for melee and ranged
