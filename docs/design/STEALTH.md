# Stealth & Awareness (Phase 4)

## Design Philosophy

Pre-Phase-4 monsters used a binary `Asleep → Hunting` transition the
moment the player entered their viewshed. That made stealth a property
of the lights-and-walls layout only — there was no skill, no DEX
contribution, no way for a Rogue to *be* a sneak. Phase 4 introduces a
**per-perceiver, per-target awareness model** driven by an **opposed
d20 roll** each turn. Hiding is now a first-class subsystem: every
non-fully-aware monster rolls perception against you each turn, and
their roll is compared to a stealth roll that aggregates the player's
Stealth skill, DEX modifier, equipped armor encumbrance, the light on
the tile, and (eventually) the noise map.

Stealth gives the Rogue a real L1 power axis. With 2 starting ranks of
Stealth, a Cloth-Wraps + Dagger Rogue can reliably break LOS, restealth
in a dim corridor, and re-enter for another Backstab. The system also
ties into Lighting ([LIGHT.md](LIGHT.md)) — a torch on the wall is
genuine information leakage now, not just décor — and to Squad AI
([SQUAD_AI.md](SQUAD_AI.md)) so spotted monsters propagate a *search*
order to squadmates rather than a radar ping. The Backstab gate is
strict: triple damage only when the monster's awareness about the
player is `Hidden`. Sleeping monsters resolve to Hidden by default,
so first-strike ambushes still triple.

## Awareness Model

Per-perceiver `Awareness` component (engine-side,
`roguelike_engine::stealth::Awareness`) maps `target_entity →
AwarenessRecord`. Each record carries the current `AwarenessState`,
the last update turn, and an `Option<Point>` for the last-seen target
position (needed for the `Aware → Searching` LOS-loss transition).

### State machine

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

**Sticky Aware.** No further rolls fire against an `Aware` record
until the perceiver loses LOS — at that point the state drops to
`Searching{last_known_pos, giveup_at_turn = now + 20}`. This is the
hysteresis: once you've been spotted, the monster doesn't randomly
forget about you mid-fight, and once they've lost you, they
investigate the last known position for ~20 turns before giving up.

**Suspicious** is reachable in V1 only through the future noise-map
populator. Its variant, transitions, and decay tick all ship in V1 —
when the V2 noise phase lands, a single new handler
(`noise_event → Suspicious{suspect_pos}`) lights it up.

**Asleep** is a *behavior mode* (`MonsterAIMode`), not an awareness
state. It maps to `AwarenessState::Hidden` plus a flat **`−10`
penalty** on the seeker's perception roll. Sleeping monsters can
still wake from a clumsy stealth attempt — they're just deeply biased
toward staying asleep.

State ordering for AI-mode resolution: `Aware > Searching >
Suspicious > Hidden`. `MonsterAI::update_mode_from_awareness` reads
the strongest hostile-tracked state and drives the mode (Aware /
Searching → Hunting, Suspicious → Idle with investigation target,
Hidden → preserve current mode so Asleep stays Asleep).

## Detection Formula

The opposed roll fires **on the perceiver's turn**, only against
entities that are in the perceiver's `Viewshed.visible_tiles` AND
whose awareness record is one of `Hidden`, `Suspicious`, or
`Searching`. `Aware` skips the roll (sticky).

```
seeker_total = d20 + perception_mod
target_total = d20 + stealth_mod

if seeker_total > target_total:
    awareness.set(target, Aware)
    emit AwarenessAlertEvent { seeker, target }
```

### perception_mod

```
perception_mod = monster.perception          // species, -3..=+5
              + (-10 if MonsterAIMode == Asleep)
              + close_range_bonus(dist)
```

`close_range_bonus`:
- adjacent (Chebyshev distance 1) → `+2`
- distance 2..=3 → `+1`
- distance ≥ 4 → `0`

### stealth_mod

```
stealth_mod = floor(Stealth / 2)             // 0..=13 across 0..=27
            + DEX_mod                        // (score - 16) / 2
            - armor_stealth_penalty          // 0..=5 per equipped armor
            + light_modifier(tile, LightMap)
            + noise_modifier(pos, NoiseMap)  // = -map.at(pos); V1 stub returns 0
```

`light_modifier`:
- `>= 0.75` (bright/torch-adjacent) → `−3`
- `>= 0.40` (dim) → `−1`
- `> 0.00` (very dim) → `+2`
- `== 0.00` (pure dark) → `+3`

Thresholds and constants live as named consts in
[src/game/stealth.rs](../../src/game/stealth.rs) for easy tuning.

### Readability helper

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

Closed-form 20×20 enumeration. Sentinel values: `delta = 0 → 0.475`,
`delta = +10 → 0.952`, `delta = −10 → 0.025`, `delta = +20 → 1.0`,
`delta = −20 → 0.0`. Used by the monster info hover tooltip to render
a human-readable "Notice this turn: X%" line alongside the
component-by-component breakdown.

## Stealth Skill

`Stealth` is the **10th trainable skill** ([SKILLS.md](SKILLS.md)).
Float level `[0.0, 27.0]`; the effect formula feeds the stealth roll
via `floor(Stealth / 2)` (not the standard `floor(skill/4)` other
skills use — Stealth scales twice as fast to make trained Rogues
meaningfully sneakier).

| Skill | Affects | Effect Formula | Weapon binding |
|---|---|---|---|
| **Stealth** | the stealth roll's `target_total` | `+ floor(Stealth / 2)` added to `stealth_mod` | none |

### Use-counter rule

`SkillUseCounters.stealth += 1` at the end of each game turn when:
- ≥1 hostile is in the player's viewshed, AND
- for ≥1 such hostile, its `Awareness.get(player).state != Aware`

In other words: every turn you successfully remain un-fully-detected
with detectors around, you train Stealth. Pure combat doesn't train
it (everyone in your LOS who hasn't won the roll is already Aware via
the attack-reveal handler). Sneaking past asleep monsters in a dark
corridor trains it heavily.

### Race aptitudes

| Race | `stealth` aptitude | Notes |
|---|---|---|
| Human | `0` | Adaptive — baseline rate |
| Dwarf | `-2` | Heavy gait fits the Stoneblood / Armor identity |
| Elf | `+2` | Keen senses also avoid being sensed |

### Class starting allocations

| Class | `stealth` starting rank | Notes |
|---|---|---|
| Warrior | 0 | No stealth identity — full plate eats the bonus anyway |
| Rogue | **2** | L1 power axis (cost: 1 ShortBlades + 1 Dodging) |
| Mage | 0 | INT goes elsewhere |
| Ranger | **1** | Flavor-fit forest scout (cost: 1 Dodging) |

> **Maintenance contract:** `every_class_starting_skills_sums_to_ten`
> and `every_race_aptitude_value_is_in_range` (in
> [src/character/asset.rs](../../src/character/asset.rs))
> automatically cover `Skill::Stealth` because the
> `SkillDistribution` / `SkillAptitudes` helpers include the `stealth`
> field. Adding a new race or class will be validated automatically.

### Armor stealth penalty

Each chest armor item gains an `armor_stealth_penalty: i32` field
(default 0). Subtracted from `stealth_mod` when the armor is equipped.

| Armor | `armor_stealth_penalty` |
|---|---|
| Cloth Wraps / Robe | 0 |
| Padded Armor | 1 |
| Leather | 1 |
| Studded Leather | 2 |
| Chain | 3 |
| Plate | 5 |

## Backstab Gate

The Dagger's `Backstab` weapon ability triples damage **only when the
target monster's awareness record about the player is `Hidden`**.
Searching / Suspicious / Aware all reject — once a monster is even
investigating, the dagger does normal damage.

Asleep monsters map to Hidden by default, so first-strike ambushes
still triple. The gate lives in [src/game/combat/mod.rs](../../src/game/combat/mod.rs);
see [ITEMS.md](ITEMS.md) for the Dagger's broader role in the weapon
lineup and the runic / enchantment story.

The attack-reveal handler flips the victim's awareness record about
the attacker to `Aware` *before* the next frame's perception roll
fires. Backstab the first monster in a pack — the rest now see you,
even if you stayed in a dark tile.

## Noise Map (V2 hook)

`NoiseMap` is a flat `Vec<i32>` resource (one entry per tile) plus
`noise_decay_system` that decrements every cell by 1 each game turn,
floored at 0. The `noise_modifier(pos, &NoiseMap) -> i32` helper
returns `-map.at(pos)` — sound *increases* perception against the
target by *reducing* their stealth roll.

**V1 has no producer.** The decay tick runs and the modifier returns
0 every turn. The system is scaffolded so a V2 noise phase can plug
in a Dijkstra-style populator (movement → low noise, attack → medium,
staff zap → high, etc.) without touching the detection formula.

Persistence: `NoiseMap` is **not saved** (transient, decays to zero
within seconds of load).

## Squad Propagation

A handler on `AwarenessAlertEvent` reads the alerted perceiver's
`Squad` component and, for each squadmate, writes their
`Awareness.get(target)` to:

```
Searching { last_known_pos: target.pos, giveup_at_turn: now + 20 }
```

Squadmates downgrade to **Searching**, *not* Aware. They begin
investigating the spotted position; they only become Aware when they
roll perception themselves. This avoids "radar squads" while
preserving the existing shared-LOS feel. See
[SQUAD_AI.md](SQUAD_AI.md) §Awareness propagation for the broader
squad-AI integration.

## Save Persistence (Schema v6 → v7)

Entity IDs are unstable across save/load, so the full per-perceiver
`Awareness.records` HashMap can't round-trip. V1 uses **degraded
persistence**: each monster persists only its player-keyed record, and
the state is collapsed at save time:

| Live state | Persisted as |
|---|---|
| `Hidden` | `Hidden` |
| `Suspicious{..}` | `Hidden` (V1 simplification) |
| `Searching{last_known_pos, giveup_at_turn}` | `Searching{last_known_pos, offset = giveup_at_turn - now}` |
| `Aware` | `Searching{last_known_pos = player.pos, offset = 20}` |

`giveup_at_turn` is serialized as an offset from "now" so the timer
stays correct after the turn counter reloads. On load, the save
system spawns monsters first, then the player, then walks each
monster's `MonsterAwarenessSave` and inserts the
`AwarenessRecord { state, last_update_turn: 0 }` into the monster's
`Awareness.records[player_entity]`. Monster-vs-monster awareness is
**not persisted** — V1 lets it regenerate within a turn or two via
the normal `perception_tick_system`.

`NoiseMap` does not need to persist (transient, decays to zero in V1).

## UI Surface

- **Nearby sidebar pill** ([src/ui/nearby.rs](../../src/ui/nearby.rs)):
  one of `Sleeping` (Asleep + Hidden), `Wandering` (Hidden + non-Asleep),
  `Suspicious` (yellow), `Searching` (yellow), `Hunting` (red, =Aware).
- **Monster info hover tooltip** ([src/ui/hover_info.rs](../../src/ui/hover_info.rs))
  and **monster inspection overlay** ([src/ui/monster_info.rs](../../src/ui/monster_info.rs))
  show a `─ Stealth ────` section with `Notice this turn: X%` + a
  component-by-component breakdown (species perception, distance bonus,
  asleep penalty, skill, DEX, armor, light, noise). When the monster
  is already Aware, the section reads `Already aware`. When the monster
  has no LOS to the player: `Out of sight`. Percentages come from
  `notice_probability(perception_mod - stealth_mod)`.

## Cross-Links

- [CHARACTER.md](CHARACTER.md) — race / class system; Rogue and Ranger
  starting-skill rows reflect the Stealth redistribution.
- [SKILLS.md](SKILLS.md) — Stealth is the 10th skill; aptitudes table
  and class allocations live there.
- [SQUAD_AI.md](SQUAD_AI.md) — awareness propagation across squadmates.
- [LIGHT.md](LIGHT.md) — `LightMap.intensity_at(pos)` feeds
  `light_modifier`.
- [ITEMS.md](ITEMS.md) — Backstab on the Dagger; armor lineup carries
  the `armor_stealth_penalty` field.
