# Trap Design

**Status:** Not yet implemented. This document proposes a schema and
integration plan for when the trap system is added. Read first if the
user asks to add traps.

Traps are a core Brogue-like mechanic and one of the highest fun-per-effort
additions to the current project (see past evaluations).

## Design intent

Traps make exploration tense. The player must read the floor before
walking into it. Three-way tension: visible traps can be avoided,
hidden traps require caution, spent traps become safe terrain.

## Proposed data model

Traps are a form of tile **decoration** with a trigger, not a separate
entity system. They live in the decoration layer alongside Grass,
Fungus, etc., but carry a payload enum.

### Rust-side (proposal)
```rust
// In roguelike_engine::map::tile or a new trap module
pub enum Decoration {
    // existing variants…
    Trap(TrapKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrapKind {
    /// Spikes deal 2d4 physical damage; no trigger limit.
    Spike,
    /// Dart deals 1d4 poison + poison DoT; fires once then disarmed.
    PoisonDart,
    /// Teleport the stepper to a random walkable tile. Fires once.
    Teleport,
    /// Alarm — wakes all monsters within radius 10 and pings their squad.
    Alarm,
    /// Fire — ignites the tile for 5 turns + 2d4 fire on step.
    Fire,
    /// Chasm — tile collapses into a chasm on step (player/monster falls).
    Pitfall,
}

#[derive(Component)]
pub struct TrapState {
    pub kind: TrapKind,
    pub hidden: bool,      // unseen until revealed by FOV near player OR step
    pub armed: bool,       // false = spent; decoration remains as visual tell
}
```

### RON schema (proposal)
```ron
// assets/traps.ron
(
    traps: {
        "Spike Trap": (
            kind: Spike,
            damage: "2d4",
            damage_type: "physical",
            hidden_chance: 0.7,   // 0.0-1.0 per floor
            rearmable: true,
            min_floor: 2,
            max_floor: 26,
            weight: 3,
            ascii_char: "^",
            ascii_fg_armed: "#A04040",
            ascii_fg_spent: "#604040",
        ),
        ...
    }
)
```

### Spawn integration
A new `TrapSpawner` builder in the pipeline, after `MonsterSpawner`.
Traps placed in corridors and room approaches at a rate scaled by
floor (lower floors: 1–2 per floor; deeper: 4–6).

Prefabs can declare deterministic trap placements via a new `traps` field.

## Behavior integration

### Trigger
New system `trigger_traps_on_step` runs in `TurnState::Processing`:
1. Query entities whose `Position` is on a tile with `Decoration::Trap(_)`
2. If the entity just moved this turn (`Changed<Position>`) AND the trap
   is `armed`:
3. Fire the trap's effect (damage, status, teleport, etc.)
4. Set `armed = false` if single-use; keep `true` for rearmable
5. Set `hidden = false` (trap is now visible to everyone)
6. Log the trigger

### Hidden / reveal
- A hidden trap is rendered as the underlying tile glyph (floor).
- Reveal conditions:
  - Player has Ring of Perception → auto-reveal within vision range
  - Triggered (any entity steps on it)
  - Searched (player S key on adjacent tile, new action)

### Symmetric combat
Traps affect monsters. A fleeing kobold may step on a spike trap. This
enables tactics like "herd the enemy over the trap."

## UI / affordances

- Hovered trap in FOV shows name + effect description
- Game log line on trigger ("You step on a spike trap! 5 damage.")
- ASCII glyph `^` for all trap types; color differentiates kind
- Spent traps render in a desaturated color

## Save/load

`TrapState` components on tiles survive save/load via the existing
decoration mutation machinery (per `.claude/rules/save-load-checklist.md`).
Since traps sit on the decoration layer, the existing `Decoration` serde
path covers them — no new field in `GameSaveData` needed if `TrapKind`
derives `Serialize + Deserialize`.

## Rollout plan (6 sub-tasks)

1. **Schema + enum** — Add `TrapKind` enum, `Decoration::Trap` variant,
   `TrapState` component. Engine-side.
2. **Data + spawn** — Create `assets/traps.ron`, `TrapManifest`, and a
   `TrapSpawner` builder in the map pipeline.
3. **Trigger system** — `trigger_traps_on_step`, including damage/status
   effects for Spike, PoisonDart, Fire.
4. **Hidden / reveal** — FOV reveal system, search action on S key.
5. **Rare traps** — Teleport, Alarm, Pitfall. Pitfall reuses the existing
   chasm system — good test of the Decoration → chasm pipeline.
6. **Design doc + tests** — `docs/design/TRAPS.md`, unit tests per
   `.claude/rules/testing-requirements.md`.

Each sub-task should land as its own commit with tests. The user has
requested that every plan includes tests — do not bundle.

## Balance notes

- Spike traps should be genre-standard (Brogue has them at 3 damage-ish
  at early floors). 2d4 avg 5 is reasonable with the game's ~20-HP start.
- Alarm traps in corridors are the interesting tactical addition —
  they convert exploration friction into encounter difficulty.
- Teleport traps can strand the player far from stairs. Balance: no
  teleport to chasm tiles, no teleport into walls.
- Pitfall traps on floor 25 with no floor 26 below have special
  handling — same as Bloat chasms on the last floor (lost to void or
  block the teleport).

## Cross-references

- `src/map/tile.rs` — decoration + liquid mutation pipeline
- `src/game/actions.rs` — `MovementIntent` → position updates
- `src/game/abilities.rs::ExplodeOnHit` — for `CrackFloor` pattern reused
  by Pitfall
- `src/game/fire.rs` — for Fire trap's ignition behavior
- `docs/design/DUNGEON.md` — where a "Traps" section should live (or
  a new TRAPS.md if the design gets large)
