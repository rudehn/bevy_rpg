# RON Schema Reference

Complete annotated RON formats for all data files the content-forge skill writes to.
All examples sourced from actual data files. All enum values sourced from Rust source.

## Current System State

The game was recently simplified to use **direct values** instead of
attribute-to-stat conversion. Some legacy fields remain in the structs but
are not used by the spawner. This reference documents what the spawner
actually reads vs. what's legacy.

## MonsterDef Schema (`assets/monsters.ron`)

```ron
MonsterAsset(
    // === Active Fields (read by spawner) ===
    name: "Monster Name",           // String — unique display name
    sprite: "sprites/monsters/x.png", // String — path to sprite asset
    level: 5,                       // i32 — used in essence reward formula
    base_hp: 22,                    // i32 — used directly as Health.max (no scaling)
    perception: 10,                 // i32 — vision range: 8 + (perception - 10), min 2
    damage: "1d6",                  // String — damage dice expression (e.g. "1d4", "2d8+4")
    faction_tag: "undead",          // String — faction identifier
    role: "melee_guard",            // String — squad role

    // === Active Optional Fields ===
    intelligence: 0,                // i32 (default: 0) — mana pool = INT * 5; 0 = no mana
    grid_size: Some((8, 8)),        // Option<UVec2> — sprite grid size
    tile_size: Some((32, 32)),      // Option<UVec2> — sprite tile size
    regen: Some(3),                 // Option<i32> — HP regen per turn
    damage_type: "physical",        // String (default: "physical") — see DamageType values
    base_armor: 0,                  // i32 (default: 0) — flat damage reduction
    ranged_range: 0,                // u32 (default: 0) — 0 = melee only; >0 = ranged range
    is_boss: false,                 // bool (default: false) — boss-specific behavior
    resistances: {},                // HashMap<String, String> — damage_type: resistance_level
    spells: [],                     // Vec<String> — spell IDs from spells.ron
    loot_table: [],                 // Vec<MonsterLootEntry> — items dropped on death

    // === Legacy Fields (on struct, NOT used by spawner) ===
    strength: 10,                   // i32 — NOT USED (legacy, kept for future reconnection)
    dexterity: 10,                  // i32 — NOT USED
    constitution: 10,               // i32 — NOT USED
    agility: 10,                    // i32 — NOT USED

    // === Ability Data (in RON files, NOT on MonsterAsset struct — orphaned) ===
    // These fields exist in monsters.ron but are silently ignored during
    // deserialization because MonsterAsset doesn't declare them.
    // The handler systems exist in abilities.rs but no monsters receive these components.
    // Preserved for future reconnection.
    //
    // is_cowardly: true,           // bool — flee behavior
    // on_hit_effects: [],          // Vec<OnHitEffect> — effects on melee hit
    // poison_body: Some(2),        // Option<i32> — poison melee attackers
    // thorn_aura: Some(3),         // Option<i32> — reflect damage
    // reanimate_hp: Some(15),      // Option<i32> — revive after first death
    // enrage_on_hit: Some(50),     // Option<u32> — enrage at HP threshold
    // explode_on_death: Some((8, 1)), // Option<(i32, i32)> — AoE on death
    // death_curse: Some(WeakenStr(2, 10)), // Option<DeathCurseEffect>
    // summon_on_death: Some(("Skeleton", 2)), // Option<(String, u32)>
)
```

### Example: Simple Melee Monster

```ron
MonsterAsset(
    name: "Goblin",
    sprite: "sprites/monsters/goblin.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    level: 1,
    base_hp: 10,
    strength: 10,
    dexterity: 10,
    constitution: 10,
    agility: 10,
    perception: 10,
    damage: "1d4",
    faction_tag: "goblin",
    role: "melee_guard",
),
```

### Example: Caster Monster

```ron
MonsterAsset(
    name: "Lich Apprentice",
    sprite: "sprites/monsters/lich_apprentice.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    level: 14,
    base_hp: 45,
    strength: 8,
    dexterity: 10,
    constitution: 12,
    agility: 10,
    intelligence: 18,
    perception: 14,
    damage: "1d6+1",
    damage_type: "necrotic",
    faction_tag: "undead",
    role: "caster",
    base_armor: 2,
    resistances: {
        "necrotic": "immune",
        "fire": "weak",
    },
    spells: ["shadow_bolt", "death_coil", "raise_skeleton", "curse", "mana_drain"],
),
```

## MonsterSpawnEntry Schema (`assets/monster_spawns.ron`)

### Simple Format (single monster type)

```ron
MonsterSpawnInfo(
    monster: "Goblin",              // String — must match a name in monsters.ron
    min_floor: 1,                   // i32 — first floor this monster can appear
    max_floor: 6,                   // i32 — last floor this monster can appear
    min_group: 2,                   // i32 (default: 1) — minimum spawn count
    max_group: 4,                   // i32 (default: 1) — maximum spawn count
    on_leader_death: "scatter",     // String (default: "") — see OnLeaderDeath values
    flee_threshold: 0.3,            // f32 (default: 0.5) — group HP ratio to trigger flee
),
```

### Composite Group Format (mixed monster types)

```ron
MonsterSpawnInfo(
    min_floor: 4,
    max_floor: 8,
    group: [
        (monster: "Goblin Warchief", min_count: 1, max_count: 1),
        (monster: "Goblin", min_count: 2, max_count: 3),
        (monster: "Goblin Archer", min_count: 1, max_count: 2),
    ],
    on_leader_death: "scatter",
    flee_threshold: 0.3,
),
```

## ItemDef Schema (`assets/items.ron`)

```ron
ItemAsset(
    // === Active Fields ===
    name: "Item Name",              // String — unique display name
    sprite: "sprites/items/x.png",  // String — path to sprite asset
    grid_size: Some((8, 8)),        // Option<UVec2> — sprite grid size
    tile_size: Some((32, 32)),      // Option<UVec2> — sprite tile size
    item_kind: Weapon,              // ItemKind (default: Consumable)
    armor_slot: Some(Chest),        // Option<ArmorSlot> — required for Armor kind
    damage: Some("1d6"),            // Option<String> — damage dice for weapons
    defense: 0,                     // i32 (default: 0) — flat armor value for armor
    rarity: Common,                 // Rarity (default: Common)
    weapon_range: 0,                // u32 (default: 0) — 0 = melee, >0 = ranged (tiles)
    effect: Some(HealHp(15)),       // Option<Effect> — consumable one-shot effect
    max_stack: 1,                   // u32 (default: 1) — stack size (>1 for consumables/ammo)
    is_ammo: false,                 // bool (default: false) — consumed by ranged attacks

    // === Legacy Fields (on struct, NOT used by spawner) ===
    str_bonus: 0,                   // i32 — NOT USED
    dex_bonus: 0,                   // i32 — NOT USED
    con_bonus: 0,                   // i32 — NOT USED
    agi_bonus: 0,                   // i32 — NOT USED
    int_bonus: 0,                   // i32 — NOT USED
    per_bonus: 0,                   // i32 — NOT USED
)
```

**Note:** The ItemBonus system has been removed. Items differentiate by
their direct `damage`/`defense` values and rarity tier only.

### Example: Weapon

```ron
ItemAsset(
    name: "Iron Sword",
    sprite: "sprites/items/iron_sword.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Weapon,
    damage: Some("1d6"),
    rarity: Common,
),
```

### Example: Armor

```ron
ItemAsset(
    name: "Chain Mail",
    sprite: "sprites/items/chain_mail.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Armor,
    armor_slot: Some(Chest),
    defense: 3,
    rarity: Uncommon,
),
```

### Example: Consumable

```ron
ItemAsset(
    name: "Healing Potion",
    sprite: "sprites/items/healing_potion.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Consumable,
    rarity: Common,
    effect: Some(HealHp(15)),
    max_stack: 5,
),
```

### Example: Spellbook

```ron
ItemAsset(
    name: "Tome of Magic Missile",
    sprite: "sprites/items/spellbook.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Spellbook,
    rarity: Uncommon,
    effect: Some(LearnSpell("magic_missile")),
),
```

### Example: Ranged Weapon with Ammo

```ron
// Weapon
ItemAsset(
    name: "Longbow",
    sprite: "sprites/items/longbow.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Weapon,
    damage: Some("1d8"),
    rarity: Uncommon,
    weapon_range: 8,
),

// Ammo
ItemAsset(
    name: "Arrows",
    sprite: "sprites/items/arrows.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Consumable,
    rarity: Common,
    max_stack: 12,
    is_ammo: true,
),
```

## ItemSpawnEntry Schema (`assets/item_spawns.ron`)

```ron
ItemSpawnInfo(
    item: "Iron Sword",             // String — must match a name in items.ron
    min_floor: 1,                   // i32 — first floor this item can appear
    max_floor: 10,                  // i32 — last floor this item can appear
    weight: 1,                      // i32 (default: 1) — spawn weight
    min_count: 1,                   // u32 (default: 1) — min stack count
    max_count: 1,                   // u32 (default: 1) — max stack count
),
```

## SpellData Schema (`assets/spells.ron`)

```ron
SpellData(
    name: "spell_id",               // String — unique identifier (snake_case)
    mana_cost: 5,                   // i32 — mana consumed on cast
    cooldown: 4,                    // u32 (default: 0) — turns before reuse
    description: "Human-readable description", // String
    target: Enemy,                  // SpellTarget — see values below
    range: 6,                       // u32 — max cast distance in tiles
    effects: [                      // Vec<SpellEffect> — see values below
        Damage(dice: "1d4", int_scaling: true),
    ],
    damage_type: Physical,          // DamageType (default: Physical)
),
```

### Example: Attack Spell

```ron
SpellData(
    name: "magic_missile",
    mana_cost: 5,
    cooldown: 4,
    description: "Fires a bolt of arcane energy at a target",
    target: Enemy,
    range: 6,
    effects: [
        Damage(dice: "1d4", int_scaling: true),
    ],
),
```

### Example: AoE Spell

```ron
SpellData(
    name: "fireball",
    mana_cost: 22,
    cooldown: 8,
    description: "Hurls an explosive ball of fire at a target area",
    target: Enemy,
    range: 6,
    effects: [
        AoeDamage(dice: "2d6", radius: 1, int_scaling: false),
    ],
    damage_type: Fire,
),
```

### Example: Summon Spell

```ron
SpellData(
    name: "raise_skeleton",
    mana_cost: 10,
    cooldown: 15,
    description: "Animate a skeleton warrior to fight for you",
    target: Castor,
    range: 0,
    effects: [
        SummonAlly(monster_name: "Skeleton", count: 1, duration: Some(15)),
    ],
    damage_type: Necrotic,
),
```

## Valid Enum Values

### ItemKind
- `Consumable` — Single-use items (potions, elixirs, arrows)
- `Weapon` — Equippable weapons (melee or ranged)
- `Armor` — Equippable armor (requires `armor_slot`)
- `Ring` — Equippable accessory (2 slots)
- `Amulet` — Equippable accessory (1 slot)
- `Spellbook` — Single-use, teaches a spell

### ArmorSlot
- `Chest`, `Helm`, `Gloves`, `Boots`, `OffHand`

### Rarity
- `Common` — 50% spawn weight
- `Uncommon` — 35% spawn weight
- `Rare` — 14% spawn weight
- `Legendary` — 1% spawn weight, run-defining

### SpellTarget
- `Castor` — Affects only the caster
- `Enemy` — Targets a visible enemy
- `Ally` — Targets the most-wounded visible ally (not self)
- `AllyOrSelf` — Targets the most-wounded visible ally or self

### DamageType
- `Physical` (default)
- `Fire`
- `Lightning`
- `Necrotic`

### ResistanceLevel
- `Weak` — Takes 150% damage
- `Normal` — Takes 100% damage (default)
- `Resistant` — Takes 50% damage (min 1)
- `Immune` — Takes 0 damage
- `Absorb` — Heals instead of taking damage

### MonsterRole (string in RON)
- `"melee_guard"` — Standard frontline fighter
- `"ranged"` — Ranged attacker
- `"brute"` — High HP brawler
- `"caster"` — Spell-based threat
- `"leader"` — Squad leader
- `"any"` — Wildcard, fills any squad position

### FactionTag (string in RON)
- `"beast"`, `"goblin"`, `"undead"`, `"orc"`, `"demon"`, `"giant"`, `"dark"`, `"boss"`

### OnLeaderDeath (string in RON)
- `""` (empty/default) — No special effect
- `"scatter"` — Members lose target and wander
- `"enrage"` — Members gain temporary damage bonus

### OnHitEffect Variants (Monster — orphaned, see balance-curves.md)
- `ApplyPoison(damage_per_turn: i32, duration: u32, chance: u32)`
- `ApplySlow(duration: u32, chance: u32)`
- `ApplyStun(duration: u32, chance: u32)`
- `ApplyBurning(damage_per_turn: i32, duration: u32, chance: u32)`
- `AttributeDrain(attribute: String, amount: i32, duration: u32, chance: u32)`
- `Knockback(distance: i32, chance: u32)`
- `LifeDrain(amount: i32, chance: u32)`
- `Disarm(duration: u32, chance: u32)`

### DeathCurseEffect Variants (orphaned)
- `Slow(duration: u32)`
- `Poison(damage_per_turn: i32, duration: u32)`
- `WeakenStr(amount: i32, duration: u32)`

### SpellEffect Variants

**Damage/Healing:**
- `Damage { dice: String, int_scaling: bool }` — Single-target damage
- `Heal { dice: String, int_scaling: bool }` — Single-target heal
- `AoeDamage { dice: String, radius: i32, int_scaling: bool }` — Area damage
- `ChainDamage { dice: String, max_jumps: i32, jump_range: i32, int_scaling: bool }` — Chain damage
- `AoeHeal { dice: String, radius: i32 }` — Area heal
- `AoeApplyPoison { damage_per_turn: i32, duration: u32, radius: i32 }` — Area poison

**Buffs/Debuffs:**
- `Buff { attribute: String, amount: i32, duration: u32 }` — Temporary stat increase
- `Debuff { attribute: String, amount: i32, duration: u32 }` — Temporary stat decrease
- `ApplyPoison { damage_per_turn: i32, duration: u32 }` — Single-target poison
- `ApplyHaste { duration: u32 }` — Speed increase
- `ApplySlow { duration: u32 }` — Speed decrease
- `ApplyStun { duration: u32 }` — Prevents actions
- `ApplyEnrage { duration: u32 }` — +50% damage
- `DrainMana { amount: i32, int_scaling: bool }` — Remove target mana
- `SpiritShield { duration: u32 }` — Mana absorbs damage

**Utility:**
- `Teleport { range: i32 }` — Move to random tile within range
- `Taunt { duration: u32 }` — Force target to attack caster
- `Mark { bonus_percent: i32, duration: u32 }` — Increase damage to target
- `Vanish { duration: u32 }` — Become invisible

**Summoning:**
- `SummonAlly { monster_name: String, count: u32, duration: Option<u32> }` — Spawn ally
- `Sacrifice { heal_percent: i32 }` — Kill summon, heal caster
- `SelfDamageHeal { damage_percent: i32, heal_percent: i32 }` — Hurt self, heal target

### Effect Variants (Consumable Items)
- `HealHp(i32)` — Restore N HP
- `RestoreMana(i32)` — Restore N mana
- `LearnSpell(String)` — Learn spell by ID
