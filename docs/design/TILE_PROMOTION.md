# Tile Promotion

Brogue-aligned per-tile timed transitions. The promotion system is the single
place where map cells change on a timer (cracked floor collapses into a chasm,
trampled grass regrows, open doors swing closed). On-step transitions
(walking through tall grass, getting tangled in cobwebs) live in
`handle_movement` and are documented separately.

## Design philosophy

Terrain is dynamic. A floor is not a fixed picture — it weathers as the player
fights through it. An explosion cracks the stone, and a few turns later the
crack widens into a void; vegetation the player crushed underfoot eventually
returns; a door the player kicked open will swing back if no one is standing
in it. Brogue's approach is to give every cell a small, independent chance to
"promote" each turn, expressed as a 0–10000 probability scale. We adopt the
same model directly so that mechanics tune to the same numeric intuitions.

All timed transitions funnel through one system. Adding a new one is a matter
of adding a `PromotionRule` to the relevant `timed_promotion()` impl — no new
plugin, no new tick.

## Data model

The entire tile-promotion pipeline — rules, the tick system, the
mutation messages it writes, and the apply systems that synchronise
`Map` ↔ tile entities — lives in the **engine** crate
(`roguelike_engine`). The game crate keeps only:
1. Re-exports through `src/game/tile_promotion.rs` and `src/map/tile.rs`
   so existing call sites compile unchanged.
2. The `chasm_fall_reaction_system` in `src/map/tile.rs`, which
   subscribes to the engine's `LiquidMutationMessage` and runs all the
   game-specific consequences of a tile becoming a chasm (player/monster
   fall, lava-kill, forced floor transition).

| Item | Where | Notes |
|------|-------|-------|
| `PromotionRule` | `roguelike_engine::map::tile` | `{ target, chance_per_turn: u16 }` — chance is on Brogue's 0–10000 scale. |
| `PromotionTarget` | `roguelike_engine::map::tile` | `Decoration(Decoration)` / `Terrain(TerrainType)` / `Liquid(LiquidType)`. |
| `Decoration::timed_promotion()` | `roguelike_engine::map::tile` | Defines decoration-driven transitions. |
| `TerrainType::timed_promotion()` | `roguelike_engine::map::tile` | Defines terrain-driven transitions. |
| `PromotionCooldown` resource | `roguelike_engine::map::promotion` | `HashSet<(i32, i32)>` of tiles that mutated externally this turn. Drained at the top of every tick. |
| `tile_promotion_tick_system` | `roguelike_engine::map::promotion` | Engine-owned tick. Game registers it via `TilePromotionPlugin`. |
| `TilePromotionPlugin` / `TilePromotionSet` | `roguelike_engine::map::promotion` | Engine plugin + set marker. The game configures `TilePromotionSet.in_set(ProcessingPhase::Cleanup)` (`src/game/turns.rs`). |
| `MapMutationPlugin` / `MapMutationSet` | `roguelike_engine::map::mutation` | Apply systems that sync `Map`, tile entities, `Viewshed.dirty`, `LightSources.dirty`, `Collider`, `PromotionCooldown`, plus universal physics like `Decoration::CrackedFloor` → `TerrainType::Floor` and `Decoration::Fungus` → `fungal_light`. |
| `chasm_fall_reaction_system` | `src/map/tile.rs` (game) | Reads `LiquidMutationMessage`, runs `.after(MapMutationSet)`, handles fall/lava-kill/transition. |

The cooldown set is populated by the engine apply systems
(`apply_tile_mutations`, `apply_decoration_mutations`,
`apply_liquid_mutations`). It exists to prevent same-turn thrash: e.g.
a `LiquidMutationMessage` that turns a cracked-floor tile into a chasm
runs in Cleanup *after* the promotion tick has already fired — but if a
future system writes a fresh mutation followed by a fresh promotion in
the same frame, the cooldown blocks the second pass from re-promoting
the just-changed tile.

## Promotion rules (current)

| Source layer | Source | Target | Chance / turn | Source line |
|--------------|--------|--------|---------------|-------------|
| Decoration | `TrampledGrass` | Decoration `TallGrass` | 100 / 10000 (~1%) | `roguelike_engine/src/map/tile.rs:210` |
| Decoration | `TrampledFungus` | Decoration `Fungus` | 100 / 10000 (~1%) | `roguelike_engine/src/map/tile.rs:214` |
| Decoration | `Embers` | Decoration `Ash` | 1000 / 10000 (10%) | `roguelike_engine/src/map/tile.rs:218` |
| Decoration | `CrackedFloor` | Liquid `Chasm` | 3300 / 10000 (~33%) | `roguelike_engine/src/map/tile.rs:222` |
| Terrain | `OpenDoor` | Terrain `Door` | 10000 / 10000 (100%) | `roguelike_engine/src/map/tile.rs:78` |

Trampled vegetation regrowth is the Brogue value. Cracked floor uses ~33% so
that in practice an explosion-cracked tile collapses within roughly 3 turns —
fast enough to be a threat the player must react to, slow enough that the
crack reads as a warning rather than an instant pit.

## Tick system

`tile_promotion_tick_system` (engine) runs inside `TilePromotionSet`,
which the game configures into `ProcessingPhase::Cleanup`, after the
game-side per-turn ticks (status expiry log, fire, gas) and before
`MapMutationSet`. After mutation apply, the game's
`chasm_fall_reaction_system` runs and finally `continue_turn_processing`
advances to the next actor (`src/game/turns.rs`).

Per turn the system:

1. Reads `TurnEndEvent` (engine-owned). If none fired this frame,
   returns immediately — so the tick is genuinely once per turn, not per
   frame.
2. Snapshots `PromotionCooldown` and clears the resource.
3. Builds a snapshot of occupied positions from
   `Query<&Position, With<Collider>>` (used to skip closing doors on
   top of creatures).
4. Walks every tile via `(0..height, 0..width)`. For each tile it asks
   the decoration layer first, then the terrain layer, for a
   `timed_promotion()` rule and rolls
   `rng.range(0, 10000) < chance_per_turn`.
5. Emits a `TileMutationMessage` / `DecorationMutationMessage` /
   `LiquidMutationMessage` based on the rule's target. The engine's
   `MapMutationSet` apply systems consume them later in the same
   Cleanup phase.

Promotion is **probabilistic per turn**, not a deterministic countdown. This
matches Brogue's behavior and keeps the data model trivial — no per-tile
counter to serialize, no need to tick every cell separately. The chance is
the only knob.

The walk is global, **not** per-tile-tick. We did not consider it worth
indexing only "promotable" tiles: 80 × 60 cells × one match arm is far below
any cost we care about, and a global pass keeps the rules honest (any tile
type can opt in by returning `Some(rule)`).

## Cracked floor → chasm

This is the highest-impact rule and motivates much of the surrounding
plumbing. When an explosion cracks a tile, the engine's
`apply_decoration_mutations` normalises the underlying terrain to
`Floor` so cracks on walls render correctly and the promotion rule
applies uniformly. Then on subsequent turns the cracked decoration
rolls 33% to convert into `LiquidType::Chasm`. When that fires:

1. The engine's `apply_liquid_mutations` does the data sync: writes
   `decoration = Decoration::None`, updates the tile entity's liquid
   component, removes `Collider`, marks viewsheds dirty, inserts the
   tile into `PromotionCooldown`.
2. The game's `chasm_fall_reaction_system` runs `.after(MapMutationSet)`,
   reads the same `LiquidMutationMessage`, and handles falling entities
   — the player gets a forced floor transition with 2d6 fall damage,
   monsters are saved into `FallenEntities` for the floor below, items
   tumble. See `CHASMS.md` for the full chasm flow.

## Trampled vegetation regrowth

`Decoration::TallGrass` and `Decoration::Fungus` both have on-step promotions
into their trampled forms (`roguelike_engine/src/map/tile.rs:201-202`). The
trampled forms then regrow on the timed tick. The two promotions are
deliberately mirror images — walk over a clump, leave a trail, come back later
and the trail is gone. ~1%/turn means a trampled tile takes on average 100
turns to regrow, which feels long enough that a single fight does not erase
itself but short enough that exploration over the whole floor smooths the path
back out.

`Decoration::Embers` regrowing to `Ash` is the cleanup half of the fire
spread system: scorched vegetation leaves embers, embers fade to ash, ash is
purely cosmetic. Fire itself is **not** a decoration and **not** a promotion
target — see `FIRE.md` once that doc exists; for now, fire lives in
`src/game/fire.rs` as its own message-driven system.

## Cooldown and anti-thrash

Externally-driven mutations (chasm collapses from explosions, doors
slammed by abilities, liquids dropped by spells) write the same three
mutation messages that promotion uses. The engine's apply systems
insert the mutated tile into `PromotionCooldown.0`.

The promotion tick reads and clears the set at the top of its run.
Today only the terrain branch consults the cooldown — decorations don't
need it because regrowth is slow enough that a single-turn revert isn't
possible, and liquids don't have any timed promotions. The terrain
check exists primarily so that a door someone just opened (producing a
`TileMutationMessage`) doesn't get re-closed by the 100% promotion on
the same frame.

The cooldown lifetime is one turn: it's drained at every tick start, so no
stale entries accumulate.

## Edge cases and resolved decisions

- **Multiple competing rules.** Each tile has at most one decoration rule and
  at most one terrain rule. The decoration rule runs first and the terrain
  rule is then evaluated independently — they could in theory both fire on
  the same tile in the same frame, but no current rule pairing produces a
  conflict.
- **Opacity changes via promotion.** No current rule changes opacity (all
  trampled/regrown vegetation has the same FOV behavior). If a future rule
  does — e.g. shutters that close on a timer — the apply-systems already mark
  every viewshed dirty when terrain changes (`src/map/tile.rs:346`,
  `:415`), so adding a new rule that flips `is_opaque()` is safe by default.
- **Fire.** Fire spreads via `flammability()` on `Decoration` / `TerrainType`
  (`roguelike_engine/src/map/tile.rs:172`), not via the promotion table.
  Tall grass burning to embers is fire-domain, not promotion-domain. Embers
  → ash *is* promotion (it's a passive fade, not an ignition event).
- **Doors blocked by creatures.** The terrain branch skips tiles in the
  `occupied` set (`src/game/tile_promotion.rs:79`) so the door doesn't try
  to close on a standing creature.
- **Doors and locks.** `LockedDoor` has no timed promotion — only `OpenDoor`
  closes automatically. A locked door reverts only when explicitly unlocked
  via `UnlockDoorIntent`.
- **Save/load.** Promotions are stateless: `Tile` already serializes its
  three layers, and `PromotionCooldown` is intentionally a one-turn ECS
  resource that does not need persistence (a save resumes at turn boundary,
  cooldown reset to empty is correct).

## Cross-links

- `CHASMS.md` — the cracked-floor → chasm pipeline and the player/monster fall
  flow.
- `DUNGEON.md` — terrain/liquid/decoration layers, where the data lives in
  `Map`, and how `apply_*_mutations` integrates with the ECS tile entities.
- `ENCOUNTERS.md` — explosion sources that lay cracked floor in the first
  place.
- (no `FIRE.md` yet — fire spread lives in `src/game/fire.rs` and is a
  candidate for documentation; embers → ash promotion is the only seam where
  the two systems touch.)
- (no `STATUS_EFFECTS.md` interaction — promotion runs on tiles, status
  effects run on entities, and they do not currently cross.)
