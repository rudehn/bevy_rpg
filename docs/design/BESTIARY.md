# Bestiary

## Overview

Enemies are organized into four factions that correspond to dungeon depth. Encounters escalate in HP, damage, and tactical complexity as the player descends.

## Unified Stat System

**Monsters and the player share the same ECS component system.** Both have:

| Component | Purpose |
|-----------|---------|
| `Attributes` | Raw stats: STR, DEX, CON, AGI, INT, PER |
| `AttributeModifiers` | Additive modifiers from status effects, gear |
| `CombatStats` | Derived bonuses: damage_bonus, hit_chance, dodge_chance, armor |
| `Level` | Scales HP and combat effectiveness |
| `Health` | Current/max HP |
| `Viewshed` | Vision range, driven by PER |

`stat_recalculation_system` runs identically for every entity. The combat pipeline (`hit_check → damage_roll → armor_reduction → apply_damage`) has no player vs. monster special cases — both sides of every fight use the same formulas.

**Practical implications:**
- Status effects that modify `AttributeModifiers` (e.g. Poison reducing CON, a Zombie's "Plague Touch") work the same on player and monsters
- Monster special abilities (Enrage, charge bonuses) are implemented as temporary `AttributeModifiers`
- A monster with STR 18 gets `hit_chance = 10 + (18-10) = 18`, just like a player would
- Monsters use `MonsterBaseHealth` for their HP base (instead of the player's rolled HP sum)

**Stat notation in tables below:**
- `HP`: base_hp value (actual max depends on Level + CON)
- `ATK`: damage dice string (e.g. `1d6+2`)
- `DEF`: planned armor value (flat damage reduction from `CombatStats.armor`)
- `SPD`: turn delay multiplier (1.0 = default; 0.9 = 10% faster; 1.2 = 20% slower)
- `XP`: experience reward on kill

**How table stats map to `MonsterAsset` fields:**

| Table column | Asset field | Notes |
|---|---|---|
| HP | `base_hp` | Added to CON bonus × level in stat system |
| ATK | `damage` | Dice string, e.g. `"2d6+1"` |
| DEF | `armor` | Not yet in asset — to be added in M3 (equipment) |
| SPD | `agility` | `delay = 1.0 - (AGI-10) × 0.025` |
| STR/DEX/CON/AGI/PER | direct fields | Set per-monster in `monsters.ron` |

---

## Faction: Beasts & Wildlife
*Floors 1-5, dominant on 1-3*

Instinct-driven creatures — fast, fragile, and numerous. A threat in groups; manageable alone.

| Enemy | HP | ATK | DEF | SPD | XP | Special |
|-------|-----|-----|-----|-----|----|---------|
| Giant Rat | 8 | 1d4 | 0 | 0.91 | 10 | Pack tactics (+1 ATK per adjacent rat) |
| Bat | 5 | 1d3 | 0 | 0.77 | 8 | Flies (ignores terrain), erratic movement |
| Wolf | 18 | 1d6 | 1 | 0.87 | 20 | Charge (moves 2 tiles if player is far) |
| Cave Bear | 35 | 2d6 | 3 | 1.11 | 45 | Maul (2 attacks on charge turn) |
| Venomous Spider | 10 | 1d4 | 0 | 1.0 | 18 | Poison on hit (3 HP/turn for 5 turns) |
| Dire Wolf | 28 | 1d8 | 2 | 0.83 | 40 | Howl (summons 1 wolf per use, 1/fight) |

---

## Faction: Humanoid Foes
*Floors 1-8, dominant on 3-6*

Intelligent, organized, and equipped. Fight with weapons and tactics. Rogues of the dungeon.

| Enemy | HP | ATK | DEF | SPD | XP | Special |
|-------|-----|-----|-----|-----|----|---------|
| Goblin | 12 | 1d4 | 0 | 0.91 | 12 | Coward (flees below 30% HP) |
| Goblin Archer | 10 | 1d6 | 0 | 1.0 | 15 | Ranged (stays at distance) |
| Bandit | 20 | 1d6 | 2 | 1.0 | 22 | Steal (chance to take gold on hit) |
| Dark Knight | 45 | 1d10 | 5 | 1.11 | 60 | Shield Block (50% chance to halve damage) |
| Orc Warrior | 32 | 2d6 | 4 | 1.05 | 50 | Cleave (hits adjacent tiles) |
| Shadow Rogue | 22 | 2d4 | 1 | 0.80 | 55 | Backstab (+2d6 from stealth), Vanish |

---

## Faction: Undead
*Floors 4-9, dominant on 5-7*

Tireless and immune to fear. Poison-resistant. Their special abilities drain or weaken the player's resources.

| Enemy | HP | ATK | DEF | SPD | XP | Special |
|-------|-----|-----|-----|-----|----|---------|
| Skeleton | 18 | 1d6 | 2 | 1.0 | 20 | Reforms (rises again once at 2 HP unless smashed with blunt) |
| Zombie | 30 | 1d8 | 3 | 1.43 | 28 | Slow, Plague Touch (CON drain, temp -1 CON for 10 turns) |
| Wraith | 20 | 1d6 | 0 | 0.91 | 45 | Phased (can pass through walls), Life Drain (HP steal) |
| Bone Archer | 15 | 1d8 | 1 | 1.05 | 32 | Ranged, Piercing (ignores half DEF) |
| Lich Apprentice | 25 | 2d6 | 2 | 1.0 | 65 | Casts Magic Missile (5 damage) once per fight |
| Vampire | 40 | 1d10 | 3 | 0.87 | 80 | Life Steal (heals 50% of damage dealt), Fear Aura |

---

## Faction: Demons & Fiends
*Floors 7-10, dominant on 8-10*

Powerful, elemental, and resistant to common attacks. Require high stats or strong spells to deal with efficiently.

| Enemy | HP | ATK | DEF | SPD | XP | Special |
|-------|-----|-----|-----|-----|----|---------|
| Imp | 18 | 1d6 | 0 | 0.80 | 40 | Fire Dart (ranged 1d6 fire), Blink (teleports when threatened) |
| Hellhound | 30 | 1d8 | 2 | 0.83 | 60 | Fire Breath (3-tile cone, 2d6 fire), Fire Immune |
| Demon Warrior | 50 | 2d8 | 5 | 1.0 | 90 | Resistant to non-magical weapons (-3 DMG) |
| Shadow Fiend | 35 | 2d6 | 3 | 0.91 | 85 | Shadowmeld (invisible until attacks), Mana Burn |
| Pit Spawn | 60 | 2d10 | 6 | 1.05 | 110 | Tremor (AoE knockback), Immune to fire |

---

## Bosses

Bosses appear in a sealed room at the end of their floor. The room cannot be exited until the boss is defeated. Bosses are marked on the map with a distinct icon once the player enters the room.

### Floor 3: Goblin Warchief

*The biggest, meanest goblin in the dungeon — and he knows it.*

| HP | ATK | DEF | SPD | XP |
|----|-----|-----|-----|----|
| 80 | 1d10 | 4 | 1.0 | 150 |

**Abilities:**
- **Battle Cry (1/fight):** Summons 2 Goblins and 1 Goblin Archer at turn 1
- **Enrage (< 40% HP):** Gains +3 ATK and SPD drops to 0.83 for remainder of fight
- **Throwing Axe:** Ranged attack (1d8) used if player is > 4 tiles away

**Loot:** Guaranteed Uncommon weapon + small gold drop

---

### Floor 6: Bone Lord

*A towering skeleton warlord bound together by dark necromancy. Destroying it weakens the magic of the catacombs.*

| HP | ATK | DEF | SPD | XP |
|----|-----|-----|-----|----|
| 140 | 2d8 | 6 | 1.11 | 280 |

**Abilities:**
- **Reassemble (2× fight):** When reduced to 0 HP for the first two times, regenerates to 30 HP instead of dying (third time is permanent)
- **Summon Minions:** Raises 1-2 Skeletons from the room's "bone piles" (3 total piles in the room)
- **Bone Shards:** AoE attack (2d4) hitting all adjacent tiles

**Loot:** Guaranteed Rare armor piece + spellbook (Smite or Healing Word)

---

### Floor 9: Pit Fiend

*Ancient demon bound to the depths. Its name was struck from all records. It remembers yours.*

| HP | ATK | DEF | SPD | XP |
|----|-----|-----|-----|----|
| 200 | 3d8 | 8 | 0.91 | 450 |

**Abilities:**
- **Hellfire Aura:** Any enemy (or ally) within 2 tiles takes 3 fire damage per turn
- **Infernal Charge:** Dashes to player's location, dealing 2d10 + knockback
- **Summon Imp (every 5 turns):** Calls 1 Imp from the ether
- **Fire Immunity:** Takes 0 damage from fire

**Loot:** Guaranteed Rare/Legendary ring or amulet + Tome of high-tier spell

---

### Floor 10: Shadow Archon (Final Boss)

*The dungeon's true master. It does not speak. It does not negotiate. It simply unmakes.*

| HP | ATK | DEF | SPD | XP |
|----|-----|-----|-----|----|
| 320 | 3d10 | 10 | 0.95 | 1000 |

**Phase 1 (100% → 50% HP):**
- **Shadow Strike:** Single target melee (3d10)
- **Void Tendrils:** Roots player in place for 2 turns (every 6 turns)
- **Shade Summon:** Summons 2 Shadow Fiends when below 75% HP

**Phase 2 (< 50% HP — transitions with a dramatic visual):**
- All Phase 1 abilities continue
- **Darkness Pulse:** AoE nova every 4 turns, 3d8 necrotic damage in 5-tile radius
- **Mana Void:** Drains 20 mana from player on hit
- **Desperate Shadows:** Summons 1 additional Shadow Fiend every 4 turns

**Defeat:**
- Shadow Archon collapses. The room's darkness lifts.
- The **Amulet of Dominion** becomes accessible on its pedestal.
- Picking it up triggers the **victory screen**.

---

## Enemy Design Notes

- **Reward exploration:** Enemies in dark, unexplored corridors are more likely to be ambush types (bats, shadow rogues).
- **Readable threat levels:** Enemy sprites and names should telegraph power clearly. A "Vampire" should look scarier than a "Zombie."
- **Boss rooms:** Always a sealed room with a single entrance. Room layout is hand-designed (or at least constrained — no open corridors).
- **Spawn weights by floor:** See the `monster_spawner.rs` builder for implementation. Each faction has a weight table per floor depth.
- **No XP for fleeing monsters:** Only awarded on kill.
