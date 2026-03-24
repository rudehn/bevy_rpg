# Items Design

## Overview

Items are found as loot throughout the dungeon. There are no shops, no
crafting, and no item identification. What you see is what you get. All
character power beyond shrines comes from equipment.

There are no stat requirements — any gear can be equipped by any player.

## Item Rarity Tiers

| Tier | Color | Drop Weight (floor 1) | Notes |
|------|-------|----------------------|-------|
| Common | White | 60% | Basic stats, no special effects |
| Uncommon | Green | 25% | +1 bonus property |
| Rare | Blue | 12% | +2 bonus properties, may have damage type |
| Legendary | Gold | 3% | Unique named items with special effects |

Rarity weights shift toward better tiers on deeper floors (see Item Generation).

## Equipment Slots

9 slots total. See PLAYER.md for the full slot list.

## Weapons

All weapons have: `base_damage`, `speed_modifier`, `range` (1 = melee).
No stat requirements.

### Melee Weapons

| Weapon | Base Damage | Speed Mod | Notes |
|--------|-------------|-----------|-------|
| Dagger | 1d4 | 0.8x (fast) | Quick strikes |
| Short Sword | 1d6 | 1.0x | Balanced |
| Long Sword | 1d8 | 1.1x | Standard warrior weapon |
| Axe | 1d8 | 1.2x | Armor penetration: reduce target armor by 1 before damage calc |
| Great Axe | 2d6 | 1.5x | Two-handed, no off-hand |
| Mace | 1d6 | 1.1x | +2 damage vs undead |
| Staff | 1d4 | 1.0x | +20% max mana |

### Ranged Weapons

| Weapon | Base Damage | Range | Notes |
|--------|-------------|-------|-------|
| Short Bow | 1d6 | 8 | Needs arrows in off-hand |
| Long Bow | 1d8 | 12 | Needs arrows |
| Crossbow | 1d10 | 10 | Slow reload (1.5x speed cost) |

**Arrows** are a consumable resource stacked in the off-hand slot (max 30).
Found as loot or dropped by archers.

### Typed Damage Weapons

Rare and Legendary weapons may deal **fire** or **lightning** damage instead of
physical. These bypass armor (only resistance reduces them) and carry their
type's unique property:

- **Fire weapons** — destroy wooden doors on hit
- **Lightning weapons** — chance to arc to a nearby enemy

Typed weapons are rare finds, never common. Examples:
- *Flameblade* (Rare) — 1d8 fire damage
- *Stormhammer* (Legendary) — 1d10 lightning damage, arcs to 1 enemy (1d4)

## Armor

Each armor piece provides flat **armor** (damage reduction). No stat
requirements. No speed penalties on armor.

### Armor Pieces

| Slot | Light | Medium | Heavy |
|------|-------|--------|-------|
| Helm | Leather Cap (1 armor) | Iron Helm (2 armor) | Full Helm (3 armor) |
| Chest | Robe (0 armor, +5 mana) | Chainmail (3 armor) | Plate Armor (5 armor) |
| Gloves | Leather Gloves (1 armor) | Splint Gloves (2 armor) | Gauntlets (3 armor) |
| Boots | Soft Boots (0 armor, -0.1x delay) | Iron Boots (2 armor) | Heavy Boots (3 armor) |

### Off-hand

| Item | Effect |
|------|--------|
| Wooden Shield | +2 armor |
| Iron Shield | +3 armor |
| Tower Shield | +5 armor, +0.1x delay (slower) |
| Quiver | Holds arrows (required for bows) |

## Rings & Amulets

Passive effects — always-on when equipped. Each ring slot is independent.

### Ring Effects

| Ring | Effect |
|------|--------|
| Ring of Protection | +2 armor |
| Ring of Might | +2 damage bonus |
| Ring of Precision | +2 hit bonus |
| Ring of Evasion | +2 dodge bonus |
| Ring of Regeneration | +1 HP regen per turn |
| Ring of the Mage | +20% max mana |
| Ring of Speed | -0.1x delay |
| Ring of Vitality | +20% max HP |

### Amulet Effects

| Amulet | Effect |
|--------|--------|
| Amulet of Life | +15 max HP |
| Amulet of Warding | +25% physical resistance |
| Amulet of Swiftness | -0.15x delay |
| Amulet of the Inferno | +50% fire resistance |
| Amulet of Grounding | +50% lightning resistance |

Rare and Legendary rings/amulets may combine two effects or have higher values.

## Consumables

### Potions

| Potion | Effect |
|--------|--------|
| Healing Potion | Restore 15 HP |
| Greater Healing Potion | Restore 30 HP |
| Mana Potion | Restore 15 mana |
| Greater Mana Potion | Restore 35 mana |
| Swiftness Potion | -0.2x delay for 20 turns |
| Potion of Invisibility | Monsters lose tracking for 15 turns |

### Scrolls

| Scroll | Effect |
|--------|--------|
| Scroll of Teleport | Teleport to random safe tile on current floor |
| Scroll of Mapping | Reveal full floor map (FOV still required to see entities) |
| Scroll of FireBall | Deal 3d6 fire damage to all enemies in 3-tile radius |
| Scroll of Fear | All nearby enemies flee for 8 turns |

## Spellbooks / Tomes

Reading a spellbook permanently adds the spell to the player's known spell list.
The player can then equip it into an active spell slot.

See SPELLS.md for the full spell list and acquisition rules.

Each spellbook is named after the spell it teaches:
- *Tome of Magic Missile*
- *Tome of Fireball*
- *Tome of Lightning Bolt*
- etc.

## Item Generation by Floor

Rarity weights shift toward better tiers on deeper floors:

| Floors | Common% | Uncommon% | Rare% | Legendary% |
|--------|---------|-----------|-------|-----------|
| 1-5 | 70% | 24% | 5% | 1% |
| 6-10 | 55% | 32% | 11% | 2% |
| 11-15 | 40% | 38% | 18% | 4% |
| 16-20 | 25% | 40% | 27% | 8% |

Typed damage weapons (fire, lightning) only appear at Rare or above.

## Inventory

- **20 slots** total
- Each item occupies 1 slot
- Consumables stack up to 5 per slot
- Arrows stack to 30 per slot
- No carry weight — slot count is the only constraint

## Resolved Decisions

- **No armor speed penalties** — armor provides flat armor only, no delay cost
- **Necrotic weapons do not exist** — necrotic damage is exclusive to spells
  and the Tyrant
- **Items only come from chests** — no random items spawned on the floor.
  All loot is found in chests placed by the map builder.

## Open Questions

1. **Weapon speed modifiers** — Are these meaningful enough? A 1.5x great axe
   means 50% slower turns. Worth the 2d6 damage?
2. **Chest placement density** — How many chests per floor on average?
3. **Scroll of Enchantment** — Too powerful? What happens on a Legendary item?
