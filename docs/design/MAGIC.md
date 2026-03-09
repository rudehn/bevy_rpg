# Magic System Design

## Overview

Magic in this game is not a class feature — any hero can learn and use spells. Spells are found as **spellbooks** in the dungeon, learned by reading them, and equipped into a limited number of **active spell slots**. Casting costs **mana**, which regenerates slowly over time.

A pure melee hero can ignore magic entirely. A hybrid can carry 2-3 utility spells. A dedicated caster builds INT, fills all slots, and relies heavily on their spell arsenal.

## Mana

| Property | Value |
|----------|-------|
| Max Mana | `INT × 5` (e.g., INT 8 = 40 mana) |
| Regen Rate | 1 mana per 5 turns (passive) |
| Staff bonus | +2 mana regen per turn when a staff is equipped |
| Mana Potion | Restore 15 mana (instant) |

Mana does **not** regenerate in combat — only on non-combat turns. (May revisit: slow regen in combat could work too.)

## Spell Slots

Spell slots are unlocked as the player levels up:

| Level | Slots Total |
|-------|-------------|
| 1 | 1 |
| 3 | 2 |
| 5 | 3 |
| 8 | 4 |
| 11 | 5 |
| 14 | 6 |

The player can equip any known spell into any slot. Swapping spells is done from the inventory screen and takes no turn (between-combat management only — no mid-combat swapping).

## Acquiring Spells

1. Find a **spellbook** (Tome / Grimoire) in the dungeon as loot
2. **Read** the spellbook from inventory
3. The spell is permanently added to the player's **known spells list**
4. Open the spell management screen and drag the spell into an active slot

Known spells are never lost — they persist through item changes and floor transitions. Only the active slots limit what can be cast at once.

## Spell Power

Spell power governs damage and effect magnitude:
```
spell_power = INT + (equipped focus orb bonus)
damage = spell_base_damage + (spell_power × spell_scaling)
```

## Spell List

### Damage Spells

| Spell | Mana Cost | Effect | Scaling |
|-------|-----------|--------|---------|
| Magic Missile | 5 | Deal 1d6 + spell_power force damage to one target | 0.5× INT |
| Fireball | 12 | 3d6 fire damage in 3-tile radius. Hits allies. | 0.8× INT |
| Freeze | 8 | 1d8 cold damage + stun target for 2 turns | 0.6× INT |
| Lightning Bolt | 10 | 2d6 lightning damage, chains to adjacent enemies (1 chain) | 0.7× INT |
| Soul Drain | 14 | 2d8 necrotic damage, heal for 50% of damage dealt | 0.6× INT |
| Smite | 8 | 1d10 radiant damage, +50% vs undead and demons | 0.5× INT |

### Utility Spells

| Spell | Mana Cost | Effect | Duration |
|-------|-----------|--------|----------|
| Blink | 8 | Teleport to any visible tile within 8 tiles | Instant |
| Detect Enemies | 6 | Reveal all enemy positions on current floor | 10 turns |
| Slow | 10 | Target enemy moves at 50% speed | 8 turns |
| Confusion | 12 | Target enemy wanders randomly | 6 turns |
| Phase Door | 15 | Pass through one wall tile (teleport to other side) | Instant |

### Buff Spells

| Spell | Mana Cost | Effect | Duration |
|-------|-----------|--------|----------|
| Mage Armor | 10 | +4 DEF (stacks with equipment) | 15 turns |
| Haste | 12 | -25% turn delay | 10 turns |
| Healing Word | 8 | Restore 2d6 + spell_power HP | Instant |
| Mana Surge | 6 | Next spell cast costs 0 mana | 1 spell |
| Invisibility | 12 | Enemies lose detection for 12 turns | 12 turns |

### Summon Spells

| Spell | Mana Cost | Effect | Duration |
|-------|-----------|--------|----------|
| Summon Familiar | 15 | Summon a spirit familiar that attacks nearby foes | 20 turns |
| Animate Bone | 18 | Raise a fallen skeleton as a temporary ally | 15 turns |
| Call Lightning | 20 | Summon a storm that strikes a random enemy each turn | 8 turns |

## Spellbook Availability by Floor

| Spell Tier | First Available |
|------------|----------------|
| Tier 1 (low cost, simple) | Floor 1+ |
| Tier 2 (medium cost, utility) | Floor 3+ |
| Tier 3 (high cost, powerful) | Floor 6+ |
| Tier 4 (very high cost, game-changing) | Floor 8+ |

Spellbooks are uncommon loot — expect to find 0-2 per floor on average.

## Design Notes

- **Mana scarcity is the primary constraint.** Players shouldn't be able to nova every fight.
- **Spell slots create meaningful choices.** A player must decide which 3 (or 6 at endgame) spells define their run.
- **Staff builds are a supported archetype.** Staff + high INT + mana regen ring = sustainable caster.
- **No spell leveling** — spells don't level up. Power comes from INT stat growth and finding better spells.
- **Friendly fire on Fireball** is intentional. Positional play should matter.
