# The Veiled Tyrant

## Player Experience

You descend into the dungeon knowing the Veiled Tyrant waits on floor 10. What
you don't know is *how* it will fight. Each run, the Tyrant invests stolen
essence into three random domains of power — its **Aspects**. These Aspects
grow stronger over time, changing the boss fight from a stat check into a puzzle
you solve across the entire run.

As you explore, you find **Shrines** in remote corners of each floor — permanent
upgrades that shape your build. Some grant raw stats. Others fundamentally change
how you fight.

Every run, you're answering two questions:
1. **Who am I becoming?** (Shrines — your build)
2. **What is the Tyrant becoming?** (Aspects — the boss's build)

---

## Tyrant Aspects

### Overview

At run start, 3 Aspects are randomly selected from a pool of 10. Each Aspect
has 3 growth stages that advance on a hunger clock tied to game time. The
Aspects determine the Tyrant's abilities, resistances, and spells during the
floor 10 boss fight.

3 from 10 Aspects = **120 unique combinations**. No dominant strategy works
across all combinations.

### Hunger Clock

All 3 Aspects advance on the same global timer:

| Stage | Game Time Threshold | Effect |
|-------|-------------------|--------|
| Stage 0 | 0 (start) | Aspects dormant — no effect on Tyrant |
| Stage 1 | 12,500 | Basic ability per Aspect |
| Stage 2 | 30,000 | Stronger ability, may add resistance |
| Stage 3 | 50,000 | Full power — immunity, strong abilities |
| Beyond | Every 25,000 after Stage 3 | Flat stat boosts: +15 HP, +1 armor per tick |

**Time reference:** Each player action costs 100 time units (BASE_ACTION_COST).
A typical floor takes 300-500 actions = 30,000-50,000 time. A 10-floor run at
moderate pace takes ~50,000-60,000 time, reaching Stage 3 near the end.

**Backtracking cost:** Each floor traversed costs ~1,500-3,000 time. A 4-floor
backtrack (~6,000-12,000 time) could push an Aspect from Stage 2 to Stage 3.
The player must weigh "is this shrine worth the time?" against the boss growing
stronger.

### Whisper Messages

The game log delivers atmospheric hints as Aspects advance:

| Event | Message |
|-------|---------|
| Stage 1 triggers | "You feel a dark power stirring in the depths below..." |
| Stage 2 triggers | "The dungeon trembles. The Tyrant grows stronger." |
| Stage 3 triggers | "Reality shudders. The Tyrant has become a force of nature." |
| Beyond ticks | "The Tyrant's power continues to grow without bound..." |

These are non-specific — they don't reveal which Aspects are active.

### Aspect Pool (10 total, 3 selected per run)

#### Flame

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts Fire Dart |
| 2 | + Casts Fireball, 50% fire resistance |
| 3 | + Fire immune (100%), Burning on melee hit (40% chance) |

#### Shadow

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts Death Coil (necrotic) |
| 2 | + Life Drain 15% on melee, 50% necrotic resistance |
| 3 | + Life Drain 30%, necrotic immune, teleports when below 30% HP |

#### Iron

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | +2 armor |
| 2 | + +4 armor total, 2 damage reflected to melee attackers |
| 3 | + +6 armor total, 3 damage reflected, 50% physical resistance |

#### Storm

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts Spark |
| 2 | + Casts Chain Lightning, 15% chance to stun on melee hit |
| 3 | + 30% stun chance on melee, knockback 2 tiles on hit |

#### Blood

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | +15 HP, regen 3/turn |
| 2 | + +30 HP total, regen 6/turn, +3 damage below 40% HP |
| 3 | + +45 HP total, regen 8/turn, +6 damage below 60% HP |

#### Mind

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts Slow |
| 2 | + Casts Curse, +25 mana |
| 3 | + Damage taken from mana first (Spirit Shield), +50 mana, Slow on melee 30% |

#### Bone

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | +2 armor, 50% physical resistance |
| 2 | + Summons 1 skeleton every 12 turns |
| 3 | + Summons 1 skeleton every 8 turns, +4 armor total |

#### Swarm

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Summons 1 monster every 10 turns |
| 2 | + Summons 2 every 8 turns |
| 3 | + Summons 2 every 5 turns |

#### Paralysis

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Slow on melee 20% |
| 2 | + Stun on melee 20%, Slow on melee 30% |
| 3 | + Stun on melee 40%, Slow on melee 40%, knockback 2 |

#### Void

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Teleports every 8 turns |
| 2 | + Teleports every 5 turns, damage taken from mana first |
| 3 | + Teleports every 3 turns, phase walk (passes through walls) |

### Tyrant Base Stats (Before Aspects)

| Stat | Value | Reasoning |
|------|-------|-----------|
| HP | 120 | 8-15 rounds of combat. Long enough to feel epic, not a slog. |
| Damage | 2d8+4 (avg 13) | 26-33% of player HP per hit. Threatening but survivable. |
| Hit Bonus | 4 | Hits ~75% with 0 dodge, ~55% with decent dodge investment. |
| Dodge Bonus | 2 | Player hits ~75% with no hit bonus, ~85% with gear. |
| Armor | 3 | Physical reduced but not negated. Fire/lightning bypass. |
| Delay | 0.9x | Slightly faster than player. Haste counters this. |
| Vision | 20 | Sees entire throne room. No sneaking. |
| Resistances | 0 all | Aspects add resistances. Base Tyrant has none. |

At Stage 0 this is a tough but fair fight. At Stage 3 with Blood + Iron + Flame,
the Tyrant could reach 165 HP, 9 armor, and fire immunity — requiring specific
build prep to overcome.

### Combination Examples

| Run | Aspects | Boss Fight Character | Player Prep |
|-----|---------|---------------------|-------------|
| A | Flame + Iron + Blood | Tanky regen bruiser who burns you | Need armor pen + fire resist + burst damage |
| B | Shadow + Mind + Void | Teleporting mana-draining life stealer | Need necrotic resist + mana protection + mobility |
| C | Storm + Paralysis + Swarm | Stun-locking summoner | Need stun immunity + AoE + patience |
| D | Bone + Blood + Iron | Unkillable wall that summons skeletons | Need sustained DPS + summon management |

---

## Corruption Sites (Deferred)

Corruption Sites will be revisited once the core machine system and Tyrant
Aspects are implemented and tested. The current concept:

- 3 per run, one per Aspect
- Placed on floors 3-5, 5-7, 7-9
- An Aspect Champion guards a Corruption Altar
- Defeating the Champion lets the player destroy the Altar (caps that Aspect
  at Stage 1) or walk away with loot
- Champion drops themed equipment

This system ties into the machine/encounter system and needs the Aspect system
working first. Details will be finalized in a future design pass.

---

## Shrines

Shrines are permanent upgrade stations found in remote dungeon locations. 3
shrines spawn per floor (see ENCOUNTERS.md for placement). Each costs essence
and grants a permanent upgrade for the rest of the run.

Shrines are the primary build system — how the player defines their character's
identity. Combined with spellbook pickups and equipment, shrines determine
playstyle.

The 3-per-floor budget is shared between **stat shrines** (listed below) and
**spell shrines** (which teach a specific spell — see SPELLS.md). A floor might
have 2 stat shrines and 1 spell shrine, or 3 stat shrines and 0 spell shrines.

### Shrine Rarity

Each of the 3 shrine slots per floor rolls independently:

| Rarity | Chance | Avg per Run (30 slots) | Essence Cost |
|--------|--------|----------------------|-------------|
| Common | 50% | ~15 | 30-50 |
| Uncommon | 30% | ~9 | 60-80 |
| Rare | 15% | ~4-5 | 80-120 |
| Legendary | 5% | ~1-2 | 125-175 |

### Shrine Appearance Rules

- Each individual shrine type appears **at most twice** per run
- The player sees the shrine name, effect, and cost before paying
- Shrines persist in the floor cache — backtracking to buy later is valid
  but costs time (hunger clock advances)
- Once purchased, the shrine is consumed

### Shrine Catalog

#### Common Shrines (30-50 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Vitality** | 40 | +5 max HP | Stat |
| **Fortitude** | 40 | +1 armor | Stat |
| **Sight** | 30 | +1 vision range | Stat |
| **Arcana** | 40 | +10 max mana | Stat |
| **Mana Well** | 40 | Restore full mana when entering a new floor | Resource |
| **Thornmail** | 50 | Melee attackers take 1 damage | Defensive |
| **Alacrity** | 50 | -0.05x action delay (permanent) | Stat |
| **Regeneration** | 50 | +1 HP regen per turn | Defensive |

#### Uncommon Shrines (60-80 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Brutality** | 70 | +2 damage bonus | Melee |
| **Riposte** | 70 | On enemy melee miss, counterattack for 50% damage | Melee |
| **Lunge** | 60 | Melee attack range extended to 2 tiles | Melee |
| **Charge** | 60 | Double damage after 3+ tile straight-line move into melee | Melee |
| **Spell Leech** | 70 | Kill with spell → restore 30% of spell's mana cost | Caster |
| **Scavenger** | 60 | Items found have +1 rarity tier | Economy |
| **Steady Hand** | 70 | +2 ranged damage bonus | Ranged |
| **Shield Bash** | 70 | Melee attacks have 20% stun chance (requires shield) | Melee |

#### Rare Shrines (80-120 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Cleave** | 100 | Melee attacks hit all adjacent enemies | Melee |
| **Overwatch** | 100 | Ranged attacks fire twice if you didn't move last turn | Ranged |
| **Phase Step** | 100 | Walk through 1 wall tile, every 8 turns | Mobility |
| **Blink Step** | 100 | Teleport up to 3 tiles, every 6 turns | Mobility |
| **Essence Siphon** | 100 | +5 essence per monster killed | Economy |
| **Quick Cast** | 100 | Casting a spell doesn't end your turn | Caster |
| **Vampiric Touch** | 100 | Melee attacks heal 15% of damage dealt | Sustain |
| **Absorb** | 90 | 25% of damage taken is converted to mana instead | Caster |

#### Legendary Shrines (125-175 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Death's Door** | 175 | First lethal hit per floor: survive at 1 HP, invulnerable 2 turns | Defensive |
| **Spell Echo** | 150 | 25% chance any spell fires twice (no extra mana) | Caster |
| **Blood Mage** | 175 | Spells cost HP instead of mana (1 HP per 2 mana cost) | Caster |
| **Necromancer** | 150 | 30% chance killed enemies rise as allied skeletons (1 turn delay) | Summon |
| **Berserker** | 150 | +50% melee damage, -3 armor, cannot cast spells | Melee |
| **Ironclad** | 150 | +5 armor, movement speed halved | Defensive |

**Total: 30 shrine types** across 4 rarities.

### Essence Economy

**Essence drop formula:** `essence = monster's base_hp`. Simple, scales naturally,
instantly debuggable. A Giant Rat (5 HP) drops 5 essence. A Cave Troll (28 HP)
drops 28. A Dragon Whelp (24 HP) drops 24. Out-of-depth kills award 2x essence.

**Income estimates:**
- Floors 1-3: ~15-25 kills/floor, avg 5-8 HP = ~75-200 essence/floor
- Floors 4-6: ~15-20 kills, avg 10-15 HP = ~150-300 essence/floor
- Floors 7-9: ~12-18 kills, avg 15-20 HP = ~180-360 essence/floor
- **Run total: ~1,500-3,000 essence**

**Spending:**
- Shrines: player sees ~30, can afford ~4-8 across the run
- A player buying mostly commons/uncommons spends ~500-800 on shrines
- A player saving for 2 legendaries + some uncommons spends ~500-600
- Remaining essence has no other sink — intentional, prevents "buy everything"
  pressure

**Backtracking economics:** Returning 4 floors for a Legendary shrine costs
~6,000-12,000 game time. At 50,000 for Stage 3, that's 12-24% of the clock.
Worth it for a build-defining shrine found early. Not worth it for Vitality.

### Example Builds

**The Blender (Melee AoE)**
- Cleave (100) + Brutality (70) + Charge (60) + Vitality (40) = 270 essence
- Run into packs, hit everything at once. Charge into the first enemy, Cleave
  damages the rest.

**The Turret (Ranged)**
- Overwatch (100) + Steady Hand (70) + Fortitude (40) + Sight (30) = 240 essence
- Find a corridor, don't move, fire twice per turn at +2 damage.

**The Blood Caster**
- Blood Mage (175) + Vampiric Touch (100) + Spell Leech (70) = 345 essence
- Cast spells using HP, recover HP by meleeing between casts. Never worry
  about mana.

**The Cockroach (Survival)**
- Death's Door (175) + Regeneration (50) + Ironclad (150) = 375 essence
- Almost impossible to kill. Regen + high armor + death prevention. Slow but
  inevitable. Swarm Aspect is a nightmare.

---

## System Interactions

### Shrines + Equipment + Spells

The three systems complement each other:
- **Shrines** define playstyle (how you fight)
- **Spellbooks** provide tools (what you can cast)
- **Equipment** provides raw stats (damage/defense numbers)

A player doesn't need all three to be strong, but combining them creates
synergy. Vampiric Touch + a high-damage weapon + Enrage spell = a sustain
melee monster. Quick Cast + Fireball spellbook + Absorb shrine = a caster
who converts incoming damage to more spell fuel.

### Shrines + Hunger Clock

| Decision | Time Cost | Worth It? |
|----------|----------|-----------|
| Backtrack 2 floors for a Legendary | ~3,000-6,000 | Usually yes |
| Backtrack 6 floors for a Legendary | ~9,000-18,000 | Risky — might push to Stage 3 |
| Backtrack 2 floors for a Common | ~3,000-6,000 | Rarely worth the time |
| Rush forward, skip shrine alcoves | 0 | Faster boss, weaker player |

The optimal play is between "explore everything" and "rush the boss." Shrines
in remote locations reward exploration, but the clock punishes excessive
thoroughness.

---

## Save/Load

**New persistent state to save:**
- `TyrantAspects`: which 3 Aspects were rolled, current growth stage of each
- `PlayerShrines`: list of shrine effects the player has purchased
- `GameTime`: current hunger clock value

---

## Resolved Decisions

- **Tyrant base stats** — 120 HP, 2d8+4 damage, 4 hit, 2 dodge, 3 armor,
  0.9x delay, 0 resistances. Aspects layer on top.
- **Essence formula** — Monsters drop essence equal to their base_hp. Simple
  and scales naturally. Out-of-depth kills award 2x.
- **Spell shrines** — Share the 3-per-floor budget with stat shrines. Not
  listed in the shrine catalog (their content comes from the spell list).

## Open Questions

1. **Shrine stacking** — Can two Vitality shrines stack to +10 HP? Current
   rule says max 2 of each type per run, stacking allowed.
2. **Legendary conflicts** — Berserker + Blood Mage is contradictory (can't
   cast spells + spells cost HP). Mutually exclusive, or let the player waste
   essence?
3. **Corruption Site design** — Deferred. Will revisit once core machine system
   and Aspects are implemented.
4. **What if all 3 Aspects are weakened?** — Should there be a minimum
   difficulty floor for the Tyrant?
