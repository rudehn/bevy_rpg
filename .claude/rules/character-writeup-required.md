# Character Writeup Must Stay in Sync

`docs/design/CHARACTER.md` is the single canonical writeup of what each
race and class give the player. It is **enforced by tests**, not by
convention — when you change `assets/races.ron`, `assets/classes.ron`,
or the `RaceTrait` enum, the writeup must be updated in the same
change.

## What's checked

Two tests in [src/character/asset.rs](../../src/character/asset.rs)
guard the contract:

- `character_md_documents_every_shipping_race` — every race name in
  `races.ron` must appear in `CHARACTER.md`, and every `RaceTrait`
  keyword (Versatile, Stoneblood, Keen Senses, Lucky) must appear.
- `character_md_documents_every_shipping_class` — every class name in
  `classes.ron` must appear in `CHARACTER.md`, and the class's
  `base_hp` value must appear in the same markdown row as the class
  name.

If you add a new race, rename one, change a trait keyword, or change a
class's base HP — these tests fail until `CHARACTER.md` is updated.

## What to update

The `## Races` and `## Classes` sections of `CHARACTER.md` each contain:

1. **A table** with one row per race/class — full attribute spread,
   trait or starting kit, etc.
2. **A "Playstyle at a glance" block** — short prose summary of what
   the choice means for the player's run.
3. (Races only) **Implementation notes** linking to the runtime
   systems that apply the trait.

When you change shipping content, update all three for the affected
row. Don't just update the table — the playstyle block describes the
*decision*, not the data, and it can rot quickly.

## What NOT to do

- Do **not** delete or weaken the maintenance-contract callout block
  inside CHARACTER.md (the `> Maintenance contract:` blockquote). It
  tells future readers the tests exist.
- Do **not** rename a `RaceTrait` enum variant without also updating
  the hardcoded keyword list in
  `character_md_documents_every_shipping_race`. The test catches the
  doc side; the enum-list update is part of the same change.
- Do **not** add a separate "race reference" or "class reference" doc.
  CHARACTER.md is the single source of truth — splitting it spreads
  drift surface area without making anything clearer.

## Related rules

- [design-docs-required.md](design-docs-required.md) — broader rule
  about every game system having a design doc. The race/class writeup
  is a refinement of that rule with stronger enforcement.
