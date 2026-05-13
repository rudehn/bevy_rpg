# Skill Writeup Must Stay in Sync

`docs/design/SKILLS.md` is the canonical writeup for the Phase 3 skill
system. Like `CHARACTER.md`, it is **test-enforced** — drift between
the asset RON files and the writeup will fail the build.

## What's checked

Maintenance tests in `src/character/asset.rs` (and a future
`src/game/skills.rs` test) guard:

- `every_class_starting_skills_sums_to_ten` — each class's
  `starting_skills` total is exactly 10
- `every_race_aptitude_value_is_in_range` — each race's `aptitudes`
  covers every `Skill` variant; each value is in −5..=+5
- `every_weapon_has_weapon_skill_or_is_staff` — every weapon-kind item
  in `items.ron` has `weapon_skill` declared, except staves which are
  intentionally skill-less (Evocations on zap, Fighting on bash)

If you add a new skill, rename one, or change shipping numbers — the
tests fire until SKILLS.md is updated.

## What to update

When you change:

- A class's starting skill distribution → update `assets/classes.ron`
  AND the matching table in SKILLS.md §2
- A race's aptitudes → update `assets/races.ron` AND SKILLS.md §3
- Skill effect formulas (`floor(skill/4)`, HP Fighting term, etc.) →
  update SKILLS.md §1 AND any related CHARACTER / PLAYER / GAME .md
  combat-math references
- Add a new `Skill` enum variant → update SKILLS.md §1 (effect table)
  AND CharacterAsset's `SkillDistribution` / `SkillAptitudes`
  helper structs in `src/character/asset.rs`
- Add a new `WeaponSkill` enum variant → update items.ron tagging
  guidance + SKILLS.md §7 weapon-to-skill mapping table

## What NOT to do

- Do **not** delete or weaken the maintenance-contract callouts in
  SKILLS.md (the `> Maintenance contract:` blockquotes). They tell
  future readers the tests exist.
- Do **not** add a separate "skills reference" doc. SKILLS.md is the
  single source of truth.
- Do **not** silently rename a `Skill` enum variant in code without
  updating the doc — `Skill::name()` is the authority for display
  strings; SKILLS.md must match.

## Related rules

- [character-writeup-required.md](character-writeup-required.md) —
  parallel rule for CHARACTER.md (race/class writeup)
- [design-docs-required.md](design-docs-required.md) — broader
  convention: every game system gets a design doc
