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

## Factions

| Faction | Floors | Role |
|---------|--------|------|
| Animals | 1-6 | Nature's hazards. Teach basic combat, speed, swarms, DoT. |
| Goblins | 1-10 | The star faction. Evolve from disorganized to structured. |
| Kobolds | 1-5 | Cowardly hoarders. Steal loot, flee from combat. |
| Undead | 4-9 | Resistant physical threats. Ranged + melee combos. |
| Fungal | 3-8 | Poison-themed. Explosive on-death effects. |
| Dragons | 5-10 | Apex predators. Rare, powerful, fire-themed. |
| *(none)* | 3-10 | Various factionless solo threats. |

### Faction Presence by Floor

| Floors | Primary | Secondary | Tertiary | Rare/Out-of-Depth |
|--------|---------|-----------|----------|-------------------|
| 1-2 | Animals | Goblins (disorganized) | Kobolds | — |
| 3-4 | Goblins (disorganized) | Animals, Fungal | Kobolds | — |
| 5 | Goblins (organizing) | Animals, Undead | Fungal | Cave Troll, Dragon Whelp |
| 6-7 | Goblins (organized) | Dragons, Undead | Fungal | Cave Troll |
| 8 | Goblins (fortified) | Dragons, Undead | — | — |
| 9 | Goblins (elite) | Dragons | Undead | — |
| 10 | All factions (final gauntlet) | — | — | — |

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

## Faction: Goblins

*The star faction. Goblins evolve from disorganized rabble on floor 1 into
fortified, machine-driven encounters on floor 9. Their progression mirrors
the dungeon's escalating complexity.*

### Goblin Evolution by Depth

| Depth | Goblin State | What Changes |
|-------|-------------|--------------|
| 1-3 | **Disorganized** | Small groups, no leaders, flee easily |
| 4-5 | **Organizing** | Shamans appear. Archers provide ranged support. Brutes tank. |
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

### Goblin Archer
**Floors 2-6 | Ranged support**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d6 | 1 | 1 | 0 | 1.0x | 10 |

- **Identity:** Fragile ranged threat that forces the player to close distance
- **Mechanic:** Introduces **ranged combat**. Range 8 tiles. Keeps distance.
- **Behavior:** Kites; retreats from melee range (within 3 tiles)
- **Group size:** 1-2

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
| 8 | 1d4 | 0 | 1 | 0 | 0.8x | 8 |

- **Identity:** Steals items from chests and hoards them — killing drops stolen loot
- **Mechanic:** Introduces **item-stealing enemies** and **intelligent cowardice**.
  Uses GOAP AI to prioritize stealing over fighting.
- **Behavior:** Cowardly + Intelligent. Flees from the player. Seeks out chests
  and steals items from them. Killing a Kobold Hoarder drops all stolen loot.
- **AI:** GOAP-driven. Goals: acquire items > flee from danger > fight (last resort)
- **Group size:** 1-2

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
**Floors 2-10 | Ancient guardian**

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

### Bloat
**Floors 1-5 | Walking bomb**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 1 | — | 0 | 0 | 0 | 1.2x | 8 |

- **Identity:** A swollen fungal creature that explodes on contact or death
- **Mechanic:** Introduces **explosive enemies** and **positional awareness**.
- **On death (any cause):** Explodes — 3d6 fire damage, 3x3 AoE centered on
  the bloat, friendly fire to all entities. Chain-reacts with nearby bloats.
- **Group size:** 1 (solo, always)
- **Behavior:** Mindless approach; no flee, no kiting

### Arrow Turret
**Floors 3-10 | Stationary corridor hazard**

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
**Floors 6-10 | Devastating brute**

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
**Floors 3-10 | Disguised ambusher**

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
| Goblin Archer | Skirmisher | — |
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
| 1-3 | Goblin Scuffle | Small room with 2-3 goblins and maybe an archer |
| 4-5 | Goblin Camp | Open room with goblins, a shaman, a totem, and a chest |
| 6-7 | Goblin Outpost | Walled room with archers, brutes at the entrance, shaman in the back |
| 8-9 | Goblin Fort | Multi-room encounter with firebombers, a warchief, and high-value loot |

## Design Notes

- **Identity over numbers.** Every monster entry should answer "what does the
  player learn from fighting this?"
- **Dragons are aspirational.** Seeing a dragon whelp on floor 4 and running
  away, then fighting one on floor 6 with better gear, is a satisfying power arc.
- **All abilities are cooldown-based.** No mana pools for monsters. This keeps
  monster design simple and predictable for the player to learn.
