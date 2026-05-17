# Systems Should Be AI-Friendly

When designing a new game system or refactoring an existing one,
optimise for the case where an AI agent (or a human dropping in cold)
needs to understand the whole system in one sitting. The codebase is
explicitly maintained as a substrate for AI-assisted development — if
a system can only be understood by reading 4+ files and grepping for
implicit conventions, it needs deepening before it ships.

## What this means in practice

1. **One concept, one file** — when something has a name (the combat
   resolver, the screen registry, the floor builder pipeline),
   somebody should be able to open ONE file and learn the contract.
   Counter-pattern: spreading the "items domain" across
   `items.rs`, `staves.rs`, `enchantment.rs`, and `actions.rs` so
   adding a sword tag has no clear home.

2. **Explicit contracts at module boundaries** — prefer a small,
   typed interface (a struct of inputs, a struct of outputs, one or
   two entry-point functions) over a sprawl of free functions that
   each query the ECS. The combat resolver in [src/game/combat/resolve.rs](../../src/game/combat/resolve.rs)
   is the reference shape: pure Rust, no Bevy imports, snapshots in,
   outcome out. The UI screen registry in [src/ui/registry.rs](../../src/ui/registry.rs)
   is the same idea applied to UI state.

3. **Default to pure functions; isolate the Bevy adapter** — combat
   math, dice formulas, resolution logic should live in plain Rust
   modules that take data structs and an injectable RNG. The Bevy
   system becomes a thin "gather snapshot → call pure code → write
   events" loop. This is what made the combat refactor possible.
   Counter-pattern: 350-line systems that compute, query, and emit
   inline so the math is unreachable from a unit test.

4. **No `Default` for snapshots / inputs that carry production state**
   — when a snapshot type aggregates "everything the math needs",
   force explicit construction at the adapter boundary. A `Default`
   impl hides a "I forgot to copy `attrs`" wiring bug as silent
   zero-bonus damage. Use a named constructor or a builder. The
   resolver's `AttackerSnapshot` / `DefenderSnapshot` deliberately
   omit `Default` for this reason.

5. **Detect drift at startup or compile time, not in playtest** — if
   two screens claim the same hotkey, two RON entries share an ID,
   two enum variants would collide on serialisation, panic at app
   startup with a clear message. The
   `detect_screen_key_collisions` system in [src/ui/registry.rs](../../src/ui/registry.rs)
   is the pattern. Tests that assert "this RON deserialises" are
   weaker than tests that assert "this map of (key → variant) has no
   duplicates."

6. **Adding one entry should be one place to edit** — when a system
   grows new "content" (a new screen, a new builder step, a new
   damage type), the cost should be proportional to the entry's
   complexity, not the system's total surface. If a new screen
   requires touching 5 files, the system has a missing registry.

7. **Match the engine boundary deliberately** — `roguelike_engine`
   owns universal physics (armor subtraction, resistance, status
   tick, map mutation). Game owns content + game-specific reactions.
   When in doubt, the engine doesn't import from the game; the game
   imports from the engine. The
   [docs/rfcs/0001-combat-resolver.md](../../docs/rfcs/0001-combat-resolver.md)
   migration is the worked example.

## How to validate before merging a new system

Read the system back as if you'd never seen the codebase. Ask:

- Can I describe what this system does in two sentences from one
  file?
- If I add a new entry of the same shape, do I touch one file or
  five?
- Can I unit-test the math without spinning up a Bevy `App`?
- If a future change breaks an invariant, will it fail loudly at
  startup or silently at runtime?
- Is the design doc in `docs/design/` linked from CLAUDE.md, and
  does it describe the *current* shape (not the original plan)?

If three or more of those answers are "no" or "kind of," the system
needs another pass before it lands.

## Related rules

- [design-docs-required.md](design-docs-required.md) — every system
  gets a design doc; CLAUDE.md reflects architectural changes
- [testing-requirements.md](testing-requirements.md) — pure functions
  get unit tests; extract testable helpers from Bevy systems
- [save-load-checklist.md](save-load-checklist.md) — concrete example
  of a multi-file checklist that should ideally be replaced by a
  registry someday
