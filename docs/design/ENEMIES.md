# Enemies

## Design Philosophy

Monsters are not stat blocks to bump and kill. Every monster has an **identity**
— it introduces a mechanic, combines mechanics in interesting ways, or forces
the player to change tactics. A fight against 4 goblins should feel different
from a fight against 1 troll, which should feel different from a dragon whelp
in a narrow corridor.

**Core rules:**
- Every floor has **at least 2 factions** present
- **Out-of-depth encounters** can spawn rarely — monsters above the floor's
  normal difficulty range. The player must recognize when to avoid a fight.
- Goblins are the **star faction** — they evolve from disorganized rabble
  into organized camps, forts, and machine-driven encounters as the player
  descends

## Monster Abilities

All monster special attacks are **cooldown-based abilities**, not spells. There
is no mana system for monsters. Each ability has a cooldown in turns. This
mirrors the player's staff system — both sides operate on cooldowns/charges.

## Species

Every monster declares a **species** — its biological category. Species is
orthogonal to faction: faction is political alignment (who fights whom),
species is biology (what kind of creature it is). A Goblin Conjurer and a
Goblin (both faction `Goblin`) share species `Humanoid`; a Spectral Blade
(also faction `Goblin`) is species `Construct` because it's animated steel,
not a living goblin.

Species is used by effects that care about biology — bane weapons
("Dagger of Insect Slaying"), ecology ("Poison does not tick on Undead"),
charm/polymorph restrictions, and UI labels. Faction-based effects
(rally auras, pack tactics, faction hostility) still key off faction.

| Species | Meaning | Examples |
|---------|---------|----------|
| `Beast` | Natural animals; mortal, breathes, bleeds | Sewer Rat, Plague Rat, Rat Broodmother, Eel, Wolf, Cave Bear, Giant Bat |
| `Humanoid` | Intelligent bipeds | Goblin, Goblin Conjurer, Goblin Warchief, Kobold Hoarder, Kobold Marauder, Kobold Chieftain, Orc Warrior |
| `Undead` | Formerly living; immune to mind / poison / bleed | Skeleton, Zombie, Bone Archer, Wraith, Necromancer, Bone Colossus, Lich Acolyte |
| `Insect` | Arthropods / hive creatures | Giant Spider |
| `Fungal` | Spore-based; can be burned, spread by moisture | Pit Bloat, Bloat, Fungal Spore, Spore Crawler, Fungal Shambler, Shrieking Mushroom, Mycoid Sovereign, Black Mold |
| `Ooze` | Amorphous; no armor slots, immune to crits | Jelly |
| `Dragon` | Reptilian apex; fire-affine by default | Dragon Whelp, Young Dragon, Drake, Wyrm, Dragon Priest, Elder Drake |
| `Construct` | Animated / artificial; immune to bleed / poison | Spectral Blade, Arrow Turret, Goblin Totem, Mimic, Stone Sentinel |
| `Aberration` | Eldritch or uncategorized | Shade, Imp |

**Every new monster must declare a species** in `assets/monsters.ron`:

```ron
"Thing": (
    name: "Thing",
    faction: "Goblin",   // politics
    species: Humanoid,   // biology — required
    ...
),
```

Missing the field defaults to `Unknown` and logs a warning on game load.

---

## Tier Structure (26 floors)

The dungeon is divided into six tiers. Each tier escalates difficulty and
teaches a distinct skill, mirroring Brogue's descent pacing.

| Tier | Floors | Name | Player skill tested | Primary factions |
|------|--------|------|---------------------|------------------|
| T1 | 1-4 | **Shallows** | Basic melee, FOV, terrain | Animals, Rats, Kobolds, disorganized Goblins |
| T2 | 5-9 | **Caves** | Status effects, packs, water/fire terrain | Rats (Broodmother peak), Animals, organizing Goblins, Fungal enters |
| T3 | 10-14 | **Depths** | Ranged threats, armor, group tactics | Goblins (fortified), Orcs, Fungal, first Undead, Cave Trolls |
| T4 | 15-19 | **Tombs** | Elemental resist, status cleansing, elite 1v1 | Undead (peak), Giants (first), Fungal elites, first Dragons |
| T5 | 20-24 | **Deep Keep** | Apex combat, resource planning | Dragons, Giants (peak), Undead elites, Constructs |
| T6 | 25-26 | **The Amulet** | Synthesis of everything | Mixed apex + Amulet Guardian on f26 |

**Phasing principle:** every faction has a *rise → peak → fade* window.
Factions do not stay present across all 26 floors — they introduce, dominate
their tier, and retreat. Overlaps during transitions create faction-vs-faction
moments. The `no_faction_is_present_on_every_floor` test enforces this.

## Factions

| Faction | Floors | Role |
|---------|--------|------|
| Animals | 1-10 | Nature's hazards. Teach basic combat, speed, swarms, DoT. |
| Rats | 1-10 | Background fauna. Danger from volume, not individual power. |
| Goblins | 1-14 | The star faction. Evolve from disorganized rabble → organized patrols → fortified warbands. |
| Kobolds | 1-22 | Hoarders (T1) → Alchemists (T2, kiting) → Marauders (T3) → Chieftain warbands (T4–T5). |
| Orcs | 10-18 | T3-T4 organized aggressors. Warbands with leaders. |
| Fungal | 2-22 | Poison-themed. Explosive on-death effects; Mycoid Sovereign at apex. |
| Undead | 12-22 | Resistant physical threats. Ranged + melee combos; Necromancer, Bone Colossus. |
| Giants | 15-25 | Brutes and casters. Regen + knockback; apex is Hill Giant. |
| Dragons | 16-26 | Apex predators. Fire-themed; culminate in Elder Drake. |
| *(none)* | 3-26 | Factionless solo threats (Jellies, Mimics, Wolves, Shades, Imps, Amulet Guardian). |

### Faction Presence by Floor (26-floor distribution)

| Floors | Primary | Secondary | Tertiary | Rare / Out-of-Depth |
|--------|---------|-----------|----------|---------------------|
| 1-2 | Animals | Goblins (rabble) | Rats, Kobolds | — |
| 3-4 | Rats | Goblins, Animals | Kobolds, Fungal (bloats) | Giant Spider |
| 5-6 | Goblins (organizing) | Rats (Broodmother) | Fungal, Eels, Kobold Alchemists | Wolf, Salamander |
| 7-9 | Goblins (organized) | Fungal | Rats, Eels, Kobold Alchemists | Cave Bear, Cave Troll |
| 10-11 | Goblins (fortified) | Orcs, Fungal (spores), Kobold Marauders | Skeletons, Mimics | Dragon Whelp |
| 12-14 | Undead (rising) | Goblin elites, Orcs | Fungal peak, Kobold Marauders | Shade, Imp |
| 15-17 | Undead (peak) | Giants (trolls), Kobold Chieftain warbands | Fungal late | Dragon Whelp, Ogre |
| 18-19 | Giants | Undead (wraiths, necromancers) | Fungal (Sovereign) | Drake |
| 20-21 | Giants (Ogre Mage, Hill) | Dragons | Constructs (Sentinels) | Elder Undead |
| 22-24 | Dragons (Drake, Wyrm, Priest) | Giants peak | Constructs | — |
| 25 | All apex mixed | — | — | — |
| 26 | Mixed gauntlet + **Amulet Guardian** | — | — | — |

## Stat System

Monsters use the same combat system as the player (symmetric combat). Each
monster has direct stats — no attribute derivation.

| Stat | Description |
|------|-------------|
| HP | Hit points |
| Damage | Damage dice |
| Hit Bonus | Added to d20 attack roll |
| Dodge Bonus | Added to base dodge target (4) |
| Armor | Flat damage reduction (physical only) |
| Move Delay | Movement speed multiplier (lower = faster). Default 1.0. |
| Atk Delay | Attack speed multiplier (lower = faster). Default 1.0. |
| Vision | Sight range in tiles |

> **Split delays:** Monsters have separate `movement_delay` and `attack_delay`
> fields in `assets/monsters.ron`. This enables fast chasers with slow attacks
> (e.g., Wolf: 0.8 move / 1.0 attack) and slow movers with fast strikes
> (e.g., Spider: 1.0 move / 0.7 attack). Status effects (Hasted/Slowed)
> multiply both delays equally. Stat tables below show a single "Delay" column
> for brevity when both values are the same.

---

## Faction: Animals

*Instinct-driven creatures. Each one teaches a specific combat lesson.*

### Giant Rat
**Floors 1-4 | Swarm creature**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d3 | 0 | 0 | 0 | 0.9x | 6 |

- **Identity:** Trivial alone, dangerous in packs
- **Mechanic:** Introduces **group encounters**. When surrounded, even weak
  enemies threaten the player.
- **Group size by floor:** 1-2 (floors 1-2), 2-3 (floor 3), 3-4 (floor 4)
- **Behavior:** Aggressive when in a group, flees when alone and wounded

### Rat Queen
**Floors 3-6 | Swarm matriarch**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 20 | 1d4 | 1 | 0 | 1 | 1.1x | 8 |

- **Identity:** Larger, tougher rat that spawns more rats mid-fight
- **Mechanic:** Introduces **summoning enemies**. Priority target.
- **Ability — Spawn Rat:** Spawns 1 Giant Rat on adjacent tile (max 4 active
  spawned rats). Cooldown: 5 turns.
- **Group size:** 1 (solo, but always accompanied by 2-3 Giant Rats at spawn)
- **Behavior:** Stays behind its rats; doesn't chase aggressively

### Giant Bat
**Floors 1-3 | Erratic flyer**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 4 | 1d3 | 0 | 2 | 0 | 0.8x | 8 |

- **Identity:** Unpredictable and fast — hard to avoid, easy to kill
- **Mechanic:** Introduces **erratic movement**. Moves in a random adjacent
  direction 30% of the time instead of toward the player. High dodge makes
  them annoying to hit.
- **Group size by floor:** 1 (floor 1), 1-2 (floor 2), 1-3 (floor 3)

### Pit Bloat
**Floors 2-6 | Environmental hazard**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 4 | 0 | 0 | 0 | 0 | 1.2x | 8 |

- **Identity:** Erratic drifting bomb that creates permanent terrain hazards
- **Mechanic:** Introduces **delayed environmental danger**. On melee hit, the
  bloat explodes (killing itself), cracking floor tiles in a Manhattan radius
  of 2. Cracked tiles collapse into chasms after ~3 turns (~33% chance per
  turn). Chasms are permanent and impassable — entities on collapsing tiles
  fall to the floor below (maintaining position, adjusted for walkability).
  Does 0 damage directly; the threat is terrain destruction and forced floor
  transitions.
- **Behavior:** Erratic movement (60% random direction) — flits unpredictably
  toward the player. Slow (1.2x delay). Never flees.
- **Group size:** Always solo
- **Counterplay:** Kill before it reaches you (only 4 HP). If it explodes,
  move off cracked tiles within 3 turns. Use ranged attacks or staves to
  avoid melee contact entirely.

### Bloat
**Floors 2-6 | Poison gas bomb**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 0 | 0 | 0 | 0 | 1.2x | 8 |

- **Identity:** Swollen green pest that detonates into a poison gas cloud — on
  contact or on death.
- **Mechanic:** Introduces **area denial via poison gas**. Two triggers:
  - **On melee hit (`ExplodeOnHit` with `GasCloud` effect):** when the Bloat
    connects its zero-damage bump, it bursts immediately. Releases poison gas
    in a Manhattan radius of 2 and self-destructs. The `GasOnDeath` ability
    is stripped before the self-damage resolves, so the cloud spawns exactly
    once.
  - **On death by other means (`GasOnDeath`):** killed at range or by melee
    counter-attack — gas releases as the Bloat dies.
- The gas cloud persists and spreads through corridors, poisoning creatures
  that stand in it (1 DMG/turn for 3 turns when concentration ≥ 100). Gas
  decays over time.
- **Behavior:** Same erratic movement as Pit Bloat (40% random). Slow (1.2x
  delay). Deals no direct damage — the gas IS the threat, whether it reaches
  you or you reach it.
- **Group size:** Always solo
- **Counterplay:** Kill at range to avoid standing in the gas. Letting it
  touch you guarantees you're in the resulting cloud. Poison gas is
  flammable — fire sources will ignite the cloud for AoE fire damage.
- **Emergent interactions:** Multiple Bloats dying nearby stack gas
  additively. Gas flows through corridors and doorways. Fire ignites the cloud.

### Wolf
**Floors 2-5 | Pack hunter**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 10 | 1d6 | 1 | 1 | 0 | 0.9x | 12 |

- **Identity:** Wide vision, pack coordination
- **Mechanic:** Introduces **shared alerting**. When one wolf spots the player,
  the entire pack converges from different directions. Wide vision (12 tiles)
  means they spot you first.
- **Group size by floor:** 1-2 (floors 2-3), 2-3 (floor 4), 3-4 (floor 5)
- **Squad behavior:** Shared alerting

### Fire Salamander
**Floors 2-5 | Burning striker**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 8 | 1d4 | 1 | 1 | 0 | 0.95x | 8 |

- **Identity:** Ember-skinned lizard that sets you on fire
- **Mechanic:** Introduces **damage over time** (fire-based). Melee attacks
  apply burning. Player learns to disengage and recover.
- **On-hit:** Burning (2 fire damage/turn for 3 turns)
- **Fire Resistant:** 50% fire resistance
- **Group size by floor:** 1 (floors 2-3), 1-2 (floors 4-5)

### Giant Spider
**Floors 3-6 | Ambush predator**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 10 | 1d4+1 | 1 | 1 | 0 | 1.0x | 10 |

- **Identity:** Web ability, ambush from dark corners
- **Mechanic:** Introduces **movement debuffs** and **poison**.
- **Ability — Web:** Range 4, applies Slow for 3 turns (delay x1.5).
  Cooldown: 8 turns.
- **On-hit:** Poison (1 poison damage/turn for 4 turns, stacks)
- **Group size by floor:** 1 (floors 3-4), 1-2 (floors 5-6)

### Cave Bear
**Floors 3-6 | Slow devastator**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 25 | 2d6 | 2 | 0 | 2 | 1.15x | 6 |

- **Identity:** Slow but devastating — you can't trade hits
- **Mechanic:** Introduces **kiting**. High damage and HP but slow speed
  teaches the player to use corridors and speed advantage.
- **Group size:** 1 (solo, always)
- **Behavior:** Doesn't chase far — gives up after 8 tiles

### Eel
**Floors 2-6 | Aquatic ambusher**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 15 | 2d4 | 1 | 1 | 0 | 0.8x | 8 |

- **Identity:** Fast water predator that ambushes from deep water
- **Mechanic:** Introduces **water-restricted enemies**. Can only move through
  water tiles (deep or shallow). Attacks players who wade into or near water.
- **Movement:** Aquatic — restricted to water tiles only
- **Behavior:** Erratic movement (30% random direction like Giant Bat). Fast
  (0.8x delay) and aggressive when player enters water.
- **Group size:** 1 (solo)

---

## Faction: Rats

*Background dungeon fauna. Present on all floors. Danger comes from volume and
persistence, not individual power. Rats are hostile only to the player; they are
neutral to all monster factions.*

### Sewer Rat
**Floors 1-26 | Swarm vermin**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d3 | 0 | 0 | 0 | 0.9x | 12 |

- **Identity:** Trivial alone, dangerous in numbers. Enduring swarm threat.
- **Mechanic:** Introduces **persistent group encounters**. Unlike other swarms
  that thin naturally, rat packs keep respawning (via Broodmother) or endlessly
  reforming from new spawns. The player must avoid sustained engagement.
- **Group size by floor:** 1-3 (floor 1), 2-4 (floor 2), 4-6 (floors 3-5),
  5-8 (floors 6-8), 6-9 (floors 9-26)
- **Behavior:** FSM AI — flees when below 25% HP, otherwise aggressive. Chase
  leash of 8 tiles. When packed together, they stand and fight; when thinned
  below 50% squad morale, they scatter and flee.
- **Squad behavior:** Leaderless ambient packs. Shared alerting at 12 tiles.
  Scatter on leader death (natural for leaderless packs) or when morale drops
  below 50%.

### Plague Rat
**Floors 3-26 | Poisonous swarm variant**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d2 | 0 | 0 | 0 | 0.9x | 12 |

- **Identity:** Poison-dealing variant. Teaches: "don't let rats sustain contact."
- **Mechanic:** Introduces **poison DoT in swarms**. Plague rats trade raw damage
  for guaranteed poison application. Hit multiple times and the poison stacks.
- **On-hit:** PoisonStrike — applies 1 poison damage/turn for 3 turns (100% chance)
- **Behavior:** Same FSM AI as Sewer Rat (flee at 25%, chase leash 8)
- **Spawn patterns:**
  - Mixed into Sewer Rat packs: floors 3-5 include 1 plague rat per group;
    floors 6-26 include 1-2 plague rats per group
  - Pure groups: 2-4 Plague Rats (independent spawn entry, floors 3-26)
- **Squad behavior:** Same leaderless pack rules as Sewer Rat

### Rat Broodmother
**Floors 5-26 | Summoning matriarch**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 20 | 1d4 | 1 | 0 | 1 | 1.2x | 14 |

- **Identity:** Mobile summoner that maintains a living swarm. The tactical puzzle.
- **Mechanic:** Introduces **summoning swarms** and **action economy pressure**.
  Killing her swarm is futile while she lives — she'll replace them on a 2-turn
  cooldown. The player must reach and kill the Broodmother herself.
- **Ability — Summon Swarm:** Summons 1 rat (70% Sewer, 30% Plague) at a random
  adjacent walkable tile. Cooldown: 2 turns. Max active summons: 6. Summons
  join her squad.
- **AI:** GOAP (Cowardly + Support traits). Priority: flee when player adjacent →
  summon if count < 6 → retreat and maintain distance → wander
- **Group size:** 1 (always spawns with escort)
- **Starting Escort:** 3-4 Sewer Rats + 1 Plague Rat (counts toward 6-summon cap)
  Start with ~4-5 summons filled, able to summon 1-2 more before hitting cap
- **Squad behavior:** Broodmother is SquadLeader, granting +0.2 morale to escorts
  and summoned rats. Makes the pack harder to scatter. On death: -0.3 morale hit
  + scatter. Remaining rats flee and do not re-form.
- **Morale recovery:** Out of combat, slow natural recovery. With Broodmother alive,
  squad recovers faster. Leaderless survivors (post-death) recover very slowly
  and typically remain scattered.

### Rat Faction Behaviors

**Faction Hostility:**
- Hostile to: Player only
- Neutral to: All other monster factions (Goblins, Kobolds, Undead, Fungal, Dragons,
  Animals). Rats coexist as dungeon scavengers and do not engage other monsters.

**Leaderless Pack Logic:**
- Ambient Sewer/Plague Rat packs have no designated leader
- Shared alerting: when any rat spots the player or takes damage, all rats within
  12 tiles converge toward the threat
- Scatter threshold: when squad morale drops below 50%, surviving rats flee
  individually and do not re-form

**Broodmother Pack Logic:**
- Broodmother acts as SquadLeader (+0.2 morale bonus to all squad members)
- Summoned rats inherit her squad and are protected by her leadership morale
- On her death, -0.3 morale penalty + immediate scatter order
- Remaining summoned rats become leaderless and typically rout

**Summon Cap System:**
- Each summoned rat gets a `SummonedBy` component pointing back to the Broodmother
- Broodmother has a `SummonCap { max: 6 }` component
- Before summoning, the ability queries all living entities with her `SummonedBy`
- If count < 6 and ability is off cooldown, summon action is available
- When a summoned rat dies, it despawns naturally. Next turn, query count is lower
  and summoning becomes available again

---

## Faction: Goblins

*The star faction. Goblins evolve from disorganized rabble on floor 1 into
fortified, machine-driven encounters on floor 9. Their progression mirrors
the dungeon's escalating complexity.*

### Goblin Evolution by Depth

| Depth | Goblin State | What Changes |
|-------|-------------|--------------|
| 1-3 | **Disorganized** | Small groups, no leaders, flee easily |
| 4-5 | **Organizing** | Shamans appear. Conjurers summon spectral blades. Brutes tank. |
| 6-7 | **Organized** | Warchief-led squads with aura buffs. Firebombers add AoE. |
| 8-9 | **Fortified** | Goblin camps and forts via machine system. Structured encounters. |

### Goblin
**Floors 1-5 | Cowardly footsoldier**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d4 | 0 | 1 | 0 | 1.0x | 8 |

- **Identity:** Cowardly and weak — but never alone
- **Mechanic:** Introduces **fleeing enemies**. Flees when below 30% HP.
- **Group size:** 1-3
- **Behavior:** Cowardly; flees at 30% HP

### Goblin Conjurer
**Floors 2-6 | Summoner support**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 8 | 1d4 | 0 | 1 | 0 | 1.0x | 10 |

- **Identity:** Fragile caster that floods the field with spectral blades
- **Mechanic:** Introduces **summoner enemies**. Summons up to 4 Spectral Blades.
  Killing the conjurer kills all active blades. Teaches "kill the summoner" priority.
- **Ability — Conjure Blades:** Summons 1 Spectral Blade adjacent. Max 4 active.
  Cooldown: 3 turns.
- **Behavior:** Cowardly + Support; stays in the back, summons blades
- **Group size:** 1 (solo, but creates its own army)

#### Spectral Blade (summoned)

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 1 | 1d6+1 | 0 | 0 | 0 | 0.9x | 12 |

- **Identity:** Glass-cannon summon. Fast, deadly, but dies to any hit.
- **Behavior:** Aggressive FSM. Never flees. Pathfinds to nearest enemy.
- **Dies when:** Hit by anything (1 HP) OR conjurer is killed.
- **No loot.** Not spawned directly — only created by Goblin Conjurer.

### Goblin Brute
**Floors 3-7 | Armored tank**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 14 | 1d8 | 1 | 0 | 2 | 1.1x | 7 |

- **Identity:** The first armored enemy — physical attacks bounce off
- **Mechanic:** Introduces **armor as a problem**. With 2 armor, weak weapons
  deal almost no damage. Teaches the player to use fire/lightning staves or
  find better weapons.
- **Group size:** 1 (solo, but often near goblin groups)

### Goblin Shaman
**Floors 3-7 | Healer support**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 6 | 1d4 | 0 | 1 | 0 | 1.0x | 10 |

- **Identity:** Priority target — heals other goblins from range
- **Mechanic:** Introduces **enemy healers** and **priority targeting**. If you
  ignore the shaman, it heals the brute you're fighting. Kill it first.
- **Ability — Heal Ally:** Heals a visible goblin ally for 8 HP. Range 8.
  Cooldown: 4 turns.
- **Ability — Poison Bolt:** 1d4 poison damage + poison DoT (2/turn, 3 turns).
  Range 6. Cooldown: 3 turns.
- **Group size:** 1 (always accompanies other goblins)

### Goblin Warchief
**Floors 5-9 | Squad leader**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 18 | 1d8 | 2 | 2 | 2 | 0.95x | 10 |

- **Identity:** The brains of the operation — killing it breaks the squad
- **Mechanic:** Introduces **leadership auras** and **decapitation tactics**.
- **Passive — Leadership Aura:** +2 damage and +2 dodge to all goblins within
  5 tiles.
- **On death:** Squad scatters (remaining goblins lose target, wander for 5 turns)
- **Ability — War Cry:** All goblins within 8 tiles gain +1 damage for 5 turns
  and recover morale. Cooldown: 10 turns.
- **Group size:** 1 (always accompanied by a squad of 3-5 goblins)

### Goblin Totem
**Floors 4-9 | Stationary force multiplier**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 15 | — | — | 0 | 0 | — | — |

- **Identity:** A carved totem that empowers nearby goblins — destroy it or suffer
- **Mechanic:** Introduces **stationary priority targets**. Does not move or attack.
- **Ability — Haste:** Targets a random goblin within 5 tiles, +50% speed for
  8 turns. Cooldown: 6 turns.
- **Ability — Chain Lightning:** 2d6 lightning damage + 2 jumps (1d6 each,
  3 tiles). Cooldown: 8 turns.
- **Cannot be healed** by shamans
- **Group size:** 1 (placed in goblin camps/forts)
- **Behavior:** Stationary. Cannot move. No melee.

### Goblin Firebomber
**Floors 6-9 | AoE threat**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 8 | 1d4 (melee) | 0 | 2 | 0 | 1.0x | 10 |

- **Identity:** Throws fire flasks that create burning ground
- **Mechanic:** Introduces **area denial**.
- **Ability — Fire Flask:** Range 6, 1d6 fire AoE (3x3), leaves burning ground
  for 3 turns (2 fire damage/turn). Cooldown: 5 turns.
- **Group size:** 1 (paired with other goblins)
- **Behavior:** Stays at range; retreats from melee

---

## Faction: Dragons

*Apex predators. Rare encounters that demand respect. Fire-themed, armored,
and fast.*

### Dragon Whelp
**Floors 5-9 | Mid-game terror**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 24 | 2d6 | 3 | 2 | 3 | 0.9x | 12 |

- **Identity:** Fast, fire-breathing, fire immune — a serious threat
- **Mechanic:** Introduces **elemental immunity**. Fire staves are useless.
  The player must use physical or lightning damage.
- **Ability — Fire Breath:** Range 5 cone (3 tiles wide at max range), 2d6 fire
  damage. Cooldown: 4 turns.
- **Fire Immune:** Takes 0 fire damage
- **Group size:** 1 (solo)
- **Out-of-depth:** Can appear as early as floor 4 (rare)

### Young Dragon
**Floors 8-9 | Apex predator**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 40 | 2d10 | 4 | 2 | 5 | 0.95x | 14 |

- **Identity:** The most dangerous enemy in the game
- **Mechanic:** Combines fire breath + fire immunity + high armor + high damage.
  Forces the player to use everything they've learned.
- **Ability — Fire Breath:** Range 6 cone, 3d6 fire damage. Cooldown: 3 turns.
- **Fire Immune:** Takes 0 fire damage
- **Group size:** 1 (solo)

---

## Faction: Kobolds

*Cowardly, cunning scavengers. They don't fight fair — they steal your loot
and run.*

### Kobold Hoarder
**Floors 1-5 | Cowardly thief**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 7 | 1d4 | 0 | 1 | 0 | 0.8x | 8 |

- **Identity:** Steals items from chests and hoards them — killing drops stolen loot
- **Mechanic:** Introduces **item-stealing enemies** and **intelligent cowardice**.
  Uses GOAP AI to prioritize stealing over fighting.
- **Behavior:** Cowardly + Intelligent. Flees from the player. Seeks out chests
  and steals items from them. Killing a Kobold Hoarder drops all stolen loot.
- **AI:** GOAP-driven. Goals: acquire items > flee from danger > fight (last resort)
- **Group size:** 1-2

### Kobold Alchemist
**Floors 6-11 | Kiting poisoner**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 12 | 1d4 poison | 0 | 1 | 0 | 0.9x | 10 |

- **Identity:** Fills the faction's mid-floor gap. Keeps distance and
  throws poison flasks, teaching "not all kobolds flee — some harass."
- **Mechanic:** Ranged poison (range 5), kites at distance 3, flees at 50% HP.
  `PoisonStrike` on successful hit applies 2/turn for 3 turns.
- **Behavior:** FSM kiter. Flees → kites → bolts → fights only cornered.
- **Group size:** 1-2
- **Loot:** 25% Antidote (thematic flavor: alchemist)

### Kobold Marauder
**Floors 10-14 | Pack aggressor**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 18 | 1d6 | 0 | 0 | 2 | 0.9x | 10 |

- **Identity:** The faction's shift from cowards to fighters. Emboldened
  by numbers, armored, willing to brawl.
- **Mechanic:** `PackTactics` — +50% damage when a faction ally is adjacent
  to the target. Teaches "flank the flanker."
- **Behavior:** Aggressive + Intelligent GOAP, morale 0.65.
- **Group size:** 1-2

### Kobold Chieftain
**Floors 15-22 | Warband commander**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 35 | 1d6+1 | 0 | 0 | 2 | 0.9x | 10 |

- **Identity:** Late-game faction apex. Gives the Goblin↔Kobold hostility
  climactic stakes when their warband meets a Goblin warchief group.
- **Mechanic:** `PackTactics` + `WarCry(radius: 3, duration: 5)` — a 3-tile
  aura at first sighting that damage-boosts every Kobold ally for 5 turns.
  Priority target.
- **Behavior:** Aggressive + Intelligent + Commander, morale 0.8.
- **Loot:** 30% Sword, 30% Healing Potion, 10% Scroll of Enchanting.
- **Group size:** 1 (solo), or as leader of a warband (Chieftain + 2-3
  Marauders + optional Alchemist).

---

## Faction: Undead

*Physically resistant skeletal threats. Dangerous in combination — archers
support melee skeletons from range.*

### Skeleton
**Floors 4-8 | Basic undead fodder**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 18 | 1d6 | 1 | 0 | 2 | 1.0x | 8 |

- **Identity:** Durable undead that shrugs off physical damage
- **Mechanic:** Introduces **physical resistance**. With 50% physical resistance
  and 2 armor, physical weapons are highly ineffective. Teaches the player to
  use fire, lightning, or poison damage.
- **Resistance:** 50% physical
- **Group size:** 1-3
- **Behavior:** Standard melee approach

### Bone Archer
**Floors 5-9 | Ranged undead support**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 15 | 1d6 | 1 | 0 | 1 | 1.0x | 10 |

- **Identity:** Ranged undead that pairs with Skeletons in Bone Crypt encounters
- **Mechanic:** Introduces **ranged + physical resistant combo**. While Skeletons
  hold the front line, Bone Archers fire from behind.
- **Ranged attack:** Range 8, 1d6 physical damage
- **Resistance:** 50% physical
- **Group size:** 1-2
- **Behavior:** Kites; keeps distance from melee range

---

## Faction: Fungal

*Poison-themed creatures that punish melee aggression. Their death is often
more dangerous than their life.*

### Fungal Spore
**Floors 3-8 | Walking poison bomb**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 8 | 1d4 | 0 | 0 | 0 | 1.0x | 6 |

- **Identity:** Walking poison bomb — like Bloat but poison instead of fire
- **Mechanic:** Introduces **poison AoE on death**. Forces the player to consider
  positioning before killing it, or to use ranged attacks.
- **On death (any cause):** Explodes — 2d4 poison damage, 3x3 AoE centered on
  the spore. Creates a poison cloud that lingers for 3 turns (1 poison
  damage/turn to entities standing in it).
- **Group size:** 1-3
- **Behavior:** Mindless approach; no flee, no kiting

---

## Factionless

### Cave Troll
**Floors 4-8 | Regenerating tank**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 28 | 2d6 | 2 | 0 | 2 | 1.1x | 8 |

- **Identity:** Regenerates HP every turn — outlasts you in a war of attrition
- **Mechanic:** Introduces **regeneration as a problem**. Fire damage is the
  counter — trolls take extra fire damage.
- **Passive — Regeneration:** 2 HP/turn
- **Fire Vulnerable:** -50% fire resistance (takes 50% extra fire damage)
- **Group size:** 1 (solo)
- **Behavior:** Slow, doesn't chase far (gives up after 10 tiles)

### Stone Sentinel
**Floors 2-26 | Ancient guardian**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 200 | 10d20 | 6 | 0 | 8 | 8.0x | 12 |

- **Identity:** Nearly indestructible, impossibly slow, instantly lethal
- **Mechanic:** Introduces **avoidance as the correct strategy** and
  **territorial enemies**. Not every enemy is meant to be fought.
- **Guard AI:** Spawns at a fixed post. Chases the player when they enter vision
  range. Returns to its post when the player leaves vision range.
- **Not out-of-depth:** Stone Sentinels are placed by the machine system as
  guardians, not by the random spawner. They appear on their listed floors as
  deliberate encounters.
- **Group size:** 1 (solo, always)

### Jelly
**Floors 2-7 | Splitting horror**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 50 | 1d3 | 0 | 0 | 0 | 1.2x | 6 |

- **Identity:** Harmless-looking blob that multiplies into a nightmare
- **Mechanic:** Introduces **splitting**. When hit, a new jelly spawns adjacent
  with half the original's remaining HP.
- **Split rules:**
  - On any hit, spawn a new jelly with `floor(original.current_hp / 2)` HP
  - The original's HP is NOT reduced by the split
  - New jellies can also split when hit
  - Minimum HP to split: 5
  - **Fire damage prevents splitting** (the jelly dies without reproducing)
- **Group size:** 1 (solo)

### Arrow Turret
**Floors 3-26 | Stationary corridor hazard**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 10 | — | 0 | 0 | 0 | — | 8 |

- **Identity:** Wall-mounted environmental hazard that controls corridors
- **Mechanic:** Introduces **stationary ranged threats**. Cannot move. Must be
  destroyed or avoided. Forces the player to choose alternate routes or close
  distance quickly.
- **Ability — Arrow Shot:** Range 8, 1d6 physical damage. Cooldown: 2 turns.
- **AI:** StationaryAI — cannot move. Fires at any entity in line of sight.
- **Group size:** 1 (solo, always)

### Shade
**Floors 5-9 | Elusive phase-shifter**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 12 | 1d6 (poison) | 1 | 3 | 0 | 0.85x | 8 |

- **Identity:** Elusive, hard-to-pin-down threat that deals poison damage
- **Mechanic:** Introduces **teleporting enemies**. High dodge and phase shift
  make it very difficult to corner or kill with melee alone.
- **Ability — Phase Shift:** Teleport to a random visible tile within 6.
  Cooldown: 4 turns.
- **Resistance:** 50% physical
- **Group size:** 1 (solo, always)

### Imp
**Floors 5-9 | Ranged fire pest**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 10 | 1d4 (melee) | 0 | 1 | 0 | 0.9x | 10 |

- **Identity:** Annoying ranged fire pest that kites relentlessly
- **Mechanic:** Introduces **ranged fire enemies**. Forces the player to use
  lightning resistance or close distance quickly.
- **Ranged attack — Fire Bolt:** Range 6, 1d6 fire damage.
- **Behavior:** Kites aggressively; retreats from melee range
- **Group size:** 1-2

### Ogre
**Floors 6-26 | Devastating brute**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 45 | 2d8 | 2 | 0 | 3 | 1.2x | 6 |

- **Identity:** Massive late-game brute — slow but devastating
- **Mechanic:** Introduces **knockback as a monster ability**. Even with good
  armor, the player gets pushed into dangerous positions.
- **Ability — Knockback:** On hit, target is pushed back 2 tiles.
- **Group size:** 1 (solo, always)
- **Behavior:** Slow, short vision. Doesn't chase far but hits incredibly hard.

### Mimic
**Floors 3-26 | Disguised ambusher**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 20 | 2d6 | 2 | 0 | 1 | 1.0x | 6 |

- **Identity:** Disguised as a chest until the player gets adjacent, then surprise
  attacks
- **Mechanic:** Introduces **disguised enemies** and **ambush awareness**. The
  player learns to be cautious around chests in suspicious locations.
- **On reveal:** When the player moves adjacent, the mimic reveals itself and
  gets a **free attack** (attacks before the player can react). Full stats
  activate on reveal.
- **Disguise:** Appears as a chest sprite. No FOV indicators. Cannot be detected
  until triggered.
- **Group size:** 1 (solo, always)

---

## Group Spawning

Monsters spawn in groups defined by their group size range. A room rolls a
monster type, then rolls a group size within that monster's range.

### Cluster Placement

Groups are placed using BFS outward from a spawn point:
- Cardinal directions only (tight clumps)
- Graceful degradation: if a room only fits 2 tiles, a group of 4 places 2
- An occupied set prevents stacking across rooms

---

## Squad System

Groups of 2+ are linked as a **squad** with shared behaviors. Squads are
assigned at spawn time. Solo monsters have no squad. Squads don't merge or
recruit dynamically.

### Squad Behaviors

| Behavior | Description |
|----------|-------------|
| **Shared alerting** | When any member spots the player or takes damage, the entire squad activates |
| **Leader leashing** | Non-leader members stay within 4 tiles of the leader; if separated, pathfind to leader first |
| **Leader death** | Squad dissolves — all members become independent |
| **Aura visual** | Aura radius shown as a visual indicator around the leader |

### Squad Roles

| Role | Behavior |
|------|----------|
| Scout | Finds and alerts nearby same-faction monsters, then reverts to Guard |
| Guard | Stays between leader and threat, charges recklessly |
| Flanker | Circles around to attack from the side |
| Bodyguard | Stays adjacent to leader |
| Skirmisher | Shoots and repositions behind allies |
| Support | Heals, buffs, stays in back line |
| Commander | The leader — issues orders, stays behind front line |

### Default Role Mapping

| Monster | Default Role | Reassigned When |
|---------|-------------|-----------------|
| Goblin | Guard | Scout (if unalerted goblins nearby), Flanker (if morale > 0.6) |
| Goblin Conjurer | Support | — |
| Goblin Brute | Bodyguard | Guard (if no warchief) |
| Goblin Shaman | Support | — |
| Goblin Warchief | Commander | — |

### Morale

Per-entity `Morale(f32)` component. Morale affects squad-level decisions.

| Event | Modifier |
|-------|----------|
| Leader alive | +0.2 |
| Healer alive | +0.1 |
| Outnumber player 3:1+ | +0.15 |
| Squad member killed | -0.15 (cumulative) |
| Leader killed | -0.3 |
| Own HP < 50% | -0.1 |
| Own HP < 25% | -0.15 |

| Average Squad Morale | Decision |
|---------------------|----------|
| 0.8+ | **Aggressive** — Assign flankers, archers advance |
| 0.5-0.8 | **Normal** — Hold positions |
| 0.3-0.5 | **Cautious** — No flanking, prioritize guard roles |
| 0.15-0.3 | **Retreat** — Leader orders retreat to fallback point |
| < 0.15 | **Rout** — Squad dissolves, everyone flees individually |

### Controlled Retreat

When morale drops to 0.15-0.3, the leader orders retreat to a fallback point
(spawn position, nearest chokepoint, or farthest explored tile from player).

During retreat:
- Archers still shoot while retreating
- Brutes hold chokepoints
- At the fallback point, the squad reforms defensively

**Player counterplay:**
- Chase the retreat (risky — they set up at a chokepoint)
- Let them go (they'll heal and return)
- Cut off the retreat (maneuver ahead and catch them in the open)

---

## Out-of-Depth Encounters

Occasionally, a monster from a deeper floor range spawns on a shallower floor.
These are rare (5-10% chance per floor) and serve as:

- **Warning signs** — a dragon whelp on floor 4 tells you "this is coming"
- **Avoidance puzzles** — the player must recognize they can't win and route around
- **Reward for strong builds** — killing an out-of-depth enemy drops better loot

Out-of-depth monsters are always **solo** and spawn in rooms the player doesn't
have to enter (never blocking the critical path).

---

## Goblin Escalation via Machines

Goblins become more dangerous not just through stronger stat blocks, but through
**structured encounters** built by the machine system (see ENCOUNTERS.md):

| Floor Range | Encounter Type | Description |
|-------------|---------------|-------------|
| 1-3 | Goblin Scuffle | Small room with 2-3 goblins and maybe a conjurer |
| 4-5 | Goblin Camp | Open room with goblins, a shaman, a totem, and a chest |
| 6-7 | Goblin Outpost | Walled room with conjurers, brutes at the entrance, shaman in the back |
| 8-9 | Goblin Fort | Multi-room encounter with firebombers, a warchief, and high-value loot |

## Design Notes

- **Identity over numbers.** Every monster entry should answer "what does the
  player learn from fighting this?"
- **Dragons are aspirational.** Seeing a dragon whelp on floor 4 and running
  away, then fighting one on floor 6 with better gear, is a satisfying power arc.
- **All abilities are cooldown-based.** No mana pools for monsters. This keeps
  monster design simple and predictable for the player to learn.

---

## New Monsters — T3-T6 Additions

Monsters added during the 26-floor distribution pass. All follow the standard
stat-block format; each includes species tag, faction (if any), floor range,
and role.

### Tier 1-2 Animals (design-backfill)

**Giant Rat** — T1, f1-4, Beast — Trivial alone, dangerous in packs. HP 5, 1d3 damage, 0.9× delay. Teaches group encounters.

**Giant Bat** — T1, f1-3, Beast — Erratic flyer (30% random direction), dodge 2. HP 4, 1d3, 0.8× delay. Teaches unpredictable movement.

**Wolf** — T2, f4-7, Beast, PackTactics — Wide vision (12), pack coordination. HP 10, 1d6, 0.9× delay.

**Cave Bear** — T2, f7-10, Beast — Slow but devastating. HP 25, armor 2, 2d6, 1.15× delay. Teaches kiting.

### Tier 3 Gap-Fillers (f10-14)

**Kobold Marauder** — T3, f10-14, Humanoid, Kobold faction — The kobolds who *didn't* flee. Armored + PackTactics. HP 18, armor 2, 1d6. Aggressive GOAP traits. Reintroduces Kobolds at depth as genuine threats, not loot-runners.

**Goblin Sapper** — T3, f11-14, Humanoid, Goblin faction — Kiting ranged trap-layer. HP 12, dodge 1, 1d4, kites at range 3-5. On death: `ExplodeOnDeath(damage: 8, radius: 2)`. Players who melee-chase get caught in the blast radius.

### Tier 4 Gap-Fillers (f15-19)

**Black Mold** — T4, f15-19, Fungal, Fungal faction, **stationary** — Wall growth. HP 8, 1d4 poison, `PoisonStrike(duration: 4)` on adjacent hits, `ExplodeOnHit(GasCloud, volume: 400)`. Environmental threat; standing adjacent is punished, attacking it releases poison gas.

**Lich Acolyte** — T4, f15-18, Undead, Undead faction — Bridges Skeleton/Zombie tier to Necromancer. HP 22, armor 1, 1d6 poison, 50% phys/poison resist, `Bolt(2d6 Poison, range 6, cooldown 3)`. First sustained Undead caster below f15.

### Tier 5 Gap-Filler (f21-25)

**Bone Colossus** — T5, f21-25, Undead, Undead faction — Undead apex (Necromancer alone was undertuned for this tier). HP 55, armor 4, 2d8+2, 60% phys / 100% poison resist, `RoughBody(2)` + `Rally(armor +2)` auras. Giant-sized — marches with lesser Undead escort.

### Tier 6 — The Amulet Guardian (f26 only)

**Amulet Guardian** — T6, f26, Construct, factionless — The last fight before grabbing the Amulet. HP 100, armor 6, 3d6+3, 75% fire / 100% poison / 25% phys resist. Abilities: `RoughBody(3)`, `Terrify(radius: 5)`, `SummonOnDeath(Stone Sentinel)`, self-buff `Strengthened` cooldown 10. Aggressive Commander GOAP. Escorted by 1-2 Stone Sentinels per the amulet_chamber horde.

**Identity:** a towering gold warden with adamant mandate — kill it and it summons one Stone Sentinel from the rubble, forcing the player to commit to a second fight before reclaiming the Amulet. Terrify aura means players without fear resistance will be routed toward the walls.

## Open Design Questions

- **Ascent spawn rules** (GAME.md §Ascending). Floors are snapshotted on descent. Do deeper-tier monsters "climb up" during the ascent, creating a mixed difficulty on the way back? Current implementation: snapshotted-as-left.
- **Stat-rebalance pass.** Several re-enabled monsters were tuned for a 10-floor game (e.g., Necromancer HP 28 originally f8-10, now f17-21). A balance pass may be needed to lift HP/damage for monsters relocated to higher floors.
