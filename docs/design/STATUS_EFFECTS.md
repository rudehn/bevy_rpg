# Status Effects

The status effect layer is the game's turn-based bookkeeping for damage-over-time, buffs, and debuffs. Burning fire, lingering poison, the temporary haste of a potion, the brief stun after a shield bash — all of them flow through one component (`StatusEffects`), one entry point (`add_effect`), and one tick system that runs in the Cleanup phase of every turn.

The damage system stays ignorant of *why* an entity is on fire; combat handlers stay ignorant of *how long* a stun lasts. Attackers write intent ("apply Burning, 3 turns, 2 dmg/turn"), and the magic layer handles the rest.

---

## Design Philosophy

- **Intent in, bookkeeping out.** Combat and ability handlers never decrement durations or roll DoT damage themselves. They call `effects.add_effect(kind, turns)` and walk away. The engine's `status_effect_tick_system` does all per-turn arithmetic.
- **Decoupled from damage.** DoT statuses (Burning, Poisoned) emit standard `DamageEvent`s. They flow through the *same* damage pipeline as a sword swing — armor (where applicable), resistances, and `is_burning()` guards on fire tiles all just work.
- **Symmetric.** Player and monsters use the same `StatusEffects` component, the same kinds, and the same tick system. A player drinking a haste potion and a goblin warchief casting War Cry land in the same code path.
- **Extensible without forking.** The engine ships a blessed set; the game adds its own statuses through `StatusEffectKind::Custom { id }` with stable u32 IDs (see `src/game/magic.rs:31-34`).

---

## Data Model

### `StatusEffects` component

A per-entity component holding a `Vec<StatusEffectInstance>`. Attached to the player, every monster, and any other entity that can be buffed or debuffed. Props and decorations omit it — see Edge Cases.

```rust
// roguelike_engine/src/status/mod.rs
pub struct StatusEffects {
    pub effects: Vec<StatusEffectInstance>,
}

pub struct StatusEffectInstance {
    pub kind: StatusEffectKind,
    pub remaining_turns: u32,
    pub magnitude: i32,        // DoT damage / 0 for binary effects
    #[serde(skip)]
    pub source: Option<Entity>, // Kill credit; not save-stable
}
```

### `StatusEffectKind` variants

`#[non_exhaustive]` so the game pattern-matches with a `_` arm. Engine ships seven blessed variants; the game adds the rest as `Custom { id }`.

| Kind | Source | Behavior | Magnitude |
|------|--------|----------|-----------|
| `Burning` | Engine | DoT, fire damage each tick | dmg/turn |
| `Poisoned` | Engine | DoT, poison damage each tick | dmg/turn |
| `Stunned` | Engine | Speed × 100 (effectively skips turn) | unused |
| `Hasted` | Engine | Speed × 0.5 | unused |
| `Slowed` | Engine | Speed × 1.5 | unused |
| `Strengthened` | Engine | Damage × 1.5 | unused |
| `Weakened` | Engine | Damage × 0.75 | unused |
| `Custom { id: STATUS_ENTANGLED }` | Game (id=1) | Cannot move; cobweb decoration cleanup on expiry | unused |
| `Custom { id: STATUS_ENRAGED }` | Game (id=2) | +50% damage (separate from Strengthened) | unused |
| `Custom { id: STATUS_FIRE_RESISTANCE }` | Game (id=3) | Immune to fire damage / Steam gas | unused |
| `Custom { id: STATUS_POISON_RESISTANCE }` | Game (id=4) | Immune to poison / Poison gas | unused |

The four custom-id constants live in `src/game/magic.rs:31-34` and **must remain stable for save-file compatibility**. Adding a new custom status means picking the next free integer and registering display metadata via `StatusEffectRegistry`.

---

## Application: One Entry Point

All status application goes through the `GameStatusEffectsExt` trait (`src/game/magic.rs:153-180`):

```rust
// Binary status (no DoT magnitude needed)
effects.add_effect(StatusEffectKind::Stunned, 3);

// DoT status with damage-per-turn
effects.add_effect_with_magnitude(
    StatusEffectKind::Burning,
    duration,        // turns
    damage_per_turn, // magnitude
    Some(attacker),  // for kill credit on DoT death
);
```

These wrap the engine's `StatusEffects::add(StatusEffectInstance)` (`roguelike_engine/src/status/mod.rs:106`).

### Application sites

- **Burning Strike** — `handle_burning_strike` (`src/game/abilities.rs:221`) — chance on melee hit, applies `Burning`.
- **Poison Strike** — `handle_poison_strike` (`src/game/abilities.rs:248`) — chance on melee hit, applies `Poisoned`.
- **Stunning Blow** — `handle_stunning_blow` (`src/game/abilities.rs:275`) — chance on melee hit, applies `Stunned`.
- **Slow Strike** — `handle_slow_strike` (`src/game/abilities.rs:378`) — chance on melee hit, applies `Slowed`.
- **Enrage (passive)** — `handle_enrage` (`src/game/abilities.rs:493`) — when HP drops below threshold, applies `Custom { STATUS_ENRAGED }` for 99 turns (effectively for the rest of combat).
- **War Cry** — `handle_war_cry` (`src/game/abilities.rs:699`) — first attack triggers it; applies Enraged to nearby allies of the same faction.
- **Cobweb tile entry** — `src/game/actions.rs:644` — entering a cobweb decoration applies `Custom { STATUS_ENTANGLED }`.
- **Fire tiles** — `src/game/fire.rs:154` — standing in fire applies `Burning` if not already burning.
- **Gas exposure** — `src/game/gas.rs` — Poison gas applies `Poisoned`, Steam applies `Burning` (immunity check first).
- **Potions** — `src/game/effects.rs` — `ApplyHaste`, `ApplyFireResistance`, `Antidote` all funnel through `add_effect`.

### Re-application policy

**One slot per kind.** `StatusEffects::add` finds an existing instance of the same kind and merges:

- `remaining_turns = max(existing, new)` — duration **refreshes to the longer**
- `magnitude = max(existing, new)` — DoT damage **takes the higher**
- `source` updates to the latest applicator

This means stacking the same DoT does **not** double the damage — it locks in the strongest tick rate for the longest duration. Distinct `Custom` IDs *do* stack independently because they compare unequal.

Resolved decisions:

- **No diminishing returns.** A second stun applied during an existing stun simply refreshes the duration.
- **No magnitude addition.** Two Burning applications take `max`, never sum, to keep DoT pressure bounded.

---

## Tick System

The engine's `status_effect_tick_system` (`roguelike_engine/src/status/mod.rs:260`) runs once per turn inside `ProcessingPhase::Cleanup`, after `resolve_turn_end` and before `status_expiry_log_system`. Wiring is in `src/game/turns.rs:193-199`.

For every entity with `StatusEffects`, in this order:

1. **Apply DoT damage** for `Burning` and `Poisoned`. Writes a `DamageEvent` carrying the effect's magnitude, the right `DamageType` (`Fire` / `Poison`), and `attacker = effect.source` for kill credit.
2. **Decrement** every effect's `remaining_turns` by 1.
3. **Remove expired** effects (those that hit zero) and emit `StatusExpiredEvent` for each.

The game's `status_expiry_log_system` (`src/game/magic.rs:295`) reads those events, writes log lines ("X is no longer burning"), and — for `STATUS_ENTANGLED` specifically — clears the cobweb decoration that entangled the entity.

### Speed modifier

`apply_speed_effects_system` (`src/game/magic.rs:345`) recomputes movement and attack delays each frame:

```rust
multiplier = compute_speed_modifier(effects).clamp(0.5, 2.0);
speed.movement_delay = speed.base_movement_delay * multiplier;
speed.attack_delay   = speed.base_attack_delay   * multiplier;
```

`compute_speed_modifier` multiplies: `Hasted = 0.5×`, `Slowed = 1.5×`, `Stunned = 100×`. Hasted + Slowed = `0.75×`. The clamp prevents pathological stacks but keeps Stunned's effective skip-turn behavior since it's checked separately in `turns.rs:515-517` before the actor even runs its brain.

`base_*` fields preserve innate speed so the buff layer is purely additive — when status drops, speed snaps back without drift.

---

## Damage Interaction

DoT statuses emit standard `DamageEvent`s, so resistances, armor, and combat invariants apply uniformly:

- **Burning** → `DamageType::Fire`, `armor: 0`. Goes through `Resistances.fire`.
- **Poisoned** → `DamageType::Poison`, `armor: 0`. Goes through `Resistances.poison`. (Poison-immune entities — e.g., undead — short-circuit damage; see `src/game/combat.rs:126`.)

Both are tagged `DamageSource::Environment` so on-hit reactive abilities (RoughBody, Cleave splash, etc.) don't trigger from a DoT tick. `attacker = effect.source` carries the kill credit so a Burning kill counts for the entity that lit the fire.

### Damage-modifier statuses

`Strengthened` (+50%) and the game's `STATUS_ENRAGED` (also +50%) both ride the damage modifier path. `is_enraged()` is read explicitly in `src/game/combat.rs:245` via `apply_damage_multipliers(rolled, is_enraged, is_terrified)`. They are **separate buffs** — Strengthened is a generic engine kind, Enraged is the game's specific narrative buff with on-hit triggers (e.g., HP-threshold autocast, War Cry, faction-broadcast).

---

## Resistance: Status vs Permanent

There are **two parallel resistance systems**:

| | Permanent (`Resistances` component) | Timed (status effect) |
|---|---|---|
| Source | Equipped amulets, monster archetype defaults | Antidote potion, Fire Resistance potion |
| Component | `Resistances { fire, poison, lightning }` (percent reduction) | `StatusEffects` containing `Custom { STATUS_FIRE_RESISTANCE }` etc. |
| Behavior | Multiplicative damage reduction | **Total immunity** while active |
| Duration | While equipped | Counts down each turn |
| Save shape | Component on player entity | Inside `StatusEffects.effects` |

Damage handlers consult both: a Fire Resistance potion grants total fire immunity for the duration; an Amulet of Fire Resistance applies a percentage reduction always. They stack (the percentage reduction simply doesn't matter while immunity is active).

Gas exposure (`src/game/gas.rs:52-57`) uses the timed status check directly — `is_immune()` returns true for `is_fire_resistant()` against Steam, `is_poison_resistant()` against Poison. The permanent resistance does **not** confer gas immunity by itself; gas immunity is a deliberate potion-only counter.

---

## UI

Status badges are shown in the player HUD as a row of colored chips above the health bar. Each chip uses the kind's color from `kind_color()` and shows turns remaining. Hovering a chip shows the description from `kind_description()` (e.g., "3 fire dmg/turn, 5 turns").

The collection helpers live in `src/ui/mod.rs:239-249` (`collect_status_effects` and `collect_status_effects_with_duration`). Monsters in the inspect overlay show the same chip row. Custom statuses look up display metadata via `kind_metadata_with(kind, ..., Some(&registry))`, falling back to the built-in formatters when no registry entry exists.

---

## Edge Cases & Resolved Decisions

- **Entities without `StatusEffects`.** Some props (chests, decorative torches, doors) lack the component. `status_query.get_mut(target)` returns `Err`, and ability handlers gracefully skip the status application. They still take the underlying damage from the triggering attack.
- **Save / load.** `StatusEffects` is `Serialize + Deserialize`. The full effect list (kinds, durations, magnitudes) round-trips on save (`src/save/mod.rs`). `source: Option<Entity>` is `#[serde(skip)]` because Entity IDs are not stable across loads — DoT damage after load attributes kills to "no source," which is acceptable.
- **Floor transitions.** Status effects **persist across floors**. Walking down stairs while burning means you arrive on the next floor still burning, with the same remaining turns. This is intentional: the player can choose to descend with a fresh haste potion still active, but also pays the cost of descending while poisoned.
- **Stunned on your own turn.** The turn dispatch in `src/game/turns.rs:515-517` checks `effects.is_stunned()` *and* `effects.is_entangled()` before running the brain. Stunned actors finish their turn immediately and the `× 100` speed multiplier ensures they wait a long time before being scheduled again — they do not waste cycles spinning.
- **Bloat detonation overlap with poison gas.** When a Pit Bloat detonates via `ExplodeOnHit::GasCloud`, it strips its own `GasOnDeath` component first (`src/game/abilities.rs:431-433`) so the imminent self-death does not spawn gas a second time. The resulting cloud applies `Poisoned` to anyone walking through it via the standard gas mechanism — no special case in the status system.
- **No permanent statuses.** Every status decays. Long-lived buffs like Enrage use a 99-turn duration, not infinity, to keep the data model uniform.
- **No diminishing returns.** Reapplying the same kind refreshes; it does not stack magnitude or shorten duration on subsequent applications.
- **Symmetric for player and monsters.** Identical code path. A monster drinking a haste potion (none exist yet, but the system would support it) would behave exactly like the player.

---

## Cross-Links

- [GAME.md](GAME.md) — damage pipeline, damage types, `Resistances` component (permanent counterpart).
- ABILITIES.md *(planned)* — full catalog of monster and weapon abilities; documents the trigger conditions for each status application listed above.
- FIRE.md *(planned)* — fire tile spread, ignition, Burning as the on-tile effect.
- GAS.md *(planned)* — gas spread, concentration thresholds, Poisoned and Burning as on-step effects, immunity gating.
- [ITEMS.md](ITEMS.md) — potions that apply timed `Hasted`, `STATUS_FIRE_RESISTANCE`, and `STATUS_POISON_RESISTANCE`.
