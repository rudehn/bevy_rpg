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

## Factions

| Faction | Floors | Role |
|---------|--------|------|
| Animals | 1-6 | Nature's hazards. Teach basic combat, speed, swarms, DoT. |
| Goblins | 1-10 | The star faction. Evolve from disorganized to structured. |
| Dragons | 5-10 | Apex predators. Rare, powerful, fire-themed. |
| *(none)* | 4-8 | Cave Troll — factionless solo threat. |

### Faction Presence by Floor

| Floors | Primary | Secondary | Rare/Out-of-Depth |
|--------|---------|-----------|-------------------|
| 1-2 | Animals | Goblins (disorganized) | — |
| 3-4 | Goblins (disorganized) | Animals | — |
| 5 | Goblins (organizing) | Animals | Cave Troll, Dragon Whelp |
| 6-7 | Goblins (organized) | Dragons | Cave Troll |
| 8 | Goblins (fortified) | Dragons | — |
| 9 | Goblins (elite) | Dragons | — |
| 10 | Veiled Tyrant | — | — |

## Stat System

Monsters use the same combat system as the player (symmetric combat). Each
monster has direct stats — no attribute derivation.

### Essence Drops

Monsters drop **essence equal to their base HP** on death. A Giant Rat (5 HP)
drops 5 essence. A Dragon Whelp (24 HP) drops 24. Out-of-depth kills award
**2x essence**. See TYRANT.md for the full essence economy.

| Stat | Description |
|------|-------------|
| HP | Hit points |
| Damage | Damage dice |
| Hit Bonus | Added to d20 attack roll |
| Dodge Bonus | Added to base dodge target (4) |
| Armor | Flat damage reduction (physical only) |
| Delay | Action speed multiplier (lower = faster) |
| Vision | Sight range in tiles |

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
- **Mechanic:** Introduces **summoning enemies**. Every 5 turns, spawns a Giant
  Rat on an adjacent tile (max 4 active spawned rats). If you don't kill the
  queen quickly, the room fills with rats. Priority target.
- **Summon:** 1 Giant Rat every 5 turns on adjacent tile (max 4 spawned)
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
- **Behavior:** Erratic; doesn't path directly to player

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
  apply burning. Player learns to disengage and recover vs. continuing to
  fight while taking DoT.
- **Burning:** 2 fire damage/turn for 3 turns on hit
- **Fire Resistant:** 50% fire resistance (takes half fire damage)
- **Group size by floor:** 1 (floors 2-3), 1-2 (floors 4-5)

### Giant Spider
**Floors 3-6 | Ambush predator**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 10 | 1d4 | 1 | 1 | 0 | 1.0x | 10 |

- **Identity:** Web ability, ambush from dark corners
- **Mechanic:** Introduces **movement debuffs**. Web slows the player for 3
  turns (delay x 1.5). Being slowed while other enemies approach is terrifying.
- **Web:** Range 4, applies Slow for 3 turns, cooldown 8 turns
- **Group size by floor:** 1 (floors 3-4), 1-2 (floors 5-6)

### Cave Bear
**Floors 3-6 | Slow devastator**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 25 | 2d6 | 2 | 0 | 2 | 1.15x | 6 |

- **Identity:** Slow but devastating — you can't trade hits
- **Mechanic:** Introduces **kiting**. The bear's high damage and HP but slow
  speed teaches the player to use corridors and speed advantage. Also the first
  monster worth avoiding entirely if undergeared.
- **Group size:** 1 (solo, always)
- **Behavior:** Doesn't chase far — gives up after 8 tiles if it can't reach player

---

## Faction: Goblins

*The star faction. Goblins evolve from disorganized rabble on floor 1 into
fortified, machine-driven encounters on floor 9. Their progression mirrors
the dungeon's escalating complexity.*

### Goblin Evolution by Depth

| Depth | Goblin State | What Changes |
|-------|-------------|--------------|
| 1-3 | **Disorganized** | Small groups, no leaders, flee easily. Mini encounters as early as floor 2. |
| 4-5 | **Organizing** | Shamans appear. Archers provide ranged support. Brutes tank. Totems appear. |
| 6-7 | **Organized** | Warchief-led squads with aura buffs. Firebombers add AoE threat. |
| 8-9 | **Fortified** | Goblin camps and forts via machine system. Structured encounters. |

### Goblin
**Floors 1-5 | Cowardly footsoldier**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d4 | 0 | 1 | 0 | 1.0x | 8 |

- **Identity:** Cowardly and weak — but never alone
- **Mechanic:** Introduces **fleeing enemies**. Flees when below 30% HP. Teaches
  the player to chase wounded enemies or let them escape (they don't heal).
- **Group size:** 1-3
- **Behavior:** Cowardly; flees at 30% HP

### Goblin Archer
**Floors 2-6 | Ranged support**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 5 | 1d6 | 1 | 1 | 0 | 1.0x | 10 |

- **Identity:** Fragile ranged threat that forces the player to close distance
- **Mechanic:** Introduces **ranged combat**. Range 8 tiles. Keeps distance from
  the player — retreats if player gets within 3 tiles.
- **Ranged:** 8 tiles
- **Group size:** 1-2
- **Behavior:** Kites; retreats from melee range

### Goblin Brute
**Floors 3-7 | Armored tank**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 14 | 1d8 | 1 | 0 | 2 | 1.1x | 7 |

- **Identity:** The first armored enemy — physical attacks bounce off
- **Mechanic:** Introduces **armor as a problem**. With 2 armor, weak weapons
  deal almost no damage. Teaches the player to use fire/lightning spells or
  find better weapons.
- **Group size:** 1 (solo, but often near goblin groups)

### Goblin Shaman
**Floors 3-7 | Healer caster**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 6 | 1d4 | 0 | 1 | 0 | 1.0x | 10 |

- **Identity:** Priority target — heals other goblins and casts from range
- **Mechanic:** Introduces **enemy casters** and **priority targeting**. If you
  ignore the shaman, it heals the brute you're fighting. Kill it first.
- **Mana:** 20
- **Spells:** Magic Missile, Minor Heal (targets allies)
- **Group size:** 1 (solo, always accompanies other goblins)
- **Spell drops:** Tome of Magic Missile (25%)

### Goblin Warchief
**Floors 5-9 | Squad leader**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 18 | 1d8 | 2 | 2 | 2 | 0.95x | 10 |

- **Identity:** The brains of the operation — killing it breaks the squad
- **Mechanic:** Introduces **leadership auras** and **decapitation tactics**.
  Grants +2 damage and +2 dodge to all goblins within 5 tiles. On death,
  remaining goblins scatter (lose target, wander aimlessly for 5 turns).
- **Aura:** +2 damage, +2 dodge to nearby goblins (5 tile range)
- **On death:** Squad scatters
- **Group size:** 1 (always accompanied by a squad of 3-5 goblins)

### Goblin Totem
**Floors 4-9 | Stationary force multiplier**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 15 | — | — | 0 | 0 | — | — |

- **Identity:** A carved totem that empowers nearby goblins — destroy it or suffer
- **Mechanic:** Introduces **stationary priority targets**. Does not move or attack.
  Casts **Haste** on a random nearby goblin every 6 turns and **Chain Lightning**
  at the player every 8 turns. Forces the player to push past the goblin
  frontline to reach the totem, or endure hasted enemies and lightning damage.
- **Haste:** Targets a random goblin within 5 tiles, +50% speed for 8 turns, cooldown 6.
  Cooldown-only ability — no mana pool, no mana cost.
- **Chain Lightning:** 2d6 lightning damage + 2 jumps (1d6 each, 3 tiles), cooldown 8.
  Cooldown-only ability — no mana pool, no mana cost.
- **Cannot be healed** — goblin shamans and other healing effects do not work
  on totems
- **Group size:** 1 (placed in goblin camps/forts, always accompanied by goblins)
- **Behavior:** Stationary. Cannot move. No melee. Dies when HP reaches 0.
- **Note:** Replaces the existing decorative totem pole prop (`assets/props.ron`).
  The old `"totem_pole"` prop and its prefab placements should be removed and
  replaced with this enemy entity.

### Goblin Firebomber
**Floors 6-9 | AoE threat**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 8 | 1d4 (melee) | 0 | 2 | 0 | 1.0x | 10 |

- **Identity:** Throws fire flasks that create burning ground
- **Mechanic:** Introduces **area denial**. Fire flask deals 1d6 fire damage in
  a 3x3 area and leaves burning ground for 3 turns (2 fire damage/turn to
  anything standing in it). Forces the player to reposition.
- **Fire Flask:** Range 6, 1d6 fire AoE (3x3), burning ground 3 turns, cooldown 5
- **Group size:** 1 (solo, paired with other goblins)
- **Behavior:** Stays at range; retreats from melee

---

## Faction: Dragons

*Apex predators. Rare encounters that demand respect. Fire-themed, armored,
and fast. On lower floors they appear as out-of-depth encounters — a clear
signal to run.*

### Dragon Whelp
**Floors 5-9 | Mid-game terror**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 24 | 2d6 | 3 | 2 | 3 | 0.9x | 12 |

- **Identity:** Fast, fire-breathing, fire immune — a serious threat
- **Mechanic:** Introduces **elemental immunity**. Fire spells are useless. The
  player must use physical or lightning damage. Also introduces **fire breath**
  as a cone attack.
- **Fire Breath:** Range 5 cone (3 tiles wide at max range), 2d6 fire damage,
  cooldown 4 turns
- **Fire Immune:** Takes 0 fire damage
- **Group size:** 1 (solo)
- **Out-of-depth:** Can appear as early as floor 4 (rare). A floor 4 dragon
  whelp is a "run away" encounter for most builds.

### Young Dragon
**Floors 8-9 | Apex predator**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 40 | 2d10 | 4 | 2 | 5 | 0.95x | 14 |

- **Identity:** The most dangerous non-boss enemy in the game
- **Mechanic:** Combines fire breath + fire immunity + high armor + high damage.
  Forces the player to use everything they've learned: positioning, kiting,
  lightning/physical damage, buff spells, consumables.
- **Fire Breath:** Range 6 cone, 3d6 fire damage, cooldown 3 turns
- **Fire Immune:** Takes 0 fire damage
- **Group size:** 1 (solo)

---

## Factionless

### Cave Troll
**Floors 4-8 | Regenerating tank**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 28 | 2d6 | 2 | 0 | 2 | 1.1x | 8 |

- **Identity:** Regenerates HP every turn — outlasts you in a war of attrition
- **Mechanic:** Introduces **regeneration as a problem**. Regenerates 2 HP/turn.
  The player must burst it down quickly or disengage. A prolonged fight against
  a troll is unwinnable for most builds. Fire damage is the counter — trolls
  take extra fire damage.
- **Regen:** 2 HP/turn (opt-in; regen is explicit on this monster, not default)
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
  **territorial enemies**. Not every enemy is meant to be fought. Also serves
  as a natural **timer** in machine encounters — the sentinel is the clock.
- **Guard AI:** Spawns at a fixed post. Chases the player when they enter vision
  range (12 tiles). Returns to its post when the player leaves vision range.
- **No regen, no mana.** Can be whittled down over multiple encounters — chip
  away, retreat, come back. But 200 HP and 8 armor makes this a massive
  investment of resources and time (hunger clock).
- **Group size:** 1 (solo, always)
- **Machine synergy:** A room where the door locks and a sentinel advances from
  the far wall. You have X turns to solve the machine before it reaches you.
  Pairs with goblin encounters to create deadly positioning traps.

### Jelly
**Floors 2-7 | Splitting horror**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 50 | 1d3 | 0 | 0 | 0 | 1.2x | 6 |

- **Identity:** Harmless-looking blob that multiplies into a nightmare
- **Mechanic:** Introduces **splitting**. When hit, a new jelly spawns on an
  adjacent tile with **half the remaining HP** of the original (rounded down).
  The original keeps all its remaining HP. A careless player creates a room
  full of jellies. Low individual damage, but 6 jellies hitting you every turn
  adds up fast.
- **Split rules:**
  - On any hit (melee, ranged, or spell), spawn a new jelly adjacent to the
    original with `floor(original.current_hp / 2)` HP
  - The original's HP is NOT reduced by the split — only by the damage dealt
  - New jellies can also split when hit
  - Minimum HP to split: 5 (jellies with < 5 HP do not spawn new ones)
  - Fire damage prevents splitting (the jelly dies without reproducing)
- **No dodge, no hit bonus, no armor, no regen**
- **Group size:** 1 (solo — it makes its own group)
- **Behavior:** Slow, mindless pursuit. No flee, no special AI.

### Bloat
**Floors 1-5 | Walking bomb**

| HP | Damage | Hit | Dodge | Armor | Delay | Vision |
|----|--------|-----|-------|-------|-------|--------|
| 1 | — | 0 | 0 | 0 | 1.2x | 8 |

- **Identity:** A swollen fungal creature that explodes on contact or death
- **Mechanic:** Introduces **explosive enemies** and **positional awareness**.
  Doesn't attack normally. Walks toward the player and explodes when adjacent,
  dealing 3d6 fire damage in a 3x3 area to **everything** (player, monsters,
  other bloats). Can chain-react with nearby bloats.
- **Explosion:** 3d6 fire damage, 3x3 AoE centered on the bloat, friendly fire
  to all entities. Destroys wooden doors caught in the blast.
- **On death (any cause):** Explodes. Killing it at range is safe if you're
  outside the 3x3 area. Killing it in melee guarantees you take the blast.
- **Chain reaction:** If a bloat's explosion kills another bloat, that one also
  explodes. A cluster of 3 bloats is a room-clearing bomb.
- **Group size:** 1 (solo, always)
- **Behavior:** Mindless approach; no flee, no kiting. Ignores other monsters.

---

## Group Spawning

Monsters spawn in groups defined by their group size range. A room rolls a
monster type, then rolls a group size within that monster's range.

### Cluster Placement

Groups are placed using BFS outward from a spawn point:
- Cardinal directions only (tight clumps)
- Graceful degradation: if a room only fits 2 tiles, a group of 4 places 2
- An occupied set prevents stacking across rooms

### Squad Behavior

Groups of 2+ are linked as a **squad** with shared behaviors:

| Behavior | Description |
|----------|-------------|
| **Shared alerting** | When any member spots the player or takes damage, the entire squad activates |
| **Leader leashing** | Non-leader members stay within 4 tiles of the leader; if separated, pathfind to leader first |
| **Leader death** | Squad dissolves — all members become independent, each acting on their own AI |
| **Aura visual** | Aura radius shown as a visual indicator around the leader. Buffed members have a visible status marker so the player can see who's affected. |

Squads are assigned at spawn time. Solo monsters have no squad. Squads don't
merge or recruit dynamically.

---

## Out-of-Depth Encounters

Occasionally, a monster from a deeper floor range spawns on a shallower floor.
These are rare (5-10% chance per floor) and serve as:

- **Warning signs** — a dragon whelp on floor 8 tells you "this is coming"
- **Avoidance puzzles** — the player must recognize they can't win and route around
- **Reward for strong builds** — a well-equipped player who kills an out-of-depth
  enemy gets **2x essence** reward

Out-of-depth monsters are always **solo** (never in groups) and spawn in rooms
the player doesn't have to enter (never blocking the critical path).

---

## Goblin Escalation via Machines

Goblins become more dangerous not just through stronger stat blocks, but through
**structured encounters** built by the machine system (see ENCOUNTERS.md):

| Floor Range | Encounter Type | Description |
|-------------|---------------|-------------|
| 1-3 | Goblin Scuffle | Small room with 2-3 goblins and maybe an archer. First taste of goblin groups. |
| 4-5 | Goblin Camp | Open room with 4-6 goblins, a shaman, a totem, and a chest. Basic group fight. |
| 6-7 | Goblin Outpost | Walled room with archers on elevated positions, brutes guarding the entrance, shaman in the back. |
| 8-9 | Goblin Fort | Multi-room prefab with alarm traps, firebombers, a warchief, and high-value loot. Clearing the fort is optional but rewarding. |

These encounters are placed by the machine system and are optional — the player
can always find the stairs without engaging them.

---

## Monster Caster Summary

| Monster | Mana | Spells | Drop Rate |
|---------|------|--------|-----------|
| Goblin Shaman | 20 | Magic Missile, Minor Heal | Tome of Magic Missile (25%) |
| Goblin Firebomber | — | Fire Flask (ability, not spell) | — |
| Dragon Whelp | — | Fire Breath (ability, not spell) | — |
| Young Dragon | — | Fire Breath (ability, not spell) | — |

Only the Goblin Shaman is a true spell caster with droppable spellbooks. Dragon
fire breath and goblin fire flasks are **abilities**, not spells — they don't
use the mana/spell slot system and don't drop tomes.

---

## The Veiled Tyrant (Floor 10)

The only boss. Its abilities are determined by 3 randomly selected Aspects that
grow stronger throughout the run. See TYRANT.md for full details.

---

## Design Notes

- **Start small, add factions later.** Three factions + factionless monsters
  is enough for a compelling 10-floor experience. Undead, Orcs, Demons, and Ogres
  can be added as future factions.
- **Goblins carry the game.** By making goblins the star faction that evolves
  throughout the entire dungeon, the player has a throughline of escalating
  encounters. Floor 1 goblins and floor 9 goblins should feel like completely
  different challenges.
- **Identity over numbers.** A goblin firebomber with 8 HP is more interesting
  than an orc with 20 HP and no special abilities. Every monster entry should
  answer "what does the player learn from fighting this?"
- **Dragons are aspirational.** Seeing a dragon whelp on floor 4 and running away,
  then fighting one on floor 6 with better gear, is a satisfying power arc.

## Resolved Decisions

- **Out-of-depth essence:** 2x bonus essence for killing out-of-depth enemies
- **Troll fire vulnerability:** -50% fire resistance (takes extra fire damage).
  Regen suppression by fire TBD — will revisit.
- **Jelly minimum split HP:** 5 HP is the floor for splitting
- **Bloat spawning:** Can appear in the open; always solo
- **Sentinel placement:** Very rare. Hand-placed in prefabs/machines preferred,
  but can appear via spawner at very low weight.
- **Squad leader death:** Simplified — squad dissolves, members act independently.
  No scatter/enrage mechanics.
- **Monster HP regen:** Opt-in only. Monsters do not regenerate by default.
  Only monsters with explicit regen (Cave Troll) regenerate.
- **Aura visuals:** Leader auras show a visual radius indicator. Buffed members
  display a visible status marker.

## Open Questions

1. **Goblin fort machine design** — How complex should the multi-room goblin
   fort prefab be? How many rooms, what layout?
2. **Dragon breath cone** — Exact cone shape? 3 tiles wide at max range, or
   a different geometry?
3. **Spider web interaction with fire** — Should fire spells burn away web
   (removing the slow)?
4. **Future factions** — Priority order for adding Undead, Orcs, Demons, Ogres?
