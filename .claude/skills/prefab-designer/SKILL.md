---
name: Prefab Designer
description: >
  Use when the user asks to "design a prefab", "create an encounter",
  "audit the prefab catalog", "brainstorm dungeon rooms", "fill gaps
  in prefabs", or discusses prefab layout, tactics, or encounter design
  for The Veiled Tyrant.
---

# Prefab Designer

Design tactically interesting prefabricated dungeon encounters and audit
the prefab catalog for The Veiled Tyrant, a Brogue-inspired roguelike
built with Bevy/Rust. This skill provides two workflows: designing
individual prefabs through guided conversation, and auditing the catalog
for gaps in coverage.

## Before Starting Either Workflow

Read these files to load context:

**Skill references (this directory):**
- `references/prefab-schema.md` — RON format, valid values, budget system
- `references/encounter-design-principles.md` — Tactical patterns, anti-patterns

**Live game data:**
- `assets/prefabs.ron` — Current prefab catalog
- `assets/props.ron` — Valid prop names
- `assets/structures.ron` — Valid structure names
- `assets/monsters.ron` — Factions, roles, depth ranges

## Cross-Cutting Principles

Every prefab design decision must follow these rules:

1. **Encounter-first** — Always start with "what does the player experience?" before touching geometry or data.
2. **Catalog awareness** — Never duplicate an existing prefab's tactical situation. Check the live catalog before and during design.
3. **Brogue inspiration** — Tactical decisions > stat checks. Environmental hazards > more monsters. Rewarding observation > punishing ignorance.
4. **Validate output** — Generated RON must use only valid values from the schema reference.
5. **One question at a time** — Don't overwhelm. Walk through design dimensions sequentially.
6. **Show your work** — Present tactical reasoning for layout choices, not just the layout.

## Workflow 1: Design a Prefab

A guided conversation through 8 encounter dimensions. Walk through each
step in order, asking questions and proposing options.

### Step 1: Read Current State

Parse `assets/prefabs.ron`. Summarize:
- Current prefab count by difficulty tier
- Placement strategy distribution (room/wall/chokepoint/landmark)
- Monster role coverage
- Depth ranges in use (identify any gaps)

### Step 2: Encounter Fantasy

Ask what the player experience should feel like. If the user wants
inspiration, offer these archetypes (from design principles reference):

- **Sentinel gauntlet** — Push through a fortified position
- **Trapped treasure** — High reward behind monster guards or hazards
- **Ambush corridor** — Walk into a kill zone, must react quickly
- **Ritual disruption** — Interrupt a caster before their spell completes
- **Monster lair** — Fight in a creature's home territory
- **Patrol checkpoint** — Guards at a passage, sneak or fight
- **Puzzle room** — Layout rewards observation and positioning

### Step 3: Tactical Geometry

Sketch 2-3 ASCII layout variants with commentary on how each plays
differently. For each variant, annotate:
- Chokepoints and funnels
- Cover positions (barrels, barricades, pillars)
- Sight lines (ranged advantage/disadvantage)
- Approach angles (single entry vs. multiple)
- Internal doors and rooms-within-rooms

### Step 4: Monster Composition

Propose roles, count, and faction considerations. Reference valid roles:
- `melee_guard` — Holds position, blocks approaches
- `ranged` — Attacks from distance, stays behind cover
- `brute` — High damage/HP, anchors the encounter
- `caster` — Spell-based attacks, high priority target
- `leader` — Squad leader, triggers on_leader_death behavior
- `any` — Flexible slot, filled by whatever the faction provides

Consider how roles interact tactically (see squad composition heuristics
in design principles reference).

**Faction note:** The role resolver picks factions automatically from
`monsters.ron` based on which factions can fill all required roles at the
current depth. The `faction_tag` field for prefab-level faction locking
is aspirational (exists in some RON entries but not yet in the Rust struct).

### Step 5: Squad Behavior

Recommend `on_leader_death` and `flee_threshold` based on encounter
drama. Reference the behavior selection guide in design principles.

Assign a `behavior` to each monster: `Sentry` (defend position), `Patrol` (walk waypoints),
`Roam` (wander within area), or `Wander` (roam freely / flank).

### Step 6: Loot & Structures

Propose reward density appropriate to difficulty. Reference the reward
scaling table in design principles. Read `assets/props.ron` and
`assets/structures.ron` for valid names.

### Step 7: Placement Strategy & Size

Recommend placement and dimensions:
- **room** — Overlay into existing rooms
- **wall** — Carve into solid walls (adds a door at border)
- **chokepoint** — Corridor bottlenecks (max 1 per floor)
- **landmark** — Large set-piece, stamped before room generation
- **any** — Try both room and wall (default)

Reference size categories from schema (small < 31, medium 31-99,
large 100-149, landmark 150+).

### Step 8: Depth Range & Difficulty Tier

Place the prefab in the 26-floor progression:

| Tier | Floors | Monsters | Complexity |
|------|--------|----------|------------|
| Easy | 1-5 | 1-2 | Simple, single role |
| Medium | 3-10 | 2-3 | Combined roles |
| Hard | 6-15 | 3-4 | Multi-role squads |
| Landmark | 8-20 | 4-6 | Full squad dynamics |
| Late game | 15-26 | Toughest | Most challenging |

Check existing catalog depth coverage to avoid dead zones.

### Step 9: Orientation

Recommend `allow_rotate` and `allow_flip` settings:
- **Both true** (default) — Symmetric or non-directional designs
- **Rotate only** — Left/right asymmetry matters tactically
- **Neither** — Directionality is critical (rare)

### Step 10: Output

Present a design summary covering all 8 dimensions, then generate the
complete RON `PrefabTemplate` entry.

**Validation checklist before presenting RON:**
- [ ] All field names and types match the schema
- [ ] Monster roles are valid: `melee_guard`, `ranged`, `brute`, `caster`, `leader`, `any`
- [ ] Prop names exist in `assets/props.ron`
- [ ] Structure names exist in `assets/structures.ron`
- [ ] Spawn coordinates land on floor (`.`) or door (`+`) tiles, not walls or unchanged
- [ ] Tile row count equals `height`; each row length equals `width`
- [ ] Coordinate system: (0,0) is top-left of tile grid
- [ ] `on_leader_death` is one of: `scatter`, `enrage`, `fight_on`, `flee`
- [ ] `placement` is one of: `room`, `wall`, `chokepoint`, `landmark`, `any`

## Workflow 2: Audit Catalog

A structured analysis identifying gaps and recommending new designs.

### Step 1: Read Current State

Parse `assets/prefabs.ron` and `assets/monsters.ron` to understand the
full catalog and available factions/roles.

### Step 2: Analyze Coverage

Evaluate across these 9 dimensions:

1. **Tactical variety** — What approach patterns exist (frontal assault, flanking, ambush, puzzle, trap)? What's missing?
2. **Monster role coverage** — Are all roles well-represented? Any underused (e.g., caster-heavy encounters)?
3. **Depth distribution** — Are there floor ranges where few or no prefabs are eligible?
4. **Size distribution** — Balance of small / medium / large / landmark prefabs
5. **Placement strategy mix** — Ratio of room / wall / chokepoint / landmark
6. **Squad behavior variety** — Distribution of on_leader_death responses and flee thresholds
7. **Terrain interaction** — Do any prefabs use water, lava, or doors creatively?
8. **Reward density** — Are high-risk prefabs appropriately rewarded?
9. **Faction coverage** — Are faction-locked prefabs balanced or concentrated?

### Step 3: Present Findings

Deliver a gap analysis prioritized by impact on gameplay variety. For
each gap, describe:
- What's missing and why it matters for player experience
- A brief encounter concept that would fill the gap
- Suggested difficulty tier and depth range

### Step 4: Optionally Transition

If the user wants to act on a gap, flow into Workflow 1 to design a
prefab that fills it.

## Quick Reference

**Monster roles:** `melee_guard`, `ranged`, `brute`, `caster`, `leader`, `any`

**Leader death:** `scatter`, `enrage`, `fight_on`, `flee`

**Placement:** `room`, `wall`, `chokepoint`, `landmark`, `any`

**Tile chars:** `#` wall, `.` floor, `+` door, ` ` unchanged

**Size thresholds:** small < 31, medium 31-99, large 100-149, landmark 150+

**Budget:** 350 tiles/floor, 2-tile padding, 3 consecutive failures stops placement
