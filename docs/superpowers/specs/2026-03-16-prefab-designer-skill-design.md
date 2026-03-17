# Prefab Designer Skill — Design Spec

## Overview

A Claude Code skill for designing prefabricated dungeon encounters and auditing the prefab catalog in The Veiled Tyrant. The skill guides collaborative design of tactically interesting encounters, producing both human-readable design rationale and ready-to-use RON data entries.

## Design Goals

- **Encounter-first thinking** — Always start with "what does the player experience?" before touching geometry or data.
- **Catalog awareness** — Never design something redundant. Always consider the existing catalog.
- **Brogue-inspired** — Tactical decisions, environmental hazards, rewarding observation.
- **Validated output** — Generated RON uses only valid schema values. Monster roles, props, structures, placement types are checked against reference docs.
- **Design-first output** — Produce a readable encounter design covering all 8 dimensions before generating the RON entry.

## Skill Structure

```
.claude/skills/prefab-designer/
  SKILL.md                          # Workflows + process
  references/
    prefab-schema.md                # RON format, all valid fields/values
    encounter-design-principles.md  # Tactical geometry, Brogue-inspired patterns
```

**No static catalog summary.** Both workflows read `assets/prefabs.ron` directly to get the live catalog state, eliminating maintenance burden.

### Skill Description (for discovery)

> Use when the user asks to "design a prefab", "create an encounter", "audit the prefab catalog", "brainstorm dungeon rooms", "fill gaps in prefabs", or discusses prefab layout, tactics, or encounter design for The Veiled Tyrant.

## Workflow 1: Design a Prefab

A guided conversation through 8 encounter dimensions. The skill walks through each in order, asking questions and proposing options at each step.

### Step 1: Read Current State

Before any design work, read `assets/prefabs.ron` to understand what already exists. Identify the current count by tier, placement strategy distribution, role coverage, and depth ranges in use.

### Step 2: Encounter Fantasy

Ask what the player experience should feel like. If the user wants inspiration, suggest archetypes:

- **Sentinel gauntlet** — Fortified position the player must push through
- **Trapped treasure** — High reward behind monster guards or environmental hazards
- **Ambush corridor** — Player walks into a kill zone, must react quickly
- **Ritual disruption** — Caster performing something the player wants to interrupt
- **Monster lair** — Creature's home territory with environmental advantages
- **Patrol checkpoint** — Guards controlling passage, player can try to sneak or fight
- **Puzzle room** — Layout rewards observation and positioning over raw power

### Step 3: Tactical Geometry

Sketch 2-3 ASCII layout variants with commentary on how each plays differently. Consider:

- Chokepoints and funnels
- Cover positions (barrels, barricades, pillars)
- Sight lines (ranged advantage/disadvantage)
- Approach angles (single entry vs. multiple)
- Internal doors and rooms-within-rooms

### Step 4: Monster Composition

Propose roles, count, and faction considerations based on the encounter fantasy. Reference valid roles:

- `melee_guard` — Holds position, blocks approaches
- `ranged` — Attacks from distance, stays behind cover
- `brute` — High damage/HP, anchors the encounter
- `caster` — Spell-based attacks, high priority target
- `leader` — Squad leader, triggers on_leader_death behavior
- `any` — Flexible slot, filled by whatever the faction provides

Consider depth-appropriate difficulty and how roles interact tactically (e.g., ranged + melee_guard creates cover-and-fire, caster + brute creates priority dilemma).

### Step 5: Squad Behavior

Recommend `on_leader_death` and `flee_threshold` based on encounter drama:

| Drama | on_leader_death | flee_threshold |
|-------|----------------|----------------|
| Desperate last stand | `fight_on` | 0.15-0.20 |
| Disciplined unit | `fight_on` | 0.25-0.30 |
| Aggressive mob | `enrage` | 0.20-0.25 |
| Opportunistic raiders | `scatter` | 0.35-0.40 |
| Cowardly ambushers | `flee` | 0.40-0.50 |

Decide which monsters are `guard: true` (defend position) vs `guard: false` (roam/flank).

### Step 6: Loot & Structures

Propose reward density appropriate to difficulty:

- Low-risk encounters: 0-1 chests, utility props
- Medium-risk: 1-2 chests, possibly a structure
- High-risk: 2-3 chests, guaranteed structure or rare item spawn
- Landmark: Significant rewards matching the commitment to clear

Reference valid props and structures from `assets/props.ron` and `assets/structures.ron`.

### Step 7: Placement Strategy & Size

Recommend placement based on geometry:

- **room** — Overlays into existing rooms. Best for encounters that fit within natural spaces.
- **wall** — Carves into solid wall. Best for hidden rooms, vaults, dens.
- **chokepoint** — Placed at corridor bottlenecks. Best for gatekeeping encounters.
- No explicit placement — system tries both room and wall.

Set width/height. Reference size categories:

- Small: < 31 tiles (fills budget gaps, high variety)
- Medium: 31-99 tiles (tactical landmarks)
- Large: 100-149 tiles (significant encounters)
- Landmark: 150+ tiles (placed in pass 1, major set pieces)

### Step 8: Depth Range & Difficulty Tier

Place the prefab in the dungeon's 26-floor progression:

- Floors 1-5: Easy tier (1-2 monsters, simple tactics)
- Floors 3-10: Medium tier (2-3 monsters, combined roles)
- Floors 6-15: Hard tier (3-4 monsters, complex tactics)
- Floors 8-20: Landmark tier (4-6 monsters, major encounters)
- Floors 15-26: Late game (toughest configurations)

Overlap between tiers is intentional — depth ranges should blend.

### Step 9: Orientation

Recommend rotation/flip settings:

- **Allow both** — For symmetric or non-directional designs (most common)
- **Rotation only** — When horizontal flip would break the logic
- **Neither** — When directionality is critical (e.g., a one-way ambush)

### Step 10: Output

Present a design summary covering all 8 dimensions, then generate the complete RON `PrefabTemplate` entry. The RON must:

- Use only valid field names and value types per the schema reference
- Use only monster roles, prop names, and structure names that exist in the game
- Have correct coordinate math for the tile layout dimensions
- Place spawns only on floor tiles (`.`), not walls (`#`) or unchanged (` `)

## Workflow 2: Audit Catalog

A structured analysis that identifies gaps and recommends new designs.

### Step 1: Read Current State

Parse `assets/prefabs.ron` to get the full live catalog.

### Step 2: Analyze Coverage

Evaluate across these dimensions:

- **Tactical variety** — What approach patterns exist (frontal assault, flanking, stealth, puzzle, trap)? What's missing?
- **Monster role coverage** — Are all roles well-represented? Any underused (e.g., caster-heavy encounters)?
- **Depth distribution** — Are there floor ranges where few or no prefabs are eligible?
- **Size distribution** — Balance of small / medium / large / landmark prefabs.
- **Placement strategy mix** — Ratio of room / wall-carve / chokepoint prefabs.
- **Squad behavior variety** — Distribution of on_leader_death responses and flee thresholds.
- **Terrain interaction** — Do any prefabs use water, lava, or doors creatively?
- **Reward density** — Are high-risk prefabs appropriately rewarded?
- **Faction coverage** — Are faction-locked prefabs balanced or concentrated?

### Step 3: Present Findings

Deliver a gap analysis with specific recommendations, prioritized by impact on gameplay variety. For each gap, describe:

- What's missing and why it matters for player experience
- A brief encounter concept that would fill the gap
- Suggested difficulty tier and depth range

### Step 4: Optionally Transition

If the user wants to act on a gap, flow into Workflow 1 to design a prefab that fills it.

## Cross-Cutting Principles

- **Encounter-first** — Always start with "what does the player experience?" before geometry.
- **Catalog awareness** — Check the live catalog before and during design to avoid redundancy.
- **Brogue inspiration** — Tactical decisions > stat checks. Environmental hazards > more monsters. Rewarding observation > punishing ignorance.
- **Validate output** — Generated RON must use only valid values from the schema reference.
- **One question at a time** — Don't overwhelm. Walk through dimensions sequentially.
- **Show your work** — Present tactical reasoning for layout choices, not just the layout.

## Reference Documents

### `references/prefab-schema.md`

Contains:

- All `PrefabTemplate` fields with types, defaults, and descriptions
- Valid enum values: placement types (`room`, `wall`), monster roles (`melee_guard`, `ranged`, `brute`, `caster`, `leader`, `any`), on_leader_death behaviors (`scatter`, `enrage`, `fight_on`, `flee`)
- Valid prop names and structure names (sourced from `props.ron` and `structures.ron`)
- Size category definitions and budget system explanation (350 tile budget, 2-tile padding, 3-pass ordering)
- Annotated example RON entry

### `references/encounter-design-principles.md`

Contains:

- Encounter archetypes with descriptions and tactical profiles
- Geometry patterns (L-cover, funnel, split approach, elevated positions, barrel maze, room-within-room)
- Squad composition heuristics (role synergies, when to use each on_leader_death behavior)
- Orientation decision guide
- Anti-patterns to avoid (decorative layouts, impossible approaches, unwinnable encounters, empty loot rooms, redundant designs)
- Brogue design philosophy distilled into actionable prefab guidance

## Implementation Notes

- This is a pure skill (`.claude/skills/` files only) — no Rust code changes needed.
- The skill reads `assets/prefabs.ron`, `assets/props.ron`, and `assets/structures.ron` at runtime.
- Reference docs should be updated when the RON schema changes (new fields, new valid values).
- The skill lives at `.claude/skills/prefab-designer/` alongside the existing `game-mechanics-designer/` skill.
