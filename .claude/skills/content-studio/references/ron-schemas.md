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

**Source:** `src/assets/mod.rs::ItemAsset`
**File:** `assets/items.ron`

```ron
"Item Name": (
    name: "Item Name",            // required
    tile_size: (32, 32),          // required for sprite rendering
    grid_size: (8, 8),            // required for sprite rendering
    item_kind: Weapon,            // required — Weapon|Armor|Ring|Amulet|Consumable|Staff
    rarity: Common,               // required — Common|Uncommon|Rare|Legendary
    ascii_char: "/",              // required
    ascii_fg: "#A0A0A0",          // required

    // Kind-specific (all optional, defaults)
    armor_slot: Some(Chest),      // Helm|Chest|Gloves|Boots|OffHand
    damage: Some("1d6"),          // weapon dice
    defense: 0,                   // flat armor
    weapon_range: 0,              // 0 = melee; >1 = ranged
    attack_speed: 1.0,
    weapon_ability: Some("Cleave"),  // Sword has none; Dagger="Backstab"; Axe="Cleave"
    staff_effect: Some(Lightning), // Fire|Lightning|Poison|Healing|Blinking|Force
    base_recharge: 250,
    max_stack: 1,                 // 1 = non-stackable
    is_ammo: false,
    is_quest_item: false,
    effect: Some(HealHp(15)),     // for Consumables — see Effects below

    // Equip modifiers (all default 0)
    dodge_bonus: 0,
    hit_bonus: 0,
    damage_bonus: 0,
    regen_bonus: 0,
    max_hp_bonus: 0,
    delay_modifier: 0.0,
    vision_bonus: 0,
    resistances: {"fire": 50},    // HashMap<String, i32> %
),
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
