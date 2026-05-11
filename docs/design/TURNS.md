# Turn & Action System

The Veiled Tyrant runs on a **time-cost queue scheduler**. Every actor (player, monsters, the world tick marker) is scheduled at a `u32` game-time tick. The actor with the earliest scheduled time acts next, emits an `ActionFinishedEvent` carrying a base cost, and is reinserted at `current_time + round(base_cost * delay)`.

Nothing in the engine runs continuously. Frames advance only when an actor consumes time.

---

## Design Philosophy

A roguelike is a sequence of decisions, not a sequence of frames. The turn system needs three properties:

1. **Determinism** — given the same input, the same actors act in the same order. Replays, save/load, and AI testing all depend on this. A binary heap keyed on `(time, insertion_order)` gives us total ordering without race conditions.
2. **Variable speed** — a haste potion, a slow-strike status, a heavy weapon, and a rat all coexist in the same queue. The cost-multiplier model expresses any of these as a single `f32` delay applied to a fixed `BASE_ACTION_COST`.
3. **Decoupled action types** — movement, melee, staves, ranged, and item use share the same scheduling machinery. The queue does not know what an action *is*; only what it *cost*.

### Resolved decisions

| Considered | Chosen | Why |
|---|---|---|
| Real-time / hybrid | **No** — strictly turn-based | Brogue lineage. Player thinks first. |
| Initiative roll per round | **No** — time-cost queue | Initiative is bursty (everyone acts once per round); time-cost gives natural fast/slow interleaving. |
| ATB / continuous time | **No** — discrete `u32` ticks | Float drift breaks determinism over a 26-floor descent. |
| Tick-based (every entity polled per tick) | **No** — event-driven dequeue | Tick polling wastes work scaling with monster count; we only run the actor whose turn it is. |

---

## Data Model

### `TurnManager` resource

A binary-heap-backed queue keyed by `(scheduled_time, insertion_order, Entity)`. Lives in the `roguelike_engine` crate and is re-exported as `crate::game::turns::TurnManager` (see `src/game/turns.rs:36-39`).

| Field | Purpose |
|---|---|
| `current_time: u32` | Game-time tick of the most recently dequeued actor. Monotonically increasing. |
| Internal queue | Min-heap of scheduled actors. `peek_time()` returns the next tick. |
| `MAX_NPC_BATCH` | Cap on NPCs dequeued in a single `Processing` cycle (prevents one slow frame from chain-reacting). |

Operations: `add_entity` (schedule at `current_time`), `insert_at(entity, time)`, `remove_entity` (called by death/cleanup), `contains(entity)` (dedup guard during floor transitions).

### `TurnState` (FSM)

A Bevy `States` enum (`src/game/turns.rs:41-48`).

| State | Meaning | Exits to |
|---|---|---|
| `Waiting` | Default. The map is loading, or game is paused at boundary. | `NextTurn` (via `start_turns` on `OnEnter(AppState::InGame)`) |
| `NextTurn` | Pop the next actor(s) from the queue, tag with `MyTurn`. | `PlayerInput` if player is up, `Processing` if NPC batch, stays in `NextTurn` if empty |
| `PlayerInput` | Read keyboard. Player keystrokes set `PendingPlayerAction`. | `Processing` once a non-free action is chosen |
| `Processing` | Run the four-phase pipeline below until queue points back at the player. | `PlayerInput` or `NextTurn` (via `continue_turn_processing`) |

`PlayerInput` is also gated on `InGameState::Running` (`src/game/turns.rs:215-217`) — open inventories and targeting overlays freeze input.

### `SpeedStats` component

Per-entity speed multipliers (`src/game/actions.rs:102-139`).

```rust
SpeedStats {
    base_movement_delay: f32,   // set at spawn, never overwritten
    base_attack_delay:   f32,
    movement_delay: f32,        // = base * status_effect_multiplier
    attack_delay:   f32,        // recomputed each frame by apply_speed_effects_system
}
```

Two delays, not one: a kobold archer kites by being fast at movement and slow at attacks; a bog mummy is the opposite. `delay_for(ActionKind)` picks the correct field.

### `ActionKind` enum

```rust
enum ActionKind { Movement, Attack }
```

Every `ActionFinishedEvent` carries an `ActionKind` so `resolve_turn_end` knows which delay field to multiply.

### Intent messages

Player input and AI dispatch produce **intent messages** (Bevy `Message`s). Execution systems consume them and emit `ActionFinishedEvent`. This decouples *deciding* from *executing*.

| Intent | Source | Handler | Cost |
|---|---|---|---|
| `MovementIntent` | `dispatch_player_action`, AI dispatch | `handle_movement` | base × decoration × deep-water |
| `MeleeIntent` | Player bump, AI dispatch, redirect from movement | `handle_melee` | base × `weapon.attack_speed` |
| `RangedAttackIntent` | Player F-key + targeting, AI | `handle_ranged_attack` | base × attack_delay |
| `WaitIntent` | Spacebar, implicit no-op | `handle_wait` | base × movement_delay |
| `PickUpIntent` | G key | `handle_pickup` | base |
| `OpenDoorIntent` | Movement redirect | `handle_door_open` | base |
| `UnlockDoorIntent` | Movement redirect on locked door | `handle_unlock_door` | base, or free if no key |
| `OpenChestIntent` | Bump on chest | `handle_open_chest` | base |
| `MachineBumpMessage` | Bump on bump-activated machine | `handle_machine_bump` | base |
| `EquipItemMessage` / `UnequipItemMessage` / `DropItemMessage` / `UseItemMessage` | Inventory UI | item handlers in `items.rs` / `effects.rs` | base |
| `ZapStaffMessage` | Staff targeting UI | `handle_zap_staff` | base |

### Lifecycle messages

| Message | Emitted by | Read by | Effect |
|---|---|---|---|
| `ActionFinishedEvent { entity, base_cost, action_kind }` | Every action handler via `finish_turn()` | `resolve_turn_end` | Reschedule entity at `current_time + round(base_cost * delay)` |
| `FreeActionEvent { entity }` | Player handlers via `free_turn()` (invalid moves, cancels) | `resolve_free_actions` | Reschedule at `current_time` exactly, then return to `PlayerInput`. **Player only.** |
| `TurnEndEvent` | `marker_dispatch` when the global `TurnMarker` ticks | Domain systems that want a once-per-round signal | Hook for global per-turn effects |

### `ActionGuard` component

Inserted by dispatch (`dispatch_player_action`, `monster_ai_dispatch`) to track that an action is in flight. Removed by `finish_turn()` / `free_turn()`. If still present at the end of `Cleanup`, `action_guard_safety_net` (`src/game/turns.rs:411-422`) emits a fallback `ActionFinishedEvent` and `warn!`s — the queue is preserved at the cost of a logged bug.

---

## System Flow

```
   ┌──────────────┐
   │   Waiting    │  (AppState::InGame entered → start_turns)
   └──────┬───────┘
          ▼
   ┌──────────────┐    select_next_actor pops queue, tags MyTurn
   │  NextTurn    │───┬──────────────► PlayerInput (player is next)
   └──────┬───────┘   └──────────────► Processing  (NPC batch)
          │
          ▼
   ┌──────────────┐    handle_player_input writes PendingPlayerAction
   │ PlayerInput  │───────────────────► Processing
   └──────────────┘
          ▼
   ┌─────────────────────────────────────────────────────┐
   │              Processing (4 phases, chained)         │
   │                                                     │
   │   Brain ──► ResolveMovement ──► ResolveActions      │
   │                                       │             │
   │   ┌───────────────────────────────────┘             │
   │   ▼                                                 │
   │   CombatReactionSet (.after(CombatDamageSet))       │
   │                                       │             │
   │   ┌───────────────────────────────────┘             │
   │   ▼                                                 │
   │   Cleanup → continue_turn_processing                │
   └─────────────────┬───────────────────────────────────┘
                     │
                     ▼  (queue head determines next state)
                  PlayerInput | Processing | NextTurn
```

### `ProcessingPhase` SystemSet ordering

Configured in `TurnOrderPlugin::build()` (`src/game/turns.rs:180-199`). All four phases are chained and gated on `in_state(TurnState::Processing)`.

| Phase | Members | Purpose |
|---|---|---|
| `Brain` | `populate_blocked_tiles`, `squad_coordinator_system`, `dispatch_player_action`, `goap_ai_dispatch`, `monster_ai_dispatch`, `marker_dispatch` | Translate intent. Runs `chain()` so each AI step sees the previous one's writes. |
| `ResolveMovement` | `handle_movement` | Movement runs before other handlers because it can redirect (bump → melee, bump → door, bump → chest). |
| `ResolveActions` | `handle_melee`, `handle_ranged_attack`, `handle_pickup`, `handle_wait`, `handle_use_item`, `handle_zap_staff`, `handle_equip/unequip/drop_item`, `handle_door_open`, `handle_unlock_door`, `handle_open_chest`, `handle_machine_bump`, `handle_drop_at_hoard` | All non-movement actions. Runs in parallel — handlers are independent. |
| `Cleanup` | `action_guard_safety_net` → `resolve_free_actions` → `resolve_turn_end` → `StatusEffectSet` → `status_expiry_log` → `fire_tick` → `gas_tick` → `tile_promotion_tick` → tile/decoration/liquid mutation appliers → `continue_turn_processing` | Wrap up the turn, apply queued tile mutations, advance the queue. Chained. |

### `CombatReactionSet`

A separate `SystemSet` configured `.after(CombatDamageSet)` and gated on `AppState::InGame` (`src/game/turns.rs:202-206`). On-hit, on-being-hit, and on-death reactions live here: runic procs, burning strike, knockback, explode-on-death, summon-on-death, terrify/rally auras. They run in the same Bevy frame as the damage that triggered them, but logically they are part of the "Processing" cycle for the attacker's turn.

### Schedule placement

Everything above runs in `Update`. There is no `FixedUpdate` work in the turn pipeline. `Schedule` is `Update` × `run_if(in_state(...))` for gating.

---

## The Action-Economy Contract

> **Every action handler MUST emit `ActionFinishedEvent` (or `FreeActionEvent` for the player) for the actor it processed.** If it doesn't, the queue stalls — the actor is never reinserted, the next dequeue picks an old time, and the game freezes.

Use `finish_turn()` / `free_turn()` from `actions.rs:185-204` instead of writing the message directly. They both clear `ActionGuard`, so the safety-net warning fires cleanly when something is missed.

```rust
// Standard end-of-action pattern:
finish_turn(&mut commands, &mut finish_writer, entity, BASE_ACTION_COST, ActionKind::Movement);

// Player-only no-op (invalid move, cancelled targeting):
free_turn(&mut commands, &mut free_writer, entity);
```

### Cost formula

`resolve_turn_end` (`src/game/turns.rs:440-489`):

```rust
let delay = stats.delay_for(event.action_kind);
let reinsert_time = compute_reinsert_time(current_time, event.base_cost, delay);
let cost = reinsert_time - current_time;            // = round(base_cost * delay)
turn_manager.insert_at(entity, current_time + cost);
```

`BASE_ACTION_COST` is the canonical unit (defined in `src/constants.rs`). A delay of `1.0` means "exactly one base turn." `0.5` means twice as fast; `2.0` means twice as slow.

### Free actions

`FreeActionEvent` reinserts at `current_time` itself — the actor will be the next thing dequeued, and `resolve_free_actions` immediately switches state back to `PlayerInput`. Player gets to retry. **Never emit for monsters** — they would loop forever. (`src/game/turns.rs:427-438`.)

### Free-action emission sites

- Bumping a wall, OOB, or impassable collider as the player → `free_turn`.
- Bumping a locked door without a key → `free_turn`.
- Pressing F (ranged) opens targeting overlay; no event is emitted — the turn loop simply stays in `PlayerInput` until targeting confirms or cancels.
- Chasm-bump → `free_turn` while the confirmation modal is open; the actual fall is processed separately.

---

## Edge Cases

### Speed runic (free turn marker)

After a successful weapon proc, the attacker may receive a `SpeedRunicProc` component. When that entity's next `ActionFinishedEvent` resolves, `resolve_turn_end` overrides `cost = 0` and removes the marker. The entity is reinserted at `current_time` and acts immediately on the next dequeue, "before" their normal turn would have come up.

This is currently the only free-turn mechanic. (Riposte was removed; the Sword is the balance baseline with no special free-action behavior.)

### Staff zaps

Charges, not mana. A staff zap costs one full turn (`BASE_ACTION_COST`, `ActionKind::Movement`). It does not bypass the action economy.

### UI screens

The turn loop is suspended whenever `InGameState != Running`. Inventory, Character Info, Targeting, Enchant Select, Modal — all switch substates and `handle_player_input` stops reading keys (its `run_if` includes `in_state(InGameState::Running)`). The actor still has `MyTurn`; nothing has happened in game time.

UI navigation (open inventory, scroll log) does **not** emit `ActionFinishedEvent`. Inventory item actions (equip, unequip, drop, use) **do** emit it via their handlers.

### Status effects that block input

`player_status_override` (`src/game/turns.rs:514-522`) checks `Stunned` and `Entangled`. When set, any movement key is silently rewritten to `Action::Wait` and a log message fires. The player still consumes a turn — the status effect ticks down via `StatusEffectSet` inside the same `Cleanup` phase. No infinite loop because each press only triggers one wait.

### Floor transitions

When the player descends, the new floor's spawner adds entities to a fresh `TurnManager`. Stale `ActionFinishedEvent`s from the old floor may still be in the message buffer; `resolve_turn_end` guards against double-insertion with `if turn_manager.contains(event.entity) { continue; }` (`src/game/turns.rs:455-457`).

### Death

Dying entities are removed from the queue via `remove_entity` in cleanup systems (death pipeline). `dequeue_next_batch_pure` also defensively skips stale entities (`world.get_entity_mut` check). A monster killed mid-batch won't act.

### NPC batching

`MAX_NPC_BATCH` caps how many NPCs can dequeue in one `select_next_actor` call. Without this, a horde of fast monsters could all dequeue at the same tick and run their AI in a single frame, locking the game thread. The cap rolls excess into the next `Processing` cycle.

---

## Cross-links

- **Combat math (hit/dodge/damage):** [GAME.md](GAME.md). The turn system schedules attacks; GAME.md says how they resolve.
- **Monster cooldown abilities:** [ABILITIES.md](ABILITIES.md) *(planned)*. Ability cooldowns count down per turn, anchored on the actor's own `ActionFinishedEvent`. They are not a global tick.
- **Status effect durations:** [STATUS_EFFECTS.md](STATUS_EFFECTS.md) *(planned)*. Durations decrement in `StatusEffectSet`, which lives inside `ProcessingPhase::Cleanup` after `resolve_turn_end`. One full turn cycle = one duration tick.
- **Squad coordination:** [SQUAD_AI.md](SQUAD_AI.md). The squad coordinator runs in `ProcessingPhase::Brain` before individual GOAP planners, on every cycle (not every turn-marker tick).
- **Chasm fall flow:** [CHASMS.md](CHASMS.md). Uses `free_turn` + `InGameState::ChasmConfirm` modal; no time consumed during the prompt.
