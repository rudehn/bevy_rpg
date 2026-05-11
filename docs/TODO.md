Upstairs sprite on nearby item is ascii
Default ascii mode
AI - put out fire?
kobold lair - fewer of them, more interesting encounter
Rare shrine to grant a runic upgrade?
goblins defend chests from kobolds?
stealth system
axe weapon
spear weapon

Win condition
Orc camps
better itesm
WASM
more status effects
more abilities
passive abilities
more magic
traps
reduce number of chests - make them special
intelligent monsters can equip items
kobold just keeps following me
pathfinding onto water

cursed runics

Game Status Review: The Veiled Tyrant
What's Implemented (it's a lot)
Core Loop — Fully functional turn-based roguelike:

8-directional movement, doors, chests, item pickup
Turn queue with speed-based scheduling
Save/load with permadeath, auto-save per floor
Multi-floor exploration (ascending/descending with floor caching)
ASCII mode toggle (F5)
Combat — Deep and working:

Melee + ranged (bow/arrows) + staves (lightning/poison/blink)
Damage types (Physical, Fire, Lightning, Necrotic, Poison) with resistances
Enchanted weapons/armor with runics (Speed, Paralysis, Quietus, etc.)
Status effects: Hasted, Slowed, Stunned, Burning, Poisoned, Enraged
Monster abilities: BurningStrike, StunningBlow, LifeDrain, Knockback, Leap
AI — Surprisingly sophisticated:

FSM AI (Asleep → Idle → Hunting) with vision, pathfinding, leashing
GOAP planner (Kobold hoarders, Goblin cowards, leader coordination)
Squad system with shared alerting, leader death effects, morale-based fleeing
Faction system (8 factions with relationship matrix)
Content — 34 monsters across 9 factions, 16+ items, procedural map gen with lakes, machines, prefabs

UI — HUD, inventory, character info, targeting, log history, enchant/staff select, nearby entity sidebar

What's Missing for MVP
Looking at docs/TODO.md and the codebase, here's what I'd consider the MVP gap — things needed for a satisfying playable experience:

Priority	Feature	Why it matters
Critical	Win condition	No way to "beat" the game — need a Tyrant boss fight or final floor
Critical	Better item variety	Only 3 weapons (dagger/sword/bow), 5 armor pieces — needs axe, spear, more interesting loot
Critical	Traps	Core Brogue mechanic, adds tactical depth to exploration
High	More consumables/potions	Limited utility items — Brogue thrives on creative consumable use
High	Stealth system	Sleeping monsters exist but no real stealth gameplay
High	Cursed runics	Risk/reward for unidentified enchantments
Medium	Reduce chest frequency	Currently too common, diminishes discovery
Medium	Intelligent monster equip	Monsters picking up/using items adds emergent gameplay
Medium	Bug fixes	Kobold pathfinding onto water, upstairs ASCII sprite
My Assessment
The game is mechanically rich — combat, AI, enchantments, factions, and squads are all well beyond MVP quality. What's lacking is content breadth (more items/weapons) and completion conditions (win state, final boss encounter).

Suggested priority order for next steps:

Win condition + Tyrant boss encounter (gives the game a goal)
Trap system (core Brogue mechanic, high impact)
More weapons & consumables (axe, spear, potions with varied effects)
Stealth system (leverages existing sleeping AI)
Cursed runics (risk/reward layer)
Want me to brainstorm and plan any of these features? I can use the game mechanics designer or content forge to flesh out designs before we start coding.


♈︎ for tall grass symbol

Create a new rust roguelike tooling (like bracketlib) project


ExplodeOnHit ability should have an effect, not always create a chasm.

Should the spawn on_leader_death functionality be deleted?


Falling down a chasm puts you right next to the stairs
bloat should kill itself when it hits an attack