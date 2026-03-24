# The Veiled Tyrant — Game Design

## Vision

A Brogue-inspired roguelike where a lone hero descends a 10-floor dungeon to kill
the **Veiled Tyrant** — a boss that grows stronger the longer you take. Death is
permanent. Every run is procedurally generated. Victory is hard-earned.

No classes, no shops, no meta-progression. The hero's identity emerges from what
they find and how they spend essence. The dungeon is dangerous, readable, and
rewards careful play.

## Design Pillars

1. **Exploration first** — The dungeon should feel worth exploring. Secrets,
   variety, prefabs, machines, and environmental storytelling on every floor.
2. **Risk vs. reward** — Every decision has a cost. Burning a spell, drinking an
   unknown potion, pushing deeper instead of backtracking for a shrine.
3. **Emergent builds** — No fixed class. Each run's identity comes from which
   items, spells, and shrines the player finds and combines.
4. **Readable danger** — Enemies telegraph their threat level. The player should
   be able to make informed decisions before committing to a fight.
5. **Symmetric combat** — Player and monsters share the same stat system and
   combat formulas. A buff or debuff works the same regardless of who receives it.
6. **Escalating tension** — The Veiled Tyrant grows stronger over game time. The
   dungeon is not a place to linger. Every turn spent exploring is a turn the boss
   spends preparing.

## Core Gameplay Loop

```
Enter floor
  -> Explore map (FOV-based, procedurally generated)
  -> Fight enemies (turn-based, d20 combat)
  -> Collect loot (items, spellbooks, essence)
  -> Grow stronger (shrines, equipment, spells)
  -> Discover machines & prefab encounters
  -> Find the down-stairs
Descend to next floor
  -> Repeat through floor 9
Floor 10: Tyrant's Throne
  -> Defeat the Veiled Tyrant
Victory
```

## Win Condition

On **floor 10**, the player enters the Tyrant's throne room. The **Veiled Tyrant**
is the final and only boss. Its abilities are determined by 3 randomly selected
Aspects that have been growing stronger throughout the run (see TYRANT.md).

Killing the Tyrant ends the run immediately with a victory screen.

## Lose Condition

The player's HP reaches 0. The character is dead. The run is over. A death summary
screen shows how far they got, what they were carrying, and how they died. A new
run starts fresh.

**No revives. Full permadeath.** The game supports save-and-quit (resumed on load,
deleted on death).

## Tone

Classic heroic fantasy. The dungeon is dangerous and atmospheric but not grimdark.
Enemies have personality. Item descriptions have flavor. The writing is dry, wry,
and occasionally ominous — in the tradition of Nethack and Brogue.

The hero is unnamed and classless — a blank slate shaped by the run.

## Progression

**Essence** is the sole progression currency. Monsters drop essence on death.
Essence is spent at **shrines** — permanent upgrade stations found in remote
corners of each floor. Shrines define playstyle: melee, ranged, caster, survival,
or hybrid. There is no XP, no leveling, and no stat points.

The three pillars of character power:
- **Shrines** define playstyle (how you fight)
- **Spellbooks** provide tools (what you can cast)
- **Equipment** provides raw stats (damage/defense numbers)

## Floor Structure

10 floors. Themed tiers TBD — floor themes, generation styles, and visual
identity will be designed after the core systems are locked. The following
constraints are established:

- Prefabs and machines appear on **all floors**, not just late game
- Corruption Sites (Aspect Champion encounters) appear on floors 3-5, 5-7,
  and 7-9 (see TYRANT.md)
- 3 shrines spawn per floor in out-of-the-way locations
- Floor 10 is the Tyrant's throne room (unique layout)
- Floor difficulty scales via monster spawns, trap density, and lighting

## The Veiled Tyrant

At run start, 3 **Aspects** are randomly selected from a pool of 10. Each Aspect
grows through 3 stages on a hunger clock tied to game time. The Aspects determine
the Tyrant's abilities, resistances, and spells during the floor 10 fight.

The player can weaken Aspects by finding and clearing **Corruption Sites** —
optional encounters scattered across the mid-game floors. Destroying a Corruption
Altar caps that Aspect at Stage 1. Ignoring it lets the Aspect grow unchecked.

3 Aspects from 10 = 120 unique boss combinations per run. No dominant strategy
works across all combinations.

Full details in TYRANT.md.

## Scope Constraints

- No persistent meta-progression between runs (pure roguelike)
- No multiplayer
- No shops or merchants — all items found as loot
- No item identification
- No XP or leveling — essence and shrines only
- No mini-bosses (TBD — may workshop later)
- WASM export is a future target

## Open Questions

1. **Floor tier themes** — How many tiers? What are the visual/mechanical
   identities? What generation style per tier (room-based vs. cavernous)?
2. **Monster faction distribution** — Which factions appear on which floors?
3. **Backtracking** — Can the player go back up stairs? If so, do floors persist
   in a floor cache? (Current implementation: yes to both.)
4. **Death summary** — What stats/info appear on the death screen?
