# Bestiary

## Overview

Enemies are organized into eight factions spanning a 26-floor dungeon. Each 5-floor zone introduces new mechanics and a zone boss. Encounters escalate in tactical complexity — not just bigger numbers — as the player descends.

## Unified Stat System

**Monsters and the player share the same ECS component system.** Both have:

| Component | Purpose |
|-----------|---------|
| `Attributes` | Raw stats: STR, DEX, CON, AGI, INT, PER |
| `AttributeModifiers` | Additive modifiers from status effects, gear, horde leaders |
| `CombatStats` | Derived bonuses: damage_bonus, hit_chance, dodge_chance, armor |
| `Level` | Scales HP and combat effectiveness |
| `Health` | Current/max HP |
| `Viewshed` | Vision range, driven by PER |

`stat_recalculation_system` runs identically for every entity. The combat pipeline (`hit_check → damage_roll → armor_reduction → apply_damage`) has no player vs. monster special cases.

### Stat Formulas

```
bonus = stat - 10                         (negative bonuses are valid and impactful)
HP (max) = base_hp + (CON_bonus × level)
hit_chance    = 10 + STR_bonus
dodge_chance  = 5  + DEX_bonus
damage_bonus  = STR_bonus  (added to every roll)
delay         = 1.0 - (AGI_bonus × 0.025), clamped [0.5, 2.0]
vision_range  = 8 + PER_bonus
mana_max      = INT × 5
```

**Speed reference**: AGI 14 → delay 0.90; AGI 18 → 0.80; AGI 4 → 1.15; AGI 2 → 1.20

### Monster Armor

Monsters have a `base_armor` field in `MonsterAsset` that maps to `CombatStats.armor` at spawn time. This provides flat damage reduction per hit, identical to how player armor works.

### How Table Stats Map to `MonsterAsset` Fields

| Table column | Asset field | Notes |
|---|---|---|
| HP | `base_hp` | Added to CON bonus × level in stat system |
| ATK | `damage` | Dice string, e.g. `"2d6+1"` |
| DEF | `base_armor` | Flat damage reduction |
| SPD / delay | `agility` | `delay = 1.0 - (AGI-10) × 0.025` |
| STR/DEX/CON/AGI/INT/PER | direct fields | Set per-monster in `monsters.ron` |

---

## Zone Overview

| Zone | Floors | Primary Faction | Secondary Faction |
|------|--------|-----------------|-------------------|
| 1 | 1-5 | Animals + Goblins | — |
| 2 | 6-10 | Goblins (elite) + Orcs | Undead (emerging) |
| 3 | 11-16 | Orcs + Ogres + Undead | — |
| 4 | 17-21 | Trolls + Deep Undead | Demons (emerging) |
| 5 | 22-26 | Demons + Dragons | Undead lords |

---

## Faction: Animals & Beasts
*Floors 1-14, dominant on 1-7*

Instinct-driven creatures. Fast and fragile (rats, bats) or slow and devastating (bears). A threat in groups; manageable alone. Teaches basic combat and squad alerting.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Giant Rat | 1-4 | 1 | 5 | 7 | 10 | 10 | 14 | 4 | 8 | 0 | 1d3 | 0.90 | 5 | Squad member |
| Giant Bat | 1-5 | 1 | 4 | 5 | 13 | 8 | 18 | 2 | 10 | 0 | 1d3 | 0.80 | 2 | Erratic movement; very fast, very fragile |
| Venomous Snake | 3-8 | 2 | 6 | 8 | 14 | 10 | 12 | 4 | 12 | 0 | 1d4 | 0.95 | 6 | Poison on hit |
| Wolf | 4-9 | 2 | 10 | 12 | 11 | 11 | 14 | 6 | 14 | 0 | 1d6 | 0.90 | 12 | Squad member; wide vision (range 12) |
| Wild Boar | 5-10 | 2 | 12 | 14 | 8 | 13 | 12 | 4 | 8 | 1 | 1d8 | 0.95 | 18 | High STR; future charge mechanic |
| Giant Spider | 6-12 | 3 | 10 | 10 | 14 | 10 | 10 | 4 | 12 | 0 | 1d4 | 1.00 | 10 | Poison + Web (slow) |
| Cave Bear | 8-14 | 4 | 18 | 17 | 6 | 15 | 8 | 4 | 8 | 2 | 2d6 | 1.05 | 38 | Maul: high base damage |

**Design notes:**
- Bats have only 2 effective HP — they die in one hit but are almost impossible to avoid being hit by first (delay 0.80).
- Giant Rats come in squads; alone they're trivial, together they overwhelm.
- The Cave Bear is the zone's capstone predator — introduced when the player has armor.
- **Wild Boar Charge**: Currently uses high base STR (14) for implicit charge flavor. Future: `Charging` component grants temporary +4 STR when attacking from ≥3 tiles away.

---

## Faction: Goblins
*Floors 1-12, dominant on 1-8*

Small, cowardly, numerous. Fight in organized squads with ranged support and a shaman healer. The Warchief acts as a squad leader whose death weakens the group.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Goblin | 1-7 | 1 | 5 | 8 | 12 | 10 | 10 | 6 | 8 | 0 | 1d4 | 1.00 | 5 | Squad member; cowardly (flees <30% HP) |
| Goblin Archer | 2-8 | 1 | 5 | 7 | 13 | 8 | 10 | 6 | 10 | 0 | 1d6 | 1.00 | 3 | ranged_range: 8; squad member |
| Goblin Brute | 3-9 | 2 | 10 | 14 | 8 | 13 | 8 | 4 | 7 | 1 | 1d8 | 1.05 | 16 | Tank of the group; solitary |
| Goblin Shaman | 4-11 | 2 | 6 | 6 | 10 | 10 | 10 | 14 | 10 | 0 | 1d4 | 1.00 | 6 | Spells: magic_missile, heal_ally; mana 70 |
| Goblin Warchief | 5-12 | 3 | 16 | 14 | 12 | 13 | 12 | 8 | 12 | 2 | 1d8 | 0.95 | 25 | Squad leader; +2 STR/AGI aura |

**Design notes:**
- The Goblin Shaman is the first caster the player encounters — teaches that enemies can deal magic damage from range. Also the first heal_ally user, making it a priority target.
- The Warchief buffs all nearby goblins with +2 STR/AGI. Killing the leader removes the buff and may cause remaining goblins to flee.
- Goblin squads are the player's introduction to coordinated enemies.

---

## Faction: Orcs
*Floors 5-18, dominant on 7-15*

Bigger, meaner, tactically organized. Orcs overlap with goblins intentionally — floors with both factions create chaotic multi-front fights.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Orc | 5-11 | 1 | 10 | 14 | 8 | 12 | 8 | 6 | 8 | 1 | 1d6 | 1.05 | 10 | regen: 20; squad member |
| Orc Archer | 6-13 | 2 | 8 | 11 | 14 | 11 | 10 | 7 | 12 | 0 | 1d8 | 1.00 | 10 | ranged_range: 10; squad member |
| Orc Warrior | 7-15 | 3 | 14 | 16 | 9 | 14 | 8 | 8 | 8 | 3 | 1d10 | 1.05 | 26 | High armor; squad member |
| Orc Shaman | 8-16 | 3 | 8 | 9 | 10 | 11 | 9 | 16 | 10 | 0 | 1d4 | 1.03 | 11 | Spells: magic_missile, fire_dart, heal_self; mana 80 |
| Orc Warlord | 10-18 | 4 | 20 | 18 | 10 | 16 | 10 | 10 | 12 | 4 | 2d8 | 1.00 | 44 | Squad leader; +3 STR/CON aura |

**Design notes:**
- The base Orc has regen (heals 1 HP every 20 turns) — teaches players not to let enemies sit at low HP.
- Orc Shaman has higher INT than Goblin Shaman, making its magic_missile noticeably stronger.
- Orc Warrior is the first heavily-armored enemy (DEF 3); teaches the player that armor reduces damage.

---

## Faction: Ogres
*Floors 10-22, dominant on 12-18*

Slow brutes with massive HP and devastating STR. The Ogre Mage adds magical threat from behind the frontline.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Ogre | 10-18 | 4 | 25 | 18 | 6 | 16 | 4 | 4 | 6 | 3 | 2d8 | 1.15 | 49 | Massive, slow; STR +8 to every hit |
| Stone Ogre | 14-22 | 5 | 30 | 18 | 5 | 18 | 4 | 4 | 6 | 6 | 2d10 | 1.20 | 70 | Extra armor; Knockback 1 tile on hit |
| Ogre Mage | 15-22 | 5 | 18 | 12 | 10 | 13 | 8 | 18 | 12 | 1 | 1d6 | 1.05 | 33 | Spells: magic_missile, fire_dart, vampiric_strike; mana 90 |

**Design notes:**
- Ogres introduce a critical lesson: you can't trade hits with everything. Their delay of 1.15-1.20 means the player can kite them.
- The Ogre Mage is a priority target (high INT = strong spells) that hides behind regular Ogres — interesting tactical decisions.
- Stone Ogre knockback pushes the player into walls or other enemies.

---

## Faction: Trolls
*Floors 13-24, dominant on 15-20*

Regenerating tanks. The central tension of this zone: regen means you must either burst trolls down or use fire/acid (future mechanic).

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Cave Troll | 13-20 | 5 | 22 | 16 | 7 | 18 | 6 | 4 | 8 | 2 | 2d6 | 1.10 | 62 | regen: 3 (fast — 1 HP every 3 turns) |
| Mountain Troll | 17-24 | 6 | 28 | 18 | 6 | 20 | 4 | 4 | 6 | 3 | 2d8 | 1.15 | 88 | regen: 2 (extreme — 1 HP every 2 turns) |

---

## Faction: Undead
*Floors 7-26, secondary throughout*

Undead appear across many zones as a secondary threat, giving the dungeon vertical continuity. Skeletons at floor 7 foreshadow the Lich at floor 22. Immune to poison.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Skeleton | 7-14 | 2 | 10 | 11 | 8 | 10 | 10 | 4 | 8 | 2 | 1d6 | 1.00 | 10 | Reforms once at 2 HP |
| Zombie | 8-15 | 2 | 14 | 13 | 4 | 14 | 4 | 4 | 6 | 1 | 1d8 | 1.15 | 22 | Plague Touch: temp -1 CON on hit |
| Bone Archer | 8-16 | 3 | 8 | 9 | 14 | 10 | 10 | 4 | 12 | 1 | 1d8 | 1.00 | 8 | ranged_range: 10 |
| Ghoul | 10-17 | 3 | 12 | 13 | 12 | 12 | 12 | 6 | 10 | 0 | 1d6 | 0.95 | 18 | Paralyzing touch: player skips turn |
| Lich Apprentice | 11-19 | 3 | 8 | 8 | 10 | 9 | 10 | 14 | 12 | 0 | 1d4 | 1.00 | 5 | Spells: magic_missile, heal_self |
| Wight | 14-21 | 4 | 16 | 14 | 10 | 13 | 10 | 10 | 12 | 2 | 1d8 | 1.00 | 28 | Attribute drain: temp -1 STR on hit |
| Wraith | 16-23 | 4 | 14 | 12 | 14 | 10 | 14 | 12 | 14 | 0 | 1d6 | 0.90 | 14 | Phase-walk (passes through walls); Life Drain |
| Vampire | 19-26 | 5 | 20 | 15 | 14 | 14 | 12 | 14 | 14 | 1 | 1d10 | 0.95 | 40 | Spells: vampiric_strike, heal_self |
| Lich | 22-26 | 7 | 18 | 9 | 12 | 12 | 10 | 20 | 16 | 2 | 1d4 | 1.00 | 30 | Spells: magic_missile, vampiric_strike, fire_dart; mini-boss tier |

**Design notes:**
- Skeleton Reform: on first death, rises again at 2 HP. Teaches "some enemies come back."
- Ghoul paralysis is the most punishing Zone 3 mechanic — losing a turn while surrounded is lethal.
- Wraith phase-walk lets it cut off escape routes through walls — terrifying in corridors.
- The Lich has INT 20 (mana: 100) and three spells, making it one of the most dangerous casters.

---

## Faction: Demons
*Floors 15-26, dominant on 20-26*

Powerful, fast, elemental. Require high stats or strong spells to handle efficiently.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Imp | 15-22 | 4 | 12 | 9 | 14 | 10 | 16 | 12 | 12 | 0 | 1d4 | 0.85 | 12 | ranged_range: 8; Spells: fire_dart; Blink |
| Hellhound | 18-24 | 5 | 18 | 14 | 12 | 13 | 14 | 8 | 14 | 1 | 1d8 | 0.90 | 33 | ranged_range: 3 (fire breath); Fire Immune |
| Demon Warrior | 20-26 | 6 | 22 | 18 | 11 | 15 | 10 | 10 | 12 | 5 | 2d8 | 1.00 | 52 | High armor; resistant to non-magical attacks (-3 DMG) |
| Shadow Fiend | 22-26 | 6 | 18 | 14 | 16 | 12 | 16 | 14 | 16 | 1 | 1d10 | 0.85 | 30 | Spells: vampiric_strike; Mana Burn (drains player mana on hit) |

---

## Faction: Dragons
*Floors 21-26, rare*

The apex predators of the dungeon. Fast, armored, devastating ranged breath weapon, and fire immune.

| Monster | Floors | Lvl | base_hp | STR | DEX | CON | AGI | INT | PER | DEF | ATK | delay | Eff. HP | Special |
|---------|--------|-----|---------|-----|-----|-----|-----|-----|-----|-----|-----|-------|---------|---------|
| Dragon Whelp | 21-26 | 5 | 24 | 16 | 12 | 14 | 14 | 10 | 14 | 4 | 2d6 | 0.90 | 44 | ranged_range: 5 (breath); Fire Immune |
| Young Dragon | 24-26 | 7 | 35 | 20 | 12 | 18 | 12 | 14 | 16 | 6 | 2d10 | 0.95 | 91 | ranged_range: 6; mini-boss tier |

---

## Bosses

Zone bosses appear every 5 floors in sealed rooms. See [BOSS_SYSTEM.md](BOSS_SYSTEM.md) for behavior tree AI and implementation details.

| Floor | Boss | Lvl | base_hp | STR | CON | AGI | INT | DEF | ATK | Eff. HP | Key Mechanics |
|-------|------|-----|---------|-----|-----|-----|-----|-----|-----|---------|---------------|
| 5 | Goblin Warchief | 3 | 28 | 15 | 14 | 12 | 10 | 3 | 1d10 | 37 | Squad leader; Battle Cry (summons 2 goblins) |
| 10 | Orc Warlord "Grak" | 5 | 35 | 20 | 16 | 10 | 10 | 5 | 2d8 | 65 | Enrage <40% HP; Cleave (hits adjacent) |
| 15 | Bone Lord | 6 | 40 | 14 | 14 | 8 | 8 | 7 | 2d8 | 64 | Reassemble ×2; Summon Skeletons; Bone Shards (AoE) |
| 20 | Vampire Lord | 7 | 40 | 16 | 16 | 14 | 16 | 3 | 1d12 | 82 | Vampiric Strike; Fear Aura; phase at <50% HP |
| 26 | Shadow Archon | 8 | 60 | 18 | 18 | 14 | 20 | 10 | 3d10 | 108 | 2-phase; Void Tendrils; Darkness Pulse; Mana Void |

---

## Mini-Boss System

Named, 1.5×-stat elites that spawn 0-1 per floor (~40% chance). Always drop guaranteed loot. Appear in normal rooms among regular monsters — not in sealed boss rooms.

**Stat scaling at spawn time**: base_hp × 2, all attributes +4, all loot table entries forced to 100%.

| Name | Type | Zone | Floors | Signature |
|------|------|------|--------|-----------|
| Skittersong | Giant Rat | 1 | 2-4 | Rallies all rats in room; +4 STR/AGI |
| Grizzlefang | Cave Bear | 2 | 6-8 | Guaranteed rare pelt; Maul every turn |
| Kruul the Hexer | Goblin Shaman | 2 | 6-9 | 3 spell slots; heals allies every 2 turns |
| Ironfang | Orc Warrior | 3 | 9-12 | Guaranteed rare weapon; base_armor 8 |
| Mistress Eight-Eyes | Giant Spider | 3 | 9-13 | Web field (room-wide slow) + triple poison |
| Rotbones | Zombie | 3 | 11-15 | Plague Touch hits adjacent tiles |
| Thornwall | Cave Troll | 4 | 14-18 | regen: 1 (heals every turn); base_armor 5 |
| The Pale Knight | Wight | 4 | 16-20 | Attribute drain on every hit |
| Ashkur the Ashborn | Imp | 5 | 21-25 | 3× fire_dart per turn; Blink ability |
| Vorthax the Chained | Demon Warrior | 5 | 22-26 | Immune to first 3 hits; guaranteed legendary |

---

## Horde / Squad System

A **horde** is a spawn group: 3-6 monsters that spawn together in the same room and move as a coordinated unit. When one member spots the player, the whole squad activates.

### Squad Compositions

| Horde Type | Typical Composition | Leader |
|------------|---------------------|--------|
| Rat Swarm | 4-5 Giant Rats | None (leaderless) |
| Wolf Pack | 2-3 Wolves | None (collective) |
| Goblin Raid | 2 Goblins + 1 Archer + 1 Shaman | Goblin Warchief (if spawned) |
| Orc Warband | 2 Orc Warriors + 1 Orc Archer | Orc Warlord (if spawned) |

### Leader Buff

When a `HordeLeader` is alive, nearby squad members receive `AttributeModifiers` (+2 STR/AGI for goblins, +3 STR/CON for orcs). On leader death, the buff is removed — making the leader a high-priority target.

### Movement

All squad members pathfind to the same `last_known_player_pos`. Individual A* paths cause them to naturally converge from different angles — flanking behavior emerges without extra code.

Monsters that never squad: solitary animals (Cave Bear, Wild Boar), Ogres, Undead, Demons, Dragons.

---

## Spells for Monster Casters

### Existing Spells (No New Code)
- `magic_missile` — Damage "2d4", int_scaling: true, target: NearestEnemy
- `heal_self` — HealCaster "1d6", target: Caster

### New Data-Only Spells

| Spell | Target | Effects | Mana | Cooldown | Monsters |
|-------|--------|---------|------|----------|----------|
| `fire_dart` | NearestEnemy | Damage "1d8" | 8 | 3 | Imp, Orc Shaman, Hellhound |
| `minor_heal` | Caster | HealCaster "1d4" | 6 | 2 | Goblin Shaman |
| `vampiric_strike` | NearestEnemy | Damage "2d4" + HealCaster "1d4" | 12 | 4 | Vampire, Ogre Mage, Shadow Fiend, Lich |

`vampiric_strike` works with no new code — multi-effect spells already iterate all effects. Damage hits the enemy; HealCaster heals the caster. Free life-steal.

### Spells Requiring New Code

| Spell | Target | Effects | Mana | Cooldown | Monsters | Code Needed |
|-------|--------|---------|------|----------|----------|-------------|
| `heal_ally` | NearestAlly | HealTarget "2d4", int_scaling: true | 15 | 5 | Goblin Shaman, Orc Shaman | New `SpellTarget::NearestAlly` variant |

### Spell Assignments

| Monster | Spells |
|---------|--------|
| Goblin Shaman | magic_missile, heal_ally |
| Orc Shaman | magic_missile, fire_dart, heal_self |
| Ogre Mage | magic_missile, fire_dart, vampiric_strike |
| Lich Apprentice | magic_missile, heal_self |
| Imp | fire_dart |
| Vampire | vampiric_strike, heal_self |
| Shadow Fiend | vampiric_strike |
| Lich | magic_missile, fire_dart, vampiric_strike |

---

## Gradual Mechanic Introduction

Each zone introduces one new concept via weaker monsters before the zone boss tests mastery.

| Zone | Floors | New Mechanic | Introduced Via | Zone Boss Tests |
|------|--------|--------------|----------------|-----------------|
| 1 | 1-5 | Ranged attacks, squad alerting | Goblin Archer (ranged), Rat/Wolf squads | Goblin Warchief: squad leader + ranged combo |
| 2 | 6-10 | Poison, spell casters, skeleton reform | Snake, Shaman, Skeleton | Orc Warlord: squad + caster-behind-frontline |
| 3 | 11-16 | Regen, heavy armor, paralysis | Cave Troll, Ogre, Ghoul | Bone Lord: high armor + reforms multiple times |
| 4 | 17-21 | Attribute drain, elemental (fire), knockback | Wight, Hellhound, Stone Ogre | Vampire Lord: drains stats while life-stealing |
| 5 | 22-26 | Mana burn, phase-walk, multi-ability | Shadow Fiend, Wraith, Young Dragon | Shadow Archon: 2-phase with everything |

---

## Additional Mechanics

### Status Effects
- **Poison**: `Poisoned { damage_per_turn, turns_remaining }` — drains HP each turn. Applied by Venomous Snake, Giant Spider, Zombie. Zone 2.
- **Paralysis**: `Stunned { turns_remaining }` — player skips turn. Applied by Ghoul. Zone 3.

### Monster Fleeing
- `is_cowardly: bool` on MonsterAsset; Goblins flee when below 30% HP (pathfind away from player).
- Teaches the player to not let wounded enemies escape and potentially heal.

### Elemental Immunity
- `FireImmune`: Hellhound, Dragon Whelp, Young Dragon
- `PoisonImmune`: all Undead (Skeleton, Zombie, Bone Archer, Ghoul, Wight, Wraith, Vampire, Lich)

### Knockback
- Stone Ogre and Shadow Archon push the player 1-2 tiles on hit. Hitting a wall deals 1 collision damage.

### Monster Alliance Hostility
- Goblins and Orcs do NOT form squads together (separate factions).
- Future: monsters with different faction IDs may aggro each other — the player can exploit this.

---

## Enemy Design Notes

- **Reward exploration:** Enemies in dark, unexplored corridors are more likely to be ambush types (bats, wraiths).
- **Readable threat levels:** Enemy sprites and names should telegraph power clearly. A "Vampire" should look scarier than a "Zombie."
- **Boss rooms:** Always a sealed room with a single entrance. Room layout is hand-designed.
- **Spawn weights by floor:** See `monster_spawner.rs`. Each faction has a weight table per floor depth.
- **No XP for fleeing monsters:** Only awarded on kill.
