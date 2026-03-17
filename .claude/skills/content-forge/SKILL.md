---
name: Content Forge
description: >
  Use when the user asks to "create a monster", "design an item",
  "brainstorm a spell", "design an ability", "create an on-hit effect",
  "add a passive", "generate a faction", "what monsters are missing",
  "fill gaps in the bestiary", "add a new enemy type", "design loot",
  or wants to brainstorm, balance, and produce new game content
  (monsters, items, spells, abilities, factions) for The Veiled Tyrant.
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
| Design Ability | `src/game/abilities.rs`, `src/game/combat.rs`, `assets/monsters.ron`, `references/ability-catalog.md` |
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

Using `references/balance-curves.md`, propose stats based on target floor range.
The system uses **direct values** — no attribute-to-stat conversion.

- **Base HP** (`base_hp`): Used directly as `Health.max`. No scaling.
  See balance-curves.md reference data for floor-appropriate values.
- **Damage dice** (`damage`): Scale so monsters deal 15-25% of player HP
  (early) to 30-50% (late) per hit.
- **Damage type** (`damage_type`): Physical, Fire, Lightning, or Necrotic.
- **Armor** (`base_armor`): 0-2 (floors 1-5), 3-5 (6-12), 5-8 (13-18),
  7-12 (19-20).
- **Perception** (`perception`): Vision range = `8 + (perception - 10)`.
  Low (6-8) = poor detection, high (12-14) = wide vision.
- **Intelligence** (`intelligence`): Casters only. Mana pool = `INT * 5`.
  Set to 0 for non-casters. Casters: 14-22.
- **Level** (`level`): Used in essence reward formula.
- **Experience**: `10 + (level * 5) + (base_hp / 2)`.

**Legacy fields** (`strength`, `dexterity`, `constitution`, `agility`):
Still present in the MonsterAsset struct but not used by the spawner.
Set to 10 (baseline) for consistency — they may be reconnected later.

Present the stat block with reasoning for each value.

### Step 4: Abilities & Resistances

**Note on current state:** Ability fields (`on_hit_effects`, `poison_body`,
`thorn_aura`, etc.) exist in `monsters.ron` data but are **orphaned** — the
`MonsterAsset` struct doesn't declare them, so they're silently ignored.
The handler systems exist in `abilities.rs` and are registered, but no
monsters receive these components at spawn time. This is WIP — the data
is preserved for future reconnection.

Still propose abilities to include in the RON data for when the pipeline is
reconnected. Reference `references/ability-catalog.md` for the full list:

- **On-hit effects**: ApplyPoison, ApplySlow, ApplyStun, ApplyBurning,
  AttributeDrain, Knockback, LifeDrain, Disarm
- **Passives**: poison_body, thorn_aura, reanimate_hp, enrage_on_hit,
  explode_on_death, death_curse, summon_on_death
- **Resistances**: Map of damage_type → ResistanceLevel (this IS active)
- **Spells**: List of spell IDs from `assets/spells.ron` (this IS active)

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
by kind, rarity, and stat coverage.

### Step 2: Item Fantasy

Ask what finding this item should feel like:

- **Kind**: Weapon, Armor (which slot?), Ring, Amulet, Consumable, Spellbook?
- **Rarity**: Common, Uncommon, Rare, Legendary?
- **Identity**: What makes this item memorable? A Legendary should be run-defining.

**Note:** The ItemBonus system has been removed. Items differentiate by
their direct `damage`/`defense` values and rarity tier. Legacy attribute
bonus fields exist on ItemAsset but are not used by the spawner.

### Step 3: Core Stats

Based on kind and rarity, propose:

- **Weapons**: Damage dice (1d4-2d8), weapon_range (0=melee, 4-8=ranged)
- **Armor**: Defense value by slot and rarity, armor_slot required
- **Consumables**: Effect type and magnitude
- **Spellbooks**: LearnSpell effect with spell ID

### Step 4: Spawn Configuration

Propose floor range, rarity, and weight for `item_spawns.ron`. Include
rendering fields (`sprite`, `tile_size`, `grid_size`). For stackable items,
set `min_count`/`max_count`. Read existing items at the same kind for
correct rendering values.

### Step 5: Approve & Write

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

## Workflow 4: Design Ability

Design passive, reactive, or triggered abilities outside the spell/mana
system. Abilities are ECS components — some are data-driven through RON,
others require Rust implementation.

### Ability Categories

There are 4 categories in the ability system. Read
`references/ability-catalog.md` to see what already exists before
proposing new abilities.

**Current state:** Ability handler systems are registered in `abilities.rs`
but the `MonsterAsset` struct does not declare ability fields. Ability data
in `monsters.ron` is orphaned (silently ignored during deserialization).
Designing abilities is still valuable — the data and handlers are preserved
for future reconnection.

| Category | How It Works | Status | Examples |
|----------|-------------|--------|----------|
| **On-Hit Effects** | Trigger on successful melee attack | Handlers active, data orphaned | ApplyPoison, LifeDrain, ApplyStun |
| **Monster Passives** | Always-on or event-triggered components | Handlers active, data orphaned | ExplodeOnDeath, Reanimate, ThornAura |
| **Auras** | Radius-based buffs/debuffs to allies or enemies | Code-only, not in RON | ArmorBonus, DamagePercent, RegenBonus |
| **Build Passives** | Player-only abilities unlocked via Essence tree | Active | Cleave, Riposte, BloodRage, Ambush |

### Step 1: Read Current State

Read `src/game/abilities.rs` to see all implemented ability components.
Read `references/ability-catalog.md` for the full categorized listing.
Identify what trigger types, effect types, and tactical niches already
exist.

### Step 2: Ability Fantasy

Ask what this ability should *feel like* in play:

- **Trigger**: When does it activate?
  - On hit (attacker lands a blow)
  - On being hit (defender is struck)
  - On death (entity or target dies)
  - On kill (attacker kills something)
  - Passive/aura (always active)
  - Threshold (HP drops below X%)
  - Conditional (specific situation like "from outside FOV")

- **Effect**: What happens?
  - Damage (direct, DoT, AoE)
  - Status (poison, slow, stun, burn, disarm)
  - Stat modification (buff/debuff an attribute)
  - Resource (heal HP, drain mana, restore on kill)
  - Positional (knockback, teleport, summon)
  - Defensive (reflect, absorb, reduce)

- **Target**: Who does it affect?
  - Self, attacker, defender, nearby allies, nearby enemies, killed entity

### Step 3: Determine Implementation Path

Based on the design, determine if this ability:

**A) Uses existing types (RON data change)**
- Can be expressed as an existing `OnHitEffect` variant on a monster
- Can use existing passive fields (`poison_body`, `thorn_aura`, etc.)
- Note: These fields are currently orphaned (not on `MonsterAsset` struct),
  so data will be preserved in RON but won't take effect until the struct
  and spawner are reconnected.
- → Output: RON entries for monsters. Note reconnection requirement.

**B) Requires a new Rust component (code change)**
- New trigger type or effect type not covered by existing components
- → Output: Design doc with component definition, handler system
  description, RON field mapping, and which monsters/items should use it.
  Flag that implementation requires the `rust-expert` skill or manual
  coding.

Always prefer path A when possible. Only propose path B when the existing
types genuinely cannot express the ability.

### Step 4: Balance Parameters

For abilities with tunable values, propose balanced parameters:

- **Chance** (on-hit): 15-30% for strong effects (stun, disarm),
  40-80% for mild effects (poison, slow). 100% for defining abilities.
- **Duration**: 2-4 turns for strong debuffs, 5-10 for mild ones.
- **Damage**: On-death explosions: 1d4-2d6 scaling with monster level.
  Thorns/reflect: 1-5 flat damage. DoT: 1-3 per turn.
- **Radius** (auras/explosions): 1-2 tiles for strong effects, 3-4 for
  mild buffs.
- **Threshold** (enrage-type): 25-50% HP.

Cross-reference similar existing abilities for consistency.

### Step 5: Monster & Item Assignment

Propose which monsters should carry this ability and which items could
grant it:

- Does this ability define a faction mechanic? (shared across roster)
- Is it a specialist ability? (elite/boss only)
- Should items grant a version of this? (future — no item ability system currently)
- Should an Essence node unlock a player version?

### Step 6: Approve & Write

**Path A (RON-only):** Present the ability configuration and which
monster/item entries to modify. On approval, update the relevant RON files.

**Path B (new component):** Present the full design doc including:
- Component struct definition
- Handler system description (what events it hooks into)
- RON field name and type for `MonsterAsset` (if applicable)
- Spawner changes needed in `src/game/spawner.rs`
- Which monsters/items use it and with what parameters
- Note: "Implementation requires Rust changes. Use `rust-expert` skill
  or implement manually, then return here to assign to monsters/items."

## Workflow 5: Generate Faction

*(Formerly Workflow 4)*

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

Propose 2-4 faction-themed items. Note that items currently only
differentiate by `damage`/`defense` values and rarity — no bonus system.
Focus on appropriate power level and thematic naming/description:

- A weapon with damage dice fitting the faction's floor range
- An armor piece for an underserved slot
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

## Workflow 6: Audit Gaps

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
8. **Ability coverage** — Trigger types or effect types underused across
   monsters (e.g., no on-death effects in a faction, few aura sources)

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
3. **Validate against implemented types** — For RON-only changes, only use
   ability types, spell effects, etc. that exist in the Rust source. Refer
   to `references/ron-schemas.md` and `references/ability-catalog.md` for
   the complete lists. For the Design Ability workflow, new types may be
   proposed but must be clearly flagged as requiring Rust implementation.
   Note that many ability fields are currently orphaned (see balance-curves.md).
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
