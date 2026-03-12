# Items Design

## Item Rarity Tiers

| Tier | Color | Drop Weight | Notes |
|------|-------|-------------|-------|
| Common | White | 60% | Basic stats, no special effects |
| Uncommon | Green | 25% | +1 bonus property |
| Rare | Blue | 12% | +2 bonus properties |
| Legendary | Gold | 3% | Unique named items with special effects |

Rarity weights shift toward better tiers on deeper floors.

## Weapons

All weapons have: `base_damage`, `stat_requirement`, `attack_speed_modifier`, `range` (1 = melee).

### Melee Weapons

| Weapon | Base Damage | Req | Speed Mod | Notes |
|--------|-------------|-----|-----------|-------|
| Dagger | 1d4 | DEX 5 | 0.8× (fast) | High crit chance |
| Short Sword | 1d6 | STR 5 | 1.0× | Balanced |
| Long Sword | 1d8 | STR 7 | 1.1× | Standard warrior weapon |
| Axe | 1d8 | STR 8 | 1.2× | Ignores some armor |
| Great Axe | 2d6 | STR 12 | 1.5× | Two-handed, no shield |
| Staff | 1d4 | INT 5 | 1.0× | +2 mana regen/turn |
| Mace | 1d6 | STR 6 | 1.1× | +10% vs undead |

### Ranged Weapons

| Weapon | Base Damage | Req | Range | Notes |
|--------|-------------|-----|-------|-------|
| Short Bow | 1d6 | DEX 6 | 8 | Needs arrows in off-hand |
| Long Bow | 1d8 | DEX 9 | 12 | Needs arrows |
| Crossbow | 1d10 | DEX 7 | 10 | Slow reload (+50% cost) |

**Arrows** are a consumable resource stacked in the off-hand slot (can hold 30 at a time). Found as loot or dropped by archers.

## Armor

Each armor piece provides `defense` (flat damage reduction) and may have stat requirements.

### Armor Pieces

| Slot | Light (DEX) | Medium | Heavy (STR) |
|------|-------------|--------|-------------|
| Helm | Leather Cap (1 DEF) | Iron Helm (3 DEF) | Full Helm (5 DEF) |
| Chest | Robe (0 DEF, +INT) | Chainmail (4 DEF) | Plate Armor (8 DEF, STR 10) |
| Gloves | Leather Gloves (1 DEF) | Splint Gloves (2 DEF) | Gauntlets (3 DEF, STR 8) |
| Boots | Soft Boots (1 DEF, +Speed) | Iron Boots (2 DEF) | Heavy Boots (3 DEF, STR 8) |

### Off-hand

| Item | Effect |
|------|--------|
| Wooden Shield | +2 DEF, +5% block |
| Iron Shield | +4 DEF, +10% block |
| Tower Shield | +6 DEF, +15% block, -1 DEX |
| Focus Orb | +3 spell power, no DEF |
| Quiver | Holds arrows (required for bows) |

## Rings & Amulets

Passive effects — always-on when equipped. Each ring slot is independent.

### Ring Effects (examples)

| Ring | Effect |
|------|--------|
| Ring of Protection | +2 DEF |
| Ring of Strength | +2 STR |
| Ring of Dexterity | +2 DEX |
| Ring of Intelligence | +2 INT |
| Ring of Regeneration | +2 HP regen per 10 turns |
| Ring of the Mage | +5 max mana |
| Ring of Speed | -10% turn delay |
| Ring of Accuracy | +10% accuracy |
| Ring of Evasion | +5% dodge chance |
| Ring of Luck | +2 LCK |

Rare/legendary rings may combine two effects or have scaled values.

### Amulet Effects (examples)

| Amulet | Effect |
|--------|--------|
| Amulet of Life | +15 max HP |
| Amulet of Clarity | +10 max mana, +1 spell power |
| Amulet of Warding | Resist 15% of all magic damage |
| Amulet of the Depths | +5% XP gained |
| Amulet of Swiftness | +15% speed |
| **Amulet of Dominion** | Win condition item (floor 10 only) |

## Consumables

### Potions

| Potion | Effect |
|--------|--------|
| Healing Potion | Restore 20 + (level × 5) HP |
| Greater Healing Potion | Restore 50 + (level × 8) HP |
| Mana Potion | Restore 15 mana |
| Greater Mana Potion | Restore 35 mana |
| Strength Elixir | +2 STR for 20 turns |
| Swiftness Potion | -20% turn delay for 20 turns |
| Antidote | Cure poison |
| Potion of Invisibility | Monsters lose tracking for 15 turns |

### Scrolls

| Scroll | Effect |
|--------|--------|
| Scroll of Identify | Identify one item (stretch goal mechanic) |
| Scroll of Teleport | Blink to random safe location on current floor |
| Scroll of Mapping | Reveal full floor map (FOV still required to see entities) |
| Scroll of Fire | Deal 3d6 fire damage to all enemies in 3-tile radius |
| Scroll of Blinding | Blind all visible enemies for 10 turns |
| Scroll of Enchantment | Upgrade one equipped item by +1 tier |
| Scroll of Fear | All nearby enemies flee for 8 turns |

## Spellbooks / Tomes

Reading a spellbook permanently adds the spell to the player's **known spell list**. The player can then equip it into an active spell slot.

See [MAGIC.md](MAGIC.md) for the full spell list.

Each spellbook is named after the spell it teaches:
- *Tome of Magic Missile*
- *Tome of Fireball*
- *Grimoire of the Healing Word*
- etc.

## Item Generation by Floor

Items found on deeper floors tend to be rarer and higher-powered:

| Floors | Common% | Uncommon% | Rare% | Legendary% |
|--------|---------|-----------|-------|-----------|
| 1-3 | 70% | 24% | 5% | 1% |
| 4-6 | 55% | 32% | 11% | 2% |
| 7-9 | 40% | 38% | 18% | 4% |
| 10 | 25% | 40% | 27% | 8% |

Spellbook generation increases with floor depth (rare on floors 1-3, more common mid-deep).

## Carry Weight / Inventory

- Player has an inventory of **20 slots**
- Each item occupies 1 slot (consumables stack up to 5 per slot)
- Arrows stack to 30 per slot
- No carry weight limit beyond slot count — simpler to manage
