# Items Design

## Overview

Items are found in **chests** throughout the dungeon. There are no shops, no
crafting, and no item identification. What you see is what you get. All
character power comes from equipment and enchanting.

There are no stat requirements — any gear can be equipped by any player.

All loot comes from chests. Chests are placed deliberately by the builder
pipeline — inside machines, guarded by monsters, behind hidden doors, in
difficult terrain, or trapped (see ENCOUNTERS.md).

## Weapons

### Weapon Types

Each weapon type has a distinct **active ability** on cooldown, giving it a
unique tactical identity beyond raw damage numbers. Base stats are similar
across types — the ability is what defines the weapon's playstyle.

**Implemented:**

| Weapon | Base Damage | Speed Mod | Active Ability |
|--------|-------------|-----------|----------------|
| Sword | 1d6 | 1.0x | **None — the Sword is the balance baseline.** Every other weapon is tuned against it. |
| Dagger | 1d4 | 0.8x (fast) | **Backstab** — If you attack an enemy that hasn't seen you (asleep or unaware), deal triple damage. Passive, no cooldown. |
| Axe | 1d4 | 1.2x (slow) | **Cleave** — Every melee swing also damages every monster in the 8 tiles surrounding *you* (excluding the primary target, who took the main hit). Splash equals the rolled damage. Trades raw damage for area coverage. Passive, no cooldown. |
| Bow | 1d4 | 1.0x | **Ranged** — Press **F** to enter targeting; fires an arrow up to `weapon_range` tiles. Consumes one Arrow. Bow also functions as a basic 1d4 melee weapon if an enemy gets adjacent. |

**Planned (not yet implemented):**

| Weapon | Base Damage | Speed Mod | Active Ability |
|--------|-------------|-----------|----------------|
| Spear | 1d6 | 1.0x | **Lunge** — Attack a target 2 tiles away, moving into the adjacent tile. Cooldown: 3 turns. |
| Mace | 1d6 | 1.1x | **Stun** — On hit, target loses their next turn. Cooldown: 6 turns. |

### Typed Damage Weapons

Rare weapons may deal **fire** or **lightning** damage instead of physical.
These bypass armor (only resistance reduces them):

- **Fire weapons** — destroy wooden doors on hit
- **Lightning weapons** — chance to arc to a nearby enemy

Examples:
- *Flameblade* — 1d6 fire damage sword
- *Stormhammer* — 1d8 lightning damage mace (planned)

## Staves

Staves are the player's magic system. Each staff fires a specific magical effect
and has a limited number of **charges** (Brogue-style). When charges are spent,
the staff is inert. Enchanting a staff adds charges and increases its power.

Staves are equipped in the weapon slot. Using a staff consumes one charge and
costs a turn.

### Staff Types

| Staff | Base Charges | Damage/Effect | Damage Type |
|-------|-------------|---------------|-------------|
| Staff of Lightning | 3 | 2d6 bolt, range 8 | Lightning |
| Staff of Fire | 3 | 2d6 in 3x3 area, range 6 | Fire |
| Staff of Poison | 4 | 1d4 + poison (3/turn, 4 turns), range 6 | Poison |
| Staff of Healing | 3 | Heals 3d6 HP (self only) | — |
| Staff of Blinking | 3 | Teleport to visible tile within 8 | — |
| Staff of Force | 4 | Knockback 3 tiles + 1d6, range 6 | Physical |

### Enchanting Staves

Each enchant scroll used on a staff:
- Adds **+1 charge** (max charges increase)
- Increases **damage/healing by +1d6** (or +1 effect level for utility staves)

A +3 Staff of Lightning has 6 charges and deals 5d6 lightning damage per shot.
This makes enchanting staves a viable build — invest scrolls into one powerful
staff for a "mage" playstyle.

## Armor

Armor provides flat **armor** (damage reduction against physical) or **dodge
bonus** (chance to avoid hits entirely). Each piece leans toward one or the other.
No speed penalties on armor. No stat requirements.

### Armor Pieces

| Slot | Light (Dodge) | Medium (Mixed) | Heavy (Armor) |
|------|--------------|----------------|---------------|
| Helm | Leather Cap (+1 dodge) | Iron Helm (+1 armor) | Full Helm (+2 armor) |
| Chest | Robe (+2 dodge) | Chainmail (+3 armor) | Plate Armor (+5 armor) |
| Gloves | Leather Gloves (+1 dodge) | Splint Gloves (+1 armor, +1 dodge) | Gauntlets (+2 armor) |
| Boots | Soft Boots (+1 dodge) | Iron Boots (+2 armor) | Heavy Boots (+3 armor) |

**Build identity from armor:** A player in light armor dodges attacks. A player
in heavy armor tanks through them. Both are viable, and enchanting amplifies the
chosen strategy.

### Off-hand

| Item | Effect |
|------|--------|
| Wooden Shield | +2 armor |
| Iron Shield | +3 armor |
| Tower Shield | +5 armor, +0.1 delay (10% slower on every action) |

## Rings & Amulets

Passive effects — always-on when equipped. Two ring slots, one amulet slot.

### Ring Effects

| Ring | Effect |
|------|--------|
| Ring of Perception | +2 hit bonus, +4 vision range |
| Ring of Protection | +2 armor |
| Ring of Might | +2 damage bonus |
| Ring of Precision | +2 hit bonus |
| Ring of Evasion | +2 dodge bonus |
| Ring of Regeneration | +1 HP regen per turn |
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
| Amulet of Antivenom | +50% poison resistance |

## Runics

Weapons and armor can have a **runic** — a special enchantment with a unique
effect. Runics are rare and define item identity. An item can have at most one
runic.

### Weapon Runics

| Runic | Effect |
|-------|--------|
| of Speed | -0.2x delay on this weapon's attacks |
| of Force | Knockback 2 tiles on hit |
| of Flames | +1d4 fire damage on hit |
| of Venom | +1d4 poison damage on hit, poison DoT (2/turn, 3 turns) |
| of Lightning | +1d4 lightning damage on hit, 25% chance to arc |
| of Slaying | +50% damage vs. a specific faction (e.g., goblins, dragons) |
| of Slowing | On hit, target's delay +0.2x for 5 turns |
| of Paralysis | 10% chance on hit to paralyze target for 2 turns |
| of Quietus | +100% damage vs. targets below 25% HP |

### Armor Runics

| Runic | Effect |
|-------|--------|
| of Reflection | 30% chance to reflect projectiles back at attacker |
| of Immunity | +75% resistance to one damage type (fire, lightning, or poison) |
| of Reprisal | When hit, deal 1d4 damage back to attacker |
| of Absorption | 10% of physical damage taken is converted to healing |

## Consumables

### Potions

Potions are one-use consumable items. Stack up to 5 per inventory slot.

| Potion | Effect |
|--------|--------|
| Healing Potion | Restore 15 HP |
| Greater Healing Potion | Restore 30 HP |
| Swiftness Potion | -0.2x delay for 20 turns |
| Fire Resistance Potion | +75% fire resistance for 30 turns |
| Antidote | Remove all poison effects, +50% poison resistance for 10 turns |

### Enchant Scrolls

**Scroll of Enchanting** — the core strategic item. Using one lets the player
choose any carried item to enchant (+1 level). Effects per item type:

| Item Type | Enchant Effect |
|-----------|---------------|
| Weapon | +1 damage bonus |
| Staff | +1 charge, +1d6 damage/healing |
| Armor piece | +1 armor or +1 dodge (matches the piece's type) |
| Shield | +1 armor |
| Ring | Effect value +1 (e.g., Ring of Protection +2 → +3) |
| Amulet | Effect value increased proportionally |

Enchant scrolls are rare — roughly 1 per floor in the early game, 1-2 on deeper
floors. The decision of which item to enchant is the defining strategic choice
of each run.

## Inventory

- **20 slots** total
- Each item occupies 1 slot
- Consumables (potions) stack up to 5 per slot
- No carry weight — slot count is the only constraint

## Item Generation

### Floor Scaling

Deeper floors produce better items:

| Floors | Weapon Quality | Staff Quality | Armor Quality |
|--------|---------------|---------------|---------------|
| 1-3 | Sword/Dagger, basic | Staff of Force, Staff of Healing | Light armor, wooden shield |
| 4-6 | Better damage dice, typed weapons appear | All staff types available | Medium armor, iron shield |
| 7-9 | Runic weapons appear | Higher base charges | Heavy armor, tower shield |
| 10 | Best items, guaranteed runic on some drops | — | — |

### Runic Chance

Tuned for the 26-floor dungeon. Formula: `floor < 5 ? 0 : min(50, (floor-4) * 5/2)`.
Implemented as `runic_chance_for_floor` in `enchantment.rs`.

| Floors | Runic Chance |
|--------|--------------|
| 1-4    | 0%           |
| 5      | 2%           |
| 9      | 12%          |
| 13     | 22%          |
| 17     | 32%          |
| 21     | 42%          |
| 24-26  | 50% (cap)    |

## Resolved Decisions

- **No armor speed penalties** — armor provides flat armor or dodge only
- **Items only come from chests** — no random items spawned on the floor
- **No item identification** — items are always known
- **No cursed items** for the POC
- **No mana system** — staves use charges, not mana
- **Enchant scrolls only** — no enchanting stations or alternative methods
- **Weapon types:** Sword is the balance baseline (no active ability — every other weapon is tuned against it). Dagger has Backstab; Axe has Cleave (8-tile attacker-centered splash, lower base damage); Bow is fired with **F** for ranged + can also melee for 1d4. Spear (Lunge) and Mace (Stun) are planned.
- **Riposte was removed.** It existed as the Sword's active ability but conflicted with the goal of having a clean balance baseline weapon. The `RiposteReady` component and free-action override are gone.
- **Shrines provide permanent upgrades via essence currency** — see GAME.md for details
- **Resistances on amulets** — Inferno/Grounding/Antivenom each grant +50% to one damage type; Warding grants +25% physical
- **Spellbook item kind removed** — the genre's spellbook concept lives in monsters' cooldown abilities and the player's staves; never reintroduce a player spellbook item without a stronger justification than "more loot variety"
