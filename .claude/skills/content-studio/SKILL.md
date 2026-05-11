---
name: Content Studio
description: This skill should be used when the user asks to "add a monster", "design an item", "create a trap", "add a weapon", "design an ability", "add a runic", "add a staff", "add an amulet", "create a faction", "balance X", "tune Y", "re-tune the difficulty curve", "audit content", "what's missing from floor N", "find content gaps", "propose stats for", or any workflow that adds, removes, balances, or audits game content for The Veiled Tyrant (monsters, items, traps, abilities, runics, staves, amulets, factions, mechanics). Also triggers on edits to `assets/*.ron` and questions about whether existing content is tuned correctly.
---

# Content Studio

Design, balance, and audit content for The Veiled Tyrant — a Brogue-inspired
26-floor roguelike. Covers the full lifecycle: adding new content, tuning
existing values, and finding gaps in the roster.

## Core principle: live data, stable workflows

The game's content is the source of truth. This skill's *workflows* are
stable; its *data* is always read fresh from asset files and Rust source.
Never trust a reference table for the current state of the game. Always
orient with a live scan first.

## Step 0: Orient (required before every workflow)

Run the content index script to see what currently exists:

```bash
bash .claude/skills/content-studio/scripts/content_index.sh
```

This prints a terse summary of:
- Active monsters grouped by faction
- Floor coverage gaps (which floors have no unique content)
- Item counts by category and rarity
- Active faction hostility pairs
- Design docs that exist under `docs/design/`

For deeper context, read the relevant RON file directly:
- `assets/monsters.ron` — monster definitions
- `assets/monster_spawns.ron` — per-floor spawn table
- `assets/items.ron` — item definitions
- `assets/item_spawns.ron` — per-floor item spawn weights
- `assets/factions.ron` — faction hostility matrix
- `assets/decorations.ron`, `assets/props.ron`, `assets/tiles.ron` — terrain

Never skip this step. The skill's reference tables describe *how to reason*,
not *what exists*.

## Workflow selection

| User said | Workflow |
|---|---|
| "Add a monster" / "create a new X" | Add Entity |
| "Balance the X" / "tune Y" / "is X overpowered" | Tune |
| "What's missing from floor N" / "audit gaps" | Audit |
| "Design a new ability / runic / staff effect" | Add Mechanic |
| "Add traps" / "design the trap system" | Add Content Type |

## Workflow 1: Add Entity

Adding an instance of an existing content type (monster, item, staff, ring,
amulet, runic, faction, decoration, prop). Entity schemas already exist;
only data gets added.

### 1. Identify the slot
- What role does this entity fill? (See `references/entity-patterns.md` for
  archetypes: brute, glass cannon, caster, ambusher, summoner, support, etc.)
- What floor range is it targeting?
- What faction does it belong to (if any)?
- What gap does it fill vs. existing content? (From Step 0's index output.)

### 2. Propose stats
Use `references/balance-targets.md` for floor-appropriate HP/damage/speed
ranges. Cross-reference the stats of existing monsters in the same floor
band — a new floor-5 creature should not dwarf an existing floor-8 one.

Mandatory fields per entity kind are in `references/ron-schemas.md`. For
monsters, always declare a `species:` — missing it defaults to `Unknown`
and logs a warning on load.

### 3. Give it identity
Every new entity must answer: *what tactical lesson does this teach the
player?* A Bloat teaches "explosive enemies exist." A Pit Bloat teaches
"terrain can be destroyed." Without a mechanic, the entity is filler.

If the entity only differs from existing content by +1/-1 numbers, cut
it and tune the existing one instead.

### 4. Draft the RON entry
Mirror the style of nearby entries in the same file. Keep field order
consistent. Fields that default sensibly can be omitted.

### 5. Wire the spawn
Add a row to the matching `*_spawns.ron` file with floor range and weight.
For monsters, choose between a single entry or a group entry (mixed pack)
based on the archetype.

### 6. Update the design doc
Every new system or content entry requires the matching `docs/design/*.md`
to reflect the addition. Monsters → `ENEMIES.md`. Items → `ITEMS.md`.
Traps → `DUNGEON.md` (or new `TRAPS.md`).

Per project rule `.claude/rules/design-docs-required.md`, this is not
optional.

### 7. Sprite
Per `.claude/rules/placeholder-sprites.md`, never reuse an existing sprite
as a placeholder. Generate a unique PNG or explicitly flag the asset as
needed. ASCII-only content can skip this (confirm renderer path first).

## Workflow 2: Tune

Adjusting values on an entity or system that already exists.

### 1. Establish the problem
Ask: what outcome is off? Is the entity dying too fast, killing too fast,
spawning too often, doing too little damage? Get the concrete complaint
before touching numbers.

### 2. Locate the knob
- Monster stats: `assets/monsters.ron`
- Monster spawn frequency: `assets/monster_spawns.ron`
- Item stats / effects: `assets/items.ron`
- Item drop frequency: `assets/item_spawns.ron`
- Weapon abilities: `src/game/combat.rs` (Backstab on Dagger, Cleave on Axe — Sword is the no-ability balance baseline)
- Runic proc rates: `src/game/enchantment.rs::base_rate`
- Staff recharge / damage: `src/game/staves.rs`
- Cooldown / on-hit abilities: `src/game/abilities.rs`
- Combat formulas: `src/game/combat.rs`
- Difficulty curves: `src/game/actions.rs::rarity_weights_for_floor`,
  `src/game/enchantment.rs::runic_chance_for_floor`

### 3. Model the change
Before editing, reason through the floor ramp:
- Floor 1–4 (early): player has minimal gear. Can they survive the new value?
- Floor 5–10 (mid): player has enchant scrolls and runics starting to land.
- Floor 11–20 (late): high enchantment levels and built-out kits.
- Floor 21–26 (end): amulet run — ascending with the Amulet of Yendor,
  all floors restored from cache. Any tuning change must survive both
  the descent and the ascent through the same content.

### 4. Apply the change + update the test
If the change touches a formula, the test for that formula must be
updated (per `.claude/rules/testing-requirements.md`). If the change is
just a numeric dial, no test is required but a quick simulation / dry-run
assertion is still good practice.

### 5. Document
Short note in the relevant design doc explaining *why* the number changed,
not just that it did. Commit messages should cite the problem statement
from Step 1.

## Workflow 3: Audit

Finding gaps in the roster or imbalances.

### 1. Scan
Run Step 0's script. Look at:
- Floors with 0 unique monsters
- Factions with 0 active members
- Damage types underrepresented (e.g., no lightning monsters below floor 10)
- Rarity tiers missing at a floor band

### 2. Compare to design docs
`docs/design/ENEMIES.md` has faction presence tables. Flag any gap between
design intent and reality.

### 3. Prioritize
Gaps are not all equal. Content breadth on floors 7–26 is currently the
single biggest fun-killer for this project (see past evaluations). Propose
fixes in impact order, not alphabetical.

### 4. Report, don't implement
Audit is a survey, not a sprint. Present findings as a prioritized list
and let the user pick what to build next.

## Workflow 4: Add Mechanic

Designing a new system — an ability variant, a runic, a staff effect,
a status effect, a trap type.

### 1. Check if the mechanic already exists in spirit
Read `src/game/abilities.rs` (cooldown and on-hit abilities),
`src/game/enchantment.rs` (runics), `src/game/staves.rs` (staff effects),
`src/game/magic.rs` (status effects). Most "new" ability ideas already
have 70% of the machinery.

### 2. Prefer composition over new types
A new on-hit runic with a status effect can often be expressed as an
existing `WeaponRunic` variant plus an existing `StatusEffectKind`. Only
introduce a new enum variant when nothing composes.

### 3. Wire all layers
A new mechanic requires: enum variant (if needed), handler system,
spawn/attach logic, UI text for tooltip/log, save/load if stateful,
design doc entry, and unit tests. See `.claude/rules/testing-requirements.md`.

### 4. Symmetric combat check
If the mechanic applies to monsters, it must also be usable by the
player (or vice versa) unless there's a strong reason. The design pillar
in `docs/design/GAME.md` is *symmetric combat* — both sides share rules.

## Workflow 5: Add Content Type

Introducing a brand-new content category (e.g., traps, shrines, altars).
This is the most involved workflow — combines schema design, system
implementation, and data population.

### Trap-specific guidance
Traps don't exist yet in the project. When the user asks to add them,
read `references/trap-design.md` first — it proposes the schema, placement
strategy, and integration with existing systems (decorations, on-step
triggers, FOV hiding).

### General flow for any new content type
1. Design the data schema. Propose an asset type (RON file structure)
   and a Rust struct/component.
2. Find the existing subsystem that most naturally hosts the new thing.
   Traps are a form of tile decoration with a trigger; they belong
   alongside `Decoration` + the tile mutation pipeline, not as a separate
   entity system.
3. Draft a minimal schema first. Three example instances. Avoid
   over-engineering the schema for imagined future variants.
4. Implement the handler system. Add save/load support.
5. Write the design doc (`docs/design/TRAPS.md` etc.) before or
   alongside the code.
6. Add entries to spawn tables / map builders as needed.

## When to refuse

Refuse or push back when:
- User asks to add a mechanic that contradicts a core design pillar from
  `docs/design/GAME.md`. Surface the conflict; let them choose.
- User asks to add content that duplicates existing content without new
  tactical meaning. Propose tuning the existing entry instead.
- User requests numeric changes without a stated problem. Ask what the
  symptom is first. Tuning without a target is noise.
- User wants to add a whole new system (crafting, classes, XP, mana)
  that has been explicitly rejected in `docs/design/GAME.md`'s
  "Resolved Decisions" section. Surface the rejection.

## Related skills

- **`game-mechanics-designer`** (user-global) — generic balance framework.
  Reuse its vocabulary and pillars; this skill is the project-specific
  implementation.
- **`prefab-designer`** — for hand-designed room layouts. Not content in
  the bestiary/loot sense.

## Additional Resources

### Script

- **`scripts/content_index.sh`** — live scan of current content. Always
  run before workflows. Outputs monster roster, spawn gaps, item tiers,
  faction hostility pairs, and design-doc coverage.

### References

- **`references/design-principles.md`** — core pillars, when to add vs. tune
  vs. refuse. Read once per session; consult when a design tradeoff arises.
- **`references/balance-targets.md`** — floor-by-floor HP/damage/armor/speed
  targets for monsters and items. Consult during every stat proposal.
- **`references/ron-schemas.md`** — current RON schemas for every asset
  type. Consult when drafting new entries or when unsure what fields exist.
- **`references/entity-patterns.md`** — archetype templates (brute, glass
  cannon, caster, ambusher, summoner, support, leader). Pick one as the
  scaffold for a new monster.
- **`references/trap-design.md`** — proposed trap system. Not yet
  implemented; read when the user asks to add traps.
