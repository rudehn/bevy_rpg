---
paths:
  - "src/**"
  - "docs/design/**"
  - "CLAUDE.md"
---

# Design Docs Required

**Every game system must have a design doc in `docs/design/`, and `CLAUDE.md`
must reflect any architectural change to those systems.** No exceptions.

## When you create or modify a game system

A "system" is anything with mechanics: combat, AI, items, squads, FOV,
chasms, factions, enchanting, traps, stealth, save/load, progression,
rendering — anything where future developers (or future-Claude) need to
understand the rules.

After implementing or modifying any system, **in the same change**:

1. **Design doc.** Check `docs/design/` for the matching `.md`. If
   missing, create one. If present, update it. Each doc should cover:
   - Design philosophy / why the system exists
   - Data model (components, resources, messages)
   - Configuration knobs (RON fields, tunable constants)
   - System-flow (what runs when, in what order)
   - Edge cases and resolved decisions
   - Cross-links to related docs

2. **CLAUDE.md.** Open `/Users/nathanrude/Development/bevy_rpg/CLAUDE.md`
   and confirm:
   - Project structure section lists the new module / file
   - Key Architectural Patterns section describes any new pattern, flow,
     or invariant the system introduces
   - Any new resolved decision goes here
   If the system is purely an extension of an existing pattern (one
   more monster, one more item kind variant), CLAUDE.md does not need
   an update — only the design doc.

3. **Skill references** (per `update-docs-and-skills.md`) — if the
   change introduces a new schema field, balance curve, or pillar.

## Documentation gaps for existing systems

If you discover an existing system that has *no* design doc, flag it
proactively. Do not silently leave it undocumented. Either create the
doc as part of the current change or note it explicitly so the user
can decide whether to address it.

## What this rule is NOT for

- Bug fixes that don't change mechanics — no doc update needed.
- Adding one more entity (a new monster, a new item) — content, not
  system. The content-studio skill handles those without doc churn.
- Refactoring without behavior change — note in commit, no doc.
