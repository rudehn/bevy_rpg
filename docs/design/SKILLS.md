# Skills (Phase 3)

## Design Philosophy

Phase 2 anchored the modifier scale at 16, so chargen mods are typically
negative and players grow into positive values via attribute gains.
**Phase 3 layers a use-trained skill system on top** — DCSS-faithful in
structure but pared down to 8 skills. Each skill tracks a float in
`[0.0, 27.0]`; effects unlock at integer breakpoints. A single global
**skill XP pool** distributes XP across skills the player has marked for
training, with **Auto** (use-weighted) and **Manual** (even split,
focus-able) modes.

This phase also fills in the missing Fighting term in the HP formula
from Phase 2.

**Phase 3 in scope:**

- 8 skills: Fighting / Axes / Short Blades / Long Blades / Ranged Weapons
  / Armor / Dodging / Evocations
- Float levels 0.0–27.0; integer breakpoints drive combat math
- DCSS shared-pool XP with a training screen on key `M`
- Class starting-skill distributions (10 points each, negatives allowed
  in schema)
- Per-race aptitudes (XP-cost multipliers per skill)
- HP formula gains the Fighting term
- Evocations replaces the inline `INT_mod` clamp on staff zaps

**Explicitly deferred:**

- Cross-training between weapon families (Short ↔ Long Blades, the
  Maces/Axes/Polearms/Staves chain)
- Per-class aptitudes — **explicitly out of scope.** Race aptitudes
  alone carry skill-training identity. Adding a parallel set of class
  aptitudes would double the balance knobs without enough payoff;
  class identity is already expressed through `starting_skills`
- ~~Skill targets~~ — **shipped as a Phase 3 follow-up.** Press `=` on
  the focused row to type a target level; the skill auto-disables when
  it reaches that level. See §8 below.
- Magic schools (Fire / Lightning / Poison / Restoration) — land with
  Phase 4 mana / spells
- Attack-speed scaling from weapon skills (DCSS's "minimum delay"
  mechanic) — first pass uses hit/damage only
- Stealth, Shields, Unarmed — not in the initial roster

## Locked Decisions

| Decision | Choice |
|---|---|
| Skill list | 8 — Fighting, Axes, Short Blades, Long Blades, Ranged Weapons, Armor, Dodging, Evocations |
| Skill scale | float `[0.0, 27.0]` |
| XP mechanism | DCSS shared-pool with training screen (Auto / Manual modes) |
| Per-skill state | Normal / Focused / Disabled |
| Aptitudes | per-race, range −5 to +5 (formula `2^(-apt/4)` for XP multiplier) |
| Cross-training | deferred to a follow-up |
| Class starting skills | 10 points per class, negatives allowed in schema |
| HP Fighting term | `+ Fighting × XL/14 + (1 + Fighting × 3) / 2` inside the race_hp_mod multiplier |
| Evocations + staves | replaces the inline `int_mod.max(0)` clamp; staff damage adds `int_mod + floor(evocations/4)`, sum clamped at 0 |
| Skill effect formula | `+ floor(skill/4)` to the corresponding combat stat (hit, damage, armor, dodge) — `+1` at skill 4, `+6` at skill 27 |
| UI key | `M` opens the skill screen |
| Save schema bump | v4 → v5 |

---

# Detailed Design

## 1. Skill Definitions

| Skill | Affects | Effect Formula |
|---|---|---|
| **Fighting** | HP + melee hit & damage | HP formula gains `+ Fighting × XL/14 + (1 + Fighting × 3) / 2` inside the race_hp_mod multiplier. Plus `+ floor(Fighting/4)` to every melee hit roll and melee damage roll. |
| **Axes** | hit & damage with axe-type weapons | `+ floor(Axes/4)` to hit and damage when an axe-type weapon is equipped (`item_kind == Weapon` AND a new `weapon_skill: Axes` field on `ItemAsset`) |
| **Short Blades** | hit & damage with short blades | Same shape; applies for Dagger / Rusted Shortsword / Throwing Knife |
| **Long Blades** | hit & damage with long blades | Same shape; applies for Sword |
| **Ranged Weapons** | hit & damage with ranged | Same shape; fires on any attack where `AttackIntentMessage.source == Ranged` |
| **Armor** | flat armor bonus when wearing armor | `+ floor(Armor/4)` added to `Armor.0` whenever a chest armor piece is equipped |
| **Dodging** | flat dodge bonus | `+ floor(Dodging/4)` added to `Dodge.0` |
| **Evocations** | staff zap damage | Replaces the existing inline `int_mod.max(0)` add in `handle_zap_staff`. New formula: `staff_damage += (int_mod + floor(evocations/4)).max(0)`. Applies to all damage-dealing staff effects (Lightning, Fire, Force; not Healing/Blinking). |

**Why `floor(skill/4)`:** keeps integer breakpoints meaningful, matches
the DCSS rhythm of "every 4 levels = +1 mechanical step", and avoids
fractional-bonus stacking in damage math.

## 2. Class Starting Skill Distributions (10 points each)

| Skill | Warrior | Rogue | Mage | Ranger |
|---|---|---|---|---|
| Fighting | 3 | 1 | 0 | 2 |
| Axes | 2 | 0 | 0 | 0 |
| Short Blades | 0 | 4 | 1 | 0 |
| Long Blades | 3 | 0 | 0 | 1 |
| Ranged Weapons | 0 | 1 | 0 | 4 |
| Armor | 2 | 0 | 0 | 1 |
| Dodging | 0 | 3 | 2 | 2 |
| Evocations | 0 | 1 | 7 | 0 |
| **Total** | **10** | **10** | **10** | **10** |

Authored in `assets/classes.ron` as a new `starting_skills` field of
type `SkillDistribution` (i32 per skill, schema allows negatives).
Initial data has no negatives.

## 3. Per-Race Aptitudes

DCSS-style XP-cost multipliers: `xp_multiplier(apt) = 2^(-apt/4)`.

- Aptitude +4 → 0.5× XP (twice as fast)
- Aptitude  0 → 1.0× XP (baseline)
- Aptitude −4 → 2.0× XP (twice as slow)

| Skill | Human | Dwarf | Elf |
|---|---|---|---|
| Fighting | 0 | +2 | −1 |
| Axes | 0 | +3 | −2 |
| Short Blades | 0 | 0 | +1 |
| Long Blades | 0 | +1 | +2 |
| Ranged Weapons | 0 | −2 | +3 |
| Armor | 0 | +3 | −2 |
| Dodging | 0 | −2 | +2 |
| Evocations | 0 | −1 | +2 |

Authored in `assets/races.ron` as a new `aptitudes` field (i32 per skill).

## 4. XP Pool, Training Modes, and Allocation

### Data

```rust
#[derive(Component)]
pub struct Skills(pub HashMap<Skill, f32>);   // level per skill, 0.0..=27.0

#[derive(Resource)]
pub struct SkillXpPool(pub u64);              // unallocated XP

#[derive(Resource)]
pub struct TrainingMode(pub Mode);            // Auto | Manual (global)

#[derive(Component)]
pub struct SkillTraining(pub HashMap<Skill, SkillState>);
// SkillState: Normal | Focused | Disabled

#[derive(Resource, Default)]
pub struct SkillUseCounters(pub HashMap<Skill, u32>);
// Auto-mode use weights; decay 10% per floor to prevent ancient-skill drift
```

### XP source

Skill XP is granted on **monster death** alongside character XP, in a
fixed ratio (e.g., `skill_xp_pool += xp_reward / 2`). Tuning constant
lives in `src/game/skills.rs`.

### Allocation

When `SkillXpPool > 0`, a system runs per game tick:

1. **Manual mode:** find all skills in `Normal` or `Focused` state.
   - `Normal` skills get 1 share; `Focused` skills get 2 shares.
   - Drain pool, distribute proportionally as raw skill XP.
2. **Auto mode:** find all skills in `Normal` or `Focused` state with
   `SkillUseCounters.get(skill) > 0`.
   - Each skill's share = its counter (×2 if `Focused`).
   - Distribute proportionally.
   - If no skill has a counter (e.g., player is moving without combat),
     the XP stays pooled — it doesn't decay.
3. For each share, **the raw XP is divided by the race's aptitude
   multiplier for that skill** before applying.
4. Skill level updates per the DCSS XP-to-level table (50 points for
   level 1, 24,325 points for level 27, exponential curve in between).
   Level is computed by binary-search through the table.
5. `SkillUseCounters` decay 10% per floor transition.

### Logging breakpoints

When a skill crosses an integer level (e.g., 3.9 → 4.0), emit a
`SkillLevelUpEvent` and log `"Your Long Blades skill improves to 4."` in
the game log. No particle (too noisy with frequent breakpoints).

## 5. Skill Use Counters

Each gameplay event that "uses" a skill increments a counter:

| Skill | Use trigger |
|---|---|
| Fighting | Any melee attack lands |
| Axes | Melee attack with axe equipped |
| Short Blades | Melee attack with short-blade equipped |
| Long Blades | Melee attack with long-blade equipped |
| Ranged Weapons | Any ranged attack |
| Armor | Damage taken while wearing armor (`Armor.0 > 0`) |
| Dodging | Successful dodge (the d20 miss condition) |
| Evocations | Staff zap fired (damaging effect) |

Use counters drive Auto-mode XP allocation. Counters never directly
become skill levels — they only weight the XP split.

## 6. HP Formula Update

```
max_hp = floor(race_hp_mod × (
    8
    + 11 × XL / 2
    + Fighting × XL / 14
    + (1 + Fighting × 3) / 2
))
```

The two Fighting terms are the DCSS formula faithfully. At Fighting 0
the formula reduces to the Phase 2 baseline.

**Worked values** (Human, `race_hp_mod = 1.00`):

| XL | Fighting 0 | Fighting 5 | Fighting 15 | Fighting 27 |
|---|---|---|---|---|
| 1 | 13 | 16 | 24 | 35 |
| 9 | 57 | 64 | 81 | 102 |
| 18 | 107 | 121 | 156 | 198 |
| 27 | 156 | 178 | 233 | 297 |

Stoneblood and Keen Senses unaffected.

## 7. Weapon → Skill Mapping

`ItemAsset` gains:

```rust
#[serde(default)]
pub weapon_skill: Option<WeaponSkill>,
```

```rust
pub enum WeaponSkill {
    Axes,
    ShortBlades,
    LongBlades,
    Ranged,
}
```

| Item | weapon_skill |
|---|---|
| Sword | LongBlades |
| Dagger | ShortBlades |
| Rusted Shortsword | ShortBlades |
| Throwing Knife | ShortBlades (melee) / Ranged (when thrown) — see note |
| Axe | Axes |
| Bow | Ranged |
| Shortbow | Ranged |
| (Staves) | none — staff zap uses Evocations; melee bash uses Fighting alone |

**Throwing Knife edge case:** when used as a melee weapon, it counts as
Short Blades; when thrown (ranged source), it counts as Ranged. The
attack-source branching in `hit_check_system` already distinguishes
these, so the weapon-skill lookup just branches on `source`.

A weapon without `weapon_skill` (e.g., a staff melee-bash) gets no
weapon-skill bonus — only Fighting.

## 8. Skill Screen (Key `M`)

New `InGameState::SkillScreen`. Toggled by `M` (currently unbound
in-game; add to help screen).

### Layout

```
                    SKILLS

  Mode: [Auto]                          XP pooled: 1,247

  Skills:
    [+] Fighting        4.7  →  5.0    (cost 350,  apt +2)
    [*] Long Blades     2.3  →  3.0    (cost 280,  apt +1)
    [+] Short Blades    0.0  →  1.0    (cost 50)
    [-] Axes            0.0  →  1.0    (DISABLED)
    [+] Ranged Weapons  0.0  →  1.0    (cost 100,  apt −2)
    [+] Armor           0.0  →  1.0    (cost 50,   apt +3)
    [+] Dodging         0.0  →  1.0    (cost 100,  apt −2)
    [+] Evocations      0.0  →  1.0    (cost 65,   apt −1)

  [↑/↓] navigate · [Enter] cycle state · [/] toggle Auto/Manual · [M/Esc] close
```

- `[+]` = Normal, `[*]` = Focused, `[-]` = Disabled (DCSS conventions)
- Cost column shows raw XP to next integer level *after aptitude*
- Live updates as the player toggles states (preview of new allocation
  split)

### State cycle

Each Enter press on the focused row cycles: Normal → Focused → Disabled
→ Normal.

### Mode toggle

`/` flips global `TrainingMode` between Auto and Manual. State
transitions: Auto mode treats every Normal/Focused skill as active
weighted by counters; Manual mode weights are 1 (Normal) or 2 (Focused).

### Skill targets

Press `=` with a skill focused to enter target-input mode: type a
1–2-digit level (1–27), then `Enter` to confirm or `Esc` to cancel.
Each row shows the active target as `→N` in a small column between
level and aptitude.

- Setting a target on a `Disabled` skill auto-flips it to `Normal` so
  the target is actually reachable.
- Entering `0` clears the target.
- When `Skills::get(skill) >= target`, the `enforce_skill_targets`
  system flips the skill to `Disabled`, clears the target, and logs
  `"Long Blades reaches target level 4 — auto-disabled."`
- Targets persist in `SkillTraining.targets: HashMap<Skill, u32>`
  and round-trip through the save file.

Common pattern: queue a sequence of targets by setting them all up-front
(`Fighting → 4`, `Long Blades → 4`, `Dodging → 6`) and let the
auto-disable do the work as each milestone hits. The player only has
to revisit the screen when they want a new target.
Disabled is identical in both modes.

## 9. Combat Math Integration

Skill bonuses are added **dynamically** at hit-check / damage-roll time,
alongside the existing `attack_attribute_bonus`. The order:

```
hit_roll = d20
         + HitBonus.0                        // equipment + class
         + attack_attribute_bonus(source)    // STR/DEX/INT mod
         + weapon_skill_bonus(equipped, skills)  // floor(Long Blades/4), etc.
         + floor(Fighting / 4)               // melee only
```

```
damage = weapon_dice_roll
       + DamageBonus.0
       + attack_attribute_bonus(source)
       + weapon_skill_bonus(equipped, skills)
       + floor(Fighting / 4)                 // melee only
```

A new pure helper `weapon_skill_bonus(weapon: Option<&ItemProperties>,
skills: &Skills, source: DamageSource) -> i32` returns the right skill
contribution for the weapon + attack source. Returns 0 for monsters (no
`Skills` component).

## 10. Save Schema v4 → v5

`PlayerSaveData` adds:

```rust
#[serde(default)]
pub skills: HashMap<Skill, f32>,         // skill levels
#[serde(default)]
pub skill_xp_pool: u64,                  // unallocated XP
#[serde(default)]
pub training_mode: Mode,                 // Auto | Manual
#[serde(default)]
pub skill_training: HashMap<Skill, SkillState>,
#[serde(default)]
pub skill_use_counters: HashMap<Skill, u32>,
```

Pre-v5 saves load with empty maps and 0 pool — equivalent to "no
skills trained yet." Existing gameplay unaffected; the player just
needs to start training. `MigrateV4ToV5` is a no-op.

`SAVE_SCHEMA_VERSION` bumps to 5.

## 11. Files to Create / Modify

**New:**
- `src/game/skills.rs` — `Skill` enum, `Skills` component, `SkillXpPool`
  resource, training-mode resources/components, XP allocation system,
  level-up bookkeeping, pure helpers (`weapon_skill_bonus`, `xp_to_level`,
  aptitude multiplier).
- `src/ui/skill_screen.rs` — `InGameState::SkillScreen` UI.
- Maintenance contract entries in `docs/design/SKILLS.md` (this file).

**Modified:**
- `src/character/asset.rs` — `ClassAsset` gains `starting_skills:
  SkillDistribution`. `RaceAsset` gains `aptitudes: HashMap<Skill, i32>`.
  (Or two flat-typed structs paralleling `AttributeDistribution`.)
- `src/character/attributes.rs` — `max_hp_for_level` gains a `fighting:
  f32` parameter. Existing callers pass 0.0 until they have a `Skills`
  component to read from.
- `src/assets/mod.rs` — `ItemAsset` gains `weapon_skill: Option<WeaponSkill>`.
- `src/game/combat.rs` — `hit_check_system` and `damage_roll_system`
  add `weapon_skill_bonus(...)` and `floor(Fighting/4)` (melee only)
  to their formulas.
- `src/game/staves.rs` — `handle_zap_staff` adds `floor(Evocations/4)`
  alongside `int_mod`, sum clamped at 0.
- `src/game/xp.rs` — `award_xp_on_death` ALSO bumps `SkillXpPool` by
  `xp_reward / 2`. `handle_level_up` recomputes HP using the new
  Fighting-aware formula.
- `src/player/mod.rs` — spawner inserts `Skills`, `SkillTraining`,
  `SkillXpPool`-related defaults from the class's starting skill
  distribution.
- `src/game/mod.rs` — adds `InGameState::SkillScreen`.
- `src/save/mod.rs` — v4 → v5 migration + new PlayerSaveData fields.
- `src/ui/mod.rs` — registers `SkillScreenPlugin`. Updates hotkey
  footer (`[I] Inv · [C] Char · [M] Skills`).
- `src/ui/help.rs` — adds `[M]` Skill screen + cycle/focus key list.
- `assets/classes.ron` — adds `starting_skills:` to each class.
- `assets/races.ron` — adds `aptitudes:` to each race.
- `assets/items.ron` — adds `weapon_skill:` to weapons.
- `docs/design/CHARACTER.md` — updates HP-formula note (Fighting term
  now active) and cross-links to SKILLS.md.
- `docs/design/PLAYER.md` — updates HP-formula example; mentions skill
  bonuses in the combat-math summary.
- `docs/design/GAME.md` — combat-math diagram annotated with skill
  contributions.
- `CLAUDE.md` — adds skills to the Phase 3 status note and to the
  game/ module list.

## 12. Maintenance Contract

Following the pattern from CHARACTER.md, SKILLS.md is **test-enforced**.
Tests in `src/game/skills.rs` (or `src/character/asset.rs`) assert:

- Every class's `starting_skills` sums to exactly 10
- Every race's `aptitudes` has an entry for every `Skill` variant
- Every weapon in `items.ron` either has `weapon_skill` declared or
  is explicitly noted as skill-less (staves)
- This doc mentions every shipping skill name

A `.claude/rules/skill-writeup-required.md` rule formalizes the convention.

## 13. Tests

**Pure-function tests:**
- `xp_to_level` — table-driven against DCSS curve at L1, L5, L10, L20, L27
- `aptitude_multiplier(apt)` — pin values from the DCSS table (+5: 0.42, 0: 1.0, −5: 2.38)
- `weapon_skill_bonus` — every weapon-kind × every relevant skill
- `floor(skill/4)` integer breakpoints

**Integration tests:**
- Spawn a character, train Long Blades via dummy use, verify level
  rises by the expected amount
- Switch mode Manual → Auto, verify XP allocation flips
- Disable a skill, verify it gets no XP even with use counters
- Focused skill gets 2× share
- Save → load preserves all skill state

**Maintenance contracts** as above.

## 14. Verification

After implementation:

1. `cargo check` / `cargo clippy` clean
2. `cargo test` passes (target: 426 → ~445)
3. Manual playtest:
   - New Human Warrior → confirm starting skills (Fighting 3, Long Blades 3, Axes 2, Armor 2)
   - Open skill screen (M), confirm 8 skills listed, modes default to Normal except disabled-by-default toggle
   - Swing the Rusted Shortsword (Short Blades 0). After ~10 kills, verify Short Blades has accrued XP in Auto mode.
   - Switch to Manual mode, Focus Fighting only. After 5 more kills, verify only Fighting trained.
   - Reach Long Blades 4 → confirm `+1 hit` and `+1 damage` show up in combat (compare a swing's damage roll before/after).
   - Spawn a Mage, zap a staff: verify Evocations 7 adds +1 damage on top of INT_mod (Mage's INT 18 → mod +1 → total +2 staff damage).
   - Save mid-run, reload → all skill state intact.
4. Read SKILLS.md end-to-end — confirm tables match `races.ron` /
   `classes.ron` / `items.ron`.

## 15. Open Questions / Deferred

- **Skill targets** (`=` key — "stop training at level X"). Useful but
  fiddly UI; defer to a follow-up.
- **Cross-training.** Short ↔ Long Blades, Axes ↔ Maces/Polearms.
  Architecturally similar to aptitudes; can layer in later without
  disrupting the v1 data model.
- **Per-class aptitudes.** Out of scope. Class identity flows through
  `starting_skills`; aptitudes belong to race.
- **Attack-speed scaling.** DCSS weapon skill reduces attack delay
  to a per-weapon minimum. We don't have a "minimum delay" concept
  yet; defer alongside the Polearms-and-friends weapon expansion.
- **Stealth / Shields / Unarmed.** Out of v1. Stealth in particular
  is a richer subsystem (requires monster detection mechanics not
  yet present).
- **Magic schools (Fire / Lightning / Poison).** Land with Phase 4 mana.
- **Per-monster skill values.** Monsters have no `Skills` component.
  Their combat numbers are authored flat. May revisit if monster
  rebalancing wants skill-like granularity.

## 16. Roadmap Position

- ✅ Phase 1: race/class/attributes (shipped)
- ✅ Phase 1.5 / 2: attribute refactor + XP/levels/stat gain (shipped)
- 🚧 **Phase 3 (this spec): skills**
- ⏭ Phase 4: mana / spell schools (will add Fire / Lightning / Poison
  / Restoration magic-school skills + Spellcasting; fills in monster
  rebalancing pressure further)
- ⏭ Saves phase (saving throws + Bleeding status)
- ⏭ Monster combat-stat rebalance (deferred from Phase 2)
- ⏭ Stealth, Shields, Unarmed skills (when their subsystems land)
