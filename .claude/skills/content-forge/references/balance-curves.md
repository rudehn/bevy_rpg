# Balance Curves

Floor-by-floor power targets and stat budgets for assigning balanced values to new content.
All formulas verified against `src/game/stats.rs`. Reference data from `assets/monsters.ron`.

## Player Power by Floor

Expected values assuming moderate Essence investment:

| Floor | Player HP | Player DPS | Player Armor | Player Mana |
|-------|-----------|------------|--------------|-------------|
| 1     | 25-28     | 3-5        | 0-1          | 50          |
| 5     | 30-35     | 5-8        | 2-4          | 55-65       |
| 10    | 38-45     | 8-12       | 4-6          | 65-80       |
| 15    | 45-55     | 12-16      | 6-8          | 80-100      |
| 20    | 55-70     | 16-22      | 8-12         | 100-120     |

**Formulas:**
- Player HP: `25 + (CON_bonus * 3) + equipment_max_hp + essence_max_hp`
- Mana max: `INT * 5 + equipment_max_mana + essence_max_mana`
- HP regen: accumulator `20 + (CON_bonus * 5) + equipment_hp_regen_flat` per turn; 100 accumulator = 1 HP. Suppressed 5 turns after taking damage.
- Mana regen: accumulator `20 + (INT_bonus * 5) + equipment_mana_regen_flat` per turn; 100 accumulator = 1 mana.

## Monster Stat Budgets

### HP

Two-stage calculation:
- `base_hp` is the RON field
- `final_hp = base_hp + (CON_bonus * level)` where `CON_bonus = CON - 10`

Set `base_hp` to target the desired final HP after CON scaling. Example: a level 10 monster with CON 14 (bonus +4) gets +40 from scaling, so `base_hp = 45` yields ~85 final HP.

### Damage Targets
- **Early game (floors 1-5)**: 15-25% of expected player HP per hit
- **Mid game (floors 6-12)**: 20-35% of expected player HP per hit
- **Late game (floors 13-20)**: 30-50% of expected player HP per hit

### Attributes
- **Baseline**: 10 for all combat-relevant stats
- **INT**: Set to 0 for non-casters (matches all existing non-caster monsters). Casters: 14-22 based on power level.
- **PER**: Varies 6-14 based on awareness. Low PER (6-8) = poor detection. High PER (12-14) = wide vision.
- **Adjustment rate**: ±1 per 3 floors from baseline, ±4 max for role emphasis.

### AGI (Speed)
AGI determines turn delay via: `delay = 1.0 - (AGI_bonus * 0.025)`, clamped [0.5, 2.0].

| Speed Class | AGI | Delay | Turns per 10 player turns |
|-------------|-----|-------|---------------------------|
| Very fast   | 18  | 0.80  | ~12.5                     |
| Fast        | 14  | 0.90  | ~11.1                     |
| Normal      | 10  | 1.00  | 10                        |
| Slow        | 6   | 1.10  | ~9.1                      |
| Very slow   | 4   | 1.15  | ~8.7                      |

### Armor
| Floor Bracket | Armor Range |
|---------------|-------------|
| 1-5           | 0-2         |
| 6-12          | 3-5         |
| 13-18         | 5-8         |
| 19-20         | 7-12        |

### Experience (Essence) Reward
Formula: `10 + (level * 5) + (base_hp / 2)`

This is the implemented BESTIARY formula. Do not use ad-hoc approximations.

## Reference Data Points

Actual values from existing monsters for calibration:

| Monster         | Level | base_hp | Damage   | AGI | Armor | Final HP (approx) | Essence |
|-----------------|-------|---------|----------|-----|-------|--------------------|---------|
| Rat             | 1     | 8       | 1d4      | 14  | 0     | ~6                 | 19      |
| Goblin          | 1     | 10      | 1d4      | 10  | 0     | ~10                | 20      |
| Giant Spider    | 4     | 18      | 1d6      | 12  | 0     | ~22                | 39      |
| Skeleton        | 5     | 22      | 1d6      | 8   | 1     | ~22                | 46      |
| Zombie          | 6     | 35      | 1d6+1    | 4   | 0     | ~47                | 58      |
| Orc             | 8     | 32      | 1d8+1    | 10  | 2     | ~48                | 66      |
| Orc Berserker   | 10    | 45      | 1d10+2   | 10  | 1     | ~85                | 83      |
| Shadow Fiend    | 14    | 55      | 1d10+2   | 12  | 2     | ~83                | 108     |
| Ogre            | 14    | 65      | 1d12+3   | 6   | 3     | ~93                | 113     |
| Vampire         | 17    | 65      | 1d10+3   | 14  | 3     | ~99                | 128     |
| Dark Knight     | 18    | 75      | 2d8+4    | 8   | 5     | ~183               | 138     |
| Veiled Tyrant   | 20    | 200     | 2d8+4    | 12  | 5     | ~440               | 210     |

*Final HP = `base_hp + (CON_bonus * level)`. Essence = `10 + (level * 5) + (base_hp / 2)`.*

## Item Power Budgets

### By Rarity

| Rarity    | Spawn % | Bonus Count | Weapon Damage | Armor Defense | Bonus Magnitude |
|-----------|---------|-------------|---------------|---------------|-----------------|
| Common    | 50%     | 0           | 1d4-1d6       | 1-2           | —               |
| Uncommon  | 35%     | 1           | 1d6-1d8       | 2-3           | 8-12%           |
| Rare      | 14%     | 2           | 1d8-2d6       | 3-5           | 12-18%          |
| Legendary | 1%      | 3           | 2d6-2d8       | 5-7           | 15-25%          |

### Seclusion Bonus
Rooms with seclusion > 0.7 get +15 to rarity roll, increasing Legendary/Rare chance.

### Equipment Slots (9 total)
Weapon, OffHand, Helm, Chest, Gloves, Boots, Ring L, Ring R, Amulet

### Stat Bonus Guidelines
- Common: no stat bonuses
- Uncommon: 1 stat at +1
- Rare: 1-2 stats at +1 to +2
- Legendary: 2-3 stats at +2 to +3

## Spell Power Budgets

| Tier        | Mana Cost | Cooldown | Damage Dice | Spell Role              |
|-------------|-----------|----------|-------------|-------------------------|
| Cantrip     | 3-8       | 0-4      | 1d4-1d6     | Filler, sustainable use |
| Standard    | 10-20     | 5-12     | 2d4-2d8     | Meaningful impact       |
| Powerhouse  | 25-35     | 15-25    | 3d6-4d6     | Encounter-changing      |

### INT Scaling Policy
- **Scales with INT**: Most single-target damage, heals, mana drain
- **Fixed (no INT scaling)**: AoE damage (fireball, meteor), cantrips (spark), buffs, debuffs, utility, summons

### Mana Economy
- Base mana at INT 10: 50, regen ~1 per 5 turns
- Caster build (INT 20): 100 mana, regen ~3 per 5 turns
- A cantrip (5 mana) is sustainable; a powerhouse (30 mana) requires 50+ turns to recover naturally

## Spawn Density Guidelines

| Floor Bracket | Monsters per Floor | Item Spawns per Floor |
|---------------|--------------------|-----------------------|
| 1-5           | 8-12               | 2-4                   |
| 6-15          | 12-18              | 3-5                   |
| 16-20         | 15-22              | 4-6                   |

### Floor-Based Item Fill Rate
| Depth | Fill Rate |
|-------|-----------|
| 1-3   | 75%       |
| 4-7   | 65%       |
| 8+    | 55%       |

## Essence Economy

| Tier | Cost    | Expected Floor |
|------|---------|----------------|
| 1    | 75      | 3-5            |
| 2    | 175-200 | 8-12           |
| 3    | 350-500 | 15+            |

A player earns roughly 200-400 Essence per floor depending on monster density and composition.

## Floor Scope Note

Current content spans floors 1-20 across 5 zones. The OVERVIEW doc describes 26 floors, but floors 21-26 are future content. Design for floors 1-20 unless explicitly targeting late-game expansion.

### Zone Overview

| Zone | Floors | Primary Factions        | Flavor                    |
|------|--------|-------------------------|---------------------------|
| 1    | 1-5    | Vermin + Goblinoid      | Surface — tutorial pacing |
| 2    | 6-10   | Goblinoid elite + Undead| Catacombs — squad tactics |
| 3    | 9-14   | Undead + Orcish         | Depths — brute force      |
| 4    | 13-17  | Orcish + Demonic + Giant| Underworld — mixed threats|
| 5    | 17-20  | Dark + Demonic + Giant  | Abyss — endgame + boss    |
