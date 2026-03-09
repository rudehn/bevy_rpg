# Game Overview

## Vision

A classic heroic roguelike where a lone hero descends into a 10-floor dungeon to retrieve the **Amulet of Dominion** — a relic of immense power lost to darkness. Death is permanent. Every run is procedurally generated. Victory is hard-earned.

The game draws inspiration from Brogue: respect the player's intelligence, reward careful play, let the environment be dangerous and legible. No hand-holding, no classes, no shops — just the hero, the dungeon, and what they find inside it.

## Design Pillars

- **Exploration first** — the dungeon should feel worth exploring. Secrets, variety, and environmental storytelling matter.
- **Risk vs. reward** — every decision has a cost. Burning a spell, drinking an unknown potion, pushing deeper.
- **Emergent builds** — with no fixed class, each run's identity comes from which items and spells the player finds and combines.
- **Readable danger** — enemies telegraph their threat level. The player should be able to make informed decisions.
- **Symmetric combat** — the player and all monsters share the same stat system (STR, DEX, CON, AGI, INT, PER) and combat formulas. There are no separate "player rules" vs "monster rules." A buff or debuff works the same regardless of who receives it.

## Core Gameplay Loop

```
Enter floor
  → Explore map (FOV-based, procedurally generated)
  → Fight enemies (turn-based, tactical)
  → Collect loot (items, gold, spellbooks)
  → Grow stronger (XP → level up → stat point + possible spell slot)
  → Find the down-stairs
Descend to next floor
  → Repeat until floor 10
Find and defeat the final boss
  → Pick up the Amulet of Dominion
Victory
```

## Win Condition

On **Floor 10**, the player finds a sealed boss chamber. Inside is the **Shadow Archon**, the dungeon's final guardian, and the **Amulet of Dominion** on a pedestal behind it. Defeating the Shadow Archon causes the amulet to become accessible. Picking up the amulet ends the run — the hero escapes victorious.

## Lose Condition

The player's HP reaches 0. The character is dead. The run is over. A death summary screen shows how far they got, what they were carrying, and how they died. A new run starts fresh.

**No revives. No saves. Full permadeath.**

## Tone

Classic heroic fantasy. The dungeon is dangerous and atmospheric but not grimdark. Enemies have personality. Item descriptions have flavor. The writing is dry, wry, and occasionally ominous — in the tradition of Nethack and Brogue rather than dark fantasy novels.

The hero is unnamed and classless — a blank slate shaped by the run.

## Floor Structure

| Floors | Theme | Boss |
|--------|-------|------|
| 1-3 | Surface dungeon — caves, goblin dens | Floor 3: Goblin Warchief (mini-boss) |
| 4-6 | Catacombs — undead, dark knights | Floor 6: Bone Lord (mini-boss) |
| 7-9 | Infernal depths — demons, hellfire | Floor 9: Pit Fiend (mini-boss) |
| 10 | The Amulet Chamber | Floor 10: Shadow Archon (final boss) |

See [BESTIARY.md](BESTIARY.md) for full enemy and boss details.

## Scope Constraints

- No persistent meta-progression between runs (pure roguelike)
- No multiplayer
- No shops or merchants — all items found as loot
- Item identification is a potential stretch goal
- WASM export is a future target
