# Stealth System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace binary monster-Asleep behavior with a per-perceiver, per-target awareness model (Hidden / Suspicious / Searching / Aware) driven by opposed d20 rolls, gate Backstab on the Hidden state, and add Stealth as the 10th trainable skill.

**Architecture:** Engine ships pure types + state-tick systems (`AwarenessState`, `Awareness` component, `NoiseMap`, `awareness_tick_system`, `noise_decay_system`, `notice_probability`). Game ships modifier composition (`compute_stealth_mod`, `compute_perception_mod`) and the per-perceiver opposed-roll system (`perception_tick_system`). Awareness drives `MonsterAIMode` transitions via an updated `update_mode` in the engine. UI surfaces awareness through the nearby sidebar and a per-monster "Notice this turn: 87%" tooltip.

**Tech Stack:** Rust, Bevy 0.17 (game + engine), `bracket-lib` for FOV/RNG/Point, `bevy_save 0.17`, `bevy_common_assets` for RON.

**Spec:** [2026-05-16-stealth-system-design.md](../specs/2026-05-16-stealth-system-design.md)

## Drift notes from spec

While verifying the plan, two corrections to the spec were identified:

1. `MonsterAsset` lives in the **game crate** (`src/assets/mod.rs:385`), not the engine. The `perception: i32` field is added there.
2. `perception_tick_system` lives in the **game crate** (it needs game-side `Skills` / `Attributes` / `LightMap` etc. to compute modifiers). The engine only ships `awareness_tick_system` and `noise_decay_system`, which are pure state work. The spec section 11 mis-attributed `perception_tick_system` to engine — this plan supersedes.

The spec is otherwise authoritative. After this plan ships, update the spec's §4.2 and §11 to reflect the corrected split.

## Cross-repo workflow

`bevy_rpg` depends on `roguelike_engine` as a Git dependency. Line 19 of `Cargo.toml` documents a path-dep override for local development. **Phase A enables the override; Phase L disables it and bumps the Git ref.** Engine-side commits must be pushed to the upstream `main` branch before bevy_rpg's Cargo lock can lock to them.

Two repo paths used throughout:
- `bevy_rpg` (this repo): `/Users/nathanrude/Development/bevy_rpg`
- `roguelike_engine` (engine): `/Users/nathanrude/Development/roguelike_engine`

---

## File Structure Overview

### Engine repo (`/Users/nathanrude/Development/roguelike_engine`)

| Action | Path | Responsibility |
| --- | --- | --- |
| Create | `src/stealth/mod.rs` | Module root, plugin, re-exports |
| Create | `src/stealth/awareness.rs` | `AwarenessState`, `AwarenessRecord`, `Awareness` component, `AwarenessAlertEvent`, `awareness_tick_system` |
| Create | `src/stealth/noise.rs` | `NoiseMap` resource, `noise_decay_system`, `noise_modifier` helper |
| Create | `src/stealth/probability.rs` | `notice_probability(delta)` helper |
| Modify | `src/lib.rs` | `pub mod stealth;` |
| Modify | `src/prelude.rs` | Re-export `AwarenessState`, `Awareness`, `AwarenessRecord`, `NoiseMap`, `notice_probability`, `AwarenessAlertEvent` |
| Modify | `src/ai/monster_ai.rs` | `update_mode` reads `Awareness` instead of `is_player_visible` |

### Game repo (`/Users/nathanrude/Development/bevy_rpg`)

| Action | Path | Responsibility |
| --- | --- | --- |
| Create | `src/game/stealth.rs` | `compute_stealth_mod`, `compute_perception_mod`, `light_modifier`, `close_range_bonus`, `StealthBreakdown`, `perception_tick_system`, squad propagation handler, attack-reveal handler, use-counter system, `StealthPlugin` |
| Create | `docs/design/STEALTH.md` | Canonical writeup |
| Modify | `Cargo.toml` | Swap to path-dep (A1), restore Git dep (L1) |
| Modify | `src/assets/mod.rs` | Add `perception: i32` to `MonsterAsset`, `armor_stealth_penalty: i32` to `ItemAsset` |
| Modify | `assets/monsters.ron` | Author `perception` per species |
| Modify | `assets/items.ron` | Author `armor_stealth_penalty` per armor |
| Modify | `src/game/skills.rs` | Add `Skill::Stealth`, propagate through `Skills`/`SkillXp`/`SkillTraining`/`SkillUseCounters` |
| Modify | `src/character/asset.rs` | Add `stealth` to `SkillDistribution` + `SkillAptitudes` |
| Modify | `assets/races.ron` | Add `stealth` aptitude per race |
| Modify | `assets/classes.ron` | Redistribute Rogue + Ranger starting skills |
| Modify | `src/game/combat.rs` | Backstab gate reads `Awareness` instead of `MonsterAIMode == Asleep` |
| Modify | `src/game/spawner.rs` | Insert `Awareness::default()` on monster spawn |
| Modify | `src/player/mod.rs` | Insert `Awareness::default()` on player spawn |
| Modify | `src/ui/nearby.rs` | Status pill per visible monster |
| Modify | `src/ui/hover_info.rs` | "Notice this turn" + breakdown |
| Modify | `src/ui/monster_info.rs` | Same block in the overlay |
| Modify | `src/save/mod.rs` | Schema v6 → v7; `MonsterAwarenessSave`; degrade-on-save + restore-on-load |
| Modify | `src/main.rs` | Register engine `StealthPlugin` + game `StealthPlugin` |
| Modify | `docs/design/CHARACTER.md` | Updated class skill tables |
| Modify | `docs/design/SKILLS.md` | New Stealth row, aptitudes, class allocations |
| Modify | `docs/design/SQUAD_AI.md` | Searching-propagation note |
| Modify | `docs/design/ENEMIES.md` | `perception` field |
| Modify | `CLAUDE.md` | Project structure + Key Architectural Patterns sections |
| Modify | `.claude/skills/content-studio/references/ron-schemas.md` | New schema fields |

---

## Phase A — Setup

### Task A1: Enable engine path-dep override

**Files:**
- Modify: `Cargo.toml:17-19`

- [ ] **Step 1: Swap the dependency lines**

Open [Cargo.toml](../../Cargo.toml). Change:

```toml
roguelike_engine = { git = "https://github.com/rudehn/roguelike_engine", branch = "main" }
# For local development, swap to the path dependency:
# roguelike_engine = { path = "../roguelike_engine" }
```

To:

```toml
# roguelike_engine = { git = "https://github.com/rudehn/roguelike_engine", branch = "main" }
# Local development override — restored to Git in Task L1 before shipping.
roguelike_engine = { path = "../roguelike_engine" }
```

- [ ] **Step 2: Verify the swap compiles**

```bash
cd /Users/nathanrude/Development/bevy_rpg
cargo check
```

Expected: success (the engine API hasn't changed yet). If a `roguelike_engine` lock entry conflicts, run `cargo update -p roguelike_engine`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "build: switch to engine path-dep for stealth phase

Local development override per the stealth implementation plan. Will be
restored to the Git dependency in the final phase once engine commits
are pushed.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase B — Engine: types & helpers

> All Phase B tasks run inside `/Users/nathanrude/Development/roguelike_engine`. Use `git -C /Users/nathanrude/Development/roguelike_engine ...` from the bevy_rpg shell if you stay in the bevy_rpg directory.

### Task B1: Create stealth module skeleton + `AwarenessState` enum

**Files:**
- Create: `/Users/nathanrude/Development/roguelike_engine/src/stealth/mod.rs`
- Create: `/Users/nathanrude/Development/roguelike_engine/src/stealth/awareness.rs`
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `/Users/nathanrude/Development/roguelike_engine/src/stealth/awareness.rs`:

```rust
//! Per-perceiver, per-target awareness model. See bevy_rpg's
//! `docs/superpowers/specs/2026-05-16-stealth-system-design.md`.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwarenessState {
    Hidden,
    Suspicious { suspect_pos: Point, decay_at_turn: u32 },
    Searching  { last_known_pos: Point, giveup_at_turn: u32 },
    Aware,
}

impl AwarenessState {
    /// Strength ordering for `Awareness::highest_against`. Hidden = 0,
    /// Aware = 3. Used to compare records when picking the dominant
    /// awareness against a set of hostile targets.
    pub fn rank(&self) -> u8 {
        match self {
            AwarenessState::Hidden => 0,
            AwarenessState::Suspicious { .. } => 1,
            AwarenessState::Searching { .. } => 2,
            AwarenessState::Aware => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AwarenessRecord {
    pub state: AwarenessState,
    /// Game-turn the record was last touched. Used by the tick system
    /// to GC stale Hidden records and to drive timer expirations.
    pub last_update_turn: u32,
    /// Last known position of the target at the time it was Aware.
    /// Set when transitioning Aware → Searching. None for fresh records.
    pub last_seen_pos: Option<Point>,
}

#[derive(Component, Default, Debug)]
pub struct Awareness {
    pub records: HashMap<Entity, AwarenessRecord>,
}

impl Awareness {
    pub fn get(&self, target: Entity) -> Option<&AwarenessRecord> {
        self.records.get(&target)
    }

    pub fn set(&mut self, target: Entity, state: AwarenessState, now: u32) {
        let entry = self.records.entry(target).or_insert(AwarenessRecord {
            state: AwarenessState::Hidden,
            last_update_turn: now,
            last_seen_pos: None,
        });
        entry.state = state;
        entry.last_update_turn = now;
    }

    /// Returns the highest-ranked state across all records; defaults to
    /// Hidden if the map is empty. Caller decides which records count
    /// as "hostile" (faction filter applied externally).
    pub fn highest(&self) -> AwarenessState {
        self.records
            .values()
            .map(|r| r.state)
            .max_by_key(|s| s.rank())
            .unwrap_or(AwarenessState::Hidden)
    }
}

#[derive(Message, Debug, Clone)]
pub struct AwarenessAlertEvent {
    pub seeker: Entity,
    pub target: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_ordering_is_total() {
        assert!(AwarenessState::Hidden.rank() < AwarenessState::Suspicious { suspect_pos: Point::zero(), decay_at_turn: 0 }.rank());
        assert!(AwarenessState::Suspicious { suspect_pos: Point::zero(), decay_at_turn: 0 }.rank()
            < AwarenessState::Searching { last_known_pos: Point::zero(), giveup_at_turn: 0 }.rank());
        assert!(AwarenessState::Searching { last_known_pos: Point::zero(), giveup_at_turn: 0 }.rank()
            < AwarenessState::Aware.rank());
    }

    #[test]
    fn empty_awareness_returns_hidden() {
        let a = Awareness::default();
        assert_eq!(a.highest(), AwarenessState::Hidden);
    }

    #[test]
    fn highest_returns_strongest_state() {
        let mut a = Awareness::default();
        let e1 = Entity::from_raw(1);
        let e2 = Entity::from_raw(2);
        a.set(e1, AwarenessState::Hidden, 0);
        a.set(e2, AwarenessState::Searching { last_known_pos: Point::new(3, 4), giveup_at_turn: 10 }, 0);
        assert!(matches!(a.highest(), AwarenessState::Searching { .. }));
    }
}
```

Create `/Users/nathanrude/Development/roguelike_engine/src/stealth/mod.rs`:

```rust
//! Stealth subsystem — awareness state, noise map, probability helper.
//!
//! Engine-side ships pure types + state-tick systems. The game crate
//! ships modifier composition and the per-turn opposed-roll system
//! (`perception_tick_system`). See bevy_rpg's stealth-system-design.md.

pub mod awareness;
pub mod noise;
pub mod probability;

pub use awareness::{Awareness, AwarenessAlertEvent, AwarenessRecord, AwarenessState};
pub use noise::{noise_decay_system, noise_modifier, NoiseMap};
pub use probability::notice_probability;
```

- [ ] **Step 2: Register the module in `lib.rs`**

Add `pub mod stealth;` to `/Users/nathanrude/Development/roguelike_engine/src/lib.rs` next to the other module declarations.

- [ ] **Step 3: Run the test (expect compile failure — `noise` not yet present)**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib stealth::awareness:: -- --nocapture
```

Expected: compilation fails on the missing `noise.rs` and `probability.rs`. Continue to B2 to add them. **Or** create empty stubs now:

```rust
// /Users/nathanrude/Development/roguelike_engine/src/stealth/noise.rs
pub struct NoiseMap;
pub fn noise_modifier(_pos: bracket_lib::prelude::Point, _map: &NoiseMap) -> i32 { 0 }
pub fn noise_decay_system() {}
```

```rust
// /Users/nathanrude/Development/roguelike_engine/src/stealth/probability.rs
pub fn notice_probability(_delta: i32) -> f32 { 0.0 }
```

Re-run the test. Expected: PASS for the three unit tests above.

- [ ] **Step 4: Commit**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git add src/stealth/ src/lib.rs
git commit -m "stealth: introduce AwarenessState + Awareness component

Per-perceiver target-keyed awareness map with four states
(Hidden/Suspicious/Searching/Aware). Tick/roll systems land in
follow-up commits.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task B2: `notice_probability` helper

**Files:**
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/stealth/probability.rs`

- [ ] **Step 1: Write the failing tests**

Replace the stub `probability.rs` with:

```rust
//! Closed-form probability for the opposed d20 stealth/perception roll.

/// P(d20 + perception_mod > d20 + stealth_mod) where
/// `delta = perception_mod - stealth_mod`.
///
/// Enumerates the full 20×20 outcome space; cheap (400 ops) and exact.
pub fn notice_probability(delta: i32) -> f32 {
    let mut wins = 0u32;
    for x in 1..=20i32 {
        for y in 1..=20i32 {
            if x + delta > y {
                wins += 1;
            }
        }
    }
    wins as f32 / 400.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn delta_zero_is_just_under_half() {
        // P(d20 > d20) = (20·19/2) / 400 = 190/400 = 0.475
        assert!(approx(notice_probability(0), 0.475));
    }

    #[test]
    fn large_positive_delta_certain() {
        assert!(approx(notice_probability(20), 1.0));
    }

    #[test]
    fn large_negative_delta_zero() {
        assert!(approx(notice_probability(-20), 0.0));
    }

    #[test]
    fn delta_plus_ten_is_about_ninety_five() {
        // P(d20 + 10 > d20) — only x=1..10 cannot beat all y. Counting
        // gives 381/400 = 0.9525.
        assert!(approx(notice_probability(10), 0.9525));
    }

    #[test]
    fn delta_minus_ten_is_complement() {
        // Symmetric: should be 19/400 = 0.0475.
        assert!(approx(notice_probability(-10), 0.0475));
    }
}
```

- [ ] **Step 2: Run tests (expect pass — function is pure and small)**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib stealth::probability
```

Expected: all 5 tests pass.

- [ ] **Step 3: Commit**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git add src/stealth/probability.rs
git commit -m "stealth: add notice_probability helper

Closed-form 20x20 enumeration of P(d20 + perc > d20 + stealth) given
a modifier delta. 5 sentinel-value tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task B3: `NoiseMap` resource + `noise_decay_system`

**Files:**
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/stealth/noise.rs`

- [ ] **Step 1: Write the failing tests + implementation together**

Replace the stub `noise.rs`:

```rust
//! Per-tile noise map. V1 ships the decay system; no source writes to
//! it. The game's `compute_stealth_mod` calls `noise_modifier(pos, map)`
//! which currently returns 0 because the map stays at zero. The V2
//! noise phase will add a producer that writes positive values from
//! action events (movement, attacks, staff zaps).

use bevy::prelude::*;
use bracket_lib::prelude::Point;

#[derive(Resource, Debug, Clone)]
pub struct NoiseMap {
    pub tiles: Vec<i32>,
    pub width: usize,
    pub height: usize,
}

impl NoiseMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            tiles: vec![0; width * height],
            width,
            height,
        }
    }

    pub fn at(&self, pos: Point) -> i32 {
        if pos.x < 0 || pos.y < 0 {
            return 0;
        }
        let (x, y) = (pos.x as usize, pos.y as usize);
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.tiles[y * self.width + x]
    }
}

/// V1 stub: returns a negative penalty proportional to the noise level
/// at the target's tile (loud tile → less stealthy). With no producer
/// in V1, this always returns 0. V2 noise phase populates `NoiseMap`
/// and this function automatically becomes meaningful.
pub fn noise_modifier(pos: Point, map: &NoiseMap) -> i32 {
    -map.at(pos)
}

/// Runs once per game turn. Decrements every cell by 1, clamped to 0.
pub fn noise_decay_system(mut map: ResMut<NoiseMap>) {
    for cell in &mut map.tiles {
        *cell = (*cell - 1).max(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_bounds_check_returns_zero() {
        let m = NoiseMap::new(4, 4);
        assert_eq!(m.at(Point::new(-1, 0)), 0);
        assert_eq!(m.at(Point::new(0, -1)), 0);
        assert_eq!(m.at(Point::new(4, 0)), 0);
        assert_eq!(m.at(Point::new(0, 4)), 0);
    }

    #[test]
    fn noise_modifier_returns_negative_of_tile_value() {
        let mut m = NoiseMap::new(4, 4);
        m.tiles[1 * 4 + 2] = 5; // (2, 1) → 5
        assert_eq!(noise_modifier(Point::new(2, 1), &m), -5);
    }

    #[test]
    fn decay_in_isolation() {
        // Avoid needing a full Bevy App: assert the inner reasoning
        // works on a borrowed mutable map.
        let mut m = NoiseMap::new(2, 2);
        m.tiles = vec![3, 1, 0, 5];
        for cell in &mut m.tiles {
            *cell = (*cell - 1).max(0);
        }
        assert_eq!(m.tiles, vec![2, 0, 0, 4]);
    }

    #[test]
    fn decay_floors_at_zero() {
        let mut m = NoiseMap::new(1, 1);
        m.tiles = vec![0];
        for cell in &mut m.tiles {
            *cell = (*cell - 1).max(0);
        }
        assert_eq!(m.tiles, vec![0]);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib stealth::noise
```

Expected: all 4 tests pass.

- [ ] **Step 3: Commit**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git add src/stealth/noise.rs
git commit -m "stealth: add NoiseMap resource with decay-by-1 tick

Per-tile i32 noise levels, decremented by 1 each game turn floored at
0. V1 has no producer; noise_modifier returns 0 in practice but the
data flow is live.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task B4: `awareness_tick_system`

**Files:**
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/stealth/awareness.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `awareness.rs`:

```rust
    #[test]
    fn searching_timer_expires_to_hidden() {
        let mut a = Awareness::default();
        let target = Entity::from_raw(99);
        a.set(target, AwarenessState::Searching {
            last_known_pos: Point::new(5, 5),
            giveup_at_turn: 10,
        }, 0);
        // Tick: now = 11 → expired.
        tick_awareness(&mut a, /*now*/ 11);
        assert_eq!(a.get(target).unwrap().state, AwarenessState::Hidden);
    }

    #[test]
    fn suspicious_timer_expires_to_hidden() {
        let mut a = Awareness::default();
        let target = Entity::from_raw(99);
        a.set(target, AwarenessState::Suspicious {
            suspect_pos: Point::new(2, 2),
            decay_at_turn: 5,
        }, 0);
        tick_awareness(&mut a, 6);
        assert_eq!(a.get(target).unwrap().state, AwarenessState::Hidden);
    }

    #[test]
    fn searching_timer_alive_holds_state() {
        let mut a = Awareness::default();
        let target = Entity::from_raw(99);
        a.set(target, AwarenessState::Searching {
            last_known_pos: Point::new(5, 5),
            giveup_at_turn: 10,
        }, 0);
        tick_awareness(&mut a, 5); // not yet expired
        assert!(matches!(
            a.get(target).unwrap().state,
            AwarenessState::Searching { .. }
        ));
    }

    #[test]
    fn aware_state_is_untouched_by_tick() {
        let mut a = Awareness::default();
        let target = Entity::from_raw(99);
        a.set(target, AwarenessState::Aware, 0);
        tick_awareness(&mut a, 100);
        assert_eq!(a.get(target).unwrap().state, AwarenessState::Aware);
    }
```

- [ ] **Step 2: Run the test to verify failure**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib stealth::awareness::tests
```

Expected: compilation fails on missing `tick_awareness` symbol.

- [ ] **Step 3: Write the implementation**

Add to the body of `awareness.rs` (above the `#[cfg(test)]` block):

```rust
/// Decay timers and demote expired Searching/Suspicious records to
/// Hidden. Called by `awareness_tick_system` per perceiver-Awareness;
/// extracted for unit testability without a Bevy App.
pub fn tick_awareness(awareness: &mut Awareness, now: u32) {
    for record in awareness.records.values_mut() {
        let expired = match record.state {
            AwarenessState::Searching { giveup_at_turn, .. } => now > giveup_at_turn,
            AwarenessState::Suspicious { decay_at_turn, .. } => now > decay_at_turn,
            _ => false,
        };
        if expired {
            record.state = AwarenessState::Hidden;
            record.last_update_turn = now;
        }
    }
    // GC: drop Hidden records older than 200 turns to keep the map small.
    awareness.records.retain(|_, r| {
        !matches!(r.state, AwarenessState::Hidden)
            || now.saturating_sub(r.last_update_turn) <= 200
    });
}

/// Bevy system: runs once per game turn, ticks every Awareness component.
/// Reads `now` from a `CurrentTurn` resource (see `roguelike_engine::turn`).
pub fn awareness_tick_system(
    current_turn: Res<crate::turn::CurrentTurn>,
    mut perceivers: Query<&mut Awareness>,
) {
    let now = current_turn.0;
    for mut a in &mut perceivers {
        tick_awareness(a.as_mut(), now);
    }
}
```

> **Note for engineer:** Verify that `crate::turn::CurrentTurn` is the right resource. If not, search for the canonical current-turn counter in the engine (`grep -rn "pub struct CurrentTurn\|game_turn\|tick_count" /Users/nathanrude/Development/roguelike_engine/src`). Adjust the import accordingly. If no engine-side turn resource exists, accept it as a `Res<U32Counter>` placeholder and have the game register a wrapper.

- [ ] **Step 4: Run tests, fix import if needed**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib stealth::awareness
```

Expected: 7 tests pass total (3 from B1 + 4 from this task).

- [ ] **Step 5: Commit**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git add src/stealth/awareness.rs
git commit -m "stealth: add awareness_tick_system + timer expiry

Per-turn Searching/Suspicious decay to Hidden when timer elapses. Pure
tick_awareness helper extracted for unit testing. GCs stale Hidden
records older than 200 turns.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task B5: Prelude re-exports + engine `StealthPlugin`

**Files:**
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/prelude.rs`
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/stealth/mod.rs`

- [ ] **Step 1: Add re-exports**

Edit `/Users/nathanrude/Development/roguelike_engine/src/prelude.rs`. Find the existing `pub use crate::ai::...` block and add below it:

```rust
pub use crate::stealth::{
    awareness_tick_system, noise_decay_system, noise_modifier, notice_probability, Awareness,
    AwarenessAlertEvent, AwarenessRecord, AwarenessState, NoiseMap,
};
```

- [ ] **Step 2: Add `StealthPlugin` to engine stealth module**

Append to `/Users/nathanrude/Development/roguelike_engine/src/stealth/mod.rs`:

```rust
use bevy::prelude::*;

/// Engine-side plugin: registers the pure state-tick systems and the
/// AwarenessAlertEvent message. Per-turn opposed-roll lives in the game
/// crate's `StealthPlugin`. Ordering against the game's
/// `ProcessingPhase` is the game crate's responsibility.
pub struct StealthPlugin;

impl Plugin for StealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AwarenessAlertEvent>();
        // Systems are exported as free functions; the game crate wires
        // ordering against ProcessingPhase. We do not add them here to
        // avoid coupling the engine to game-side SystemSets.
    }
}
```

- [ ] **Step 3: Verify build**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo check
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git add src/stealth/mod.rs src/prelude.rs
git commit -m "stealth: prelude re-exports + StealthPlugin

Engine-side plugin registers AwarenessAlertEvent only. The
awareness_tick_system + noise_decay_system are exported as free
functions for the game crate to schedule against its ProcessingPhase.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task B6: Update `MonsterAI::update_mode` to read `Awareness`

**Files:**
- Modify: `/Users/nathanrude/Development/roguelike_engine/src/ai/monster_ai.rs`

- [ ] **Step 1: Find the existing mode-update logic**

```bash
grep -n 'fn update_mode\|alert_to_position\|is_player_visible\|fn execute' /Users/nathanrude/Development/roguelike_engine/src/ai/monster_ai.rs
```

Note the line range of the existing mode-update path.

- [ ] **Step 2: Write a unit test for the new behavior**

Add to the `mod tests` block at the bottom of `monster_ai.rs`:

```rust
    #[test]
    fn awareness_aware_drives_hunting() {
        use crate::stealth::{Awareness, AwarenessState};
        let mut ai = MonsterAI::new();
        let mut aware = Awareness::default();
        aware.set(Entity::from_raw(42), AwarenessState::Aware, 0);
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
    }

    #[test]
    fn awareness_searching_drives_hunting() {
        use crate::stealth::{Awareness, AwarenessState};
        use bracket_lib::prelude::Point;
        let mut ai = MonsterAI::new();
        let mut aware = Awareness::default();
        aware.set(Entity::from_raw(42), AwarenessState::Searching {
            last_known_pos: Point::new(3, 3),
            giveup_at_turn: 100,
        }, 0);
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Hunting);
    }

    #[test]
    fn awareness_hidden_keeps_default_mode() {
        use crate::stealth::Awareness;
        let mut ai = MonsterAI::new();
        ai.mode = MonsterAIMode::Asleep;
        let aware = Awareness::default(); // empty == Hidden
        ai.update_mode_from_awareness(&aware);
        assert_eq!(ai.mode, MonsterAIMode::Asleep);
    }
```

- [ ] **Step 3: Run tests (expect compile failure)**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib ai::monster_ai
```

Expected: compile error — `update_mode_from_awareness` doesn't exist.

- [ ] **Step 4: Implement the new method**

Add to `impl MonsterAI` in `/Users/nathanrude/Development/roguelike_engine/src/ai/monster_ai.rs`:

```rust
    /// Awareness-driven mode update. Replaces the legacy LOS-only check.
    /// Caller provides this monster's `Awareness` component; the highest
    /// state present determines the mode.
    pub fn update_mode_from_awareness(&mut self, awareness: &crate::stealth::Awareness) {
        use crate::stealth::AwarenessState;
        match awareness.highest() {
            AwarenessState::Aware => {
                self.mode = MonsterAIMode::Hunting;
            }
            AwarenessState::Searching { .. } => {
                self.mode = MonsterAIMode::Hunting;
            }
            AwarenessState::Suspicious { .. } => {
                // Investigation rides on Idle; the game's AI dispatch
                // uses the suspect_pos as a temporary target tile.
                if self.mode != MonsterAIMode::Hunting {
                    self.mode = MonsterAIMode::Idle;
                }
            }
            AwarenessState::Hidden => {
                // Preserve current default — Asleep stays Asleep until
                // a successful perception roll wakes them up.
            }
        }
    }
```

- [ ] **Step 5: Run tests, expect pass**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib ai::monster_ai
```

Expected: all tests pass (3 new + the existing ones).

- [ ] **Step 6: Commit**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git add src/ai/monster_ai.rs
git commit -m "ai: drive MonsterAIMode from Awareness component

Adds update_mode_from_awareness which replaces the legacy is_player_visible
check. Hidden state preserves the prior mode (Asleep keeps sleeping)
so the existing wake-on-bump pathway still works.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task B7: Verify engine builds clean

- [ ] **Step 1: Build + test**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo check
cargo test --lib
cargo clippy --lib -- -D warnings
```

Expected: all green. Fix any clippy issues with named fixes (no `#[allow(...)]` blanket).

- [ ] **Step 2: Verify game compiles against the updated engine**

```bash
cd /Users/nathanrude/Development/bevy_rpg
cargo check
```

Expected: success. The new APIs aren't called yet in the game crate, so nothing breaks.

---

## Phase C — Game: `MonsterAsset.perception` + items.ron stealth penalty

### Task C1: Add `perception: i32` to `MonsterAsset`

**Files:**
- Modify: `src/assets/mod.rs:384-410`
- Modify: `assets/monsters.ron`

- [ ] **Step 1: Find the struct**

```bash
grep -n 'pub struct MonsterAsset' src/assets/mod.rs
```

Open `src/assets/mod.rs` at that line. Find the `tier` field declaration (search for `pub tier: u32,`).

- [ ] **Step 2: Add the field**

Below the `tier` field, add:

```rust
    /// Base perception score (Phase 4 stealth system). Modifier to the
    /// d20 perception roll vs. a target's stealth. Defaults to 0; range
    /// roughly -3..=+5 across the shipping monster roster.
    #[serde(default)]
    pub perception: i32,
```

- [ ] **Step 3: Author values in monsters.ron**

Open `assets/monsters.ron`. For each monster entry, add a `perception:` line with these recommended starts:

| Species | perception |
| --- | --- |
| goblin_grunt | 0 |
| goblin_archer | 1 |
| goblin_support | 1 |
| goblin_brute | -1 |
| goblin_commander | 3 |
| kobold_hoarder | 1 |
| hound / wolf | 5 |
| rat / giant_rat | 2 |
| (anything not listed above) | 0 |

> **Discovery tip:** `grep '^\s*"[a-z_]*":' assets/monsters.ron` enumerates every monster id. Apply the table above and default to 0 for any not listed.

Example diff for goblin_grunt:

```ron
"goblin_grunt": (
    name: "Goblin Grunt",
    ...,
    tier: 1,
    perception: 0,    // NEW LINE
    ...
),
```

- [ ] **Step 4: Verify the RON parses**

```bash
cd /Users/nathanrude/Development/bevy_rpg
cargo check
```

Expected: success. If a RON parse error fires at game start, it'll surface in the next test run; for now, type check is sufficient.

- [ ] **Step 5: Run any existing monster-asset tests**

```bash
cargo test --lib monsters
```

Expected: existing tests pass. Add no new tests for this task — the field is plumbing.

- [ ] **Step 6: Commit**

```bash
git add src/assets/mod.rs assets/monsters.ron
git commit -m "assets: add perception field to MonsterAsset

Default 0; per-species values authored in monsters.ron. Modifier into
the d20 perception roll for the stealth system. Hounds get +5,
commanders +3, goblin brutes -1, rats +2, others 0.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task C2: Add `armor_stealth_penalty: i32` to weapon/armor asset

**Files:**
- Modify: `src/assets/mod.rs`
- Modify: `assets/items.ron`

- [ ] **Step 1: Find the item asset struct**

```bash
grep -n 'pub struct .*ItemAsset\|pub struct WeaponAsset\|pub struct ArmorAsset' src/assets/mod.rs
```

If a single `ItemAsset` covers armor, add the field there. If armor has its own struct (e.g. `ArmorAsset`), add it there. Use `grep -n 'armor' src/assets/mod.rs | head -20` to orient.

- [ ] **Step 2: Add the field**

Add to the appropriate struct (above its `Default`-deriving fields if any):

```rust
    /// Stealth penalty for the wearer (Phase 4 stealth system). Subtracted
    /// from the d20 stealth roll. 0 = silent (cloth, robe), 5 = plate.
    /// Defaults to 0 so non-armor items don't carry a phantom penalty.
    #[serde(default)]
    pub armor_stealth_penalty: i32,
```

- [ ] **Step 3: Author values in items.ron**

Per the spec §4.4. Add `armor_stealth_penalty:` to each armor entry:

| Item id | armor_stealth_penalty |
| --- | --- |
| `cloth_wraps` / `robe` | 0 |
| `padded_armor` | 1 |
| `leather_armor` | 1 |
| `studded_leather` | 2 |
| `chain_mail` | 3 |
| `plate_armor` | 5 |

Enumerate armor entries with `grep '"name":' assets/items.ron`; map by inspection. Default any non-armor entry to 0 (the serde default already covers this; only author it on armor for clarity).

- [ ] **Step 4: Verify**

```bash
cargo check
cargo test --lib items
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add src/assets/mod.rs assets/items.ron
git commit -m "assets: add armor_stealth_penalty to items

Per-armor i32 penalty subtracted from stealth_mod. Cloth/Robe 0,
Padded/Leather 1, Studded 2, Chain 3, Plate 5.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase D — Game: Skill::Stealth + race/class allocations

### Task D1: Add `Skill::Stealth` enum variant + propagate

**Files:**
- Modify: `src/game/skills.rs:17-89` (Skill enum + Skills + SkillXp + SkillTraining + SkillUseCounters)

- [ ] **Step 1: Read current Skill enum + propagation surface**

```bash
sed -n '15,80p' src/game/skills.rs
```

Identify every match arm and field that lists the existing 9 skills.

- [ ] **Step 2: Write the failing test**

Append to `src/game/skills.rs` (or its `#[cfg(test)] mod tests` block — confirm it exists; if not, create one at file end):

```rust
#[cfg(test)]
mod stealth_skill_tests {
    use super::*;

    #[test]
    fn stealth_in_skill_all() {
        let names: Vec<_> = Skill::all().map(|s| s.name()).collect();
        assert!(names.contains(&"Stealth"));
    }

    #[test]
    fn skill_count_is_ten() {
        assert_eq!(Skill::all().count(), 10);
    }

    #[test]
    fn skills_struct_round_trips_stealth() {
        let mut s = Skills::new();
        s.set(Skill::Stealth, 7.0);
        assert_eq!(s.get(Skill::Stealth), 7.0);
    }

    #[test]
    fn skill_xp_round_trips_stealth() {
        let mut x = SkillXp::new();
        x.add(Skill::Stealth, 100);
        assert_eq!(x.get(Skill::Stealth), 100);
    }

    #[test]
    fn use_counters_round_trip_stealth() {
        let mut c = SkillUseCounters::default();
        c.bump(Skill::Stealth);
        c.bump(Skill::Stealth);
        // Whatever the internal field is, two bumps should produce >=2
        // by the existing semantics of `bump`. If the type doesn't
        // expose a getter, assert via Debug/serialization.
    }
}
```

- [ ] **Step 3: Run (expect failure)**

```bash
cargo test --lib skills::stealth_skill_tests
```

Expected: compile failure on `Skill::Stealth`.

- [ ] **Step 4: Add the variant + propagate**

Edit `src/game/skills.rs`:

1. **`Skill` enum** — add `Stealth` as the last variant:
```rust
pub enum Skill {
    Fighting,
    Axes,
    ShortBlades,
    LongBlades,
    RangedWeapons,
    Armor,
    Dodging,
    Shields,
    Evocations,
    Stealth,   // NEW
}
```

2. **`Skill::all()`** — find the iterator/array constructor, add `Stealth`:
```rust
impl Skill {
    pub fn all() -> impl Iterator<Item = Skill> {
        [
            Skill::Fighting, Skill::Axes, Skill::ShortBlades, Skill::LongBlades,
            Skill::RangedWeapons, Skill::Armor, Skill::Dodging, Skill::Shields,
            Skill::Evocations, Skill::Stealth,  // NEW
        ]
        .into_iter()
    }
    ...
}
```

3. **`Skill::name()`** — add an arm:
```rust
Skill::Stealth => "Stealth",
```

4. **`Skills` struct** — add a field:
```rust
pub struct Skills {
    pub fighting: f32,
    ...
    pub evocations: f32,
    pub stealth: f32,   // NEW
}
```

Then update `Skills::new()`, `Skills::get(skill)` match arms, `Skills::set(skill, level)` match arms.

5. **`SkillXp`** — same pattern: add `stealth: u64`, update `new()`/`get()`/`add()` matches.

6. **`SkillTraining`** — add `stealth: SkillState`, update `new()`/`get()`/`cycle()`/`target()`/`set_target()`/`clear_target()` matches.

7. **`SkillUseCounters`** — add `stealth: u32` (or whatever the existing type is), update `bump()` match.

8. Re-check `weapon_skill_bonus`, `fighting_melee_bonus`, `armor_skill_bonus`, `dodging_skill_bonus`, `shields_skill_bonus` — none of these need a Stealth variant (they're per-skill bonuses for specific combat contexts). Stealth has no on-action bonus in V1; the modifier is consumed by `compute_stealth_mod` (game/stealth.rs, Phase E).

- [ ] **Step 5: Run tests, expect pass**

```bash
cargo test --lib skills
```

Expected: 5 new tests + all existing skill tests pass. If a `Skill::all()` consumer panics on a missing branch, propagate the fix.

- [ ] **Step 6: Commit**

```bash
git add src/game/skills.rs
git commit -m "skills: add Stealth as 10th trainable skill

Skill::Stealth variant propagated through Skills/SkillXp/SkillTraining/
SkillUseCounters. No combat-bonus integration in V1 — the value feeds
compute_stealth_mod (Phase E) via floor(skill / 2).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task D2: Update `SkillDistribution` + `SkillAptitudes` helpers

**Files:**
- Modify: `src/character/asset.rs`

- [ ] **Step 1: Locate the helper structs**

```bash
grep -n 'pub struct SkillDistribution\|pub struct SkillAptitudes\|every_class_starting_skills_sums_to_ten\|every_race_aptitude_value_is_in_range' src/character/asset.rs
```

- [ ] **Step 2: Read the structs and tests**

Open `src/character/asset.rs` around those lines. Confirm field naming convention (snake_case matching the RON fields).

- [ ] **Step 3: Add the `stealth` field to both structs**

```rust
pub struct SkillDistribution {
    #[serde(default)]
    pub fighting: u32,
    ...
    #[serde(default)]
    pub evocations: u32,
    #[serde(default)]
    pub stealth: u32,    // NEW
}
```

Update its `total()`/`sum()` method to include the new field, and the iterator that yields `(Skill, level)` pairs to emit `(Skill::Stealth, self.stealth as f32)`.

```rust
pub struct SkillAptitudes {
    #[serde(default)]
    pub fighting: i32,
    ...
    #[serde(default)]
    pub evocations: i32,
    #[serde(default)]
    pub stealth: i32,    // NEW
}
```

Update the iterator/lookup helper to include `(Skill::Stealth, self.stealth)`.

- [ ] **Step 4: Run the maintenance tests**

```bash
cargo test --lib character::asset
```

Expected: PASS (the existing tests will discover the new field via the RON parser once the RON ships values in Tasks D3/D4 — for now they should pass with `#[serde(default)]` providing 0).

- [ ] **Step 5: Commit**

```bash
git add src/character/asset.rs
git commit -m "character: add stealth slot to SkillDistribution + SkillAptitudes

Field defaults to 0 so existing RON parses still load. Phase D3/D4
populate per-race aptitude and per-class starting allocation.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task D3: Update `assets/races.ron` with stealth aptitudes

**Files:**
- Modify: `assets/races.ron`

- [ ] **Step 1: Add `stealth:` line to each race's `aptitudes:` block**

Open `assets/races.ron`. For each race entry, add a `stealth:` line at the end of `aptitudes:` (after `evocations`):

| Race | stealth |
| --- | --- |
| human | 0 |
| dwarf | -2 |
| elf | 2 |

Example (human):
```ron
aptitudes: (
    fighting: 1,
    axes: 0,
    short_blades: 0,
    long_blades: 0,
    ranged_weapons: 0,
    armor: 1,
    dodging: 0,
    shields: 1,
    evocations: 0,
    stealth: 0,    // NEW
),
```

- [ ] **Step 2: Verify the maintenance test passes**

```bash
cargo test --lib character::asset::every_race_aptitude_value_is_in_range
```

Expected: PASS. The test asserts each aptitude is in `-5..=5`; our values (0, -2, +2) are all in range.

- [ ] **Step 3: Commit**

```bash
git add assets/races.ron
git commit -m "races: add stealth aptitude per race

Human 0 (Adaptive), Dwarf -2 (Stoneblood heavy gait), Elf +2 (Keen
Senses). Each race's aptitudes block now covers all 10 skills.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task D4: Redistribute Rogue + Ranger starting skills

**Files:**
- Modify: `assets/classes.ron`

- [ ] **Step 1: Edit Rogue's `starting_skills`**

Current:
```ron
starting_skills: (
    fighting: 1,
    short_blades: 4,
    ranged_weapons: 1,
    dodging: 3,
    evocations: 1,
),
```

Replace with:
```ron
starting_skills: (
    fighting: 1,
    short_blades: 3,
    ranged_weapons: 1,
    dodging: 2,
    evocations: 1,
    stealth: 2,
),
```

(Drops 1 short_blades + 1 dodging; gains 2 stealth. Sum: 1+3+1+2+1+2 = 10.)

- [ ] **Step 2: Edit Ranger's `starting_skills`**

Current:
```ron
starting_skills: (
    fighting: 2,
    long_blades: 1,
    ranged_weapons: 4,
    armor: 1,
    dodging: 2,
),
```

Replace with:
```ron
starting_skills: (
    fighting: 2,
    long_blades: 1,
    ranged_weapons: 4,
    armor: 1,
    dodging: 1,
    stealth: 1,
),
```

(Drops 1 dodging; gains 1 stealth. Sum: 2+1+4+1+1+1 = 10.)

- [ ] **Step 3: Run the maintenance test**

```bash
cargo test --lib character::asset::every_class_starting_skills_sums_to_ten
```

Expected: PASS for all four classes.

- [ ] **Step 4: Commit**

```bash
git add assets/classes.ron
git commit -m "classes: redistribute Rogue and Ranger for Stealth skill

Rogue gains 2 ranks of Stealth at the cost of 1 ShortBlade + 1 Dodging.
Ranger gains 1 rank at the cost of 1 Dodging. Warrior and Mage stay
unchanged at Stealth 0.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase E — Game: stealth module (modifiers + per-turn systems)

### Task E1: Create `src/game/stealth.rs` skeleton + `MonsterPerception` component + pure helpers

**Files:**
- Create: `src/game/stealth.rs`
- Modify: `src/game/mod.rs` (register the new module)

- [ ] **Step 1: Write the failing tests**

Create `src/game/stealth.rs` with:

```rust
//! Stealth system — game-side modifier composition + per-turn systems.
//! See docs/design/STEALTH.md for the canonical writeup.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use roguelike_engine::stealth::{Awareness, AwarenessState, NoiseMap};

/// Per-monster species perception modifier, copied from MonsterAsset
/// at spawn time. Read by perception_tick_system to build
/// PerceptionComponents.base. Inserted on every monster in Task F1.
#[derive(Component, Debug, Clone, Copy)]
pub struct MonsterPerception(pub i32);

/// Tile-light → stealth modifier. Bright = penalty, dark = bonus.
/// Thresholds are placeholders — expect post-implementation tuning.
pub fn light_modifier(intensity: f32) -> i32 {
    if intensity >= 0.75 {
        -3
    } else if intensity >= 0.40 {
        -1
    } else if intensity > 0.0 {
        2
    } else {
        3
    }
}

/// Distance → perception bonus. Closer = easier to see.
/// Chebyshev distance (matches 8-way movement).
pub fn close_range_bonus(chebyshev_distance: i32) -> i32 {
    match chebyshev_distance {
        d if d <= 1 => 2,
        2..=3 => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_buckets() {
        assert_eq!(light_modifier(1.0), -3);
        assert_eq!(light_modifier(0.75), -3);
        assert_eq!(light_modifier(0.74), -1);
        assert_eq!(light_modifier(0.40), -1);
        assert_eq!(light_modifier(0.39), 2);
        assert_eq!(light_modifier(0.01), 2);
        assert_eq!(light_modifier(0.0), 3);
    }

    #[test]
    fn close_range_buckets() {
        assert_eq!(close_range_bonus(0), 2);
        assert_eq!(close_range_bonus(1), 2);
        assert_eq!(close_range_bonus(2), 1);
        assert_eq!(close_range_bonus(3), 1);
        assert_eq!(close_range_bonus(4), 0);
        assert_eq!(close_range_bonus(99), 0);
    }
}
```

- [ ] **Step 2: Register the module**

Edit `src/game/mod.rs` — find the `pub mod ...;` block and add `pub mod stealth;` (alphabetical position).

- [ ] **Step 3: Run tests**

```bash
cargo test --lib game::stealth
```

Expected: both tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/game/stealth.rs src/game/mod.rs
git commit -m "stealth: add game-side module with light + distance helpers

light_modifier and close_range_bonus are the two pure positional
modifiers used by compute_stealth_mod / compute_perception_mod.
Bucket thresholds are placeholders for playtest tuning.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E2: `compute_stealth_mod` + `compute_perception_mod` + breakdown structs

**Files:**
- Modify: `src/game/stealth.rs`

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `src/game/stealth.rs`:

```rust
    #[test]
    fn stealth_mod_components_sum_correctly() {
        let parts = StealthComponents {
            skill_half: 6,
            dex_mod: 4,
            armor_penalty: 1,
            light_mod: 2,
            noise_mod: 0,
        };
        assert_eq!(parts.total(), 11);
    }

    #[test]
    fn perception_mod_components_sum_correctly() {
        let parts = PerceptionComponents {
            base: 3,
            asleep_penalty: 0,
            close_range_bonus: 2,
        };
        assert_eq!(parts.total(), 5);
    }

    #[test]
    fn asleep_monster_carries_minus_ten() {
        let parts = PerceptionComponents {
            base: 0,
            asleep_penalty: -10,
            close_range_bonus: 0,
        };
        assert_eq!(parts.total(), -10);
    }
```

- [ ] **Step 2: Add the breakdown structs**

```rust
/// Component breakdown for the stealth side of the opposed roll.
/// Returned by `compute_stealth_mod_explain` for UI display.
#[derive(Debug, Clone, Copy)]
pub struct StealthComponents {
    pub skill_half: i32,
    pub dex_mod: i32,
    pub armor_penalty: i32,
    pub light_mod: i32,
    pub noise_mod: i32,
}

impl StealthComponents {
    pub fn total(&self) -> i32 {
        self.skill_half + self.dex_mod - self.armor_penalty + self.light_mod + self.noise_mod
    }
}

/// Component breakdown for the perception side.
#[derive(Debug, Clone, Copy)]
pub struct PerceptionComponents {
    pub base: i32,
    pub asleep_penalty: i32,    // -10 if asleep, 0 otherwise
    pub close_range_bonus: i32,
}

impl PerceptionComponents {
    pub fn total(&self) -> i32 {
        self.base + self.asleep_penalty + self.close_range_bonus
    }
}
```

- [ ] **Step 3: Run unit tests**

```bash
cargo test --lib game::stealth
```

Expected: 5 tests pass.

- [ ] **Step 4: Add Bevy-side orchestrators**

Append to `src/game/stealth.rs`:

```rust
use crate::components::Position;
use crate::game::skills::Skills;
use crate::character::attributes::Attributes;
// LightMap is engine-side per roguelike_engine::lighting; adjust import:
use roguelike_engine::prelude::LightMap;

/// Build the stealth breakdown for `target` from world state. Returns
/// `None` if `target` has no Position (despawned mid-frame).
///
/// Engineer note: pass `equipped_armor_penalty` via a system param tied
/// to the target's equipped armor slot — see Task E3 for the inventory
/// integration. For now this helper takes the value pre-resolved.
pub fn compute_stealth_components(
    skills: Option<&Skills>,
    attrs: Option<&Attributes>,
    equipped_armor_penalty: i32,
    target_pos: Point,
    light_map: &LightMap,
    noise_map: &NoiseMap,
) -> StealthComponents {
    use crate::game::skills::Skill;
    use crate::character::attributes::ability_mod;
    let stealth_level = skills.map(|s| s.get(Skill::Stealth)).unwrap_or(0.0) as i32;
    let dex_mod = attrs.map(|a| ability_mod(a.dex)).unwrap_or(0);
    let intensity = light_map.intensity_at(target_pos);
    StealthComponents {
        skill_half: stealth_level / 2,
        dex_mod,
        armor_penalty: equipped_armor_penalty,
        light_mod: light_modifier(intensity),
        noise_mod: roguelike_engine::stealth::noise_modifier(target_pos, noise_map),
    }
}

pub fn compute_perception_components(
    monster_base_perception: i32,
    is_asleep: bool,
    chebyshev_distance: i32,
) -> PerceptionComponents {
    PerceptionComponents {
        base: monster_base_perception,
        asleep_penalty: if is_asleep { -10 } else { 0 },
        close_range_bonus: close_range_bonus(chebyshev_distance),
    }
}

pub fn compute_stealth_mod(
    skills: Option<&Skills>,
    attrs: Option<&Attributes>,
    equipped_armor_penalty: i32,
    target_pos: Point,
    light_map: &LightMap,
    noise_map: &NoiseMap,
) -> i32 {
    compute_stealth_components(skills, attrs, equipped_armor_penalty, target_pos, light_map, noise_map).total()
}

pub fn compute_perception_mod(
    monster_base_perception: i32,
    is_asleep: bool,
    chebyshev_distance: i32,
) -> i32 {
    compute_perception_components(monster_base_perception, is_asleep, chebyshev_distance).total()
}
```

> **Engineer note:** Verify the imports. The actual paths in the codebase:
> - `Skills` is at `crate::game::skills::Skills`
> - `Attributes` is at `crate::character::attributes::Attributes` (or similar — `grep -n 'pub struct Attributes' src/character/`)
> - `ability_mod` formula: `(score - 16) / 2` per CLAUDE.md
> - `LightMap.intensity_at(pos) -> f32` — confirm against `/Users/nathanrude/Development/roguelike_engine/src/lighting/`

- [ ] **Step 5: Build**

```bash
cargo check
```

Expected: success. Fix any import mismatches.

- [ ] **Step 6: Commit**

```bash
git add src/game/stealth.rs
git commit -m "stealth: compute_stealth_mod + compute_perception_mod

Component-broken-down breakdown structs (StealthComponents,
PerceptionComponents) feed both the orchestrator total() and the
UI breakdown lines.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E3: Armor lookup helper

**Files:**
- Modify: `src/game/stealth.rs`

The `equipped_armor_penalty` value needs to be sourced from the player's currently-equipped armor. Add a Bevy system-param-friendly helper.

- [ ] **Step 1: Find the equipped-armor pattern**

```bash
grep -rn 'EquippedArmor\|equipped.*armor\|fn equipped_armor' src/game/items.rs src/components.rs --include='*.rs'
```

Note the component / system pattern the existing codebase uses to track equipped armor (it likely lives on the wearer as a `Equipped` marker on the item entity, plus a query on the wearer for `Children` / `Equipment` component).

- [ ] **Step 2: Add the helper**

Append to `src/game/stealth.rs`:

```rust
/// Walks `wearer`'s equipped items and sums `armor_stealth_penalty`
/// across the armor slot. Returns 0 if no armor is equipped.
///
/// Engineer note: adapt this to the actual equipped-armor query shape
/// in items.rs. If the project uses a single `EquipmentSlot::Armor`
/// component on the item entity pointing to the wearer, the body
/// becomes a `Query<&ItemProperties, With<EquippedAsArmor>>` filtered
/// by wearer relationship.
pub fn equipped_armor_stealth_penalty(
    wearer: Entity,
    equipment_query: &Query<(&crate::game::items::Equipped, &crate::game::items::ItemProperties)>,
) -> i32 {
    equipment_query
        .iter()
        .filter(|(eq, _)| eq.wearer == wearer && eq.slot == crate::game::items::EquipmentSlot::Armor)
        .map(|(_, props)| props.armor_stealth_penalty)
        .sum()
}
```

> **Engineer note:** The `Equipped` and `ItemProperties` names above are illustrative — replace with the actual types from `src/game/items.rs`. The grep in Step 1 will reveal them. Most likely the function body is one query iteration; the structure won't change. Add a test using a fake `App` with mock items if the equipment plumbing supports it; otherwise rely on the integration test in Task G.

- [ ] **Step 3: Build**

```bash
cargo check
```

Expected: success (after fixing the actual type names).

- [ ] **Step 4: Commit**

```bash
git add src/game/stealth.rs
git commit -m "stealth: equipped_armor_stealth_penalty helper

Sums the stealth penalty across the wearer's equipped armor slot
items. Used by the perception_tick_system to feed compute_stealth_mod.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E4: `perception_tick_system`

**Files:**
- Modify: `src/game/stealth.rs`

This is the per-turn opposed-roll system that drives the awareness state machine.

- [ ] **Step 1: Add an RNG type and unit-test helper**

Find the codebase's RNG pattern:
```bash
grep -rn 'RandomNumberGenerator\|RngResource\|Rand\|thread_rng' src/game/combat.rs | head -5
```

Use the existing combat-side RNG (likely `bracket_lib::prelude::RandomNumberGenerator`).

- [ ] **Step 2: Write the system**

Append to `src/game/stealth.rs`:

```rust
use roguelike_engine::stealth::{AwarenessAlertEvent, AwarenessRecord};
use roguelike_engine::components::viewshed::Viewshed;
use roguelike_engine::ai::{MonsterAI, MonsterAIMode};
use bracket_lib::prelude::RandomNumberGenerator;

/// Per-perceiver opposed roll system. Runs in ProcessingPhase::Brain
/// before monster AI dispatch so the mode update sees fresh awareness.
///
/// For each perceiver (entity with Awareness + Viewshed), for each
/// entity in their viewshed where awareness state is not Aware:
///   - Compute perception_mod (from perceiver's MonsterAsset + asleep +
///     distance) and stealth_mod (from target's Skills/Attributes/armor/
///     light/noise).
///   - Roll d20 + each, compare. Perception-win → Aware, emit
///     AwarenessAlertEvent for squad propagation.
pub fn perception_tick_system(
    mut perceivers: Query<(Entity, &mut Awareness, &Viewshed, Option<&MonsterAI>, &Position, &MonsterPerception)>,
    targets: Query<(Entity, &Position, Option<&Skills>, Option<&Attributes>)>,
    equipment: Query<(&crate::game::items::Equipped, &crate::game::items::ItemProperties)>,
    light_map: Res<LightMap>,
    noise_map: Res<NoiseMap>,
    current_turn: Res<roguelike_engine::turn::CurrentTurn>,
    mut alerts: MessageWriter<AwarenessAlertEvent>,
    mut rng: ResMut<roguelike_engine::dice::RngResource>,
) {
    let now = current_turn.0;
    for (seeker, mut awareness, vs, ai, seeker_pos, monster_perception) in &mut perceivers {
        let seeker_pos = seeker_pos.to_point();
        let is_asleep = ai.map(|a| a.mode == MonsterAIMode::Asleep).unwrap_or(false);
        let monster_base_perception = monster_perception.0;

        for (target, target_pos_comp, target_skills, target_attrs) in &targets {
            if seeker == target { continue; }
            let target_pos = target_pos_comp.to_point();
            if !vs.visible_tiles.contains(&target_pos) { continue; }

            // Sticky Aware skips the roll.
            if let Some(rec) = awareness.get(target) {
                if matches!(rec.state, AwarenessState::Aware) { continue; }
            }

            let dist = chebyshev(seeker_pos, target_pos);
            let perc_components = compute_perception_components(monster_base_perception, is_asleep, dist);
            let armor_pen = equipped_armor_stealth_penalty(target, &equipment);
            let stealth_components = compute_stealth_components(
                target_skills,
                target_attrs,
                armor_pen,
                target_pos,
                &light_map,
                &noise_map,
            );

            let perc_roll = rng.roll_dice(1, 20);
            let stealth_roll = rng.roll_dice(1, 20);
            let perc_total = perc_roll + perc_components.total();
            let stealth_total = stealth_roll + stealth_components.total();

            if perc_total > stealth_total {
                let new_state = AwarenessState::Aware;
                let mut updated = awareness.records.entry(target).or_insert(AwarenessRecord {
                    state: AwarenessState::Hidden,
                    last_update_turn: now,
                    last_seen_pos: None,
                });
                updated.state = new_state;
                updated.last_update_turn = now;
                updated.last_seen_pos = Some(target_pos);
                alerts.write(AwarenessAlertEvent { seeker, target });
            }
        }
    }
}

fn chebyshev(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}
```

> **Engineer notes:**
> - The `MonsterPerception` component is added in Task F1; until then, leave the placeholder `0` and proceed.
> - `rng.roll_dice(1, 20)` returns 1..=20 inclusive per bracket-lib convention.
> - `Position::to_point()` — confirm this method exists or use `Point::new(pos.x, pos.y)`.
> - The system param signature is long; consider extracting into a `SystemParam`-deriving helper struct if the engineer wishes (not required).

- [ ] **Step 3: Build (expect possible name mismatches; fix iteratively)**

```bash
cargo check
```

Expect a handful of import / method-name mismatches. Fix with greps. The structure should hold.

- [ ] **Step 4: Commit**

```bash
git add src/game/stealth.rs
git commit -m "stealth: perception_tick_system (opposed d20 roll)

Per-perceiver, per-visible-target opposed roll. Sticky Aware skips
the roll. Perception-win transitions to Aware, emits
AwarenessAlertEvent for squad propagation (Phase E5).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E5: Squad propagation handler

**Files:**
- Modify: `src/game/stealth.rs`

- [ ] **Step 1: Find the Squad component**

```bash
grep -n 'pub struct Squad\b\|pub.*Squad\|squad_id' src/game/squad.rs /Users/nathanrude/Development/roguelike_engine/src/squad/*.rs 2>/dev/null | head -10
```

- [ ] **Step 2: Write the handler**

Append to `src/game/stealth.rs`:

```rust
use crate::game::squad::Squad; // adjust path to actual

/// On every AwarenessAlertEvent (a perceiver transitioned to Aware),
/// downgrade the alerted target's awareness to Searching across
/// squadmates. Squadmates move toward last_known_pos but still need
/// to roll perception to confirm visual contact.
pub fn squad_propagate_awareness(
    mut alerts: MessageReader<AwarenessAlertEvent>,
    squad_lookup: Query<(Entity, &Squad)>,
    target_positions: Query<&Position>,
    mut perceivers: Query<&mut Awareness>,
    current_turn: Res<roguelike_engine::turn::CurrentTurn>,
) {
    let now = current_turn.0;
    let giveup_at = now + 20;

    for ev in alerts.read() {
        let Ok(seeker_squad) = squad_lookup.get_component::<Squad>(ev.seeker) else { continue; };
        let Ok(target_pos) = target_positions.get(ev.target) else { continue; };
        let target_pt = target_pos.to_point();

        for (squadmate, sq) in &squad_lookup {
            if squadmate == ev.seeker { continue; }
            if sq.id != seeker_squad.id { continue; }   // adjust field name
            let Ok(mut awareness) = perceivers.get_mut(squadmate) else { continue; };
            let cur = awareness.get(ev.target).map(|r| r.state);
            // Only upgrade — don't downgrade an already-Aware squadmate.
            let should_upgrade = match cur {
                None | Some(AwarenessState::Hidden) | Some(AwarenessState::Suspicious { .. }) => true,
                Some(AwarenessState::Searching { .. }) => false, // refresh timer below
                Some(AwarenessState::Aware) => false,
            };
            if should_upgrade {
                awareness.set(ev.target, AwarenessState::Searching {
                    last_known_pos: target_pt,
                    giveup_at_turn: giveup_at,
                }, now);
            } else if matches!(cur, Some(AwarenessState::Searching { .. })) {
                // Refresh the last_known_pos and timer with the fresh sighting.
                awareness.set(ev.target, AwarenessState::Searching {
                    last_known_pos: target_pt,
                    giveup_at_turn: giveup_at,
                }, now);
            }
        }
    }
}
```

> **Engineer note:** Verify `Squad` field is `.id` or `.squad_id`; adjust accordingly. If the existing `Squad` component is more elaborate, the comparison may instead be `squadmate's squad == seeker's squad`.

- [ ] **Step 3: Build**

```bash
cargo check
```

Expected: success after fixing field-name mismatches.

- [ ] **Step 4: Commit**

```bash
git add src/game/stealth.rs
git commit -m "stealth: squad propagation on AwarenessAlertEvent

When a perceiver becomes Aware, downgrade-propagate Searching to
squadmates with last_known_pos = target's current pos. Squadmates
must still roll to confirm visual contact.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E6: Attack-reveal handler

**Files:**
- Modify: `src/game/stealth.rs`

- [ ] **Step 1: Find the damage-taken event**

```bash
grep -rn 'DamageTakenEvent\|DamageMessage\|pub struct.*Damage' /Users/nathanrude/Development/roguelike_engine/src/combat/ src/game/combat.rs --include='*.rs' | head -10
```

- [ ] **Step 2: Write the handler**

```rust
use roguelike_engine::combat::DamageTakenEvent; // adjust if different

/// Force the victim's perceiver to Aware about its attacker, regardless
/// of stealth roll outcomes that turn. Backstab's "Hidden only" gate
/// reads awareness *before* the damage event is applied — so the first
/// strike still triples; subsequent ones do not.
pub fn attack_reveals_attacker(
    mut events: MessageReader<DamageTakenEvent>,
    mut awareness_query: Query<&mut Awareness>,
    current_turn: Res<roguelike_engine::turn::CurrentTurn>,
) {
    let now = current_turn.0;
    for ev in events.read() {
        let Ok(mut victim_awareness) = awareness_query.get_mut(ev.victim) else { continue; };
        victim_awareness.set(ev.attacker, AwarenessState::Aware, now);
    }
}
```

> **Engineer note:** The `DamageTakenEvent` field names (`victim`, `attacker`) are illustrative — confirm via grep and adjust. If the event doesn't carry the attacker (e.g. environmental damage), filter the read.

- [ ] **Step 3: Commit**

```bash
git add src/game/stealth.rs
git commit -m "stealth: attacking a victim reveals attacker

DamageTakenEvent forces victim.Awareness.set(attacker, Aware). The
Backstab gate reads awareness *before* damage is applied, preserving
the first-hit power spike.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E7: Stealth use-counter system

**Files:**
- Modify: `src/game/stealth.rs`

- [ ] **Step 1: Write the system**

```rust
use crate::components::{Hostile, Player};   // or whichever player marker exists
use crate::game::skills::{Skill, SkillUseCounters};

/// End-of-turn: bump SkillUseCounters.stealth if the player has any
/// hostile in their viewshed whose awareness about the player is not
/// Aware. Each game turn counts at most once.
pub fn bump_stealth_use_counter(
    player_query: Query<(Entity, &Viewshed), With<Player>>,
    hostile_query: Query<&Awareness, With<Hostile>>,
    mut counters: ResMut<SkillUseCounters>,
) {
    let Ok((player_entity, viewshed)) = player_query.single() else { return; };
    for awareness in &hostile_query {
        let rec = awareness.get(player_entity);
        let knows_aware = rec.map(|r| matches!(r.state, AwarenessState::Aware)).unwrap_or(false);
        if !knows_aware {
            // Confirm the hostile is currently visible to the player —
            // viewshed is the cheapest proxy for "in close enough range
            // that stealth matters."
            counters.bump(Skill::Stealth);
            return;
        }
    }
}
```

> **Engineer note:** The `Hostile` marker may not exist; the codebase uses `Faction`. Adjust the query to filter by hostile faction relative to the player. Likely something like `Query<(&Awareness, &Faction)>` followed by a `FactionMatrix::is_hostile(faction, player_faction)` check.

- [ ] **Step 2: Commit**

```bash
git add src/game/stealth.rs
git commit -m "stealth: bump Stealth use counter on each successful hidden turn

End-of-turn: if any hostile within sight has non-Aware awareness about
the player, bump SkillUseCounters.stealth. Trains naturally during
stealth play; pure combat doesn't train it.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task E8: `StealthPlugin` + main.rs registration

**Files:**
- Modify: `src/game/stealth.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the plugin**

Append to `src/game/stealth.rs`:

```rust
pub struct StealthPlugin;

impl Plugin for StealthPlugin {
    fn build(&self, app: &mut App) {
        // Engine plugin registers AwarenessAlertEvent + types.
        app.add_plugins(roguelike_engine::stealth::StealthPlugin)
            // Brain phase — opposed roll runs before AI dispatch.
            .add_systems(
                Update,
                perception_tick_system
                    .in_set(crate::game::turns::ProcessingPhase::Brain)
                    .before(crate::game::turns::monster_ai_dispatch)
                    .run_if(in_state(crate::game::AppState::InGame)),
            )
            // ResolveActions phase — propagate awareness alerts after movement
            // resolves and before AI mode update reads the awareness next turn.
            .add_systems(
                Update,
                (squad_propagate_awareness, attack_reveals_attacker)
                    .in_set(crate::game::turns::ProcessingPhase::ResolveActions)
                    .run_if(in_state(crate::game::AppState::InGame)),
            )
            // Cleanup phase — tick timers + decay noise + bump use counters.
            .add_systems(
                Update,
                (
                    roguelike_engine::stealth::awareness_tick_system,
                    roguelike_engine::stealth::noise_decay_system,
                    bump_stealth_use_counter,
                )
                    .in_set(crate::game::turns::ProcessingPhase::Cleanup)
                    .run_if(in_state(crate::game::AppState::InGame)),
            );

        // NoiseMap resource: insert with map dimensions once the map is
        // available. The simplest pattern: insert it during floor
        // materialization (see src/map/floor_materializer.rs). For V1,
        // insert with zero-sized fallback and let floor materialization
        // overwrite. Adjust to whatever pattern your codebase uses for
        // floor-bound resources.
        app.insert_resource(roguelike_engine::stealth::NoiseMap::new(80, 60));
    }
}
```

> **Engineer note:** `monster_ai_dispatch` system name — confirm via `grep -n 'fn monster_ai_dispatch\|fn monster_ai' src/game/`. If the dispatch is a closure or `.in_set(...)` doesn't work for the before-ordering, swap to a fine-grained sub-set marker.

- [ ] **Step 2: Register in `main.rs`**

Open `src/main.rs`. Find the existing plugin registration block (search for `.add_plugins`). Add `crate::game::stealth::StealthPlugin` to the registration list, near the other game plugins.

- [ ] **Step 3: Build**

```bash
cargo check
cargo build
```

Expected: success (compile clean). If a panic fires at runtime due to `RngResource` ordering, debug from the panic message.

- [ ] **Step 4: Commit**

```bash
git add src/game/stealth.rs src/main.rs
git commit -m "stealth: StealthPlugin wires per-turn systems

perception_tick_system runs in Brain (before AI dispatch).
squad_propagate_awareness + attack_reveals_attacker in ResolveActions.
awareness_tick_system + noise_decay_system + bump_stealth_use_counter
in Cleanup.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase F — Spawning Awareness + per-species perception

### Task F1: Insert `Awareness` + `MonsterPerception` on monster spawn

**Files:**
- Modify: `src/game/spawner.rs`

The `MonsterPerception` component is already defined in Task E1. This task wires it to the spawn pipeline.

- [ ] **Step 1: Update spawner to insert the components**

In `src/game/spawner.rs`, find the monster spawn function (around `spawn_monster` or similar). After the monster entity is created and other components inserted, add:

```rust
entity_commands.insert((
    roguelike_engine::stealth::Awareness::default(),
    crate::game::stealth::MonsterPerception(monster_asset.perception),
));
```

> **Engineer:** the spawn site likely uses a `commands.spawn(...).insert(...)` chain; adapt to that.

- [ ] **Step 2: Update player spawn**

Open `src/player/mod.rs`. Find the player spawn function. Insert `Awareness::default()` on the player too:

```rust
entity_commands.insert(roguelike_engine::stealth::Awareness::default());
```

(The player's `Awareness` is reserved for future stealthed-monster gameplay — empty in V1.)

- [ ] **Step 3: Build + smoke run**

```bash
cargo check
cargo run --release
```

Verify the game launches and the player can move around. Stealth interactions aren't visible yet without UI (Phase H), but the `perception_tick_system` from E4 should now actually iterate over monsters (since they now have the components it queries). No crashes should occur.

- [ ] **Step 4: Commit**

```bash
git add src/game/stealth.rs src/game/spawner.rs src/player/mod.rs
git commit -m "stealth: spawn Awareness component + MonsterPerception

Every monster gets Awareness::default() + MonsterPerception(asset.perception).
Player gets Awareness::default() (empty map; reserved for future stealthed
monsters).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase G — Backstab gate

### Task G1: Update Backstab to require `Hidden` awareness

**Files:**
- Modify: `src/game/combat.rs:374-383`

- [ ] **Step 1: Write the failing test (or augment existing)**

Find the existing Backstab tests:
```bash
grep -n 'Backstab\|backstab' src/game/combat.rs
```

Add (in the existing `#[cfg(test)] mod tests` block):

```rust
    #[test]
    fn backstab_fires_when_target_is_hidden() {
        use roguelike_engine::stealth::{Awareness, AwarenessState};
        let mut a = Awareness::default();
        // No record → defaults to Hidden.
        let player = Entity::from_raw(1);
        let monster = Entity::from_raw(2);
        // Helper:
        assert!(backstab_applies(&a, player));
    }

    #[test]
    fn backstab_skips_when_target_is_aware() {
        use roguelike_engine::stealth::{Awareness, AwarenessState};
        let mut a = Awareness::default();
        let player = Entity::from_raw(1);
        a.set(player, AwarenessState::Aware, 0);
        assert!(!backstab_applies(&a, player));
    }

    #[test]
    fn backstab_skips_when_target_is_searching() {
        use roguelike_engine::stealth::{Awareness, AwarenessState};
        use bracket_lib::prelude::Point;
        let mut a = Awareness::default();
        let player = Entity::from_raw(1);
        a.set(player, AwarenessState::Searching {
            last_known_pos: Point::new(0, 0),
            giveup_at_turn: 100,
        }, 0);
        assert!(!backstab_applies(&a, player));
    }
```

- [ ] **Step 2: Run (expect failure on missing `backstab_applies`)**

```bash
cargo test --lib game::combat::tests::backstab
```

Expected: compile error.

- [ ] **Step 3: Extract the gate as a pure helper**

Edit `src/game/combat.rs`. Above the existing combat systems, add:

```rust
/// Returns true if a Backstab weapon should trigger triple damage
/// against `target_of_attack`, based on the *attacker monster's*
/// awareness of `target_of_attack`. Hidden = no idea = ambush.
///
/// Note: when the player wields a Backstab weapon and attacks a
/// monster, the relevant awareness is the *monster's* Awareness about
/// the *player* (the player is the target of the monster's perception).
pub(crate) fn backstab_applies(
    attacker_target_awareness: &roguelike_engine::stealth::Awareness,
    attacker: Entity,
) -> bool {
    use roguelike_engine::stealth::AwarenessState;
    matches!(
        attacker_target_awareness.get(attacker).map(|r| r.state),
        None | Some(AwarenessState::Hidden)
    )
}
```

Then update the existing Backstab block (around line 374-383) to call this helper. The current block reads `MonsterAIMode::Asleep`; rewrite to:

```rust
// Phase 4 stealth: Backstab triples damage when the monster is unaware.
if props.weapon_ability.as_deref() == Some("Backstab") {
    if let Ok(monster_awareness) = monster_query.get_component::<roguelike_engine::stealth::Awareness>(target_entity) {
        if backstab_applies(monster_awareness, player_entity) {
            damage *= 3;
            log_writer.write(GameLogMessage("Backstab! Triple damage!".to_string()));
        }
    }
}
```

> **Engineer note:** Confirm the variable names (`props`, `damage`, `target_entity`, `player_entity`, `monster_query`). The structure should hold; only names change.

- [ ] **Step 4: Run tests, expect pass**

```bash
cargo test --lib game::combat
```

Expected: 3 new tests pass + all existing combat tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/game/combat.rs
git commit -m "combat: Backstab gates on Hidden awareness, not Asleep mode

Asleep monsters are still Hidden (mapping is preserved), so the
existing first-hit power spike against sleeping monsters continues to
work. Searching / Suspicious / Aware all reject Backstab now.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase H — UI

### Task H1: Nearby sidebar status pill

**Files:**
- Modify: `src/ui/nearby.rs`

- [ ] **Step 1: Read the existing sidebar layout**

```bash
sed -n '1,40p' src/ui/nearby.rs
```

Identify the per-monster row spawn — likely a function that builds child nodes for each visible monster.

- [ ] **Step 2: Add a status-text helper**

Append to `src/ui/nearby.rs`:

```rust
use roguelike_engine::ai::MonsterAIMode;
use roguelike_engine::stealth::{Awareness, AwarenessState};

/// Status pill text + colour for a monster row. Returns ("Wandering",
/// dim grey) by default; reads MonsterAIMode + Awareness about the player.
pub(super) fn awareness_pill(
    mode: MonsterAIMode,
    awareness: &Awareness,
    player_entity: Entity,
) -> (&'static str, Color) {
    if mode == MonsterAIMode::Asleep {
        return ("Sleeping", Color::srgb(0.45, 0.45, 0.45));
    }
    match awareness.get(player_entity).map(|r| r.state) {
        Some(AwarenessState::Aware) => ("Hunting", Color::srgb(0.85, 0.20, 0.20)),
        Some(AwarenessState::Searching { .. }) => ("Searching", Color::srgb(0.95, 0.78, 0.20)),
        Some(AwarenessState::Suspicious { .. }) => ("Suspicious", Color::srgb(0.95, 0.78, 0.20)),
        None | Some(AwarenessState::Hidden) => ("Wandering", Color::srgb(0.55, 0.55, 0.55)),
    }
}
```

- [ ] **Step 3: Spawn the pill in the per-monster row**

Find the row-building function (look for the per-monster `commands.spawn(...).with_children(|parent| { ... })` block). Inside the row's children, after the name/HP text, add:

```rust
let (text, color) = awareness_pill(mode, awareness, player_entity);
parent.spawn((
    Text::new(text),
    TextFont { font_size: 11.0, ..Default::default() },
    TextColor(color),
));
```

> **Engineer note:** `mode`, `awareness`, and `player_entity` need to flow in as query params. Likely the existing function already queries the monster's components — add `&MonsterAI` and `&Awareness` to the parameter list, and the player entity as a separate `Query<Entity, With<Player>>` resolved once.

- [ ] **Step 4: Smoke test**

```bash
cargo run --release
```

Walk around the dungeon. Verify the nearby sidebar shows "Wandering" / "Sleeping" / "Hunting" pills below each visible monster. Trigger a wake by walking into a goblin's LOS — the pill should flip to "Hunting" within a turn.

- [ ] **Step 5: Commit**

```bash
git add src/ui/nearby.rs
git commit -m "ui: nearby sidebar shows per-monster awareness pill

Sleeping/Wandering/Suspicious/Searching/Hunting pill below each visible
monster row. Reads MonsterAIMode + Awareness.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task H2: Hover tooltip "Notice this turn" block

**Files:**
- Modify: `src/ui/hover_info.rs`

- [ ] **Step 1: Read the existing tooltip layout**

```bash
sed -n '1,60p' src/ui/hover_info.rs
```

Find the function that builds the tooltip content for a hovered monster.

- [ ] **Step 2: Add a Stealth section helper**

Append to `src/ui/hover_info.rs`:

```rust
use crate::game::stealth::{
    compute_perception_components, compute_stealth_components,
    equipped_armor_stealth_penalty, MonsterPerception, StealthComponents,
    PerceptionComponents,
};
use roguelike_engine::stealth::{notice_probability, Awareness, AwarenessState};

pub(super) struct StealthDisplayLines {
    pub headline: String,    // "Notice this turn: 87%" or "Already aware" or "Out of sight"
    pub perception: Option<PerceptionComponents>,  // None for non-roll cases
    pub stealth: Option<StealthComponents>,
}

pub(super) fn stealth_display_for(
    monster: Entity,
    monster_perception: i32,
    monster_pos: Point,
    is_asleep: bool,
    in_viewshed: bool,
    awareness: &Awareness,
    player_entity: Entity,
    player_pos: Point,
    player_skills: Option<&Skills>,
    player_attrs: Option<&Attributes>,
    player_armor_pen: i32,
    light_map: &LightMap,
    noise_map: &NoiseMap,
) -> StealthDisplayLines {
    let state = awareness.get(player_entity).map(|r| r.state);
    if matches!(state, Some(AwarenessState::Aware)) {
        return StealthDisplayLines {
            headline: "Already aware".to_string(),
            perception: None,
            stealth: None,
        };
    }
    if !in_viewshed {
        return StealthDisplayLines {
            headline: "Out of sight".to_string(),
            perception: None,
            stealth: None,
        };
    }
    let dist = (player_pos.x - monster_pos.x).abs().max((player_pos.y - monster_pos.y).abs());
    let perc = compute_perception_components(monster_perception, is_asleep, dist);
    let stealth = compute_stealth_components(player_skills, player_attrs, player_armor_pen, player_pos, light_map, noise_map);
    let delta = perc.total() - stealth.total();
    let pct = (notice_probability(delta) * 100.0).round() as i32;
    StealthDisplayLines {
        headline: format!("Notice this turn: {}%", pct),
        perception: Some(perc),
        stealth: Some(stealth),
    }
}

pub(super) fn render_stealth_lines(parent: &mut ChildBuilder, lines: &StealthDisplayLines) {
    parent.spawn((
        Text::new("─ Stealth ───────────"),
        TextFont { font_size: 11.0, ..Default::default() },
        TextColor(Color::srgb(0.5, 0.5, 0.5)),
    ));
    parent.spawn((
        Text::new(&lines.headline),
        TextFont { font_size: 12.0, ..Default::default() },
        TextColor(Color::WHITE),
    ));
    if let (Some(p), Some(s)) = (&lines.perception, &lines.stealth) {
        let body = format!(
            "  Perception: {:+}\n    base species: {:+}\n    adjacent:    {:+}\n    asleep:      {:+}\n  Stealth:    {:+}\n    skill:       {:+}\n    DEX:         {:+}\n    armor:       {:+}\n    light:       {:+}\n    noise:       {:+}",
            p.total(),
            p.base, p.close_range_bonus, p.asleep_penalty,
            s.total(),
            s.skill_half, s.dex_mod, -s.armor_penalty, s.light_mod, s.noise_mod,
        );
        parent.spawn((
            Text::new(body),
            TextFont { font_size: 10.0, ..Default::default() },
            TextColor(Color::srgb(0.7, 0.7, 0.7)),
        ));
    }
}
```

- [ ] **Step 3: Call the helper from the hover-tooltip builder**

In the existing hover-tooltip builder function (where the monster row is rendered into the tooltip panel), after the monster stats section, call:

```rust
let lines = stealth_display_for(/* args from queries */);
parent.with_children(|p| render_stealth_lines(p, &lines));
```

The exact wiring depends on the existing function signature; add the necessary `Query` params:

- `Query<(&MonsterPerception, &MonsterAI, &Position, &Awareness)>` for the monster
- `Query<(Entity, &Position, &Viewshed, Option<&Skills>, Option<&Attributes>)>` for the player
- `Res<LightMap>`, `Res<NoiseMap>`

- [ ] **Step 4: Smoke test**

```bash
cargo run --release
```

Hover over a sleeping monster. The tooltip should show "Notice this turn: 5%" or similar (asleep monster, dim corridor → low chance). Hover over an aware monster mid-fight → "Already aware". Hover over a monster behind a wall (out of its viewshed but in yours) → "Out of sight".

- [ ] **Step 5: Commit**

```bash
git add src/ui/hover_info.rs
git commit -m "ui: hover tooltip shows Notice-this-turn percentage + breakdown

Always-on inline breakdown. Edge cases: Aware → 'Already aware', no
LOS → 'Out of sight'. Percentage from notice_probability(perc - stealth).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task H3: Monster info screen "Stealth" block

**Files:**
- Modify: `src/ui/monster_info.rs`

- [ ] **Step 1: Reuse the helper from H2**

The monster info overlay already shows monster name, HP, abilities. Add a Stealth section using the same `stealth_display_for` and `render_stealth_lines` helpers.

- [ ] **Step 2: Build + smoke test**

Press the inspection key (probably `x` or `i` — check `src/ui/menu.rs` or `src/ui/monster_info.rs` for the binding) on a visible monster. The expanded overlay should show the Stealth block.

- [ ] **Step 3: Commit**

```bash
git add src/ui/monster_info.rs
git commit -m "ui: monster info overlay shows Stealth block

Same Notice-this-turn percentage + breakdown as the hover tooltip,
in the dedicated monster inspection screen.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase I — Save / load

### Task I1: Add `MonsterAwarenessSave` + degrade-on-save

**Files:**
- Modify: `src/save/mod.rs`

- [ ] **Step 1: Find the SavedMonster struct + save version constant**

```bash
grep -n 'pub struct SavedMonster\|SAVE_VERSION\|schema_version' src/save/mod.rs
```

- [ ] **Step 2: Add the new save type + bump version**

In `src/save/mod.rs`, add (near `SavedMonster`):

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MonsterAwarenessSave {
    /// Player-only-keyed awareness state, collapsed per the V1 degraded
    /// persistence spec: Aware → Searching{last_known}, Suspicious/Searching
    /// → Hidden if no last_known available, Hidden → Hidden.
    pub state: SavedAwarenessState,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SavedAwarenessState {
    Hidden,
    Searching {
        last_known_x: i32,
        last_known_y: i32,
        giveup_at_offset: u32,   // = giveup_at_turn - now_at_save
    },
}

impl Default for SavedAwarenessState {
    fn default() -> Self { SavedAwarenessState::Hidden }
}
```

Add a field to `SavedMonster`:

```rust
#[serde(default)]
pub awareness: MonsterAwarenessSave,
```

Bump the save schema version from 6 to 7. Find the version constant (likely `const SAVE_VERSION: u32 = 6;`) and increment. Update any migration code that handles older saves to default `awareness` to `Default::default()` for v6-and-earlier loads.

- [ ] **Step 3: Wire the degrade-on-save**

Find the auto-save system (`auto_save_system` per earlier grep). In the per-monster save loop, build the `MonsterAwarenessSave`:

```rust
let awareness = monster_awareness_query
    .get(monster_entity)
    .ok()
    .map(|aw| degrade_awareness_for_save(aw, player_entity, player_pos, current_turn))
    .unwrap_or_default();
```

Add the pure helper at the top of `src/save/mod.rs`:

```rust
pub(crate) fn degrade_awareness_for_save(
    awareness: &roguelike_engine::stealth::Awareness,
    player_entity: Entity,
    player_pos: Point,
    now: u32,
) -> MonsterAwarenessSave {
    use roguelike_engine::stealth::AwarenessState;
    let saved_state = match awareness.get(player_entity).map(|r| r.state) {
        Some(AwarenessState::Aware) => SavedAwarenessState::Searching {
            last_known_x: player_pos.x,
            last_known_y: player_pos.y,
            giveup_at_offset: 20,
        },
        Some(AwarenessState::Searching { last_known_pos, giveup_at_turn }) => {
            SavedAwarenessState::Searching {
                last_known_x: last_known_pos.x,
                last_known_y: last_known_pos.y,
                giveup_at_offset: giveup_at_turn.saturating_sub(now),
            }
        }
        _ => SavedAwarenessState::Hidden,
    };
    MonsterAwarenessSave { state: saved_state }
}

#[cfg(test)]
mod stealth_save_tests {
    use super::*;
    use roguelike_engine::stealth::{Awareness, AwarenessState};

    #[test]
    fn aware_collapses_to_searching() {
        let mut a = Awareness::default();
        let player = Entity::from_raw(1);
        a.set(player, AwarenessState::Aware, 10);
        let saved = degrade_awareness_for_save(&a, player, Point::new(5, 5), 10);
        assert!(matches!(saved.state, SavedAwarenessState::Searching { last_known_x: 5, last_known_y: 5, giveup_at_offset: 20 }));
    }

    #[test]
    fn searching_preserves_last_known_with_offset() {
        let mut a = Awareness::default();
        let player = Entity::from_raw(1);
        a.set(player, AwarenessState::Searching {
            last_known_pos: Point::new(3, 4),
            giveup_at_turn: 50,
        }, 10);
        let saved = degrade_awareness_for_save(&a, player, Point::new(99, 99), 30);
        assert!(matches!(saved.state, SavedAwarenessState::Searching { last_known_x: 3, last_known_y: 4, giveup_at_offset: 20 }));
    }

    #[test]
    fn suspicious_collapses_to_hidden() {
        let mut a = Awareness::default();
        let player = Entity::from_raw(1);
        a.set(player, AwarenessState::Suspicious {
            suspect_pos: Point::new(7, 7),
            decay_at_turn: 100,
        }, 0);
        let saved = degrade_awareness_for_save(&a, player, Point::new(0, 0), 0);
        assert_eq!(saved.state, SavedAwarenessState::Hidden);
    }

    #[test]
    fn no_record_means_hidden() {
        let a = Awareness::default();
        let saved = degrade_awareness_for_save(&a, Entity::from_raw(1), Point::new(0, 0), 0);
        assert_eq!(saved.state, SavedAwarenessState::Hidden);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib save::stealth_save
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/save/mod.rs
git commit -m "save: degrade awareness on save (schema v7)

MonsterAwarenessSave persists per-monster player-keyed awareness as
either Hidden or Searching{last_known_pos, giveup_at_offset}. Aware
collapses to Searching with player's current pos. Suspicious/Searching
without last_known collapse to Hidden.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task I2: Restore-on-load

**Files:**
- Modify: `src/save/mod.rs`

- [ ] **Step 1: Find the load path**

```bash
grep -n 'apply_player_load_system\|fn.*load_floor\|restore' src/save/mod.rs | head -10
```

The monster restore likely happens in `load_floor_data` or similar — where each `SavedMonster` is turned back into a spawned entity.

- [ ] **Step 2: Add a pure restore helper**

In `src/save/mod.rs`:

```rust
pub(crate) fn restore_awareness_from_save(
    saved: &MonsterAwarenessSave,
    player_entity: Entity,
    now: u32,
) -> roguelike_engine::stealth::Awareness {
    use roguelike_engine::stealth::{Awareness, AwarenessRecord, AwarenessState};
    let mut a = Awareness::default();
    let (last_known, giveup_at_turn) = match saved.state {
        SavedAwarenessState::Hidden => return a,
        SavedAwarenessState::Searching { last_known_x, last_known_y, giveup_at_offset } => {
            (Point::new(last_known_x, last_known_y), now + giveup_at_offset)
        }
    };
    a.records.insert(player_entity, AwarenessRecord {
        state: AwarenessState::Searching { last_known_pos: last_known, giveup_at_turn },
        last_update_turn: now,
        last_seen_pos: Some(last_known),
    });
    a
}

#[cfg(test)]
mod stealth_load_tests {
    use super::*;
    use bracket_lib::prelude::Point;

    #[test]
    fn hidden_save_restores_empty_awareness() {
        let saved = MonsterAwarenessSave { state: SavedAwarenessState::Hidden };
        let a = restore_awareness_from_save(&saved, Entity::from_raw(1), 0);
        assert!(a.records.is_empty());
    }

    #[test]
    fn searching_save_restores_with_recomputed_turn() {
        let saved = MonsterAwarenessSave {
            state: SavedAwarenessState::Searching {
                last_known_x: 7,
                last_known_y: 8,
                giveup_at_offset: 15,
            },
        };
        let now = 100;
        let player = Entity::from_raw(1);
        let a = restore_awareness_from_save(&saved, player, now);
        let rec = a.records.get(&player).unwrap();
        match rec.state {
            roguelike_engine::stealth::AwarenessState::Searching { last_known_pos, giveup_at_turn } => {
                assert_eq!(last_known_pos, Point::new(7, 8));
                assert_eq!(giveup_at_turn, 115);
            }
            _ => panic!("expected Searching"),
        }
    }
}
```

- [ ] **Step 3: Wire it into the monster restore loop**

After each monster is spawned during load, call `restore_awareness_from_save(&saved.awareness, player_entity, current_turn)` and insert the result onto the new monster entity:

```rust
let restored = restore_awareness_from_save(&saved_monster.awareness, player_entity, current_turn);
commands.entity(monster_entity).insert(restored);
```

> **Engineer note:** The `player_entity` must already exist by the time this runs. If the existing load order spawns monsters before the player, swap the order so the player spawns first, then monsters with their restored awareness pointing at the player.

- [ ] **Step 4: Run tests**

```bash
cargo test --lib save
```

Expected: all save tests pass.

- [ ] **Step 5: Smoke test save/load round-trip**

```bash
cargo run --release
```

In-game: walk near a monster until it's Hunting → save (whatever the existing save key/menu is) → exit → relaunch → load. The monster should resume as Searching, moving toward your save-time position.

- [ ] **Step 6: Commit**

```bash
git add src/save/mod.rs
git commit -m "save: restore awareness on load with reconstructed timer

Player-keyed awareness reconstruction. Hidden saves restore to an
empty Awareness map; Searching saves restore to Searching with
recomputed giveup_at_turn = now + giveup_at_offset.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase J — Documentation

### Task J1: Create `docs/design/STEALTH.md`

**Files:**
- Create: `docs/design/STEALTH.md`

- [ ] **Step 1: Write the canonical doc**

Create `docs/design/STEALTH.md`:

```markdown
# STEALTH.md — Stealth & Awareness System

> Canonical writeup. Test-enforced design contracts marked with
> `> Maintenance contract:` blockquotes.

## Design Philosophy

[2-3 paragraphs explaining why stealth exists, how it ties to Rogue
class identity, how it interacts with combat and lighting.]

## Awareness model

[Describe the 4 states + transitions + sticky Aware + Searching decay.
Include the same state-machine ASCII diagram from the spec.]

## Detection formula

[d20 + perception_mod vs d20 + stealth_mod. Tabulate the modifier
sources. Quote `notice_probability(delta)` for reference.]

## Stealth skill

[Per-skill summary. Race aptitudes table. Class starting allocation
table. Use counter rule.]

> Maintenance contract: tests `every_class_starting_skills_sums_to_ten`
> and `every_race_aptitude_value_is_in_range` automatically cover
> `Skill::Stealth` because the helpers in `src/character/asset.rs`
> include the `stealth` field. Adding a new race or class will also
> automatically be validated.

## Backstab

[Backstab gates on Hidden only. Asleep maps to Hidden, so first-strike
ambushes still triple. Cross-link to ITEMS.md for the Dagger ability.]

## Noise map (V2 hook)

[Describe NoiseMap resource and the decay system. Note that V1 ships
the data flow but no producer writes to it. V2 will populate from
action events via Dijkstra.]

## Save persistence

[V1 degraded approach: Aware → Searching{last_known} at save time.
Schema v6 → v7.]

## Cross-links

- [CHARACTER.md](CHARACTER.md) — Rogue class skill allocation
- [SKILLS.md](SKILLS.md) — Stealth skill in the trainable skill list
- [SQUAD_AI.md](SQUAD_AI.md) — Searching-propagation across squadmates
- [LIGHT.md](LIGHT.md) — Light intensity buckets feed light_modifier
- [SAVE schema v7] — degraded awareness persistence
```

Fill out the bracketed sections from the spec — paraphrase rather than copy-pasting, but preserve all the same constants and decisions.

- [ ] **Step 2: Cross-link from CLAUDE.md**

In `CLAUDE.md`'s Design Documentation table (the per-system-docs section), add a row:

```markdown
| [STEALTH.md](docs/design/STEALTH.md) | Per-perceiver awareness model, opposed d20 detection, Stealth skill, Backstab gate, noise map V2 hook |
```

- [ ] **Step 3: Commit**

```bash
git add docs/design/STEALTH.md CLAUDE.md
git commit -m "docs: canonical STEALTH.md writeup

Per-perceiver awareness model, detection formula, Stealth skill,
Backstab gate, noise map V2 hook. Cross-linked from CLAUDE.md.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task J2-J5: Update related design docs

**Files:**
- Modify: `docs/design/CHARACTER.md`
- Modify: `docs/design/SKILLS.md`
- Modify: `docs/design/SQUAD_AI.md`
- Modify: `docs/design/ENEMIES.md`

For each file, edit the relevant sections:

**CHARACTER.md** — find the class table and update Rogue + Ranger `starting_skills` rows to include Stealth 2 / Stealth 1. Update the Rogue and Ranger "Playstyle at a glance" prose to mention the Stealth axis. The `every_class_*` tests automatically validate.

**SKILLS.md** — Add a row in §1 (skill list) for Stealth with `floor(skill / 2)` formula, "Notice opposed roll" effect, use-counter rule. Add a row in §2 (class starting skills) for the new distributions. Add a column in §3 (race aptitudes) for Stealth. Make sure §7 (weapon-to-skill mapping) is unchanged — Stealth has no weapon binding.

**SQUAD_AI.md** — Add a paragraph: "When one squad member transitions to `Aware`, squadmates receive `Searching{last_known_pos}` via `AwarenessAlertEvent`, not direct `Aware`. They must still roll perception to confirm visual contact. This is intentional — instant squadwide `Aware` would feel like radar."

**ENEMIES.md** — In the monster authoring section, document the `perception: i32` field on `MonsterAsset` with the recommended value table from Task C1.

- [ ] **Step 1: Make the edits**

- [ ] **Step 2: Run the maintenance tests to verify content matches RON**

```bash
cargo test --lib character::asset
```

Expected: PASS (Rogue/Ranger row matches `classes.ron`).

- [ ] **Step 3: Commit each file individually** (clearer history):

```bash
git add docs/design/CHARACTER.md && git commit -m "docs: update CHARACTER.md for Stealth skill / Rogue+Ranger redistribution"
git add docs/design/SKILLS.md    && git commit -m "docs: update SKILLS.md with Stealth row + aptitudes + class allocations"
git add docs/design/SQUAD_AI.md  && git commit -m "docs: update SQUAD_AI.md — searching-propagation, not aware"
git add docs/design/ENEMIES.md   && git commit -m "docs: document perception field in monster authoring"
```

### Task J6: Update CLAUDE.md project structure

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add the new module to the project structure block**

Find the "Project Structure" code block in `CLAUDE.md`. Add lines under `src/game/`:

```
    stealth.rs            # Stealth system (Phase 4): compute_*_mod, perception_tick_system, squad propagation, Backstab gate, use-counter, StealthPlugin
```

- [ ] **Step 2: Add an "Awareness / Stealth System" subsection under "Key Architectural Patterns"**

Add (after the existing combat-related sections):

```markdown
### Stealth & Awareness ([STEALTH.md](docs/design/STEALTH.md))
- Per-perceiver `Awareness` component (engine-side, `roguelike_engine::stealth::Awareness`) maps `target_entity → AwarenessState`.
- 4 states: `Hidden | Suspicious | Searching | Aware`. Aware is sticky — no rolls fire against an Aware target until LOS is lost.
- Opposed d20 roll fires on each perceiver's turn against non-Aware visible targets in [src/game/stealth.rs](src/game/stealth.rs).
- `MonsterAIMode` is driven by `Awareness` via `MonsterAI::update_mode_from_awareness` (engine).
- Backstab triple-damage gates on `AwarenessState::Hidden` only (combat.rs).
- `NoiseMap` resource ships in V1 with decay-by-1 tick but no producer. V2 noise phase plugs in Dijkstra populator.
- Save schema v7: degraded persistence (Aware → Searching{last_known_pos} on save).
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "claude-md: document stealth module + architectural pattern"
```

### Task J7: Update content-studio ron-schemas reference

**Files:**
- Modify: `.claude/skills/content-studio/references/ron-schemas.md`

- [ ] **Step 1: Document the new fields**

Find the `MonsterAsset` schema section in the file. Add:

```markdown
| `perception` | `i32` (default `0`) | Phase-4 stealth perception modifier. Range ~-3..=+5 per shipping monster. Subtracted from the d20 perception roll. |
```

Find the armor schema section (or item / wearable section). Add:

```markdown
| `armor_stealth_penalty` | `i32` (default `0`) | Stealth modifier penalty when worn. 0 cloth, 1 leather/padded, 2 studded, 3 chain, 5 plate. |
```

- [ ] **Step 2: Commit**

```bash
git add .claude/skills/content-studio/references/ron-schemas.md
git commit -m "skills: document perception + armor_stealth_penalty RON fields"
```

---

## Phase K — Verify

### Task K1: Full build + test + clippy

- [ ] **Step 1: From bevy_rpg root, build clean**

```bash
cd /Users/nathanrude/Development/bevy_rpg
cargo clean
cargo build --release 2>&1 | tail -20
```

Expected: success.

- [ ] **Step 2: Run all tests**

```bash
cargo test --workspace
```

Expected: all tests pass. Note any new failures and address them before proceeding.

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no warnings. Fix any issues with named fixes (no blanket `#[allow]`).

- [ ] **Step 4: Run engine-side tests in isolation**

```bash
cd /Users/nathanrude/Development/roguelike_engine
cargo test --lib
cargo clippy --lib -- -D warnings
```

Expected: all green.

### Task K2: Manual smoke test

- [ ] **Step 1: Play through the early-game stealth scenarios**

```bash
cd /Users/nathanrude/Development/bevy_rpg
cargo run --release
```

Verify in-game:

1. Start a Rogue character. Confirm the skill screen (`M`) shows Stealth with 2 points.
2. Walk into a forest tile with a sleeping monster. Hover over it. Should see "Notice this turn: <small>%" and a breakdown.
3. Walk adjacent in a dark tile. Should still be "Sleeping" pill.
4. Walk adjacent in a lit tile. Should flip to "Hunting" within ~1 turn.
5. Walk into combat, then run 5+ tiles away breaking LOS. Pill should flip to "Searching".
6. Wait 20+ turns at distance. Pill should flip back to "Wandering".
7. Save in mid-combat. Reload. Verify the monster is Searching toward your save-time position.
8. Backstab a sleeping monster — should trigger "Backstab! Triple damage!" in the log.
9. Backstab a Searching monster — should NOT trigger.

- [ ] **Step 2: Note any gameplay-feel issues**

Adjust constants in `src/game/stealth.rs` (light thresholds, close_range_bonus buckets) if anything feels obviously wrong. These are placeholders by design.

---

## Phase L — Ship the engine commit

### Task L1: Push engine + restore Git dep + bump ref

**Files:**
- Modify: `Cargo.toml` (revert path-dep)
- Modify: `Cargo.lock` (auto-updated)

- [ ] **Step 1: Push the engine repo**

```bash
cd /Users/nathanrude/Development/roguelike_engine
git log --oneline -10
git push origin main
```

Expected: push succeeds.

- [ ] **Step 2: Note the new HEAD commit SHA**

```bash
git rev-parse HEAD
```

Copy the full SHA.

- [ ] **Step 3: Restore the Git dep in bevy_rpg's Cargo.toml**

Open `/Users/nathanrude/Development/bevy_rpg/Cargo.toml`. Revert the path-dep swap from Task A1:

```toml
roguelike_engine = { git = "https://github.com/rudehn/roguelike_engine", branch = "main" }
# For local development, swap to the path dependency:
# roguelike_engine = { path = "../roguelike_engine" }
```

- [ ] **Step 4: Update Cargo.lock**

```bash
cd /Users/nathanrude/Development/bevy_rpg
cargo update -p roguelike_engine
```

Expected: lockfile picks up the new HEAD commit on `main`.

- [ ] **Step 5: Build clean against Git dep**

```bash
cargo clean
cargo build --release
cargo test --workspace
```

Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: bump roguelike_engine to include stealth subsystem

Restores the Git dep after the local path-dep development phase.
Engine commit pulled in by this bump:
  - stealth: AwarenessState + Awareness + NoiseMap + tick systems
  - ai: MonsterAI::update_mode_from_awareness

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review Checklist

After completing all tasks, verify spec coverage:

| Spec section | Implemented by |
| --- | --- |
| §3 State machine (4 states, sticky Aware, hysteresis) | B1 (types), B4 (tick), E4 (roll), E5/E6 (transitions) |
| §4.1 Engine types | B1 |
| §4.2 MonsterAsset.perception | C1 (game-side per drift note) |
| §4.3 Game stealth module | E1-E8 |
| §4.4 armor_stealth_penalty | C2 |
| §5.1 perception_mod formula | E2, E4, F1 |
| §5.2 stealth_mod formula | E1, E2 |
| §5.3 notice_probability | B2 |
| §6.1 perception_tick_system | E4, E8 |
| §6.2 awareness_tick_system | B4, E8 |
| §6.3 noise_decay_system | B3, E8 |
| §6.4 MonsterAIMode driven by awareness | B6 |
| §6.5 Squad propagation | E5 |
| §6.6 Attack reveals attacker | E6 |
| §6.7 Backstab gate | G1 |
| §7 Stealth skill | D1-D4 |
| §8.1 Nearby sidebar pill | H1 |
| §8.2/8.3 Hover tooltip + monster info | H2, H3 |
| §9 Save persistence | I1, I2 |
| Test plan §10 | Distributed across B1, B2, B3, B4, D1, E1, E2, G1, I1, I2 |

If any row above can't be filled with a real task ID after the engineer implements, that's a plan gap — add a follow-up task.
