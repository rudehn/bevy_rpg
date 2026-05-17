# RFC 0001 — Combat as a Pure Resolution Pipeline

**Status:** Landed
**Branch:** `refactor/combat-resolver-rfc`
**Related docs:** [GAME.md](../design/GAME.md), [CHARACTER.md](../design/CHARACTER.md), [SKILLS.md](../design/SKILLS.md), [ABILITIES.md](../design/ABILITIES.md)

## What shipped

- `src/game/combat/resolve.rs` — pure-Rust resolver, 49 unit tests
- `attack_resolution_system` in `src/game/combat/mod.rs` — Bevy adapter (~200 LOC, replaces a 350-LOC two-stage pair)
- `resolve::apply_damage` consumed by Cleave splash (per-target armor + shield)
- `resolve::roll_damage` + `resolve::apply_damage` consumed by Lightning chain + Fire AoE staff zaps
- `range_to_dice` helper in `src/game/staves.rs` converting `(low, high)` curves to engine dice notation
- `DefenderQueries` + `StaffEventWriters` SystemParams centralizing defender snapshot construction and writer bundling

### Deviations from the original plan

- **Resistance moved out of the resolver.** The drafted resolver applied resistance internally, which would have double-resisted every hit alongside the engine's `damage_application_system`. Resistance is the engine's job; the resolver stops at shield block + armor roll and exposes `armor_roll` to the adapter.
- **`ShieldKind` enum dropped.** Production shields come from items.ron as integer `Block` values; the enum forced a brittle `Block(3) → Buckler` mapping on the adapter. Replaced with `shield_block_bonus: i32`.
- **Cleave splash, Lightning chain, Fire AoE now respect shields.** This was promised in CLAUDE.md ("shield blocks beat fire/poison/lightning equally") but had been unmet in production. Cleave splash also now respects per-victim armor.
- **Force / Healing / Poison / Blinking staff effects stay inline.** Force is knockback, Healing is `HealEvent` (a different pipeline), Poison only applies a status, Blinking is teleport. None have a damage roll the resolver could reuse.

## Problem

The combat math is glued to Bevy systems and sprinkled across six files.

- **`hit_check_system` and `damage_roll_system` in `src/game/combat.rs`** run the d20 roll, accumulate bonuses, branch on attribute/finesse/skill, perform the shield d20, roll armor, and apply resistance — all inline against ECS queries. There is no pure function that takes "attacker, defender, weapon" and returns "outcome."
- **Bonus sources are scattered.** `attack_attribute_bonus` lives in `src/character/mod.rs`. `weapon_skill_bonus` and `fighting_melee_bonus` live in `src/game/skills.rs`. `shield_check_passes` and the Enraged/Terrified multipliers live in `src/game/combat.rs`. Status-effect modifiers cross into `src/game/magic.rs`. Staff zap damage adds `floor(Evocations/4) + INT_mod` inside `src/game/staves.rs` independently.
- **Adding a damage type touches nine files.** Audit traced: engine `DamageType` enum, `combat.rs` hit-check + damage-roll branches, `character/mod.rs` attribute branching, `skills.rs` skill branching, `magic.rs` status registry + display, `staves.rs` staff effect dispatch, `abilities.rs` on-hit handlers, `items.rs` or RON assets for weapon tagging.
- **No pure tests exist for combat math.** "Elf Mage (DEX +2) with +2 Long Sword vs Dwarf with leather and a kite shield" requires building a Bevy `App`, spawning entities with the right component soup, writing `AttackIntentMessage`, and reading `DamageEvent`. The math is correct — but it is not verifiable in isolation, and it will become more so as Skills, Mana, and per-monster attributes land.
- **Cleave (axe ability) already breaks the pattern.** It re-rolls or shares damage across eight tiles via ad-hoc code in the ability handler. AoE staff zaps and future Sweep / Chain Lightning will repeat this hack.

This is the system AIs touch most often in a content-driven roguelike. Every time bonus rules shift (Skills phase, Mana phase, future weapon mastery), the cost compounds.

## Proposed Interface

A new module **`src/game/combat/resolve.rs`** owns all combat math. No Bevy imports, no ECS queries, no globals — pure functions over plain-data snapshots and an injected RNG. The Bevy systems in `src/game/combat.rs` shrink to thin adapters that gather snapshots, call the resolver, and write events.

```rust
//! Pure attack resolver. No Bevy, no ECS. Seeded RNG in, outcome out.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AttackSource { Melee, Ranged, Staff, Ability }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DamageType { Physical, Poison, Fire, Lightning }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HitResult { Miss, Hit, Crit }

/// Frozen view of the attacker. NO `Default` impl — use the ctor.
#[derive(Clone, Debug)]
pub struct AttackerSnapshot {
    pub hit_bonus: i32,
    pub damage_bonus: i32,
    pub attrs: Attrs,            // STR/DEX/INT
    pub skills: SkillView,       // Fighting, weapon skill, Evocations
    pub enraged: bool,
    pub terrified: bool,
    pub unaware_target: bool,    // backstab gate
}

#[derive(Clone, Debug)]
pub struct DefenderSnapshot {
    pub dodge: i32,
    pub armor_max: i32,
    pub armor_skill: i32,
    pub shields_skill: i32,
    pub shield: ShieldKind,
    pub shield_budget_left: u8,  // decremented on a successful block
    pub resistances: Resistances,
}

#[derive(Clone, Debug)]
pub struct WeaponSnapshot {
    pub dice: DiceExpr,
    pub damage_type: DamageType,
    pub finesse: bool,
    pub weapon_skill: Option<WeaponSkill>,
}

/// Non-melee tweaks. Explicit struct literal — no nested `Default::default()`.
#[derive(Clone, Debug, Default)]
pub struct AttackOverrides {
    pub damage_type: Option<DamageType>,  // staff zap → Fire/Lightning/...
    pub auto_hit: bool,                   // staves never roll-to-hit
    pub crit_disabled: bool,              // flat abilities
    pub bypass_shield: bool,              // gas, environmental
}

#[derive(Clone, Debug)]
pub struct AttackOutcome {
    pub result: HitResult,
    pub blocked: bool,
    pub final_damage: i32,
    pub damage_type: DamageType,
    pub use_counters: UseCounterBumps,    // adapter applies to the player
}

/// Roll one damage packet — for AoE / Cleave / Sweep, where one roll feeds many tiles.
#[derive(Clone, Debug)]
pub struct DamagePacket {
    pub amount: i32,
    pub damage_type: DamageType,
    pub crit: bool,
}

#[derive(Clone, Debug)]
pub struct AppliedOutcome {
    pub blocked: bool,
    pub final_damage: i32,
    pub use_counters: UseCounterBumps,
}

// ============ Entry points ============

/// One full attack: hit check → damage roll → defenses → outcome.
pub fn resolve_attack(
    src: AttackSource,
    atk: &AttackerSnapshot,
    def: &mut DefenderSnapshot,
    weapon: &WeaponSnapshot,
    overrides: AttackOverrides,
    rng: &mut RandomNumberGenerator,
) -> AttackOutcome;

/// The dominant case. Four args, no source enum at the call site.
#[inline]
pub fn resolve_melee(
    atk: &AttackerSnapshot,
    def: &mut DefenderSnapshot,
    weapon: &WeaponSnapshot,
    rng: &mut RandomNumberGenerator,
) -> AttackOutcome;

/// Roll damage in isolation — for sharing one roll across many targets.
pub fn roll_damage(
    atk: &AttackerSnapshot,
    weapon: &WeaponSnapshot,
    crit: bool,
    rng: &mut RandomNumberGenerator,
) -> DamagePacket;

/// Apply a pre-rolled packet against one defender's full defense pipeline.
pub fn apply_damage(
    packet: DamagePacket,
    def: &mut DefenderSnapshot,
    rng: &mut RandomNumberGenerator,
) -> AppliedOutcome;
```

### Usage at the Bevy adapter

```rust
// Melee — one line in the loop body.
let out = resolve_melee(&atk_snap, &mut def_snap, &weapon_snap, &mut rng);

// Ranged — same call shape, just specify the source.
let out = resolve_attack(
    AttackSource::Ranged, &atk_snap, &mut def_snap, &bow_snap,
    AttackOverrides::default(), &mut rng,
);

// Staff zap — auto-hit, override damage type, no crit.
let out = resolve_attack(
    AttackSource::Staff, &atk_snap, &mut def_snap, &staff_snap,
    AttackOverrides {
        damage_type: Some(DamageType::Fire),
        auto_hit: true,
        crit_disabled: true,
        ..Default::default()
    },
    &mut rng,
);

// Cleave — one to-hit, one damage roll, apply to 8 tiles.
let primary = resolve_attack(/* axe attack on primary target */);
if matches!(primary.result, HitResult::Hit | HitResult::Crit) {
    let splash = DamagePacket {
        amount: primary.final_damage,
        damage_type: weapon_snap.damage_type,
        crit: primary.result == HitResult::Crit,
    };
    for tile in eight_around(attacker_pos) {
        if let Some(mut def) = snapshot_defender_at(world, tile) {
            apply_damage(splash.clone(), &mut def, &mut rng);
        }
    }
}
```

### What the resolver hides

- d20 hit math, nat-1 / nat-20 branching
- attribute bonus dispatch on `(source, weapon.finesse, weapon.weapon_skill)` — the rule today scattered across `character::attack_attribute_bonus` and weapon-skill tag lookups
- weapon-skill + Fighting bonus aggregation
- shield d20 + budget gating + the rule that shield blocks beat every damage type
- armor random roll + the Physical-only gate
- resistance percentage
- Enraged ×1.5 / Terrified ×0.5 / crit ×2 multiplier stack and their commutativity rules
- which use-counters bump on hit / miss / block (Fighting, weapon skill, Dodging, Shields, Armor)
- staff zap's `floor(Evocations/4) + INT_mod` damage bonus — folded into the snapshot, applied uniformly

### What stays in the Bevy adapter (`src/game/combat.rs`)

- Reading ECS components and building the three snapshots
- Decrementing the actual `ShieldBlocksUsed` component after a block
- Bumping the actual `SkillUseCounters` resource from `out.use_counters`
- Emitting `DamageEvent`, `OnHitTriggerMessage`, `OnBeingHitTriggerMessage` based on the outcome
- Writing game log entries from the outcome fields
- Spawning damage-number particles

### Design choices and rejected alternatives

| Decision | Kept | Rejected | Why |
|---|---|---|---|
| Damage types | concrete `enum` | open `DamageType(u16)` newtype | exhaustive `match` finds every callsite when a new type lands |
| Snapshot defaults | explicit ctor / `new()` | `#[derive(Default)]` on snapshots | `Default` masks the "forgot to copy `attrs`" wiring bug |
| Defense layers | shield → armor → resistance pipeline inside `apply_damage` | `trait DefenseLayer` + `Box<dyn>` list per defender | parry / magic ward each land as one new field + one new branch when they ship — not a trait architecture maintained forever |
| Observability | adapter reads `AttackOutcome` fields | `trait ResolveObserver` hooks per phase | adapter already has the data; add hooks when a real consumer exists |
| Damage primitive split | `resolve_attack` + `roll_damage` + `apply_damage` | single `resolve_attack` | Cleave + AoE staff already exist and re-implement this; no future feature gating |
| Ergonomic wrapper | `resolve_melee(atk, def, weapon, rng)` | force all callers through the 6-arg form | 95% of attacks are melee; the wrapper is a 1-line inline |
| Mutation of defender | `&mut DefenderSnapshot` for shield budget | immutable + separate `ShieldState` return | inflates the melee call site for no real safety win — adapter writes back unconditionally |

## Dependency Strategy

**Category: in-process.** The resolver consumes plain data and an RNG handle. The only "dependency" is randomness, and the resolver takes it as a parameter (`&mut RandomNumberGenerator`).

- Production wires the global `bracket_lib::RandomNumberGenerator` through.
- Tests pass a seeded RNG and assert exact outcomes.

No port-and-adapter pattern needed. The Bevy adapter is the production caller; tests are the test caller. Both pass an RNG, both build snapshots, both consume `AttackOutcome`.

## Testing Strategy

### New boundary tests (in `src/game/combat/resolve.rs` `#[cfg(test)]`)

- **Hit math:**
  - Bare baseline (zero bonuses on both sides) → `d20 >= 4 + dodge`
  - Nat 1 always misses; Nat 20 always hits and crits
  - Attribute bonus dispatch: STR for plain melee, DEX for ranged, DEX for finesse weapon skills (Short/Long Blade), STR for axes regardless of finesse flag
  - Weapon skill bonus: `floor(skill/4)` applied for the weapon's `weapon_skill` tag; `None` weapons (fists, staff bash) get no skill bonus
  - Fighting bonus: melee only, never on ranged or staff
- **Damage math:**
  - Dice + bonuses → final pre-defense value
  - Crit ×2 stacks multiplicatively with Enraged ×1.5 and Terrified ×0.5
  - Damage bonus from `damage_bonus` field applies once regardless of source
- **Shield block:**
  - Block beats Physical, Poison, Fire, Lightning equally
  - Budget gating: a Buckler blocks at most once before `shield_budget_left == 0` shorts the check
  - `bypass_shield` override skips the check entirely
- **Armor:**
  - Roll range `0..=armor_max + floor(armor_skill/4)`
  - Applies to Physical only; Poison/Fire/Lightning skip armor unchanged
- **Resistance:**
  - 100% resistance → final damage zero
  - 50% resistance → final damage halved (round semantics tested explicitly)
- **Use-counter bumps:**
  - Fighting + weapon skill bump on melee hit
  - Dodging bumps on miss
  - Shields bumps on successful block only
  - Armor bumps when armor roll is non-zero
- **`roll_damage` + `apply_damage` symmetry:** a packet rolled with `crit=true` and applied to a defender produces the same `final_damage` as a full `resolve_attack` that crit on the same RNG sequence
- **`AttackOverrides`:** `auto_hit` skips the hit check, `damage_type` override replaces the weapon's type for resistance/armor purposes, `crit_disabled` prevents nat-20 from crit-multiplying damage

### Tests to delete

Currently there are no isolated unit tests for `hit_check_system` or `damage_roll_system` — the test surface is entirely empty. Nothing to delete; this is greenfield coverage.

If `src/game/skills.rs` has tests for `weapon_skill_bonus` or `fighting_melee_bonus` that operate on the helpers directly, leave them — the resolver consumes the same numbers and they remain useful as helper-level tests.

### Test environment needs

None beyond `cargo test`. The resolver is pure Rust; no Bevy `App`, no ECS world, no fixtures.

## Implementation Recommendations

### Module ownership

The resolver owns:
- The d20 + bonus aggregation rule
- The damage formula and multiplier stack
- The defensive pipeline (shield → armor → resistance) and its damage-type gates
- The use-counter bump policy
- Crit and override semantics

The resolver does NOT own:
- Component layout, ECS queries, message types
- Status-effect application (on-hit Burning, Poisoned). Adapter reads `AttackOutcome` and emits the existing `OnHitTriggerMessage` so ability handlers still own status application.
- Game log messages, particles, sound
- Target selection, LOS, range checks (ranged/staff still validate before invoking the resolver)

### Interface contract

- **Snapshots are read-mostly value objects.** `DefenderSnapshot` is the only mutable input, and only `shield_budget_left` may change. The adapter writes that delta back to the ECS component.
- **No interior mutability.** The RNG is the only mutating dependency, passed by `&mut`.
- **All damage types route through the same pipeline.** A new damage type adds one enum variant and at most one branch inside `apply_damage` (e.g. for an armor-bypass rule). Hit math never branches on damage type.
- **All sources route through the same hit math.** A new `AttackSource` adds one enum variant; whether it triggers Fighting / weapon-skill bonuses is decided inside the bonus aggregation helper.

### Migration path

1. Land `src/game/combat/resolve.rs` with full tests, **no callers yet**. The module compiles alongside today's code.
2. Convert `hit_check_system` + `damage_roll_system` to thin adapters that build snapshots and delegate. Existing behavior tests (if any) and playtest parity confirm no regression.
3. Convert the axe Cleave handler to use `roll_damage` + `apply_damage`. Delete the inline math in the ability handler.
4. Convert `handle_zap_staff` in `src/game/staves.rs` to call `resolve_attack` with `AttackSource::Staff` + overrides. The INT and Evocations bonuses move into snapshot construction.
5. Convert ranged attack handling in `src/game/ranged.rs` to use `AttackSource::Ranged`.
6. Audit `src/character/mod.rs` and `src/game/skills.rs` for bonus helpers that are now only called from snapshot construction; consider moving them into the adapter module.

### Caller migration policy

The new interface is additive. Callers migrate one source at a time (melee → ranged → staff → ability). At each step, the resolver and the legacy path coexist for that source. No "big bang" cutover.

### Naming

- Module: `src/game/combat/resolve.rs`. Keeps `src/game/combat.rs` as the Bevy adapter (renamed to `src/game/combat/adapter.rs` once `mod.rs` exists). Pre-existing imports `use crate::game::combat::*` continue to work via re-export.
- Type prefixes: `Attacker*`, `Defender*`, `Weapon*` for snapshots. `AttackOutcome` / `AppliedOutcome` for results. `AttackOverrides` for the non-melee tweak struct.

### Out of scope for this RFC

- Status effect application architecture (separate deepening candidate)
- The `magic.rs` / `effects.rs` naming overlap (separate cleanup)
- AoE pattern generators (`eight_around`, ray, cone) — those live in geometry, not combat
- Mana / spell-casting refactor (Phase 4)
- Per-monster attribute parity (deferred per current Phase 2 → 3 plan)
