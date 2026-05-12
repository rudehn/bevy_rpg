# Character System

## Design Philosophy

The original Brogue-style design had no character creation — every run
started as the same blank adventurer, with identity emerging from gear
and enchant-scroll choices ([PLAYER.md](PLAYER.md)).

Phase 1 introduced races, classes, and four attributes (STR/DEX/CON/INT).
**Phase 2 restructures the foundation while adding XP and levels:**

1. **CON is removed** as a player attribute. HP now scales with race
   and level via a DCSS-inspired formula — there is no per-character
   "toughness stat."
2. **The modifier scale is anchored at 16** (not 10). A score of 16 is
   the +0 reference; mods at chargen are deliberately modest, often
   ranging -4 to +2. Players grow into positive mods over levels.
3. **There is no chargen attribute-point allocation.** Final scores
   are entirely the sum of `race + class` contributions.
4. **Halfling is removed.** Three races remain: Human, Dwarf, Elf.

**Phase 2 in scope today:**

- 3 races × 4 classes character creation
- HP derived from `race_hp_mod × (8 + 11 × XL / 2)`
- Attribute mods anchored at 16
- (Phase 2 second chunk, in progress) XP, levels, racial stat-gain
  schedule, player-choice ASIs, character info screen

**Deferred to later phases:**

- Saving throws (player and monster)
- Skills (DCSS use-trained Fighting/weapon/spell skills — the HP
  formula's missing Fighting term lands here)
- Mana / spells / spell schools
- Monster combat-stat rebalancing for the Phase 2 power curve

## Data Model

### Components

| Component | Fields | Notes |
|---|---|---|
| `Race` | enum: `Human`, `Dwarf`, `Elf` | Marker + lookup key for race data |
| `Class` | enum: `Warrior`, `Rogue`, `Mage`, `Ranger` | Marker + lookup key for class data |
| `Attributes` | `{ strength: i32, dexterity: i32, intelligence: i32 }` | Three attributes (CON removed in Phase 2) |

`ability_mod(score) -> i32` returns `(score - 16).div_euclid(2)`:
- 16 → 0, 14 → -1, 12 → -2, 10 → -3, 8 → -4
- 18 → +1, 20 → +2, 24 → +4, 28 → +6

### Assets

`assets/races.ron` and `assets/classes.ron`.

**Race entry (RON):**
```ron
"dwarf": (
    name: "Dwarf",
    str_bonus: 12, dex_bonus: 4, int_bonus: 8,
    hp_mod: 1.20,
    gain_schedule: (
        interval: 4,
        allowed: [Str, Int, Dex],
    ),
    description: "Hardy folk of the deep places. ...",
)
```

**Class entry (RON):**
```ron
"warrior": (
    name: "Warrior",
    attribute_distribution: (str: 8, dex: 2, int: 2),
    starting_kit: [
        (name: "Rusted Shortsword"),
        (name: "Padded Armor"),
    ],
    description: "Steel and discipline. ...",
)
```

### Spawn payload

`CharacterChoice { race: Race, class: Class }` — the character creation
screen writes this; the spawner reads it and builds the player entity.
Phase 2 has no free-point step; the chargen UI just picks race + class.

## Races

> **Maintenance contract:** every shipping race in `assets/races.ron`
> must appear in this section with its trait keyword. The
> `character_md_documents_every_shipping_race` test enforces this.
> Three races ship; Halfling was removed in Phase 2.

Each race contributes a **24-point distribution** across STR/DEX/INT,
an **HP multiplier** applied through the HP formula, a **stat-gain
schedule** that fires every N XP levels, and one **passive trait**.

| Race | STR | DEX | INT | HP × | Schedule | Trait |
|---|---|---|---|---|---|---|
| Human | 8 | 8 | 8 | 1.00 | 4:SDI (any) | **Adaptive** |
| Dwarf | 12 | 4 | 8 | 1.20 | 4:SID (any) | **Stoneblood** — 50% poison resistance |
| Elf | 4 | 10 | 10 | 0.90 | 4:DI (no STR) | **Keen Senses** — vision range +2 |

**Schedule notation** (DCSS-style):  `N:LLL` = every N XP levels, the
player gains +1 in one of the listed attribute letters (S/D/I). When
the schedule's `allowed` set has more than one letter, the player is
prompted via the ASI modal; a single-letter schedule (none currently)
would auto-apply.

**Playstyle at a glance:**

- **Human** — adaptive in every way. Even +8 spread across all three
  stats; can pick S/D/I freely at racial-schedule levels. Pairs well
  with any class.
- **Dwarf** — hardy and strong. The +12 STR distribution and 1.20 HP
  multiplier make a Dwarf Warrior the most durable opener. Stoneblood
  is a quiet life-saver on poison-heavy floors. Less DEX means a Dwarf
  Mage or Rogue will feel slow at chargen until they level into it.
- **Elf** — fragile and clever. Lowest HP multiplier (0.90), highest
  DEX+INT spread, and a schedule that never bumps STR. Pairs naturally
  with Mage or Rogue; Warrior Elf is intentionally awkward.

## Classes

> **Maintenance contract:** every class in `assets/classes.ron` must
> appear in this section with its name and its 12-point distribution.
> The `character_md_documents_every_shipping_class` and
> `every_class_distribution_sums_to_twelve` tests enforce this.

Each class allocates a **12-point distribution** across STR/DEX/INT
(no negatives in shipping data; the schema allows them for future
class designs). All class differentiation flows through the
distribution — there is no `class_attack_bonus` or `class_dodge_bonus`
fudge factor. A Warrior's hit advantage comes from their +8 STR
yielding STR_mod on melee; a Rogue's dodge advantage comes from their
+8 DEX yielding DEX_mod on dodge.

| Class | STR | DEX | INT | Starting Kit |
|---|---|---|---|---|
| Warrior | 8 | 2 | 2 | Rusted Shortsword (1d6), Padded Armor (armor 1) |
| Rogue | 2 | 8 | 2 | Dagger (1d4 +1 hit), Cloth Wraps (+1 dodge), 1 throwing knife |
| Mage | 1 | 3 | 8 | Apprentice Staff (1d3 lightning, slow recharge), Robe |
| Ranger | 3 | 8 | 1 | Shortbow (1d6, range 6), 6 arrows, Padded Armor |

**Playstyle at a glance:**

- **Warrior** — STR-driven melee. +8 STR means at chargen a Dwarf
  Warrior has STR 20 (mod +2), Human Warrior has STR 16 (mod 0). Hit
  and damage scale with STR on every swing.
- **Rogue** — DEX-driven, dodge-focused. +8 DEX scales hit/damage on
  the Dagger and on every ranged attack; also feeds Dodge.
  Intentionally fragile until you grow into INT for scroll use.
- **Mage** — INT-driven. The +8 INT yields INT_mod ≥ +0 for an Elf
  Mage at chargen; staff zap damage scales with INT_mod (clamped at 0).
  Frailest class — Elf Mage has only 12 HP at L1.
- **Ranger** — DEX-driven ranged. Bow attacks consume DEX_mod for both
  hit and damage. Six arrows to start; conserving ammo is the early
  puzzle.

## HP Formula

```
max_hp = floor(race_hp_mod × (8 + 11 × xp_level / 2))
```

Adapted from DCSS's HP formula, minus the Fighting-skill term (which
lands when the Skills phase ships). HP is recomputed from scratch on
every level-up; equipment HP bonuses layer on top via the existing
recalc.

**Worked values:**

| Race (HP ×) | L1 | L9 | L18 | L27 |
|---|---|---|---|---|
| Dwarf (1.20) | 16 | 69 | 128 | 187 |
| Human (1.00) | 13 | 57 | 107 | 156 |
| Elf (0.90) | 12 | 51 | 96 | 140 |

## Combat Math Integration

HitBonus, DamageBonus, Dodge stay as flat-value components on the
player entity. **Attribute mods for attacks are added dynamically**
at hit-check / damage-roll time, branching on `AttackIntentMessage.source`:

| Combat value | Phase 2 derivation | When applied |
|---|---|---|
| `HitBonus` component | `equipment.hit_bonus` only | baked at spawn / equip |
| Hit-roll attribute bonus | STR_mod for melee, DEX_mod for ranged, 0 otherwise | dynamic in `hit_check_system` |
| `DamageBonus` component | `equipment.damage_bonus` only | baked at spawn / equip |
| Damage-roll attribute bonus | STR_mod for melee, DEX_mod for ranged, 0 otherwise | dynamic in `damage_roll_system` |
| Staff zap damage adder | `ability_mod(int).max(0)` | dynamic in `handle_zap_staff` |
| `Dodge` | `dex_mod + equipment.dodge_bonus` | baked at spawn / equip |
| `Armor` | `equipment.armor` (attribute-independent) | baked at spawn / equip |
| `MaxHp` | `floor(race_hp_mod × (8 + 11 × XL / 2))` | recomputed on every level-up |

The pure helper that drives the dynamic side is
`attack_attribute_bonus(source, attrs)` in
[src/character/attributes.rs](../../src/character/attributes.rs).

## Level Progression

> **Phase 2 second chunk** — XP grant, level-up handling, racial
> stat-gain modal, and player-choice ASIs are not yet implemented at
> the time of this rewrite. The data shape and intended flow are
> documented below so the second chunk has a clear target.

**Level cap:** 27 (DCSS).

**XP curve (slow-then-fast cubic-ish):**
```rust
xp_required_for_level(L) = 100·(L-1)² + 50·(L-1) + (10·(L-1)³)/8
```
- L2: 151
- L5: 1,925
- L10: 19,151
- L20: 60,000
- L27: ≈ 150,000

**Per-monster XP:** `MonsterAsset.tier` (explicit u32) → base XP
roughly `20 + (tier-1)·25 + ((tier-1)²)/4`. Final reward is multiplied
by an anti-farming function of `player_level - monster_tier`:

| Diff (player − tier) | Multiplier |
|---|---|
| ≤ -3 | 1.5× (bonus for punching up) |
| -2 to +2 | 1.0× (full) |
| +3 | 0.75× |
| +4 | 0.50× |
| ≥ +5 | **0×** (no farming low-level mobs) |

**Level-up effects:**
- Recompute `max_hp` from the formula at the new XL; heal to full
- Emit "LEVEL UP" particle on the player
- Game log: "You reach level N!"
- If the level is a multiple of `race.gain_schedule.interval`: open
  ASI modal with 1 point and the race's allowed letters
- If the level is in `{3, 9, 15, 21, 27}`: open ASI modal with 2 free
  points and all three letters allowed

**ASI modal UX** (DCSS-inspired): a small inline overlay shows
`(S)trength · (D)exterity · (I)ntelligence` with the disallowed
letters greyed out. Input is blocked until the player has spent all
available points. Single keypress per point.

**Total stat gains by L27** for a typical character:
- 6 racial-schedule bumps (every 4 levels)
- 10 player-choice points (5 events × 2 each)
- 16 points added on top of the chargen sum (36 for any race+class)

By L27, a focused stat can plausibly reach the mid-20s (mod +4 to +6).

## Character Creation Flow

`AppState::Menu` → `AppState::CharacterCreation` → `AppState::InGame`.

The character creation screen is a single keyboard-driven panel:

1. **Race selection** — left/right to change selected race
2. **Class selection** — left/right to change selected class
3. **Live preview** — HP / Dodge / melee+ranged Hit & Damage / Spell
   Damage, all computed from the current race+class via
   `compose_attributes` + `derive_stats`
4. **Begin Descent** — Enter to confirm, transitions to InGame

↑/↓ cycles focus between Race / Class / Begin. Esc returns to main menu.

## Save / Load

`Race`, `Class`, `Attributes`, `HitBonus`, `DamageBonus`, `Level`,
`Experience` all persist in `PlayerSaveData`. Save schema v4 drops the
Phase 1 `attributes.constitution` field and adds the level/XP fields.

## Cross-links

- [PLAYER.md](PLAYER.md) — base player stats and equipment slots
- [GAME.md](GAME.md) — combat formulas; attribute mods feed the d20
  hit check and damage roll dynamically
- [ITEMS.md](ITEMS.md) — starting kits reference items.ron

## Resolved Decisions

- **3 attributes (STR/DEX/INT), no CON.** HP scales with race + level.
- **Modifier anchor at 16.** Most chargen mods are negative; players
  earn positive mods through level-ups.
- **No chargen point allocation.** Race + class fully determine
  starting attribute scores.
- **3 races, not 4.** Halfling and its Lucky d20 reroll are gone.
- **Class identity through stats, not fudge factors.** No
  class_attack_bonus or class_dodge_bonus — distributions are the
  differentiator.
- **Two-source level-up stat gain.** Racial schedule (constrained
  letters per race) + player-choice ASIs at levels 3/9/15/21/27.
- **HP formula adapts DCSS.** No Fighting term yet; lands with Skills.
- **Level cap 27**, matching DCSS.
- **Anti-farming:** monsters 5+ levels below the player give 0 XP.

## Open Questions (Phase 2 second chunk + beyond)

1. Monster-side combat rebalancing against the new player power
   curve — separate phase.
2. Whether level-up heals to full or partial — currently planned
   full-heal (DCSS standard).
3. Whether the racial-schedule modal and the player-choice modal
   queue if they ever land on the same level (none under current
   schedules, but defensive code should handle it).
4. Skill system and Fighting integration into the HP formula.
