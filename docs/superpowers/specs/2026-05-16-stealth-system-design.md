# Stealth System — Design Spec

**Date:** 2026-05-16
**Status:** Draft (awaiting user review before implementation plan)
**Related docs:** [CHARACTER.md](../../design/CHARACTER.md) · [SKILLS.md](../../design/SKILLS.md) · [SQUAD_AI.md](../../design/SQUAD_AI.md) · [LIGHT.md](../../design/LIGHT.md)

## 1. Goal

Replace the existing binary `MonsterAIMode::Asleep → Hunting` transition with a **per-perceiver, per-target awareness model** driven by an **opposed d20 roll** each turn. Make Stealth a first-class trainable skill, give the Rogue a real stealth gameplay loop, and scaffold a **noise map** so a future phase can layer Dijkstra-based sound propagation in without reshaping the formula.

## 2. Core decisions (locked)

| Decision | Choice |
| --- | --- |
| Awareness ownership | Per-monster, target-keyed map. Symmetric — monsters track awareness of each other too in V1. |
| States | 4 states: Hidden / Suspicious / Searching / Aware |
| Detection roll | Opposed d20: `d20 + perception_mod` vs `d20 + stealth_mod`. Perception wins → immediate `Aware`. |
| Aware stickiness | Once `Aware`, no further rolls. Only path out: lose LOS → `Searching` → search-timer expires → `Hidden`. |
| Search decay | DCSS-style timer (~20 game turns at last-known-pos). |
| Backstab gate | Strictest: only fires when target's awareness about player is `Hidden`. |
| V1 modifier set | Stealth skill, DEX mod, armor encumbrance, tile light intensity, distance (perceiver-side close-range bonus), per-monster `perception` stat. |
| Noise map | Scaffold now: `Vec<i32>` per tile, decay-by-1-each-turn system. No source writes in V1. Modifier returns negative penalty. |
| UI surface | Status pill below each monster in the nearby sidebar. Notice-chance + inline breakdown in hover tooltip and monster info screen. |
| Persistence | Degraded: on save, `Aware` collapses to `Searching{last_known=player.pos}`; Suspicious/Searching → Hidden. Avoids entity-ID stability work. |

## 3. State machine

```
Hidden ───perception wins (in LOS)──→ Aware
Hidden ───(out of LOS / no roll)─────→ Hidden  [no-op]

Aware ───target leaves LOS──→ Searching { last_known_pos: target.pos,
                                          giveup_at_turn: now + 20 }
Aware (target still in LOS) ────────→ Aware    [no roll — sticky]

Searching ──perception wins (in LOS)──→ Aware
Searching ──giveup_at_turn reached────→ Hidden
Searching ──(no LOS, timer alive)─────→ Searching

Suspicious ──perception wins (in LOS)──→ Aware
Suspicious ──decay_at_turn reached─────→ Hidden
```

**Suspicious** is reachable in V1 only through the future noise-map populator. Its variant, transitions, and decay tick ship in V1 but no system enters the state — when the V2 noise phase lands, a single new handler (`noise_event → Suspicious{suspect_pos}`) lights it up.

**Asleep** is a *behavior mode* (`MonsterAIMode`), not an awareness state. It maps to `AwarenessState::Hidden` plus a flat **`−10` penalty** on the seeker's perception roll. Sleeping monsters can still wake from a clumsy stealth attempt — they're just deeply biased toward staying asleep.

## 4. Data model

### 4.1 Engine crate — `roguelike_engine::stealth`

```rust
pub enum AwarenessState {
    Hidden,
    Suspicious { suspect_pos: Point, decay_at_turn: u32 },
    Searching  { last_known_pos: Point, giveup_at_turn: u32 },
    Aware,
}

pub struct AwarenessRecord {
    pub state: AwarenessState,
    pub last_update_turn: u32,
}

#[derive(Component, Default)]
pub struct Awareness {
    pub records: HashMap<Entity, AwarenessRecord>,
}
```

The `Awareness` component lives on **every actor that can perceive** — both monsters and the player. In V1 only monster-side records are populated (monsters track awareness of the player and of each other). The player's `Awareness` ships as an empty map, reserved for future stealthed-monster gameplay where the player would need to roll perception against invisible enemies.

```rust

#[derive(Resource)]
pub struct NoiseMap {
    pub tiles: Vec<i32>,   // value at each tile; decremented by 1 each turn to a floor of 0
    pub width: usize,
    pub height: usize,
}
```

### 4.2 Engine crate — `MonsterAsset` schema extension

`MonsterAsset` (in `roguelike_engine::monster::asset`) gains:

```rust
pub perception: i32,   // default 0; authored in monsters.ron per species
```

Recommended starting values: goblin grunt `0`, archer `+1`, commander `+3`, brute `−1`, hound `+5`, rat `+2`, kobold hoarder `+1`. These ship as RON edits in the implementation plan, not part of this design.

### 4.3 Game crate — `src/game/stealth.rs`

Owns the modifier composition (which needs game-side components: `Skills`, `Attributes`, equipped armor, `Position`, `LightMap`). Exposes:

```rust
pub fn compute_stealth_mod(target: Entity, world: &World) -> i32;
pub fn compute_perception_mod(seeker: Entity, target: Entity, world: &World) -> i32;
pub fn notice_probability(delta: i32) -> f32;   // re-exported from engine
```

### 4.4 Game crate — items.ron extension

Each armor item gains an `armor_stealth_penalty: i32` field (default 0). Starting values (placeholder, expect tuning):

| Armor | `armor_stealth_penalty` |
| --- | --- |
| Cloth Wraps / Robe | 0 |
| Padded Armor | 1 |
| Leather | 1 |
| Studded Leather | 2 |
| Chain | 3 |
| Plate | 5 |

## 5. Detection formula

The opposed roll fires **on the perceiver's turn**, only against entities that:
- are in the perceiver's `Viewshed.visible_tiles`, AND
- the perceiver's `Awareness.get(target).state` is one of `Hidden`, `Suspicious`, `Searching`. (Sticky `Aware` skips the roll.)

```
seeker_total = d20 + perception_mod
target_total = d20 + stealth_mod

if seeker_total > target_total:
    awareness.set(target, Aware)
    emit AwarenessAlertEvent { seeker, target }
```

### 5.1 `perception_mod`

```
perception_mod = monster.perception          // -3..=+5 per species
              + (-10 if MonsterAIMode == Asleep)
              + close_range_bonus(dist_seeker_to_target)
```

`close_range_bonus`:
- adjacent (Chebyshev distance 1) → `+2`
- distance 2..=3 → `+1`
- distance ≥ 4 → `0`

### 5.2 `stealth_mod`

```
stealth_mod = floor(Stealth_skill / 2)               // 0..=13 across 0..=27
            + DEX_mod                                 // typically -2..=+5
            - armor_stealth_penalty                   // 0..=5
            + light_modifier(target_tile, LightMap)
            + noise_modifier(target_pos, NoiseMap)    // = -map.at(pos); V1 stub returns 0
```

`light_modifier` reads `LightMap.intensity_at(pos)` (existing engine resource):
- `>= 0.75` (bright/torch-adjacent) → `−3`
- `>= 0.40` (dim) → `−1`
- `< 0.40` (dark) → `+2`
- `== 0.00` (no light source at all) → `+3`

Thresholds are placeholders; expect post-impl tuning. These constants live as named consts in `src/game/stealth.rs` for easy adjustment.

`noise_modifier`:
```rust
pub fn noise_modifier(pos: Point, map: &NoiseMap) -> i32 { -map.at(pos) }
```
Returns 0 for V1 (no producers); returns a negative penalty in V2 once a populator writes to the map.

### 5.3 Probability helper

Engine-side, in `roguelike_engine::stealth`:

```rust
pub fn notice_probability(delta: i32) -> f32 {
    let mut wins = 0u32;
    for x in 1..=20i32 {
        for y in 1..=20i32 {
            if x + delta > y { wins += 1; }
        }
    }
    wins as f32 / 400.0
}
```

Sentinel test values: `delta = 0 → 0.475`, `delta = +10 → 0.952`, `delta = −10 → 0.025`, `delta = +20 → 1.0`, `delta = −20 → 0.0`.

### 5.4 Worked examples

**L1 Rogue, dim corridor, padded armor, adjacent goblin grunt:**
- `perception_mod = 0 (species) + 0 (awake) + 2 (adjacent) = +2`
- `stealth_mod = floor(0/2) + 2 (DEX) − 1 (padded) − 1 (dim) + 0 = 0`
- delta = +2 → notice probability ≈ 56%. **Unreliable to ambush at adjacent.**

**XL 12 Rogue, dark room, cloth wraps, 5 tiles from sleeping goblin commander:**
- `perception_mod = +3 (commander) − 10 (asleep) + 0 (dist 5) = −7`
- `stealth_mod = floor(10/2) + 4 (DEX) − 0 (cloth) + 2 (dark) + 0 = +11`
- delta = −18 → notice probability ≈ 1%. **Reliably hidden.**

## 6. Behavior integration

### 6.1 `perception_tick_system` (new, engine-side)

Runs at the start of each perceiver's turn, inside `ProcessingPhase` before `MonsterAI::execute`. For each entity in the perceiver's viewshed where awareness is not `Aware`:
1. Build `perception_mod` and `stealth_mod` via the game-side compute functions.
2. Roll opposed d20s.
3. On perception-win, set state to `Aware` and emit `AwarenessAlertEvent`.

### 6.2 `awareness_tick_system` (new, engine-side, runs once per game turn)

- Tick `Searching.giveup_at_turn` and `Suspicious.decay_at_turn`; expired records → `Hidden`.
- For each `Aware` record, verify target is still in perceiver's viewshed. If not, transition to `Searching { last_known_pos: <last seen pos>, giveup_at_turn: now + 20 }`. The "last seen pos" requires the perceiver to remember a previous frame's target position — store it on `AwarenessRecord` as `last_seen_pos: Option<Point>` updated each tick.
- GC `Hidden` records older than 200 turns to prevent map bloat.

### 6.3 `noise_decay_system` (new, engine-side, runs once per game turn)

```rust
for cell in &mut noise_map.tiles {
    *cell = (*cell - 1).max(0);
}
```

V1 only decay. V2 noise-source phase adds a populator that writes positive values from action events.

### 6.4 `MonsterAIMode` driven by awareness

Replaces the existing `is_player_visible` check in `MonsterAI::update_mode`. New logic (engine-side):

```rust
let highest = awareness.highest_state_against_hostiles();
match highest {
    Aware               => { mode = Hunting; target = current_pos_of_hostile; }
    Searching{pos,..}   => { mode = Hunting; target = pos; }
    Suspicious{pos,..}  => { mode = Idle;    investigation_target = Some(pos); }
    Hidden              => { mode = default_for_species; /* Asleep if authored, else Idle patrol */ }
}
```

State ordering (highest → lowest): `Aware > Searching > Suspicious > Hidden`. `highest_state_against_hostiles` walks the awareness records, filters to hostiles via `FactionMatrix`, and returns the strongest variant present (defaulting to `Hidden` when no hostile is tracked).

No new `MonsterAIMode` variants. Investigation rides on existing Idle pathing with a temporary target tile.

### 6.5 Squad propagation

A handler on `AwarenessAlertEvent` reads the alerted perceiver's `Squad` component and, for each squadmate, writes their `Awareness.get(target)` to:

```
Searching { last_known_pos: target.pos, giveup_at_turn: now + 20 }
```

Squadmates downgrade to **Searching**, not Aware. They begin investigating the spotted position; they only become Aware if they actually roll perception against the player. This avoids "radar squads" while preserving the existing shared-LOS feel.

### 6.6 Reaction to being attacked

A handler on `DamageTakenEvent` forces the attacker (if known) into the victim's `Aware` state regardless of stealth roll. Attacking always reveals you — with the standard Backstab exception, which is computed *before* the awareness update on the same frame.

### 6.7 Backstab gate update

[src/game/combat.rs:374-383](../../src/game/combat.rs#L374-L383) currently checks `MonsterAIMode == Asleep`. New check:

```rust
let is_hidden = monster
    .awareness
    .get(&player_entity)
    .map(|r| matches!(r.state, AwarenessState::Hidden))
    .unwrap_or(true);   // unknown target == fully Hidden

if is_hidden && weapon_ability == Some("Backstab") {
    // triple damage
}
```

Asleep monsters map to Hidden, so they still take Backstab. Searching / Suspicious / Aware all reject.

## 7. Stealth skill

### 7.1 Enum extension

`Skill` (in [src/game/skills.rs](../../src/game/skills.rs#L17)) gains a `Stealth` variant — 10th skill. Touches:
- `Skill::all()`, `Skill::name() → "Stealth"`
- `Skills` struct (per-skill level)
- `SkillXp`, `SkillTraining`, `SkillUseCounters`
- Skill screen UI auto-discovers via `Skill::all()` — no special-casing needed
- `SkillDistribution` and `SkillAptitudes` in [src/character/asset.rs](../../src/character/asset.rs) gain a `stealth` field — maintenance contract tests (`every_class_starting_skills_sums_to_ten`, aptitude range) automatically cover it

### 7.2 Class starting allocations

Currently each class's `starting_skills` sums to 10 across 9 skills. With Stealth added, redistribute:

| Class | Current (sums to 10) | Proposed (sums to 10) |
| --- | --- | --- |
| Warrior | fighting 3, long_blades 2, axes 2, armor 2, shields 1 | **unchanged** (stealth 0) |
| Rogue | fighting 1, short_blades 4, ranged_weapons 1, dodging 3, evocations 1 | fighting 1, short_blades 3, ranged_weapons 1, dodging 2, evocations 1, **stealth 2** |
| Mage | short_blades 1, dodging 2, evocations 7 | **unchanged** (stealth 0) |
| Ranger | fighting 2, long_blades 1, ranged_weapons 4, armor 1, dodging 2 | fighting 2, long_blades 1, ranged_weapons 4, armor 1, dodging 1, **stealth 1** |

Rogue gains 2 ranks of Stealth (drops 1 short_blade, 1 dodging). Ranger gains 1 rank of Stealth (drops 1 dodging).

### 7.3 Race aptitudes

Each race's `aptitudes` block gains a `stealth` field:

| Race | `stealth` aptitude |
| --- | --- |
| Human | `0` (Adaptive — trains at standard rate) |
| Dwarf | `−2` (heavy gait fits the Stoneblood / Armor identity) |
| Elf | `+2` (Keen senses also avoid being sensed) |

### 7.4 Use-counter rule

`SkillUseCounters.stealth += 1` at the end of each game turn when:
- ≥1 hostile is in the player's viewshed, AND
- for ≥1 such hostile, its `Awareness.get(player).state != Aware`

In other words: every turn you successfully remain un-fully-detected with detectors around, you train Stealth. Pure combat doesn't train it (everyone in your LOS is already Aware). Sneaking past asleep monsters in a dark corridor trains it heavily.

## 8. UI

### 8.1 Nearby sidebar — status pill

[src/ui/nearby.rs](../../src/ui/nearby.rs) gains a small `Text` child below each visible monster's row:

| Pill text | Trigger | Color |
| --- | --- | --- |
| Sleeping | `MonsterAIMode == Asleep` | dim grey |
| Wandering | `Hidden` & non-Asleep (Idle with no investigation target) | grey |
| Suspicious | `AwarenessState::Suspicious` | yellow |
| Searching | `AwarenessState::Searching` | yellow |
| Hunting | `AwarenessState::Aware` | red |

### 8.2 Monster hover tooltip & monster info overlay

[src/ui/hover_info.rs](../../src/ui/hover_info.rs) and [src/ui/monster_info.rs](../../src/ui/monster_info.rs) both gain a "Stealth" section, always-on inline (no nested hover for V1):

When `awareness.state != Aware` AND target in monster's viewshed:
```
─ Stealth ─────────────────
Notice this turn: 87%
  Perception: +5
    base species:  +3
    adjacent:     +2
  Stealth:    +12
    skill (12):   +6
    DEX (+4):     +4
    armor:         0
    light:        +2
    noise:         0
```

When `awareness.state == Aware`: show `Already aware`. When monster has no LOS to player: show `Out of sight`.

Percentage from `notice_probability(perception_mod - stealth_mod)`. The breakdown lines reuse the same `compute_*_mod` helpers but expose intermediate values via a `StealthBreakdown { perc_components, stealth_components }` struct returned from a `_explain` variant of the compute helpers.

## 9. Persistence (degraded, schema v6 → v7)

On **save**: do not serialize `Awareness.records` directly (it's `HashMap<Entity, _>` and entity IDs are unstable). Instead, for each monster, flatten **only the player-keyed record** to a tiny persisted blob:

```rust
struct MonsterAwarenessSave {
    // Collapsed at save time per the degradation rules below.
    state: SavedAwarenessState,
}
enum SavedAwarenessState {
    Hidden,
    Searching { last_known_pos: Point, giveup_at_turn_offset: u32 },
}
```

Degradation rules applied at save time, before serialization:
- `Aware` → `Searching { last_known_pos: player.pos, giveup_at_turn: now + 20 }`
- `Searching` → preserved as-is (its `last_known_pos` is already a `Point`)
- `Suspicious` → `Hidden`
- `Hidden` → `Hidden` (unchanged)

`giveup_at_turn` is serialized as an **offset from now** so it remains correct after the turn counter is reloaded.

On **load**: the save system spawns monsters first (giving each a fresh `Entity`), then spawns the player, then walks each monster's `MonsterAwarenessSave` and reconstructs `monster.Awareness.records.insert(player_entity, AwarenessRecord { state, last_update_turn: 0 })`. Monster-vs-monster awareness is **not persisted** — V1 lets it regenerate within a turn or two via the normal `perception_tick_system`. This is acceptable because monster-vs-monster awareness is mostly used for active cross-faction fights, which resume naturally.

`NoiseMap` does not need to persist (transient, decays to zero in V1).

## 10. Testing strategy

Unit tests (game crate or engine crate as appropriate):
- `notice_probability(delta)` — sentinel values 0, ±5, ±10, ±20
- State machine transitions — given rolls + prior state, assert next state
- `compute_stealth_mod` — fixed-component fixtures (no skill, full skill, with/without DEX, each armor tier, each light bucket)
- `compute_perception_mod` — fixed fixtures (each distance bucket, awake vs. asleep)
- Searching timeout — given `now > giveup_at_turn`, state → Hidden
- Squad propagation — squadmate's Awareness becomes Searching when one alerts
- Backstab gate — Aware/Searching/Suspicious reject; Hidden + Asleep accept
- Save/load round-trip — `Aware` collapses to `Searching{last_known}` on save, restored as Searching on load

Bevy integration tests:
- Sleeping monster, dark tile, adjacent player → notice probability stays low across N turns
- Player gets seen → walks out of LOS → 20 turns elapse → monster's awareness back to Hidden
- Monster A spots player → squadmate Monster B's awareness becomes Searching with correct `last_known_pos`
- Hostile cross-faction monsters in viewshed roll perception against each other and resolve to Aware ↔ Hunting

## 11. Engine vs game split (summary)

| Layer | Owns |
| --- | --- |
| **Engine (`roguelike_engine`)** | `AwarenessState`, `Awareness` component, `AwarenessRecord`, `NoiseMap`, `notice_probability`, `noise_modifier`, `perception_tick_system`, `awareness_tick_system`, `noise_decay_system`. Updates to `MonsterAI::update_mode` to read awareness. `MonsterAsset.perception` field. `AwarenessAlertEvent`. |
| **Game (`bevy_rpg`)** | `Skill::Stealth`, race/class RON edits, `StealthPlugin`, `compute_stealth_mod` / `compute_perception_mod` (composes DEX, Skills, armor, light), `armor_stealth_penalty` field in items.ron, Backstab gate update in combat.rs, all UI changes (nearby pill, hover tooltip, monster info overlay), save/load schema bump + degrade-on-save hook, squad propagation handler. |

## 12. Out of scope for V1 (explicit deferrals)

- **Noise sources.** Decay system ships, but no producer writes to `NoiseMap`. V2 will add a Dijkstra-style fill from action events (movement, attack, staff zap, etc.).
- **Suspicious entry path.** State + transitions + decay are wired, but nothing produces Suspicious in V1. Activated by the V2 noise phase.
- **Player-side concealment items.** No invisibility potions, cloaks of shadow, scroll of stealth, etc. Stealth in V1 is positional + skill-driven only.
- **Monster Stealth skill or DEX.** Monsters use a simplified `stealth_mod = light_modifier + noise_modifier` only. Per-species stealth tags can land later.
- **Stealth-aware AI behaviors.** Searching monsters use existing Hunting pathing toward `last_known_pos`. No "fan out and check rooms" patterns.
- **Hover-the-percentage for breakdown.** Always-on inline only; nested hover deferred.
- **Sound effects / audio cues** for state transitions.

## 13. Implementation order (preview — exact plan generated by writing-plans)

1. **Engine foundations**: `AwarenessState` + `Awareness` + `NoiseMap` types, `notice_probability` + `noise_modifier` + `noise_decay_system`, `MonsterAsset.perception`. Unit tests.
2. **Game-side modifier composition**: `compute_stealth_mod`, `compute_perception_mod`, helpers. Unit tests with fixtures.
3. **Per-turn systems**: `perception_tick_system`, `awareness_tick_system`. Hook into ProcessingPhase ordering. Bevy integration tests.
4. **AI mode integration**: rewrite `MonsterAI::update_mode` to read awareness. Verify existing AI tests still pass.
5. **Squad propagation & attack-reveal handlers**. Tests.
6. **Stealth skill**: enum variant, race aptitudes, class redistribution, use-counter wiring, maintenance contract test extensions.
7. **Backstab gate update**. Test combat outcomes for each awareness state.
8. **UI**: nearby sidebar pill, hover tooltip + monster info "Stealth" section, breakdown helpers.
9. **Save/load**: degrade-on-save hook, schema v6 → v7 bump.
10. **Documentation**: new `docs/design/STEALTH.md` (canonical writeup, referenced from CLAUDE.md), updates to CHARACTER.md / SKILLS.md / SQUAD_AI.md / .claude/rules/.

## 14. Open / future questions

- Tuning of `close_range_bonus` thresholds and `light_modifier` thresholds will need playtest. Constants live in named `const` so they're easy to bump.
- Whether `armor_stealth_penalty` should *also* be tracked separately from the existing Armor encumbrance penalty (`dodge_bonus`) or kept as its own field. V1 keeps them separate so light armor can be stealth-friendly without being dodge-friendly.
- Future: a "Sneak Attack" Backstab generalization that fires when target's awareness is Searching AND player has not been attacked-from-this-side. Out of V1 scope.
