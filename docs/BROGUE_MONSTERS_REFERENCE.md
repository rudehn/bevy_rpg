# Brogue Monster Reference

A comprehensive reference of every monster in Brogue CE, organized by depth
tier. Used as inspiration and design reference for The Veiled Tyrant.

Source: BrogueCE source code (`BrogueCE/src/brogue/Globals.c`,
`BrogueCE/src/variants/GlobalsBrogue.c`, `BrogueCE/src/brogue/Monsters.c`).

---

## Monster Roster by Depth

Stats: **HP, Def** (defense), **Acc** (accuracy), **Damage** (min-max, clump
factor), **Move** speed, **Atk** speed. Speed 100 = normal; lower = faster.

### Depth 1-5: Shallows

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Rat | 6 | 0 | 80 | 1-3 | 100 | 100 | None |
| Kobold | 7 | 0 | 80 | 1-4 | 100 | 100 | None |
| Jackal | 8 | 0 | 70 | 2-4 | 50 | 100 | Packs of 1-3; fast movement |
| Monkey | 12 | 17 | 100 | 1-3 | 100 | 100 | **Steals item on hit, then flees** |
| Eel | 18 | 27 | 100 | 3-7 | 50 | 100 | Deep water only; submerges |
| Bloat | 4 | 0 | 100 | 0 | 100 | 100 | **Explodes into poison gas on death** |
| Pit Bloat | 4 | 0 | 100 | 0 | 100 | 100 | **Explodes and destroys the floor** |

### Depth 3-10: Goblin Territory

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Goblin | 15 | 10 | 70 | 2-5 | 100 | 100 | Penetrating attack; avoids corridors |
| Goblin Conjurer | 10 | 10 | 70 | 2-4 | 100 | 100 | **Summons 3-5 Spectral Blades** |
| Goblin Mystic | 10 | 10 | 70 | 2-4 | 100 | 100 | Casts **Shielding** on allies |
| Goblin Totem | 30 | 0 | 0 | 0 | — | 300 | Immobile; casts **Haste** on allies, **Spark** on enemies |
| Goblin Warlord | 30 | 17 | 100 | 3-6 | 100 | 100 | Boss; summons conjurer + goblins; penetrating |

### Depth 4-13: Mid-Shallows

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Toad | 18 | 0 | 90 | 1-4 | 100 | 100 | **Causes hallucination** |
| Pink Jelly | 50 | 0 | 85 | 1-3 | 100 | 100 | **Splits into two when hit** |
| Vampire Bat | 18 | 25 | 100 | 2-6 | 50 | 100 | **Life steal** (40% of damage dealt) |
| Acid Mound | 15 | 10 | 70 | 1-3 | 100 | 100 | **Corrodes armor on hit; corrodes weapon when hit** |
| Arrow Turret | 30 | 0 | 90 | 2-6 | — | 250 | Wall-mounted ranged |

### Depth 7-14: Mid-Dungeon

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Centipede | 20 | 20 | 80 | 4-12 | 100 | 100 | **Causes weakness** |
| Ogre | 55 | 60 | 125 | 9-13 | 100 | 200 | **Stagger** (knockback 1 tile) |
| Bog Monster | 55 | 60 | 5000 | 3-4 | 200 | 100 | Mud only; submerges; **seizes** target |
| Spider | 20 | 70 | 90 | 3-4 | 100 | 200 | **Poison damage**; shoots **web bolts** |

### Depth 9-17: Dar Territory

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Dar Blademaster | 35 | 70 | 160 | 5-9 | 100 | 100 | Casts **Blink** to teleport into melee |
| Dar Priestess | 20 | 60 | 100 | 2-5 | 100 | 100 | **Negation**, Healing, Haste, Spark |
| Dar Battlemage | 20 | 60 | 100 | 1-3 | 100 | 100 | **Fire**, **Slow**, **Discord** |
| Wisp | 10 | 90 | 100 | 0 | 100 | 100 | **Ignites on hit**; fire aura; ignites terrain |

### Depth 10-19: Deep Mid

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Wraith | 50 | 60 | 120 | 6-13 | 50 | 100 | Flies; **permanently invisible** |
| Zombie | 80 | 0 | 120 | 7-12 | 100 | 100 | **Emits rot gas every turn** (nausea) |
| Explosive Bloat | 10 | 0 | 100 | 0 | 100 | 100 | **Massive fire explosion on death** |
| Troll | 65 | 70 | 125 | 10-15 | 100 | 100 | **Regenerates 10x faster** |
| Spark Turret | 80 | 0 | 100 | 0 | — | 150 | Chain lightning |
| Ogre Totem | 70 | 0 | 0 | 0 | — | 400 | Immobile; **Healing** on ogres, **Slow** on player |
| Ogre Shaman | 45 | 40 | 100 | 5-9 | 100 | 200 | Haste, Spark; **summons ogres** |

### Depth 13-22: Deep Dungeon

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Naga | 75 | 70 | 150 | 7-11 | 100 | 100 | Deep water; **hits ALL adjacent** (cleave) |
| Salamander | 60 | 70 | 150 | 5-11 | 100 | 100 | Lava; **whip attack (2-tile reach)**; fire trail |
| Centaur | 35 | 50 | 175 | 4-8 | 50 | 100 | **Ranged physical**; kites at distance |
| Acidic Jelly | 60 | 0 | 115 | 2-6 | 100 | 100 | Splits + corrodes armor + corrodes weapons |
| Pixie | 10 | 90 | 100 | 1-3 | 50 | 100 | **Negation, Slow, Discord, Spark** |
| Kraken | 120 | 0 | 150 | 15-20 | 50 | 100 | Deep water; **seizes** + pulls into water |
| Dart Turret | 20 | 0 | 140 | 1-2 | — | 250 | **Poison + Weakness** darts |
| Flame Turret | 40 | 0 | 150 | 1-2 | — | 250 | Fire bolts |

### Depth 16-26: Abyss

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Phantom | 35 | 70 | 160 | 12-18 | 50 | 200 | **Permanently invisible**; flies; flits |
| Imp | 35 | 90 | 225 | 4-9 | 100 | 100 | **Steals items + blinks away** |
| Fury | 19 | 90 | 200 | 6-11 | 50 | 100 | Always packs of 3-5; extremely fast |
| Revenant | 30 | 0 | 200 | 15-20 | 100 | 100 | **Immune to weapons**; must use magic/fire/environment |

### Depth 21+: Endgame

| Monster | HP | Def | Acc | Damage | Move | Atk | Special |
|---|---|---|---|---|---|---|---|
| Lich | 35 | 80 | 175 | 2-6 | 100 | 100 | Summons phantoms/furies; fire bolt; **resurrects from phylactery** |
| Phylactery | 30 | 0 | 0 | 0 | — | 150 | Immobile; must destroy to permanently kill lich |
| Golem | 400 | 70 | 225 | 4-8 | 100 | 100 | **Reflects 50% of bolts**; dies instantly to negation |
| Tentacle Horror | 120 | 95 | 225 | 25-35 | 100 | 100 | Pure physical juggernaut |
| Dragon | 150 | 90 | 250 | 25-50 | 50 | 200 | **Dragonfire breath**; hits all adjacent; **always drops an item** |

### Bosses (Machine Rooms)

| Monster | HP | Def | Acc | Damage | Special |
|---|---|---|---|---|---|
| Goblin Warlord | 30 | 17 | 100 | 3-6 | Summons a goblin army |
| Black Jelly | 120 | 0 | 130 | 3-8 | Splits + corrodes armor/weapons |
| Vampire | 75 | 60 | 120 | 4-15 | Life steal; on death → 3 bats that can reform |
| Flamedancer | 65 | 80 | 120 | 3-8 | Fire melee + fire bolts + fire corona trail |

### Legendary Allies (freed from machine rooms)

| Monster | HP | Def | Acc | Damage | Special |
|---|---|---|---|---|---|
| Unicorn | 40 | 60 | 175 | 2-10 | Ranged **Healing + Shielding** bolts |
| Ifrit | 40 | 75 | 175 | 5-13 | Fast flying fighter; casts **Discord**; fire immune |
| Phoenix | 30 | 70 | 175 | 4-10 | On death → egg → hatches new phoenix; fire immune |
| Mangrove Dryad | 70 | 60 | 175 | 2-8 | Casts **Ancient Spirit Vines** (mass entangle) |

---

## Melee Special Abilities

| Ability | Monsters | Effect |
|---|---|---|
| Steal + Flee | Monkey, Imp | Steals an item on hit, then flees |
| Hallucination | Toad | All monsters appear as random types |
| Ignite | Wisp, Flamedancer | Sets target on fire |
| Corrode Armor | Acid Mound, Acidic Jelly | Permanently reduces armor enchantment on hit |
| Corrode Weapon | Acid Mound, Acidic Jelly | Permanently reduces weapon enchantment when you hit them |
| Poison | Spider | Damage dealt as poison (delayed damage over time) |
| Weakness | Centipede, Dart Turret | Reduces strength; may force unequipping heavy gear |
| Life Steal | Vampire Bat, Vampire | Heals 40-90% of damage dealt |
| Stagger | Ogre | Knocks target back one tile |
| Penetrating | Goblin, Goblin Warlord | Hits through one layer of enemies (spear-like) |
| Cleave | Naga, Dragon | Hits ALL adjacent enemies |
| Whip | Salamander | Attacks reach 2 tiles in a cardinal line |
| Seize | Bog Monster, Kraken | Grapples target, preventing movement |

## Splitting / Kamikaze

| Monster | Trigger | Effect |
|---|---|---|
| Pink Jelly | When struck | Splits into two; new jelly gets half HP |
| Acidic Jelly | When struck | Splits + corrodes your weapon |
| Black Jelly | When struck | Splits (boss-tier HP) |
| Bloat | On death | Poison gas cloud |
| Pit Bloat | On death | Destroys the floor tile (creates pit) |
| Explosive Bloat | On death | Massive fire explosion |

## Bolt Casters

| Monster | Bolts | Role |
|---|---|---|
| Goblin Totem | Haste, Spark | Buff allies, zap enemies |
| Goblin Mystic | Shielding | Shield allies |
| Ogre Totem | Healing, Slow | Heal ogres, slow player |
| Ogre Shaman | Haste, Spark | Buff + summon ogres |
| Dar Priestess | Negation, Healing, Haste, Spark | Full support caster |
| Dar Battlemage | Fire, Slow, Discord | Offensive caster |
| Dar Blademaster | Blinking | Gap closer |
| Imp | Blinking | Escape after stealing |
| Pixie | Negation, Slow, Discord, Spark | Full debuff suite |
| Spider | Spiderweb | Ranged entangle |
| Centaur | Distance Attack | Ranged physical |
| Arrow Turret | Distance Attack | Ranged physical |
| Spark Turret | Spark | Chain lightning |
| Dart Turret | Poison Dart | Poison + weakness |
| Flame Turret | Fire | Fire bolt |
| Sentinel | Healing, Spark | Heal other sentinels |
| Lich | Fire | Also summons |
| Dragon | Dragonfire | Devastating fire breath |
| Vampire | Blinking, Discord | Hit-and-run + chaos |
| Flamedancer | Fire | Ranged fire + fire corona trail |

## Summoners

| Monster | Summons | Notes |
|---|---|---|
| Goblin Conjurer | 3-5 Spectral Blades | Die when conjurer dies |
| Goblin Warlord | 1 Conjurer + 3-4 Goblins | Summoned at distance |
| Ogre Shaman | 1 Ogre | |
| Lich | 2-3 Phantoms OR 2-3 Furies | |
| Vampire | 3 Vampire Bats | On death; bats can reform into vampire |

## Resurrection Mechanics

| Monster | Mechanism |
|---|---|
| Lich / Phylactery | Lich dies → phylactery summons new lich. Destroy phylactery first. |
| Vampire | Dies → blood explosion → 3 vampire bats. Kill all bats before one reforms. |
| Phoenix / Egg | Dies → leaves egg → egg hatches new phoenix if not destroyed. |

---

## Terrain Interactions

### Water

| Trait | Monsters | Effect |
|---|---|---|
| Restricted to liquid | Eel, Bog Monster, Kraken | Can ONLY move on deep water / mud |
| Immune to water | Eel, Kraken, Naga | Full speed in deep water |
| Submerges | Eel, Bog Monster, Kraken, Naga, Salamander | Hides underwater/in lava, becoming invisible |

- Non-immune monsters entering deep water are slowed and may drown
- Naga leaves puddles of water as it moves

### Lava / Fire

| Trait | Monsters | Effect |
|---|---|---|
| Fire immune | Wisp, Salamander, Flamedancer, Dragon, Phoenix, Ifrit | Cannot burn, survives lava |
| Fiery | Wisp, Salamander, Flamedancer | Carries fire aura, ignites flammable terrain |

- Salamander is restricted to lava, leaves fire trails
- Non-fire-immune creatures burn on lava or when ignited

### Gas

- Zombie emits rot gas every turn (causes nausea)
- Bloat explodes into poison gas on death
- Swamp gas ignites from fire — chain explosions possible

### Webs

- Spiders are immune to webs and shoot web bolts to entangle
- Most monsters can get stuck; all totems are immune

### Flight

Flies: All Bloat types, Wisp, Phantom, Fury, Vampire Bat, Phoenix, Ifrit

Flits (moves randomly 1/3 of turns): Bloat types, Eel, Bog Monster, Kraken,
Wisp, Pixie, Phantom

Flying creatures are immune to ground traps, pits, and certain terrain effects.

---

## Pack Composition (Horde Catalog)

### Early Packs

- **Jackals**: 1 leader + 1-3 jackals (depth 3-7)
- **Monkeys**: 1 leader + 2-4 monkeys (depth 5-13)
- **Vampire Bats**: 1 leader + 1-2 bats (depth 6-13)
- **Goblin war party**: 1 goblin + 2-3 goblins + 1-2 mystics + 1-2 jackals (depth 6-12)

### Mid-Game Packs

- **Goblin camp**: 1 totem + 2-4 goblins (depth 5-13, in camp area)
- **Large goblin camp**: 1-2 totems + 1-2 conjurers + 1-2 mystics + 3-5 goblins (depth 10-17)
- **Ogre camp**: 1 totem + 2-4 ogres (depth 12-19)
- **Ogre shaman** + 1-3 ogres (depth 14-20)
- **Acid mound cluster**: 1 + 2-4 acid mounds (depth 9-13)
- **Eel school**: 1 + 2-4 eels (depth 8-22)
- **Bog monster cluster**: 1 + 2-4 bog monsters (depth 12-26)

### Late-Game Packs

- **Dar war party**: 1-2 blademasters + 0-1 priestess (depth 15-17)
- **Full dar squad**: 1-2 blademasters + 1 priestess + 1 battlemage (depth 18-25)
- **Wraith pack**: 1 + 1-4 wraiths (depth 16-23)
- **Fury swarm**: 1 + 2-4 furies (depth 18-26)
- **Centaur pair**: 1 + 1 centaur (depth 14-21)

### Endgame Packs

- **Dragon pair**: 1 + 1 dragon (depth 27+)
- **Dragon flight**: 1 + 3-5 dragons (depth 34+)
- **Golem squad**: 1-2 golems + 0-1 priestess + 0-1 battlemage (depth 27+)
- **Golem army**: 1 + 5-10 golems (depth 30+)
- **Kraken school**: 1 + 5-10 krakens (depth 30+)
- **Horror + revenants**: 1-3 horrors + 2-4 revenants (depth 32+)

---

## Status Effects Applied by Monsters

| Status Effect | Applied By | Mechanism |
|---|---|---|
| Poison | Spider (melee), Bloat (gas on death), Dart Turret | Delayed damage over time |
| Weakness | Centipede (melee), Dart Turret (bolt) | Reduces strength, may force unequipping gear |
| Hallucination | Toad (melee) | All monsters appear as random other monsters |
| Burning | Wisp (melee), Flamedancer (melee), Salamander (trail), Explosive Bloat | Ongoing fire damage, spreads to flammable terrain |
| Nausea | Zombie (rot gas aura) | Causes vomiting, prevents actions |
| Slow | Ogre Totem (bolt), Dar Battlemage (bolt), Pixie (bolt) | Doubles movement/attack cost |
| Discord | Dar Battlemage (bolt), Pixie (bolt), Vampire (bolt), Ifrit (bolt) | Monster attacks everyone, including allies |
| Negation | Dar Priestess (bolt), Pixie (bolt) | Strips enchantments, kills summons |
| Seizure/Grapple | Bog Monster (melee), Kraken (melee) | Target immobilized |
| Entanglement | Spider (web bolt), Mangrove Dryad (vines bolt) | Target stuck |
| Armor Degradation | Acid Mound (melee), Acidic Jelly (melee) | Permanently reduces armor enchantment |
| Weapon Degradation | Acid Mound (defend), Acidic Jelly (defend) | Hitting them damages your weapon |
| Stagger/Knockback | Ogre (melee) | Pushed back one tile |

---

## The Mutation System

Any monster can spawn with a rare mutation (probability scales with depth):

| Mutation | Effect | Design Purpose |
|---|---|---|
| Explosive | Explodes on death | Familiar enemy, new positioning concern |
| Infested | Corpse spreads deadly lichen | Can't safely kill in tight spaces |
| Agile | 50% faster, +50% defense, flees when hurt | Trivial enemy becomes uncatchable pest |
| Juggernaut | 3x HP, 2x damage, 2x slower, staggers | Glass cannon becomes immovable wall |
| Grappling | 1.5x HP, seizes targets | Ranged enemy suddenly pins you in melee |
| Vampiric | Life steal on every hit | Attrition fight becomes unwinnable |
| Toxic | Poisons and weakens on hit | Safe enemy becomes resource drain |
| Reflective | Reflects 50% of bolts | Your wands become dangerous to use |

---

## Design Principles

### 1. Every Monster Is a Puzzle, Not a Stat Check

Brogue never asks "is your DPS high enough?" It asks "do you understand this
problem?"

- **Pink Jelly**: Do you fight it (risking exponential splits) or use
  fire/gas/environment?
- **Bloat**: 4 HP, 0 damage — but killing it near you = poison cloud. Kill at
  range or weaponize it against other enemies.
- **Acid Mound**: Hitting it damages your weapon. Its hits damage your armor.
  Do you engage at all, or find another path?
- **Revenant**: Immune to weapons. You *must* have a non-weapon answer or you
  die.
- **Golem**: 400 HP, reflects bolts... but one charge of negation kills it
  instantly. The right tool beats brute force.

### 2. Terrain Is the Second Monster

Rather than combat in a vacuum, Brogue monsters are deeply tied to terrain:

- **Eels/Krakens**: Safe on land, lethal in water. Crossing a flooded room
  becomes a route-planning problem.
- **Bog Monsters**: Invisible in mud until they grab you.
- **Salamander**: Lives in lava, leaves fire trails. The room gets more
  dangerous every turn.
- **Naga**: Emerges from deep water with a cleave attack. Positioning around
  water's edge matters.
- **Zombie**: Emits rot gas constantly. Doors become ventilation tools.

### 3. The Corridor Fighter Problem — Solved

Brogue prevents the classic exploit of backing into a hallway:

1. **`MA_AVOID_CORRIDORS`**: Goblins, ogres, and dar will NOT follow you into
   corridors when in groups. They wait in open rooms.
2. **Ranged support**: Totems and casters work regardless of chokepoints.
3. **Summoners**: Conjurers fill corridors with spectral blades *behind* you.
4. **AOE attacks**: Naga and dragon cleave all adjacent — corridors give them
   *more* attack surface.
5. **Terrain monsters**: Water/lava/mud monsters fight in open terrain by
   definition.

### 4. Composition > Individual Complexity

Individual monsters are simple. Packs create emergent complexity:

**Goblin Camp** (the template for all faction design):
- Totem: Hastes allies, sparks enemies (priority target — but immobile)
- Conjurer: Summons spectral blades (killing conjurer kills all blades)
- Mystic: Shields allies (makes other goblins harder to kill)
- Goblins: Penetrating melee, avoid corridors

**Dar War Party** (late-game masterclass):
- Blademaster: Blinks past your front line into melee
- Priestess: Negation strips your enchantments; heals/hastes allies
- Battlemage: Fire/slow/discord from range

### 5. Escalation Through Mechanics, Not Just Numbers

Each depth tier introduces qualitatively new problems:

| Tier | New Problem |
|---|---|
| 1-5 | Basic combat, item theft (monkey), environmental hazards (bloat gas) |
| 6-10 | Organized factions (goblin camps), support casters (totem, mystic) |
| 7-13 | Terrain control (bog monster, spider webs), knockback (ogre) |
| 10-14 | Debuffs (weakness, poison), undead (wraith invisibility) |
| 13-17 | Combined arms (dar war parties), aquatic/lava threats |
| 16-20 | Full debuff suite (pixie), permanent invisibility (phantom), mass swarms (fury) |
| 21+ | Resurrection (lich), weapon immunity (revenant), raw lethality (dragon, horror) |

### 6. Information as Currency

Several designs attack the player's ability to make informed decisions:

- **Toad hallucination**: Every monster looks like a random other monster.
- **Phantom invisibility**: You don't know it's there until it hits you.
- **Submerging monsters**: Eels/bog monsters/krakens disappear between attacks.
- **Pixie negation**: Your accumulated enchantments can be stripped in one bolt.
- **Discord**: Your ally becomes your enemy.

### 7. The Ally System

- Captive monsters can be freed (from cages, shackles) and become allies
- Allies follow the player, fight enemies, and use stairs
- The **Wand of Empowerment** upgrades ally stats and teaches new abilities:
  - Learnable: Invisibility, Flight, Fire Immunity, 50% Reflection
  - Learnable: Transference (life steal), Causes Weakness
- Legendary allies (Unicorn, Ifrit, Phoenix, Dryad) are found in deep machine
  rooms guarded by challenges

---

## Key Takeaways for The Veiled Tyrant

1. **Every monster should present a decision, not a DPS check** — aligns with
   "Risk vs. reward" pillar
2. **Terrain interaction multiplies monster variety for free** — aligns with
   "Exploration first" pillar
3. **Pack composition creates emergent difficulty** — the GOAP squad system is
   the right framework
4. **MA_AVOID_CORRIDORS is essential** — without it, every group encounter
   becomes trivial
5. **Status effects that change the rules** (hallucination, negation, discord)
   are more interesting than effects that change numbers (slow, weakness)
6. **Late-game monsters need qualitative immunity/weakness pairs** (revenant =
   immune to weapons; golem = dies to negation) to reward diverse builds
7. **The mutation system adds replayability** — a familiar monster with a
   random modifier becomes a new puzzle
