# Dungeon Alarm System

## Overview

Replaces the `TyrantAspects` time-based boss escalation with a per-floor **Dungeon Alarm** that creates visible, environmental pressure through spawned threats. The dungeon punishes lingering — patrols search the floor, reinforcements arrive from below, and eventually elite strike teams hunt the player down.

**Design goals:**
- Environmental pressure the player can see and feel on every floor
- Simpler than the 4-Aspect system it replaces
- Per-floor alarm with depth-scaled baseline and rate
- Monsters arrive from the stairs, as if ascending from deeper in the dungeon

## Alarm Resource

```rust
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct DungeonAlarm {
    pub level: u32,                         // Total alarm ticks (time + combat noise)
    pub stage: AlarmStage,                  // Derived from level thresholds
    pub floor_entered_time: u32,            // TurnManager::current_time when floor was entered
    pub combat_noise_accumulated: u32,      // Accumulated noise from combat actions
    pub patrols_spawned: u32,               // Waves spawned this floor
    pub reinforcements_spawned: u32,
    pub strike_teams_spawned: u32,
    pub next_ongoing_patrol_at: u32,        // Alarm tick threshold for next ongoing patrol
    pub next_ongoing_reinforcement_at: u32, // Alarm tick threshold for next ongoing reinforcement
    pub next_ongoing_strike_at: u32,        // Alarm tick threshold for next ongoing strike team
    pub last_ambient_message_at: u32,       // Alarm tick when last ambient message fired
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AlarmStage {
    #[default]
    Calm,       // 0-499 alarm ticks
    Uneasy,     // 500-1499
    Alert,      // 1500-2999
    Alarmed,    // 3000-4999
    Hunted,     // 5000+
}
```

### Stage Thresholds

| Stage | Alarm Ticks | ~Player Turns (floor 1) | Effects |
|-------|------------|-------------------------|---------|
| **Calm** | 0-499 | ~0-50 | Nothing. Grace period. |
| **Uneasy** | 500-1,499 | ~50-150 | 1 patrol spawns. Atmospheric messages. |
| **Alert** | 1,500-2,999 | ~150-300 | 2nd patrol. |
| **Alarmed** | 3,000-4,999 | ~300-500 | Reinforcement squad arrives. |
| **Hunted** | 5,000+ | ~500+ | Strike team. Endless reinforcements. |

### Per-Floor Lifecycle

- **Reset on floor entry**: `DungeonAlarm` resets to `Calm` with `level: 0` when `spawn_dungeon` runs.
- **No carry-over**: Each floor starts fresh.
- **Depth scaling**: Deeper floors start with a baseline bonus and accumulate faster.

## Alarm Advancement

### Primary: Time

Every `TurnEndEvent`:
```
elapsed_game_ticks = current_time - floor_entered_time
time_ticks = (elapsed_game_ticks / TICKS_PER_ALARM_TICK) * depth_alarm_rate(depth)
alarm.level = depth_baseline(depth) + time_ticks + alarm.combat_noise_accumulated
```

Where `TICKS_PER_ALARM_TICK = 10`. The time component is recomputed from the global clock each tick. The `combat_noise_accumulated` field is additive and never recomputed — it only grows when combat events fire. This ensures combat noise is never overwritten by the time recalculation.

A player action (100 game ticks) adds ~10 alarm ticks at base rate.

### Depth Scaling

**Rate multiplier:**
```rust
fn depth_alarm_rate(depth: u32) -> f32 {
    1.0 + (depth as f32 - 1.0) * 0.15
}
```
Floor 1: 1.0x. Floor 5: 1.6x. Floor 10: 2.35x.

**Baseline bonus:**
```rust
fn depth_baseline(depth: u32) -> u32 {
    if depth <= 2 { 0 } else { (depth - 2) * 75 }
}
```
Floor 3: 75. Floor 6: 300. Floor 10: 600 (starts near Uneasy immediately).

### Secondary: Combat Noise

Each relevant action handler emits an `AlarmNoiseMessage { amount: u32 }` alongside its normal intent/action flow. The `alarm_combat_noise_system` reads these messages and adds the amount to `alarm.combat_noise_accumulated`. Only **player-initiated** actions generate noise.

| Action | Alarm Ticks | Emission Point |
|--------|------------|----------------|
| Melee attack | +15 | `handle_melee` (player entity only) |
| Ranged attack | +10 | `handle_ranged_attack` (player entity only) |
| Spell cast | +20 | `handle_cast_spell` (player entity only) |
| Monster killed | +25 | `death_system` (when killer is player) |
| Door opened (by player) | +5 | `handle_door_open` (player entity only) |

```rust
#[derive(Message, Debug, Clone)]
pub struct AlarmNoiseMessage {
    pub amount: u32,
}
```

This keeps alarm concerns decoupled from the turn system — `ActionFinishedEvent` is not modified.

### Player Agency

No explicit way to reduce alarm. Efficient play (don't linger, don't backtrack) naturally keeps it low. Fighting is necessary but costly. Waiting is pure waste.

## Spawn Point: DownStairs

All alarm-spawned monsters enter from the **DownStairs tile**, as if ascending from deeper in the dungeon. On floor 10 (boss floor, no DownStairs), they spawn from the **UpStairs** instead.

If the stairs are in the player's FOV, the player sees them arrive — this is intentional and creates a great tension moment.

**Cluster placement**: Squad members spawn on walkable tiles adjacent to the stairs using BFS outward (reuse existing cluster logic from `monster_spawner.rs`).

## Patrol Behavior: RandomDestination

A new `PatrolState` variant for alarm-spawned patrols (note: `PatrolRoute` is a struct containing a `state: PatrolState` field — the variant lives on `PatrolState`, using `(i32, i32)` tuples per existing serde convention):

```rust
PatrolState::RandomDestination {
    target: Option<(i32, i32)>,
    player_bias: bool,  // true for alarm patrols
}
```

**Behavior:**
1. Squad leader picks a random walkable tile, **biased toward the player's current position** (weighted random: tiles closer to the player are more likely to be selected).
2. Leader pathfinds to it via A*.
3. Squad followers leash to the leader (existing squad system, 4-tile range).
4. When the leader reaches the destination (or pathfinding fails), pick a new random destination.
5. If any member spots the player, the whole squad transitions to `Hunting` via `squad_alert_system`.
6. If they lose the player (chase leash exceeded), they revert to `RandomDestination` patrol — still biased toward the player's last known position.

**Player bias weighting:** When selecting a random walkable tile as a destination, weight by inverse distance to the player (or last known player position). Tiles within 20 tiles of the player are 3x more likely than distant tiles. This makes patrols feel like active searchers without being omniscient.

## Escalation Compositions

### Floor 1-3 (Vermin & Early Goblins)

| Stage | Type | Composition | Behavior |
|-------|------|-------------|----------|
| Uneasy | Patrol | 1-2x Giant Rat | RandomDestination (biased), scatter on leader death |
| Alert | Patrol | 2x Goblin | RandomDestination (biased), scatter on leader death |
| Alarmed | Reinforcement | Goblin + Goblin Archer | Hunting, squad, scatter on leader death |
| Hunted | Strike Team | Goblin Warchief + 2 Goblin + Goblin Archer | Hunting, no leash, no flee |

### Floor 4-6 (Organized Groups)

| Stage | Type | Composition | Behavior |
|-------|------|-------------|----------|
| Uneasy | Patrol | 2x Wolf | RandomDestination (biased), scatter on leader death |
| Alert | Patrol | Goblin + Goblin Archer | RandomDestination (biased), scatter on leader death |
| Alarmed | Reinforcement | Orc Warrior + 2 Goblin | Hunting, squad, scatter on leader death |
| Hunted | Strike Team | Orc Warlord + 2 Orc Warrior + Orc Archer | Hunting, no leash, no flee |

### Floor 7-9 (Heavy Hitters)

| Stage | Type | Composition | Behavior |
|-------|------|-------------|----------|
| Uneasy | Patrol | 2x Skeleton | RandomDestination (biased), scatter on leader death |
| Alert | Patrol | Orc Warrior + Bone Archer | RandomDestination (biased), scatter on leader death |
| Alarmed | Reinforcement | Ogre + 2 Orc Warrior | Hunting, squad, scatter on leader death |
| Hunted | Strike Team | Hill Giant + 2 Ogre + Orc Archer | Hunting, no leash, no flee |

### Ongoing Spawns

Tracked via `next_ongoing_*_at` fields on `DungeonAlarm`. When `alarm.level` crosses the threshold, a spawn fires and the threshold advances by the interval.

| Stage | Type | Interval | Initial Threshold | Cap |
|-------|------|----------|-------------------|-----|
| Alert | Patrol | 500 ticks | 2,000 | 2 extra patrols |
| Alarmed | Reinforcement | 1,000 ticks | 4,000 | No cap |
| Hunted | Reinforcement | 1,000 ticks | 6,000 | No cap |
| Hunted | Strike Team | 2,500 ticks | 7,500 | No cap |

Example: `next_ongoing_patrol_at` starts at 2,000. When alarm reaches 2,000, spawn a patrol and set `next_ongoing_patrol_at = 2,500`. When alarm reaches 2,500, spawn another and stop (cap of 2 extra patrols in Alert stage).

## Boss Interaction (Floor 10)

The Tyrant's power is determined by the **alarm stage when the player reaches the boss room**, not by global time.

| Alarm Stage | Boss Bonus |
|-------------|-----------|
| Calm | Base stats only |
| Uneasy | +10 HP, +1 armor |
| Alert | +20 HP, +2 armor, gains fire_dart |
| Alarmed | +35 HP, +3 armor, fire_dart + fireball |
| Hunted | +50 HP, +5 armor, fire_dart + fireball + BurningStrike |

`BossAI` and the HP-threshold phase system (3-phase fight) stay unchanged. The Aspect system (Flame/Iron/Blood/Storm, `TyrantAspects`, `AspectKind`, `AspectState`) is removed entirely.

## HUD Indicator

A colored label below the floor depth display:

| Stage | Color | Text |
|-------|-------|------|
| Calm | Gray | "Calm" |
| Uneasy | Yellow | "Uneasy" |
| Alert | Orange | "Alert" |
| Alarmed | Red | "Alarmed" |
| Hunted | Pulsing bright red | "HUNTED" |

## Atmospheric Messages

**Stage transitions:**
- Uneasy: *"Something stirs in the darkness..."*
- Alert: *"The dungeon grows restless. Shadows gather at the edges of your vision."*
- Alarmed: *"The dungeon trembles with rage. You hear boots echoing in distant corridors."*
- Hunted: *"The dungeon has found you. There is no hiding now."*

**Ambient flavor** (10% chance per turn within each stage, cooldown tracked by `last_ambient_message_at`, min 50 alarm ticks between messages):
- Uneasy: *"You hear skittering in a nearby passage."* / *"A cold draft extinguishes a distant light."*
- Alert: *"Guttural voices echo through the stone."* / *"Something heavy drags itself through a corridor nearby."*
- Alarmed: *"The air grows thick with dread."* / *"You hear a horn blast from somewhere below."*
- Hunted: *"They know exactly where you are."* / *"The dungeon itself seems to close in around you."*

## Integration Points

### New File: `src/game/alarm.rs`

- `DungeonAlarm` resource and `AlarmStage` enum
- `AlarmPlugin` (replaces boss escalation registration)
- `alarm_tick_system` — runs on `TurnEndEvent`, advances alarm from elapsed time
- `AlarmNoiseMessage` — new message type emitted by combat handlers
- `alarm_combat_noise_system` — reads `AlarmNoiseMessage`, adds to `combat_noise_accumulated`
- `alarm_stage_check_system` — detects stage transitions, triggers spawns
- `alarm_spawn_system` — spawns patrols/reinforcements/strike teams at DownStairs
- `alarm_atmosphere_system` — log messages at transitions and ambient flavor
- `alarm_reset_system` — resets alarm on floor entry
- `apply_alarm_bonuses_on_spawn` — applies alarm-based bonuses to boss entity
- Helpers: `alarm_patrol_composition(depth)`, `alarm_reinforcement_composition(depth)`, `alarm_strike_team_composition(depth)`

### Modified Files

1. **`src/game/ai.rs`**: Add `PatrolState::RandomDestination { target: Option<(i32, i32)>, player_bias: bool }` variant. Implement biased random destination selection in the patrol movement logic. Add transition from Hunting back to RandomDestination when chase leash exceeded. Strike team monsters need post-spawn override of `chase_leash: 0` and `flee_at_hp_percent: 0.0` (these come from asset defaults and must be overwritten after `spawn_monster_by_name`).
2. **`src/game/boss.rs`**: Remove `TyrantAspects`, `AspectKind`, `AspectState`, all `apply_*_aspect` functions, `tyrant_escalation_system`. Keep `BossAI` and `boss_phase_system`. Add `apply_alarm_bonuses_on_spawn` that reads `DungeonAlarm`.
3. **`src/game/mod.rs`**: Replace boss escalation plugin registration with `AlarmPlugin`. Add `pub mod alarm;`.
4. **`src/ui/mod.rs`**: Add `AlarmStageText` component. Add alarm stage HUD indicator below floor depth.
5. **`src/save/mod.rs`**: Replace `tyrant_aspects` with `dungeon_alarm` in `GameSaveData`. Update auto_save and load paths. Use `#[serde(default)]` for backwards compatibility.
6. **`src/map/dungeon.rs`**: Insert `DungeonAlarm::new(depth, current_time)` in `spawn_dungeon`. Restore from save data on load.
7. **`src/map/builders/monster_spawner.rs`**: Extract `find_cluster_points` to a shared utility so the alarm spawner can reuse it.

### System Ordering

```
TurnEndEvent
  -> alarm_tick_system              (recompute level from time + combat_noise_accumulated)
  -> alarm_stage_check_system       (detect stage transitions, check ongoing spawn thresholds)
  -> alarm_spawn_system             (spawn patrols/reinforcements/strike teams at stairs)
  -> alarm_atmosphere_system        (log messages, respects last_ambient_message_at cooldown)

AlarmNoiseMessage (emitted by combat handlers)
  -> alarm_combat_noise_system      (adds to combat_noise_accumulated)
```

### Boss Floor Behavior

On floor 10, the alarm system still runs but spawns from UpStairs instead of DownStairs. Once the player enters the boss room (detected by proximity to the boss entity or entering the boss arena area), alarm spawns are **suppressed** — the boss fight itself is the pressure. The alarm stage at that moment determines the boss's stat bonuses via `apply_alarm_bonuses_on_spawn`.

### Backtracking

When the player backtracks to a previously visited floor (via floor cache), the alarm resets to `Calm`. This is intentional — backtracking costs real game time which advances the alarm on whatever floor you end up on. The trade-off is time spent, not accumulated alarm.

## Save/Load

1. Add `dungeon_alarm: DungeonAlarm` to `GameSaveData` with `#[serde(default)]`
2. `auto_save_system`: read `Res<DungeonAlarm>`, populate field
3. Load path in `spawn_dungeon`: `commands.insert_resource(save_data.dungeon_alarm.clone())`
4. DungeonAlarm does NOT need floor caching — it resets on each floor entry
5. Keep old `tyrant_aspects` field with `#[serde(default)]` so old saves parse (but ignore it)
