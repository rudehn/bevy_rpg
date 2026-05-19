# Player Design

## Overview

> **As of Phase 2:** the player picks a **race** (Human / Dwarf / Elf)
> and a **class** (Warrior / Rogue / Mage / Ranger) at character
> creation. Attribute scores are fully **race + class sum** — there is
> no chargen allocation step. Three attributes: **STR / DEX / INT**
> (CON is gone; HP scales from race + level). Players grow into
> positive modifiers through XP / levels: a racial gain schedule fires
> every 4 levels, and dedicated player-choice ASIs at levels 3 / 9 /
> 15 / 21 / 27 give +2 free points each. See [CHARACTER.md](CHARACTER.md)
> for the full character system: race/class tables, HP formula,
> stat-gain rules.
>
> Equipment and shrines drive run-to-run progression on top of the
> character baseline. This doc describes the *post-character-creation*
> stat shape that gear and shrines modify.

## Base Stats

Starting values below assume **Human Warrior** — the default character.
Other race × class combos produce different starting numbers. See
[CHARACTER.md](CHARACTER.md) §Races and §Classes for the full tables.

| Stat | Default (Human Warrior, L1) | Increased By |
|------|---|--------------|
| HP | 13 (`floor(1.00 × (8 + 11×1/2))`) | Level (HP formula), equipment, shrines |
| Mana | 10 | Equipment, shrines (full mana system deferred to Phase 4) |
| Hit Bonus | 0 at spawn (attribute mod added dynamically per hit) | Equipment, shrines (DEX_mod for ranged or finesse melee; STR_mod for brute melee — see Combat below) |
| Dodge Bonus | 0 (DEX_mod 0) | DEX_mod, equipment, shrines |
| Armor | 1 (from Padded Armor starting kit) | Equipment, shrines |
| Damage | 1d6 (from Rusted Shortsword, finesse → DEX_mod) | DEX_mod (ranged or finesse melee) / STR_mod (brute melee) at runtime, weapon, equipment, shrines |
| Action Delay | 1.0x (baseline) | Equipment, shrines |
| Vision Range | 8 tiles (10 for Elf via Keen Senses; min 4) | Race trait, equipment, shrines |
| Spell Slots | 1 | Shrines |
| Level | 1 (cap 27) | XP from kills |
| Experience | 0 | XP from kills (slow-then-fast cubic curve) |

**Attribute mods are applied dynamically.** Combat math reads the
player's `Attributes`, the `AttackIntentMessage.source`, and the
equipped weapon's `weapon_skill` tag at hit-check and damage-roll time.
Selection rule:

- **Ranged** attack → DEX_mod
- **Melee + finesse weapon** (Short Blades or Long Blades) → DEX_mod
- **Melee + any other weapon** (Axes, fists, staff bash) → STR_mod
- Spell / Environment → 0 (staves add INT_mod separately)

The static `HitBonus` and `DamageBonus` components carry only
equipment contributions. See [CHARACTER.md](CHARACTER.md) §Combat
Math Integration.

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
Attacker rolls: d20 + hit_bonus + attribute_bonus
              + weapon_skill_bonus + fighting_melee_bonus
Target number:  4 + target_dodge_bonus

If roll >= target: hit
If roll < target:  miss (turn still consumed)
If natural 20:     critical hit (always hits)
```

`attribute_bonus` is added dynamically based on
`AttackIntentMessage.source` and the equipped weapon's `weapon_skill`:
- **Ranged:** DEX_mod
- **Melee, finesse weapon** (Short Blades / Long Blades): DEX_mod
- **Melee, any other weapon** (Axes / fists / staff bash): STR_mod
- **Spell / Environment:** 0 (staff zaps add INT_mod + Evocations skill
  bonus separately in `handle_zap_staff`)

Phase 3 skill bonuses (also dynamic):
- `weapon_skill_bonus` = `floor(skill/4)` for the weapon family
  (Long Blades on a Sword, Ranged Weapons on any ranged attack, etc.)
- `fighting_melee_bonus` = `floor(Fighting/4)` on melee attacks only

The player and monsters use the same formula structure, but monsters
have no `Attributes` or `Skills` components and contribute 0 from both
helpers. See [CHARACTER.md](CHARACTER.md) §Combat Math Integration and
[SKILLS.md](SKILLS.md) for the full breakdown.

### Critical Hits

- Triggered by a **natural 20** on the d20 attack roll
- Always hits regardless of target's dodge
- **Double damage dice** — roll damage twice, then apply armor and resistances
  normally. Resistances are never bypassed.
- Future: skills/abilities may expand crit range to 18-20

### Melee Attack

1. Player bumps into enemy — triggers melee intent
2. Hit check: `d20 + hit_bonus + attr_mod >= 4 + target_dodge_bonus`
   - `attr_mod` = DEX_mod for finesse weapons (Short/Long Blades),
     otherwise STR_mod
3. On hit: `damage = weapon_dice + damage_bonus + attr_mod`, reduced
   by target armor and resistance
4. On miss: 0 damage, turn consumed

### Ranged Attack

- Requires bow equipped and arrows in off-hand quiver
- Arrows are consumed on each shot
- Range limited (default: 8 tiles)
- Uses the same d20 hit formula but with **DEX_mod** for both hit and
  damage regardless of weapon. A DEX-focused Ranger thus scales their
  bow shots even with a low STR.

### Damage Pipeline

```
AttackIntentMessage { source: Melee | Ranged | Spell | Environment }
  -> hit_check_system
      roll d20
      finesse       = matches!(weapon.weapon_skill, ShortBlades | LongBlades)
      attr_bonus    = attack_attribute_bonus(source, finesse, attacker_attrs)
                      # Ranged: DEX_mod
                      # Melee + finesse: DEX_mod
                      # Melee, non-finesse: STR_mod
                      # else: 0
      weapon_bonus  = weapon_skill_bonus(weapon, source, attacker_skills)
                      # floor(skill/4) for the equipped weapon's family
      fighting_bonus = fighting_melee_bonus(source, attacker_skills)
                      # floor(Fighting/4), melee only
      if roll == 20: is_critical = true, auto-hit
      else: d20 + hit_bonus + attr_bonus + weapon_bonus + fighting_bonus
              >= 4 + target_dodge_bonus
      -> DamageRollMessage { is_critical, damage_type, source }
      # Successful hits also bump use counters for the trained skills

  -> damage_roll_system
      attr_bonus     = attack_attribute_bonus(source, finesse, attacker_attrs)
      weapon_bonus   = weapon_skill_bonus(weapon, source, attacker_skills)
      fighting_bonus = fighting_melee_bonus(source, attacker_skills)
      if is_critical: roll damage dice x2 + damage_bonus + attr_bonus + weapon_bonus + fighting_bonus
      else: roll damage dice + damage_bonus + attr_bonus + weapon_bonus + fighting_bonus
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

- **Starting HP:** derived from race + level. At L1: Dwarf 16, Human 13,
  Elf 12. Formula: `floor(race_hp_mod × (8 + 11 × XL / 2))`. Recomputed
  from scratch on every level-up; equipment HP bonuses layer on top.
- **Level-up heals to full** (DCSS default).
- **Regen:** HP regenerates slowly over time.
- **Regen suppression:** Regen is suppressed for 5 turns after taking damage.
  This creates a "recover between fights" pacing — the player heals up in
  corridors, not mid-combat.

## XP & Levels

- Player gains XP from killing monsters. Each monster declares a `tier`
  in `monsters.ron` (currently all default to 1; a balancing pass will
  set per-monster tiers).
- **Anti-farming:** XP reward scales by `player_level - monster_tier`:
  full XP within ±2 levels, 50% at +4, **0 XP at +5 or more**. Killing
  a tier-1 sewer rat as a level 6 character gives nothing.
- **Punching up bonus:** killing a monster ≥3 levels above you gives
  1.5× XP.
- **Level cap 27** (DCSS). Slow-then-fast cubic curve: ~150 XP to L2,
  ~2,000 to L5, ~20,000 to L10, ~90,000 to L27.
- **Stat-gain on level-up:**
  - **Racial schedule** fires every 4 levels (L4, 8, 12, 16, 20, 24):
    +1 to one of the race's allowed attributes (Human and Dwarf can
    pick any of S/D/I; Elf can pick D or I, never S).
  - **Player-choice ASIs** at L3, 9, 15, 21, 27: +2 free points to
    spend across any attribute.
  - When a level-up triggers a prompt, the game enters
    `InGameState::AsiSelect` (DCSS-style inline modal) — press
    `S` / `D` / `I` to spend a point.

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
