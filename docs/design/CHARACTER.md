# Character System

## Design Philosophy

The original Brogue-style design had no character creation — every run
started as the same blank adventurer, with identity emerging from gear
and enchant-scroll choices ([PLAYER.md](PLAYER.md)).

The character system is a deliberate **piecemeal pivot** toward a
D&D-flavored RPG layer on top of that foundation. The player picks a
**race**, a **class**, and allocates a small number of free attribute
points before the first floor. Those choices feed into combat math
through four attributes — **STR, DEX, CON, INT** — without replacing
the existing gear-driven progression.

**What's in scope for the character system today (Phase 1):**
- Character creation screen (Race → Class → Attribute allocation)
- 4 races × 4 classes (16 starting combos)
- Attribute components and modifier helpers
- HP derived from CON
- Attribute modifiers contributing to derived combat stats
- Intentionally **weak starting kits** so power still has to be earned
  through gear discovery

**Explicitly deferred to later phases:**
- **Saving throws** (player and monster). When added, monster saves
  derive from monster tier + Species defaults with per-monster overrides.
- **XP and levels.** Phase 1 freezes the player at level 1.
- **Skills** (use-trained DCSS-style tiers on weapon families, defenses,
  magic schools).
- **Mana and spells.** Mages still use staves with charges until the
  Mana phase ships.
- **Racial attribute caps** (ADOM-style ceilings). Revisited alongside
  ASIs / levels.
- **Charm / fear / morale saves** (WIS/CHA are absent from the attribute
  roster; deferred decisions about which save handles those effects live
  in the Saves phase).

## Data Model

### Components

| Component | Fields | Notes |
|---|---|---|
| `Race` | enum: `Human`, `Dwarf`, `Elf`, `Halfling` | Marker + lookup key for race data |
| `Class` | enum: `Warrior`, `Rogue`, `Mage`, `Ranger` | Marker + lookup key for class data |
| `Attributes` | `{ str: i32, dex: i32, con: i32, int: i32 }` | Final attribute scores after race + class + free points |

`Attributes::ability_mod(score) -> i32` returns the D&D 5e modifier:
`(score - 10).div_euclid(2)` — floor division, so 8 → -1, 9 → -1,
10 → 0, 11 → 0, 12 → +1, 18 → +4, 20 → +5.

### Assets

`assets/races.ron` and `assets/classes.ron`, loaded via the existing
`bevy_common_assets` RON pipeline ([src/assets/mod.rs](../../src/assets/mod.rs)).

**Race entry (RON):**
```ron
Race(
    id: "dwarf",
    name: "Dwarf",
    str_bonus: 2, dex_bonus: 0, con_bonus: 2, int_bonus: 0,
    trait: Stoneblood,  // enum
    description: "Hardy folk of the deep places.",
)
```

**Class entry (RON):**
```ron
Class(
    id: "warrior",
    name: "Warrior",
    primary_attr: Str,
    secondary_attr: Con,
    base_hp: 12,
    class_attack_bonus: 1,
    class_dodge_bonus: 0,
    starting_kit: [
        ItemRef("rusted_shortsword"),
        ItemRef("padded_armor"),
    ],
    description: "Steel and discipline.",
)
```

### Spawn payload

Character creation hands off to the player spawner via
`CharacterChoice { race: Race, class: Class, free_points: [i32; 4] }`.
The spawner resolves baselines, applies free points, computes final
`Attributes`, and inserts the appropriate `Race`/`Class`/`Attributes`
components plus the starting kit.

## Races

Race contributes baseline attribute bonuses on top of `[10, 10, 10, 10]`,
plus one passive trait. Halfling's trait is deliberately strong so it
competes with Dwarf's poison resistance — a 5% reroll on every d20 is
quietly massive.

> **Maintenance contract:** every shipping race in `assets/races.ron`
> must appear in this table with its trait keyword, and every trait
> keyword listed here must match the `RaceTrait` enum in
> `src/character/race.rs`. The `character_md_documents_every_shipping_race`
> test enforces this — changing a race without updating this section
> will fail the build.

| Race | STR | DEX | CON | INT | Passive Trait |
|---|---|---|---|---|---|
| Human | +1 | +1 | +1 | +1 | **Versatile** — one of your 4 free points can exceed the per-stat allocation cap by 1 |
| Dwarf | +2 | 0 | +2 | 0 | **Stoneblood** — 50% poison resistance |
| Elf | 0 | +2 | 0 | +2 | **Keen Senses** — vision range +2 tiles |
| Halfling | 0 | +2 | +1 | +1 | **Lucky** — reroll any natural 1 on a d20 (no cooldown); must take the second result |

**Playstyle at a glance:**
- **Human** — best for indecisive builds. Even stat spread plus one
  attribute that can climb to 19 with allocation. Pairs well with any
  class but shines when you want a melee/caster hybrid (Warrior+staff
  finds or Mage+armor finds).
- **Dwarf** — the toughest opener. Poison resistance is a quiet life-saver
  on deep floors (jellies, spider venom, poison gas). STR+CON spread
  makes Warrior the natural match.
- **Elf** — vision and brains. The +2 vision range catches ambushes
  earlier; INT+DEX spread is what Mages and Rogues want. Don't pick if
  you're going Warrior — you'll feel the missing CON.
- **Halfling** — the safest run. Lucky removes worst-case hit and save
  rolls forever — no cap, no cooldown. DEX+CON+INT spread plays Rogue
  cleanly and lets a low-STR Ranger function.

**Implementation notes:**
- Stoneblood stacks **multiplicatively** with the existing poison
  resistance pipeline ([damage_reduction_system](../../src/game/combat.rs)).
  A Dwarf wearing a 50%-poison-resistance ring takes 25% damage from poison.
- Keen Senses applies to the player's `VisionRange` at spawn; later
  effects that modify vision (potions, items) stack on top.
- Lucky is implemented via the `roll_d20_with_race` helper in
  [src/character/dice.rs](../../src/character/dice.rs). Every player d20
  call site must route through it.
- Versatile changes only the allocation UI — it's not a runtime effect.

## Classes

Class sets primary/secondary attribute focus, base HP, small flat
attack/dodge bonuses, and a deliberately **weak** starting kit. The
weak kit is intentional: the class teaches you the role without
short-circuiting the gear discovery loop.

> **Maintenance contract:** every class in `assets/classes.ron` must
> appear in this table with its name and base HP. The
> `character_md_documents_every_shipping_class` test enforces this —
> changing a class's name or base HP without updating this section
> fails the build.

| Class | Primary / Secondary | Base HP | Attack Bonus | Dodge Bonus | Starting Kit |
|---|---|---|---|---|---|
| Warrior | STR / CON | 12 | +1 | 0 | Rusted Shortsword (1d6, no bonuses), Padded Armor (armor 1, no dodge) |
| Rogue | DEX / INT | 8 | 0 | +1 | Dagger (1d4, +1 hit), Cloth Wraps (0 armor, +1 dodge), 1 throwing knife |
| Mage | INT / CON | 6 | 0 | 0 | Apprentice Staff (1d3 lightning, 1 charge), Robe (0 armor, 0 dodge) |
| Ranger | DEX / STR | 8 | 0 | 0 | Shortbow (1d6, 6-tile range), 6 arrows, Padded Armor (armor 1) |

**Class baselines:** primary attribute gets +2, secondary +1, others 0.
Stacked on top of race bonuses, then free points apply.

**Playstyle at a glance:**
- **Warrior** — the highest base HP, +1 attack bonus on every melee
  swing, heaviest starting armor. The forgiving entry-class; trades
  finesse and magic for raw durability.
- **Rogue** — fastest dodge ramp (+1 base + DEX scaling), Dagger gives
  +1 hit, a single throwing knife for emergencies. Squishy at 8 HP —
  positioning is everything. INT secondary opens scrolls/staves later.
- **Mage** — the most fragile class at 6 HP, but INT scales the
  damage of every staff zap they ever pick up (clamped at 0 — dump-INT
  Mages can't make a staff weaker than baseline). Find or buy more
  staves; until then your Apprentice Staff has one charge.
- **Ranger** — DEX-primary like the Rogue but with STR secondary, so
  bow-or-melee builds both feel okay. Six arrows to start; conserving
  ammo is the early-floor puzzle. Hit and damage branch by weapon type
  at runtime: bows consume DEX_mod, melee weapons consume STR_mod (see
  [§Combat Math Integration](#combat-math-integration)).

## Attribute Allocation

Three-step flow in the character creation screen:

1. **Race selection** — applies race bonuses on top of `[10, 10, 10, 10]`.
2. **Class selection** — applies class baseline (primary +2, secondary +1).
3. **Allocate 4 free points** across STR/DEX/CON/INT.

**Allocation rules:**
- Per-stat cap: `baseline_after_race_and_class + 4`.
- Per-stat floor: 8 (race + class never penalize below this, but the
  floor exists for future-proofing).
- **Human Versatile exception:** one stat may reach `baseline + 5` instead
  of `baseline + 4`.

After allocation, `Attributes` is finalized; modifiers are computed via
`ability_mod`.

### HP Formula (Phase 1, no levels)

```
max_hp = class_base_hp + ability_mod(con)
```

So a Warrior (`base 12`) with CON 14 (`mod +2`) starts at 14 HP. A Mage
(`base 6`) with CON 8 (`mod -1`) starts at 5 HP.

**Phase 2 will extend this** to
`+ (level - 1) × (class_per_level + ability_mod(con))`.

## Combat Math Integration

`Attributes` doesn't replace the existing stat components in
[src/game/stats.rs](../../src/game/stats.rs). Attacker-side scaling
(hit / damage) is **dynamic** — `hit_check_system` and `damage_roll_system`
read the attacker's `Attributes` and the `AttackIntentMessage.source` and
add the right modifier per attack. Defender-side scaling (Dodge) and
one-time values (MaxHp) are **baked** at spawn.

| Combat value | Phase 1 derivation | When applied |
|---|---|---|
| `HitBonus` component | `class_attack_bonus + equipment.hit_bonus` | baked at spawn / equip |
| Hit-roll attribute bonus | STR_mod for melee, DEX_mod for ranged, 0 otherwise | dynamic in `hit_check_system` |
| `DamageBonus` component | `equipment.damage_bonus` | baked at spawn / equip |
| Damage-roll attribute bonus | STR_mod for melee, DEX_mod for ranged, 0 otherwise | dynamic in `damage_roll_system` |
| Staff zap damage adder | `ability_mod(int).max(0)` | dynamic in `handle_zap_staff` |
| `Dodge` | `ability_mod(dex) + class_dodge_bonus + equipment.dodge_bonus` | baked at spawn / equip |
| `Armor` | `equipment.armor` (attribute-independent) | baked at spawn / equip |
| `MaxHp` | `class_base_hp + ability_mod(con)` | baked at spawn |

**Why dynamic on the attack side?** A single `HitBonus` value can't
correctly represent both "STR for melee" and "DEX for ranged" — a
Ranger with high DEX would either get a wrong melee swing or a wrong
bow shot. Reading the modifier at roll time using the attack's source
gives every weapon class the right scaling without bookkeeping.

**Why baked on the defender side?** Dodge is attack-type-agnostic
(a nimble target dodges arrows and swords equally well) and MaxHp is
a one-time spawn value. Recomputing them dynamically would just be
work without behavior change.

The pure helper that drives the dynamic side is
`attack_attribute_bonus(source, attrs)` in
[src/character/attributes.rs](../../src/character/attributes.rs).
Every attribute-aware combat site routes through it.

**INT in Phase 1:** without saves and spells, INT would be inert.
Letting `ability_mod(int)` scale Mage's staff zap damage gives the Mage
class an immediate, tangible reason to pump INT, and makes "dump INT
for HP" a legible trade-off.

**Equipment is additive, not overriding.** A Warrior can wield a bow —
the bow's hit bonus stacks with whatever ability mod fires (DEX for
ranged). Phase 3 adds weapon-family skill tiers that further amplify
the appropriate attribute.

**Critical hits unchanged:** natural 20 still doubles damage dice.

### Halfling Lucky implementation

All d20 rolls go through a single helper:

```rust
fn d20_roll(entity: Entity, race_query: &Query<&Race>, rng: &mut RandomNumberGenerator) -> u32 {
    let roll = rng.range(1, 21);
    if roll == 1 && race_query.get(entity).is_ok_and(|r| *r == Race::Halfling) {
        rng.range(1, 21)
    } else {
        roll
    }
}
```

Existing direct `rng.range(1, 21)` call sites in
[src/game/combat.rs](../../src/game/combat.rs) and other d20 consumers
must be migrated to this helper as part of Phase 1.

## Character Creation Flow

`AppState::Menu` → `MenuState::CharacterCreation` → three substeps
(Race, Class, Attribute Allocation). UI in
`src/menu/character_creation.rs`. Back button at each step. Default
preselection (Human Warrior, all 4 points into STR) for one-click new-game.

The screen live-previews: HP, hit bonus (melee + ranged), damage bonus
(melee + ranged + staff), dodge.

## Save / Load

`Race`, `Class`, and `Attributes` components must persist. Save version
is bumped to refuse pre-character-system saves on load (permadeath roguelike,
save churn is acceptable). See
[.claude/rules/save-load-checklist.md](../../.claude/rules/save-load-checklist.md).

## Cross-links

- [PLAYER.md](PLAYER.md) — base player stats and equipment slots; the
  "no attributes" wording is superseded by this doc.
- [GAME.md](GAME.md) — combat formulas; the d20 hit check and damage
  pipeline are unchanged, but `HitBonus`, `Dodge`, and `DamageBonus`
  inputs now include attribute contributions.
- [ITEMS.md](ITEMS.md) — starting kits reference items in items.ron;
  the four weak starting items are added in this phase.
- Save phase (future) — will add saving throws to the system.
- XP & Levels phase (future) — will add `Level` component and extend
  the HP formula.

## Resolved Decisions

- **4 attributes, not 6.** STR, DEX, CON, INT only. No WIS, no CHA.
  Charm/fear/morale saves will use INT/CON respectively when saves arrive.
- **Hybrid attribute allocation.** Race + class set baselines, then 4
  free points. Not pure point-buy, not pure rolled stats, not pure
  fixed.
- **HP uses CON modifier**, not raw CON. `(CON - 10) / 2`. Prevents the
  numbers from exploding at higher CON.
- **Asymmetric, but only for now.** Player has attributes; monsters do
  not. Monster equivalents (save bonuses, etc.) will be added via the
  existing [Species](../../src/components.rs) enum's defaults system in
  the Saves phase.
- **Weak starting kits.** A class is a role-shape, not a power lever.
  Gear discovery still drives mid- and late-run identity.
- **Halfling Lucky is unconditional.** No per-floor or per-fight cap.
  The reliability is the point.

## Open Questions (revisit in later phases)

1. Should racial caps prevent a Halfling from reaching STR 18+? Decide
   alongside ASIs.
2. Existing shrines and runics bump `HitBonus`/`DamageBonus` directly.
   They still work because `Attributes` contributes additively, not
   authoritatively. Implementation must keep this invariant; double-check
   when wiring `recalculate_stats`.
3. Mage's `Apprentice Staff` is the weakest staff tier — exact stats and
   sprite TBD during the asset phase. May reuse the weakest existing staff
   rather than authoring a new one.
