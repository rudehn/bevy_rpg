# Implementation Roadmap

## Goal

Get to a playable, fun game as fast as possible. Each phase has a playtest
gate — don't move to the next phase until the current one feels right.

---

## Phase 1: Core Combat Loop

*Make fighting feel crisp on one floor.*

### Tasks

- [ ] Replace d100 hit system with d20: `d20 + hit_bonus >= 4 + dodge_bonus`
- [ ] Implement crits: natural 20 = auto-hit + double damage dice
- [ ] Add 4 damage types: Physical, Fire, Lightning, Necrotic
- [ ] Physical: `(raw - armor).max(0) * (1.0 - resist/100)`
- [ ] Fire/Lightning/Necrotic: `raw * (1.0 - resist/100)` (skip armor)
- [ ] Resistance system: 0=normal, 50=half, 100=immune, >100=heals, negative=vulnerable
- [ ] Remove attribute stats (STR/DEX/CON/AGI/INT/PER) from player and monsters
- [ ] Convert monsters to direct stats (HP, damage, hit_bonus, dodge_bonus, armor, delay, vision)
- [ ] Remove XP/leveling system
- [ ] Add essence drops on monster kill (= monster base_hp)
- [ ] Burning status effect: fire damage/turn, duration-based, extinguished by shallow water
- [ ] Status effect stacking: new application refreshes duration, no intensity stacking

### Reference Docs
- PLAYER.md: Combat formulas, damage pipeline, damage types
- ENEMIES.md: Monster stat tables, essence drops

### Playtest Gate
Can I fight a goblin and a rat on floor 1 and the combat feels crisp?

---

## Phase 2: Monster Identity

*Every fight feels different.*

### Monsters to Implement (8)

| Monster | HP | Damage | Key Mechanic |
|---------|-----|--------|-------------|
| Giant Rat | 5 | 1d3 | Swarms (group 1-2), flees alone |
| Giant Bat | 4 | 1d3 | Erratic movement (30% random), dodge 2 |
| Wolf | 10 | 1d6 | Pack alerting, vision 12 |
| Fire Salamander | 8 | 1d4 | Burning on hit (2/turn, 3 turns), 50% fire resist |
| Goblin | 5 | 1d4 | Cowardly, flees at 30% HP |
| Goblin Archer | 5 | 1d6 | Ranged 8 tiles, kites from melee |
| Goblin Brute | 14 | 1d8 | Armor 2, first armored enemy |
| Cave Bear | 25 | 2d6 | Slow (1.15x), gives up chase after 8 tiles |

### Tasks

- [ ] Implement group spawning (BFS cluster placement)
- [ ] Squad shared alerting (wolf packs, rat groups)
- [ ] Fleeing AI (pathfind away from player below HP threshold)
- [ ] Kiting AI (ranged enemies retreat from melee range)
- [ ] Erratic movement (random direction % chance)
- [ ] Leash behavior (give up chase after N tiles)
- [ ] Burning-on-hit ability (Fire Salamander)
- [ ] Group size scaling by floor depth

### Reference Docs
- ENEMIES.md: Full stat tables, behaviors, group sizes

### Playtest Gate
Do I make different tactical decisions fighting each monster?

---

## Phase 3: Loot & Growth

*Getting stronger feels good.*

### Tasks

- [ ] Chest entity: spawned by ItemSpawner, contains 1 item
- [ ] Items only from chests (no random floor drops)
- [ ] Weapon types: Dagger (1d4, 0.8x), Short Sword (1d6), Long Sword (1d8, 1.1x),
      Axe (1d8, 1.2x, -1 target armor), Mace (1d6, +2 vs undead), Staff (1d4, +2 mana)
- [ ] Ranged: Short Bow (1d6, range 8), arrows as consumable (stack 30)
- [ ] Armor: 3 tiers per slot (light/medium/heavy)
- [ ] Shields: Wooden (+2), Iron (+3), Tower (+5, +0.1x delay)
- [ ] Rarity tiers: Common (60%), Uncommon (25%), Rare (12%), Legendary (3%)
- [ ] Rarity weights scale with floor depth
- [ ] Healing Potion (15 HP), Greater Healing Potion (30 HP)
- [ ] Mana Potion (15 mana), Greater Mana Potion (35 mana)
- [ ] Inventory: 20 slots, consumables stack to 5, arrows stack to 30

### Reference Docs
- ITEMS.md: Full weapon/armor/consumable tables, rarity weights

### Playtest Gate
Am I excited to open a chest? Does finding a Long Sword feel meaningful?

---

## Phase 4: Shrines & Spells

*Build identity emerges. My floor 5 character is different from floor 1.*

### Shrines to Implement (8 of 30)

| Shrine | Cost | Effect | Why First |
|--------|------|--------|-----------|
| Vitality | 40 | +5 max HP | Core stat |
| Fortitude | 40 | +1 armor | Core stat |
| Arcana | 40 | +10 max mana | Caster enabler |
| Alacrity | 50 | -0.05x delay | Speed build |
| Brutality | 70 | +2 damage bonus | Melee build |
| Regeneration | 50 | +1 HP regen/turn | Sustain build |
| Cleave | 100 | Melee hits all adjacent | Build-defining |
| Death's Door | 175 | Survive first lethal hit per floor at 1 HP | Run-saving |

### Spells to Implement (6 of 18)

| Spell | Tier | Mana | Effect |
|-------|------|------|--------|
| Spark | 1 | 3 | 1d4 lightning, no cooldown |
| Magic Missile | 1 | 5 | 2d4 physical, CD 4 |
| Fire Dart | 1 | 8 | 2d6 fire, CD 3 |
| Minor Heal | 1 | 4 | 1d4 heal, CD 2 |
| Enrage | 1 | 8 | +3 damage, 6 turns, CD 10 |
| Weaken | 1 | 8 | -3 damage on target, 8 turns, CD 10 |

### Tasks

- [ ] Shrine placement system (3/floor in secluded nooks via chokepoint analysis)
- [ ] Shrine interaction UI (show name, effect, cost; buy or leave)
- [ ] Essence spending at shrines
- [ ] Spell slot system (start with 1, shrines unlock more, max 6)
- [ ] Spellbook screen (equip known spells into active slots)
- [ ] Spell targeting UI (cursor targeting for enemy spells)
- [ ] Spell cooldown tracking
- [ ] Goblin Shaman monster (casts Magic Missile, drops tome at 25%)
- [ ] Spell shrine variant (shows which spell, costs essence)
- [ ] Buff/debuff system (Enrage, Weaken with turn duration tracking)
- [ ] Regen suppression (5 turns after taking damage)

### Reference Docs
- TYRANT.md: Shrine catalog, essence economy
- SPELLS.md: Spell list, acquisition, mana system
- ENEMIES.md: Goblin Shaman stats

### Playtest Gate
Does my floor 5 character feel meaningfully different from floor 1?
Do I make real choices at shrines?

---

## Phase 5: Dungeon Variety

*Exploration is rewarding. Each floor has something to find.*

### Machines to Implement (4 of 11)

| Machine | Gate | Content |
|---------|------|---------|
| Goblin Camp | Open | goblin_squad + goblin_archers, watchfire, chest |
| Treasure Vault | Locked | melee guard, 2 chests |
| Hidden Armory | Hidden | chest (no monsters) |
| Monster Den | Open | threat horde + swarm horde, chest |

### Tasks

- [ ] Machine placer (find chokepoints, gate regions, populate interiors)
- [ ] Horde definitions (rat_pack, goblin_patrol, goblin_squad, goblin_archers)
- [ ] Tag-based horde resolution (guard, patrol, swarm, threat)
- [ ] Guard AI (home position, patrol 3 tiles, return after chase)
- [ ] Lock & key system (LockedDoor terrain, key item, bump-to-unlock)
- [ ] Key placement via widening Dijkstra search
- [ ] Hidden doors (10-60% conversion chance, 15% discovery per turn)
- [ ] Hidden door discovery (adjacent + in FOV check)
- [ ] Machine budget: 2-3 per floor
- [ ] Cavern generation ratio scaling with depth

### Reference Docs
- ENCOUNTERS.md: Machine system, horde definitions, lock & key
- DUNGEON.md: Hidden doors, generation style, builder pipeline

### Playtest Gate
Do I want to explore each floor? Are machines worth entering?

---

## Phase 6: The Tyrant

*The game has an ending. You can win or lose.*

### Aspects to Implement (4 of 10)

| Aspect | Stage 1 | Stage 2 | Stage 3 |
|--------|---------|---------|---------|
| Flame | Fire Dart | + Fireball, 50% fire resist | + Fire immune, 40% burning on melee |
| Iron | +2 armor | + +4 armor, 2 reflected | + +6 armor, 3 reflected, 50% phys resist |
| Blood | +15 HP, regen 3 | + +30 HP, regen 6, +3 dmg <40% | + +45 HP, regen 8, +6 dmg <60% |
| Storm | Spark | + Chain Lightning, 15% stun | + 30% stun, knockback 2 |

### Tasks

- [ ] Tyrant base stats: 120 HP, 2d8+4, 4 hit, 2 dodge, 3 armor, 0.9x delay
- [ ] Aspect selection (3 random from 4 at run start)
- [ ] Hunger clock (global game time, thresholds at 12.5k/30k/50k)
- [ ] Aspect stage advancement when clock crosses thresholds
- [ ] Whisper messages in game log on stage transitions
- [ ] Tyrant ability application (read Aspects, apply abilities at spawn)
- [ ] Floor 10 generation with throne room constraint
- [ ] Victory screen on Tyrant death
- [ ] Death screen with run summary (floor, essence, shrines, equipment, cause)
- [ ] Permadeath (delete save on death)
- [ ] Beyond Stage 3 scaling (+15 HP, +1 armor per 25k time)

### Reference Docs
- TYRANT.md: Aspect pool, hunger clock, base stats
- GAME.md: Win/lose conditions
- DUNGEON.md: Floor 10 generation

### Playtest Gate
Is the boss fight a satisfying conclusion? Does the hunger clock create tension?
Do I want to play again with different Aspects?

---

## Phase 7: Depth & Polish

*The game is fun. Now make it deep and replayable.*

### 7a: More Monsters
- [ ] Giant Spider (web/slow)
- [ ] Cave Troll (regen, fire vulnerable)
- [ ] Goblin Warchief (aura +2 damage/dodge, squad dissolves on death)
- [ ] Goblin Firebomber (AoE fire flask, area denial)
- [ ] Goblin Totem (stationary, casts Haste + Chain Lightning, cooldown-only)
- [ ] Rat Queen (summons rats every 5 turns)
- [ ] Jelly (splits on hit, fire prevents split)
- [ ] Bloat (1 HP, explodes 3d6 fire AoE on death, chain reaction)
- [ ] Stone Sentinel (200 HP, 10d20, 8x delay, guard AI)
- [ ] Dragon Whelp (fire breath cone, fire immune)
- [ ] Young Dragon (apex predator)

### 7b: More Shrines & Spells
- [ ] Remaining 22 shrines
- [ ] Remaining 12 spells (Ignite, Heal, Haste, Slow, Lightning Bolt, Fireball,
      Chain Lightning, Greater Heal, Curse, Teleport, Death Coil)
- [ ] Haste/Slow speed multiplier system

### 7c: More Items
- [ ] Rings (Protection, Might, Precision, Evasion, Regeneration, Mage, Speed, Vitality)
- [ ] Amulets (Life, Warding, Swiftness, Inferno, Grounding)
- [ ] Scrolls (Teleport, Mapping, Fireball, Fear)
- [ ] Legendary items with unique effects
- [ ] Typed damage weapons (fire sword, lightning mace)
- [ ] Great Axe (2d6, two-handed), Crossbow (1d10, slow reload)

### 7d: More Encounters
- [ ] Sub-machines (machines inside machines)
- [ ] Goblin Outpost machine (brutes, archers, shaman, totem)
- [ ] Goblin Fort machine (warchief, locked, sub-machine treasury)
- [ ] Environmental machines (Flooded Chamber, Fungal Grotto, Lava Vault)
- [ ] Full horde/tag system with coverage validation
- [ ] Placement hints (AtGate, NearGate, Center, DeepInterior, AlongWalls)
- [ ] Barricade prop (10 HP, destructible, blocks projectiles)
- [ ] Out-of-depth encounters (2x essence reward)

### 7e: Remaining Aspects
- [ ] Shadow, Mind, Bone, Swarm, Paralysis, Void Aspects

### 7f: Dungeon Features
- [ ] Deep water item sweep (Brogue-style)
- [ ] Lava instant death (fire resist = 15 HP/turn instead)
- [ ] Lake placement as thematic encounters (fewer, larger)
- [ ] Lava lakes starting floor 5, increasing probability
- [ ] Decoration future: burnable grass, visibility-blocking tall grass

### 7g: Deferred Systems
- [ ] Corruption Sites (Aspect Champions, Corruption Altars)
- [ ] Behavior tree AI for Tyrant
- [ ] Mini-bosses
- [ ] Future factions (Undead, Orcs, Demons, Ogres)

---

## Summary

| Phase | What You Get | Est. Scope |
|-------|-------------|------------|
| 1 | Combat feels crisp | Core systems rewrite |
| 2 | Fights feel different | 8 monsters + AI behaviors |
| 3 | Loot is exciting | Items + chests + rarity |
| 4 | Builds emerge | Shrines + spells + essence economy |
| 5 | Exploration is rewarding | Machines + hidden doors + lock & key |
| 6 | Game has an ending | Tyrant + Aspects + win/lose screens |
| 7 | Game has depth | Everything else |

**Phases 1-3:** Playable game.
**Phases 4-5:** Fun game.
**Phase 6:** Complete game.
**Phase 7:** Deep game.
