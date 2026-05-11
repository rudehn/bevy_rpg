---
paths:
  - "src/**"
---

# Update Docs and Skills with New Systems

When a new game **system** or **mechanic** is added (not just a new
content entry), update:

1. **Design docs** (`docs/design/*.md`) — document the new system's
   mechanics, stats, and interactions.

2. **Skill references**, in this priority order:
   - `.claude/skills/content-studio/references/ron-schemas.md` — if the
     system introduces a new asset type or changes an existing schema.
   - `.claude/skills/content-studio/references/balance-targets.md` — if
     the system has tunable numeric ranges (damage, cooldowns, weights).
   - `.claude/skills/content-studio/references/design-principles.md` —
     if the system forces a new "resolved decision" or changes a pillar.
   - `.claude/skills/content-studio/references/entity-patterns.md` — if
     the system introduces a new archetype or changes existing ones.

## What NOT to update the skill for

Adding a single new monster, item, staff, runic variant, or amulet does
**NOT** require updating the skill. The content-studio skill reads live
asset files via `scripts/content_index.sh` at invocation, so it
automatically picks up new content.

Only update skill files when the *shape* of content changes:
- New RON field on an existing struct
- New enum variant (Species, WeaponRunic, ExplodeEffect, etc.)
- New balance curve formula (e.g., runic_chance_for_floor)
- New content type category (traps, shrines, altars)
- A resolved design decision being added or overturned

This keeps the skill useful without constant manual bookkeeping.
