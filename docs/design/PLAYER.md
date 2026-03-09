# Player Design

## Core Stats

| Stat | Abbrev | Governs |
|------|--------|---------|
| Strength | STR | Melee damage bonus, carry weight, heavy armor/weapon requirements |
| Dexterity | DEX | Accuracy, dodge chance, speed modifier, light armor/ranged requirements |
| Intelligence | INT | Max mana, spell power scaling, spellbook identification chance |
| Constitution | CON | Max HP, HP gained per level, poison/stun resistance |
| Luck | LCK | Crit chance, rare item find rate, trap detection |

**Starting values:** All stats begin at 5. The player gets +2 points to distribute freely at character creation (or could be randomized — TBD).

## Derived Values

| Value | Formula |
|-------|---------|
| Max HP | `10 + (CON × 3) + (level × CON)` |
| Max Mana | `INT × 5` |
| Melee Damage | `weapon base damage + STR bonus` |
| Accuracy | `base 80% + DEX bonus` |
| Dodge Chance | `DEX bonus (capped at ~30%)` |
| Speed | Turn delay multiplier; `1.0 - ((DEX - 5) × 0.02)` (lower = faster; feeds into TurnManager) |

STR/DEX/INT bonus = `(stat - 5)` (so stat 5 = +0, stat 8 = +3, stat 3 = -2).

## Leveling & XP

The player gains XP from killing enemies. XP requirements scale exponentially.

| Level | XP Required (cumulative) | Reward |
|-------|--------------------------|--------|
| 1 | 0 (start) | — |
| 2 | 100 | +1 stat point |
| 3 | 250 | +1 stat point, **+1 spell slot** |
| 4 | 450 | +1 stat point |
| 5 | 700 | +1 stat point, **+1 spell slot** |
| 6 | 1000 | +1 stat point |
| 7 | 1400 | +1 stat point |
| 8 | 1900 | +1 stat point, **+1 spell slot** |
| 9 | 2500 | +1 stat point |
| 10 | 3200 | +1 stat point |
| 11 | 4000 | +1 stat point, **+1 spell slot** |
| 12 | 5000 | +1 stat point |
| 13 | 6200 | +1 stat point |
| 14 | 7600 | +1 stat point, **+1 spell slot** |
| 15 | 9200 | +1 stat point (max level) |

The player can distribute each stat point to any stat on level-up. Max spell slots = 6 (1 starting + 5 unlocked).

## Equipment Slots

The player has 9 equipment slots:

| Slot | Examples |
|------|---------|
| Weapon | Sword, Dagger, Staff, Bow |
| Off-hand | Shield, Quiver, Focus orb |
| Helm | Iron Helm, Leather Cap |
| Chest | Plate Armor, Robe, Chainmail |
| Gloves | Gauntlets, Leather Gloves |
| Boots | Iron Boots, Soft Boots |
| Ring (×2) | Two ring slots |
| Amulet | One amulet slot |

See [ITEMS.md](ITEMS.md) for full equipment details.

## Combat

### Melee Attack
1. Player moves into enemy — triggers melee intent
2. Hit check: `roll d100 < accuracy%`
3. On hit: `damage = weapon_base + STR_bonus` minus `enemy_DEF`
4. On miss: 0 damage, turn still consumed

### Ranged Attack
- Requires bow equipped and arrows in off-hand
- Range is limited (default: 8 tiles)
- Uses DEX instead of STR for accuracy
- No penalty for ranged attacks, but consumes ammunition

### Magic
- Casts from active spell slots using mana
- See [MAGIC.md](MAGIC.md)

## Speed & Turn Order

The player's turn delay is computed as:
```
delay = base_cost × SpeedStats::delay_multiplier
delay_multiplier = 1.0 - ((DEX - 5) × 0.02)  // capped at 0.6 min
```
Higher DEX = faster turns relative to enemies. This feeds directly into the existing `TurnManager` queue.

## Death & Death Screen

On death, show:
- Floor reached
- Level and stats at time of death
- Last action / cause of death
- Items carried
- Total XP / enemies killed
