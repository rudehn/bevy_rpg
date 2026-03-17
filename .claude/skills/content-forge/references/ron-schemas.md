# RON Schema Reference

Complete annotated RON formats for all data files the content-forge skill writes to.
All examples sourced from actual data files. All enum values sourced from Rust source.

## MonsterDef Schema (`assets/monsters.ron`)

```ron
MonsterAsset(
    // === Required Fields ===
    name: "Monster Name",           // String — unique display name
    sprite: "sprites/monsters/x.png", // String — path to sprite asset
    level: 5,                       // i32 — determines HP scaling and essence reward
    base_hp: 22,                    // i32 — raw HP before CON scaling: final_hp = base_hp + (CON_bonus * level)
    strength: 10,                   // i32 — melee damage bonus (bonus = stat - 10)
    dexterity: 10,                  // i32 — dodge chance (5 + DEX_bonus)
    constitution: 10,               // i32 — HP scaling (CON_bonus * level added to base_hp)
    agility: 10,                    // i32 — speed: delay = 1.0 - (AGI_bonus * 0.025), clamped [0.5, 2.0]
    perception: 10,                 // i32 — vision range: 8 + PER_bonus (min 2)
    damage: "1d6",                  // String — damage dice expression (e.g. "1d4", "2d8+4")
    faction_tag: "undead",          // String — faction identifier (see FactionTag values below)
    role: "melee_guard",            // String — squad role (see MonsterRole values below)

    // === Optional Fields (with defaults) ===
    intelligence: 0,                // i32 (default: 0) — mana max = INT * 5; set 0 for non-casters
    grid_size: Some((8, 8)),        // Option<UVec2> — sprite grid size
    tile_size: Some((32, 32)),      // Option<UVec2> — sprite tile size
    regen: Some(3),                 // Option<i32> — HP regen per turn (None = no regen)
    regen_suppress_immune: false,   // bool (default: false) — immune to regen suppression after damage
    damage_type: "physical",        // String (default: "physical") — see DamageType values
    base_armor: 0,                  // i32 (default: 0) — flat damage reduction
    ranged_range: 0,                // u32 (default: 0) — 0 = melee only; >0 = ranged attack range
    is_cowardly: false,             // bool (default: false) — enables flee behavior
    is_boss: false,                 // bool (default: false) — boss-specific behavior

    // === Abilities (all optional, default: None/empty) ===
    on_hit_effects: [],             // Vec<OnHitEffect> — effects applied on successful melee hit
    resistances: {},                // HashMap<String, String> — damage_type: resistance_level
    spells: [],                     // Vec<String> — spell IDs from spells.ron
    loot_table: [],                 // Vec<MonsterLootEntry> — items dropped on death

    // === Passive Abilities (all Option, default: None) ===
    poison_body: None,              // Option<i32> — damage dealt to melee attacker
    thorn_aura: None,               // Option<i32> — flat damage reflected to melee attackers
    reanimate_hp: None,             // Option<i32> — revives with this HP after first death
    enrage_on_hit: None,            // Option<u32> — gains enrage buff for N turns when hit
    explode_on_death: None,         // Option<(i32, i32)> — (damage, radius) on death
    death_curse: None,              // Option<DeathCurseEffect> — curse applied to killer
    summon_on_death: None,          // Option<(String, u32)> — (monster_name, count) spawned on death
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
    faction_tag: "goblinoid",
    role: "melee_guard",
),
```

### Example: Complex Caster

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
    death_curse: Some(WeakenStr(2, 10)),
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
        (monster: "Goblin Warchief", count: 1),
        (monster: "Goblin", count: (2, 3)),       // (min, max) range
        (monster: "Goblin Archer", count: (1, 2)),
    ],
    on_leader_death: "scatter",
    flee_threshold: 0.3,
),
```

### Example: Solo Spawn

```ron
MonsterSpawnInfo(
    monster: "Giant Spider",
    min_floor: 3,
    max_floor: 8,
),
```

### Example: Pack Spawn with Flee

```ron
MonsterSpawnInfo(
    monster: "Rat",
    min_floor: 1,
    max_floor: 5,
    min_group: 2,
    max_group: 5,
    flee_threshold: 0.4,
),
```

## ItemDef Schema (`assets/items.ron`)

```ron
ItemAsset(
    // === Required Fields ===
    name: "Item Name",              // String — unique display name
    sprite: "sprites/items/x.png",  // String — path to sprite asset

    // === Optional Fields (with defaults) ===
    grid_size: Some((8, 8)),        // Option<UVec2> — sprite grid size
    tile_size: Some((32, 32)),      // Option<UVec2> — sprite tile size
    item_kind: Weapon,              // ItemKind (default: Consumable) — see ItemKind values
    armor_slot: Some(Chest),        // Option<ArmorSlot> — required for Armor kind
    damage: Some("1d6"),            // Option<String> — damage dice for weapons
    defense: 0,                     // i32 (default: 0) — flat armor value for armor
    rarity: Common,                 // Rarity (default: Common) — see Rarity values
    weapon_range: 0,                // u32 (default: 0) — 0 = melee, >0 = ranged (tiles)

    // === Stat Bonuses (all i32, default: 0) ===
    str_bonus: 0,
    dex_bonus: 0,
    con_bonus: 0,
    agi_bonus: 0,
    int_bonus: 0,
    per_bonus: 0,

    // === Item Bonuses (default: empty) ===
    bonuses: [],                    // Vec<ItemBonus> — see ItemBonus values

    // === Consumable Fields ===
    effect: Some(HealHp(15)),       // Option<Effect> — see Effect values
    max_stack: 1,                   // u32 (default: 1) — stack size (>1 for consumables/ammo)
    is_ammo: false,                 // bool (default: false) — consumed by ranged attacks
)
```

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
    agi_bonus: -1,
),
```

### Example: Ring with Bonuses

```ron
ItemAsset(
    name: "Ring of the Berserker",
    sprite: "sprites/items/ring_berserker.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Ring,
    rarity: Rare,
    str_bonus: 2,
    bonuses: [
        MeleeDamagePercent(15),
        LifestealPercent(8),
    ],
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

### Example: Legendary Weapon

```ron
ItemAsset(
    name: "Soulreaper",
    sprite: "sprites/items/soulreaper.png",
    grid_size: Some((8, 8)),
    tile_size: Some((32, 32)),
    item_kind: Weapon,
    damage: Some("2d8"),
    rarity: Legendary,
    str_bonus: 3,
    bonuses: [
        LifestealPercent(20),
        MeleeDamagePercent(15),
        CritChance(10),
    ],
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
    weight: 1,                      // i32 (default: 1) — spawn weight within rarity tier
    rarity: Common,                 // Rarity (default: Common) — determines spawn probability
    min_count: 1,                   // u32 (default: 1) — min stack count (for stackable items)
    max_count: 1,                   // u32 (default: 1) — max stack count
),
```

### Example: Stackable Ammo Spawn

```ron
ItemSpawnInfo(
    item: "Arrows",
    min_floor: 1,
    max_floor: 20,
    rarity: Common,
    min_count: 5,
    max_count: 12,
),
```

## SpellData Schema (`assets/spells.ron`)

```ron
SpellData(
    name: "spell_id",               // String — unique identifier (snake_case)
    mana_cost: 5,                   // i32 — mana consumed on cast
    cooldown: 4,                    // u32 (default: 0) — turns before reuse (0 = no cooldown)
    description: "Human-readable description", // String
    target: Enemy,                  // SpellTarget — see SpellTarget values
    range: 6,                       // u32 — max cast distance in tiles
    effects: [                      // Vec<SpellEffect> — see SpellEffect values
        Damage(dice: "1d4", int_scaling: true),
    ],
    damage_type: Physical,          // DamageType (default: Physical) — see DamageType values
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

### Example: Heal Spell

```ron
SpellData(
    name: "heal_self",
    mana_cost: 8,
    cooldown: 8,
    description: "Channels healing energy to restore health",
    target: Castor,
    range: 0,
    effects: [
        Heal(dice: "1d6", int_scaling: true),
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

### Example: Buff Spell

```ron
SpellData(
    name: "enrage",
    mana_cost: 8,
    cooldown: 10,
    description: "Enter a furious rage, increasing damage",
    target: Castor,
    range: 0,
    effects: [
        ApplyEnrage(duration: 6),
    ],
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
- `Common` — 50% spawn weight, 0 bonuses typical
- `Uncommon` — 35% spawn weight, 1 bonus typical
- `Rare` — 14% spawn weight, 2 bonuses typical
- `Legendary` — 1% spawn weight, 3 bonuses typical, run-defining

### SpellTarget
- `Castor` — Affects only the caster (self-buffs, self-heals)
- `Enemy` — Targets a visible enemy
- `Ally` — Targets the most-wounded visible ally (not self)
- `AllyOrSelf` — Targets the most-wounded visible ally or self

### DamageType
- `Physical` (default)
- `Fire`
- `Lightning`
- `Necrotic`

*Note: Ice and Poison are referenced in BESTIARY.md but not in the DamageType enum. The implemented types are the 4 above.*

### ResistanceLevel
- `Weak` — Takes 150% damage
- `Normal` — Takes 100% damage (default)
- `Resistant` — Takes 50% damage (min 1)
- `Immune` — Takes 0 damage
- `Absorb` — Heals instead of taking damage

### MonsterRole (string in RON)
- `"melee_guard"` — Standard frontline fighter
- `"ranged"` — Ranged attacker
- `"brute"` — High HP/STR brawler
- `"caster"` — Spell-based threat
- `"leader"` — Squad leader with buffs/summons
- `"any"` — Wildcard, fills any squad position

### FactionTag (string in RON)
- `"vermin"`, `"goblinoid"`, `"undead"`, `"orcish"`, `"demonic"`, `"giant"`, `"dark"`, `"boss"`

### OnLeaderDeath (string in RON)
- `""` (empty/default) — No special effect
- `"scatter"` — Members lose target and wander
- `"enrage"` — Members gain temporary damage bonus

### ItemBonus Variants

**Damage:**
- `MeleeDamagePercent(i32)` — +N% melee damage
- `RangedDamagePercent(i32)` — +N% ranged damage
- `SpellDamagePercent(i32)` — +N% spell damage
- `ArmorPenetration(i32)` — Reduces target armor by N
- `CritChance(i32)` — +N% critical hit chance

**Defensive:**
- `DamageReductionPercent(i32)` — Reduces incoming damage by N%
- `DamageReflection(i32)` — Reflects N flat damage to melee attackers
- `DodgeChance(i32)` — +N% dodge chance
- `BlockChance(i32)` — +N% block chance

**Speed:**
- `ActionSpeedPercent(i32)` — +N% action speed (all actions)
- `AttackSpeedPercent(i32)` — +N% attack speed only

**Sustain:**
- `LifestealPercent(i32)` — Heal for N% of damage dealt
- `HpRegenFlat(i32)` — +N to HP regen accumulator per turn
- `ManaRegenFlat(i32)` — +N to mana regen accumulator per turn

**Spell Enhancement:**
- `SpellAoeRadius(i32)` — +N AoE radius
- `SpellChainBounces(i32)` — +N chain spell bounces
- `ManaCostReduction(i32)` — -N% mana cost
- `SpellRange(i32)` — +N spell range
- `CooldownReduction(i32)` — -N% cooldown duration

**On-Hit:**
- `OnHitPoison { chance: u32, damage: i32, duration: u32 }` — N% chance to poison
- `OnHitBurn { chance: u32, damage: i32, duration: u32 }` — N% chance to burn
- `OnHitSlow { chance: u32, duration: u32 }` — N% chance to slow
- `OnHitKnockback { chance: u32, distance: i32 }` — N% chance to knockback
- `OnHitStun { chance: u32, duration: u32 }` — N% chance to stun

**Resource:**
- `MaxHp(i32)` — +N maximum HP
- `MaxMana(i32)` — +N maximum mana

**Healing:**
- `HealingReceivedPercent(i32)` — +N% healing received
- `PotionEffectiveness(i32)` — +N% potion effect

**Weapon:**
- `WeaponRange(i32)` — +N weapon range (tiles)

**Summoner:**
- `SummonDuration(i32)` — +N% summon duration
- `MaxSummons(i32)` — +N max active summons
- `SummonHpPercent(i32)` — +N% summon HP

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

### OnHitEffect Variants (Monster)
- `ApplyPoison(damage_per_turn: i32, duration: u32, chance: u32)` — Poison on hit
- `ApplySlow(duration: u32, chance: u32)` — Slow on hit
- `ApplyBurning(damage_per_turn: i32, duration: u32, chance: u32)` — Burn on hit
- `AttributeDrain(attribute: String, amount: i32, duration: u32, chance: u32)` — Drain stat
- `Knockback(distance: i32, chance: u32)` — Push target away
- `LifeDrain(amount: i32, chance: u32)` — Heal attacker, damage target
- `Disarm(duration: u32, chance: u32)` — Remove weapon temporarily

### DeathCurseEffect Variants
- `Slow(duration: u32)` — Slow the killer
- `WeakenStr(amount: i32, duration: u32)` — Reduce killer's STR

### Effect Variants (Consumable Items)
- `HealHp(i32)` — Restore N HP
- `RestoreMana(i32)` — Restore N mana
- `GainStr(i32)` — Permanent STR increase
- `GainDex(i32)` — Permanent DEX increase
- `GainCon(i32)` — Permanent CON increase
- `GainAgi(i32)` — Permanent AGI increase
- `GainInt(i32)` — Permanent INT increase
- `GainPer(i32)` — Permanent PER increase
- `LearnSpell(String)` — Learn spell by ID (e.g., `LearnSpell("magic_missile")`)
