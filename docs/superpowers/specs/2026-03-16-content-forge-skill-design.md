# Content Forge Skill — Design Spec

## Overview

A Claude Code skill for brainstorming, balancing, and producing new game content (monsters, items, spells, factions) for The Veiled Tyrant. The skill guides collaborative design from initial concept through balanced stat assignment to ready-to-use RON data entries written directly into the game's asset files.

## Design Goals

- **Fantasy-first creation** — Always start with "what does encountering/finding/casting this feel like?" before touching numbers.
- **Balance-aware** — Every stat, bonus, and cost is validated against floor-by-floor power curves derived from the existing game data.
- **Catalog-aware** — Never design something redundant. Always read existing content and identify where the new entry fits.
- **Valid output** — Generated RON uses only implemented types, abilities, bonuses, and spell effects from the Rust source.
- **End-to-end** — From concept brainstorm to approved RON appended to data files in one conversation.

## Skill Structure

```
.claude/skills/content-forge/
  SKILL.md                              # Workflows + process
  references/
    balance-curves.md                   # Floor-by-floor power targets, stat formulas, damage budgets
    ron-schemas.md                      # RON format for monsters, items, spells, spawns
    faction-design-guide.md             # How to build a cohesive faction roster
```

### Skill Description (for discovery)

> Use when the user asks to "create a monster", "design an item", "brainstorm a spell", "generate a faction", "what monsters are missing", "fill gaps in the bestiary", "add a new enemy type", "design loot", or wants to brainstorm, balance, and produce new game content (monsters, items, spells, factions) for The Veiled Tyrant.

## Workflow 1: Create Monster

A guided conversation through 7 dimensions, then RON output.

### Step 1: Read Current State

Read `assets/monsters.ron` and `assets/monster_spawns.ron`. Note existing monsters by faction, floor range, and role coverage. Identify where the new monster fits or what gap it fills.

### Step 2: Monster Fantasy

Ask what this creature feels like to encounter. If the user wants inspiration, suggest archetypes:

- **Brute** — Slow, hits hard, soaks damage. Player must kite or burst.
- **Swarm fodder** — Weak alone, dangerous in groups. Tests AoE and positioning.
- **Glass cannon** — High damage, low HP. Priority target in mixed groups.
- **Caster** — Spell-based threat. Forces the player to close distance or interrupt.
- **Tank** — High armor/resistances, blocks corridors. Stalls while allies deal damage.
- **Skirmisher** — Fast, repositions. Frustrating to pin down.
- **Ambusher** — Appears suddenly or from stealth. Punishes inattention.

### Step 3: Stat Assignment

Using the balance curves reference, propose stats based on target floor range:

- **Base HP**: Scale from ~10 (floor 1) to ~120 (floor 20), following the curve: `base_hp ≈ floor * 6` (±30% for role — brutes high, glass cannons low)
- **Attributes**: Start from 10 baseline, adjust ±1 per 3 floors from baseline, ±4 max for role emphasis
- **Damage dice**: Scale so monsters deal 15-25% of expected player HP per hit in early game, 30-50% in late game
- **Defense/armor**: 0-2 early, 3-5 mid, 5-8 late, 7-12 endgame
- **Speed delay**: 1.0 default, faster for skirmishers (0.8), slower for brutes (1.3)
- **Experience reward**: Roughly `floor * 12 + difficulty_modifier`

Present the stat block with reasoning for each value.

### Step 4: Abilities & Resistances

Propose 0-3 special abilities from the valid set (on-hit effects, passives like PoisonBody/ExplodeOnDeath/Reanimate, resistances). Each ability must reinforce the monster fantasy — not tacked on for complexity.

Validate that abilities use only implemented ability types from `abilities.rs`.

### Step 5: Squad Role & Behavior

Recommend the monster's role in group encounters: `melee_guard`, `ranged`, `brute`, `caster`, `leader`, or `any`. Propose whether it should typically be a squad leader or follower, and suggest spawn group sizes (min/max).

### Step 6: Spawn Configuration

Propose floor range, spawn weight, and group sizes for `monster_spawns.ron`. Cross-reference existing spawns to avoid overcrowding floor ranges.

### Step 7: Approve & Write

Present the complete design summary, then the RON entries for both `monsters.ron` and `monster_spawns.ron`. On approval, append to both files.

## Workflow 2: Create Item

### Step 1: Read Current State

Read `assets/items.ron` and `assets/item_spawns.ron`. Note existing items by kind, rarity, and bonus coverage.

### Step 2: Item Fantasy

Ask what finding this item should feel like. Key questions:

- **Kind**: Weapon, Armor (which slot?), Ring, Amulet, Consumable, Spellbook?
- **Rarity**: Common (0 bonuses), Uncommon (1), Rare (2), Legendary (3)?
- **Identity**: What makes this item memorable? A Legendary should be run-defining.

### Step 3: Core Stats

Based on kind and rarity, propose:

- **Weapons**: Damage dice, weapon range (1 for melee, 4-8 for ranged)
- **Armor**: Defense value scaled by slot and rarity
- **Consumables**: Effect type and magnitude (heal amount, stat boost value)

### Step 4: Bonus Selection

For Uncommon+, propose bonuses from the valid set (28+ types). Prioritize thematic coherence — a fire sword should have OnHitBurn, not OnHitSlow. Validate bonus values against existing items at the same rarity tier.

### Step 5: Spawn Configuration

Propose floor range and rarity weight for `item_spawns.ron`. Ensure the item appears at floors where its power level is appropriate.

### Step 6: Approve & Write

Present design summary + RON entries. On approval, append to `items.ron` and `item_spawns.ron`.

## Workflow 3: Create Spell

### Step 1: Read Current State

Read `assets/spells.ron`. Note existing spells by damage type, targeting mode, and role (attack/heal/buff/debuff/utility/summon).

### Step 2: Spell Fantasy

Ask what casting this spell should feel like:

- **Role**: Damage, healing, buff, debuff, control, utility, summon?
- **Targeting**: Self, single enemy, single ally, AoE tile, chain?
- **Frequency**: Cantrip (cheap, spammable) or big spell (expensive, impactful)?

### Step 3: Effect Design

Propose spell effects from the valid set. For damage/healing, propose dice and scaling. For status effects, propose duration. Multiple effects are fine for higher-cost spells.

### Step 4: Cost & Cooldown Balance

- **Cantrip** (3-8 mana, 0-4 CD): 1d4-1d6 damage, minor utility
- **Standard** (10-20 mana, 5-12 CD): 2d4-2d8 damage, meaningful effect
- **Powerhouse** (25-35 mana, 15-25 CD): 3d6-4d6 damage, encounter-changing

Cross-reference existing spells at similar power levels.

### Step 5: Monster Access

Ask whether any monsters should also cast this spell. If yes, note which monster definitions would need updating.

### Step 6: Approve & Write

Present design summary + RON entry. On approval, append to `spells.ron`. If a spellbook item is needed, chain into Create Item workflow.

## Workflow 4: Generate Faction

The composite workflow. Designs a cohesive faction as a unit.

### Step 1: Read Current State

Read all data files (`monsters.ron`, `items.ron`, `spells.ron`, and spawn files). Note existing factions, their floor ranges, and roster composition.

### Step 2: Faction Identity

Guided questions:

- **Theme**: What unifies this faction visually and mechanically? (e.g., "fungal creatures that spread spores")
- **Floor range**: Where in the 26-floor dungeon does this faction appear?
- **Personality**: How do they fight as a group? (aggressive, defensive, tricky, swarming)

### Step 3: Roster Design

Propose 4-6 monsters covering the standard roles, with power tiers that span the faction's floor range:

- **Fodder** (1-2): Weak, appears in groups. Bottom of floor range.
- **Standard** (1-2): Core encounter unit. Middle of floor range.
- **Elite** (1): Dangerous solo or as squad leader. Upper floor range.
- **Boss candidate** (0-1): Could anchor a boss fight. Top of range.

Each monster gets a brief concept + how it synergizes with the others.

### Step 4: Faction Abilities

Design 2-4 signature abilities or mechanics that make this faction feel distinct. These might be shared across the roster (e.g., all fungal creatures apply Poison) or distributed (fodder spreads spores, elite detonates them).

### Step 5: Themed Loot

Propose 2-4 faction-themed items that could drop from these monsters or appear in faction prefabs:

- A weapon that plays into the faction's mechanic
- An armor piece or accessory with thematic bonuses
- A consumable or spellbook if appropriate

### Step 6: Faction Spells

If the faction has casters, design their spells. These might also be learnable by the player via spellbooks.

### Step 7: Sequential Approval & Write

Walk through each content piece (monsters → spells → items → spawn configs) one at a time. Approve each before writing. This prevents a massive all-or-nothing commit.

## Workflow 5: Audit Gaps

### Step 1: Read All Data Files

Parse `monsters.ron`, `items.ron`, `spells.ron`, and all spawn files.

### Step 2: Analyze Coverage

Check for gaps across:

- Floor ranges with low monster variety
- Underrepresented damage types (items, spells, resistances)
- Missing item kinds at certain rarities
- Spell roles with few options (e.g., few debuff spells, no AoE healing)
- Factions with incomplete role coverage

### Step 3: Present Prioritized Recommendations

Rank gaps by impact on gameplay variety. For each, suggest a brief concept.

### Step 4: Optionally Chain

If the user picks a gap, flow into the appropriate creation workflow.

## Reference Documents

### `references/balance-curves.md`

Floor-by-floor power targets used to assign balanced stats.

**Player Power by Floor** (expected values assuming moderate Essence investment):

| Floor | Player HP | Player DPS | Player Armor | Player Mana |
|-------|-----------|------------|--------------|-------------|
| 1     | 25-28     | 3-5        | 0-1          | 50          |
| 5     | 30-35     | 5-8        | 2-4          | 55-65       |
| 10    | 38-45     | 8-12       | 4-6          | 65-80       |
| 15    | 45-55     | 12-16      | 6-8          | 80-100      |
| 20    | 55-70     | 16-22      | 8-12         | 100-120     |

**Monster Stat Budgets** (target ranges by floor):

- **HP formula**: `base_hp ≈ floor * 6` (±30% for role)
- **Damage target**: 15-25% of expected player HP per hit (early), 30-50% (late)
- **Attribute baseline**: 10 all, ±1 per 3 floors, ±4 max for role emphasis
- **Armor**: 0-2 (floors 1-5), 3-5 (6-12), 5-8 (13-18), 7-12 (19-26)
- **Experience reward**: `floor * 12 + role_modifier` (brute +15, fodder -10, caster +10)

**Item Power Budgets** by rarity:

- **Common**: Raw stats only, no bonuses. Weapon: 1d4-1d6. Armor: 1-2.
- **Uncommon**: 1 bonus (8-12% range). Weapon: 1d6-1d8. Armor: 2-3.
- **Rare**: 2 bonuses (12-18% range). Weapon: 1d8-2d6. Armor: 3-5.
- **Legendary**: 3 bonuses (15-25% range). Weapon: 2d6-2d8. Armor: 5-7. Unique identity.

**Spell Power Budgets**:

- **Cantrip** (3-8 mana, 0-4 CD): 1d4-1d6 damage, minor utility
- **Standard** (10-20 mana, 5-12 CD): 2d4-2d8 damage, meaningful effect
- **Powerhouse** (25-35 mana, 15-25 CD): 3d6-4d6 damage, encounter-changing

**Spawn Density Guidelines**:

- Early floors (1-5): 8-12 monsters, 2-4 items
- Mid floors (6-15): 12-18 monsters, 3-5 items
- Late floors (16-26): 15-22 monsters, 4-6 items

### `references/ron-schemas.md`

Annotated RON format for every data file the skill writes to:

- `MonsterDef` schema with all fields, types, and valid enum values
- `MonsterSpawnEntry` schema with floor range, group sizes, squad config
- `ItemDef` schema with all item kinds, armor slots, bonus types
- `ItemSpawnEntry` schema with rarity weights and floor ranges
- `SpellData` schema with effect types, targeting modes, damage types
- Valid enum values for: `FactionKind`, `MonsterRole`, `DamageType`, `ResistanceLevel`, `ItemBonus`, `SpellEffect`, `SpellTarget`, `OnLeaderDeath`
- Annotated example entries for each type

### `references/faction-design-guide.md`

Guidance for the Generate Faction workflow:

- **Roster template**: Every faction needs fodder (1-2), standard (1-2), elite (1), optional boss candidate
- **Mechanical identity**: Pick 1-2 signature mechanics that unify the faction
- **Role synergies**: How roles interact within squads
- **Ability distribution**: Shared traits vs. specialist abilities
- **Themed loot principles**: Items should reflect faction identity
- **Floor range sizing**: A faction should span 6-10 floors
- **Existing faction analysis**: Summary of how current factions are built, as exemplars

## Cross-Cutting Rules

1. **Always read current state first** — Never propose content without knowing what exists.
2. **One question at a time** — Walk through dimensions sequentially.
3. **Validate against implemented types** — Only use ability types, bonus types, spell effects, etc. that exist in the Rust source. No inventing new mechanics.
4. **Present before writing** — Always show the complete design summary and RON before modifying files.
5. **Append, don't overwrite** — New entries are appended to existing RON files, never replacing content.
6. **Chain when appropriate** — Creating a monster that casts spells → chain into Create Spell. Creating faction loot → chain into Create Item. Creating a spell the player can learn → chain into Create Item for the spellbook.

## Implementation Notes

- This is a pure skill (`.claude/skills/` files only) — no Rust code changes needed.
- The skill reads `assets/monsters.ron`, `assets/items.ron`, `assets/spells.ron`, `assets/monster_spawns.ron`, `assets/item_spawns.ron`, `assets/props.ron`, and `assets/structures.ron` at runtime.
- Reference docs (`balance-curves.md`, `ron-schemas.md`, `faction-design-guide.md`) should be updated when balance changes ship or RON schemas change.
- The skill lives at `.claude/skills/content-forge/` alongside the existing `game-mechanics-designer/` and `prefab-designer/` skills.
- The `ron-schemas.md` reference must be populated from the actual current RON files and Rust enum definitions to ensure validity.
