# Magic System Design

## Overview

Magic in this game is not a class feature — any hero can learn and use spells. Spells are found as **spellbooks** in the dungeon, learned by reading them, and equipped into a limited number of **active spell slots**. Casting costs **mana**, which regenerates very slowly over time.

A pure melee hero can ignore magic entirely. A hybrid can carry 2-3 utility spells. A dedicated caster builds INT, fills all slots, and relies heavily on their spell arsenal.

---

## Mana

| Property | Value |
|----------|-------|
| Max Mana | `INT × 5` (e.g., INT 10 = 50, INT 14 = 70, INT 20 = 100) |
| Base Regen | `1 + floor(INT_bonus / 5)` mana per 5 turns (passive, always active) |
| Staff bonus | Future: reduces turns_between_regen (e.g., from 5 to 4) |
| Mana Potion | Restore 15 mana (instant) |

**Mana is scarce by design.** Higher INT increases both your pool AND your regen rate, with breakpoints at INT 15 and 20 that make stat investment feel rewarding.

**Regen by INT:**

| INT | Bonus | Regen/5 turns | Mana/10 turns |
|-----|-------|---------------|---------------|
| 10  | 0     | 1             | 2             |
| 14  | 4     | 1             | 2             |
| 15  | 5     | 2             | 4             |
| 18  | 8     | 2             | 4             |
| 20  | 10    | 3             | 6             |

**Mana budget per encounter** (10-turn fight):
- INT 10: 50 pool + 2 regen = **52 total**
- INT 14: 70 pool + 2 regen = **72 total**
- INT 15: 75 pool + 4 regen = **79 total**
- INT 20: 100 pool + 6 regen = **106 total**

**Future enhancement hooks**:
- Boss traits: "Mana Font" reduces regen interval to 3 turns
- Equipment: "Staff of Focus" reduces regen interval by 1
- Arcane Surge spell: temporarily reduces regen interval by 2

---

## Spell Slots

Spell slots are unlocked as the player levels up:

| Level | Slots Total |
|-------|-------------|
| 1 | 1 |
| 3 | 2 |
| 5 | 3 |
| 8 | 4 |
| 11 | 5 |
| 14 | 6 |

The player can equip any known spell into any slot. Swapping spells is done from the inventory screen and takes no turn.

---

## Acquiring Spells

1. Find a **spellbook** (Tome) in the dungeon as loot
2. **Use** the spellbook from inventory
3. The spell is permanently added to the player's **known spells list**
4. Open the spell management screen and slot the spell into an active slot

Known spells are never lost. Only the active slots limit what can be cast at once.

---

## Spell Targeting

| Target Type | Description | UI |
|-------------|-------------|-----|
| `Castor` | Affects the caster only | No targeting; instant cast |
| `Enemy` | Targets a visible enemy | Cursor targeting |
| `Ally` | Most-wounded visible ally (not self) | Cursor targeting (filter to friendlies) |
| `AllyOrSelf` | Most-wounded ally or self | Cursor targeting (friendlies + self) |

`Ally` and `AllyOrSelf` apply to both heal and buff spells.

---

## Spell Effects

| Effect | Description |
|--------|-------------|
| `Damage` | Deal dice damage to target; optionally scaled by INT bonus |
| `Heal` | Restore HP to target (whoever SpellTarget resolved to); optionally INT-scaled |
| `AoeDamage` | Damage all entities in radius around target tile |
| `ChainDamage` | Hit primary target, then jump to nearby enemies |
| `Buff` | Temporarily boost target's stat via AttributeModifiers |
| `Debuff` | Temporarily reduce target's stat |
| `ApplyPoison` | Apply damage-over-time status |
| `ApplyHaste` | +50% speed (delay × 0.5) for N turns |
| `ApplySlow` | -50% speed (delay × 1.5) for N turns |
| `DrainMana` | Remove mana from target, add to caster |
| `SpiritShield` | Damage taken from mana instead of HP for N turns |
| `Teleport` | Move caster; range=0 → random, range=N → controlled |

### INT Scaling Policy
- **Scales with INT (reward investment):** magic_missile, ice_shard, shadow_bolt, lightning_bolt, chain_lightning, death_coil, heal_self, heal_ally, cure_wounds, greater_heal, mana_drain
- **Fixed effect (reward smart usage):** spark, fire_dart, poison_bolt, vampiric_strike, fireball, meteor, minor_heal, all buffs, all debuffs, haste, slow, blink, teleport, spirit_shield

---

## Spell List

### Attack Spells (Target: Enemy)

| Spell | Tier | Mana | CD | Effects | INT Scale | Avg @INT14 | Eff (dmg/mana) | Notes |
|-------|------|------|----|---------|-----------|------------|----------------|-------|
| `spark` | 1 | 3 | 0 | Damage 1d4 | No | 2.5 | 0.83 | Cantrip; no cooldown, spammable |
| `magic_missile` | 1 | 5 | 4 | Damage 1d4 | Yes | 6.5 | 1.30 | Workhorse; grows with INT |
| `fire_dart` | 1 | 8 | 3 | Damage 1d8 | No | 4.5 | 0.56 | Higher burst; fire themed |
| `ice_shard` | 2 | 10 | 4 | Damage 2d4 | Yes | 9.0 | 0.90 | Upgrade from magic_missile |
| `poison_bolt` | 2 | 12 | 6 | Damage 1d4 + Poison(2/t, 4t) | No | 10.5 total | 0.88 | DoT; damage over 5 turns |
| `vampiric_strike` | 2 | 12 | 4 | Damage 2d4 + Heal 1d4 | No | 5.0 + 2.5 heal | 0.42 + sustain | Life steal |
| `shadow_bolt` | 3 | 18 | 5 | Damage 2d8 | Yes | 13.0 | 0.72 | Necrotic themed |
| `lightning_bolt` | 3 | 20 | 6 | Damage 3d6 | Yes | 14.5 | 0.73 | Big single-target nuke |
| `fireball` | 3 | 22 | 8 | AoeDamage 2d6, radius 1 (3×3) | No | 7.0 × N targets | 0.32+ | Destroys wooden doors/items; AoE |
| `chain_lightning` | 3 | 25 | 8 | ChainDamage 2d6 + 2 jumps(1d6), 2 tiles | Yes | 11.0 + 7.0 splash | 0.72 | Jumps between enemies |
| `death_coil` | 4 | 30 | 8 | Damage 4d6 | Yes | 18.0 | 0.60 | Highest single-target |
| `meteor` | 4 | 35 | 10 | AoeDamage 3d8, radius 1 (3×3) | No | 13.5 × N | 0.39+ | Ultimate AoE |

### Healing Spells

| Spell | Tier | Target | Mana | CD | Effects | INT Scale | Avg @INT14 | Eff (heal/mana) | Notes |
|-------|------|--------|------|----|---------|-----------|------------|-----------------|-------|
| `minor_heal` | 1 | Castor | 4 | 2 | Heal 1d4 | No | 2.5 | 0.63 | Emergency patch; cheap |
| `heal_self` | 1 | Castor | 8 | 8 | Heal 1d6 | Yes | 7.5 | 0.94 | Sustained self-heal |
| `heal_ally` | 2 | Ally | 12 | 5 | Heal 2d4 | Yes | 9.0 | 0.75 | Shaman support spell |
| `cure_wounds` | 2 | AllyOrSelf | 15 | 6 | Heal 2d6 | Yes | 11.0 | 0.73 | Flexible; whoever needs it most |
| `greater_heal` | 3 | AllyOrSelf | 25 | 8 | Heal 3d8 | Yes | 17.5 | 0.70 | Big emergency heal |

### Buff Spells

| Spell | Tier | Target | Mana | CD | Effect | Duration | Stat-Turns/Mana | Notes |
|-------|------|--------|------|----|--------|----------|-----------------|-------|
| `enrage` | 1 | Castor | 8 | 10 | Buff STR +4 | 6 turns | 3.0 | Damage boost; short burst |
| `fortify` | 1 | Castor | 8 | 12 | Buff CON +3 | 10 turns | 3.75 | Effective +30 max HP at lvl 10 |
| `haste` | 2 | Castor | 10 | 12 | ApplyHaste (+50% speed) | 8 turns | — | Massive tactical value |
| `haste_ally` | 2 | Ally | 12 | 10 | ApplyHaste (+50% speed) | 8 turns | — | Buff an ally with double speed |
| `iron_skin` | 2 | Castor | 12 | 15 | Buff Armor +3 | 10 turns | 2.5 | -3 damage per hit |
| `battle_hymn` | 2 | AllyOrSelf | 15 | 15 | Buff STR +2, AGI +2 | 8 turns | 2.13 | Squad buff; support role |
| `arcane_surge` | 3 | Castor | 20 | 20 | Buff INT +6 | 8 turns | 2.4 | Amplifies all INT-scaling spells |
| `spirit_shield` | 3 | Castor | 20 | 25 | SpiritShield | 10 turns | — | Damage taken from mana, not HP |

### Debuff Spells (Target: Enemy)

| Spell | Tier | Mana | CD | Effect | Duration | Stat-Turns/Mana | Notes |
|-------|------|------|----|--------|----------|-----------------|-------|
| `weaken` | 1 | 8 | 10 | Debuff STR -3 | 8 turns | 3.0 | Reduces enemy damage |
| `slow` | 2 | 10 | 10 | ApplySlow (-50% speed) | 8 turns | — | Trivializes fast enemies |
| `curse` | 3 | 18 | 15 | Debuff STR -2, DEX -2, CON -2 | 10 turns | 3.33 | Multi-stat debuff |
| `mana_drain` | 3 | 10 | 8 | DrainMana 15 | — (instant) | 1.5-1.9 net+ | Disrupts casters; INT-scaled |

### Utility Spells (Target: Castor)

| Spell | Tier | Mana | CD | Effect | Notes |
|-------|------|------|----|--------|-------|
| `blink` | 2 | 8 | 6 | Teleport range=3 | Controlled short-range teleport |
| `teleport` | 3 | 15 | 20 | Teleport range=0 | Random destination; escape button |

---

## Spellbook Availability by Zone

| Zone | Floors | Available Spellbooks |
|------|--------|---------------------|
| 1 | 1-5 | spark, magic_missile, minor_heal, fire_dart |
| 2 | 6-10 | ice_shard, poison_bolt, vampiric_strike, heal_self, heal_ally, enrage, fortify, weaken, haste, blink |
| 3 | 11-16 | shadow_bolt, lightning_bolt, fireball, chain_lightning, cure_wounds, haste_ally, iron_skin, battle_hymn, slow |
| 4 | 17-21 | death_coil, greater_heal, arcane_surge, spirit_shield, curse, mana_drain, teleport |
| 5 | 22-26 | meteor |

Spellbooks are uncommon loot — expect to find 0-2 per floor on average.

---

## Monster Caster Assignments

| Monster | INT | Spells | Role |
|---------|-----|--------|------|
| Goblin Shaman | 14 | magic_missile, heal_ally | Squad healer |
| Orc Shaman | 16 | ice_shard, fire_dart, heal_self | Aggressive caster |
| Lich Apprentice | 14 | magic_missile, heal_self | Conservative |
| Ogre Mage | 18 | lightning_bolt, fire_dart, vampiric_strike | Heavy hitter + sustain |
| Imp | 12 | fire_dart, spark | Glass cannon; spams cheap fire |
| Vampire | 14 | vampiric_strike, heal_self | Life-steal + sustain |
| Shadow Fiend | 14 | shadow_bolt, mana_drain | Disrupts player casting |
| Lich | 20 | lightning_bolt, death_coil, vampiric_strike | Boss-tier caster |
| Goblin Warchief | 10 | enrage | Self-buff before combat |
| Orc Warlord | 10 | battle_hymn | Squad buff |

---

## Design Notes

- **Mana scarcity is the primary constraint.** At 1 per 5 turns regen, players must conserve mana across encounters.
- **Spell slots create meaningful choices.** A player must decide which 3 (or 6 at endgame) spells define their run.
- **No spell leveling** — spells don't level up. Power comes from INT stat growth and finding better spells.
- **Friendly fire on Fireball** is intentional. Positional play should matter.
- **INT scaling rewards investment.** Signature spells (magic_missile, lightning_bolt, death_coil) grow meaningfully with INT. Utility spells have fixed effects to reward smart usage regardless of build.
- **Haste/Slow are speed multipliers, not stat buffs.** +50%/-50% speed is applied AFTER normal AGI-based delay calculation. These are game-changing effects with appropriately high costs.
- **Spirit Shield** creates a mana-as-HP dynamic — extremely powerful but drains your casting resources.
- **Vampiric Strike works with existing multi-effect system.** Damage hits the enemy; Heal heals the caster. No special code needed.
