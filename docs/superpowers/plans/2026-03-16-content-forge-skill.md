# Content Forge Skill Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `content-forge` Claude Code skill for brainstorming, balancing, and producing new game content (monsters, items, spells, factions) in The Veiled Tyrant.

**Architecture:** Pure skill files (no Rust code). One `SKILL.md` with 5 workflows + 3 reference documents containing balance curves, RON schemas, and faction design guidance. Follows the same pattern as the existing `prefab-designer` skill.

**Tech Stack:** Claude Code skill (markdown files only)

**Spec:** `docs/superpowers/specs/2026-03-16-content-forge-skill-design.md`

---

## File Structure

```
.claude/skills/content-forge/
  SKILL.md                              # Skill frontmatter + 5 workflows + cross-cutting rules
  references/
    balance-curves.md                   # Player power table, monster stat budgets, item/spell budgets
    ron-schemas.md                      # Annotated RON format for all data files with examples
    faction-design-guide.md             # Roster template, ability distribution, existing faction analysis
```

All content is sourced from the spec and from actual RON data files / Rust source.

---

### Task 1: Create `references/ron-schemas.md`

The foundation — every other file references this for valid types and formats. Must be sourced from actual RON files and Rust enums, not approximated.

**Files:**
- Create: `.claude/skills/content-forge/references/ron-schemas.md`

**Source data (read but don't modify):**
- `assets/monsters.ron` — MonsterDef fields and examples
- `assets/monster_spawns.ron` — spawn entry format including composite groups
- `assets/items.ron` — ItemDef fields and examples by kind
- `assets/item_spawns.ron` — spawn entry format
- `assets/spells.ron` — SpellData fields and examples
- `src/game/items.rs` — ItemBonus enum (all variants)
- `src/game/abilities.rs` — OnHitEffect, passive ability components, FactionKind
- `src/game/spells.rs` — SpellEffect enum, SpellTarget enum
- `src/game/combat.rs` — DamageType enum, ResistanceLevel enum
- `src/game/effects.rs` — Effect enum (consumable effects)

- [ ] **Step 1: Create the ron-schemas.md file**

Write the complete RON schema reference document with these sections:

1. **MonsterDef Schema** — All fields with types, optionality, and defaults. Include `sprite`, `is_cowardly`, `damage_type`, `faction_tag`, `role`, `on_hit_effects`, `resistances`, `spells`, `loot_table`, passive abilities (`poison_body`, `reanimate_hp`, `enrage_on_hit`, `explode_on_death`, `thorn_aura`, `death_curse`, `summon_on_death`). Include 2 annotated examples (one simple melee monster, one complex caster).

2. **MonsterSpawnEntry Schema** — Both simple format (`monster`, `min_floor`, `max_floor`, `min_group`, `max_group`, `on_leader_death`, `flee_threshold`) and composite `group` format. Include examples of both.

3. **ItemDef Schema** — All fields including rendering (`sprite`, `tile_size`, `grid_size`), `item_kind`, `damage`, `weapon_range`, `defense`, `armor_slot`, `agi_bonus`, `bonuses`, `rarity`, `effect`, `max_stack`, `is_ammo`. Include examples for each item kind (Weapon, Armor, Ring, Amulet, Consumable, Spellbook).

4. **ItemSpawnEntry Schema** — `item`, `min_floor`, `max_floor`, `rarity`, optional `min_count`/`max_count`.

5. **SpellData Schema** — `name`, `mana_cost`, `cooldown`, `description`, `target`, `range`, `effects`, `damage_type`. Include examples for attack, heal, buff, and summon spells.

6. **Valid Enum Values** — Complete listing of:
   - `ItemKind`: Consumable, Weapon, Armor, Ring, Amulet, Spellbook
   - `ArmorSlot`: Chest, Helm, Gloves, Boots, OffHand
   - `Rarity`: Common, Uncommon, Rare, Legendary
   - `SpellTarget`: Castor, Enemy, Ally, AllyOrSelf
   - `DamageType`: Physical, Fire, Lightning, Necrotic (in RON: Ice, Poison also used in design docs but verify in source)
   - `ResistanceLevel`: Weak, Normal, Resistant, Immune, Absorb
   - `MonsterRole`: melee_guard, ranged, brute, caster, leader, any
   - `OnLeaderDeath`: scatter, enrage, fight_on
   - All `ItemBonus` variants with parameter types
   - All `SpellEffect` variants with parameter types
   - All `OnHitEffect` variants with parameter types
   - All `Effect` variants (consumable effects)

All examples must be copied from actual data files, not fabricated.

- [ ] **Step 2: Verify the file exists and is well-formed**

```bash
cat .claude/skills/content-forge/references/ron-schemas.md | head -5
wc -l .claude/skills/content-forge/references/ron-schemas.md
```

Expected: File exists, starts with `# RON Schema Reference`, 300+ lines.

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/content-forge/references/ron-schemas.md
git commit -m "feat(content-forge): add RON schema reference from actual data files"
```

---

### Task 2: Create `references/balance-curves.md`

Power curves and stat budgets the skill uses to assign balanced numbers.

**Files:**
- Create: `.claude/skills/content-forge/references/balance-curves.md`

**Source data (read but don't modify):**
- `docs/design/BESTIARY.md` — Monster stat formulas, zone breakdown
- `.claude/skills/game-mechanics-designer/references/balance-parameters.md` — All tunable parameters
- `assets/monsters.ron` — Actual monster stats for reference data points
- `docs/design/ITEMS.md` — Item power budget info
- `docs/design/MAGIC.md` — Spell cost/power info

- [ ] **Step 1: Create the balance-curves.md file**

Write the balance curves reference with these sections, sourcing all numbers from the spec and verifying against actual data:

1. **Player Power by Floor** — Table from spec (floors 1/5/10/15/20 with HP/DPS/Armor/Mana ranges)

2. **Monster Stat Budgets** — Including:
   - Two-stage HP formula: `final_hp = base_hp + (CON_bonus * level)`
   - Damage targets: 15-25% player HP early, 30-50% late
   - Attribute guidance: 10 baseline, INT=0 for non-casters, PER 6-14
   - AGI → delay formula with fast/normal/slow guidance
   - Armor ranges by floor bracket
   - XP formula: `10 + (level * 5) + (base_hp / 2)`

3. **Reference Data Points** — Table from spec with corrected values:
   ```
   | Monster         | Level | base_hp | Damage   | AGI | Final HP |
   |-----------------|-------|---------|----------|-----|----------|
   | Rat             | 1     | 8       | 1d4      | 14  | ~6       |
   | Goblin          | 1     | 10      | 1d4      | 10  | ~10      |
   | Skeleton        | 5     | 22      | 1d6      | 8   | ~22      |
   | Orc             | 8     | 32      | 1d8+1    | 10  | ~48      |
   | Orc Berserker   | 10    | 45      | 1d10+2   | 10  | ~85      |
   | Shadow Fiend    | 14    | 55      | 1d10+2   | 12  | ~83      |
   | Dark Knight     | 18    | 75      | 2d8+4    | 8   | ~183     |
   | Veiled Tyrant   | 20    | 200     | 2d8+4    | 12  | ~440     |
   ```

4. **Item Power Budgets** — By rarity (Common through Legendary), weapon damage dice, armor values, bonus count and percentage ranges.

5. **Spell Power Budgets** — Cantrip/Standard/Powerhouse tiers with mana cost, cooldown, and damage dice ranges.

6. **Spawn Density Guidelines** — Monsters and items per floor bracket.

7. **Floor Scope Note** — Current content spans floors 1-20. The OVERVIEW describes 26 floors but 21-26 are future content.

- [ ] **Step 2: Verify the file**

```bash
cat .claude/skills/content-forge/references/balance-curves.md | head -5
wc -l .claude/skills/content-forge/references/balance-curves.md
```

Expected: File exists, starts with `# Balance Curves`, 150+ lines.

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/content-forge/references/balance-curves.md
git commit -m "feat(content-forge): add balance curves reference with verified data points"
```

---

### Task 3: Create `references/faction-design-guide.md`

Guidance for the Generate Faction workflow, including analysis of existing factions as exemplars.

**Files:**
- Create: `.claude/skills/content-forge/references/faction-design-guide.md`

**Source data (read but don't modify):**
- `docs/design/BESTIARY.md` — Faction design rationale, role synergies, zone breakdown
- `assets/monsters.ron` — Actual faction rosters
- `assets/monster_spawns.ron` — Spawn configurations including composite groups

- [ ] **Step 1: Create the faction-design-guide.md file**

Write the faction design guide with these sections:

1. **Roster Template** — Every faction needs: fodder (1-2), standard (1-2), elite (1), optional boss candidate. Explain the purpose of each tier.

2. **Mechanical Identity** — How to pick 1-2 signature mechanics. Examples from existing factions:
   - Goblinoid: numbers + scatter on leader death
   - Undead: Reanimate + Necrotic damage
   - Vermin: fast + cowardly + poison
   - Demonic: fire damage + high individual power
   - Orcish: brute force + enrage on leader death

3. **Role Synergies** — How roles interact within squads: caster behind melee_guard, ranged on flanks, leader buffing nearby allies. Reference the squad system mechanics.

4. **Ability Distribution** — Shared traits (all members get X) vs. specialist (only elite gets Y). When to use each approach.

5. **Themed Loot Principles** — Items should reflect faction identity. Goblin weapons crude but fast, undead items necrotic-themed, etc.

6. **Floor Range Sizing** — A faction should span 6-10 floors for adequate exposure. Reference existing faction floor ranges.

7. **Existing Faction Analysis** — For each of the 8 current factions (Vermin, Goblinoid, Undead, Orcish, Demonic, Giant, Dark, Boss), document:
   - Floor range
   - Roster members and roles
   - Signature mechanics
   - Squad behaviors (on_leader_death, flee_threshold)
   - What makes them tactically distinct

Source from both `monsters.ron` data and `BESTIARY.md` design rationale.

- [ ] **Step 2: Verify the file**

```bash
cat .claude/skills/content-forge/references/faction-design-guide.md | head -5
wc -l .claude/skills/content-forge/references/faction-design-guide.md
```

Expected: File exists, starts with `# Faction Design Guide`, 200+ lines.

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/content-forge/references/faction-design-guide.md
git commit -m "feat(content-forge): add faction design guide with existing faction analysis"
```

---

### Task 4: Create `SKILL.md`

The main skill file with frontmatter, workflows, and cross-cutting rules.

**Files:**
- Create: `.claude/skills/content-forge/SKILL.md`

**Source data:**
- `docs/superpowers/specs/2026-03-16-content-forge-skill-design.md` — The spec (primary source)
- `.claude/skills/prefab-designer/SKILL.md` — Pattern to follow for frontmatter and structure

- [ ] **Step 1: Create the SKILL.md file**

Write the complete skill file with these sections, following the spec exactly:

**Frontmatter** (YAML between `---` delimiters):
```yaml
---
name: Content Forge
description: >
  Use when the user asks to "create a monster", "design an item",
  "brainstorm a spell", "generate a faction", "what monsters are missing",
  "fill gaps in the bestiary", "add a new enemy type", "design loot",
  or wants to brainstorm, balance, and produce new game content
  (monsters, items, spells, factions) for The Veiled Tyrant.
---
```

**Body sections:**

1. **Title & intro paragraph** — What the skill does, one sentence.

2. **Before Starting Any Workflow** — List of reference files to read:
   - `references/balance-curves.md`
   - `references/ron-schemas.md`
   - `references/faction-design-guide.md` (faction workflow only)
   - Live game data files to read per workflow

3. **Workflow 1: Create Monster** — Steps 1-8 from spec:
   - Read current state (monsters.ron, monster_spawns.ron)
   - Monster fantasy (archetype list)
   - Stat assignment (using balance-curves reference)
   - Abilities & resistances (validate against ron-schemas)
   - Squad role & behavior (cowardly, flee, on_leader_death)
   - Spawn configuration (simple + composite group formats)
   - Sprite assignment
   - Approve & write (present summary + RON, append on approval)

4. **Workflow 2: Create Item** — Steps 1-6 from spec:
   - Read current state (items.ron, item_spawns.ron)
   - Item fantasy (kind, rarity, identity)
   - Core stats
   - Bonus selection (validate against ron-schemas)
   - Spawn configuration (include rendering fields)
   - Approve & write

5. **Workflow 3: Create Spell** — Steps 1-6 from spec:
   - Read current state (spells.ron)
   - Spell fantasy (role, targeting, frequency)
   - Effect design (validate against ron-schemas)
   - Cost & cooldown balance
   - Monster access
   - Approve & write (chain to Create Item for spellbook if needed)

6. **Workflow 4: Generate Faction** — Steps 1-7 from spec:
   - Read current state (all data files)
   - Faction identity (theme, floor range, personality)
   - Roster design (fodder/standard/elite/boss tiers)
   - Faction abilities (signature mechanics)
   - Themed loot (chain to Create Item)
   - Faction spells (chain to Create Spell)
   - Sequential approval & write

7. **Workflow 5: Audit Gaps** — Steps 1-4 from spec:
   - Read all data files including essence_nodes.ron and BESTIARY.md
   - Analyze coverage (7 dimensions from spec)
   - Present prioritized recommendations
   - Optionally chain into creation workflow

8. **Cross-Cutting Rules** — All 6 rules from spec:
   - Always read current state first
   - One question at a time
   - Validate against implemented types
   - Present before writing
   - Append, don't overwrite
   - Chain when appropriate

- [ ] **Step 2: Verify the file**

```bash
cat .claude/skills/content-forge/SKILL.md | head -12
wc -l .claude/skills/content-forge/SKILL.md
```

Expected: File exists, starts with `---` frontmatter, 300+ lines.

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/content-forge/SKILL.md
git commit -m "feat(content-forge): add main skill file with 5 workflows"
```

---

### Task 5: Verify skill loads and test discovery

Verify the skill is correctly structured and discoverable.

**Files:**
- Read: `.claude/skills/content-forge/SKILL.md`
- Read: `.claude/skills/content-forge/references/ron-schemas.md`
- Read: `.claude/skills/content-forge/references/balance-curves.md`
- Read: `.claude/skills/content-forge/references/faction-design-guide.md`

- [ ] **Step 1: Verify all 4 files exist**

```bash
ls -la .claude/skills/content-forge/SKILL.md
ls -la .claude/skills/content-forge/references/
```

Expected: SKILL.md + 3 reference files present.

- [ ] **Step 2: Verify SKILL.md frontmatter is valid YAML**

Check the frontmatter has proper `---` delimiters, `name:` and `description:` fields.

```bash
head -10 .claude/skills/content-forge/SKILL.md
```

Expected: Starts with `---`, has `name: Content Forge` and `description:` fields, ends with `---`.

- [ ] **Step 3: Verify reference files are referenced correctly in SKILL.md**

Check that the "Before Starting" section in SKILL.md references the correct relative paths:
- `references/balance-curves.md`
- `references/ron-schemas.md`
- `references/faction-design-guide.md`

```bash
grep -c "references/" .claude/skills/content-forge/SKILL.md
```

Expected: Multiple matches (at least 3).

- [ ] **Step 4: Verify RON schema examples match actual data**

Spot-check that example RON entries in `ron-schemas.md` match actual entries in the data files. Read a monster from `assets/monsters.ron` and compare against the example in the schema doc.

- [ ] **Step 5: Final commit if any fixes needed**

If any issues found, fix and commit:
```bash
git add .claude/skills/content-forge/
git commit -m "fix(content-forge): address verification issues"
```
