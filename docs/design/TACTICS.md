# Tactics

Per-monster, data-driven AI decision-making. Every monster's behavior
is an ordered list of `Tactic`s evaluated top-to-bottom each turn;
the first non-`None` wins. Replaced the legacy FSM mega-dispatcher
(`execute_monster_ai`) and the GOAP planner (`execute_goap`).

---

## Why this exists

The original AI shipped as two parallel mega-dispatchers:

- **FSM** (`execute_monster_ai`) — one 200-LOC function with a hardcoded
  sequence of branches (stun → mode update → submerge → ability →
  ranged → flee → kite → erratic → squad-leash → resolve_movement →
  wait). Per-monster variation came from boolean fields on
  `MonsterAI` gating runtime branches. The function had no unit tests
  because it took `&mut World` and read 12 components.

- **GOAP** (`execute_goap`) — 2,211 LOC of framework (planner +
  WorldState + goals + actions + per-trait builders) driving 10
  monsters. The planner ran at depth 4 but in practice every "plan"
  was one action deep. Adding a new behavior required editing the
  trait dispatcher, world-state gathering, goal list, action list,
  and string-keyed action handler.

Both worked. Both had the same structural problem: per-state logic
concentrated in one place and grew linearly with monster diversity,
with no extension seam.

The tactic registry replaced both with a uniform shape:

1. Each tactic is a small, pure function over a snapshot.
2. Each monster owns an ordered list of tactic references.
3. The dispatcher walks the list, first match wins.

Per-monster variation became *data* (the list) instead of *runtime
branches in a shared function*. Per-tactic logic became
*unit-testable* (pure function over a hand-built snapshot) instead of
integration-only.

---

## Architecture: three layers

```
┌─────────────────────────────────────────────────────────────┐
│  LAYER 1: STATE — the FSM                                    │
│  MonsterAI component (engine-side)                           │
│    .mode: MonsterAIMode { Asleep, Idle, Hunting }            │
│    .last_known_player_position                               │
│    .flee_at_hp_percent, .kite_distance, .chase_leash, …      │
│                                                              │
│  Game-side sticky `Fleeing` overlay component                │
│    { since_turn, last_known_threat_pos }                     │
│  Synthesized as AiMode::Fleeing in the snapshot.             │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  LAYER 2: TRANSITIONS                                         │
│  Small focused systems that update MonsterAI.mode             │
│    refresh_monster_modes_system  (chase tracking, give-up,   │
│                                   waypoint snapback)         │
│    damage_triggers_flee          (any reactive → Fleeing)    │
│    maybe_exit_fleeing            (Fleeing → Idle)            │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  LAYER 3: BEHAVIOR — the tactic registry                      │
│  Per-monster TacticBrain.tactics (&'static slice)            │
│  Each tactic reads MonsterAI.mode and gates itself           │
│    FleePanicked     → fires only when mode == Fleeing        │
│    FleeAtLowHp      → Hunting + HP < threshold + visible     │
│    UseAbility       → Hunting + visible + ready ability      │
│    RangedAttack     → Hunting + ranged_range + in range      │
│    KiteRetreat      → Hunting + kites + threat too close     │
│    MeleeAdjacent    → any active mode + adjacent enemy       │
│    SquadLeash       → leader exists + too far                │
│    HuntVisibleTarget → Hunting + visible (not adjacent)      │
│    PursueLastKnownPos → Hunting + no visible + leash OK      │
│    SubmergeOrSurface → aquatic + tile mismatch               │
│    IdleMove          → Idle (dispatches on idle_movement)    │
│    Wait              → unconditional fallback                │
└─────────────────────────────────────────────────────────────┘
```

The state machine is the spine. Tactics are the per-state action
chooser. Compared to the legacy FSM, the change is in the per-mode
dispatcher: instead of one giant function with chained branches, it
is a list of named, individually testable tactics. The mode enum,
the transitions, and the tuning knobs on `MonsterAI` are unchanged
from the engine — the game adds the `Fleeing` overlay on top.

---

## The pure resolver

Lives in [src/game/tactics/resolve.rs](../../src/game/tactics/resolve.rs).
No Bevy imports, no `World`, no `Entity`. Mirrors the shape of
[src/game/combat/resolve.rs](../../src/game/combat/resolve.rs).

### Core types

- **`ActorId(u64)`** — opaque entity handle. The adapter holds the
  `ActorId ↔ bevy::Entity` mapping for one tick; the resolver only
  ever forwards them in outcomes.
- **`TurnSnapshot`** — everything a tactic is allowed to read. Built
  once per actor per turn by the adapter. **No `Default`** — explicit
  construction at the adapter boundary catches "I forgot to copy X"
  wiring bugs at compile time, following the combat-resolver
  precedent.
- **`TacticAction`** — what the tactic decided. One variant per
  intent shape (`Move{dir}`, `Melee{target}`, `Ranged{target}`,
  `UseAbility{slot, target}`, `PickUp`, `OpenChest{chest}`,
  `DropAtHoard`, `SetSubmerged(bool)`, `OrderRetreat`, `Wait`).
  The adapter translates each variant into the matching existing
  Bevy intent message.
- **`TacticStateDelta`** — write-backs the tactic produced (mode
  change, waypoint-index update, ability-cooldown set, roam-target
  update, etc.). The resolver never mutates ECS; the adapter applies
  the delta after the action lands.
- **`TurnOutcome { tactic_name, action, delta }`** — what
  `resolve_turn` returns. Always returns *something* (falls through
  to `Wait` if no tactic fires).
- **`Tactic` trait** — `fn name(&self) -> &'static str` plus
  `fn evaluate(&self, &TurnSnapshot, &mut dyn RngCore) -> Option<(TacticAction, TacticStateDelta)>`.
  Implementors are zero-sized structs.

### Path injection

Pathfinding is a port, not a direct call. The resolver receives a
`Box<dyn PathContext>` in the snapshot; the adapter constructs one
wrapper (`MapPathContext`) that captures a `Map` reference. Tests
inject toy implementations (`ToyPaths`, `BlockedPaths`).

```rust
pub trait PathContext: Send {
    fn next_step_toward(&self, from: Point, to: Point) -> Option<Point>;
    fn next_flee_step(&self, from: Point, threat: Point) -> Option<Point>;
    fn pick_random_nearby(&self, from: Point, radius: i32, rng: &mut dyn RngCore) -> Option<Point>;
    fn pick_random_walkable(&self, rng: &mut dyn RngCore) -> Option<Point>;
}
```

### Entry point

```rust
pub fn resolve_turn(
    snap: &TurnSnapshot,
    tactics: &[&dyn Tactic],
    rng: &mut dyn RngCore,
) -> TurnOutcome
```

The single entry point. Walks the list top-to-bottom, returns the
outcome of the first tactic that fires, or `FallbackWait` if every
tactic passes (every well-formed list ends with `Wait`, so the
fallback should never fire in practice).

---

## The Bevy adapter

Lives in [src/game/tactics/dispatch.rs](../../src/game/tactics/dispatch.rs).

- **`TacticBrain`** component — `{ tactics: &'static [&'static dyn Tactic],
  last_tactic, idle_movement, roam_target }`. Replaces the role
  `MonsterAI` played in dispatching; `MonsterAI` continues to hold
  the state and tuning knobs.
- **`tactic_dispatch_system`** — exclusive system in
  `ProcessingPhase::Brain`, scheduled in the Brain chain after
  `refresh_monster_modes_system` so the dispatcher always reads a
  fresh mode. Walks all `(TacticBrain, MyTurn)` entities, builds a
  snapshot, calls `resolve_turn`, applies the state delta, writes
  the intent.
- **`build_snapshot(entity, world) -> Option<(TurnSnapshot, IdMap, TacticList)>`** —
  the boundary. Reads ECS components, faction-filters and sorts
  `visible_enemies` by Chebyshev distance, wraps `Map + MovementMode`
  in a `MapPathContext`, converts the spawn-time `PatrolRoute` (if
  any) into a `PatrolView`.
- **`apply_state_delta(entity, delta, world)`** — one block per
  delta field. Writes `MonsterAI.mode`,
  `MonsterAI.last_known_player_position`, `MonsterAI.chase_distance`,
  `PatrolRoute.current_index`, `TacticBrain.roam_target`.
- **`BracketRngAdapter`** — bridges `bracket_lib::random::RandomNumberGenerator`
  (the game's `GameRng` resource) to `rand`'s `RngCore` (what the
  pure resolver consumes).

---

## TACTIC_REGISTRY

[src/game/tactics/library/mod.rs](../../src/game/tactics/library/mod.rs)
holds:

- A `const &dyn Tactic` per shipping tactic (every tactic is a
  zero-sized struct so the reference is trivially `'static`).
- `lookup_tactic(name: &str) -> Option<&'static dyn Tactic>` — the
  name → instance table. Used by the spawner.
- `ALL_TACTIC_NAMES: &[&str]` — alphabetized list of every shipping
  tactic name, for the startup validator.
- `TERMINAL_TACTIC_NAME: &str = "Wait"` — required final entry on
  every monster's list.

`validate_tactic_names_system` (registered by `TacticsPlugin`) runs
once after `MonsterManifest` loads. It panics if a `monsters.ron`
entry references an unknown tactic name or if any list does not end
with `"Wait"`. Catches typos at boot rather than at the first time
the monster spawns.

---

## RON authoring

`monsters.ron`'s `ai:` field is the single `AiConfig::TacticList`
variant:

```ron
ai: TacticList(
    tactics: [
        "FleePanicked",          // only fires when mode == Fleeing
        "FleeAtLowHp",           // Hunting + hp low
        "RangedAttack",          // Hunting + visible + in range
        "KiteRetreat",           // Hunting + too close + kiter
        "MeleeAdjacent",         // any active mode + adjacent enemy
        "UseAbility",            // Hunting + visible + ready ability
        "SquadLeash",            // strayed from leader
        "HuntVisibleTarget",     // Hunting + visible not adjacent
        "PursueLastKnownPosition", // Hunting + no visible + leash OK
        "IdleMove",              // Idle (dispatches on idle_movement)
        "Wait",                  // fallback
    ],
    flee_at_hp_percent: 0.25,
    chase_leash: 8,
    base_morale: 0.6,
    idle_movement: PathToRandomTile,  // omit to use the default
)
```

**Per-monster knobs** (all flat fields, all serde-default-friendly):

| Field | Default | Purpose |
|-------|---------|---------|
| `tactics` | required | ordered tactic list (must end with `"Wait"`) |
| `flee_at_hp_percent` | 0.0 | HP fraction below which `FleeAtLowHp` fires (0.0 = never) |
| `chase_leash` | 0 | turns of unseen pursuit before giving up (0 = no leash) |
| `kites` | false | enables `KiteRetreat` |
| `kite_distance` | 3 | preferred ranged spacing |
| `ranged_range` | 0 | sets `RangedCapable.range`; 0 = no ranged |
| `base_morale` | 0.6 | starting `Morale` (0.0–1.0, bosses raise) |
| `idle_movement` | `PathToRandomTile` | what `IdleMove` does (see below) |

**No archetype layer.** Earlier design iterations explored a
`BrainSpec(archetype: Brawler, …)` shorthand that would compile to a
tactic list. It was dropped: the explicit list makes priority order
legible in one place, matches the "explicit-everywhere" convention
already used by items/weapons/traps, and removes a layer of
indirection. Across ~35 monsters the typing cost is real but
bounded; revisit only when content scale demands it (see "Future
Work" below).

Adding a new tactic = one new file in `library/` + one row in
`lookup_tactic` + one row in `ALL_TACTIC_NAMES`. The RON references
it by name.

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
to hunt; hunt with no visible enemy falls through to pursue or idle.
`Wait` at the bottom is the unconditional last resort.

---

## Stateful tactics

Tactics are stateless zero-sized structs. State lives on ECS
components (`MonsterAI`, `PatrolRoute`, `TacticBrain.roam_target`,
`MonsterAbilities`) and is read via the snapshot, written via the
state delta.

Example — `IdleMove`'s `PathToRandomTile` variant reads
`snap.self_.roam_target`, walks toward it, and on arrival or
pathfind failure picks a new random walkable tile via
`snap.paths.pick_random_walkable(rng)` and writes the new target via
`delta.set_roam_target = Some(Some(new_target))`. The adapter copies
the delta into `TacticBrain.roam_target` after the action lands.

---

## FSM additions: the Fleeing mode

The migration added one sticky FSM state, implemented as a game-side
overlay component rather than a new engine enum variant:

```rust
#[derive(Component)]
pub struct Fleeing {
    pub since_turn: u32,
    pub last_known_threat_pos: Option<Point>,
}
```

**Sticky.** Once a monster enters Fleeing, it stays Fleeing for at
least `FLEE_MIN_TURNS` (10) regardless of HP recovery. Exit requires
three conditions: minimum elapsed turns + no visible threat + HP
recovered above `flee_at_hp_percent + FLEE_HYSTERESIS_MARGIN` (0.15).
The hysteresis prevents flee/engage oscillation when HP wavers
around the trigger threshold.

**Entry from any reactive state.** `damage_triggers_flee` runs in
`ProcessingPhase::ResolveActions`, reads `DamageEvent`, and inserts
`Fleeing` when the target's HP drops below
`MonsterAI.flee_at_hp_percent`. Fires for **both** Idle and Hunting
monsters — a wandering creature struck from stealth panics without
first transitioning through Hunting to identify the attacker.
Asleep is excluded; sleeping monsters wake to Hunting via the
existing awareness path. The system is gated `With<TacticBrain>` so
unmigrated AI paths (today: none) are unaffected.

**Exit** via `maybe_exit_fleeing` in the same phase.

**Tactic gating.** The `FleePanicked` tactic fires only when
`snap.self_.mode == AiMode::Fleeing { .. }`. It does **not** check
HP threshold — that's the entry transition's job. Once the monster
is in Fleeing, it stays panicked regardless of current HP, and the
exit transition restores Idle.

This is the DCSS `BEH_FLEE` pattern, applied via the tactic registry
instead of as a hardcoded per-mode function.

---

## Idle movement: the `IdleMove` tactic

`IdleMove` handles all non-combat movement via the per-monster
`IdleMovement` knob declared on the asset (default
`PathToRandomTile`):

| Variant | Behavior | Required spawn-time state |
|---------|----------|---------------------------|
| `PathToRandomTile` | Pick a random walkable tile, pathfind there, repeat. | none (uses map + `roam_target` stored on `TacticBrain`) |
| `Patrol` | Walk a fixed list of waypoints in a loop. | `PatrolRoute::Waypoint { points, .. }` attached at spawn |
| `Roam` | Bounded random walk inside a rectangle. | `PatrolRoute::AreaRoam { min, max }` attached at spawn |
| `Stationary` | Never produce idle movement. | none |

The asset declares **what kind of wander** (the enum value); the
spawn-time builder (currently `TownNpcBuilder`) attaches the
`PatrolRoute` component with **where to wander** (bounds/waypoints).
Separation of concerns — the asset is content-time, the route is
placement-time.

A monster with no `idle_movement` field uses the default
(`PathToRandomTile`). A monster whose tactic list excludes `IdleMove`
never moves when idle, regardless of the field value.

`IdleMove` only fires in `AiMode::Idle`. Combat-mode movement is the
job of the dedicated combat tactics (`HuntVisibleTarget`,
`PursueLastKnownPosition`, `KiteRetreat`, etc.).

---

## Run order each turn

```
ProcessingPhase::Brain
    perception_tick_system          (updates Awareness)
    refresh_monster_modes_system    (chase tracking, leash give-up,
                                     waypoint snapback)
    tactic_dispatch_system          (reads mode, picks tactic,
                                     writes intent + state delta)
    marker_dispatch                 (other turn-tagged entities)

ProcessingPhase::ResolveMovement
    handle_movement                 (executes movement intents)

ProcessingPhase::ResolveActions
    handle_melee, handle_ranged_attack, ability handlers, etc.
    damage_triggers_flee            (DamageEvent → insert Fleeing)
    maybe_exit_fleeing              (Fleeing + safe + healed → remove)
```

The `Awareness → Mode` transition (engine's
`update_mode_from_awareness`) runs inside
`refresh_monster_modes_system`. Game-side mode logic (chase
tracking, leash, waypoint snapback) runs in the same call. By the
time `tactic_dispatch_system` reads the mode, it's fresh.

---

## Migration summary

Tactic registry replaced FSM + GOAP across six phases, completed in
9 commits. Total delta: roughly **−1,700 LOC** (added ~700 LOC of
tactic registry + adapter + 12 tactic files with 100+ tests;
deleted ~2,400 LOC of FSM dispatcher + GOAP framework). All 35
monsters use `AiConfig::TacticList`.

The 10 GOAP-trait monsters lost their unique behaviors (Support's
`StayBackBuff`, Hoarder's `PickUpAndReturn`) per design decision —
their abilities still fire via `UseAbility`, but the trait-specific
goal chains are gone. If a future monster genuinely needs
multi-turn planning (fetch key → unlock door → reach altar), the
right answer is a bespoke multi-turn tactic in one file, not a
2,200-LOC planning framework.

---

## Known limitations carried forward

- **Save schema for `Fleeing`**: the component isn't serialized.
  A monster panicking when you save loses its sticky state on load.
  Not blocking; fix is a v7 schema bump per `save-load-checklist.md`.
- **Stun/entangle skip is silent**: was a log line `"X is stunned!"`
  in the FSM. The resolver's `maybe_skip_turn` emits a `WaitIntent`
  without a feedback line. If it's missed, add a separate
  status-effect log system; do not re-couple to the tactic adapter.

---

## Future work

If the asset file becomes unreadable from monster count growth
(~50+ entries), introduce **tactic presets** — named tactic-list
shorthands in code that RON can reference:

```ron
ai: TacticList(preset: "Brute", base_morale: 0.7)
```

Presets live in a `lookup_preset(name) -> Option<&'static [&'static str]>`
table next to `lookup_tactic`. Custom monsters still use explicit
lists. This is the smallest reversible step — adopt only when
repetition costs exceed the cost of indirection.

Do not bring back the GOAP-style `traits: [...]` system. It
recreates the "what does this monster actually do?" problem that
the migration was solving.

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
