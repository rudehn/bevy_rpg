# Factions

Factions are political alignments that drive who attacks whom. Every actor in the dungeon — the player, every monster, every summoned creature — carries a `Faction` component, and the global `FactionMatrix` resource resolves any pair of faction names into one of three relations: `Hostile`, `Neutral`, or `Allied`. The matrix is the single source of truth used by combat resolution, AI target selection, ally healing, squad alerting, and weapon "Slaying" runics.

## Design Philosophy

- **Faction is political, species is biological.** A `Faction("Undead")` zombie and an `Undead` species lich are different concepts: one decides who they fight, the other decides what they are. See `Species` in `src/components.rs:155-185` and the matching `species` field on every monster archetype.
- **Symmetric, data-driven.** Hostility is defined once between two factions and applies in both directions. The matrix is loaded from `assets/factions.ron` at startup — no hardcoded faction relations in code.
- **Three-state, not boolean.** `Hostile` triggers combat; `Neutral` makes the two factions ignore each other; `Allied` is required for healing/buff targeting and squad alerting (members of the same faction are auto-allied).
- **Hostile-to-hostile combat is a tactical lever.** When two enemy factions occupy the same room, they fight each other and the player can slip past or pick off survivors. Kobolds and goblins, fungal pods and goblins, undead and orcs — these collisions create emergent combat puzzles.
- **Squads alert within faction.** Goblin scouts wake other goblins, not nearby kobolds. Faction match is the gate on cross-squad reinforcement.

## Data Model

### `Faction` component

```rust
// roguelike_engine/src/components/faction.rs:21
pub struct Faction(pub FactionKind);

// roguelike_engine/src/components/faction.rs:31
pub struct FactionKind(pub String);
```

Every actor (player + every monster) gets a `Faction`. The kind is a `String` newtype so games can ship arbitrary rosters without modifying the engine. Equality is **case-sensitive** (see `equality_is_case_sensitive` test, `roguelike_engine/src/components/faction.rs:96-102`).

### `FactionMatrix` resource

```rust
// roguelike_engine/src/factions/mod.rs:53
pub struct FactionMatrix {
    relations: HashMap<(String, String), Relation>,
}

pub enum Relation { Hostile, Neutral, Allied }
```

The matrix stores every directed pair `(A, B) → Relation`. When built from `from_entries`, each input triple `(A, B, X)` is inserted as both `(A, B, X)` and `(B, A, X)` so lookups are symmetric (`roguelike_engine/src/factions/mod.rs:60-67`).

### Lookup API

Three layers, picked by what the caller already has in hand:

```rust
// roguelike_engine/src/factions/mod.rs — string-based (most primitive)
fn is_hostile_to(&self, a: &str, b: &str) -> bool;
fn is_allied_to(&self, a: &str, b: &str) -> bool;
fn is_neutral(&self,   a: &str, b: &str) -> bool;

// roguelike_engine/src/factions/mod.rs — `&Faction` overloads (both
// factions known, typically inside an ECS query iter)
fn are_hostile(&self, a: &Faction, b: &Faction) -> bool;
fn are_allied(&self, a: &Faction, b: &Faction) -> bool;

// src/game/factions.rs — `Option<&Faction>` helpers (one side may
// lack the component) with the game's None-policy baked in
fn factions_hostile(a: Option<&Faction>, b: Option<&Faction>, m: &FactionMatrix) -> bool;
fn factions_allied (a: Option<&Faction>, b: Option<&Faction>, m: &FactionMatrix) -> bool;
fn faction_hostile_to_player(faction: Option<&Faction>, m: &FactionMatrix) -> bool;
```

Resolution rules:

| Case | Result |
|------|--------|
| `a == b` (same faction string) | `is_allied_to → true`, `is_hostile_to → false`, `is_neutral → false` |
| Pair found in matrix | The stored `Relation` |
| Pair **not** found in matrix | Defaults to `Neutral` (engine `FactionMatrix::get`) |
| `factions_hostile(None, _) / (_, None)` | `false` (neutral default — faction-less entities are inert in pairwise checks) |
| `factions_allied(None, _) / (_, None)` | `false` (same policy) |
| `faction_hostile_to_player(None)` | `true` — the **asymmetric** default that wakes unfactioned monsters into Hunting on first sight of the player (legacy behavior the awareness gate relies on) |

The asymmetric default in `faction_hostile_to_player` is the only place "no faction = hostile" applies; everywhere else, missing a `Faction` component means "skip this entity in the hostility check." Use the matching helper for each call site instead of recomputing the policy inline — it has bitten us before when one site missed the `None` arm.

### Veiled Tyrant faction constants

Game-specific names live in `src/components.rs:127-144`:

```rust
pub struct VeiledTyrantFactions;
impl VeiledTyrantFactions {
    pub const PLAYER:  &'static str = "Player";
    pub const MONSTER: &'static str = "Monster";
    pub const KOBOLD:  &'static str = "Kobold";
    pub const RAT:     &'static str = "Rat";
    // ...constructors returning FactionKind
}
```

Only a subset is materialized as constants — the full active roster lives in `assets/factions.ron` and `assets/monsters.ron`.

## Loading: `assets/factions.ron`

The relation list is a flat array of `(a, b, relation)` entries. Each entry is loaded once; symmetry is filled in by `FactionMatrix::from_entries`.

```ron
(
    relations: [
        ( a: "Player", b: "Goblin", relation: Hostile ),
        ( a: "Goblin", b: "Kobold", relation: Hostile ),
        // ...
    ],
)
```

The asset wires up via `RonAssetPlugin::<FactionMatrixAsset>` in `src/assets/mod.rs:136`, the handle is stored in `FactionMatrixHandle`, and `apply_faction_matrix_asset` (`roguelike_engine/src/factions/mod.rs:131-150`) builds the live `FactionMatrix` resource the first time the asset is observed. The plugin is `FactionsPlugin`, registered by the engine.

## Active Roster

Nine faction names appear in `factions.ron`: `Player`, `Monster`, `Goblin`, `Orc`, `Fungal`, `Undead`, `Giant`, `Dragon`, `Rat`, `Kobold`. Of these, eight have living monster archetypes in `assets/monsters.ron` (everything except `Monster`, which is a legacy umbrella retained so older entries that haven't been migrated to a specific faction still register as hostile to the player).

### Hostility Matrix

Below: `H` = Hostile, `N` = Neutral, `—` = same-faction (auto-Allied), blank = symmetric duplicate. Pairs are sourced from `assets/factions.ron`.

|         | Player | Monster | Goblin | Orc | Fungal | Undead | Giant | Dragon | Rat | Kobold |
|---------|:------:|:-------:|:------:|:---:|:------:|:------:|:-----:|:------:|:---:|:------:|
| Player  | —      | H       | H      | H   | H      | H      | H     | H      | H   | H      |
| Monster |        | —       | N      | N   | N      | N      | N     | N      | N   | N      |
| Goblin  |        |         | —      | N   | **H**  | **H**  | (def) | (def)  | N   | **H**  |
| Orc     |        |         |        | —   | (def)  | **H**  | (def) | (def)  | N   | N      |
| Fungal  |        |         |        |     | —      | N      | (def) | (def)  | N   | N      |
| Undead  |        |         |        |     |        | —      | (def) | (def)  | N   | N      |
| Giant   |        |         |        |     |        |        | —     | N      | N   | N      |
| Dragon  |        |         |        |     |        |        |       | —      | N   | N      |
| Rat     |        |         |        |     |        |        |       |        | —   | N      |
| Kobold  |        |         |        |     |        |        |       |        |     | —      |

`(def)` = pair is **not** declared in `factions.ron` and therefore resolves to the default `Neutral`. Undeclared pairs do not fight on sight; if you want two factions hostile, you must declare it. This makes adding new factions safe — you only spell out the relationships that should produce conflict.

### Active Hostility Pairs (firing in play)

The cross-faction `H` declarations that aren't `Player` vs anything:

- **Goblin ↔ Fungal** — green-skin warbands brawl with fungal pods at floor 4-7.
- **Goblin ↔ Undead** — when an undead crypt overlaps a goblin camp on floor 6-8.
- **Goblin ↔ Kobold** — overlapping floor 4-6 ranges, both have full archetype coverage.
- **Orc ↔ Undead** — orc camps (floor 8-10) and undead lairs (floor 7-10).

All four pairs have at least one populated archetype on each side, so all four can fire in real play when generation places overlapping spawn lists.

## Reading the Matrix from Game Code

Every faction-aware system clones the resource into its world callback (because most call sites are `World &mut` GOAP/AI dispatch routines that can't hold concurrent borrows). Examples:

- **Bump combat** — `resolve_bump` in `src/game/actions.rs:504` calls `factions_hostile(actor, target, &matrix)` to decide whether a bump becomes a melee attack vs. a "swap places" non-action.
- **Awareness gate / Idle→Hunting** — `update_mode` in `src/game/ai.rs` calls `faction_hostile_to_player(faction, matrix)`. This is the only site that defaults missing-faction to *Hostile*.
- **Monster ability target selection** — `src/game/ai.rs` (squad-alerted nearest enemy / wounded ally scans): the caster scans for the nearest entity where `is_hostile_to(self_kind, other_kind)` is true. These use the `&str` API because `caster_faction` is held as `Option<FactionKind>`.
- **War cry / pack tactics / rally / terrify** — `src/game/abilities.rs:695,723,764,793`: call `faction_matrix.are_allied(self, other)` or `.are_hostile(...)` directly on `&Faction` refs returned from the iter.
- **AoE friendly fire avoidance** — `src/game/abilities.rs:793`: AoE filters out anyone the source `are_allied` with.
- **Fleeing** — `src/game/fleeing.rs:191`: `factions_hostile(my_faction, other_faction, &matrix)` decides whether a visible nearby entity counts as a threat to flee from.
- **Player target picker** — `src/game/targeting.rs:112`: cycles between visible entities the player is **not** `are_allied` with, so allies can't be accidentally zapped.

## AI Integration

A monster's GOAP/FSM never picks "target the player" directly — it picks "the nearest hostile entity." When two enemy factions share a room, this is what produces fight-each-other behavior:

1. A goblin enters the player's FOV. Its perception loop scans visible entities, filters by `is_hostile_to("Goblin", other)`.
2. Both the player (`Player`, hostile) and a kobold (`Kobold`, hostile) match. AI tie-breakers (distance, ability priority, GOAP utility) pick a target.
3. The goblin attacks whichever scored higher. The kobold and player are both hostile to the goblin, so they may also engage it.

This three-way combat is why "let monsters fight each other" is a real player tactic: when generation drops overlapping spawn lists into the same chamber, the player can hang back and pick winners.

### Squad Alerting Within Faction

Squad scouts and shared-FOV alerts only propagate to monsters of the **same** faction (see `src/game/squad.rs` and the faction-match comment at `roguelike_engine/src/squad/mod.rs:382`). A goblin scout shouting will not wake adjacent sleeping kobolds — kobolds are hostile to goblins and would attack on alert anyway. See [SQUAD_AI.md](SQUAD_AI.md) for the cross-squad alert flow.

## Edge Cases & Resolved Decisions

- **Symmetric only.** No support for "A hates B but B does not hate A." All relations are bidirectional. Asymmetric grudge mechanics, if ever wanted, would need a separate component (e.g., `PersonalGrudge`).
- **No allied-NPC state for the player.** `Player` is its own faction with no `Allied` partners in `factions.ron`. Future companions/charmed monsters will need an `Allied` row added (or a temporary faction reassignment).
- **No reputation / single-bit hostility.** There is no per-monster reputation, no "wounded then withdrew" cooldown, no decay. A pair is hostile or it isn't. A monster can be temporarily tamed only by replacing its `Faction` component or muting its AI via status effects (Charmed/Confused).
- **Missing `Faction` component.** Resolves through `factions_hostile` / `factions_allied` — `(None, _)` or `(_, None)` returns `false`, so bumping a faction-less prop falls through to the destructible/chest/collider checks (`src/game/actions.rs:504`). The exception is the player-awareness gate `faction_hostile_to_player`, which defaults `None` to `true` so unfactioned monsters still wake up and hunt the player. Practically, every spawned actor has a `Faction`; props deliberately do not (chests, watchfires, machine props).
- **Unknown faction name.** Resolves to `Neutral` against everything (default in `FactionMatrix::get`). Misspellings in monster RON produce a "won't fight, won't be fought" creature. Easy to miss visually — verify faction strings against `factions.ron` when adding monsters.
- **Same-faction combat.** Forbidden at the lookup layer: `is_hostile_to(a, a)` always returns `false` regardless of matrix contents. There is no "civil war" mode.
- **Slaying runics reference faction strings.** `WeaponRunic::Slaying { faction: String }` (`src/game/enchantment.rs:51`) tags a weapon with a damage bonus vs. a specific faction name. The pool is `["Goblin", "Dragon", "Undead", "Kobold", "Monster"]` — `Monster` is the universal-bane catch-all. The string must match an actual faction in `monsters.ron` or the runic is dead loot.

## Cross-Links

- [ENEMIES.md](ENEMIES.md) — enumerates every faction's roster (members, depth ranges, abilities) and which factions are populated.
- [SQUAD_AI.md](SQUAD_AI.md) — squad alert propagation gates on `is_allied_to(scout, sleeper)`; multi-faction encounters keep separate squads.
- [GAME.md](GAME.md) — combat flow that consumes `is_hostile_to` to decide bump-into-attack and target eligibility.
- [ITEMS.md](ITEMS.md) — `Slaying` runic mechanic and the SLAYING_FACTIONS pool.
