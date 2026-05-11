# Gas

Gas is volumetric area-of-effect terrain. Clouds spawn from sources (Bloats, staves, fungus, dying monsters), redistribute to cardinal neighbors each turn, decay 10% per turn, apply status effects above a concentration threshold, and ignite into fire pulses when they touch flame. Brogue's gas-cloud feel — but volume-tracked rather than binary "yes/no".

## Design Philosophy

- **Gas is terrain, not a buff.** A poison cloud is a region of the map for as long as the volume holds. Walking through it is a tactical choice, like wading through water.
- **Volume drives everything.** Concentration determines whether the cloud damages, blocks vision, ignites, and how aggressively it diffuses. A faint trail of poison does nothing; a thick cloud kills.
- **Gas spreads, but it does not chase.** Pure diffusion to cardinal neighbors. The cloud fills the room it was born in, leaks through doorways, and pools in dead-ends — there is no "AI" to it.
- **Decay is global and predictable.** Every cloud loses 10% volume per turn. The player can outlast a cloud by waiting one or two corridor lengths away.
- **Fire interaction is the punchline.** Poison gas is flammable. A torch, a Burning Strike, or a fire staff into a gas-filled room produces a multi-tile fire pulse that often catches the caster too.

See [STATUS_EFFECTS.md](STATUS_EFFECTS.md) for the Poisoned/Burning effects gas applies, [FIRE.md](FIRE.md) for ignition, [DUNGEON.md](DUNGEON.md) for terrain, and [ENEMIES.md](ENEMIES.md) for Bloat variants and Mycoid Sovereign.

## Data Model

Gas is **entity-based** with a spatial-index resource.

```rust
// src/game/gas.rs:28
pub enum GasType { Poison, Steam }

// src/game/gas.rs:120
pub struct GasMarker {              // ECS marker on each gas entity
    pub gas_type: GasType,
    pub concentration: u16,
}

// src/game/gas.rs:126
pub struct GasTileData {
    pub gas_type: GasType,
    pub concentration: u16,
    pub entity: Entity,             // index back to the ECS entity
}

// src/game/gas.rs:134
pub struct GasTiles(pub HashMap<(i32, i32), GasTileData>);
```

Each occupied tile holds **one** gas entity (with `Position`, `FloorEntityMarker`, `GameEntityMarker`, `GasMarker`). The `GasTiles` resource is the spatial index — every spawn/despawn/tick keeps it in sync with the entities. Concentration is a `u16` so volumes can run from 1 (about to dissipate) up to thousands (Mycoid super-emission). The `FloorEntityMarker` means gas is despawned on floor change and rebuilt from the cached `BuilderMap` if the floor is re-entered.

### Constants

| Name | Value | Source | Purpose |
|------|-------|--------|---------|
| `EFFECT_THRESHOLD` | 100 | `gas.rs:35` | Concentration ≥ this applies status effects and changes display name |
| Decay rate | 10% per turn | `gas.rs:149` | `new = concentration * 9 / 10` integer math |
| Redistribution share | 20% to each neighbor | `gas.rs:299` | `share = concentration / 5` |
| `GAS_EMISSION_CHANCE` | 12 (out of 100) | `gas.rs:141` | Per-turn chance per fungus tile to belch gas |
| Fungus emission volume | 200 | `gas.rs:279` | Hardcoded volume per emission burst |
| Ignition AoE | 3×3 (Chebyshev radius 1) | `gas.rs:375` | Damage radius around each ignited tile |

## Spread System

`gas_tick_system` runs once per `TurnEndEvent` and performs five passes (`gas.rs:250-437`):

1. **Emission.** Walk every map tile; if `decoration == Fungus` and `rng.range(0,100) < 12`, queue a 200-volume Poison emission on that tile.
2. **Redistribution.** Snapshot every gas tile, then redistribute. Each tile sends 1 share (= 20%) to itself and 1 share to each of its 4 cardinal neighbors. Walls and `Empty` tiles eat the share — gas is "lost to the wall".

   ```text
   share = concentration / 5
   self_keeps:    +share          // 20%
   each cardinal: +share if can_gas_occupy(neighbor)  // 20% × 4 = 80%
   total preserved: up to 100% (less when neighbors are walls)
   ```

   Two clouds of the **same type** at the same destination tile **add**; two clouds of **different type** at the same destination tile do **not** mix — each gas type only contributes to its own type entry, the smaller is effectively cancelled. This is the same neutralization rule used by `spawn_gas` (`gas.rs:188`).
3. **Fire interaction.** Any flammable gas (`GasType::Poison.flammable()` is `true`; Steam is `false`) on a `FireTiles` position is removed and triggers an AoE fire damage burst.
4. **Decay.** Every remaining tile loses 10%. `decay_concentration` returns `None` (despawn the tile) when integer math collapses to 0 — i.e., concentration of 1.
5. **Creature effects.** Any creature standing on a tile whose concentration ≥ 100 receives the gas's `on_step_effect` (Poisoned/Burning) unless they have the relevant resistance.

Pure helpers `decay_concentration` and `can_gas_occupy` (`gas.rs:148`, `gas.rs:154`) are unit-tested at the bottom of the file.

## Decay

| Tick concentration in | Tick concentration out |
|----------------------:|-----------------------:|
| 500 | 450 |
| 200 | 180 |
| 100 | 90 |
| 50 | 45 |
| 10 | 9 |
| 1 | 0 (despawn) |

A 200-volume cloud reaches sub-100 (no longer harmful) after 7 ticks (`200 → 180 → 162 → 145 → 130 → 117 → 105 → 94`) and dissipates entirely after about 50 ticks if it never spreads or stacks. Spreading speeds dissipation drastically — every share that flows into a wall is permanently lost.

Decay is **global and uniform**. There is no per-tile timer or age field; the only state is current concentration. This is a resolved decision: see "Resolved Decisions" below.

## Damage & Status

```rust
// gas.rs:41
pub fn on_step_effect(&self, concentration: u16) -> Option<(StatusEffectKind, u32, i32)> {
    if concentration < 100 { return None; }
    match self {
        GasType::Poison => Some((StatusEffectKind::Poisoned, 3, 1)),
        GasType::Steam  => Some((StatusEffectKind::Burning,  3, 2)),
    }
}
```

| Gas | Threshold | Status applied | Duration | Magnitude |
|-----|-----------|----------------|----------|-----------|
| Poison | ≥ 100 | Poisoned | 3 turns | 1 dmg/turn |
| Steam | ≥ 100 | Burning | 3 turns | 2 dmg/turn |

Effects are **re-applied every turn** while the creature stands in the cloud (the call goes through `add_effect_with_magnitude`, which refreshes duration), so lingering in a thick poison cloud means a constant Poisoned 3 — not 3 once. Resistances short-circuit in pass 5: poison-resistant creatures ignore Poison gas, fire-resistant creatures ignore Steam (`gas.rs:53`).

## FOV Blocking

**Currently binary.** A tile with a gas entity blocks FOV via the existing `is_opaque` rules; concentration is not used by the FOV traversal. This means a wisp of gas at concentration 5 occludes vision the same as a thick concentration 800 cloud.

This is intentional for now. The full Brogue model — per-ray opacity accumulation, color tinting, partial obscuration — is tracked in the user's project memory `project_brogue_gas_visibility`. Until then we accept the binary cliff because (a) volumes high enough to show on-screen at all are usually high enough that "you can't see through it" matches player intuition, and (b) implementing per-ray opacity requires touching the bracket-lib FOV path.

## Sources

| Source | File:line | Mechanic |
|--------|-----------|----------|
| **Bloat — `ExplodeOnHit { GasCloud }`** | `abilities.rs:127`, `abilities.rs:423` | On melee hit, the Bloat dies and `gas_positions_in_radius(radius)` are filled with Poison at the configured volume. The `ExplodeOnHit` handler also strips `GasOnDeath` from itself (`abilities.rs:433`) so the kill doesn't double-spawn. |
| **`GasOnDeath` (Pit Bloat, etc.)** | `abilities.rs:120`, `abilities.rs:553` | On `DeathEvent`, fill the Manhattan radius with Poison gas at `volume`. Same helper as above. |
| **Staff of Poison** | `staves.rs` (existing) | Targeted gas burst at the cursor tile. |
| **Mycoid Sovereign** | `monsters.ron` (Mycoid Sovereign uses fungus emission via decoration) | A Sovereign laying down `Decoration::Fungus` causes the per-tile 12% emission to fire each turn. |
| **Fungus tiles (passive)** | `gas.rs:268` | Every fungus tile rolls 12% per turn for a 200-volume Poison emission on itself. |
| **Steam from fire+water** | `fire.rs` water interaction | When a fire tile sits on shallow water, a Steam cloud is spawned. (See FIRE.md.) |

The ECS-side helper `gas_positions_in_radius` (`abilities.rs:534`) returns every tile inside a Manhattan radius that passes `gas::can_gas_occupy` — so chasms, walls, and `Empty` tiles are skipped automatically by the source.

## Ignition

Pass 3 of `gas_tick_system` (`gas.rs:361`) checks every gas tile against `FireTiles`. Any flammable gas sitting on fire ignites and is consumed:

```rust
let dmg = gas_type.ignition_damage(concentration);  // gas.rs:76
// 3×3 AoE around the ignited tile
for (entity, pos, _, name) in creature_query.iter() {
    if (pos.x - x).abs() <= 1 && (pos.y - y).abs() <= 1 {
        damage_writer.write(DamageEvent { amount: dmg, damage_type: Fire, .. });
    }
}
despawn_gas(...);
```

| Concentration at ignition | Fire damage to each creature in 3×3 |
|--------------------------:|------------------------------------:|
| 50  | 1 |
| 100 | 2 |
| 200 | 4 |
| 500 | 10 (clamped) |
| 1000 | 10 (clamped) |

The damage type is `DamageType::Fire`, source `Environment`, and there is no armor mitigation. Each ignited tile fires its own 3×3, so a multi-tile cloud chains: the 3×3 from the first tile may overlap with the 3×3 from a neighboring still-igniting tile, and any unconsumed neighboring gas tiles will themselves ignite next tick if they were also on fire. **Steam is not flammable** (`gas.rs:60`) and contributes 0 damage if it ever ended up on a fire tile.

## Edge Cases

- **Walkable-only spawning.** `can_gas_occupy(tile)` (`gas.rs:154`) excludes `Wall` and `Empty`. Floor tiles, doors, and tiles with liquid (water, lava, chasm) all accept gas. Chasms count: gas can pool over a chasm even though creatures can't stand there. Walls eat any share sent to them and that volume is permanently lost — this is also how clouds dissipate naturally in tight corridors.
- **Two gas types on one tile.** Not stored — `GasTiles` is keyed by position with a single `(GasType, u16)` value. `spawn_gas` resolves a collision by neutralizing equal volume of both, then storing whichever survives. The `gas_tick_system` redistribution preserves this rule per-tick: each (position, type) entry is independent and only adds to itself.
- **Floor transitions.** Gas entities carry `FloorEntityMarker`, so they despawn on floor change. The `BuilderMap` cache stores `Decoration::Fungus`, so re-entered floors with fungus immediately resume emission. **Player-spawned clouds (e.g. from a Staff of Poison) are not preserved across floor change** — re-entering a floor returns it to its cached pre-action state.
- **Doors.** Closed doors are walkable in `can_gas_occupy` terms — gas does pass through them on redistribution. Open doors behave the same. (FOV blocking by closed doors is unchanged.) This is intentional and matches Brogue: shutting a door does not contain a gas leak.
- **Bloat double-spawn guard.** `ExplodeOnHit { GasCloud }` strips `GasOnDeath` before triggering its own death (`abilities.rs:433`) so a Bloat configured with both components only emits gas once.

## Resolved Decisions

- **Volume-based, not binary.** Concentration is a `u16` rather than a presence flag. This drives ignition damage scaling, the effect threshold, decay, and per-tick redistribution.
- **Decay is global, not per-tile.** No per-cell age or expiration timer — every tile loses 10% per turn from its current concentration. This means a freshly stacked tile (e.g. two Bloats overlapping their clouds) decays at the same rate as a thin trail; it just takes longer to drop below the effect threshold.
- **FOV blocking is binary for now.** Per-ray opacity (Brogue-style tinting through clouds) is deferred to `project_brogue_gas_visibility`. Today: any non-empty gas tile occludes vision.
- **Same-type stacking adds, different-type collisions neutralize.** Established in `spawn_gas` (`gas.rs:175-220`) and preserved by the snapshot-based redistribution.
- **Walls absorb shares (no reflection).** Volume sent to a wall is permanently lost. This was deliberate: it keeps clouds from filling indefinitely in closed rooms and gives the player a way to outlast a Bloat detonation by hugging a wall corner.

## Cross-Links

- [STATUS_EFFECTS.md](STATUS_EFFECTS.md) — Poisoned (1 dmg/turn × 3) and Burning (2 dmg/turn × 3) applied by gas inhalation.
- [FIRE.md](FIRE.md) — `FireTiles` resource consumed by `gas_tick_system` ignition pass; Steam-from-water source.
- [DUNGEON.md](DUNGEON.md) — `Decoration::Fungus` (passive emitter) and tile terrain that gates `can_gas_occupy`.
- [ENEMIES.md](ENEMIES.md) — Bloat archetype family (`ExplodeOnHit`, `GasOnDeath`) and Mycoid Sovereign (fungus spreader).
- [CHASMS.md](CHASMS.md) — chasm tiles accept gas overlay even though creatures cannot stand on them.
