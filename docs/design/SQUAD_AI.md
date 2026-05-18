# Squad AI & Goblin Faction

Covers the generic squad coordinator system usable by any faction,
with goblins as the first implementation.

> **Implementation status (overworld milestone):** this doc is the
> forward-looking design. What's actually wired up today is a subset:
> `SquadId`, `SquadLeader` (marker), `SquadConfig`, `SquadBlackboard`,
> alert propagation, and the basic morale component. **None of the
> goblin / role / morale-table content described below ships in the
> current build** — those depend on monsters and spawn tables, which
> are disabled for the overworld + temple themes.
>
> The simple `LeaderDeathBehavior::Scatter` config flag (and the
> `squad_leader_death_system` that consumed it) was removed during
> the overworld milestone. The design intent "kill the warchief →
> squad scatters" now relies entirely on the morale system below:
> leader death contributes −0.3 morale, which drops the squad past
> the Rout threshold (0.15), at which point individual flee behavior
> takes over. The hard-coded scatter event is no longer part of the
> architecture.

---

## Player Experience

A goblin encounter should feel like fighting a **disorganized-but-cunning mob** that becomes an **organized warband** as the player descends. Early goblins scatter when hurt. Mid-game goblins call for help and hide behind brutes. Deep goblins execute coordinated retreats, set up defensive positions, and the shaman keeps them alive while the warchief rallies them.

The player should learn:
- **Kill the shaman first** — or the warband regenerates
- **Don't let scouts escape** — or reinforcements arrive
- **Chasing retreating goblins is dangerous** — they're regrouping at a chokepoint
- **Kill the warchief** — and the squad scatters

This creates a **puzzle within combat**: target priority and positioning matter more than raw stats.

---

## Architecture: Two-Layer System

Based on the F.E.A.R. pattern (Jeff Orkin, GDC 2006) and Brogue's emergent coordination:

```
Layer 1: Squad Coordinator (runs once per squad per processing cycle)
  Reads: squad member positions, HP, player position, alert state
  Writes: SquadBlackboard (shared state resource on the squad leader entity)

Layer 2: Individual GOAP (runs per goblin per turn)
  Reads: SquadBlackboard + local perception (viewshed, adjacency)
  Writes: Intent messages (MovementIntent, MeleeIntent, CastSpellMessage, etc.)
```

Individual goblins are **unaware** they're in a squad. The coordinator steers behavior by modifying the world state that individual planners read from. This keeps the planner's search space small and makes squad behavior independently tunable.

---

## Squad Blackboard (Generic — Any Faction)

The blackboard is a **generic component** attached to the squad leader entity. Any faction can use it — goblins, orcs, undead, dragons. Faction-specific behavior comes from GOAP goal/action configurations, not from the blackboard itself.

```
SquadBlackboard {
    // Awareness
    alert_level: AlertLevel,          // Unaware / Alerted / InCombat
    known_player_pos: Option<Point>,  // Shared last-known position
    turns_since_contact: u32,         // Resets on any member sighting

    // Tactical State
    retreat_ordered: bool,            // Leader called retreat
    fallback_point: Option<Point>,    // Where to retreat to

    // Role Assignments
    roles: HashMap<Entity, SquadRole>,

    // Spatial (Dijkstra maps, computed by coordinator)
    player_distance_map: Option<DijkstraMap>,
    retreat_map: Option<DijkstraMap>,
    flank_map: Option<DijkstraMap>,

    // Position Reservation
    reserved_positions: HashMap<Point, Entity>,
}

enum AlertLevel { Unaware, Alerted, InCombat }

enum SquadRole {
    Scout,        // Go find and alert nearby same-faction monsters
    Guard,        // Stay between leader and threat
    Flanker,      // Circle around to attack from the side
    Bodyguard,    // Stay adjacent to leader
    Skirmisher,   // Shoot and reposition behind allies
    Support,      // Heal, buff, stay in back line
    Commander,    // The leader — issues orders, stays behind front line
}
```

Roles are generic labels. How each role *behaves* is defined by per-archetype GOAP actions. A goblin Guard charges recklessly. An orc Guard holds formation. Same role, different execution.

---

## Morale System

Morale is a **per-entity `Morale(f32)` component** (not on the blackboard). This means:
- It persists across floor transitions via the save system
- A goblin that fled a fight on floor 3 starts floor 3 with low morale when the player returns
- Different squad members can have different morale (a brute holds while grunts break)
- Non-squad monsters can also have morale (future: wolves, undead fear of fire, etc.)

The squad coordinator **reads** individual morale values to compute squad-level decisions (retreat orders), then **writes** morale modifiers back to each member based on shared events.

### Morale Modifiers (applied by the coordinator each cycle)

Base morale is set at spawn time per archetype (e.g., goblins 0.6, orcs 0.8, brutes 0.7).

| Event | Modifier | Scope | Notes |
|-------|----------|-------|-------|
| **Leader alive in squad** | +0.2 | All members | Leadership bonus |
| **Healer alive in squad** | +0.1 | All members | Healing confidence |
| **Outnumber player 3:1+** | +0.15 | All members | Mob courage |
| **Outnumber player 2:1** | +0.05 | All members | Slight confidence |
| **Squad member killed** | -0.15 | All members | Per death, cumulative |
| **Leader killed** | -0.3 | All members | Devastating (replaces alive bonus) |
| **Healer killed** | -0.15 | All members | Replaces alive bonus |
| **Own HP < 50%** | -0.1 | Self only | Personal injury |
| **Own HP < 25%** | -0.15 | Self only | Desperate |
| **Saw ally die this turn** | -0.05 | Witnesses only | Immediate shock |
| **WarCry / rally active** | +0.1 | Affected members | Battle fury |

Morale **recovers slowly** when out of combat: +0.05 per turn when `turns_since_contact > 5`.

### Morale Thresholds (read by coordinator for squad decisions)

| Average Squad Morale | Coordinator Decision |
|---------------------|---------------------|
| 0.8+ | **Aggressive** — Assign flankers, archers advance |
| 0.5-0.8 | **Normal** — Hold positions, execute assigned roles |
| 0.3-0.5 | **Cautious** — Prioritize guard/support roles, no flanking |
| 0.15-0.3 | **Retreat** — Leader orders retreat, sets fallback point |
| < 0.15 | **Rout** — Coordinator dissolves. Every monster flees individually |

### Individual Morale in GOAP

Each goblin's GOAP world state includes `self_morale_low: bool` (own morale < 0.3). This affects individual decisions independent of squad orders.

---

## Role Assignment

The coordinator assigns roles based on archetype and situation. Roles are reassigned when squad composition changes (member dies, new goblins alerted and join).

### Default Role Mapping

| Monster | Default Role | Reassigned When |
|---------|-------------|-----------------|
| Goblin | Guard | Scout (if unalerted goblins nearby), Flanker (if morale > 0.6) |
| Goblin Archer | Skirmisher | -- |
| Goblin Brute | Bodyguard | Guard (if no warchief) |
| Goblin Shaman | Support | -- |
| Goblin Warchief | Commander | -- |

### Dynamic Reassignment Rules

1. **If unalerted goblins are within 15 tiles**: Assign the grunt nearest to them as `Scout`. Scout runs toward sleeping goblins to alert them, then returns to `Guard`.
2. **If morale > 0.6 and grunts > 2**: Assign one grunt as `Flanker`. Flanker circles around to the opposite side of the player.
3. **If warchief dies**: All `Bodyguard` roles become `Guard`. Remove `Commander`.
4. **If shaman dies**: Morale penalty already applied. No role change.
5. **If squad has 1 member left**: All roles become irrelevant -- individual flee behavior takes over.

---

### Awareness propagation (stealth integration)

When any squad member transitions to `AwarenessState::Aware` about a target, squadmates receive `Searching{last_known_pos}` via `AwarenessAlertEvent`, **not direct `Aware`**. They begin investigating the spotted position; they only become Aware when they roll perception successfully themselves. This is intentional — instant squadwide `Aware` would feel like radar. See [STEALTH.md](STEALTH.md) §Squad Propagation for the full handler, and the alert handler in [src/game/stealth.rs](../../src/game/stealth.rs) (`squad_propagate_awareness`).

## Alert Propagation

Extends the existing `squad_alert_system` (12-tile range within a squad) with cross-squad alerting.

### Scout Mechanic

When a monster with `SquadRole::Scout` is assigned:
1. The coordinator identifies the nearest sleeping same-faction monster (`MonsterAIMode::Asleep`, not in the scout's squad). Goblins target other "Goblin" faction monsters -- they won't alert kobolds or undead.
2. Sets a `scout_target: Option<Point>` on the blackboard
3. The scout's GOAP planner sees `unalerted_allies_nearby: true` and selects the `alert_allies` action
4. Scout pathfinds toward the target. When adjacent, emits `AlertNearbyMessage { source, position }`
5. **Alert handler** wakes all sleeping monsters within 6 tiles:
   - **No squad**: Add to the scout's squad, assign role by archetype
   - **Different squad**: Wake entire squad via existing `squad_alert_system`. Both squads operate independently but converge on the player from different directions
   - **Same squad**: Already covered by `squad_alert_system`
6. Scout's role reverts to `Guard`

### Scout Shout / Audio Feedback

When the `AlertNearbyMessage` handler fires:
1. Emit a `SoundEvent { position, sound_type: GoblinShout }` at the scout's position
2. **If the scout is in the player's FOV**: The player sees the goblin shriek (particle effect)
3. **If the scout is NOT in the player's FOV**: Log message with direction:
   - `"You hear a frantic goblin shriek from the [north/east/south/west]!"`

The `SoundEvent` system is generic -- any faction can emit sounds. Future: wolf howls, orc war drums, undead chanting.

### Multi-Squad Encounters

When alerted monsters already belong to a squad, they keep their squad:
- Squad A (3 goblins + warchief) engages the player
- Squad A's scout alerts Squad B (2 goblins + brute) in the next room
- Squad B's coordinator independently decides to engage from a different direction
- The player now faces two coordinated groups with **separate retreat triggers and morale**

### Alert Cascade Timing

- Turn 1: Goblin sees player, squad enters `InCombat`
- Turn 2: Coordinator assigns nearest grunt as Scout
- Turn 3-5: Scout runs toward sleeping monsters (2-4 tiles away)
- Turn 6: Scout arrives, alerts them
- Turn 7+: Reinforcements arrive

The player has a **3-5 turn window** to kill the scout or prepare.

---

## Controlled Retreat

When morale drops to 0.15-0.3, the warchief orders a retreat.

### Retreat Flow

1. **Coordinator sets** `retreat_ordered = true` and picks a `fallback_point`:
   - Priority 1: The warchief's spawn position (its "camp")
   - Priority 2: The nearest chokepoint behind the squad (away from player)
   - Priority 3: The farthest explored tile from the player

2. **Individual goblins** see `squad_retreating: true` in their world state. The `retreat_to_fallback` action becomes available with very low cost.

3. **During retreat**: Archers still shoot while retreating. Brutes hold chokepoints.

4. **At the fallback point**, the squad reforms: Brutes at chokepoint, archers behind, shaman behind everyone, warchief in the back.

5. **Retreat cancels** when: morale recovers above 0.5, player not visible for 5+ turns, or all members reach fallback.

### Player Counterplay

- **Chase the retreat**: Risky -- goblins set up at a chokepoint, brute blocks the door
- **Let them go**: They'll heal up (shaman) and come back with reinforcements
- **Cut off the retreat**: Maneuver ahead of them and catch them in the open

---

## Dijkstra Maps for Spatial Reasoning

The Coordinator computes **Dijkstra maps once per squad per turn**. Individual entities check 8 neighbors instead of running A*.

### Map Types

**Player Distance Map**: Player position = 0, all tiles increase in cost outward.
**Flanking Map**: Tiles adjacent to the player are high cost. Tiles behind the player are low cost.
**Retreat Map**: Fallback point = 0, combined with player distance to penalize tiles near the player.

### Action Dispatch via Maps

```
"flee"              -> move to adjacent tile with HIGHEST value on Player Distance Map
"retreat"           -> move to adjacent tile with LOWEST value on Retreat Map
"flank"             -> move to adjacent tile with LOWEST value on Flanking Map
"engage"            -> move to adjacent tile with LOWEST value on Player Distance Map
"reposition_behind" -> move to adjacent tile where ally is between self and player
```

O(8) per entity instead of O(V+E) per A* search.

---

## Chokepoint Slot Reservation

The `SquadBlackboard` maintains `reserved_positions: HashMap<Point, Entity>`.

When `retreat_ordered`:
1. Coordinator identifies the chokepoint at `fallback_point`
2. Assigns first brute as `PrimaryDefender` -- reserves the position
3. Additional brutes see `chokepoint_occupied: true` and become `SupportDefender` (1 tile behind)

---

## Individual GOAP: Per-Archetype Configuration

### Extended WorldState (squad additions)

```
// Squad-derived (set by coordinator)
squad_morale_low: bool,
squad_retreating: bool,
near_leader: bool,
assigned_scout: bool,
unalerted_allies_nearby: bool,
war_cry_needed: bool,

// Combat positioning
ally_between_self_and_threat: bool,
at_chokepoint: bool,
at_reserved_position: bool,
chokepoint_occupied: bool,

// Spells
can_cast_useful_spell: bool,
ally_wounded: bool,
```

### Goblin (Grunt)

| Goal | Priority | Desired State |
|------|----------|---------------|
| Survive | 10 | `!adjacent_to_threat OR has_escape_route` |
| Retreat | 8 (when active) | `at_fallback_point` |
| Follow Squad | 7 | `near_leader` |
| Alert Allies | 6 | `!unalerted_allies_nearby` (Scout only) |
| Engage | 3 | `adjacent_to_threat` |

### Goblin Archer

| Goal | Priority | Desired State |
|------|----------|---------------|
| Survive | 10 | `!adjacent_to_threat` |
| Retreat | 8 | `at_fallback_point` |
| Maintain Distance | 6 | `ally_between_self_and_threat` |
| Engage Ranged | 4 | `player_visible` |

### Goblin Brute

| Goal | Priority | Desired State |
|------|----------|---------------|
| Survive | 10 | `!hp_low` |
| Protect Leader | 8 | `near_leader` |
| Hold Chokepoint | 7 | `at_chokepoint` (when retreating) |
| Engage | 5 | `adjacent_to_threat` |

### Goblin Shaman

| Goal | Priority | Desired State |
|------|----------|---------------|
| Survive | 10 | `!adjacent_to_threat` |
| Heal Allies | 8 | `!ally_wounded` |
| Stay Safe | 7 | `ally_between_self_and_threat` |
| Retreat | 8 | `at_fallback_point` |
| Attack | 2 | `player_visible` |

### Goblin Warchief

| Goal | Priority | Desired State |
|------|----------|---------------|
| Survive | 10 | `!adjacent_to_threat OR has_escape_route` |
| Order Retreat | 9 (when morale < 0.3) | `squad_retreating` |
| Rally Squad | 8 | `war_cry_active` |
| Command Position | 6 | `ally_between_self_and_threat` |
| Engage | 4 | `adjacent_to_threat` |

---

## Spell Integration with GOAP

### GOAP Decides *When*, Spell Scorer Decides *What*

```
GOAP Layer (strategic):    "Should I cast, flee, guard, or reposition?"
  | selects "cast_spell"
Spell Scorer (tactical):   "Which specific spell is best right now?"
  | returns (spell_slot, target)
Spell System (execution):  "Apply the spell effects"
```

Single `CastSpell` GOAP action with precondition `can_cast_useful_spell: true`.
Adding spells to a monster in `monsters.ron` requires zero GOAP changes.

### Role-Based Spell Bias (soft multipliers)

| Squad Role | Spell Bias | Multiplier | Condition |
|-----------|-----------|------------|-----------|
| Support | PreferHeal | 3.0x | Target is ally AND ally HP < 50% |
| Support | PreferBuff | 2.0x | Target is ally (haste, shields) |
| Commander | PreferBuff | 2.5x | Self-buff (enrage, spirit shield) |
| Guard / Skirmisher | PreferOffense | 2.0x | Target is enemy AND at chokepoint |
| Any (default) | None | 1.0x | No bias |

```rust
struct SpellBias {
    heal_multiplier: f32,
    buff_multiplier: f32,
    offense_multiplier: f32,
}
```

---

## Depth Progression

| Depth | Encounter Style | GOAP Complexity |
|-------|----------------|-----------------|
| 1-3 | **Disorganized** -- 1-3 goblins, no squad, flee at 30% HP | Low: 3 goals, 4 actions |
| 4-5 | **Organizing** -- Squads of 3-5 with shaman. Alert propagation. | Medium: 5 goals, 6 actions |
| 6-7 | **Organized** -- Full warbands with warchief. Retreat, scout, chokepoints. | Full: 6+ goals, 8+ actions |
| 8-9 | **Fortified** -- Machine encounters. Defensive positions. | Full + spatial setup |

---

## Machine Placement -> Initial Role Mapping

| Placement Hint | Initial Role |
|----------------|-------------|
| **AtGate** | Guard |
| **NearGate** | Guard or Skirmisher |
| **Center** | Commander |
| **DeepInterior** | Support |
| **AlongWalls** | Skirmisher |
| **Random** | Guard (default) |

### Goblin Camp (floors 4-7)

```
Gate: door (locked if depth >= 6)
AtGate: 1x Goblin Brute (Guard)
NearGate: 1-2x Goblin (Guard)
AlongWalls: 1x Goblin Archer (Skirmisher)
Center: 1x Goblin Warchief (Commander)
DeepInterior: 1x Goblin Shaman (Support)
```

### Goblin Fort (floors 7-9)

```
Outer gate: locked door
AtGate: 2x Goblin Brute (Guard, Bodyguard)
Outer NearGate: 2x Goblin (Guard)
Outer AlongWalls: 1-2x Goblin Archer (Skirmisher)
Inner gate: door
Inner Center: 1x Goblin Warchief (Commander)
Inner DeepInterior: 1x Goblin Shaman (Support)
Inner: 1x chest (loot reward)
```

---

## Monster Asset AI Configuration Refactor

Currently, AI behavior flags (`flee_at_hp_percent`, `erratic_chance`, `chase_leash`, `kites`, `kite_distance`) are top-level fields on `MonsterAsset`. This mixes combat stats with AI configuration and provides no way to specify GOAP behavior in data.

### New `ai` Field on MonsterAsset

Replace the scattered AI flags with a single `ai` field that's an enum — either standard FSM config or GOAP archetype config:

```ron
// Standard FSM monster (current behavior, most monsters):
"Goblin Archer": (
    name: "Goblin Archer",
    // ... combat stats ...
    ai: Fsm(
        flee_at_hp_percent: 0.3,
        erratic_chance: 0.0,
        chase_leash: 0,
        kites: true,
        kite_distance: 3,
    ),
),

// GOAP monster:
"Kobold Hoarder": (
    name: "Kobold Hoarder",
    // ... combat stats ...
    ai: Goap(
        archetype: "kobold_hoarder",
        base_morale: 0.5,
    ),
),

// GOAP goblin with squad coordination:
"Goblin Warchief": (
    name: "Goblin Warchief",
    // ... combat stats ...
    ai: Goap(
        archetype: "goblin_commander",
        base_morale: 0.7,
    ),
),
```

### Rust Types

```rust
// In assets/mod.rs:
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AiConfig {
    /// Standard 3-state FSM (Asleep/Hunting/Idle).
    Fsm {
        #[serde(default)]
        flee_at_hp_percent: f32,
        #[serde(default)]
        erratic_chance: f32,
        #[serde(default)]
        chase_leash: u32,
        #[serde(default)]
        kites: bool,
        #[serde(default = "default_kite_distance")]
        kite_distance: u32,
    },
    /// Goal-Oriented Action Planning.
    Goap {
        /// Archetype ID that maps to a goals/actions configuration function.
        archetype: String,
        /// Initial morale value (0.0-1.0).
        #[serde(default = "default_morale")]
        base_morale: f32,
    },
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig::Fsm {
            flee_at_hp_percent: 0.0,
            erratic_chance: 0.0,
            chase_leash: 0,
            kites: false,
            kite_distance: 3,
        }
    }
}
```

### Spawner Integration

In `spawn_monster()`, read the `ai` field:
- `AiConfig::Fsm { .. }` -> insert `MonsterAI` component with the FSM flags (current behavior)
- `AiConfig::Goap { archetype, base_morale }` -> insert `GoapAI` component with archetype-specific goals/actions + `Morale(base_morale)`. Also insert a minimal `MonsterAI` as fallback (for squad leash, etc.)

### GOAP Archetype Registry

The `archetype` string maps to a configuration function:

```rust
pub fn goap_config_for(archetype: &str) -> Option<(Vec<Goal>, Vec<ActionDef>)> {
    match archetype {
        "kobold_hoarder"     => Some((kobold_hoarder_goals(), kobold_hoarder_actions())),
        "goblin_grunt"       => Some((goblin_grunt_goals(), goblin_grunt_actions())),
        "goblin_archer"      => Some((goblin_archer_goals(), goblin_archer_actions())),
        "goblin_brute"       => Some((goblin_brute_goals(), goblin_brute_actions())),
        "goblin_support"     => Some((goblin_support_goals(), goblin_support_actions())),
        "goblin_commander"   => Some((goblin_commander_goals(), goblin_commander_actions())),
        _ => None,
    }
}
```

### Migration

All existing monsters get `ai: Fsm(...)` with their current values moved into the struct. Monsters that previously had bare `flee_at_hp_percent: 0.3` at the top level become:

```ron
ai: Fsm(flee_at_hp_percent: 0.3),
```

Monsters with no AI flags get the default (all zeros/false), which matches current behavior since those fields already defaulted to zero.

### Backward Compatibility

Use `#[serde(default)]` on the `ai` field. Old save files and cached floors without the `ai` field will get `AiConfig::default()` (standard FSM with zero flags).

---

## Interaction With Existing Systems

- **Squad System**: Reuses SquadId/SquadLeader/SquadConfig. Implements scatter purely through morale (no `on_leader_death` config flag — that was removed; see status note at the top of this doc).
- **Faction Matrix**: Add "Goblin" faction. Goblin-Player: Hostile, Goblin-Monster: Neutral, Goblin-Kobold: Neutral.
- **Status Effects**: WarCry applies Enraged + morale bonus.
- **Save/Load**: Morale component persists. SquadBlackboard serialized with role assignments.

---

## Open Questions

1. **Morale recovery rate**: +0.05/turn out of combat. Full recovery from 0.15 to 0.6 takes 9 turns.
2. **Non-GOAP squad members**: SquadOrder component (`Retreat(Point)` / `Alert` / `Hold`) for FSM monsters to read.
3. **Multi-floor alert**: Persist alert state in floor cache for returning to cleared floors.

---

## Implementation Phases

| Phase | Scope | Dependencies |
|-------|-------|-------------|
| 0 | MonsterAsset `ai` field refactor (AiConfig enum, migrate monsters.ron) | None |
| 1 | SquadBlackboard + squad_coordinator_system | Existing squad system |
| 2 | Morale component + calculation | SquadBlackboard |
| 3 | Goblin grunt GOAP conversion | GOAP engine (done), AiConfig |
| 4 | Role assignment in coordinator | SquadBlackboard + Goblin GOAP |
| 5 | Archer + Brute GOAP conversion | Role system |
| 6 | Shaman GOAP + spell bias integration | SpellBias parameter |
| 7 | Warchief GOAP + retreat mechanic | Full role system + morale |
| 8 | Dijkstra maps in coordinator | SquadBlackboard |
| 9 | Scout/alert + shout mechanic | Cross-squad alerting |
| 10 | Chokepoint slot reservation | Dijkstra maps + retreat |
| 11 | Depth-scaled configuration | All above |
