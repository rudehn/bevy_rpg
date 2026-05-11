# Ranged Combat

Ranged combat is a separate intent path that shares the same hit and damage
formula as melee, gated by line-of-sight, weapon range, and ammunition.
The player fires bows at visible enemies; certain monsters fire back.

## Design Philosophy

Ranged combat exists to give both the player and the AI a meaningful
"engage at distance" option without inventing a second damage system.

- **Symmetric pipeline.** A ranged intent ends in the same
  `AttackIntentMessage` -> `hit_check_system` -> `DamageRollMessage`
  pipeline as melee (`combat.rs:171`). The only structural difference is
  `DamageSource::Ranged`, which lets some downstream rules (Backstab,
  Cleave splash) opt out while keeping on-hit triggers active.
- **Bounded by sight.** A target must be in the attacker's
  `Viewshed::visible_tiles`. No through-wall trick shots
  (`ranged.rs:96`).
- **Bounded by range.** Pythagorean tile distance must be `<= range`,
  where range comes from `weapon_range` (player) or `RangedCapable.range`
  (monsters) (`ranged.rs:108-143`).
- **Bounded by ammo.** The player consumes one Arrow per shot
  (`ranged.rs:146-172`). Monsters do not consume ammo (deliberate — they
  use cooldowns or AI gating instead).
- **No projectile entities.** A ranged shot resolves instantly inside the
  same Processing phase. The arrow particle is cosmetic only
  (`ranged.rs:180`); nothing is in flight gameplay-wise.

## Data Model

### Messages

```rust
// src/game/actions.rs:87
#[derive(Message)]
pub struct RangedAttackIntent {
    pub attacker: Entity,
    pub target: Entity,
}
```

### Components

```rust
// src/game/ranged.rs:31
#[derive(Component, Clone)]
pub struct RangedCapable {
    pub range: u32,
}
```

`RangedCapable` is attached at spawn time when a monster's
`AiConfig::Fsm { ranged_range, .. }` is non-zero or its
`AiConfig::Goap { traits, .. }` includes `AiTrait::Ranged { range }`
(`spawner.rs:193-205`).

### Items (assets/items.ron)

| Item  | Kind       | Damage | weapon_range | Notes                          |
|-------|------------|--------|--------------|--------------------------------|
| Bow   | Weapon     | 1d4    | 8            | Only weapon with weapon_range > 0 |
| Arrow | Consumable | -      | -            | `is_ammo: true`, `max_stack: 30` |

`weapon_range` defaults to `0` — only the Bow has a non-zero value.

## Player Flow

The player fires with **F**:

1. Player presses **F** during `TurnState::PlayerInput`
   (`turns.rs:560`).
2. `TargetingContext.mode` is set to `TargetingMode::RangedAttack` and
   the state machine transitions to `InGameState::Targeting`. **No
   pending action is set yet** — targeting must complete first.
3. `setup_targeting` snaps the cursor to the nearest visible monster
   (`targeting.rs:130-137`). Cursor is yellow.
4. Arrow keys / WASD move the cursor freely on the map. The cursor is
   *not* clamped to weapon range at the input layer — range is enforced
   at the intent handler.
5. **Enter** or **Space** confirms. If a monster occupies the cursor
   tile, `Action::RangedAttack { target }` is queued and the state
   transitions to `Processing` (`targeting.rs:306-319`).
6. **Esc** cancels back to `InGameState::Running`. No turn cost.
7. `dispatch_player_action` translates the queued `Action::RangedAttack`
   into a `RangedAttackIntent` (`actions.rs:241`).
8. `handle_ranged_attack` validates LOS, range, and ammo, then either
   emits an `AttackIntentMessage` or a free-action / failure message.

## Monster Flow

Monsters with `RangedCapable` (or the GOAP `AiTrait::Ranged`) prefer
ranged attacks at distance:

- **FSM AI** (`ai.rs:623-641`): `try_ranged_attack` fires when the
  player is more than 1.5 tiles away (i.e. not adjacent) but within
  `range`. Adjacent enemies always melee.
- **GOAP AI** (`goap/dispatch.rs:337-348`): the `ranged_attack` action
  writes a `RangedAttackIntent` directly. The behavior layer
  (`goap/behaviors.rs:29`) reads the same `AiTrait::Ranged { range }`
  to determine viability.

Monsters always pay full `BASE_ACTION_COST` for a shot — no free
re-tries on misses, no "lose a turn" on missing (unlike the player).

## Validation Pipeline (`handle_ranged_attack`)

The handler runs in `ProcessingPhase::ResolveActions` and applies these
checks in order:

1. **Attacker still exists** — drop the intent, finish turn at base
   cost (`ranged.rs:66-71`).
2. **Target still exists** — player gets a free action; monsters pay
   the turn cost (`ranged.rs:73-80`).
3. **Target not submerged** — submerged entities (in deep water) cannot
   be targeted (`ranged.rs:83-91`).
4. **Line of sight** — `viewshed.visible_tiles.contains(target_point)`.
   No clear LOS -> log message, free action (player) or paid turn
   (monster) (`ranged.rs:96-106`).
5. **Range** — for monsters, `RangedCapable.range`; for the player,
   `Equipment.weapon`'s `ItemProperties.weapon_range`. If the player has
   no equipped weapon or the equipped weapon has `weapon_range == 0`,
   the shot fails as a free action with "You have no ranged weapon
   equipped." (`ranged.rs:108-130`).
6. **Distance check** — Pythagorean (`DistanceAlg::Pythagoras`),
   `dist <= range as f32`. Out of range -> log, free / paid
   (`ranged.rs:132-143`).
7. **Ammo (player only)** — find the first stackable `Ammo` item in the
   player inventory. None -> "You have no arrows!", free action.
   Otherwise decrement the stack by 1 (or despawn if the last arrow)
   (`ranged.rs:146-172`).
8. **Fire.** Log, spawn arrow particle, emit `AttackIntentMessage` with
   `DamageType::Physical` and `DamageSource::Ranged`. Pay base action
   cost (`ranged.rs:174-194`).

From here the shot enters the shared combat pipeline: hit check ->
damage roll -> resistance -> on-hit triggers.

## Hit and Damage

Identical to melee — see [GAME.md](GAME.md) "Combat System". The shot
is `DamageType::Physical`, so target armor and physical resistance both
apply. Critical hits work the same (natural 20 always hits and feeds
into the damage roll as a crit).

`DamageSource::Ranged` differs from `DamageSource::Melee` in:

- **No Backstab triple-damage** — Backstab checks
  `DamageSource::Melee` in `damage_roll_system`.
- **No Cleave splash** — Cleave's environmental splash is melee-only;
  the `attacker_is_player && message.source == DamageSource::Melee`
  guard skips it for ranged hits.
- **On-hit and on-being-hit triggers DO fire** — `combat_trigger_system`
  explicitly accepts both Melee and Ranged (`combat.rs:355`). So
  BurningStrike, PoisonStrike, and similar weapon runics on a Bow do
  trigger from arrow hits.

## Bow as a Melee Weapon

Equipping a Bow does **not** disable melee. Bumping into an adjacent
enemy still emits `MeleeIntent` and the Bow's listed damage (1d4) is
used for the melee swing. There is no "bare-fisted fallback" branch —
the bow swing is the fallback. This is intentional: the player isn't
locked out of close combat just because they switched to a bow, but the
1d4 damage discourages it.

## Edge Cases and Resolved Decisions

- **No friendly fire.** `TargetingMode::RangedAttack` confirm only
  finds monsters at the cursor (`targeting.rs:307`). Allies, summons,
  and the player are not selectable.
- **Out-of-ammo.** Player consumes a free action ("You have no
  arrows!"); turn does not end (`ranged.rs:158-162`). Monsters never
  hit this branch.
- **No bow equipped, F pressed.** Targeting still opens — the F key
  doesn't pre-validate. Confirm hits the `weapon_range == 0` branch and
  fails as a free action ("You have no ranged weapon equipped."). This
  is a known minor UX gap; could be tightened by gating F at the input
  layer.
- **Target moves out of LOS during targeting.** The cursor is not
  re-validated each tick — the target may walk behind a wall before
  the player confirms. The LOS check at intent time
  (`ranged.rs:96-106`) catches this and returns a free action.
- **Submerged targets.** Hidden by `Submerged` (deep water); ranged
  attacks against them log "The target is submerged and cannot be hit!"
  (`ranged.rs:83-91`).
- **Dead or despawned targets.** Caught by the `target_query.get(...)`
  failure branch (`ranged.rs:73-80`).
- **No charge-up time.** Firing is instant, costing one
  `BASE_ACTION_COST` (modulated by attack speed, same as a melee
  swing).
- **No projectile entity.** The arrow particle (`ParticleRequest::arrow`)
  is purely visual. Nothing can intercept, deflect, or block an arrow
  mid-flight — resolution is atomic.
- **Damage type.** Always `DamageType::Physical`. There is currently no
  way for a Bow to fire elemental arrows; that would be a runic / staff
  feature, not a ranged-system feature.

## Configuration Knobs

| Knob | Where | Notes |
|------|-------|-------|
| `weapon_range` | `assets/items.ron` per weapon | 0 disables ranged for that weapon |
| Arrow `max_stack` | `assets/items.ron` Arrow | Default 30 |
| `ranged_range` | `assets/monsters.ron` (FSM AI) | Per-monster, 0 disables |
| `AiTrait::Ranged { range }` | `assets/monsters.ron` (GOAP AI) | Per-monster |

## Cross-Links

- [GAME.md](GAME.md) — Hit check (d20 + hit_bonus vs 4 + dodge_bonus),
  damage rolls, resistance pipeline.
- [ITEMS.md](ITEMS.md) — Bow weapon entry, Arrow consumable entry.
- [TURNS.md](TURNS.md) — Ranged attacks pay `BASE_ACTION_COST` at
  `ActionKind::Attack` delay; failed shots emit `FreeActionEvent` for
  the player only.
- [ENEMIES.md](ENEMIES.md) — Monster `RangedCapable` and
  `AiTrait::Ranged` configuration; monster ability cooldowns are
  separate from ranged attacks.
- [CHASMS.md](CHASMS.md) — Targets across chasms are valid (chasms
  don't block FOV); arrows fly over them.
