# Prefab Designer Skill Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a Claude Code skill that helps design tactically interesting prefabricated dungeon encounters and audit the existing prefab catalog.

**Architecture:** Three markdown files — one SKILL.md defining two workflows (Design Prefab + Audit Catalog), and two reference documents (RON schema + encounter design principles). The skill reads live RON data files at runtime for catalog awareness.

**Tech Stack:** Claude Code skills (markdown files only), no Rust code changes.

**Spec:** `docs/superpowers/specs/2026-03-16-prefab-designer-skill-design.md`

---

## File Structure

```
.claude/skills/prefab-designer/
  SKILL.md                          # Main skill: workflows, process, quick reference
  references/
    prefab-schema.md                # Complete RON schema with all valid values
    encounter-design-principles.md  # Tactical design patterns and anti-patterns
```

All three files are new. No existing files are modified.

---

### Task 1: Create `references/prefab-schema.md`

The foundational reference — all other files depend on knowing the exact schema.

**Files:**
- Create: `.claude/skills/prefab-designer/references/prefab-schema.md`

- [ ] **Step 1: Write the prefab schema reference document**

This file documents the complete `PrefabTemplate` RON format. Include:

**PrefabTemplate fields** (from `src/assets/mod.rs:243-271`):

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | String | required | Unique identifier |
| `width` | i32 | required | Tile width |
| `height` | i32 | required | Tile height |
| `min_floor` | i32 | required | Earliest floor this can appear |
| `max_floor` | i32 | required | Latest floor this can appear |
| `tiles` | Vec\<String\> | required | Row-major ASCII layout, one string per row |
| `props` | Vec\<PrefabPropEntry\> | [] | Decorative/interactive world objects |
| `structures` | Vec\<PrefabStructureEntry\> | [] | Special map structures with AI |
| `monster_spawns` | Vec\<PrefabMonsterSpawn\> | [] | Monster positions with role assignments |
| `item_spawns` | Vec\<PrefabItemSpawn\> | [] | Item positions (specific or random) |
| `on_leader_death` | String | "" | Squad behavior when leader dies |
| `flee_threshold` | f32 | 0.35 | HP ratio triggering flee |
| `placement` | String | "any" | Placement strategy |
| `allow_rotate` | bool | true | Enable 90/180/270° rotation |
| `allow_flip` | bool | true | Enable horizontal flip |

**Sub-entry formats:**

```ron
// PrefabPropEntry
(x: 2, y: 1, prop: "barricade")

// PrefabStructureEntry
(x: 3, y: 3, structure: "Goblin Totem")

// PrefabMonsterSpawn
(x: 1, y: 2, role: "melee_guard", guard: true)

// PrefabItemSpawn — specific item or None for random
(x: 4, y: 1, item: Some("Iron Sword"))
(x: 4, y: 1, item: None)
```

**Tile characters:**

| Char | Meaning |
|------|---------|
| `#` | Wall |
| `.` | Floor |
| `+` | Door |
| ` ` | Unchanged (passthrough — keeps whatever the map already has) |

**Valid monster roles:** `melee_guard`, `ranged`, `brute`, `caster`, `leader`, `any`

**Valid on_leader_death values:** `scatter`, `enrage`, `fight_on`, `flee`
- Note: `fight_on` and `flee` are defined in prefab data but currently fall through to `Nothing` in `squad.rs`. Document as intended vocabulary pending code fix.

**Valid placement values:** `room`, `wall`, `chokepoint`, `landmark`, `any` (default)

**Valid prop names** (from `assets/props.ron`):
candle, watchfire, totem_pole, barricade, barrel, small_chest, chest, small_red_chest, red_chest, fountain, corrupted_fountain, tyrants_offering

**Valid structure names** (from `assets/structures.ron`):
Goblin Totem, Tyrant's Altar, Orc War Drum, Necromancer's Pillar, Spider Egg Sac, Explosive Barrel, Poison Mushroom, Healing Spring, Soul Anchor, Necrotic Obelisk, Void Rift, Warding Stone, Tyrant's Eye

**Size categories and budget system:**
- Budget: 350 tiles per floor (width × height consumed per prefab)
- Padding: 2 tiles between prefabs
- Small: < 31 tiles (Pass 2, fills remaining budget)
- Medium/Large: ≥ 31 tiles (Pass 1, tactical landmarks)
- Landmark: large set-pieces placed before room generation
- Chokepoint: max 1 per floor, placed at corridor bottlenecks (Pass 0)
- 3 consecutive placement failures → stop placing

**Placement pass order:**
1. Pass 0: Chokepoint prefabs (max 1)
2. Pass 1: Medium/large prefabs (shuffled, one attempt each)
3. Pass 2: Small prefabs (random selection until budget exhausted)

**Annotated example:**

```ron
(
    name: "Sentry Post",
    width: 7,
    height: 6,
    min_floor: 1,
    max_floor: 8,
    placement: "room",
    tiles: [
        "       ",
        " ..... ",
        " ..... ",
        " ..... ",
        " ..... ",
        "       ",
    ],
    props: [
        (x: 1, y: 1, prop: "barricade"),
        (x: 2, y: 1, prop: "barricade"),
        (x: 1, y: 2, prop: "barricade"),
        (x: 5, y: 1, prop: "chest"),
    ],
    monster_spawns: [
        (x: 3, y: 3, role: "melee_guard", guard: true),
    ],
    on_leader_death: "scatter",
    flee_threshold: 0.4,
    allow_rotate: true,
    allow_flip: true,
)
```

**Faction note:** The `faction_tag` field appears in some prefab RON entries (e.g., Goblin Shrine) but is NOT currently in the `PrefabTemplate` Rust struct — serde silently ignores it. The monster role resolution system picks factions from `monsters.ron` based on which factions can fill all required roles at the current depth. Faction-locking requires adding the field to the struct (tracked separately).

- [ ] **Step 2: Review the file for accuracy against source code**

Verify all field names, types, defaults, and valid values match:
- `src/assets/mod.rs:211-271` (struct definitions)
- `src/map/builders/prefab_placer.rs` (placement logic)
- `assets/prefabs.ron` (live examples)
- `assets/props.ron` (prop names)
- `assets/structures.ron` (structure names)

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/prefab-designer/references/prefab-schema.md
git commit -m "feat(skill): add prefab schema reference for prefab-designer skill"
```

---

### Task 2: Create `references/encounter-design-principles.md`

Design guidance for creating tactically interesting encounters.

**Files:**
- Create: `.claude/skills/prefab-designer/references/encounter-design-principles.md`

- [ ] **Step 1: Write the encounter design principles document**

Include these sections:

**Encounter Archetypes:**

| Archetype | Player Experience | Key Geometry | Typical Roles |
|-----------|-------------------|--------------|---------------|
| Sentinel gauntlet | Push through fortified position | Barricade lines, narrow approach | melee_guard + ranged |
| Trapped treasure | High reward behind danger | Enclosed room, single entry | brute or caster + melee_guard |
| Ambush corridor | Walk into kill zone, react fast | Long hall, side alcoves | ranged + melee flankers (guard: false) |
| Ritual disruption | Interrupt caster before spell completes | Open center, peripheral cover | caster (center) + melee_guard |
| Monster lair | Fight in creature's territory | Organic cave shape, debris | brute + any |
| Patrol checkpoint | Guards at chokepoint, sneak or fight | Corridor with pillars/barricades | melee_guard (guard: true) |
| Puzzle room | Layout rewards positioning over power | Unusual geometry, terrain features | varied, fewer monsters |

**Tactical Geometry Patterns:**

- **L-shaped cover** — Barricades form an L, ranged unit behind the corner. Player must approach from exposed angle or flank around.
- **Funnel chokepoint** — Barricades/pillars narrow approach to 1-2 tiles. Melee guards hold the gap, ranged fires over.
- **Split approach** — Two entrances force player to choose. Monsters positioned to cover both.
- **Barrel maze** — Barrels create winding path. Monsters at intersections create ambush points.
- **Room-within-room** — Wall-carved inner chamber with single door. Defenders inside, player must breach.
- **Diamond/ring formation** — Props arranged in diamond/ring around central high-value target (caster, chest, structure).
- **Elevated position** — Ranged unit on a raised platform or behind cover with clear sight lines, melee guards at base.

**Squad Composition Heuristics:**

- ranged + melee_guard = cover-and-fire (ranged behind barricade, melee blocks approach)
- caster + brute = priority dilemma (kill the caster quickly, but brute is in the way)
- leader + melee_guard + ranged = combined arms (killing leader triggers behavior change)
- brute alone = simple but dangerous (high damage gatekeeper)
- multiple melee (guard: false) = swarm/ambush (flankers close from multiple directions)

**Squad Behavior Selection Guide:**

| Encounter Drama | on_leader_death | flee_threshold | Reasoning |
|-----------------|-----------------|----------------|-----------|
| Desperate defenders | `fight_on` | 0.15-0.20 | They have nowhere to run |
| Disciplined soldiers | `fight_on` | 0.25-0.30 | Hold the line |
| Aggressive mob | `enrage` | 0.20-0.25 | Rage makes them dangerous when cornered |
| Raiders/bandits | `scatter` | 0.35-0.40 | Self-preservation over loyalty |
| Cowardly ambushers | `flee` | 0.40-0.50 | Only fight with advantage |

**Guard vs. Roam Decision:**

- `guard: true` — Monster patrols near its spawn point (3-tile radius). Use for sentries, defenders, anything holding a position.
- `guard: false` — Monster roams freely. Use for flankers, ambushers, patrols that should chase the player.
- Mix both in a single prefab for interesting dynamics (guards hold position while flankers pursue).

**Orientation Guidance:**

- **Allow both rotate + flip** (default) — For symmetric designs or when approach direction doesn't matter. Most prefabs should use this.
- **Rotate only, no flip** — When the prefab has left/right asymmetry that matters tactically (e.g., cover is on one side only).
- **No rotate, no flip** — When the prefab depends on a specific directional relationship (rare — most designs work in any orientation).

**Reward Scaling:**

- 0-1 monsters: 0-1 props (candle, barrel), maybe a small_chest
- 2 monsters: 1 chest or 1-2 useful props
- 3+ monsters: 1-2 chests + structure or multiple props
- Landmark (4-6 monsters): Significant rewards — multiple chests, structure, item spawns
- Risk must match reward. Empty rooms with high danger feel unfair; easy rooms with rich loot feel unearned.

**Anti-Patterns:**

- **Decorative only** — Prefab has interesting geometry but no tactical purpose. Every wall, barrel, and prop should affect how the fight plays out.
- **Impossible approach** — No way to engage without taking guaranteed hits. Player should always have a decision to make.
- **Unwinnable odds** — Too many monsters for the depth range. Reference existing prefabs: floors 1-5 have 1-2 monsters, not 4.
- **Empty loot room** — Rich rewards with no challenge. Even "unguarded" caches should have nearby threats or trade-offs.
- **Redundant design** — Too similar to an existing prefab. Check the catalog before finalizing. If the tactical situation is the same, it's redundant even if the geometry differs.
- **Overly complex geometry** — Prefabs larger than necessary for the encounter. A 5×5 single-monster room doesn't need to be 10×10. Tight geometry creates more interesting decisions.
- **Ignoring terrain** — Never using doors, never varying tile types. Doors create information asymmetry (what's behind the door?). Mixed terrain creates movement decisions.

**Brogue Design Philosophy Applied to Prefabs:**

1. **Tactical depth from simple rules** — A barricade + ranged monster creates more interesting decisions than a room full of melee enemies. Prefer fewer monsters with terrain interaction over more monsters in open space.
2. **Environmental storytelling** — The prefab layout should suggest a story. A watchfire with barricades is a camp. Barrels arranged in a maze with monsters at intersections is an ambush. A caster surrounded by totems is a ritual.
3. **Transparent systems** — The player should be able to look at a prefab and understand the tactical situation. Sight lines, cover positions, and approach angles should be readable from FOV.
4. **Meaningful risk/reward** — Every prefab presents a choice: engage for the reward, or skip and conserve resources. The value of the reward should be proportional to the risk.
5. **Terrain as weapon** — Doors block sight. Barricades block movement. Chokepoints limit approach angles. Use these as design tools, not just decoration.

- [ ] **Step 2: Review the file for completeness**

Verify it covers all design dimensions from the spec and provides actionable guidance (not just abstract principles).

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/prefab-designer/references/encounter-design-principles.md
git commit -m "feat(skill): add encounter design principles reference for prefab-designer skill"
```

---

### Task 3: Create `SKILL.md`

The main skill file that ties everything together with both workflows.

**Files:**
- Create: `.claude/skills/prefab-designer/SKILL.md`

**Dependencies:** Tasks 1 and 2 must be complete (SKILL.md references both files).

- [ ] **Step 1: Write the SKILL.md**

Follow the pattern established by `.claude/skills/game-mechanics-designer/SKILL.md`. The skill should have:

**Frontmatter / description block:**
```
---
name: prefab-designer
description: Use when the user asks to "design a prefab", "create an encounter", "audit the prefab catalog", "brainstorm dungeon rooms", "fill gaps in prefabs", or discusses prefab layout, tactics, or encounter design for The Veiled Tyrant.
---
```

**Reference loading instructions:**
At the start of both workflows, read:
- `references/prefab-schema.md` (this skill's directory)
- `references/encounter-design-principles.md` (this skill's directory)
- `assets/prefabs.ron` (live catalog)
- `assets/props.ron` (valid prop names)
- `assets/structures.ron` (valid structure names)
- `assets/monsters.ron` (factions, roles, depth ranges)

**Workflow 1: Design a Prefab**

Guided conversation through these steps (one question at a time):

1. **Read current state** — Parse `assets/prefabs.ron`. Summarize current catalog count by tier, placement distribution, role coverage, depth ranges.
2. **Encounter fantasy** — Ask what the player experience should feel like. Offer archetypes from the design principles reference if inspiration is needed.
3. **Tactical geometry** — Sketch 2-3 ASCII layout variants with commentary on sight lines, chokepoints, cover, approach angles. Each variant should play differently.
4. **Monster composition** — Propose roles, count, faction considerations. Reference valid roles from schema. Consider faction locking via `faction_tag` or leaving open.
5. **Squad behavior** — Recommend `on_leader_death` and `flee_threshold` using the drama table from design principles. Decide guard vs. roam for each monster.
6. **Loot & structures** — Propose reward density. Reference valid props and structures from schema. Match risk to reward.
7. **Placement strategy & size** — Recommend placement type and dimensions. Reference size categories and budget from schema.
8. **Depth range & difficulty tier** — Place in the 26-floor progression. Reference existing depth coverage to avoid dead zones.
9. **Orientation** — Recommend rotate/flip settings based on whether design is directional.
10. **Output** — Present design summary covering all dimensions, then generate complete RON `PrefabTemplate` entry. Validate that:
    - All field names and types match the schema
    - Monster roles, prop names, structure names are valid
    - Spawn coordinates land on floor (`.`) or door (`+`) tiles
    - Tile row count matches `height`, each row length matches `width`
    - Coordinate system: (0,0) is top-left of the tile grid

**Workflow 2: Audit Catalog**

1. **Read current state** — Parse `assets/prefabs.ron`, `assets/monsters.ron`.
2. **Analyze coverage** across 9 dimensions:
   - Tactical variety (approach patterns represented vs. missing)
   - Monster role coverage (which roles are underrepresented)
   - Depth distribution (floor ranges with few/no eligible prefabs)
   - Size distribution (small/medium/large/landmark balance)
   - Placement strategy mix (room/wall/chokepoint/landmark ratio)
   - Squad behavior variety (on_leader_death and flee_threshold distribution)
   - Terrain interaction (use of doors, water, lava)
   - Reward density (risk vs. reward balance)
   - Faction coverage (faction-locked vs. open prefabs)
3. **Present findings** — Gap analysis prioritized by gameplay impact. For each gap: what's missing, why it matters, brief encounter concept to fill it, suggested tier/depth.
4. **Optionally transition** to Workflow 1 to design a prefab filling a specific gap.

**Cross-cutting rules** (stated in SKILL.md):
- Encounter-first: always start with player experience
- Catalog awareness: never duplicate an existing prefab's tactical situation
- Validate all output against the schema reference
- One question at a time during design workflow
- Show tactical reasoning for layout choices

**Quick reference section** (inline in SKILL.md for fast access):
- Valid roles: `melee_guard`, `ranged`, `brute`, `caster`, `leader`, `any`
- Valid leader death: `scatter`, `enrage`, `fight_on`, `flee`
- Valid placement: `room`, `wall`, `chokepoint`, `landmark`, `any`
- Tile chars: `#` wall, `.` floor, `+` door, ` ` unchanged
- Size thresholds: small < 31, medium 31-99, large 100-149, landmark 150+
- Budget: 350 tiles/floor, 2-tile padding

- [ ] **Step 2: Verify the skill is discoverable**

Check that the skill description in SKILL.md matches the trigger phrases listed in the spec. Verify the file is in the correct location (`.claude/skills/prefab-designer/SKILL.md`).

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/prefab-designer/SKILL.md
git commit -m "feat(skill): add prefab-designer skill with design and audit workflows"
```

---

### Task 4: Smoke Test

Verify the skill works end-to-end.

- [ ] **Step 1: Verify file structure**

```bash
find .claude/skills/prefab-designer -type f
```

Expected output:
```
.claude/skills/prefab-designer/SKILL.md
.claude/skills/prefab-designer/references/prefab-schema.md
.claude/skills/prefab-designer/references/encounter-design-principles.md
```

- [ ] **Step 2: Verify skill discovery**

Check that the skill description would match these trigger phrases:
- "design a prefab"
- "create an encounter"
- "audit the prefab catalog"
- "brainstorm dungeon rooms"

- [ ] **Step 3: Verify references are internally consistent**

Cross-check that:
- All prop names in `prefab-schema.md` match `assets/props.ron`
- All structure names in `prefab-schema.md` match `assets/structures.ron`
- All monster roles in `prefab-schema.md` match `src/assets/mod.rs` doc comments
- The example RON entry in `prefab-schema.md` is valid syntax

- [ ] **Step 4: Final commit (if any fixes needed)**

```bash
git add -A .claude/skills/prefab-designer/
git commit -m "fix(skill): address smoke test findings in prefab-designer skill"
```
