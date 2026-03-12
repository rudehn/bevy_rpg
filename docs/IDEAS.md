# Ideas

This document is a curation of gameplay ideas that would be nice to add to the game (potentially with modification)

HP regen should be slow.
* Most monsters should regain hp slowly, while some monsters, like Trolls, should regain HP at an accelerated rate
* Player should be able to find resources to regen HP faster (items, higher constitution, etc)
* No HP regen while in combat

MP regen should be very slow.
* No MP regen while in combat
* Regen should scale with intelligence 

Doors
* Hidden / secret
* Requires a key

Monsters
* Each monster should have a unique "feel" or characteristic about them. Something that makes them special amongst all the other monsters
* Add support type monsters (heal, buff, haste, life drain player, split some hp to ally, etc)

Items
* Items with higher than common rarity should have a hue around them when on the floor. This hue is defined in ITEMS.md
* Remove is_victory from required item definition ron file



1. System Design: The "Chemistry" of the World
Instead of isolated mechanics, aim for interlocking systems. This makes the world feel alive and rewards experimentation.

Environmental Interaction: If a player casts a fire spell near a wooden door, the door should burn. If they cast it on a frozen tile, it creates a puddle. This turns the environment into a secondary weapon or a hazard.

Data-Driven Entities: Design monsters and items with "Tags" rather than hard-coded logic. A "Flying" tag might mean the creature ignores trap triggers, while a "Metallic" tag makes them susceptible to magnetic or lightning-based effects.

Event-Based Communication: Structure your game logic so that systems "listen" for messages. When a player moves, an AfterMove event can trigger hunger checks, trap checks, and monster AI updates in a clean, predictable sequence.

2. Progression: Beyond Just Numbers
Flat stat increases (+1 Strength) are functional but rarely "fun." Focus on horizontal progression—giving the player more tools, not just bigger numbers.

The "Legacy" System: Since permadeath is a staple, allow the player’s previous run to affect the next one in small, flavor-heavy ways. Perhaps a previous hero’s ghost haunts the level where they died, or their "fame" allows the next character to start with a slightly better starting kit.

Skill Synergies: Instead of a linear tree, use a system where combining two disparate skills unlocks a third "hidden" trait. (e.g., Fire Magic + Shield Bash = Flaming Retribution).

3. Combat & Bosses: Tactical "Puzzles"
In a turn-based environment, combat should feel like a high-stakes puzzle.

Telegraphed Attacks: For bosses or elite mobs, use a "Wind-up" turn. The boss glows or shifts its stance, and the UI highlights the tiles it will hit next turn. This shifts the focus from "Can I tank this hit?" to "How do I reposition to punish this move?"

The "Clock" Mechanic: Traditional roguelikes use a "Bumping" system, but you can add depth by giving different weapons different "Recovery Times." A heavy greataxe might take 1.5 turns to swing, allowing a faster enemy to potentially hit you twice in the interim.

4. Player Choice: Meaningful Trade-offs
A choice isn't fun if there is an obvious "correct" answer.

Identify-by-Use: The classic trope of drinking an unknown potion. To make this fun, ensure that even "negative" effects can be used strategically. A potion of blindness is bad for the player, but what if they can throw it at a boss?

Resource Scarcity vs. Greed: Provide "Vaults" that are clearly visible but highly guarded. The choice is: "Do I spend my last two healing potions to get that artifact now, or do I hope I find a better way in later?"

5. Atmosphere & "The Feel"
Fog of War as a Mechanic: Don't just hide tiles; make the darkness feel oppressive. Light sources should be a limited resource (torches, lanterns) that provide a tactical advantage by letting you see telegraphed attacks from further away.

Minimalist UI: Keep the screen uncluttered. If a monster is bleeding, show a small blood particle or a tint rather than a text log that says "The Orc is bleeding."


To build a traditional roguelike, you're essentially orchestrating a complex simulation where dozens of independent systems must shake hands every turn. These systems generally fall into four main categories: Foundational, Mechanical, World-Building, and Meta-Systems.

1. Foundational Systems (The "Engine" Room)
These are the invisible gears that keep the game running. In a turn-based environment, these must be rock-solid.

Turn Scheduler: Manages the "Action Point" or "Energy" economy. It determines if a fast rogue acts twice before a slow golem acts once.

Field of View (FOV) & Line of Sight (LOS): Calculates what the player can see vs. what is hidden in the "fog of war."

Pathfinding (A or Dijkstra Maps):* Tells monsters how to hunt the player, flee when wounded, or navigate around a lava pit.

Collision & Spatial Indexing: Keeps track of which entity (monster, item, trap) occupies which tile.

2. World & Generation Systems
The "rogue" in roguelike usually implies procedural unpredictability.

Map Generation: The algorithms (BSP trees, Cellular Automata, or Drunkard’s Walk) that carve out the dungeon.

Static/Dynamic Spawning: Controls the "density" of a floor—ensuring there aren't 50 dragons in Room 1.

Tilemap Management: Handles the rendering of floors, walls, and liquid, often including "auto-tiling" logic to make corners look right.

Environmental Hazards: Manages traps, slippery ice, burning oil, or poisonous gas clouds that expand over time.

3. Gameplay & RPG Mechanics
This is where the player interacts with your "fantasy" ruleset.

Combat Engine: Calculates hit chance, damage reduction, critical hits, and elemental resistances.

Status Effect Registry: A "listener" system that tracks poisons, stuns, bleeds, or buffs like "Strength of the Giant."

Inventory & Equipment: Manages the player's "paper doll" (slots for head, chest, rings) and item weight/encumbrance.

Identification System: The "mystery" logic for potions, scrolls, and wands (e.g., "A bubbly blue potion" becomes "Healing" once tasted).

AI Behavior Trees: Simple (bump-to-attack) or complex (archer keeps distance, healer stays behind tank).

4. Progression & Economy
How the player grows (or fails) over time.

Experience & Leveling: The math behind XP curves and stat increases.

Skill/Feat Trees: Unlocking active abilities or passive perks.

Loot Tables (Weighted Randomness): Ensures that rare artifacts actually feel rare while "trash" loot provides basic utility.

Faction/Reputation: (Optional) Determines if the Goblins attack on sight or are willing to trade.

5. Meta & UX Systems
The wrapper that makes the game a "product."

Messaging/Log System: The "combat log" that narrates the game (e.g., "The Bat misses you!").

Save/Load & Persistence: Handling "Permadeath" by deleting the save file on death or preserving "Meta-progression" (like unlocked classes).

UI/HUD: Displays health bars, hotbars, and the "examine" tool for looking at monsters.

Seed Management: Allows players to replay a specific dungeon layout by sharing a string of numbers.

System,Simple (Traditional),Advanced (Simulation-Heavy)
Movement,Random wander,Goal-oriented (hunting/fleeing)
Detection,Proximity based,Sound/Scent/Light level based
Interaction,"""Bump"" to attack",Environment manipulation (pushing boulders)