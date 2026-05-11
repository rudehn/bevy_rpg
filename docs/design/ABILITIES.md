# Monster Abilities

Monsters in *The Veiled Tyrant* express identity through **abilities**,
not raw stat lines. Two mobs with identical HP and damage feel different
because one summons skeletons on a 12-turn cooldown and the other has
Pack Tactics. This doc covers the system end-to-end: data model, RON
schema, runtime triggers, and the AI plumbing that fires
ranged/utility abilities.

Per-monster ability assignments live in [`ENEMIES.md`](ENEMIES.md);
player active abilities (Riposte, Backstab, Cleave) in
[`ITEMS.md`](ITEMS.md).

## Design philosophy

- **Identity over stats.** Same HP, different feel.
- **No mana, no spell slots.** Every ability is either a passive trigger
  or a per-ability cooldown.
- **No global cooldown / GCD.** Each ability has its own
  `current_cooldown`; one firing does not lock out another.
- **Two families.** Trigger-based (on-hit / on-being-hit / on-death /
  passive) wired through the Bevy reaction pipeline; cooldown-based
  (Bolt / Heal / Summon / ApplyStatus / SelfBuff / SummonCapped)
  selected by the AI when their cooldown rolls to zero.
- **Data-driven.** Abilities live in the monster's RON via `abilities`
  and `monster_abilities`. The spawner attaches the right components.
- **Damage is canonical.** Abilities emit `DamageEvent` rather than
  mutating HP, so armor → resistance → HP runs in one place.

## The two ability families

| Family   | Trigger model                | RON field           | Storage                         |
|----------|-------------------------------|---------------------|---------------------------------|
| Trigger  | Component → reaction handler  | `abilities: [..]`   | One ECS component per ability   |
| Cooldown | Per-turn tick + AI selection  | `monster_abilities` | `MonsterAbilities(Vec<entry>)`  |

A monster can have any mix. The Goblin Conjurer carries `WarCry`
(trigger) plus `Summon` and `Heal` (cooldown).

---

## Trigger family

Each trigger ability is a Bevy `Component` attached at spawn time. A
matching handler in
[`src/game/abilities.rs`](../../src/game/abilities.rs) reads the
component when the relevant message arrives. All handlers run in
`CombatReactionSet`, which is scheduled `.after(CombatDamageSet)` (see
[`TURNS.md`](TURNS.md)).

### Trigger messages

`abilities.rs:28-45` defines the two combat-reaction messages:
`OnHitTriggerMessage { attacker, defender, final_damage, source }` and
`OnBeingHitTriggerMessage { attacker, defender, final_damage, source,
damage_type }`. `DeathEvent` (defined in `combat.rs`) is the
engine-canonical on-death signal. Most on-hit handlers short-circuit on
`source != DamageSource::Melee` — they only fire on melee strikes, not
stray spell or environment damage.

### On-hit triggers (attacker has the component)

Fired by `OnHitTriggerMessage` after the attacker lands a melee blow.

| Component       | RON variant       | Source line                | Effect |
|-----------------|-------------------|----------------------------|--------|
| `BurningStrike` | `BurningStrike`   | `abilities.rs:52, 205`     | `chance%` to apply Burning DoT (`damage_per_turn` × `duration`). |
| `PoisonStrike`  | `PoisonStrike`    | `abilities.rs:60, 232`     | `chance%` to apply Poisoned DoT. |
| `StunningBlow`  | `StunningBlow`    | `abilities.rs:68, 259`     | `chance%` to stun for `duration` turns. |
| `SlowStrike`    | `SlowStrike`      | `abilities.rs:88, 362`     | `chance%` to apply Slowed status. |
| `LifeDrain`     | `LifeDrain`       | `abilities.rs:75, 286`     | Heal attacker for `percent%` of damage dealt (min 1). |
| `Knockback`     | `Knockback`       | `abilities.rs:81, 311`     | `chance%` to push defender `distance` tiles along the attack vector. |
| `PackTactics`   | `PackTactics`     | `abilities.rs:165, 707`    | +50% damage when an allied monster is adjacent to the defender. Emits a bonus `DamageEvent`. |
| `WarCry`        | `WarCry`          | `abilities.rs:157, 675`    | Passive aura that auto-fires once on first hit; enrages allies in `radius` for `duration`. |
| `ExplodeOnHit`  | `ExplodeOnHit`    | `abilities.rs:140, 390`    | Detonate per `ExplodeEffect` and self-damage for 9999. See below. |

### On-being-hit triggers (defender has the component)

Fired by `OnBeingHitTriggerMessage`.

| Component    | RON variant   | Source line              | Effect |
|--------------|---------------|--------------------------|--------|
| `RoughBody`  | `RoughBody`   | `abilities.rs:96, 449`   | Reflect flat `damage` (Physical) back at the melee attacker. |
| `Enrage`     | `Enrage`      | `abilities.rs:101, 477`  | Apply `STATUS_ENRAGED` (custom status) once HP ≤ `threshold_percent%`. Idempotent — does nothing if already enraged. |
| `SplitOnHit` | `SplitOnHit`  | `abilities.rs:181, 804`  | Spawn a clone with half HP at an adjacent walkable tile. Suppressed if `damage_type == Fire` or current HP < `min_hp`. Status effects copy to the clone. |

### On-death triggers (dying entity has the component)

Fired by `DeathEvent`.

| Component         | RON variant         | Source line              | Effect |
|-------------------|---------------------|--------------------------|--------|
| `ExplodeOnDeath`  | `ExplodeOnDeath`    | `abilities.rs:108, 504`  | AoE damage in Manhattan `radius`. Damage type defaults to Fire (`default_fire_damage_type`). |
| `GasOnDeath`      | `GasOnDeath`        | `abilities.rs:121, 553`  | Spawn poison gas of `volume` in Manhattan `radius`. |
| `SummonOnDeath`   | `SummonOnDeath`     | `abilities.rs:149, 583`  | Spawn `count` copies of `monster_name` at adjacent walkable tiles. |
| `SummonedBy` link | (none — runtime)    | `abilities.rs:643`       | When a summoner dies, all monsters with `SummonedBy { summoner: <dead> }` despawn ("dissipates!"). Generic — works for any summoner kind. |

### Passive / aura triggers

Aura systems re-run each `TurnEndEvent`. They clear last turn's marker
component and re-apply it to entities currently in range.

| Component   | RON variant | Source line              | Effect |
|-------------|-------------|--------------------------|--------|
| `Rally`     | `Rally`     | `abilities.rs:168, 748`  | Each turn, applies `RallyBuff { armor_bonus }` to faction allies within `radius`. Combat reads the marker for armor calculation. |
| `Terrify`   | `Terrify`   | `abilities.rs:191, 777`  | Each turn, applies `Terrified` marker to enemies in `radius`. Combat reads the marker for the −25% damage debuff. |
| `MimicDisguise` | `MimicDisguise` | `abilities.rs:187, 873` | Disguised as a chest until the player is adjacent (`mimic_reveal_system` on `TurnEndEvent`). On reveal, the AI is alerted. |

`PackTactics` and `WarCry` straddle the trigger/passive line — they
live on attackers but fire from the on-hit message, so they only
matter in active combat. (Reflection on the **player** side is the
"armor runic" via the enchantment system, not `RoughBody`; see
[`ITEMS.md`](ITEMS.md).)

### `ExplodeEffect` enum

`abilities.rs:127-137` defines what an `ExplodeOnHit` does at detonation:

```rust
pub enum ExplodeEffect {
    CrackFloor,                       // Pit Bloat: turns radius into chasms
    GasCloud { volume: u16 },         // Bloat: spawns poison gas
}
```

`CrackFloor` mutates Manhattan-radius tiles to
`Decoration::CrackedFloor`, which the chasm system later promotes to
`LiquidType::Chasm` (see [`CHASMS.md`](CHASMS.md)). `GasCloud` calls
`gas::spawn_gas` over the radius. The enum defaults to `CrackFloor` for
backward compatibility with older RON entries
(`ExplodeEffectDef::default` in `assets/mod.rs:477`).

### RON syntax for trigger abilities

Triggers go in the `abilities:` array on a `MonsterAsset`. Variants and
their fields are defined in `assets/mod.rs:483-516`.

```ron
abilities: [
    PoisonStrike(damage_per_turn: 2, duration: 5, chance: 75),
    Knockback(distance: 2, chance: 50),
    Enrage(threshold_percent: 30),
    ExplodeOnHit(radius: 2, effect: GasCloud(volume: 500)),
    ExplodeOnDeath(damage: 8, radius: 2, damage_type: Some("fire")),
    SummonOnDeath(monster: "spectral_blade", count: 2),
    Rally(radius: 4, armor_bonus: 2),
    Terrify(radius: 3),
    PackTactics,
    MimicDisguise,
],
```

The spawner walks this list and inserts the corresponding ECS component
on the entity. Component fields mirror the RON struct field-for-field.

---

## Cooldown family

Cooldown abilities live on a single `MonsterAbilities` component
defined in [`src/game/staves.rs`](../../src/game/staves.rs) (sharing
the file with the player staff system because both reuse dice helpers
and the per-turn tick).

### Data model

`staves.rs:637-666`:

```rust
pub enum MonsterAbilityKind {
    Bolt { dice: String, damage_type: DamageType },     // ranged damage
    Heal { dice: String },                              // heal self / ally
    ApplyStatus { effect: StatusEffectKind, duration: u32 },
    SelfBuff   { effect: StatusEffectKind, duration: u32 },
    Summon       { monster: String, count: u32 },
    SummonCapped { weights: Vec<(String, u32)>, max_summons: u32 },
}

pub struct MonsterAbilityDef {
    pub kind: MonsterAbilityKind,
    pub cooldown: u32,           // base cooldown in turns
    pub current_cooldown: u32,   // turns until ready (0 = ready)
    pub range: u32,              // tiles
    pub name: String,            // logged on use
}

#[derive(Component, Default)]
pub struct MonsterAbilities(pub Vec<MonsterAbilityDef>);
```

### Cooldown ticking

`tick_monster_abilities_system` (`staves.rs:669`) runs on every
`TurnEndEvent` and decrements `current_cooldown` for each owning
monster. Cooldowns tick **per-monster**, not globally — two shamans on
the same floor have independent cooldowns despite a shared ability
list. When `current_cooldown == 0` the ability is "ready"; firing it
resets the counter to `cooldown`.

### AI selection

The FSM AI selects cooldown abilities in
[`src/game/ai.rs:435-619`](../../src/game/ai.rs) via `try_use_ability`,
which runs before melee/movement on a monster's turn:

1. Reads `MonsterAbilities`; finds the nearest hostile and the
   most-wounded faction ally via `FactionMatrix`.
2. Iterates abilities in declaration order, skipping any with
   `current_cooldown > 0`.
3. Per-kind logic:
   - **Bolt** — nearest enemy within `range`; rolls dice; emits
     `DamageEvent { source: Spell }`.
   - **Heal** — self if HP < 60%, else most-wounded ally.
   - **ApplyStatus** — applies `effect` to nearest hostile in range.
   - **SelfBuff** — skipped if the caster already has this status.
   - **Summon** — inserts a `PendingSummon` resource consumed by
     `magic.rs` next frame.
   - **SummonCapped** — counts active summons via
     `count_active_summons`; bails at `max_summons`. Picks from
     `weights` via `pick_weighted_monster`; tags the new summon with
     `SummonedBy { summoner: self }`.
4. Firing sets `current_cooldown = cooldown` and returns `true` —
   ability use replaces movement/melee for the turn.

The GOAP AI consults `MonsterAbilities` similarly
(`goap.rs:863-867`) but ranks via its planner instead of fixed order.

### RON syntax for cooldown abilities

```ron
monster_abilities: [
    (
        kind: Bolt(dice: "2d6", damage_type: Fire),
        cooldown: 4,
        current_cooldown: 0,
        range: 6,
        name: "Fire Bolt",
    ),
    (
        kind: Heal(dice: "1d8+2"),
        cooldown: 8,
        current_cooldown: 0,
        range: 4,
        name: "Lesser Heal",
    ),
    (
        kind: SummonCapped(
            weights: [("skeleton", 60), ("zombie", 40)],
            max_summons: 3,
        ),
        cooldown: 12,
        current_cooldown: 6, // not ready immediately
        range: 0,
        name: "Raise Dead",
    ),
],
```

`current_cooldown` lets a monster start "warm" (already partially or
fully on cooldown) so it can't open with its strongest ability on turn 1.

---

## Damage routing

Damage-dealing abilities emit `DamageEvent` rather than mutating
`Health`. The pipeline stays canonical:
`DamageEvent → armor → resistance → HP → DeathEvent → on-death
triggers`. On-death handlers chain naturally — a mob's
`ExplodeOnDeath` can kill a second mob with its own `ExplodeOnDeath`
and both fire correctly because both read the same `DeathEvent`
stream.

---

## Player-side parallel

Players do **not** carry `MonsterAbilities`. Their active abilities
live on weapons via the `weapon_ability: Option<String>` field on
`ItemAsset` (`assets/mod.rs:773`):

| Ability  | Weapon  | Trigger |
|----------|---------|---------|
| Backstab | Dagger  | Triple damage vs. sleeping target (resolved in `damage_roll_system`). |
| Cleave   | Axe     | After a melee hit, also damages every monster in the 8 tiles around the *attacker* for the rolled damage. Splash uses `DamageSource::Environment` to be recursion-safe. |

The Sword has no active ability — it's the balance baseline. Riposte
was removed in favor of keeping the baseline weapon clean.

These are evaluated in the combat system, not the abilities system —
see [`ITEMS.md`](ITEMS.md) for the full table. Staves (player ranged)
use the Brogue-style charges model (`Rechargeable`); they share
`staves.rs` with `MonsterAbilities` for historical reasons.

---

## Edge cases & resolved decisions

- **`ExplodeOnHit` GasCloud double-fire.** When a Bloat detonates with
  `effect: GasCloud`, the handler removes any `GasOnDeath` before
  queueing the 9999 self-damage (`abilities.rs:433`). Without this both
  the on-hit explosion **and** the on-death gas burst would fire and
  double-spawn the cloud.
- **Submerged targets immune to staves.** Staff zaps refund the charge
  on submerged targets (`staves.rs:352`); monster `Bolt` does not
  duplicate the check, but the AI range check naturally skips most
  edge cases. Water-walking monsters in deep water can't be hit by
  pathing-line abilities.
- **Summon caps.** `SummonCapped` walks `SummonedBy { summoner: self }`
  markers (`magic::count_active_summons`) and bails at `max_summons`.
  `handle_summoner_death` (`abilities.rs:643`) reaps tagged summons
  generically when their summoner dies.
- **`SplitOnHit` immune to fire.** `damage_type == Fire` skips the
  split entirely (`abilities.rs:820`); fire is the designed counter to
  slimes/oozes.
- **WarCry one-shot.** `WarCry.activated` is set to `true` on first
  fire and persists (component not removed) so save/load is stable.
- **Aura timing.** `Rally`/`Terrify` clear-and-reapply on
  `TurnEndEvent`, so removing the leader removes the buff at the next
  turn end — combat resolution this turn still sees the bonus.
- **Mimic reveal.** `MimicDisguise` is checked on `TurnEndEvent`, so
  the disguise drops at the end of the turn the player steps adjacent,
  not the instant they step in.
- **No mana, ever.** No `Mana` component, no `SpellSlots`, no caster
  level. The prior spell system was removed (see `assets/mod.rs:94`,
  `staves.rs:631`).
- **No global cooldown.** Every `MonsterAbilityDef` has its own
  counter; a shaman can summon and heal back-to-back if both are ready.

`AbilitiesPlugin` (`abilities.rs:1080`) registers every trigger handler
in `CombatReactionSet`. `StavesPlugin` (`staves.rs:688`) owns the
per-turn cooldown tick. The full ordering is
`PlayerAction → CombatDamageSet → CombatReactionSet → TurnEndEvent`.

---

## Cross-links

- [`STATUS_EFFECTS.md`](STATUS_EFFECTS.md) — Burning, Poisoned, Stunned,
  Slowed, Enraged, Terrified rules. *(Doc not yet present in
  `docs/design/`; flagged as a documentation gap.)*
- [`TURNS.md`](TURNS.md) — `TurnEndEvent` cadence and
  `CombatReactionSet` ordering relative to `CombatDamageSet`.
- [`ENEMIES.md`](ENEMIES.md) — per-monster ability assignments,
  factions, squad behaviors.
- [`ITEMS.md`](ITEMS.md) — player weapon abilities and staff effects.
- [`CHASMS.md`](CHASMS.md) — what `ExplodeEffect::CrackFloor` produces.
- [`SQUAD_AI.md`](SQUAD_AI.md) — squad-level use of `Rally`, `WarCry`,
  and how summons inherit a leader's `SquadId`.
