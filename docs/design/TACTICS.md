# Tactics

Per-monster, data-driven AI decision-making. Replaces the FSM mega-
dispatcher (`execute_monster_ai`) and the GOAP planner (`execute_goap`)
with one ordered list of `Tactic`s evaluated top-to-bottom each turn;
first non-`None` wins.

> **Implementation status (in-progress migration):** this doc describes
> the target architecture. As of this commit, the module scaffolding
> exists but no monster has been migrated. FSM and GOAP both still run
> in production. Phases land incrementally — see "Migration" below.

---

## Why this exists

The original AI shipped as two parallel mega-dispatchers:

- **FSM** (`execute_monster_ai`) — one 200-LOC function with a hardcoded
  sequence of branches (stun → mode update → submerge → ability →
  ranged → flee → kite → erratic → squad-leash → resolve_movement →
  wait). Every monster runs the same function; per-monster variation
  comes from boolean fields (`stationary`, `kites`, `erratic_chance`)
  that gate runtime branches inside the dispatcher. The function has
  no unit tests because it takes `&mut World` and reads 12 components.

- **GOAP** (`execute_goap`) — 2,211 LOC of framework (planner +
  WorldState + goals + actions + per-trait builders) driving 10
  monsters. The planner runs at depth 4 but in practice every "plan"
  it produces is one action deep. Adding a new behavior requires
  editing the trait dispatcher, the world-state gathering, the goal
  list, the action list, and the string-keyed action handler.

Both work today. Both have the same structural problem: the per-state
logic concentrates in one place and grows linearly with monster
diversity, with no extension seam.

The Tactic registry replaces both with a uniform shape:

1. Each tactic is a small, pure function over a snapshot.
2. Each monster owns an ordered list of tactic references.
3. The dispatcher walks the list, first match wins.

Per-monster variation becomes *data* (the list) instead of *runtime
branches in a shared function*. Per-tactic logic becomes *unit-testable*
(pure function over a hand-built snapshot) instead of integration-only.

---

## Architecture: three layers

```
┌─────────────────────────────────────────────────────────────┐
│  LAYER 1: STATE — the FSM                                    │
│  MonsterAI component (kept, not removed)                     │
│    .mode: MonsterAIMode { Asleep, Idle, Hunting, Fleeing }   │
│    .last_known_player_position                               │
│    .flee_at_hp_percent, .kite_distance, .chase_leash         │
│    (all tuning knobs stay here)                              │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  LAYER 2: TRANSITIONS                                         │
│  Small focused systems that update MonsterAI.mode             │
│    update_mode_from_awareness   (Asleep/Idle ↔ Hunting)       │
│    damage_triggers_flee          (any reactive → Fleeing)     │
│    maybe_exit_fleeing            (Fleeing → Idle)             │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  LAYER 3: BEHAVIOR — the tactic registry                      │
│  Per-monster TacticList, evaluated top-to-bottom             │
│  Each tactic reads MonsterAI.mode and gates itself           │
│    FleePanicked     → fires only when mode == Fleeing        │
│    HuntVisibleTarget → fires only when mode == Hunting       │
│    WaypointWalk      → fires only when mode == Idle          │
└─────────────────────────────────────────────────────────────┘
```

The state machine is still the spine. Tactics are the per-state
action chooser. Compared to today's FSM, what changes is the per-mode
dispatcher: instead of a giant function with chained branches, it's a
list of named, individually testable tactics. The mode enum, the
transitions, and the tuning knobs on `MonsterAI` are unchanged (with
one addition — the new sticky `Fleeing` mode, see "FSM additions"
below).

---

## The pure resolver

Lives in `src/game/tactics/resolve.rs`. No Bevy imports, no `World`,
no `Entity`. Mirrors the shape of `src/game/combat/resolve.rs`.

### Core types

- **`ActorId(u64)`** — opaque entity handle. The adapter holds the
  `ActorId ↔ bevy::Entity` mapping for one tick; the resolver only
  ever forwards them in outcomes.
- **`TurnSnapshot`** — everything a tactic is allowed to read. Built
  once per actor per turn by the adapter. **No `Default`** — explicit
  construction at the adapter boundary catches "I forgot to copy X"
  wiring bugs at compile time, following the combat-resolver precedent.
- **`TacticAction`** — what the tactic decided. One variant per
  intent shape (`Move{dir}`, `Melee{target}`, `Ranged{target}`,
  `UseAbility{slot, target}`, `PickUp`, `OpenChest{chest}`,
  `DropAtHoard`, `SetSubmerged(bool)`, `OrderRetreat`, `Wait`).
  The adapter translates each variant into the matching existing
  Bevy intent message.
- **`TacticStateDelta`** — write-backs the tactic produced (mode
  change, waypoint-index update, ability-cooldown set, etc.). The
  resolver never mutates ECS; the adapter applies the delta after
  the action lands.
- **`TurnOutcome { tactic_name, action, delta }`** — what
  `resolve_turn` returns. Always returns *something* (falls through
  to `Wait` if no tactic fires).
- **`Tactic` trait** — `fn name(&self) -> &'static str` plus
  `fn evaluate(&self, &TurnSnapshot, &mut dyn RngCore) -> Option<(TacticAction, TacticStateDelta)>`.
  Implementors are zero-sized structs.

### Path injection

Pathfinding is a port, not a direct call. The resolver receives a
`Box<dyn PathContext>` in the snapshot; the adapter constructs one
wrapper that captures a `Map` reference. Tests inject toy
implementations (`ToyPaths`, `BlockedPaths`).

```rust
pub trait PathContext: Send {
    fn next_step_toward(&self, from: Point, to: Point) -> Option<Point>;
    fn next_flee_step(&self, from: Point, threat: Point) -> Option<Point>;
    fn pick_random_nearby(&self, from: Point, radius: i32, rng: &mut dyn RngCore) -> Option<Point>;
}
```

### Entry point

```rust
pub fn resolve_turn(
    snap: &TurnSnapshot,
    tactics: TacticList,
    rng: &mut dyn RngCore,
) -> TurnOutcome
```

That's the entire pure surface.

---

## The Bevy adapter

Lives in `src/game/tactics/dispatch.rs`.

- **`TacticBrain`** component — `{ tactics: TacticList, last_tactic: Option<&'static str> }`.
  Replaces the role `MonsterAI` and `GoapAI` play in dispatching;
  `MonsterAI` continues to hold the state and tuning knobs.
- **`tactic_dispatch_system`** — exclusive system in
  `ProcessingPhase::Brain`, scheduled
  `.after(perception_tick_system).before(monster_ai_dispatch)`.
  Walks all `(TacticBrain, MyTurn)` entities, builds a snapshot,
  calls `resolve_turn`, applies the state delta, writes the intent.
- **`build_snapshot(entity, world) -> Option<(TurnSnapshot, TacticList)>`** —
  the boundary. Reads ECS components, faction-filters and sorts
  `visible_enemies` by Chebyshev distance, wraps `Map + MovementMode`
  in a `MapPathContext`.
- **`apply_state_delta(entity, delta, world)`** — one block per
  delta field. Writes `MonsterAI.mode`,
  `MonsterAI.last_known_player_position`, `PatrolRoute.current_index`,
  `MonsterAbilities` cooldowns, `SquadBlackboard.retreat_ordered`.

---

## TACTIC_REGISTRY

```rust
pub static TACTIC_REGISTRY: phf::Map<&'static str, &'static dyn Tactic> = phf::phf_map! {
    "FleeAtLowHp"             => &FleeAtLowHp,
    "FleePanicked"            => &FleePanicked,
    "MeleeAdjacent"           => &MeleeAdjacent,
    ...
};
```

Startup validation (`validate_tactic_names`) panics if any
`monsters.ron` entry references an unknown tactic name, matching the
`detect_screen_key_collisions` pattern in `src/ui/registry.rs`. A
second startup check enforces that `Wait` is always the last entry —
the dispatcher's fallback guarantee.

---

## RON authoring

`monsters.ron` `ai:` field gains a new variant:

```ron
ai: TacticList([
    "FleePanicked",          // only fires when mode == Fleeing
    "FleeAtLowHp",           // fires when Hunting AND hp low
    "MeleeAdjacent",         // fires when enemy adjacent
    "UseAbility",
    "HuntVisibleTarget",     // fires when mode == Hunting
    "PursueLastKnownPos",    // fires when mode == Hunting AND no LOS
    "WaypointWalk",          // fires when mode == Idle (only with PatrolRoute)
    "FreeWander",            // fires when mode == Idle
    "Wait",                  // always fires (final fallback)
])
```

**No archetype layer.** Earlier design iterations explored a
`BrainSpec(archetype: Brawler, flee_at_hp_percent: 0.25)` shorthand
that would compile to a tactic list. It was dropped: the explicit
list makes priority order legible in one place, matches the
"explicit-everywhere" convention already used by items/weapons/traps,
and removes a layer of indirection. Across ~30 monsters the typing
cost is real but bounded.

Adding a new tactic shape that doesn't yet exist = one new file in
`src/game/tactics/library/` + one row in `TACTIC_REGISTRY`. The RON
references it by name.

---

## How selection works

The dispatcher loop is the simplest possible thing:

```rust
for tactic in &brain.tactics {
    if let Some((action, delta)) = tactic.evaluate(&snap, &mut rng) {
        brain.last_tactic = Some(tactic.name());
        apply(action, delta);
        return;  // STOP. Lower tactics don't run.
    }
}
// Fallback (Wait tactic at end of every list guarantees this never fires).
```

**List order is priority.** No scoring system. No dynamic priority.
No planner. Each tactic encodes its own "do I apply right now?"
predicate inside `evaluate()`. If conditions aren't met or the
chosen action can't be produced (e.g., pathfinding returns `None`),
the tactic returns `None` and the next tactic gets a chance.

This means selection isn't strictly "first applicable tactic" — it's
"first tactic that successfully produces an action." Cornered flee
falls through to melee; melee with no adjacent enemy falls through
to hunt; hunt with no visible enemy falls through to patrol. `Wait`
at the bottom is the unconditional last resort.

---

## Stateful tactics

Tactics are stateless zero-sized structs. State lives on ECS
components (`MonsterAI`, `PatrolRoute`, `MonsterAbilities`) and is
read via the snapshot, written via the state delta.

Example — `WaypointWalk` reads `snap.self_.patrol` (which mirrors
`PatrolRoute`), and on arrival sets `delta.set_waypoint_index =
Some(next_idx)`. The adapter applies the delta to the live
`PatrolRoute.current_index`.

---

## FSM additions: the Fleeing mode

The migration adds one new `MonsterAIMode` variant:

```rust
MonsterAIMode::Fleeing {
    since_turn: u32,
    last_known_threat_pos: Option<Point>,
}
```

**Sticky.** Once a monster enters Fleeing, it stays Fleeing for at
least N turns regardless of HP recovery. Exit requires three
conditions: minimum elapsed turns + no visible threat + HP recovered
above a hysteresis threshold (e.g., `flee_at_hp_percent + 0.15`).
The hysteresis prevents flee/engage oscillation when HP wavers around
the trigger threshold.

**Entry from any reactive state.** A new system `damage_triggers_flee`
runs in `ProcessingPhase::ResolveActions`, reads `DamageEvent`, and
sets `Fleeing` when the target's HP drops below
`MonsterAI.flee_at_hp_percent`. This fires for **both** Idle and
Hunting monsters — a wandering creature struck from stealth panics
without first transitioning through Hunting to identify the attacker.
Asleep is excluded; sleeping monsters wake to Hunting via the
existing awareness path.

**Exit** via `maybe_exit_fleeing` in the same phase: 10-turn minimum +
no visible threat + HP recovered with hysteresis.

**Tactic gating.** The `FleePanicked` tactic fires only when
`snap.self_.mode == Fleeing`. It does **not** check HP threshold —
that's the entry transition's job. Once the monster is in Fleeing,
it stays panicked regardless of current HP, and the exit transition
is what restores Idle.

This is the DCSS `BEH_FLEE` pattern, applied via the tactic registry
instead of as a hardcoded per-mode function.

---

## Relationship to GOAP

GOAP is **deleted** as the migration completes. The 10 GOAP monsters
get explicit `TacticList(...)` entries. Their trait-derived behaviors
map onto existing tactics with two additions:

- `StayBackBuff` — Support trait. ~40 LOC + tests.
- `PickUpAndReturn` — Hoarder trait. ~60 LOC + tests. The only
  genuinely multi-step behavior GOAP produced.

The other GOAP traits (`Cowardly`, `Aggressive`, `Intelligent`,
`Commander`) collapse to existing tactics:

- `Cowardly` → flee tactics at top of list, lower flee threshold knob
- `Aggressive` → omit flee tactics, raise chase leash
- `Intelligent` → keep `UseAbility` + `RangedAttack` tactics
- `Commander` → overlaps with `SquadBlackboard` role assignments;
  no new tactic needed beyond ensuring `SquadLeash` is present

GOAP's planner (depth-4 search) is replaced by ordered list
evaluation. No current or planned monster needs multi-step
lookahead. If one ever does, the right answer is a bespoke
multi-turn tactic in one file, not 2,200 LOC of framework.

---

## Run order each turn

```
ProcessingPhase::Brain
    perception_tick_system          (updates Awareness)
    update_mode_from_awareness      (Awareness → MonsterAI.mode)
    tactic_dispatch_system          (reads mode, picks tactic, writes intent)
    (legacy) monster_ai_dispatch    (until Phase 4 migration completes)
    (legacy) goap_ai_dispatch       (until Phase 5 migration completes)

ProcessingPhase::ResolveMovement
    handle_movement                 (executes movement intents)

ProcessingPhase::ResolveActions
    handle_melee, handle_ranged_attack, etc.
    damage_triggers_flee            (DamageEvent → MonsterAI.mode = Fleeing)
    maybe_exit_fleeing              (Fleeing + safe + healed → mode = Idle)
```

The legacy systems coexist with the new one during migration. Both
read the same `MyTurn` marker; the dispatcher that matches the
entity's components wins. Once Phase 5 completes, the legacy systems
are deleted.

---

## Migration phases

Tracked in the active branch's plan; summarized here for historical
reference:

- **Phase 0** — Scaffolding + this doc.
- **Phase 1** — Pure resolver + 6 simple tactics with unit tests
  (FleeAtLowHp, KiteRetreat, HuntVisibleTarget, PursueLastKnownPos,
  MeleeAdjacent, Wait). No Bevy adapter yet.
- **Phase 2** — Bevy adapter, `TACTIC_REGISTRY`, RON `TacticList`
  variant. Coexists with FSM and GOAP; no monsters migrated.
- **Phase 2.5** — Add `MonsterAIMode::Fleeing` + `damage_triggers_flee`
  + `maybe_exit_fleeing` systems + `FleePanicked` tactic.
- **Phase 3** — Canary: migrate Giant Rat to `TacticList`. Validate
  save/load + UI display end-to-end.
- **Phase 4** — Bulk-migrate every FSM monster. Delete
  `execute_monster_ai`, `monster_ai_dispatch`, and the `Fsm` variant
  of `AiConfig`.
- **Phase 5** — Migrate the 10 GOAP monsters. Delete the GOAP
  framework, `GoapAI`, and the `Goap` variant of `AiConfig`. ~2,200
  LOC deletion.
- **Phase 6** — Docs cleanup. Remove migration-status callouts.

---

## Cross-links

- [GAME.md](GAME.md) — combat resolver (the structural template this
  follows)
- [TURNS.md](TURNS.md) — `ProcessingPhase` ordering
- [STEALTH.md](STEALTH.md) — `Awareness` driving mode transitions
- [SQUAD_AI.md](SQUAD_AI.md) — `SquadBlackboard`, leader leash
- [FACTIONS.md](FACTIONS.md) — hostility checks gating enemy filtering
- [SPAWNING.md](SPAWNING.md) — `SpawnEntry` attaching `TacticBrain`
- [NPCS.md](NPCS.md) — peaceful NPCs reusing the same dispatcher
  through the faction gate
