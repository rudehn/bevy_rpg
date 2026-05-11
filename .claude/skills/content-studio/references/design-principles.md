# Design Principles

Core pillars and decision rules for content design in The Veiled Tyrant.
Consult when a tradeoff arises; not meant to be re-read every session.

## The pillars (from `docs/design/GAME.md`)

1. **Exploration first** — Secrets, variety, environmental storytelling.
2. **Risk vs. reward** — Every decision has cost.
3. **Emergent builds** — No classes; identity from weapons, staves, and
   enchantments found in-run.
4. **Readable danger** — Enemies telegraph threat level.
5. **Symmetric combat** — Player and monsters share the stat system and
   formulas.
6. **Resource scarcity** — Enchant scrolls are the core strategic decision.
   Staves have charges. Potions are finite.

Every content proposal must be checkable against these six. If a proposal
breaks one, surface the conflict and let the user decide.

## Resolved decisions (never quietly reintroduce)

Several systems have been explicitly rejected. Do not propose content
that relies on them:

- **No mana system.** Player magic comes from staves (Brogue-style charges).
  Do not propose mana pools, mana potions, or mana-scaling spells.
- **No player spellbooks / spells.** Staves and monster cooldown abilities
  replace the old spell system. `ItemKind::Spellbook` was deleted.
- **No XP or levels.** Power comes from items + enchant scrolls only.
- **No item identification.** What you see is what you get.
- **No cursed items (yet).** The TODO has "cursed runics" but they are
  explicitly out of scope until actively pulled in.
- **No shops.** All loot comes from chests, placed deliberately by builders.
- **No carry weight.** Inventory is 20 slots, stacking applies only to
  consumables.
- **No stat requirements on gear.** Anyone can wield anything.
- **No armor speed penalties by default.** The single exception is
  Tower Shield (`delay_modifier: 0.1`) — a Rare item that deliberately
  trades tempo for defense.
- **No classes, races, or attributes.** Monsters and the player use direct
  values; no STR/DEX/CON/AGI derivation.
- **Abilities are cooldown-based, not mana-based.** Monster and player
  abilities both key off cooldown turns.

## When to add content

Add a new entity only if it answers **yes** to at least one:

- Does it fill a floor band with 0 unique monsters / items?
- Does it introduce a mechanic the player hasn't seen yet?
- Does it combine existing mechanics in a new tactical pattern?
- Does it replace a stub / stale entry?

If none apply, the correct action is usually to *tune an existing entry*,
not add another.

## When to tune

Tune when:
- Concrete symptom reported ("this monster kills the player on floor 5
  in 2 hits").
- Gap exists between design-intent doc and code.
- Curve was written for old scale (e.g., 10-floor tuning on a 26-floor
  dungeon).

Do not tune without a stated symptom. "Let's balance this" with no
problem attached produces drift.

## When to refuse

Push back, don't implement, when:

- Request contradicts a resolved-decision pillar. Surface it.
- Request duplicates existing content with no new mechanic. Propose tuning.
- Request introduces a new content type without a schema proposal or
  design-doc pass first.
- Request bypasses `.claude/rules/` rules (testing-requirements,
  design-docs-required, placeholder-sprites, save-load-checklist,
  update-docs-and-skills).

## Scope discipline

The dungeon is 26 floors. The *biggest* fun-killer identified in past
evaluations is thin floor 7–26 content, not missing mechanics. When in
doubt between "add a new ability system" and "activate an existing
faction on mid floors," the latter nearly always wins on fun-per-effort.

## Rule of three

Before adding a fourth variant of anything (fourth weapon runic element,
fourth summon-type ability, fourth movement-mode enum), check whether
the first three already cover the tactical space. If a fourth is
"similar but slightly different," cut it.

## Tactical identity check

Every new entity must pass: "In one sentence, what lesson does this
teach the player?" If the answer is "it's like X but bigger," cut it.

Good identity sentences from current content:
- *Bloat*: "Some enemies detonate on contact — range matters."
- *Pit Bloat*: "Terrain can be destroyed beneath you."
- *Rat Broodmother*: "Kill the summoner first."
- *Kobold Hoarder*: "Some enemies fight over loot, not you."
- *Spectral Blade*: "Summons can outlast their caster."

Bad identity sentences:
- "It's a stronger goblin."
- "It does poison instead of fire."
- "It has more HP than the last one."
