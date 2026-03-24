# Magic System Implementation Roadmap

This roadmap implements the spell system redesign from [MAGIC.md](MAGIC.md). Each phase builds on the previous one and results in a compilable, testable state.

---

## Phase 1: SpellTarget & SpellEffect Refactor (Foundation)

**Goal**: Rename targeting enum, unify heal effects, add new effect variants. No new gameplay yet — just the data model.

### 1a. Rename SpellTarget variants

**File: `src/game/spells.rs`**
- Rename `Caster` → `Castor`
- Rename `NearestEnemy` → `Enemy`
- Add `Ally` and `AllyOrSelf` variants (no handler yet — just the enum)

**Files that reference SpellTarget (update all match arms):**
- `src/game/magic.rs` — `handle_cast_spell` target resolution
- `src/game/ai.rs` — `choose_spell` target type checks
- `src/game/turns.rs` — `handle_player_input` targeting mode decision
- `assets/spells.ron` — update existing spell target values

### 1b. Unify Heal effect

**File: `src/game/spells.rs`**
- Rename `HealCaster { dice, int_scaling }` → `Heal { dice, int_scaling }`
- The target is now determined by `SpellTarget`, not the effect variant

**File: `src/game/magic.rs` — `handle_cast_spell`**
- Current `HealCaster` always sends `HealMessage` to caster entity
- Change: `Heal` sends `HealMessage` to the **resolved target** (which is the caster for `Castor` spells, but will be an ally for `Ally` spells later)

**File: `src/game/ai.rs` — `choose_spell`**
- Update `HealCaster` match arm → `Heal`
- Scoring logic stays the same for now (still evaluates heal on self)

**File: `assets/spells.ron`**
- Change `HealCaster` → `Heal` in heal_self definition

### 1c. Add new SpellEffect variants (stubs)

**File: `src/game/spells.rs`** — add to enum (with serde derives):
```rust
AoeDamage { dice: String, radius: i32, int_scaling: bool }
ChainDamage { dice: String, max_jumps: i32, jump_range: i32, int_scaling: bool }
Buff { attribute: String, amount: i32, duration: u32 }
Debuff { attribute: String, amount: i32, duration: u32 }
ApplyPoison { damage_per_turn: i32, duration: u32 }
ApplyHaste { duration: u32 }
ApplySlow { duration: u32 }
DrainMana { amount: i32, int_scaling: bool }
SpiritShield { duration: u32 }
Teleport { range: i32 }
```

**File: `src/game/magic.rs` — `handle_cast_spell`**
- Add match arms for all new variants with `warn!("not yet implemented")` + no-op
- This ensures the code compiles and existing spells still work

**Verification**: `cargo check` passes. Existing magic_missile and heal_self work unchanged.

---

## Phase 2: Mana Regen Overhaul

**Goal**: Change mana regen from "INT_bonus + 1 per turn" to INT-scaled regen every 5 turns.

**Formula**: `regen_amount = 1 + floor(INT_bonus / 5)` every 5 turns. Breakpoints at INT 15 (+1) and INT 20 (+1).

### 2a. Add ManaRegen component

**File: `src/game/magic.rs`**
- Add component:
  ```rust
  #[derive(Component, Debug, Clone, Reflect)]
  pub struct ManaRegen {
      pub turns_between_regen: u32,  // default: 5
      pub turns_since_last: u32,     // counter
  }
  ```
- Register type in MagicPlugin

### 2b. Rewrite mana_regen_system

**File: `src/game/magic.rs`** — replace current `mana_regen_system`:
- Current: adds `(INT_bonus + 1).max(1)` per TurnEndEvent
- New: increment `turns_since_last`; when it reaches `turns_between_regen`, add `1 + (INT_bonus / 5)` mana, reset counter
- Query requires `&mut ManaRegen` alongside `&mut Mana` and `&CombatStats` (for INT_bonus)
- INT_bonus = `stats.intelligence - 10`

### 2c. Spawn ManaRegen on entities

**File: `src/game/spawner.rs`**
- Player spawn: insert `ManaRegen { turns_between_regen: 5, turns_since_last: 0 }`
- Monster spawn (if has mana): insert same component

**File: `src/game/stats.rs`**
- Remove any INT-based mana regen logic from `stat_recalculation_system` (there may not be any there — regen is in magic.rs, but verify)

### 2d. Save/Load

Per the save checklist in memory:
- Add `ManaRegen` fields to `GameSaveData` in `src/save/mod.rs`
- Serialize in `auto_save_system`, restore in `spawn_dungeon` / `apply_player_load_system`

**Verification**: Mana now regens at 1 per 5 turns (INT 10-14), 2 per 5 turns (INT 15-19), 3 per 5 turns (INT 20+). Player with INT 14 still has 70 max mana. Spells still cost same mana.

---

## Phase 3: Buff & Debuff System

**Goal**: Implement timed stat modifications via AttributeModifiers.

### 3a. TimedModifier component

**File: `src/game/magic.rs`** (or new file `src/game/status_effects.rs`):
```rust
#[derive(Component, Debug, Clone, Reflect)]
pub struct TimedModifiers {
    pub modifiers: Vec<TimedModifier>,
}

#[derive(Debug, Clone, Reflect)]
pub struct TimedModifier {
    pub attribute: String,     // "strength", "dexterity", "constitution", "agility", "intelligence", "perception", "armor"
    pub amount: i32,           // positive = buff, negative = debuff
    pub turns_remaining: u32,
    pub source_spell: String,  // prevents stacking same spell
}
```

### 3b. Apply system

**System: `apply_timed_modifiers`** (runs in Update, InGame, BEFORE stat_recalculation_system):
- For each entity with `TimedModifiers`:
  - Zero out `AttributeModifiers` (reset each frame)
  - Sum all active TimedModifier amounts per attribute
  - Write totals to `AttributeModifiers`
- This triggers `stat_recalculation_system` via `Changed<AttributeModifiers>`

**Consideration**: Other systems may also write to AttributeModifiers (horde leader buffs). Need to ensure they compose. Options:
- Option A: TimedModifiers writes to AttributeModifiers; horde buffs add on top
- Option B: Separate `SpellModifiers` component that stat_recalc reads alongside AttributeModifiers
- **Recommend Option A** — simpler, and horde buffs can be TimedModifiers too (with very long duration)

### 3c. Tick system

**System: `tick_timed_modifiers`** (on TurnEndEvent):
- Decrement `turns_remaining` for all modifiers
- Remove expired modifiers from the Vec
- When a modifier is removed, `apply_timed_modifiers` will recalculate on next frame

### 3d. Wire Buff/Debuff in handle_cast_spell

**File: `src/game/magic.rs` — `handle_cast_spell`**:
- `Buff { attribute, amount, duration }`:
  - Get or insert `TimedModifiers` on target entity
  - Remove any existing modifier with same `source_spell` (refresh, don't stack)
  - Push new `TimedModifier { attribute, amount, duration, source_spell }`
  - Log: "{target} gains {spell_name}! (+{amount} {attribute} for {duration} turns)"
- `Debuff { attribute, amount, duration }`:
  - Same as Buff but amount is negative
  - Log: "{target} is weakened! (-{amount} {attribute} for {duration} turns)"

### 3e. Add buff/debuff spells to spells.ron

Add to `assets/spells.ron`:
- `enrage`: Castor, Buff STR +4, 6 turns, 8 mana, 10 cd
- `fortify`: Castor, Buff CON +3, 10 turns, 8 mana, 12 cd
- `iron_skin`: Castor, Buff armor +3, 10 turns, 12 mana, 15 cd
- `battle_hymn`: AllyOrSelf, Buff STR +2 + Buff AGI +2, 8 turns, 15 mana, 15 cd
- `arcane_surge`: Castor, Buff INT +6, 8 turns, 20 mana, 20 cd
- `weaken`: Enemy, Debuff STR -3, 8 turns, 8 mana, 10 cd
- `curse`: Enemy, Debuff STR -2 + Debuff DEX -2 + Debuff CON -2, 10 turns, 18 mana, 15 cd

Add corresponding `SpellKind` variants to `src/game/spells.rs`.

**Verification**: Player can cast enrage → see STR +4 in character info for 6 turns → modifier disappears. Weaken reduces enemy damage output.

---

## Phase 4: Haste & Slow

**Goal**: +50%/-50% speed effects as standalone components (not stat buffs).

### 4a. Components

**File: `src/game/magic.rs`** (or `status_effects.rs`):
```rust
#[derive(Component, Debug, Clone, Reflect)]
pub struct Hasted { pub turns_remaining: u32 }

#[derive(Component, Debug, Clone, Reflect)]
pub struct Slowed { pub turns_remaining: u32 }
```

### 4b. Speed effect system

**System: `apply_speed_effects`** — runs AFTER `sync_action_speed_system` in `src/game/stats.rs`:
```rust
fn apply_speed_effects(mut query: Query<(&mut SpeedStats, Option<&Hasted>, Option<&Slowed>)>) {
    for (mut speed, hasted, slowed) in query.iter_mut() {
        if hasted.is_some() { speed.delay *= 0.5; }
        if slowed.is_some() { speed.delay *= 1.5; }
        speed.delay = speed.delay.clamp(0.5, 2.0);
    }
}
```

Register in StatsPlugin after `sync_action_speed_system`.

### 4c. Tick system

**System: `tick_speed_effects`** (on TurnEndEvent):
- Decrement `turns_remaining` on Hasted/Slowed
- Remove component when expired (commands.entity(e).remove::<Hasted>())

### 4d. Wire in handle_cast_spell

- `ApplyHaste { duration }`: insert `Hasted { turns_remaining: duration }` on target
- `ApplySlow { duration }`: insert `Slowed { turns_remaining: duration }` on target
- If component already exists, refresh duration (don't stack)

### 4e. Add spells to spells.ron

- `haste`: Castor, ApplyHaste 8 turns, 10 mana, 12 cd
- `haste_ally`: Ally, ApplyHaste 8 turns, 12 mana, 10 cd
- `slow`: Enemy, ApplySlow 8 turns, 10 mana, 10 cd

**Verification**: Cast haste → player acts twice as often. Cast slow on troll → it acts half as often. Effects expire after 8 turns.

---

## Phase 5: Poison

**Goal**: Damage-over-time status effect.

### 5a. Component

```rust
#[derive(Component, Debug, Clone, Reflect)]
pub struct Poisoned { pub damage_per_turn: i32, pub turns_remaining: u32 }
```

### 5b. Tick system

**System: `process_poison`** (on TurnEndEvent):
- For each entity with `Poisoned`: apply damage directly to `Health.current`, log "{entity} takes {N} poison damage"
- Decrement turns_remaining; remove when expired
- Check for death (emit DeathEvent if HP ≤ 0)

### 5c. Wire in handle_cast_spell

- `ApplyPoison { damage_per_turn, duration }`: insert `Poisoned` on target. If already poisoned, refresh duration and use higher damage_per_turn.

### 5d. Add poison_bolt to spells.ron

- `poison_bolt`: Enemy, Damage 1d4 + ApplyPoison(2/turn, 4 turns), 12 mana, 6 cd

**Verification**: Cast poison_bolt → enemy takes 1d4 immediately + 2 damage per turn for 4 turns. Log shows each tick.

---

## Phase 6: Mana Drain

**Goal**: Remove mana from target, add to caster.

### Wire in handle_cast_spell

- `DrainMana { amount, int_scaling }`:
  - Calculate drain: `amount + (int_scaling ? int_bonus : 0)`
  - Get target's Mana component; subtract drain (clamp to 0)
  - Get caster's Mana component; add drained amount (clamp to max)
  - Log: "{caster} drains {N} mana from {target}!"

### Add mana_drain to spells.ron

- `mana_drain`: Enemy, DrainMana 15 (INT scaling), 10 mana, 8 cd

**Verification**: Cast mana_drain on caster enemy → their mana drops, yours increases. Net positive at INT 14+.

---

## Phase 7: Spirit Shield

**Goal**: Damage redirected to mana instead of HP.

### 7a. Component

```rust
#[derive(Component, Debug, Clone, Reflect)]
pub struct SpiritShielded { pub turns_remaining: u32 }
```

### 7b. Intercept damage

**File: `src/game/combat.rs` — `damage_application_system`**:
- Before subtracting from `health.current`, check for `SpiritShielded`
- If present: subtract damage from `mana.current` instead
  - If mana runs out mid-hit, overflow goes to HP
  - Log: "{entity}'s spirit shield absorbs {N} damage! (Mana: {current}/{max})"

### 7c. Tick system

- Decrement `turns_remaining` on TurnEndEvent; remove when expired

### 7d. Wire in handle_cast_spell + add to spells.ron

- `spirit_shield`: Castor, SpiritShield 10 turns, 20 mana, 25 cd

**Verification**: Cast spirit_shield → take damage → mana decreases instead of HP. When mana hits 0, remaining damage hits HP. Expires after 10 turns.

---

## Phase 8: Teleport & Blink

**Goal**: Movement spells for repositioning.

### 8a. Blink (controlled, range=3)

**Wire in handle_cast_spell**:
- `Teleport { range }` where range > 0:
  - Need tile targeting (not entity targeting). This requires a new targeting mode.
  - Add `TargetingMode::Tile { range: i32 }` to targeting.rs
  - In handle_targeting_input: allow confirming on any **walkable, unoccupied** tile within range
  - On confirm: move caster's `Position` to target tile, mark viewshed dirty
  - Log: "{caster} blinks to ({x}, {y})!"

**File: `src/game/targeting.rs`**:
- Add `TargetingMode::Tile { range }` variant
- In `handle_targeting_input` confirm: validate tile is walkable + in range + unoccupied
- In `setup_targeting`: for Tile mode, start cursor at player position

**File: `src/game/turns.rs` — `handle_player_input`**:
- For spells with `Teleport` effect where range > 0: enter `TargetingMode::Tile { range }`

### 8b. Teleport (random, range=0)

- `Teleport { range: 0 }`:
  - No targeting needed (Castor target)
  - Pick a random walkable, unoccupied tile on the map
  - Move caster there
  - Log: "{caster} teleports to ({x}, {y})!"

### 8c. Add to spells.ron

- `blink`: Castor, Teleport range=3, 8 mana, 6 cd
- `teleport`: Castor, Teleport range=0, 15 mana, 20 cd

**Verification**: Cast blink → cursor appears → select tile within 3 → player moves there. Cast teleport → player appears at random location.

---

## Phase 9: AoE Damage (Fireball, Meteor)

**Goal**: Area-of-effect damage centered on a target tile.

### 9a. Tile targeting for AoE

Reuse `TargetingMode::Tile { range }` from Phase 8. For AoE spells, show a **radius highlight** around the cursor (visual indicator of blast area).

**File: `src/game/targeting.rs`**:
- Add optional `radius: i32` to Tile targeting mode
- In `sync_cursor_to_context`: spawn/update highlight sprites for all tiles within radius

### 9b. Tile targeting for AoE spells

AoE spells need to target a **tile**, not an entity, so the player can position the blast precisely.

**File: `src/game/turns.rs` — `handle_player_input`**:
- For spells with `AoeDamage` effect: enter `TargetingMode::Tile { range, radius }` instead of entity targeting
- The player picks the center tile; the radius highlight shows the blast area

**Note on targeting**: CastSpellMessage currently takes an Entity target. For tile-targeted spells, we need either:
- Option A: Store the tile position in the message (add `target_pos: Option<Position>`)
- Option B: Create a temporary invisible entity at the tile position
- **Recommend Option A** — cleaner, add `target_pos: Option<(i32, i32)>` to CastSpellMessage

### 9c. Wire AoeDamage in handle_cast_spell

- `AoeDamage { dice, radius, int_scaling }`:
  - Get target tile position from `CastSpellMessage.target_pos`
  - Find all entities with `Position` within `radius` Manhattan distance of target tile
  - For each: roll damage, send `ApplyDamageMessage`
  - **Friendly fire**: AoE hits ALL entities in the blast area (including the player and allies). This is intentional — positional play should matter.
  - Log: "{caster} casts Fireball! {N} creatures hit for {dmg} damage!"

### 9d. Tile effect propagation system

Rather than hard-coding "fireball destroys doors", implement a generic system where spell effects can affect tiles:

**File: `src/game/magic.rs` (or new `src/game/tile_effects.rs`)**:
- `TileEffectMessage { position: (i32, i32), effect: TileEffect }`
- `TileEffect` enum: `DestroyDoor`, `IgniteTerrain` (future), etc.
- `handle_tile_effects` system: reads `TileEffectMessage`, modifies `Map` tiles accordingly
- AoeDamage handler emits `TileEffectMessage` for each tile in the blast area (fireball emits `DestroyDoor` for Door tiles)

This keeps spell resolution clean and makes it easy to add future tile interactions (lava + ice = steam, lightning + water = chain, etc.).

### 9e. Add to spells.ron

- `fireball`: Tile-targeted, AoeDamage 2d6 radius=1, 22 mana, 8 cd
- `meteor`: Tile-targeted, AoeDamage 3d8 radius=1, 35 mana, 10 cd

**Verification**: Cast fireball at a tile → all creatures in 3×3 take 2d6 (including player if in range). Door in blast radius is destroyed via tile effect system.

---

## Phase 10: Chain Lightning

**Goal**: Damage that jumps between nearby enemies.

### Wire ChainDamage in handle_cast_spell

- `ChainDamage { dice, max_jumps, jump_range, int_scaling }`:
  - Roll damage for primary target, send ApplyDamageMessage
  - Find nearest enemy within `jump_range` tiles of primary target (excluding already-hit entities)
  - Roll secondary dice (1d6), send ApplyDamageMessage
  - Repeat up to `max_jumps` times, each time finding nearest unhit enemy within `jump_range` of the last-hit entity
  - Log each jump: "Lightning arcs to {target} for {dmg} damage!"

### Add to spells.ron

- `chain_lightning`: Enemy, ChainDamage 2d6 + 2 jumps(1d6) within 2 tiles, 25 mana, 8 cd, INT scaling

**Verification**: Cast chain_lightning at goblin pack → primary takes 2d6+INT, 2 nearby take 1d6 each.

---

## Phase 11: Ally & AllyOrSelf Targeting

**Goal**: Enable heal_ally, haste_ally, cure_wounds, battle_hymn, greater_heal targeting.

### 11a. Target resolution

Target resolution is NOT in `handle_cast_spell`. The target is already decided before the spell message is sent:
- **Player**: picks a target via cursor targeting UI (TargetingMode::SpellAlly)
- **Monster AI**: `choose_spell` in `ai.rs` selects the best target as part of scoring

`handle_cast_spell` receives a `CastSpellMessage` with the target already resolved — it just applies the effect.

### 11b. Targeting UI for allies

**File: `src/game/targeting.rs`**:
- Add `TargetingMode::SpellAlly { slot, include_self }` variant
- In `handle_targeting_input` confirm: validate target is a friendly entity (or self if include_self)
- For `AllyOrSelf`, player is also a valid target

**File: `src/game/turns.rs` — `handle_player_input`**:
- For Ally/AllyOrSelf target spells: enter `TargetingMode::SpellAlly`

### 11c. Monster AI scoring for ally spells

**File: `src/game/ai.rs` — `choose_spell`**:
- Add scoring branch for `Ally` / `AllyOrSelf` targets:
  - For Heal effects: score = heal_amount × 2 × (missing_hp / max_hp)
  - Find most-wounded ally within range
  - For Buff effects: score based on buff value (use same stat-turns logic)
  - For `AllyOrSelf`: also consider self as candidate

### 11d. Add ally spells to spells.ron

- `heal_ally`: Ally, Heal 2d4 INT-scaled, 12 mana, 5 cd
- `cure_wounds`: AllyOrSelf, Heal 2d6 INT-scaled, 15 mana, 6 cd
- `greater_heal`: AllyOrSelf, Heal 3d8 INT-scaled, 25 mana, 8 cd
- `haste_ally`: Ally, ApplyHaste 8 turns, 12 mana, 10 cd
- `battle_hymn`: AllyOrSelf, Buff STR +2 + Buff AGI +2, 8 turns, 15 mana, 15 cd

**Verification**: Goblin Shaman AI heals wounded goblin ally. Player can target self with cure_wounds or an NPC ally (future).

---

## Phase 12: Remaining Attack Spells

**Goal**: Fill out the full attack spell roster.

### Add to spells.ron + SpellKind enum

All of these use existing `Damage` effect with varying dice/costs:
- `spark`: Enemy, Damage 1d4, 3 mana, 0 cd, no INT scale
- `ice_shard`: Enemy, Damage 2d4, 10 mana, 4 cd, INT scale
- `shadow_bolt`: Enemy, Damage 2d8, 18 mana, 5 cd, INT scale
- `lightning_bolt`: Enemy, Damage 3d6, 20 mana, 6 cd, INT scale
- `death_coil`: Enemy, Damage 4d6, 30 mana, 8 cd, INT scale
- `vampiric_strike`: Enemy, Damage 2d4 + Heal 1d4, 12 mana, 4 cd (already works with existing multi-effect)

Update existing `magic_missile` and `fire_dart` definitions if needed.

**Verification**: All 12 attack spells appear in spells.ron and can be cast.

---

## Phase 13: Spellbook Items & Learning

**Goal**: Ensure players can find and learn all 31 spells through Spellbook items.

### 13a. Add spellbook items to items.ron

For each learnable spell, add a Spellbook item entry:
```ron
"tome_of_fireball": ItemAsset(
    name: "Tome of Fireball",
    sprite: "items/books.png#2",
    item_kind: Spellbook,
    rarity: Rare,
    effects: [LearnSpell("fireball")],
),
```

### 13b. Add to item_spawns.ron

Set min_floor/max_floor per spellbook matching the zone progression table:
- Zone 1 (1-5): spark, magic_missile, minor_heal, fire_dart
- Zone 2 (6-10): ice_shard, poison_bolt, vampiric_strike, heal_self, heal_ally, enrage, fortify, weaken, haste, blink
- Zone 3 (11-16): shadow_bolt, lightning_bolt, fireball, chain_lightning, cure_wounds, haste_ally, iron_skin, battle_hymn, slow
- Zone 4 (17-21): death_coil, greater_heal, arcane_surge, spirit_shield, curse, mana_drain, teleport
- Zone 5 (22-26): meteor

### 13c. Verify LearnSpell flow

The `Effect::LearnSpell(SpellKind)` → `handle_use_item` → auto-slot flow already exists. Verify it works for all new SpellKind variants.

**Verification**: Find Tome of Fireball → use from inventory → spell appears in known spells → equip to slot → cast fireball.

---

## Phase 14: AI Updates for All Spell Types

**Goal**: Monster AI can intelligently use buffs, debuffs, haste, and all new spell categories.

### Update choose_spell in ai.rs

Add scoring branches for each new effect type:
- **Buff (self)**: score = amount × duration / 4 (valuable pre-combat)
- **Debuff (enemy)**: score = amount × duration / 4
- **ApplyHaste (self/ally)**: score = 15 (high fixed value — speed is very powerful)
- **ApplySlow (enemy)**: score = 12
- **ApplyPoison**: score = damage_per_turn × duration
- **DrainMana**: score = drain_amount if target has mana, else 0
- **SpiritShield**: score = 10 if HP < 50%, else 3
- **Teleport**: score = 0 for monsters (don't use — or small score for Imp blink)
- **AoeDamage/ChainDamage**: query all entities with `Position` within spell radius of the target tile. Score = `(single_target_damage × enemy_count) - (single_target_damage × ally_count)`. Negative score means the spell would hurt more allies than enemies — AI should never cast it in that case

Apply same normalization: `effective = raw / (sqrt(mana_cost) * ln(cd + 1))`

**Verification**: Goblin Warchief casts enrage before engaging. Orc Warlord casts battle_hymn on nearby ally. Shadow Fiend uses mana_drain on player.

---

## Phase 15: Save/Load Integration

Per the save checklist, ensure all new components are persisted:
- `ManaRegen` — player + monsters (via PendingPlayerLoad + monster spawn)
- `TimedModifiers` — player (active buffs/debuffs survive save/load)
- `Hasted` / `Slowed` — player
- `Poisoned` — player
- `SpiritShielded` — player
- `SpellCooldowns` — already saved? Verify.

Update `GameSaveData`, `auto_save_system`, `apply_player_load_system`, and `CachedFloor`/`CachedFloorSave` as needed.

---

## Phase Summary & Dependencies

```
Phase 1  ─── SpellTarget/SpellEffect refactor (FOUNDATION — everything depends on this)
  │
  ├── Phase 2  ─── Mana regen overhaul (independent)
  │
  ├── Phase 3  ─── Buff/Debuff system
  │     │
  │     └── Phase 4  ─── Haste/Slow (uses similar pattern, needs speed system hook)
  │
  ├── Phase 5  ─── Poison (independent status effect)
  │
  ├── Phase 6  ─── Mana Drain (simple effect, no new components)
  │
  ├── Phase 7  ─── Spirit Shield (needs combat.rs integration)
  │
  ├── Phase 8  ─── Teleport/Blink (needs tile targeting in targeting.rs)
  │     │
  │     └── Phase 9  ─── AoE Damage (reuses tile targeting from Phase 8)
  │
  ├── Phase 10 ─── Chain Lightning (standalone effect logic)
  │
  ├── Phase 11 ─── Ally/AllyOrSelf targeting (needs AI + targeting.rs updates)
  │
  └── Phase 12 ─── Remaining attack spells (data-only, uses existing Damage effect)

Phase 13 ─── Spellbook items (data-only, after all spells exist)
Phase 14 ─── AI scoring updates (after all effects are implemented)
Phase 15 ─── Save/Load (after all new components exist)
```

**Parallelizable**: Phases 2-12 are all independent once Phase 1 is complete. They can be implemented in any order.

**Recommended order for fastest playable results**:
1. Phase 1 (foundation)
2. Phase 12 (attack spells — immediately adds variety with existing Damage effect)
3. Phase 2 (mana regen — changes game balance)
4. Phase 3 → 4 (buffs/debuffs → haste/slow — big gameplay impact)
5. Phase 5 (poison)
6. Phase 11 (ally targeting — enables shaman healers)
7. Phase 8 → 9 (teleport → AoE)
8. Phase 10 (chain lightning)
9. Phase 6, 7 (mana drain, spirit shield)
10. Phase 13, 14, 15 (items, AI, save/load)

---

## Files Modified (Complete List)

| File | Phases |
|------|--------|
| `src/game/spells.rs` | 1, 12 |
| `src/game/magic.rs` | 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11 |
| `src/game/ai.rs` | 1, 11, 14 |
| `src/game/turns.rs` | 1, 4, 8, 11 |
| `src/game/targeting.rs` | 8, 9, 11 |
| `src/game/combat.rs` | 7 |
| `src/game/stats.rs` | 4 |
| `src/game/spawner.rs` | 2 |
| `src/game/mod.rs` | 3, 4, 5 (register new systems) |
| `src/save/mod.rs` | 15 |
| `assets/spells.ron` | 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12 |
| `assets/items.ron` | 13 |
| `assets/item_spawns.ron` | 13 |
