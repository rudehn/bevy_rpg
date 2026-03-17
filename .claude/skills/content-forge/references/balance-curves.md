# Balance Curves

Floor-by-floor power targets and stat budgets for assigning balanced values to new content.
All formulas verified against `src/game/stats.rs` and `src/game/combat.rs`.

## Simplified Combat System

The game uses **direct values** — no attribute-to-stat conversion pipeline.
Monsters and items specify their combat values directly in RON.

**Core combat components:**
- `Health { current, max }` — Set directly from `base_hp`
- `Armor(i32)` — Flat damage reduction: `final_damage = max(1, raw_damage - armor)`
- `Dodge(i32)` — Flat dodge chance: hit roll (1d20) must be ≥ `2 + dodge`. Currently hardcoded to 0 for all monsters.
- `Mana { current, max }` — Casters only: `max = intelligence * 5`

**Damage pipeline:** AttackIntent → HitCheck (1d20 vs 2+dodge) → DamageRoll (5% crit = 150%) → ArmorReduction → Apply

## MonsterAsset Fields — What's Actually Used

The `MonsterAsset` struct has legacy attribute fields (`strength`, `dexterity`,
`constitution`, `agility`) that are **not read by the spawner**. Only these
fields affect gameplay:

| Field | Used By | Effect |
|-------|---------|--------|
| `base_hp` | Spawner → `Health` | Direct HP value |
| `base_armor` | Spawner → `Armor` | Flat damage reduction |
| `damage` | Spawner → `Damage` | Dice expression (e.g., "1d6+1") |
| `perception` | Spawner → `Viewshed` | Vision range: `8 + (perception - 10)` |
| `intelligence` | Spawner → `Mana` | Mana pool: `intelligence * 5` (casters only) |
| `level` | Spawner | Used for essence reward formula |
| `spells` | Spawner → `KnownSpells`/`ActiveSpells` | Spell IDs from spells.ron |
| `regen` | Spawner → `HpRegen` | HP regen per turn |
| `damage_type` | Spawner → `DamageTypeTag` | Melee damage type |
| `resistances` | Spawner → `Resistances` | Damage type resistance map |
| `ranged_range` | Spawner | Ranged attack range in tiles |
| `is_boss` | Spawner → `FinalBoss`/`BossAI` | Boss markers |
| `faction_tag` | Spawner → squad system | Faction for prefab/squad |
| `role` | Spawner → squad system | Combat role for squad |

**Legacy fields (still in struct/RON, NOT used):** `strength`, `dexterity`,
`constitution`, `agility`

**Ability fields (in RON data, NOT on MonsterAsset struct — orphaned):**
`on_hit_effects`, `poison_body`, `thorn_aura`, `reanimate_hp`, `enrage_on_hit`,
`explode_on_death`, `death_curse`, `summon_on_death`, `is_cowardly`

These ability fields exist in `monsters.ron` but the `MonsterAsset` struct
doesn't declare them, so they're silently ignored during deserialization.
The handler systems in `abilities.rs` are registered but no monsters receive
these components. This is a WIP state — the data is preserved for future
reconnection.

## Player Power by Floor

Expected values assuming moderate equipment upgrades:

| Floor | Player HP | Player DPS | Player Armor | Player Mana |
|-------|-----------|------------|--------------|-------------|
| 1     | 25-30     | 3-5        | 0-1          | 50          |
| 5     | 30-35     | 5-8        | 2-4          | 55-65       |
| 10    | 38-45     | 8-12       | 4-6          | 65-80       |
| 15    | 45-55     | 12-16      | 6-8          | 80-100      |
| 20    | 55-70     | 16-22      | 8-12         | 100-120     |

## Monster Stat Budgets

### HP
`base_hp` is used directly as `Health { current: base_hp, max: base_hp }`.
No attribute scaling. The RON value IS the final HP.

### Damage Targets
- **Early game (floors 1-5)**: 15-25% of expected player HP per hit
- **Mid game (floors 6-12)**: 20-35% of expected player HP per hit
- **Late game (floors 13-20)**: 30-50% of expected player HP per hit

### Perception
- `perception` determines vision range: `8 + (perception - 10)` (min 2)
- Low PER (6-8) = poor detection. High PER (12-14) = wide vision.

### Intelligence (Casters Only)
- `intelligence` determines mana pool: `max_mana = intelligence * 5`
- Set to 0 for non-casters (no mana component spawned)
- Casters: 14-22 based on power level

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

| Monster         | Level | base_hp | Damage   | Armor | Perception | INT | Essence |
|-----------------|-------|---------|----------|-------|------------|-----|---------|
| Rat             | 1     | 8       | 1d4      | 0     | 10         | 0   | 19      |
| Goblin          | 1     | 10      | 1d4      | 0     | 10         | 0   | 20      |
| Giant Spider    | 4     | 18      | 1d6      | 0     | 10         | 0   | 39      |
| Skeleton        | 5     | 22      | 1d6      | 1     | 10         | 0   | 46      |
| Zombie          | 6     | 35      | 1d6+1    | 0     | 8          | 0   | 58      |
| Orc             | 8     | 32      | 1d8+1    | 2     | 10         | 0   | 66      |
| Orc Berserker   | 10    | 45      | 1d10+2   | 1     | 10         | 0   | 83      |
| Shadow Fiend    | 14    | 55      | 1d10+2   | 2     | 12         | 0   | 108     |
| Ogre            | 14    | 65      | 1d12+3   | 3     | 8          | 0   | 113     |
| Vampire         | 17    | 65      | 1d10+3   | 3     | 14         | 0   | 128     |
| Dark Knight     | 18    | 75      | 2d8+4    | 5     | 10         | 0   | 138     |
| Veiled Tyrant   | 20    | 200     | 2d8+4    | 5     | 14         | 18  | 210     |

*HP is the direct `base_hp` value. Essence = `10 + (level * 5) + (base_hp / 2)`.*

## Item Power Budgets

### By Rarity

Items use direct `damage` (dice) and `defense` (flat armor) values.
No item bonus system — `str_bonus` etc. fields exist on `ItemAsset` but are
**not applied by the spawner**.

| Rarity    | Spawn % | Weapon Damage | Armor Defense |
|-----------|---------|---------------|---------------|
| Common    | 50%     | 1d4-1d6       | 1-2           |
| Uncommon  | 35%     | 1d6-1d8       | 2-3           |
| Rare      | 14%     | 1d8-2d6       | 3-5           |
| Legendary | 1%      | 2d6-2d8       | 5-7           |

### Seclusion Bonus
Rooms with seclusion > 0.7 get +15 to rarity roll, increasing Legendary/Rare chance.

### Equipment Slots (9 total)
Weapon, OffHand, Helm, Chest, Gloves, Boots, Ring L, Ring R, Amulet

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
