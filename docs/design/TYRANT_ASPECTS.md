# Tyrant Aspects & Shrine System

## Player Experience

You descend into the dungeon knowing the Veiled Tyrant waits on floor 20. What you
don't know is *how* it will fight. Each run, the Tyrant is investing stolen essence
into three random domains of power — its Aspects. These Aspects grow stronger over
time, changing the boss fight from a stat check into a puzzle you solve across the
entire run.

As you explore, you find **Shrines** in remote corners of each floor — permanent
upgrades that shape your build. Some grant raw stats. Others fundamentally change how
you fight. You also encounter **Corruption Sites** where the Tyrant's power has
crystallized. An Aspect Champion guards each site. Beat it, and you can weaken that
Aspect on the Tyrant — or walk away with loot and leave the Aspect at full power.

Every run, you're answering two questions:
1. **Who am I becoming?** (Shrines — your build)
2. **What is the Tyrant becoming?** (Aspects — the boss's build)

---

## System 1: Tyrant Aspects

### Overview

At run start, 3 Aspects are randomly selected from a pool of 10. Each Aspect has
3 growth stages that advance on a hunger clock tied to game time. The Aspects
determine what abilities, resistances, and spells the Tyrant has during the floor 20
boss fight.

### Hunger Clock

All 3 Aspects advance on the same global timer:

| Stage | Game Time Threshold | Effect |
|-------|-------------------|--------|
| Stage 0 | 0 (start) | Aspects dormant — no effect on Tyrant |
| Stage 1 | 25,000 | Basic ability per Aspect |
| Stage 2 | 60,000 | Stronger ability, may add resistance |
| Stage 3 | 100,000 | Full power — immunity, strong abilities |
| Beyond | Every 50,000 after Stage 3 | Flat stat boosts: +15 HP, +1 armor per tick |

**Time reference:** Each player action costs 100 time units (BASE_ACTION_COST). A
typical floor takes 300-500 actions = 30,000-50,000 time. A 20-floor run at moderate
pace takes ~100,000-120,000 time, reaching Stage 3 near the end.

**Backtracking cost:** Traveling back up floors to revisit shrines or corruption
sites costs time. Each floor traversed costs ~1,500-3,000 time (navigating to
stairs + descending/ascending). A 4-floor backtrack (~6,000-12,000 time) could push
an Aspect from Stage 2 to Stage 3. The player must weigh "is this shrine worth the
time?" against the boss growing stronger.

### Whisper Messages

The game log delivers atmospheric hints as Aspects advance:

| Event | Message |
|-------|---------|
| Stage 1 triggers | "You feel a dark power stirring in the depths below..." |
| Stage 2 triggers | "The dungeon trembles. The Tyrant grows stronger." |
| Stage 3 triggers | "Reality shudders. The Tyrant has become a force of nature." |
| Beyond ticks | "The Tyrant's power continues to grow without bound..." |

These are non-specific — they don't reveal which Aspects are active. The player
must find Corruption Sites for that information.

### Aspect Pool (10 total, 3 selected per run)

#### Flame

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts fire_dart |
| 2 | + Casts fireball, fire resistant |
| 3 | + Fire immune, BurningStrike 40% on melee |

Champion: **Ember Knight** — fire immune, BurningStrike, high armor.
Champion Drop: **Fireward Ring** — grants fire resistant to wearer.

#### Shadow

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts shadow_bolt |
| 2 | + Life Drain 15% on melee, necrotic resistant |
| 3 | + Life Drain 30%, necrotic immune, teleports when below 30% HP |

Champion: **Shade Stalker** — Life Drain, Terrify aura, necrotic damage.
Champion Drop: **Shadowbane Blade** — weapon deals necrotic damage, bonus vs necrotic-immune.

#### Iron

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | +2 armor |
| 2 | + +4 armor total, Rough Body 2 |
| 3 | + +6 armor total, Rough Body 3, physical resistant |

Champion: **Iron Golem** — Rough Body 5, physical resistant, massive HP.
Champion Drop: **Armorbreaker Mace** — attacks ignore 3 target armor.

#### Storm

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts spark |
| 2 | + Casts lightning_bolt, StunningBlow 15% |
| 3 | + StunningBlow 30%, Knockback 2 |

Champion: **Storm Caller** — StunningBlow 50%, casts lightning_bolt.
Champion Drop: **Grounding Amulet** — immune to stun.

#### Blood

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | +15 HP, regen 3/turn |
| 2 | + +30 HP total, regen 6/turn, Enrage at 40% HP |
| 3 | + +45 HP total, regen 8/turn, Enrage at 60% HP |

Champion: **Blood Berserker** — Enrage at 80%, regen 5/turn, very high damage.
Champion Drop: **Bloodletter** — weapon heals 10% of damage dealt.

#### Mind

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Casts slow |
| 2 | + Casts mana_drain, +25 mana |
| 3 | + Spirit Shield, +50 mana, SlowStrike 30% on melee |

Champion: **Thought Eater** — casts mana_drain + slow, Spirit Shield.
Champion Drop: **Mindguard Helm** — immune to mana drain.

#### Bone

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | +2 armor, physical resistant |
| 2 | + Casts raise_dead every 12 turns |
| 3 | + Casts raise_dead every 8 turns, +4 armor total |

Champion: **Bone Lord** — casts raise_dead, physical resistant, high armor.
Champion Drop: **Bonecrusher** — melee attacks deal double damage to summoned creatures.

#### Swarm

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Summons 1 faction monster every 10 turns |
| 2 | + Summons 2 every 8 turns |
| 3 | + Summons 2 every 5 turns |

Champion: **Hive Queen** — summons 2 monsters every 3 turns, low personal stats.
Champion Drop: **Cleaving Axe** — melee hits all adjacent enemies.

#### Paralysis

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | SlowStrike 20% on melee |
| 2 | + StunningBlow 20%, SlowStrike 30% |
| 3 | + StunningBlow 40%, SlowStrike 40%, Knockback 2 |

Champion: **Petrified Guardian** — StunningBlow 50%, SlowStrike, massive armor.
Champion Drop: **Swift Boots** — immune to slow.

#### Void

| Stage | Tyrant Gains |
|-------|-------------|
| 1 | Teleports every 8 turns |
| 2 | + Teleports every 5 turns, Spirit Shield |
| 3 | + Teleports every 3 turns, Spirit Shield, phase walk |

Champion: **Void Walker** — teleports every 3 turns, Spirit Shield.
Champion Drop: **Anchor Stone** — prevents enemy teleport within 3 tiles.

### Corruption Sites

3 Corruption Sites spawn per run, one per Aspect:

| Site | Floor Range | Contains |
|------|------------|----------|
| Site 1 | Random floor 6-9 | Aspect 1 Champion + Altar |
| Site 2 | Random floor 10-13 | Aspect 2 Champion + Altar |
| Site 3 | Random floor 14-17 | Aspect 3 Champion + Altar |

Each site is a special prefab room. The Aspect Champion guards a Corruption Altar.
The encounter is fully optional — the player can see it from the doorway and leave.

**On defeating the Champion:**
- Gain 50 essence
- Champion drops its themed loot item
- The Corruption Altar becomes interactable

**At the Altar, the player chooses:**
- **Destroy** — This Aspect is capped at Stage 1 regardless of time. If it hasn't
  reached Stage 1 yet, it never will.
- **Leave** — The Aspect continues growing normally. Walk away with just the loot
  and essence from the Champion kill.

**If the player skips the site entirely:** No intel, no loot, no weakening. The
Aspect grows to whatever stage the clock dictates.

**Backtracking to Corruption Sites:** Since floors persist in the floor cache, a
player can return to a site on a previous floor. However, the Champion must be
defeated on the first visit (it doesn't respawn). If the player fled the Champion,
they can return to try again. If the Champion is dead and the Altar unused, they
can return to make their choice.

### Combination Analysis (Replayability)

3 from 10 Aspects = 120 unique combinations. Sample runs that feel very different:

| Run | Aspects | Boss Fight Character | Player Prep |
|-----|---------|---------------------|-------------|
| A | Flame + Iron + Blood | Tanky regen bruiser who burns you | Need armor pen + fire resist + burst damage |
| B | Shadow + Mind + Void | Teleporting mana-draining life stealer | Need necrotic resist + mana protection + mobility |
| C | Storm + Paralysis + Swarm | Stun-locking summoner | Need stun immunity + AoE + patience |
| D | Bone + Blood + Iron | Unkillable wall that summons skeletons | Need sustained DPS + summon management |

No dominant strategy works across all combinations.

---

## System 2: Shrines

### Overview

Shrines are permanent upgrade stations found in remote locations throughout the
dungeon. 3 shrines spawn per floor in out-of-the-way alcoves, rewarding exploration.
Each shrine costs essence and grants a permanent upgrade for the rest of the run.

Shrines are the primary build system — how the player defines their character's
identity. Combined with spellbook pickups and equipment, shrines determine playstyle.

### Shrine Rarity

Each of the 3 shrine slots per floor rolls independently:

| Rarity | Chance | Avg per Run (60 slots) | Essence Cost |
|--------|--------|----------------------|-------------|
| Common | 50% | ~30 | 30-50 |
| Uncommon | 30% | ~18 | 60-80 |
| Rare | 15% | ~9 | 80-120 |
| Legendary | 5% | ~3 | 125-175 |

Any shrine can appear on any floor. Finding a Legendary on floor 2 creates a
meaningful "can I afford this / should I backtrack for it later?" decision.

### Shrine Appearance Rules

- Each individual shrine type appears **at most twice** per run
- The player sees the shrine name, effect, and cost before paying
- Shrines persist in the floor cache — backtracking to buy one later is valid
  but costs time (hunger clock advances)
- Once purchased, a shrine is consumed (the alcove becomes empty)

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
| **Alacrity** | 50 | +5% action speed (permanent) | Stat |
| **Regeneration** | 50 | Regen 1 HP/turn (or +1 if already have regen) | Defensive |

#### Uncommon Shrines (60-80 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Brutality** | 70 | Melee attacks deal +2 flat damage | Melee |
| **Riposte** | 70 | On enemy melee miss, auto-counterattack for 50% damage | Melee |
| **Lunge** | 60 | Melee attack range extended to 2 tiles | Melee |
| **Charge** | 60 | Double damage after 3+ tile straight-line move into melee | Melee |
| **Spell Leech** | 70 | Kill with spell → restore 30% of spell's mana cost | Caster |
| **Scavenger** | 60 | Items found on the ground have +1 rarity tier | Economy |
| **Steady Hand** | 70 | Ranged attacks deal +2 flat damage | Ranged |
| **Shield Bash** | 70 | Melee attacks have 20% stun chance (requires armor equipped) | Melee |

#### Rare Shrines (80-120 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Cleave** | 100 | Melee attacks hit all adjacent enemies | Melee |
| **Overwatch** | 100 | Ranged attacks fire twice while you didn't move last turn | Ranged |
| **Phase Step** | 100 | Walk through 1 wall tile, every 8 turns | Mobility |
| **Blink Step** | 100 | Teleport up to 3 tiles, every 6 turns | Mobility |
| **Essence Siphon** | 100 | +5 essence per monster killed | Economy |
| **Quick Cast** | 100 | Casting a spell doesn't end your turn | Caster |
| **Vampiric Touch** | 100 | Melee attacks heal you for 15% of damage dealt | Sustain |
| **Absorb** | 90 | 25% of damage you take is converted to mana instead | Caster |

#### Legendary Shrines (125-175 essence)

| Shrine | Cost | Effect | Category |
|--------|------|--------|----------|
| **Death's Door** | 175 | First lethal hit per floor: survive at 1 HP, invulnerable 2 turns | Defensive |
| **Spell Echo** | 150 | 25% chance any spell fires twice (no extra mana cost) | Caster |
| **Blood Mage** | 175 | Spells cost HP instead of mana (1 HP per 2 mana cost) | Caster |
| **Necromancer** | 150 | 30% chance killed enemies rise as allied skeletons (1 turn delay) | Summon |
| **Berserker** | 150 | +50% melee damage, -3 armor, cannot cast spells | Melee |
| **Ironclad** | 150 | +5 armor, movement speed halved | Defensive |

**Total: 30 shrine types** across 4 rarities.

### Essence Economy

**Income:** ~200-400 essence per floor from monster kills = ~4,000-8,000 total
across 20 floors. Champion kills add 150 total (50 each × 3 sites).

**Spending:**
- Shrines: player sees ~60, can afford ~10-15 across the run
- A player buying mostly commons/uncommons spends ~500-800 on shrines
- A player saving for 2 legendaries + some uncommons spends ~500-600
- Remaining essence has no other sink — this is intentional, it prevents
  "I need to buy everything" pressure

**Backtracking economics:** Returning 4 floors to buy a Legendary shrine
costs ~6,000-12,000 game time. At 100,000 for Stage 3, this is 6-12% of the
total clock. Worth it for a build-defining shrine found early. Not worth it
for a Vitality shrine.

### Example Builds

**The Blender (Melee AoE)**
- Cleave (100) + Brutality (70) + Charge (60) + Vitality (40) = 270 essence
- Identity: Run into packs, hit everything at once. Charge into the first enemy,
  Cleave damages the rest. Brutality adds +2 to every target hit.
- Weakness: Ranged enemies, bosses with few adds.

**The Turret (Ranged)**
- Overwatch (100) + Steady Hand (70) + Fortitude (40) + Sight (30) = 240 essence
- Identity: Find a corridor, don't move, fire twice per turn at +2 damage. See
  enemies from far away.
- Weakness: Being flanked, enemies that close distance fast.

**The Blood Caster**
- Blood Mage (175) + Vampiric Touch (100) + Spell Leech (70) = 345 essence
- Identity: Cast spells using HP, recover HP by meleeing between casts. High risk,
  high reward cycle. Never worry about mana.
- Weakness: Getting stunned while low HP, no mana for Spirit Shield.

**The Cockroach (Survival)**
- Death's Door (175) + Regeneration (50) + Ironclad (150) = 375 essence
- Identity: Almost impossible to kill. Regen + high armor + death prevention.
  Slow but inevitable. Wins by attrition.
- Weakness: Very slow, clock is the real enemy. Swarm Aspect is a nightmare.

---

## System Interactions

### Shrines + Corruption Sites

Shrines build the player UP. Corruption Sites tear the boss DOWN. The player
allocates essence between the two based on what they've learned about the Aspects
and what their build needs.

A melee build might:
- Buy Cleave + Brutality (essential for their playstyle)
- Destroy the Paralysis Aspect (stun-lock would kill them)
- Leave the Iron Aspect alone (they have armor penetration from loot)

A caster build might:
- Buy Quick Cast + Spell Leech (essential for spell economy)
- Destroy the Mind Aspect (mana drain would cripple them)
- Leave the Blood Aspect alone (they can kite and burst)

### Shrines + Spellbooks + Items

The three systems complement each other:
- **Shrines** define playstyle (how you fight)
- **Spellbooks** provide tools (what you can cast)
- **Items** provide raw stats (damage/defense numbers)

A player doesn't need all three to be strong, but combining them creates
synergy. Vampiric Touch + a high-damage weapon + the Enrage spell = a
sustain melee monster. Quick Cast + fireball spellbook + Absorb shrine =
a caster who converts incoming damage to more spell fuel.

### Shrines + Hunger Clock

Backtracking for shrines costs time. The hunger clock creates pressure:

| Decision | Time Cost | Worth It? |
|----------|----------|-----------|
| Backtrack 2 floors for a Legendary | ~3,000-6,000 | Usually yes |
| Backtrack 6 floors for a Legendary | ~9,000-18,000 | Risky — might push to Stage 3 |
| Backtrack 2 floors for a Common | ~3,000-6,000 | Rarely worth the time |
| Rush forward, skip shrine alcoves | 0 | Faster boss, weaker player |

The optimal play is somewhere between "explore everything" and "rush the boss."
Shrines in remote locations reward exploration, but the clock punishes excessive
thoroughness.

### Save/Load

**New persistent state to save:**
- `TyrantAspects`: which 3 Aspects were rolled, current growth stage of each,
  which have been weakened via Corruption Site destruction
- `PlayerShrines`: list of shrine effects the player has purchased
- `CorruptionSiteState`: per-site status (undiscovered / champion alive /
  champion dead + altar unused / altar destroyed / altar left)
- Corruption Sites must persist in the floor cache (they're prefab rooms
  with state)

---

## Implementation Notes

### New Components Needed
- `TyrantAspects` resource: holds the 3 selected Aspects and their states
- `PlayerShrines` component or resource: list of purchased shrine effects
- `ShrineMarker` component: marks shrine entities in the world
- `CorruptionSite` component: marks corruption altar entities with Aspect ID
- `AspectChampion` component: marks champion entities with Aspect ID

### New Systems Needed
- Shrine interaction system (player walks onto shrine, UI prompt, essence deduction)
- Corruption Site interaction system (altar choice after Champion death)
- Aspect growth system (replaces current TyrantPower escalation)
- Boss ability application system (reads TyrantAspects, applies abilities at spawn)
- Champion spawner (places Champions in Corruption Site prefabs)
- Shrine spawner (places 3 shrines per floor in remote locations)

### Files to Modify
- `src/game/boss.rs` — replace TyrantPower with TyrantAspects
- `src/game/spawner.rs` — shrine and champion spawning
- `src/save/mod.rs` — persist new state
- `src/map/builders/` — new shrine placement and corruption site prefab logic
- `assets/monsters.ron` — add Champion monster definitions
- `assets/items.ron` — add Champion drop items

### Prefabs Needed
- 3 Corruption Site room prefabs (different sizes/shapes)
- Shrine alcove prefab (small, 1-2 tile nook off a corridor)

---

## Open Questions

1. **Shrine stacking** — Can two Vitality shrines stack to +10 HP? Current rule
   says max 2 of each type per run, stacking allowed.

2. **Legendary limit** — Can the player take multiple Legendaries? Berserker +
   Blood Mage is contradictory (can't cast spells + spells cost HP). Should
   conflicting Legendaries be mutually exclusive, or just let the player waste
   essence on bad combos?

3. **Champion difficulty scaling** — Should Champions on floors 6-9 be easier
   than those on floors 14-17? Or all the same difficulty? Scaling makes sense
   (early Champions are beatable with less gear).

4. **Aspect reveal at Corruption Site** — Does the player learn which Aspect
   it is before or after fighting the Champion? Before (visible from doorway)
   lets the player decide if it's worth fighting. After (hidden until altar
   interaction) is more dramatic but might feel unfair.

5. **What if the player destroys all 3 Aspects?** The Tyrant falls back to
   base stats (120 HP, 2d8+4, 3 armor, 6 spells). Is that too easy? Could
   add a minimum difficulty floor.
