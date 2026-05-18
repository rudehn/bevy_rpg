# RON Schemas

Current data formats for every asset file. Fields marked **required**
have no serde default. Fields marked (default) can be omitted.

When uncertain, consult the Rust source of truth listed at the top of
each section — schemas drift faster than this doc.

## Monsters

**Source:** `src/assets/mod.rs::MonsterAsset`
**File:** `assets/monsters.ron`

```ron
"Monster Name": (
    name: "Monster Name",         // required — display name
    vision: 8,                    // required — tile radius
    damage: "1d4",                // required — dice string
    base_hp: 10,                  // required
    faction: "Goblin",            // required if faction logic applies
    species: Humanoid,            // required — see Species enum below
    ai: Fsm(                      // required — see AI configs below
        flee_at_hp_percent: 0.25,
        chase_leash: 8,
        erratic_chance: 0.0,
        kites: false,
        kite_distance: 0,
        ranged_range: 0,
    ),

    // Optional — all default sensibly
    damage_type: "physical",      // "physical" | "fire" | "lightning" | "poison" | "necrotic"
    base_armor: 0,
    base_dodge: 0,
    perception: 0,                // Phase-4 stealth — d20 perception modifier (range ~-3..=+5)
    movement_delay: 1.0,
    attack_delay: 1.0,
    movement_mode: Ground,        // Ground | RestrictedToLiquid
    regen: None,                  // Option<i32> — per turn
    resistances: {"fire": 50},    // HashMap<String, i32> %
    stationary: false,
    abilities: [...],             // see Abilities section
    monster_abilities: [...],     // cooldown casts, see below
    loot_table: [...],            // per-monster drop list
    sprite: "sprites/x.png",      // optional; ASCII-only monsters omit
    ascii_char: "g",              // required for ASCII renderer
    ascii_fg: "#3CB43C",          // required — hex RGB
),
```

### Species enum (`components.rs`)
`Beast | Humanoid | Undead | Insect | Fungal | Ooze | Dragon | Construct | Aberration | Unknown`

Missing defaults to `Unknown` and logs a warning on load.

### AI configs
- `Fsm { flee_at_hp_percent, chase_leash, erratic_chance, kites, kite_distance, ranged_range }`
- `Goap { traits: [Trait, ...], base_morale: f32 }`
  - Traits: `Cowardly | Intelligent | Aggressive | Hoarder | Support | Commander | Ranged { range: i32 }`

### Abilities (`abilities` array)
Live list in `src/game/abilities.rs`. Parseable RON forms:
- `BurningStrike(damage_per_turn, duration, chance)` — on-hit fire DoT
- `PoisonStrike(damage_per_turn, duration, chance)` — on-hit poison DoT
- `StunningBlow(duration, chance)` — on-hit stun
- `SlowStrike(duration, chance)` — on-hit slow
- `LifeDrain(percent)` — on-hit heal-from-damage
- `Knockback(distance, chance)` — on-hit push
- `RoughBody(damage)` — on-being-hit reflect
- `Enrage(threshold_percent)` — buff at HP threshold
- `ExplodeOnHit(radius, effect)` — self-destruct on contact
  - `effect`: `CrackFloor` | `GasCloud(volume: u16)`
- `ExplodeOnDeath(damage, radius, damage_type)` — AoE on death
- `GasOnDeath(radius, volume)` — poison cloud on death
- `SummonOnDeath(monster, count)` — spawn on death
- `SplitOnHit(min_hp)` — Jelly clone on hit
- `PackTactics` — +damage when faction ally adjacent to target
- `WarCry(radius, duration)` — aura buff
- `Rally(radius, armor_bonus)` — passive armor aura
- `Terrify(radius)` — fear aura
- `MimicDisguise` — mimic-as-item

### Equipped loadout (`equipped` array)

```ron
equipped: ["Ritual Dagger", "Cult Robes"],
```

Names look up `items.ron`. At spawn the monster gets an `Equipment`
component with the items in the appropriate slots, each item spawned
as a minimal entity (no world Position, no rendering). Stat overrides
apply automatically: equipped weapon's `damage:` replaces the monster's
intrinsic `damage:`; armor `defense` adds to Armor (Block for OffHand
shields); armor `dodge_bonus` adds to Dodge. Weapon `on_hit_effects`
proc through the same handler the player's weapon procs use. Defaults
to empty; monsters without `equipped:` keep their intrinsic stats.

### Monster abilities (`monster_abilities` array) — cooldown casts
```ron
monster_abilities: [(
    kind: Bolt(dice: "2d4", damage_type: Physical),
    cooldown: 3,
    current_cooldown: 0,
    range: 6,
    name: "Magic Missile",
)],
```
Kinds: `Bolt | Heal | Summon | SummonCapped | ApplyStatus | SelfBuff`.
See `src/game/staves.rs::MonsterAbilityKind`.

## Monster spawns

**File:** `assets/monster_spawns.ron`

Single monster entry:
```ron
(monster: "Goblin", min_floor: 1, max_floor: 2, min_group: 1, max_group: 3),
```

Group entry (mixed pack with leader semantics):
```ron
(group: [
    (monster: "Rat Broodmother", min_count: 1, max_count: 1),
    (monster: "Sewer Rat",       min_count: 3, max_count: 4),
], min_floor: 5, max_floor: 10, on_leader_death: "scatter", flee_threshold: 0.5),
```

Optional: `spawn_on_liquid: true` for aquatic monsters.

## Items

**Source:** `src/assets/mod.rs::ItemAsset`, `src/assets/mod.rs::ItemKindData`
**File:** `assets/items.ron`

Items use a tagged-union `kind:` field. Kind-specific data lives inside
the matching variant; **universal equip bonuses stay flat**.

```ron
"Item Name": (
    name: "Item Name",            // required
    tile_size: (32, 32),          // required for sprite rendering
    grid_size: (8, 8),            // required for sprite rendering
    kind: Weapon((...)),          // required — see variants below
    rarity: Common,               // required — Common|Uncommon|Rare|Legendary
    ascii_char: "/",              // required
    ascii_fg: "#A0A0A0",          // required

    // Universal equip modifiers (apply to weapons, armor, rings, amulets)
    dodge_bonus: 0,
    hit_bonus: 0,
    damage_bonus: 0,
    regen_bonus: 0,
    max_hp_bonus: 0,
    delay_modifier: 0.0,
    vision_bonus: 0,
    resistances: {"fire": 50},    // HashMap<String, i32> %
    is_quest_item: false,
),
```

### `kind:` variants

```ron
// Weapons — damage required; everything else optional.
kind: Weapon((
    damage: "1d6",                              // REQUIRED — dice string
    attack_speed: 1.0,                          // default 1.0
    weapon_range: 0,                            // 0 = melee; >1 = ranged
    weapon_ability: Some("Backstab"),           // "Backstab" | "Cleave" | None
    weapon_skill: Some(LongBlades),             // weapon-family skill tag
    on_hit_effects: [                           // proc-weapons declare here
        PoisonStrike(damage_per_turn: 1, duration: 3, chance: 30),
    ],
)),

// Armor — slot required.
kind: Armor((
    slot: Chest,                                // REQUIRED — Helm|Chest|Gloves|Boots|OffHand
    defense: 3,                                 // default 0
    max_blocks: 0,                              // OffHand shields only (1/2/3)
    armor_stealth_penalty: 1,                   // Phase-4 stealth — subtracted from stealth_mod (default 0)
)),

// Staff — effect required.
kind: Staff((
    effect: Lightning,                          // REQUIRED — Fire|Lightning|Poison|Healing|Blinking|Force
    base_recharge: 250,
)),

// Consumable — all fields optional.
kind: Consumable((
    effect: Some(HealHp(15)),                   // None = inert (e.g. arrows)
    max_stack: 10,                              // 1 = non-stackable
    is_ammo: false,
)),

// Rings & amulets — unit variants; all data lives in the flat equip bonuses.
kind: Ring,
kind: Amulet,
```

### Phase-4 stealth fields (cross-cutting)

| Field | Type | Where | Authoring guidance |
|---|---|---|---|
| `perception` | `i32` (default `0`) | `MonsterAsset` | Phase-4 stealth perception modifier. Range ~`-3..=+5` per shipping monster. Added to the d20 perception roll in opposed-stealth checks. Keen-sensed predators positive, blundering brutes negative. See [docs/design/STEALTH.md](../../../../docs/design/STEALTH.md). |
| `armor_stealth_penalty` | `i32` (default `0`) | `kind: Armor` on `ItemAsset` | Per-armor stealth penalty subtracted from `stealth_mod`. Shipping curve: 0 cloth/robe, 1 padded/leather, 2 studded, 3 chain, 5 plate. |

### `OnHitEffect` variants (`src/game/items.rs::OnHitEffect`)

Wielder-agnostic on-hit procs. Apply to any entity with `Equipment.weapon`
pointing at this item — player or monster (Phase B). Mirrors a subset
of monster `AbilityDef`'s on-hit variants:

```ron
PoisonStrike  { damage_per_turn, duration, chance }   // %
BurningStrike { damage_per_turn, duration, chance }   // %
StunningBlow  { duration, chance }                    // %
SlowStrike    { duration, chance }                    // %
LifeDrain     { percent }                             // % of damage healed
```

### Consumable effects (`src/game/effects.rs::Effect`)
- `HealHp(amount: i32)`
- `ApplyHaste(turns: u32)`
- `ApplyFireResistance(turns: u32)`
- `Antidote(turns: u32)`
- `EnchantItem` — opens the enchant target UI
- `ZapStaff` — auto-inserted for staff items on spawn

## Item spawns

**File:** `assets/item_spawns.ron`

```ron
(item: "Healing Potion", min_floor: 1, max_floor: 26, weight: 5),
(item: "Arrow",          min_floor: 1, max_floor: 26, weight: 4,
 min_count: 5, max_count: 12),
```

Weights are relative within the floor band. Rarer items get lower weights.

## Factions

**File:** `assets/factions.ron`

```ron
(
    relations: [
        ( a: "Player", b: "Goblin", relation: Hostile ),
        ( a: "Goblin", b: "Kobold", relation: Hostile ),
    ],
)
```

Relation: `Hostile | Neutral | Friendly`. Pairs without an entry default
to Neutral. The engine resolves symmetrically — `(a,b)` implies `(b,a)`.

## Tiles

**File:** `assets/tiles.ron`

```ron
"Tile Name": (
    name: "Tile Name",
    walkable: true,
    opaque: false,
    tile_size: (32, 32),
    grid_size: (1, 1),
    ascii_char: "Ω",
    ascii_fg: "#C850FF",
    ascii_bg: "#3A2820",
),
```

## Decorations

**File:** `assets/decorations.ron`
Enum variants in `roguelike_engine::map::tile::Decoration`:
`None | Grass | TallGrass | DeadGrass | Rubble | Moss | Fungus | Cobweb
| Bloodstain | TrampledGrass | TrampledFungus | Embers | Ash | CrackedFloor
| Custom`.

## Props

**File:** `assets/props.ron`
Schema in `src/assets/mod.rs::PropAsset`. Has blocking/opacity/light fields.

## Traps

**Not yet implemented.** See `trap-design.md` in this folder for the
proposed schema.

## Common mistakes

- Forgetting `species:` on a new monster → defaults to `Unknown`, logs
  a warning at load.
- Using `"physical"` vs `Physical` inconsistently. `damage_type` takes
  a string; Rust enum forms like `DamageType::Physical` are code-only.
- Putting runic data in `items.ron`. Runics are rolled at spawn by
  `enchant_item` in `src/game/enchantment.rs`; items.ron never declares them.
- Giving a consumable an `effect: None`. Consumables with no effect
  silently do nothing when used.
- Omitting `ascii_char` / `ascii_fg` — the ASCII renderer shows `?` for
  missing glyphs and white for missing colors.
