# Player Design

## Core Stats

| Stat | Abbrev | Governs |
|------|--------|---------|
| Strength | STR | Melee damage bonus, carry weight, heavy armor/weapon requirements |
| Dexterity | DEX | Accuracy, dodge chance, light armor/ranged weapon requirements |
| Constitution | CON | Max HP, HP gained per level, poison/stun resistance |
| Agility | AGI | Turn speed (lower delay = more turns per cycle) |
| Intelligence | INT | Max mana (INT × 5), spell power scaling, spellbook requirements |
| Perception | PER | Vision range, trap/secret detection, ranged accuracy |

**Starting values:** All stats begin at 10. The player receives 1 stat point per level-up.

## Bonus Formula

Every stat uses the same formula: **`bonus = stat - 10`**

This means:
- At stat **10** (starting baseline): bonus is **+0** — neutral
- At stat **11**: bonus is **+1** — immediately visible the turn you level up
- At stat **14**: bonus is **+4** — a meaningful jump after a few floors
- At stat **8** (debuffed/cursed): bonus is **−2** — noticeable penalty

Every single stat point directly changes derived combat values. There are no "dead" odd-numbered points.

## Derived Values

| Value | Formula |
|-------|---------|
| Max HP | `10 + rolled_hp_sum + (CON bonus × level)` |
| Max Mana | `INT × 5` |
| Melee Damage | `weapon dice + STR bonus` |
| Hit Chance | `10 + STR bonus` (roll 1d20 + hit_chance vs 10 + target dodge) |
| Dodge Chance | `5 + DEX bonus` |
| Action Delay | `1.0 − (AGI bonus × 0.025)` clamped to [0.5, 2.0] |
| Vision Range | `(8 + PER bonus).max(2)` tiles |

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

## Unified Stat System (Player & Monsters)

The player and all monsters share the same ECS components and formulas. `stat_recalculation_system` runs on every entity with `Attributes` — no special-casing for who is the player. The combat pipeline is fully symmetric.

The only structural differences:
- **HP source:** Player uses a sum of 1d4 rolls per level (`RolledHp`); monsters use a flat `MonsterBaseHealth` value
- **Mana:** Only the player has a `Mana` component; monsters have no mana pool
- **Spell slots:** Player-only feature

This means status effects, debuffs, and buff items work identically whether applied to the player or an enemy. See [BESTIARY.md](BESTIARY.md) for full details on the shared system.

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
delay_multiplier = 1.0 - (AGI_bonus × 0.025)   // clamped [0.5, 2.0]
AGI_bonus = agility - 10
```
At AGI 10 (start): delay = 1.0x (baseline). At AGI 18: delay = 0.8x (20% faster). At AGI 6 (debuffed): delay = 1.1x (slower). Higher AGI = more turns per cycle relative to enemies. Feeds directly into the existing `TurnManager` queue.

## Death & Death Screen

On death, show:
- Floor reached
- Level and stats at time of death
- Last action / cause of death
- Items carried
- Total XP / enemies killed
