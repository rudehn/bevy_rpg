# Ability Catalog

Complete catalog of all implemented abilities in The Veiled Tyrant,
organized by category. Sourced from `src/game/abilities.rs`.

Abilities are passive, reactive, or triggered mechanics that operate
outside the spell/mana system. They have no mana cost, no cooldown
(unless internally managed), and are never "cast" by the player.

## Category 1: On-Hit Effects (Monster)

Trigger when a monster lands a successful melee attack. Configured via
`on_hit_effects` field in `monsters.ron`. Each has a `chance` (1-100%).

| Effect | Parameters | What It Does |
|--------|-----------|--------------|
| `ApplyPoison` | `damage_per_turn`, `duration`, `chance` | DoT over N turns |
| `ApplySlow` | `duration`, `chance` | Halves target speed |
| `ApplyStun` | `duration`, `chance` | Prevents all actions |
| `ApplyBurning` | `damage_per_turn`, `duration`, `chance` | Fire DoT |
| `AttributeDrain` | `attribute`, `amount`, `duration`, `chance` | Temporarily reduce a stat |
| `LifeDrain` | `amount`, `chance` | Heal attacker, damage target |
| `Knockback` | `distance`, `chance` | Push target away |
| `Disarm` | `duration`, `chance` | Remove weapon temporarily |

### RON Format

```ron
on_hit_effects: [
    ApplyPoison(damage_per_turn: 2, duration: 4, chance: 50),
    LifeDrain(amount: 3, chance: 100),
],
```

### Which Monsters Use What

| Monster | Effects |
|---------|---------|
| Spiderling | ApplyPoison (1/turn, 3 turns, 40%) |
| Giant Spider | ApplyPoison (2/turn, 4 turns, 60%) |
| Plague Rat | ApplyPoison (2/turn, 5 turns, 50%) |
| Wraith | LifeDrain (3, 100%) |
| Hell Hound | ApplyBurning (2/turn, 3 turns, 50%) |
| Shadow Fiend | ApplySlow (3 turns, 40%) |
| Vampire | LifeDrain (5, 100%), AttributeDrain (str, 1, 8 turns, 30%) |
| Ogre | Knockback (2, 40%) |
| Troll | ApplySlow (2 turns, 25%) |
| Veiled Tyrant | LifeDrain (4, 30%), ApplySlow (3 turns, 25%) |

## Category 2: On-Hit Effects (Item)

Trigger when the player lands an attack (melee or ranged). Configured via
`bonuses` field in `items.ron` using `ItemBonus` variants.

| Bonus | Parameters | What It Does |
|-------|-----------|--------------|
| `OnHitPoison` | `chance`, `damage`, `duration` | Poison target |
| `OnHitBurn` | `chance`, `damage`, `duration` | Burn target |
| `OnHitSlow` | `chance`, `duration` | Slow target |
| `OnHitKnockback` | `chance`, `distance` | Push target |
| `OnHitStun` | `chance`, `duration` | Stun target |

### RON Format

```ron
bonuses: [
    OnHitPoison(chance: 25, damage: 2, duration: 4),
    OnHitStun(chance: 10, duration: 2),
],
```

## Category 3: Monster Passives

Always-on or event-triggered abilities. Each is its own ECS component,
configured via dedicated fields in `monsters.ron`.

### Defensive Passives

| Ability | RON Field | Parameters | What It Does |
|---------|-----------|-----------|--------------|
| **Poison Body** | `poison_body` | `i32` (stacks) | Poisons anyone who melees this entity |
| **Thorn Aura** | `thorn_aura` | `i32` (damage) | Reflects flat damage to melee attackers |
| **Reanimate** | `reanimate_hp` | `i32` (revive HP) | Revives once after first death |

### Offensive Passives

| Ability | RON Field | Parameters | What It Does |
|---------|-----------|-----------|--------------|
| **Enrage on Hit** | `enrage_on_hit` | `u32` (threshold %) | Gains +50% damage when HP drops below threshold |

### On-Death Effects

| Ability | RON Field | Parameters | What It Does |
|---------|-----------|-----------|--------------|
| **Explode on Death** | `explode_on_death` | `(i32, i32)` — (damage, radius) | AoE damage to nearby entities on death |
| **Death Curse** | `death_curse` | `DeathCurseEffect` | Debuffs the killer on death |
| **Summon on Death** | `summon_on_death` | `(String, u32)` — (monster_name, count) | Spawns monsters on death |

### DeathCurseEffect Variants
- `Slow { duration }` — Slow the killer
- `Poison { damage_per_turn, duration }` — Poison the killer
- `WeakenStr { amount, duration }` — Reduce killer's STR

### RON Format

```ron
// Defensive
poison_body: Some(2),
thorn_aura: Some(3),
reanimate_hp: Some(15),

// Offensive
enrage_on_hit: Some(50),  // triggers at 50% HP

// On-death
explode_on_death: Some((8, 1)),  // 8 damage, radius 1
death_curse: Some(WeakenStr(2, 10)),
summon_on_death: Some(("Skeleton", 2)),
```

### Which Monsters Use What

| Monster | Passives |
|---------|----------|
| Skeleton | Reanimate (8 HP) |
| Zombie | Reanimate (12 HP) |
| Lich Apprentice | Death Curse (WeakenStr 2, 10 turns) |
| Imp | Explode on Death (6 damage, radius 1) |
| Troll | Regen 4 (via `regen` field, not a passive component) |
| Goblin Shaman | Summon on Death ("Goblin", 1) |

## Category 4: Aura System

Radius-based passive effects applied to nearby allies or enemies each
turn. Uses the `Aura` component. Not yet exposed in `monsters.ron` —
currently code-only.

### AuraTarget
- `Allies` — Same faction
- `Enemies` — Hostile faction
- `All` — All entities (e.g., fountains)

### AuraEffect Variants
- `ArmorBonus(i32)` — Flat armor to affected entities
- `DamagePercent(i32)` — % melee damage bonus
- `RegenBonus(i32)` — Flat HP regen per turn

### Rust Format

```rust
Aura {
    radius: 3,
    target: AuraTarget::Allies,
    effects: vec![AuraEffect::ArmorBonus(2)],
}
```

## Category 5: Build-Defining Passives (Player Only)

Unlocked via Essence tree nodes. These are marker components on the
player entity — not configurable via RON. Relevant for understanding
what player abilities exist when designing counter-play or synergies.

### Fighter Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Cleave** | `Cleave` | On melee kill, deal weapon damage to random adjacent hostile |
| **Riposte** | `Riposte` | On enemy melee miss, auto-counterattack for 50% damage |
| **Exploit Opening** | `ExploitOpening` | +30% damage vs stunned/slowed/debuffed |
| **Weapon Mastery** | `WeaponMastery` | Consecutive hits on same target stack +10% (max 3) |

### Ranger Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Steady Shot** | `SteadyShot` | +50% ranged damage if didn't move last turn |
| **Piercing Shots** | `PiercingShots` | Ranged attacks ignore 50% armor |

### Tank Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Immovable** | `Immovable` | Immune to knockback; failed knockback stuns attacker 1 turn |
| **Guardian** | `Guardian` | Adjacent allies take 30% less damage |

### Berzerker Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Blood Rage** | `BloodRage` | Below 50%: +25% melee, +15% speed. Below 25%: +50% melee, +30% speed, -3 armor |
| **Reckless Strike** | `RecklessStrikeReady` | Next melee deals double, self-damage 25% current HP |
| **Bloodlust** | `Bloodlust` | On kill: heal 15% of killed enemy's max HP + Haste 2 turns |

### Shadow Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Ambush** | `Ambush` | First attack from outside enemy FOV deals double damage |
| **Crippling Strikes** | `CripplingStrikes` | On melee hit: -1 STR per stack (max 5) for 10 turns |
| **Exploit Weakness** | `ExploitWeakness` | +25% damage vs any debuffed enemy |

### Sorcerer Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Elemental Affinity** | `ElementalAffinity(DamageType)` | +25% damage for chosen element |
| **Spell Echo** | `SpellEcho { chance }` | Chance to re-cast spell for free |
| **Improved Mana Shield** | `ImprovedManaShield` | Spirit Shield absorbs at 2:1 ratio instead of 1:1 |
| **Arcane Feedback** | `ArcaneFeedback { mana_percent }` | Taking damage restores mana |

### Summoner Tree
| Ability | Component | What It Does |
|---------|-----------|--------------|
| **Soul Link** | `SoulLink` | When summon dies: gain 5 mana + summon explodes for 2d4 in radius 1 |

## Design Space: Unused Trigger Types

Trigger types that exist in other roguelikes but are NOT yet implemented.
Useful for brainstorming new abilities:

- **On block/dodge** — Reward defensive play
- **On spell cast** — Synergy with magic builds
- **On status applied** — Chain reactions (poison + fire = explosion?)
- **On ally death** — Sacrifice/revenge mechanics
- **Periodic/timed** — Every N turns, do X
- **On enter tile** — Terrain interaction (water + lightning = AoE?)
- **On low mana** — Desperation mechanics
- **On adjacency** — Triggered by being near specific entities
