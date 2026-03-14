# Project Plan

This plan is ordered by dependency — each milestone builds on the last.

---

## Current State (Baseline)

What already works:
- Turn-based combat (hit roll, damage roll, armor reduction, death)
- Stats: STR, DEX, CON, AGI (4 attributes, modifier system, `AttributeModifiers` for equipment)
- XP on kill + leveling (HP roll, stat point awarded)
- Stat point allocation UI exists in `src/ui/character_info.rs` — `StatDraft` resource, +/- buttons per stat, confirm button, live combat stat preview
- Monster AI: hunt/wander/sleep state machine with A* pathfinding
- Map generation: 10-floor dungeon with up/down stairs, FOV
- Item pickup: item despawns, AmuletOfBevy triggers a placeholder victory
- Turn system: speed-scaled queue, intent message pipeline

What doesn't exist yet: inventory, equipment, magic, item types, bosses, status effects, enemy abilities, horde spawning.

---

## Milestone 1 — Stats Foundation

*Prerequisite for everything else. Small scope.*

**Goal:** Align the stat system with the design doc and unlock mana + spell slots as data.

Tasks:
- Add `Intelligence` and `Luck` to the `Attributes` struct in `src/game/stats.rs`
- Add `StatDraft` fields for INT and LCK; add `AllocationAction` variants `PlusIntelligence`, `MinusIntelligence`, `PlusLuck`, `MinusLuck` in `src/ui/character_info.rs`
- Add INT and LCK attribute rows to `spawn_character_info_ui` (same pattern as existing rows)
- Derive `Mana { current: i32, max: i32 }` component from `INT × 5`; add mana to the character info preview
- Add `spell_slots_unlocked: u8` to `Experience` component, incremented at levels 3, 5, 8, 11, 14
- Update `assets/` player RON with INT and LCK starting values (default: 5)

Files: `src/game/stats.rs`, `src/game/level.rs`, `src/ui/character_info.rs`, `assets/`

---

## Milestone 2 — Inventory System

*Foundation for items, equipment, and consumables.*

**Goal:** Items picked up are stored in the player's inventory (20 slots) rather than despawned.

Tasks:
- Add `Inventory { items: Vec<Entity>, capacity: usize }` component to the player
- Add `ItemKind` enum: `Weapon`, `Armor`, `Ring`, `Amulet`, `Consumable`, `Spellbook`
- Add `ItemProperties` component: damage dice, defense value, stat bonuses, effect (see M5), rarity
- Update pickup action in `src/game/actions.rs` to add item to `Inventory` instead of despawning
- Add inventory UI screen (`I` key): list items, highlight selection, show properties panel
- Add drop action: remove from inventory, respawn item entity at player position

Files: `src/components.rs`, `src/game/actions.rs`, `src/game/turns.rs`, `src/ui/`, `assets/items.ron`

---

## Milestone 3 — Equipment System

*Depends on Milestone 2.*

**Goal:** Player can equip/unequip gear from inventory; equipped gear modifies stats.

Tasks:
- Add `Equipment` component with 9 named slots: `weapon`, `offhand`, `helm`, `chest`, `gloves`, `boots`, `ring_l`, `ring_r`, `amulet`
- Add equip/unequip actions from inventory screen (`E` on selected item)
- On equip: apply item's stat delta to the existing `AttributeModifiers` component, trigger stat recalculation
- On unequip: remove stat delta, return item to inventory
- Combat reads equipped weapon's damage dice instead of base `Damage` component
- Combat reads total DEF (base + sum of armor pieces) for damage reduction
- Show equipped slots in character info screen

Files: `src/components.rs`, `src/game/combat.rs`, `src/game/stats.rs`, `src/ui/character_info.rs`

---

## Milestone 4 — Item Generation & Loot Tables

*Depends on Milestone 2. Enables all subsequent content milestones.*

**Goal:** Floors place items as loot; monsters drop items on death. Spawn probability is configurable — high but not guaranteed.

Tasks:
- Add `spawn_chance: f32` (0.0–1.0) to each loot table entry in `assets/items.ron`. Floor item generation rolls against this per entry — default ~0.75 for floor drops, varies per monster table.
- Define item tables in `assets/items.ron`: all weapons, armor, rings, consumables with stats, rarity, and `min_floor`/`max_floor` depth range
- Rarity tier field on `ItemAsset`; rarity weights shift toward better tiers by floor depth (see `design/ITEMS.md`)
- Add item spawner builder step (alongside `monster_spawner.rs`) that samples the floor's item table and places items at random walkable positions
- Add `LootTable { entries: Vec<LootEntry> }` component to monster entities; on `DeathEvent`, iterate entries, roll `spawn_chance` per entry, spawn winners
- Ensure all `ItemKind` variants from M2 have content entries

Files: `assets/items.ron`, `src/game/spawner.rs`, `src/map/builders/`, `src/game/combat.rs`

---

## Milestone 5 — Shared Effect System + Consumables

*Depends on Milestones 2 and 4.*

**Goal:** Define a single `Effect` enum used by both consumables and spells. Potions and scrolls trigger effects from inventory.

The effect system is the shared primitive — `HealHp(i32)` means the same thing whether triggered by a Healing Potion or a Healing Word spell. This avoids duplicating logic between M5 and M6.

Tasks:
- Define `Effect` enum in `src/game/effects.rs` (new file):
  - `HealHp(i32)`
  - `RestoreMana(i32)`
  - `BuffStat { stat: StatKind, amount: i32, duration: u32 }`
  - `CurePoison`
  - `Teleport`
  - `RevealMap`
  - `AreaDamage { damage: String, radius: u32, damage_type: DamageType }`
  - `FleeEnemies { duration: u32 }`
  - `ApplyStatusEffect(StatusEffectKind, duration)` (used once M7 exists)
- Add `apply_effect(effect, target, &mut World)` execution system — all consumers call this
- Add `UseItem` action and intent message; on resolution, read item's `Effect`, call `apply_effect`, remove item from inventory
- Implement all potion and scroll effects using the `Effect` system
- Add game log messages per effect application

Files: new `src/game/effects.rs`, `src/game/actions.rs`, `src/game/turns.rs`

---

## Milestone 6 — Magic System

*Depends on Milestone 1 (mana + spell slots) and Milestone 5 (shared Effect system).*

**Goal:** Player learns spells from spellbooks, equips them to active slots, and casts them using mana.

Tasks:
- Add `KnownSpells { spells: Vec<SpellKind> }` and `ActiveSpells { slots: [Option<SpellKind>; 6] }` components to player
- Add `SpellKind` enum with all spells from `design/MAGIC.md` and their `mana_cost`, `Effect` (or `Vec<Effect>`)
- Spellbook item use: reads spellbook's `SpellKind`, adds to `KnownSpells`
- Add spell management UI: view known spells, assign to active slot, show mana cost
- Add `CastSpell(slot_index)` action (keybinds `1`–`6`); check mana, deduct, call `apply_effect`
- Mana regeneration system: +1 mana per 5 turns (passive); staff equipped gives +2/turn
- Spell scaling: damage effects scale by `spell_power = INT + focus_orb_bonus`
- Spells reuse `Effect` variants from M5 — no duplicate damage/heal logic

Files: `src/components.rs`, `src/game/actions.rs`, new `src/game/magic.rs`, `src/ui/`

---

## Milestone 7 — Enemy Abilities, Cooldowns & AI Decision Making

*Depends on Milestones 5 and 6 (effect system established).*

**Goal:** Enemies have special abilities with cooldowns; AI picks actions intelligently via a score-based system.

### Reference Design
`abilities.rs` (project root) contains a complete ability evaluation system from a prior project (Shipyard ECS). Port its core approach to Bevy:
- `KnownAbility` struct with `cooldown`, `current_cd`, `range`, `min_range`, `radius`, `target`, `effects`
- `AbilityTarget` enum: `Caster` (self-only), `Entity`, `Tile`, `EntityOrSelf`
- `choose_ability()` iterates off-cooldown abilities, scores each candidate, returns the best `(ability, target)` if score > 0
- Scoring is per-`Effect` variant, weighted by faction reaction (`Ally` / `Attack` / `Flee`)
- AOE abilities sum scores over all entities within radius
- Cache `(caster_entity, ability_name, target_entity) → i32` to avoid re-evaluating the same triple in one decision pass

### Faction & Reaction System
The scoring approach requires knowing how the caster relates to a target:
- Add `Faction(String)` component to all actors (e.g., `"goblin"`, `"undead"`, `"player"`)
- Add a `faction_reaction(caster_faction, target_faction) → Reaction` lookup (data-driven table in RON or hardcoded map)
- `Reaction` enum: `Ally`, `Attack`, `Flee`, `Neutral`
- Monsters treat `player` faction as `Attack`; monsters of same faction treat each other as `Ally`

### Status Effects
- Add `ActiveStatusEffects(Vec<StatusEffect>)` component:
  - `Poison { dmg_per_turn, turns_remaining }`
  - `Stunned { turns }`
  - `Slowed { multiplier: f32, turns }`
  - `ConDrain { amount, turns }`
  - `Feared { turns }`
  - `Invisible { turns }`
- Status tick system: runs each actor's turn, applies damage/modifiers, decrements, removes on expiry
- `Stunned` and `Feared` short-circuit AI — forced behavior before ability scoring runs

### Ability Definitions
- `KnownAbility` (ported from `abilities.rs`):
  ```
  name: String
  cooldown: u32           // turns between uses
  current_cd: u32         // decremented each turn; usable when 0
  target: AbilityTarget   // Caster | Entity | Tile | EntityOrSelf
  range: u32
  min_range: Option<u32>  // e.g. AoE with friendly fire risk
  radius: u32             // 0 = single target
  effects: Vec<Effect>    // reuses M5 Effect enum
  ```
- `KnownAbilities { abilities: Vec<KnownAbility> }` component on monster entities
- Cooldown decrement runs every turn per actor; reset to `cooldown` after use

### Ability Scoring (port from `abilities.rs`)
`score_single_target(ability, caster, reaction, target) → i32`:
- `Effect::Damage { amount }`: enemy → `+min(target.hp, amount)`; ally → `-amount`
- `Effect::Heal { amount }`: ally → `+(missing_hp.min(amount) × 50)` (high weight to prioritize healing); enemy → `-50`
- `Effect::Slow`: enemy → `+10`; ally → `-50`
- `Effect::Haste`: ally → `+10`; enemy → `-50`
- `Effect::Lifesteal { amount }`: enemy → `+min(target.hp, amount) + min(caster.missing_hp, amount)`; ally → `-50`
- Particle/cosmetic effects → `0`
- Only use ability if total score > 0 (net positive outcome)

AOE scoring sums `score_single_target` over all entities within `radius`. Tile-targeted abilities iterate visible tiles, find entities in radius at each, sum scores.

### AI Decision Flow (full action selection)
Each turn, after status checks:
```
1. If Stunned  → wait
2. If Feared   → flee away from threat
3. Run choose_ability() → if returns Some(ability, target), execute it
4. If target is adjacent → melee attack (MeleeIntent)
5. If target in ranged range + LOS → ranged attack (RangedAttackIntent, M8)
6. If HP < 25% AND no flee-preventing component → flee
7. Else → move toward last known player position (existing A* logic)
```

This keeps each decision path independent and extensible — adding a new ability type only requires adding a score rule.

### Ability Roster
Implement abilities from `design/BESTIARY.md` per faction using `KnownAbility` + `Effect` entries:
- Beasts: Poison on hit (Venomous Spider), Howl (Dire Wolf — Summon effect)
- Humanoids: Cleave (Orc — AoE melee), Backstab (Shadow Rogue — high Damage when Invisible)
- Undead: Plague Touch (Zombie — ConDrain status), Life Drain (Wraith — Lifesteal), Life Steal (Vampire)
- Demons: Hellfire Aura (Hellhound — passive AoE Damage each turn, radius 2), Mana Burn (Shadow Fiend)

Files: `src/game/ai.rs`, `src/game/combat.rs`, new `src/game/status.rs`, new `src/game/abilities.rs`, `src/game/spawner.rs`, `src/components.rs`

---

## Milestone 8 — Ranged Combat

*Depends on Milestone 3 (equipment). Relatively self-contained.*

**Goal:** Player bows and enemy ranged attacks use a single unified ranged attack pipeline.

Tasks:
- Add `RangedAttackIntent { attacker, target, damage, range }` — the single message for all ranged attacks (player bow, enemy archer, thrown axe, etc.)
- Add line-of-sight check using existing FOV tile data
- Player ranged attack: requires bow equipped; consumes arrows from off-hand `Arrows(u32)` stack
- Enemy ranged AI: if target in range and LOS, prefer `RangedAttackIntent` over closing to melee (feeds into M7 scoring)
- Combat system handles `RangedAttackIntent` with same hit-check / damage-roll pipeline as melee

Files: `src/game/actions.rs`, `src/game/combat.rs`, `src/game/ai.rs`

---

## Milestone 9 — Boss System

*Depends on Milestones 7 (enemy abilities) and the map system.*

**Goal:** Sealed boss rooms on floors 3, 6, 9, 10 with scripted multi-phase boss encounters.

Tasks:
- Add `Boss { phase: u8, phase_thresholds: Vec<f32> }` component; phase advances when HP crosses a threshold
- Map builder: generate a sealed rectangular boss room at the far end of boss floors; place a locked door that opens on room entry trigger
- Boss seal: once player enters boss room, door locks until boss is defeated
- Implement the 4 bosses (abilities already powered by M7 ability + cooldown system):
  - **Floor 3 — Goblin Warchief:** Battle Cry summon (1/fight), Enrage phase at 40% HP (SPD → 0.83, +3 ATK), Throwing Axe ranged
  - **Floor 6 — Bone Lord:** Reassemble (survives to 30 HP twice before permanent death), Summon Minions from bone piles, Bone Shards AoE
  - **Floor 9 — Pit Fiend:** Hellfire Aura (passive AoE each turn), Infernal Charge dash, periodic Imp summon (cooldown 5 turns)
  - **Floor 10 — Shadow Archon:** Phase 2 at 50% HP adds Darkness Pulse and Mana Void; Shade Summons on schedule
- Floor 10: on boss death, spawn the Amulet of Dominion as a pickable item
- Amulet pickup triggers win screen

Files: `src/map/builders/`, `src/game/spawner.rs`, new `src/game/boss.rs`, `src/game/mod.rs`

---

## Milestone 10 — Enemy Roster & Horde System

*Depends on Milestone 7. Content milestone.*

**Goal:** All 21 enemies from the bestiary are in the game. Difficulty scales by spawning hordes, not inflating stats.

### Enemy Roster
- Implement all 21 regular enemies with correct stats, SPD (1.0 format), and abilities assigned via `EnemyAbilities`
- Spawn table entries in `assets/monsters.ron` per faction with `min_floor`/`max_floor` and weight

### Horde System
Rather than scaling enemy stats with depth, deeper floors spawn larger groups:
- Add `HordeConfig { min_group: u8, max_group: u8 }` to spawn table entries
- Spawner rolls `rng.gen_range(min_group..=max_group)` and places that many of the same enemy in a cluster around the spawn point
- Example progression for Goblin:
  - Floors 1-2: `HordeConfig { min: 1, max: 2 }`
  - Floors 3-4: `HordeConfig { min: 2, max: 3 }`
  - Floors 5+: `HordeConfig { min: 3, max: 4 }`
- Horde placement: first enemy at spawn point, remaining placed in adjacent walkable tiles (BFS outward)
- Test variety and density across a full 10-floor run

Files: `src/game/spawner.rs`, `src/map/builders/monster_spawner.rs`, `assets/monsters.ron`

---

## Milestone 11 — UI & HUD Polish

*Can be done incrementally alongside other milestones.*

**Goal:** Player has clear visibility into their character state at all times.

Tasks:
- HUD: HP/max HP, Mana/max Mana, floor depth, XP progress bar
- Active spell slot display on HUD (slots 1-6, keybind labels, mana cost, greyed if insufficient mana)
- Inventory screen: grid or list view, item details panel, equip/use/drop context actions
- Character screen: extend existing screen with INT, LCK, Mana; add equipment slot display
- Spell management screen: known spells list + slot assignment UI
- Death screen: floor reached, level, cause of death, items carried, XP earned
- Victory screen: run summary

Files: `src/ui/`

---

## Milestone 12 — Balance & Content Pass

*Final milestone before v1.0.*

**Goal:** The game is completable from floor 1 to floor 10 and feels fair and fun.

Tasks:
- Tune XP curve vs actual enemy density and floor count
- Balance item drop rates (enough healing to survive; not so much that risk is removed)
- Tune spell costs vs mana availability — casters should feel mana-constrained but not starved
- Boss tuning (HP, damage, ability cooldowns)
- Horde density tuning per floor tier
- Verify full win condition: floor 1 → floor 10 → boss → amulet → victory screen
- Fix known bugs from `gameplan.md`
- Full playtest run; note and fix pain points

---

## Milestone 13 — Environment & Map Hazards

*Can be developed in parallel with M7-M10. Map generation changes are mostly independent.*

**Goal:** Map generation shifts from dungeon to cavern across floors 1-10; liquid tiles have active effects; gas clouds and traps add environmental danger.

### Map Generation Pipeline

- Add `CellularAutomataBuilder` as a new `InitialMapBuilder` using existing `BlobGenConfig` / `Grid` utilities in `src/map/builders/algorithms.rs`
- Configure `BuilderChain` per floor tier in `src/map/dungeon.rs`:
  - Floors 1-3: `BrogueLikeBuilder` (high room weight) + small water lakes
  - Floors 4-6: `BrogueLikeBuilder` (balanced) + medium water lakes + fewer candles
  - Floors 7-9: `CellularAutomataBuilder` + lava lakes + minimal candles
  - Floor 10: `CellularAutomataBuilder` + lava + `BossRoomBuilder`
- Add `LakeBuilder` config params: `liquid_type` (Water vs Lava), `num_lakes`, `size_range`

### Liquid Tile Systems

- **Shallow water:** on actor turn-start in `ShallowWater` tile: remove `Burning` status; when a lightning spell resolves, flood-fill connected water tiles and arc to all entities within (50% damage per arc)
- **Lava:** on actor turn-start in `Lava` tile: deal 15 HP unless `FireImmune`; on exit: apply `Burning { 5 dmg/turn, 5 turns }` unless `FireImmune`
- Add `FireImmune` marker component; assign to Hellhound, Pit Fiend, Pit Spawn
- Lava tiles emit ambient light: add a dim `PointLight2d` to each lava tile entity (reuses `bevy_light_2d`)

### Gas Clouds

- Add `GasCloud { gas_type: GasType, intensity: u8 }` entity with a tile position
- `GasType` enum: `Poison`, `Sleep`, `Confusion`, `Smoke`
- Gas tick system: decrement intensity each turn; despawn at 0; apply status effect to any actor sharing the tile
- Gas sources: room vent traps (see below); zombie/lich death emissions; optionally throwable flask items
- Gas entities get `FloorEntityMarker` for automatic cleanup on floor transition

### Pressure Plate Traps

- Add `Trap { trap_type: TrapType, triggered: bool, reset_timer: Option<u32> }` entity
- `TrapType` enum: `Dart`, `Alarm`, `Pit`, `GasVent`, `BearTrap`
- Traps start with `Hidden` component — no sprite rendered until detected or triggered
- Detection: on player move adjacent, roll LCK check; success reveals the trap
- Trigger: any entity steps on the trap tile; apply `TrapType`-specific effect
- Placement: corridor intersections and room entrances; density scales 1-2 per floor (floors 1-3) up to 4-6 (floors 7-9); never in starting room or boss room

See `design/ENVIRONMENT.md` for full effect tables.

Files: `src/map/builders/`, `src/map/dungeon.rs`, `src/map/tile.rs`, new `src/game/environment.rs`

---

## Stretch Goals (Post v1.0)

- Item identification system (consumables have randomized unknown appearances each run)
- Particle effects (stairs, spells, death)
- More spell variety (Summon Familiar, Animate Bone, Call Lightning)
- Animated water/lake tiles

---

## Dependency Graph

```
M1 (Stats)
├── M2 (Inventory)
│   ├── M3 (Equipment) ──────────────── M8 (Ranged)
│   └── M4 (Loot Tables)
│       └── M5 (Effects + Consumables)
│           └── M6 (Magic) ───────────┐
│               └── M7 (Abilities/AI) ┤
│                   ├── M9 (Bosses) ──┘
│                   └── M10 (Roster + Hordes)
├── M11 (UI) [ongoing]
├── M13 (Environment) [parallel to M7-M10]
└── M12 (Balance) [final]
```
