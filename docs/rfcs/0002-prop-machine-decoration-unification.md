# RFC 0002 — Unify Props, Machines, and Decoration Step-Effects

**Status:** Landed
**Branch:** `worktree-rfc-0002-prop-effects`
**Related docs:** [GAME.md](../design/GAME.md), [FIRE.md](../design/FIRE.md), [TILE_PROMOTION.md](../design/TILE_PROMOTION.md), [SAVE.md](../design/SAVE.md), [RFC 0001 — Combat Resolver](0001-combat-resolver.md)

## What shipped

- `src/game/prop_effects.rs` — pure type vocabulary + Bevy adapter. 27 unit tests cover effect flattening, audience filtering, activation mode, and the `classify_activation` decision helper. PropEffectsPlugin registers bump dispatch, step dispatch, decoration step dispatch, and deferred spawn processors.
- `PropAsset.trigger: Option<PropTrigger>` field — authors declare prop-level effects in `props.ron`. The altar prop carries HealFull + SpawnItem (PlayerOnly, OnceInert); `monster_trap` is a new invisible step trap (Anyone, OnceConsumed).
- `Decoration::step_effect` lookup — Cobweb applies Slowed(3) on step; all other decorations stay silent.
- `EverFired` persisted in saves; schema bumped v9 → v10 (backward-compatible via `#[serde(default)]`).
- `resolve_bump` priority fix — non-faction candidates prefer Collider entities, so a chest wins over a colocated invisible trap.
- Trapped Vault migrated: chest + monster_trap props colocated. Open chest, take loot, step into the now-empty tile, get ambushed.
- `src/game/machines.rs` deleted (220+ lines). `PrefabTemplate.triggers` field removed. `MachineSpawn` / `machine_spawn_list` / `MachinePlan` deleted. 33 fewer lines of indirection in `floor_materializer.rs`.

All 713 tests in bevy_rpg + 452 in roguelike_engine still pass.

## Summary

Collapse the **Machine** concept into **Prop** so every interactive
tile-level thing in the world is one entity type with an optional
`trigger` block declared in `props.ron`. The trigger kind (step vs
bump) is derived from the prop's `is_blocking` flag — you can only
step onto non-blocking props, you can only bump blocking props, so no
explicit kind field is needed. Introduce a parallel lookup-driven
`step_effect` for the **Decoration** enum so per-cell tile flavor
(cobwebs primarily) can carry simple effects without becoming
entities.

After this lands, "stepping on this cell does X" has exactly two
authoring surfaces — `props.ron` for entities, `Decoration::step_effect`
for tile-packed variants — using a shared `TileEffect` vocabulary.

## Problem

### Three parallel layers express "interactive cell"

The codebase currently has three independent ways to attach behavior to
a map cell. Each was added incrementally without re-examining the prior
layer, and they overlap.

| Layer | Lives in | Auth surface | What it does today |
|-------|----------|--------------|--------------------|
| `Decoration` enum | `Tile.decoration` (packed in `Map.tiles`) | Code (enum variant) + `decorations.ron` (gen-time seeding only) | Visual flavor. **Zero gameplay effect on entities walking on it.** Embers and CrackedFloor exist but are silent. |
| `Prop` entity | ECS entity with `PropName` | `props.ron` | Static scenery — sprite, glyph, blocking, opaque, light radius. **No behavior.** |
| `Machine` entity | ECS entity colocated with a prop | `MachineTrigger` + `MachineEffect` in code; `triggers:` array in `prefabs.ron` | Interactive logic — `BumpActivate`/`StepActivate` → `HealFull`/`SpawnItem`/`SpawnMonsters`/`Multi`. |

Authoring a campfire that **(a)** emits light and **(b)** burns the
walker requires touching all three layers:

1. A new `props.ron` entry (light + sprite, but the prop format has no
   effect fields).
2. A new `MachineEffect::ApplyDamage` or `ApplyStatus` variant in
   [src/game/machines.rs](../../src/game/machines.rs) (does not exist
   today).
3. Every prefab using the campfire has to duplicate the prop at
   `(x, y)` in `props:` and a paired `triggers:` entry at the same
   coordinates with the same kind/effect repeated.

The two existing Machine-using prefabs (Shrine altar, Trapped Vault
chest) already do this duplication.

### Concrete pain points

- **Two effect vocabularies.** `MachineEffect` (HealFull, SpawnItem,
  SpawnMonsters, Multi) is one enum; nothing else uses it. Adding a
  new effect kind ("apply Burning") means extending only Machines
  even though Props and Decorations both want the same vocabulary.
- **Prefab `triggers:` array duplicates `props:` array.** Each Machine
  has a paired Prop at the same `(x, y)`. The prefab author edits
  both. The [floor materializer](../../src/map/floor_materializer.rs)
  spawns them as separate entities that happen to share a tile.
- **Machine state doesn't persist.** A used Shrine and a sprung
  Trapped Vault reset to fresh on save/load — `src/save/mod.rs` has
  zero `Machine` references. Latent bug.
- **Decorations have no shared vocabulary for step effects.** When
  a decoration *should* affect a walker (cobwebs slowing movement is
  the canonical example), there's no infrastructure. Anyone adding
  one today would have to drop ad-hoc logic into a movement system.
- **Light authoring is split.** `PropAsset.light_radius` configures
  one light source path. Fire entities use a separate code path via
  the engine's lighting system. Same engine pipeline, two authoring
  surfaces.
- **Machine targets only the Player.** [machines.rs:355](../../src/game/machines.rs#L355)
  filters `With<Player>`. Monsters stepping on a Trapped Vault tile
  do not trigger it. This is wrong for symmetric combat — see the
  pillar in CLAUDE.md "Symmetric combat is partially broken."

### Why this system, why now

Per [`.claude/rules/ai-friendly-systems.md`](../../.claude/rules/ai-friendly-systems.md):

> One concept, one file — when something has a name (the combat
> resolver, the screen registry, the floor builder pipeline),
> somebody should be able to open ONE file and learn the contract.

A reader who wants to understand "how does the campfire work?"
currently opens five files: `tiles.ron`, `decorations.ron`,
`props.ron`, `prefabs.ron`, and `machines.rs`. The split is
storage-justified (per-cell vs entity) but the **behavior** crosses
all of them without a shared vocabulary.

This is also the same shape as RFC 0001 — a behavior domain
scattered across N files, pulled into one declarative authoring
surface with a thin adapter.

## Proposed Interface

### One effect vocabulary

A new module **`src/game/prop_effects.rs`** owns the shared `TileEffect`
enum and its application functions. No Bevy in the enum itself —
just data. (Named `prop_effects.rs` to avoid collision with the
existing `src/game/effects.rs`, which owns *consumable item* effects
— HealHp, ZapStaff, EnchantItem — a player-driven vocabulary distinct
from this world-driven one.)

```rust
//! Shared effect vocabulary for props (trigger block) and
//! decorations (step_effect). Pure data; application is via the
//! adapter system in this module.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TileEffect {
    /// Roll dice damage of the given type against the stepper/bumper.
    DealDamage { dice: String, kind: DamageType },
    /// Apply a status effect for N turns.
    ApplyStatus { effect: StatusEffect, duration: u32 },
    /// Heal the activator to full HP.
    HealFull,
    /// Spawn an item at an adjacent walkable tile.
    SpawnItem { item_name: String },
    /// Spawn N monsters at adjacent walkable tiles. Empty name picks
    /// from the level's spawn table.
    SpawnMonsters { monster_name: String, count: u32 },
    /// Apply multiple effects in order.
    Multi(Vec<TileEffect>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EffectAudience {
    /// Effect fires for any entity (player + monsters).
    Anyone,
    /// Effect fires only for the player (today's Machine default).
    PlayerOnly,
    /// Effect fires only for monsters (e.g., player-laid traps).
    MonstersOnly,
}
```

Application functions take the effect, the activator entity, and a
small `EffectContext` SystemParam that bundles the writes the effects
need (Health query, Commands, log writer, status writer). Pattern
mirrors the combat resolver — pure data in, side effects via
adapter.

### Props gain an optional trigger block

`PropAsset` in [src/assets/mod.rs](../../src/assets/mod.rs) extends
with **one** optional field — a `trigger: Option<PropTrigger>` block
that bundles effect + activation policy:

```rust
pub struct PropAsset {
    // ... existing fields (name, is_blocking, is_opaque,
    // light_radius, light_color, sprite paths, ascii)
    pub trigger: Option<PropTrigger>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropTrigger {
    /// The effect fired when this prop is activated.
    pub effect: TileEffect,
    /// Who triggers the effect. Default: Anyone.
    #[serde(default = "PropTrigger::default_audience")]
    pub audience: EffectAudience,
    /// Activation lifecycle. Default: Repeating.
    #[serde(default)]
    pub mode: ActivationMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum ActivationMode {
    /// Fires every time the prop is activated. The campfire pattern.
    #[default]
    Repeating,
    /// Fires once; the prop remains visible/blocking but inert
    /// afterward. The used-altar pattern — you healed once, the
    /// altar is still there, bumping does nothing.
    OnceInert,
    /// Fires once; the prop entity despawns afterward. The sprung-trap
    /// pattern — the chest opens, monsters appear, the chest is gone.
    OnceConsumed,
}
```

The two-bool space (`single_use` + `consume_on_activate`) collapses to
three explicit states with no degenerate combination. Saved
activation state is a single `bool ever_fired` on the `Effected`
component, interpreted against `mode`.

**Step vs bump is derived, not declared.** Blocking props can only be
bumped (you can't enter the tile). Non-blocking props can only be
stepped on (you'd never stop walking to bump them). The activation
dispatcher reads `is_blocking` to know which path applies. Edge case:
a prop that wants to fire both on entering *and* on bumping (e.g.,
a future "interact + walk through" door) is out of scope here and can
add a `trigger_mode: Step | Bump | Both` field in a follow-up RFC if
ever needed.

`props.ron` examples:

```ron
"campfire": PropAsset(
    name: "Campfire",
    is_blocking: false,                       // non-blocking → step trigger
    light_radius: Some(28.0),
    light_color: Some((1.0, 0.55, 0.18)),
    trigger: Some(PropTrigger(
        effect: DealDamage(dice: "1d4", kind: Fire),
        audience: Anyone,
        mode: Repeating,                      // burns every time
    )),
    // sprite + glyph as today
),

"altar": PropAsset(
    name: "Altar",
    is_blocking: true,                        // blocking → bump trigger
    light_radius: Some(20.0),
    light_color: Some((0.8, 0.9, 1.0)),
    trigger: Some(PropTrigger(
        effect: Multi([
            HealFull,
            SpawnItem(item_name: "Scroll of Enchanting"),
        ]),
        audience: PlayerOnly,
        mode: OnceInert,                      // heals once; remains as scenery
    )),
),

"trapped_chest": PropAsset(
    name: "Chest",
    is_blocking: true,
    trigger: Some(PropTrigger(
        effect: SpawnMonsters(monster_name: "", count: 2),
        audience: Anyone,                     // any actor springs it
        mode: OnceConsumed,                   // chest despawns after firing
    )),
),

"barrel": PropAsset(
    name: "Barrel",
    is_blocking: true,
    trigger: None,                            // passive scenery
    // sprite + glyph as today
),
```

### Decorations gain a step_effect lookup

Decoration stays a packed `Tile.decoration` enum — no per-cell entity
cost. Effects are looked up by variant via a static `fn step_effect`
on the enum, owned in the engine alongside the variants themselves
(`crates/roguelike_engine/src/map/tile.rs`).

```rust
impl Decoration {
    pub fn step_effect(self) -> Option<TileEffect> {
        match self {
            Decoration::Cobweb => Some(TileEffect::ApplyStatus {
                effect: StatusEffect::Slowed,
                duration: 3,
            }),
            _ => None,
        }
    }
}
```

A new system in `prop_effects.rs` watches `Changed<Position>` for any
actor with `Health`, resolves the tile's decoration via `Map`, and
fires the effect if present. Same system handles the prop step-trigger
lookup by colocated-entity query.

**Decorations that intentionally stay silent.** Embers, Ash, Bloodstain,
Rubble, Moss, Fungus, and the grass variants carry no step effect.
They are flavor traces or passive map state, not hazards:

- **Embers** are the post-fire residue. The fire system already wrote
  these tiles to mark "this was on fire seconds ago." Punishing the
  player for walking through a battlefield they just cleared feels
  unfair, and the active hazard (live fire) has its own entity layer.
- **CrackedFloor** already has its tile-promotion path
  ([TILE_PROMOTION.md](../design/TILE_PROMOTION.md)) — stepping on it
  may promote to a chasm. That mechanic stays where it is; no
  duplication via `step_effect`.
- Rubble, Bloodstain, grass: pure visual storytelling.

Future decoration variants (e.g., a hypothetical "burning oil pool")
can opt into a step effect by adding a match arm. Cobweb's slow is
the only shipped effect in this RFC.

### Prefab format simplification

`PrefabTemplate.triggers` is removed. Authors place props that
already carry their effect:

```ron
// Was: separate props + triggers
PrefabTemplate(
    name: "Shrine",
    props: [],
    triggers: [
        (x: 2, y: 2, prop_name: "altar", trigger: BumpActivate,
         effect: Multi([HealFull, SpawnItem(...)]),
         consume_on_use: false),
    ],
),

// Becomes:
PrefabTemplate(
    name: "Shrine",
    props: [(x: 2, y: 2, prop: "altar")],
),
```

The altar's effect data lives in `props.ron`, not duplicated per
prefab.

### Machine code path deleted

[src/game/machines.rs](../../src/game/machines.rs) is deleted.
Replaced by:
- `src/game/prop_effects.rs` — `TileEffect`, application functions,
  step/bump dispatch systems.
- `Effected` component (replaces `Machine` marker) — carries the
  `PropTrigger` payload copied from the PropAsset at spawn (effect,
  audience, mode).
- `EverFired(bool)` component — per-instance activation state.
  Always set to `false` at spawn; flipped to `true` on first
  activation. Read against `ActivationMode` to decide what to do
  next: `Repeating` ignores it; `OnceInert` short-circuits future
  activations; `OnceConsumed` despawns the entity. Persisted in
  save (see Migration).

`MachineBumpMessage` becomes `PropBumpMessage`. The bump-dispatch
logic in [src/game/actions.rs:438-516](../../src/game/actions.rs#L438)
switches its `BumpResult::Machine(...)` variant to
`BumpResult::ActivateProp(...)`.

### Floor materializer

[src/map/floor_materializer.rs:95-96](../../src/map/floor_materializer.rs#L95)
currently spawns separate Machine entities from `PrefabTemplate.triggers`.
After this RFC, prop spawning reads `PropAsset.trigger` and attaches
`Effected` + per-prop activation state in one pass when present. The
separate "machine spawn" loop is deleted.

## Migration Plan

Locked decisions (per RFC scoping interview):

1. **Full merge.** Delete Machine concept entirely.
2. **Decoration step effects: minimal lookup.** No `decorations.ron`
   schema change; per-variant effects live in code via
   `Decoration::step_effect`.
3. **Schema bump v9 → v10, clean break.** Old saves are unloadable.

### Sequence

1. **Land `TileEffect` + `prop_effects.rs`** without removing Machines.
   Both systems coexist for one commit window so tests can be
   written against the new shape against known-good Machine
   behavior.
2. **Migrate `prefabs.ron`** — Shrine entry rewritten to use a
   prop-level trigger on the altar (PlayerOnly + OnceInert + Multi[
   HealFull, SpawnItem]). Verified floor generation, on-bump behavior,
   single-use semantics. (Trapped Vault was deferred until step 5
   pending a resolve_bump priority decision; landed via option a
   below.)
3. **Wire `Decoration::step_effect`** for Cobweb (Slowed 3 turns).
   Add unit tests for the lookup. No new decoration variants and no
   other variants opt in during this RFC — Embers/Ash/Bloodstain/etc.
   remain silent flavor.
4. **Persist `EverFired` in saves.** Bump `SAVE_SCHEMA_VERSION`
   to 10 in [src/save/mod.rs](../../src/save/mod.rs#L101). Update
   the `schema_version_is_nine` test to `_is_ten`. Closes the
   latent bug where used Machines reset on reload.
5. **Delete `src/game/machines.rs`**, the `MachinesPlugin`
   registration in [src/game/mod.rs](../../src/game/mod.rs), and
   the `triggers:` array support in `PrefabTemplate`. Update
   `prefabs.ron` schema doc in
   [.claude/skills/prefab-designer/references/prefab-schema.md](../../.claude/skills/prefab-designer/references/prefab-schema.md).

### Audience expansion (closes the player-only Machine bug)

`EffectAudience::Anyone` is the new default. The Trapped Vault is
explicitly `PlayerOnly` for backward compatibility (its current
behavior). Future trap props can use `Anyone` to threaten monsters
too — the symmetric-combat pillar from CLAUDE.md.

## Tests

Unit tests in `src/game/prop_effects.rs` (mirrors combat resolver pattern):

- `step_effect_deals_damage_to_player`
- `step_effect_deals_damage_to_monster_when_audience_is_anyone`
- `step_effect_skips_monster_when_audience_is_player_only`
- `bump_effect_heals_full_and_spawns_item`
- `repeating_prop_fires_every_step`
- `once_inert_prop_fires_once_then_stays_silent`
- `once_consumed_prop_despawns_after_firing`
- `multi_effect_applies_in_order`
- `cobweb_decoration_applies_slowed_on_step`
- `embers_decoration_is_silent_on_step`
- `no_effect_when_decoration_is_silent_variant`

Save round-trip tests in `src/save/mod.rs`:

- `effect_used_flag_persists_across_save_load`
- `consumed_prop_stays_despawned_across_save_load`

## Files Touched

| File | Change |
|------|--------|
| `src/game/prop_effects.rs` | **NEW** — TileEffect enum, application systems, plugin |
| `src/game/machines.rs` | **DELETED** |
| `src/game/mod.rs` | Remove `MachinesPlugin`, register `PropEffectsPlugin` |
| `src/game/actions.rs` | `BumpResult::Machine` → `BumpResult::ActivateProp`; writer swap |
| `src/assets/mod.rs` | `PropAsset` gains 5 new fields |
| `src/map/floor_materializer.rs` | Drop the trigger-spawn loop; props carry effects |
| `crates/roguelike_engine/src/map/tile.rs` | Add `Decoration::step_effect` impl |
| `src/save/mod.rs` | Bump `SAVE_SCHEMA_VERSION` to 10; persist `EverFired` |
| `assets/props.ron` | Add `trigger` block to altar; new `campfire` entry with light + step trigger (DealDamage Fire) |
| `assets/prefabs.ron` | Rewrite Shrine + Trapped Vault to use prop-level effects; remove `triggers:` arrays |
| `docs/design/` | **NEW** `PROPS.md` — props as the unified interactive-prop layer; cross-link from existing docs |
| `CLAUDE.md` | Replace "Machine system" reference with "Prop effect system"; update file map |
| `.claude/skills/prefab-designer/references/prefab-schema.md` | Drop `triggers:` field; document the `trigger` block on props |

## Risks & Mitigations

- **Save schema break for in-progress runs.** Clean break per locked
  decision. Game is pre-1.0; no players to support. Mitigation: ship
  in a single commit-train so the cut-over is atomic.
- **Decoration `step_effect` fires per move and might double-fire on
  multi-tile moves.** Mitigation: gate on `Changed<Position>` with a
  one-frame debounce (same pattern the existing `machine_step_system`
  uses). Test: stepping diagonally through corner-shared decorations
  fires once per cell entered.
- **`EffectAudience::Anyone` could let stray monster spawns trigger
  Trapped Vault and break encounter pacing.** Mitigation: that prefab
  is explicitly `PlayerOnly`. New props are `Anyone` by default but
  authors opt-in to broader effects.
- **Sprite policy.** `campfire` needs a unique sprite per
  [`.claude/rules/placeholder-sprites.md`](../../.claude/rules/placeholder-sprites.md).
  Cannot reuse `watchfire.png`. Mitigation: Pillow-generated asset
  task included in the implementation phase.
- **Decoration step effects on the player's spawn tile.** If the
  player spawns onto a Cobweb (or any future hazardous decoration),
  the effect fires on turn 0. Mitigation: floor-init guard skips
  step effects on the player's initial position (one-frame); same
  pattern as the existing `NeedsExploredInit` flag.

## Deferred / Out of Scope

- **Full `decorations.ron` schema overhaul.** No new authoring
  surface for decorations. Per-variant effects stay in code via
  `Decoration::step_effect`. A future RFC can lift them into RON if
  modders need it.
- **`MachineGroup` (lever→gate wiring).** No active use today.
  Future RFC introduces a `prop_group` field when puzzle wiring
  becomes a need.
- **Light source unification (PropAsset vs Fire entity).** Out of
  scope. Both still use the engine's lighting pipeline; only the
  authoring story is split. A follow-up RFC can collapse them.
- **`tiles.ron` / `TerrainType` enum coupling.** Out of scope. Same
  pattern (RON + enum coupled by name string) but lower payoff to
  refactor.
- **New decoration variants.** This RFC wires a step effect to
  existing Cobweb only. Adding "burning grass" or "pool of
  flammable oil" decorations is content work for a later phase.

## Non-Goals

- Adding new game mechanics. Every behavior in this RFC must be
  expressible today via Machine or paired prop+code; this is a
  refactor of authoring surface, not a feature expansion.
- Changing how `Decoration` is stored on tiles. Still a packed enum
  on `Tile.decoration`. Still set by the propagator at gen-time.
- Changing how lights propagate or get computed. Same engine
  lighting pipeline.

## Open Questions

1. **Should the schema bump drop old saves silently or refuse to
   load?** Recommend refuse-to-load with a clear migration message,
   since `SAVE_SCHEMA_VERSION` bump already signals clean break.
2. **Does decoration step-effect cost a turn?** Recommend no — same
   as lava liquid damage today, applied as a side effect of the
   move that triggered it. Test will lock the behavior either way.
3. **Should the `campfire` prop also be flammable** (i.e., a real
   Fire entity colocated with the prop, so it spreads to adjacent
   grass)? Recommend defer — start as a static prop with damage.
   The Fire-entity path is a follow-up if campsites need to spread
   to ignite grass.
4. **Trapped Vault migration. Resolved via option (a).** The
   `resolve_bump` loop in [actions.rs](../../src/game/actions.rs)
   now prefers Collider-bearing entities over non-Collider entities
   when picking `bump_target` among non-faction candidates. This
   preserves the original "open chest → next step springs the trap"
   game feel: bumping the (chest + invisible monster_trap)
   colocation routes to the chest (which has Collider), the chest
   opens and despawns, and the player's next step onto the now-empty
   tile springs the trap. The "monster_trap" prop is a new
   non-blocking entity in `props.ron` with an OnceConsumed trigger
   that spawns 2 level-appropriate monsters.
