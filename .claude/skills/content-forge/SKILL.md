---
name: Content Forge
description: >
  Use when the user asks to "create a monster", "design an item",
  "brainstorm a spell", "generate a faction", "what monsters are missing",
  "fill gaps in the bestiary", "add a new enemy type", "design loot",
  or wants to brainstorm, balance, and produce new game content
  (monsters, items, spells, factions) for The Veiled Tyrant.
---

# Content Forge

Brainstorm, balance, and produce new game content for The Veiled Tyrant.
Guides collaborative design from initial concept through balanced stat
assignment to ready-to-use RON data entries written directly into the
game's asset files.

## Before Starting Any Workflow

Read the relevant reference files and live game data before proposing content.

**Always read:**
- `references/balance-curves.md` — Power targets and stat budgets
- `references/ron-schemas.md` — Valid RON formats and enum values

**Per workflow:**
| Workflow | Additional Files to Read |
|----------|-------------------------|
| Create Monster | `assets/monsters.ron`, `assets/monster_spawns.ron` |
| Create Item | `assets/items.ron`, `assets/item_spawns.ron` |
| Create Spell | `assets/spells.ron` |
| Generate Faction | All of the above + `references/faction-design-guide.md` + `docs/design/BESTIARY.md` |
| Audit Gaps | All of the above + `assets/essence_nodes.ron`, `assets/props.ron`, `assets/structures.ron` |

## Workflow 1: Create Monster

A guided conversation through 8 steps, then RON output.

### Step 1: Read Current State

Read `assets/monsters.ron` and `assets/monster_spawns.ron`. Note existing
monsters by faction, floor range, and role coverage. Identify where the
new monster fits or what gap it fills.

### Step 2: Monster Fantasy

Ask what this creature *feels like* to encounter. If the user wants
inspiration, suggest archetypes:

- **Brute** — Slow, hits hard, soaks damage. Player must kite or burst.
- **Swarm fodder** — Weak alone, dangerous in groups. Tests AoE and positioning.
- **Glass cannon** — High damage, low HP. Priority target in mixed groups.
- **Caster** — Spell-based threat. Forces the player to close distance or interrupt.
- **Tank** — High armor/resistances, blocks corridors. Stalls while allies deal damage.
- **Skirmisher** — Fast, repositions. Frustrating to pin down.
- **Ambusher** — Appears suddenly or from stealth. Punishes inattention.

### Step 3: Stat Assignment

Using `references/balance-curves.md`, propose stats based on target floor range:

- **Base HP**: `final_hp = base_hp + (CON_bonus * level)`. Set `base_hp`
  accounting for CON scaling. Example: level 10, CON 14 → +40 from CON,
  so `base_hp = 45` yields ~85 final HP.
- **Attributes**: Baseline 10. INT = 0 for non-casters. PER 6-14. Adjust
  ±1 per 3 floors, ±4 max for role emphasis.
- **Damage dice**: Scale so monsters deal 15-25% of player HP (early) to
  30-50% (late) per hit.
- **Damage type**: Physical, Fire, Lightning, or Necrotic. Ask what fits
  the monster's fantasy.
- **Armor**: 0-2 (floors 1-5), 3-5 (6-12), 5-8 (13-18), 7-12 (19-20).
- **AGI**: Determines delay via `1.0 - (AGI_bonus * 0.025)`. Fast: 14-18,
  Normal: 10, Slow: 4-6.
- **Experience**: `10 + (level * 5) + (base_hp / 2)`.

Present the stat block with reasoning for each value.

### Step 4: Abilities & Resistances

Propose 0-3 special abilities from the implemented set. Each must
reinforce the monster fantasy. Validate against `references/ron-schemas.md`:

- **On-hit effects**: ApplyPoison, ApplySlow, ApplyBurning, AttributeDrain,
  Knockback, LifeDrain, Disarm
- **Passives**: poison_body, thorn_aura, reanimate_hp, enrage_on_hit,
  explode_on_death, death_curse, summon_on_death
- **Resistances**: Map of damage_type → ResistanceLevel
- **Spells**: List of spell IDs from `assets/spells.ron` (casters only)

### Step 5: Squad Role & Behavior

Recommend the monster's role: `melee_guard`, `ranged`, `brute`, `caster`,
`leader`, or `any`.

Determine behavioral traits:
- **Cowardly** (`is_cowardly`): Should this monster flee when wounded?
- **On leader death**: `scatter`, `enrage`, or `""` (fight on)
- **Flee threshold**: 0.25-0.30 (brave), 0.35-0.45 (normal), 0.50+ (cowardly)

### Step 6: Spawn Configuration

Propose floor range, group sizes (min/max), and squad behaviors for
`monster_spawns.ron`. Support both simple (single monster type) and
composite (mixed group) formats. Cross-reference existing spawns to
avoid overcrowding floor ranges.

### Step 7: Sprite Assignment

Either:
- Assign an existing sprite from `assets/sprites/monsters/`
- Note that a placeholder sprite is needed (per project convention)

### Step 8: Approve & Write

Present the complete design summary, then the RON entries for both
`monsters.ron` and `monster_spawns.ron`. On approval, append to both files.

## Workflow 2: Create Item

### Step 1: Read Current State

Read `assets/items.ron` and `assets/item_spawns.ron`. Note existing items
by kind, rarity, and bonus coverage.

### Step 2: Item Fantasy

Ask what finding this item should feel like:

- **Kind**: Weapon, Armor (which slot?), Ring, Amulet, Consumable, Spellbook?
- **Rarity**: Common (0 bonuses), Uncommon (1), Rare (2), Legendary (3)?
- **Identity**: What makes this item memorable? A Legendary should be run-defining.

### Step 3: Core Stats

Based on kind and rarity, propose:

- **Weapons**: Damage dice (1d4-2d8), weapon_range (0=melee, 4-8=ranged)
- **Armor**: Defense value by slot and rarity, armor_slot required
- **Consumables**: Effect type and magnitude
- **Spellbooks**: LearnSpell effect with spell ID

### Step 4: Bonus Selection

For Uncommon+, propose bonuses from `references/ron-schemas.md`. Prioritize
thematic coherence — a fire sword should have OnHitBurn, not OnHitSlow.
Validate values against existing items at the same rarity tier.

### Step 5: Spawn Configuration

Propose floor range, rarity, and weight for `item_spawns.ron`. Include
rendering fields (`sprite`, `tile_size`, `grid_size`). For stackable items,
set `min_count`/`max_count`. Read existing items at the same kind for
correct rendering values.

### Step 6: Approve & Write

Present design summary + RON entries. On approval, append to `items.ron`
and `item_spawns.ron`.

## Workflow 3: Create Spell

### Step 1: Read Current State

Read `assets/spells.ron`. Note existing spells by damage type, targeting
mode, and role (attack/heal/buff/debuff/utility/summon).

### Step 2: Spell Fantasy

Ask what casting this spell should feel like:

- **Role**: Damage, healing, buff, debuff, control, utility, summon?
- **Targeting**: Castor, Enemy, Ally, AllyOrSelf?
- **Frequency**: Cantrip (cheap, spammable) or big spell (expensive, impactful)?

### Step 3: Effect Design

Propose spell effects from `references/ron-schemas.md`. For damage/healing,
propose dice and INT scaling. For status effects, propose duration. Multiple
effects are fine for higher-cost spells.

### Step 4: Cost & Cooldown Balance

Using `references/balance-curves.md`:

- **Cantrip** (3-8 mana, 0-4 CD): 1d4-1d6 damage, minor utility
- **Standard** (10-20 mana, 5-12 CD): 2d4-2d8 damage, meaningful effect
- **Powerhouse** (25-35 mana, 15-25 CD): 3d6-4d6 damage, encounter-changing

Cross-reference existing spells at similar power levels.

### Step 5: Monster Access

Ask whether any monsters should also cast this spell. If yes, note which
monster definitions would need updating.

### Step 6: Approve & Write

Present design summary + RON entry. On approval, append to `spells.ron`.
If a spellbook item is needed for the player to learn it, chain into
**Create Item** workflow.

## Workflow 4: Generate Faction

The composite workflow. Designs a cohesive faction as a unit.

### Step 1: Read Current State

Read all data files (`monsters.ron`, `items.ron`, `spells.ron`, and spawn
files). Read `references/faction-design-guide.md` for roster templates
and existing faction analysis. Read `docs/design/BESTIARY.md` for design
rationale.

### Step 2: Faction Identity

Guided questions:

- **Theme**: What unifies this faction visually and mechanically?
- **Floor range**: Where does this faction appear? (Current content:
  floors 1-20. Floors 21-26 are future content.)
- **Personality**: How do they fight as a group? (aggressive, defensive,
  tricky, swarming)

### Step 3: Roster Design

Propose 4-6 monsters following `references/faction-design-guide.md`:

- **Fodder** (1-2): Weak, appears in groups. Bottom of floor range.
- **Standard** (1-2): Core encounter unit. Middle of floor range.
- **Elite** (1): Dangerous solo or as squad leader. Upper floor range.
- **Boss candidate** (0-1): Could anchor a boss fight. Top of range.

Each monster gets a brief concept + how it synergizes with the others.

### Step 4: Faction Abilities

Design 2-4 signature abilities that make this faction feel distinct.
Use only implemented types from `references/ron-schemas.md`. Choose
between shared traits (all members) and specialist abilities (elite only).

### Step 5: Themed Loot

Propose 2-4 faction-themed items:

- A weapon that plays into the faction's mechanic
- An armor piece or accessory with thematic bonuses
- A consumable or spellbook if appropriate

### Step 6: Faction Spells

If the faction has casters, design their spells. These might also be
learnable by the player via spellbooks.

### Step 7: Sequential Approval & Write

Walk through each content piece one at a time:
1. Monsters (one by one)
2. Spells
3. Items
4. Spawn configurations

Approve each before writing. This prevents all-or-nothing commits.

## Workflow 5: Audit Gaps

### Step 1: Read All Data Files

Read `monsters.ron`, `items.ron`, `spells.ron`, `essence_nodes.ron`,
`props.ron`, `structures.ron`, and all spawn files. Reference
`docs/design/BESTIARY.md` for faction design rationale.

### Step 2: Analyze Coverage

Check for gaps across 7 dimensions:

1. **Floor coverage** — Floor ranges with low monster variety
2. **Damage types** — Underrepresented in items, spells, resistances
3. **Item kinds** — Missing kinds at certain rarities (e.g., no Rare amulets)
4. **Spell roles** — Categories with few options (few debuffs, no AoE healing)
5. **Faction roles** — Factions with incomplete role coverage
6. **Essence synergies** — Essence tree bonuses not represented in items
7. **Resistance gaps** — Damage types with no corresponding counter-items

### Step 3: Present Prioritized Recommendations

Rank gaps by impact on gameplay variety. For each gap, suggest a brief
concept for what could fill it.

### Step 4: Optionally Chain

If the user picks a gap, flow into the appropriate creation workflow.

## Cross-Cutting Rules

1. **Always read current state first** — Never propose content without
   knowing what exists.
2. **One question at a time** — Walk through dimensions sequentially.
   Don't overwhelm with multiple questions per message.
3. **Validate against implemented types** — Only use ability types, bonus
   types, spell effects, etc. that exist in the Rust source. Refer to
   `references/ron-schemas.md` for the complete list. No inventing new
   mechanics.
4. **Present before writing** — Always show the complete design summary
   and RON entries before modifying any files.
5. **Append, don't overwrite** — New entries are appended to existing RON
   files, never replacing existing content.
6. **Chain when appropriate** — Creating a monster that casts spells →
   chain into Create Spell. Creating faction loot → chain into Create Item.
   Creating a spell the player can learn → chain into Create Item for the
   spellbook.

## Relationship to Other Skills

- **game-mechanics-designer**: For rebalancing *existing* content, analyzing
  the game loop, and writing design docs. Content-forge is for *creating
  new* content. If the user wants to tune existing monster stats, use
  game-mechanics-designer. If they want to brainstorm a new monster, use
  content-forge.
- **prefab-designer**: Creates encounter layouts (prefabs) that reference
  monsters and props. Content-forge creates the monsters and items that
  prefabs use. Design content first, then design encounters that feature it.
