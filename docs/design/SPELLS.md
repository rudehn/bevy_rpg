# Spells & Magic

## Overview

Any hero can learn and use spells — there are no class restrictions. Spells are
learned in two ways: **killing monster casters** (chance to drop their spellbook)
and **spell shrines** (spend essence to learn a specific spell). Casting costs
**mana**, which regenerates over time.

A melee-focused player might carry 1-2 utility spells. A dedicated caster fills
all 6 slots and relies on their spell arsenal. The system rewards engagement with
dangerous enemies and deliberate essence spending.

## Mana

| Property | Value |
|----------|-------|
| Starting Max Mana | 10 |
| Mana Regen | 1 per turn (baseline, not suppressed by damage) |
| Mana Growth | Equipment and shrines increase max mana and regen rate |
| Mana Potion | Restore 15 mana (instant, consumable) |

**Mana is scarce by design.** The starting pool of 10 means early spells must be
used carefully. Growth through shrines and equipment is the path to sustained
casting.

## Spell Slots

- **Start with 1 slot**
- Additional slots unlocked via shrines (max 6 total)
- The player can equip any known spell into any slot
- Swapping spells is done from the spellbook screen and costs no turn

## Acquiring Spells

### Monster Caster Drops

Monster casters have a **25% chance to drop a spellbook** on death. If a monster
knows multiple spells, each spell has an independent chance that sums to 25%
(e.g., 2 spells = 12.5% each, 3 spells = 8.3% each).

- The dropped spellbook teaches one of the monster's known spells
- If the player already knows all of the monster's spells, **nothing drops**
- Creates a "hunt the caster" dynamic — engaging dangerous enemies is rewarded
- Spellbooks are picked up and used from inventory (consumes the item, learns
  the spell permanently)
- **Monster drops ignore spell tiers.** A Goblin Shaman on floor 8 can drop a
  Tier 1 Magic Missile tome. Spell tiers control floor loot tables, not what
  monsters know.

### Spell Shrines

Spell shrines are found alongside regular shrines in remote dungeon locations.
They cost essence and teach a specific, visible spell. **Spell shrines count
against the 3-per-floor shrine budget.**

- The player **sees which spell** the shrine teaches before paying
- Competes with stat shrines for the player's essence budget
- Higher-tier spells cost more essence (exact numbers TBD)
- A spell shrine for a spell the player already knows is wasted placement —
  the player simply skips it

### No Duplicates

A spell is either known or not. The player cannot learn a spell they already know.

## Damage Types

Four damage types, each with distinct mechanical identity:

| Type | Armor Applied? | Resistance Applied? | Unique Property |
|------|---------------|---------------------|-----------------|
| Physical | Yes (flat subtraction) | Yes (%) | Standard; most melee and basic spells |
| Fire | No | Yes (%) | Destroys wooden doors; all DoT effects are fire-based (burning) |
| Lightning | No | Yes (%) | Can chain jump to additional enemies (range and count per spell) |
| Necrotic | No | Yes (%) | Dark magic; associated with the Tyrant and undead |

**Fire always destroys wooden doors** it hits, whether from a spell or a
fire-type weapon.

**All damage-over-time is fire-based.** There is no poison damage type. DoT
effects apply the Burning status (fire damage per turn).

**Lightning chains** where specified. Each spell defines how many jumps and at
what range. Chain targets are nearest enemies not already hit.

## Spell Targeting

| Target Type | Description | UI |
|-------------|-------------|-----|
| `Caster` | Affects the caster only | No targeting; instant cast |
| `Enemy` | Targets a visible enemy | Cursor targeting |
| `AllyOrSelf` | Targets an ally or self | Cursor targeting (friendlies + self) |

## Spell List

All spells have fixed damage/healing values. There is no spell power scaling.
Power progression comes from unlocking stronger spells on deeper floors.

### Attack Spells (Target: Enemy)

| Spell | Tier | Mana | CD | Damage | Type | Notes |
|-------|------|------|----|--------|------|-------|
| Spark | 1 | 3 | 0 | 1d4 | Lightning | Cantrip; no cooldown, spammable |
| Magic Missile | 1 | 5 | 4 | 2d4 | Physical | Reliable workhorse |
| Fire Dart | 1 | 8 | 3 | 2d6 | Fire | Bypasses armor; destroys wooden doors |
| Ignite | 2 | 12 | 6 | 1d4 + Burning(2/turn, 4 turns) | Fire | DoT; 10.5 total over 5 turns |
| Lightning Bolt | 3 | 20 | 6 | 3d6, chains to 1 enemy (1d6) | Lightning | Big nuke + splash |
| Fireball | 3 | 22 | 8 | 2d6 AoE, radius 1 (3x3) | Fire | Friendly fire; destroys wooden doors |
| Chain Lightning | 3 | 25 | 8 | 2d6 + 2 jumps (1d6 each, 3 tiles) | Lightning | Multi-target; jumps between enemies |
| Death Coil | 4 | 30 | 8 | 4d6 | Necrotic | Highest single-target damage |

### Healing Spells

| Spell | Tier | Target | Mana | CD | Heal | Notes |
|-------|------|--------|------|----|------|-------|
| Minor Heal | 1 | Caster | 4 | 2 | 1d4 | Cheap emergency patch |
| Heal | 2 | AllyOrSelf | 8 | 8 | 2d6 | Flexible main heal |
| Greater Heal | 3 | AllyOrSelf | 25 | 8 | 3d8 | Big emergency heal |

### Buff Spells

| Spell | Tier | Target | Mana | CD | Effect | Duration | Notes |
|-------|------|--------|------|----|--------|----------|-------|
| Enrage | 1 | Caster | 8 | 10 | +3 damage bonus | 6 turns | Short burst of power |
| Fortify | 1 | Caster | 8 | 12 | +3 armor | 10 turns | Defensive; long duration |
| Haste | 2 | AllyOrSelf | 10 | 12 | +50% speed (delay x 0.5) | 8 turns | Massive tactical value |

### Debuff Spells (Target: Enemy)

| Spell | Tier | Mana | CD | Effect | Duration | Notes |
|-------|------|------|----|--------|----------|-------|
| Weaken | 1 | 8 | 10 | -3 damage on target | 8 turns | Reduces enemy threat |
| Slow | 2 | 10 | 10 | -50% speed (delay x 1.5) | 8 turns | Trivializes fast enemies |
| Curse | 3 | 18 | 15 | -2 damage, -2 dodge, -2 armor | 10 turns | Multi-stat debuff |

### Utility Spells (Target: Caster)

| Spell | Tier | Mana | CD | Effect | Notes |
|-------|------|------|----|--------|-------|
| Teleport | 3 | 15 | 20 | Teleport to random safe tile on floor | Panic button; uncontrolled |

**Teleport safety:** Always lands on a walkable, non-lava, non-occupied tile.
Scroll of Teleport follows the same rule.

**Total: 18 spells** (8 attack, 3 heal, 3 buff, 3 debuff, 1 utility)

## Spellbook Availability by Floor Tier

| Tier | Floors | Available Spells |
|------|--------|-----------------|
| 1 | 1-5 | Spark, Magic Missile, Fire Dart, Minor Heal, Enrage, Fortify, Weaken |
| 2 | 6-10 | Ignite, Heal, Haste, Slow |
| 3 | 11-15 | Lightning Bolt, Fireball, Chain Lightning, Greater Heal, Curse, Teleport |
| 4 | 16-20 | Death Coil |

Spellbooks are uncommon — expect 0-2 spell acquisition opportunities per floor
across both monster drops and spell shrines.

## Monster Casters

Monsters cast spells using the same system as the player. They have fixed mana
pools, know specific spells, and obey cooldowns. Monster AI prioritizes:
1. Heal self if below 50% HP (if they have a heal spell)
2. Buff self if not already buffed and enemy is visible
3. Debuff player if in range and not already debuffed
4. Cast highest-damage available attack spell
5. Fall back to melee

| Monster | Mana | Spells | Drops |
|---------|------|--------|-------|
| Goblin Shaman | 20 | Magic Missile | Tome of Magic Missile (25%) |
| Imp | 15 | Fire Dart, Spark | One of: Fire Dart / Spark (12.5% each) |
| Orc Shaman | 30 | Fire Dart, Minor Heal | One of: Fire Dart / Minor Heal (12.5% each) |
| Vampire | 25 | Minor Heal | Tome of Minor Heal (25%) |
| Shadow Fiend | 30 | Weaken, Slow | One of: Weaken / Slow (12.5% each) |
| Ogre Mage | 40 | Lightning Bolt, Fire Dart | One of: Lightning Bolt / Fire Dart (12.5% each) |
| Lich | 60 | Lightning Bolt, Death Coil | One of: Lightning Bolt / Death Coil (12.5% each) |

## Fireball & Friendly Fire

Fireball damages **all entities** in its radius, including the caster and allies.
Positional play matters — casting fireball in a narrow corridor with the player
adjacent to enemies will hurt the player too. This is intentional and creates
meaningful tactical decisions about when and where to use AoE.

## Design Notes

- **Fixed damage keeps the system simple.** Power progression comes from finding
  better spells deeper in the dungeon, not from stat investment.
- **Two acquisition paths create variety.** Monster drops are reactive and
  opportunistic; spell shrines are deliberate and costly.
- **Spell slots are the real constraint.** With max 6 slots and 18 spells, the
  player must choose which spells define their run.
- **Cooldowns prevent spam.** The best spells have 6-10 turn cooldowns, forcing
  the player to mix spells or fall back to melee between casts.
- **Damage types create tactical depth.** Fire bypasses armor and burns through
  doors. Lightning chains between clustered enemies. Necrotic is rare and
  powerful. Physical is stopped by armor. Resistances on monsters create
  puzzle-like encounters (fire immune enemies force physical/lightning).
- **Haste/Slow are speed multipliers, not stat buffs.** +50%/-50% speed is
  applied after normal delay calculation.

## Status Effect Stacking

**General rule:** A new application of the same status effect **refreshes the
duration** to whichever is longer. Effects do not stack intensity.

- Slow (3 turns) + Slow (8 turns) = Slow with 8 turns remaining
- Burning (2/turn, 3 turns) + Burning (2/turn, 4 turns) = Burning 2/turn, 4 turns
- Haste (8 turns) + Haste (8 turns) = Haste with 8 turns remaining (no double speed)

Different status effects stack freely (Slow + Burning + Curse all apply).

## Open Questions

1. **Spell shrine cost curve** — Exact essence costs per tier TBD.
2. **Lightning chain range** — Is 3 tiles the right jump distance for all
   lightning chain effects?
3. **Necrotic unique property** — Fire destroys doors, lightning chains. Should
   necrotic have a unique mechanical property? (e.g., heals the caster for a
   portion, or prevents target healing)
