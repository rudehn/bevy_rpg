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

```rust
// roguelike_engine/src/factions/mod.rs:69-95
fn is_hostile_to(&self, a: &str, b: &str) -> bool;
fn is_allied_to(&self, a: &str, b: &str) -> bool;
fn is_neutral(&self,   a: &str, b: &str) -> bool;
fn get(&self,          a: &str, b: &str) -> Relation;  // private
```

Resolution rules (encoded in the public methods):

| Case | Result |
|------|--------|
| `a == b` (same faction string) | `is_allied_to → true`, `is_hostile_to → false`, `is_neutral → false` |
| Pair found in matrix | The stored `Relation` |
| Pair **not** found in matrix | Defaults to `Hostile` (`get()`, line 90-95) |

The "missing pair = Hostile" default is load-bearing: any monster whose faction is misspelled or absent from `factions.ron` becomes hostile to everyone, including its supposed allies. This fails loud rather than silently making rogue monsters peaceful.

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

- **Bump combat** — `resolve_bump` in `src/game/actions.rs:453,504` checks `faction_matrix.is_hostile_to(&actor.0.0, &target.0.0)` to decide whether a bump becomes a melee attack vs. a "swap places" non-action.
- **Monster ability target selection** — `src/game/ai.rs:447-456`: the caster scans for the nearest entity where `is_hostile_to(self_faction, other_faction)` is true.
- **Healing target selection** — `src/game/ai.rs:467-478` and `src/game/abilities.rs:695,723,764`: heal/buff abilities require `is_allied_to(caster, target)`. Same-faction members satisfy this automatically.
- **AoE friendly fire avoidance** — `src/game/abilities.rs:782-793`: AoE filters out anyone the source `is_allied_to`.
- **Adjacent-enemy detection (flee/cower triggers)** — `src/game/ai.rs:309-325`: `has_adjacent_enemy` walks the 8 neighbors and matches against `is_hostile_to`.
- **GOAP perception** — `src/game/goap.rs:790` and `src/game/goap/dispatch.rs:100`: enemy visibility uses the same hostility predicate, so a goblin "sees an enemy" only when the visible entity's faction is hostile to its own.
- **Player target picker** — `src/game/targeting.rs:112`: the player's targeting cursor cycles between visible entities that are **not** allied to the player, so allies can't be accidentally zapped.

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
- **Missing `Faction` component.** Bump resolution treats a missing faction as "no hostility check possible" and falls through to the destructible/chest/collider checks (`src/game/actions.rs:503-506`). Practically, every spawned actor has a `Faction`; props deliberately do not (chests, watchfires, machine props).
- **Unknown faction name.** Resolves to `Neutral` against everything (default in `FactionMatrix::get`, line 90-95). Misspellings in monster RON produce a "won't fight, won't be fought" creature. Easy to miss visually — verify faction strings against `factions.ron` when adding monsters.
- **Same-faction combat.** Forbidden at the lookup layer: `is_hostile_to(a, a)` always returns `false` regardless of matrix contents. There is no "civil war" mode.
- **Slaying runics reference faction strings.** `WeaponRunic::Slaying { faction: String }` (`src/game/enchantment.rs:51`) tags a weapon with a damage bonus vs. a specific faction name. The pool is `["Goblin", "Dragon", "Undead", "Kobold", "Monster"]` — `Monster` is the universal-bane catch-all. The string must match an actual faction in `monsters.ron` or the runic is dead loot.

## Cross-Links

- [ENEMIES.md](ENEMIES.md) — enumerates every faction's roster (members, depth ranges, abilities) and which factions are populated.
- [SQUAD_AI.md](SQUAD_AI.md) — squad alert propagation gates on `is_allied_to(scout, sleeper)`; multi-faction encounters keep separate squads.
- [GAME.md](GAME.md) — combat flow that consumes `is_hostile_to` to decide bump-into-attack and target eligibility.
- [ITEMS.md](ITEMS.md) — `Slaying` runic mechanic and the SLAYING_FACTIONS pool.
