# The Veiled Tyrant — Game Design

## Vision

A Brogue-inspired roguelike where a lone hero descends a 26-floor dungeon to
retrieve the **Amulet of Ascension** and escape through a portal. Death is
permanent. Every run is procedurally generated. Victory is hard-earned.

No classes, no shops, no meta-progression. The hero's identity emerges from
what they find and how they enchant it. The dungeon is dangerous, readable,
and rewards careful play.

## Design Pillars

1. **Exploration first** — The dungeon should feel worth exploring. Secrets,
   variety, machines, and environmental storytelling on every floor.
2. **Risk vs. reward** — Every decision has a cost. Opening a trapped chest,
   wading through deep water, pushing deeper instead of retreating.
3. **Emergent builds** — No fixed class. Each run's identity comes from which
   weapons, staves, and enchantments the player finds and combines.
4. **Readable danger** — Enemies telegraph their threat level. The player should
   be able to make informed decisions before committing to a fight.
5. **Symmetric combat** — Player and monsters share the same stat system and
   combat formulas. A buff or debuff works the same regardless of who receives it.

## Core Gameplay Loop

```
Enter floor 1 (Escape Portal present but inert)
  -> Explore map (FOV-based, procedurally generated)
  -> Fight enemies (turn-based, d20 combat)
  -> Loot chests (all items come from chests)
  -> Grow stronger (equipment, staves, enchanting)
  -> Discover machines & encounters
  -> Find the down-stairs
Descend to next floor
  -> Repeat through floor 25
Floor 26: The Amulet
  -> Find the Amulet of Ascension (no down-stairs here)
  -> Ascend back up through floors 25 → 1
  -> Floors ascended are restored from cache (same layout,
     surviving monsters, fallen chasm victims, etc.)
  -> Reach the Escape Portal on floor 1
Victory
```

## Win Condition

The player must retrieve the **Amulet of Ascension** from **floor 26** and
carry it back to the **Escape Portal** on **floor 1**. Floor 26 is a full
dungeon floor with normal encounters, monsters, and machines — not a boss
arena — and has no down-stairs. The only way out is back up.

The Escape Portal stands at the player's starting tile on floor 1. Stepping
on it without the amulet prints a flavor message and does nothing. Stepping
on it while carrying the amulet ends the run with a victory screen.

Ascending is not free: every floor the player revisits has been snapshotted,
so surviving monsters are still present, fire/gas/water state persists, and
any enemies that fell through chasms on the way down are waiting on the
floor below. The climb back is a second gauntlet, not a victory lap.

## Lose Condition

The player's HP reaches 0. The character is dead. The run is over. A death summary
screen shows how far they got, what they were carrying, and how they died. A new
run starts fresh.

**No revives. Full permadeath.** The game supports save-and-quit (resumed on load,
deleted on death).

## Tone

Classic heroic fantasy. The dungeon is dangerous and atmospheric but not grimdark.
Enemies have personality. Item descriptions have flavor. The writing is dry, wry,
and occasionally ominous — in the tradition of Nethack and Brogue.

The hero is unnamed and classless — a blank slate shaped by the run.

## Progression

Character power comes from equipment and shrine upgrades. There are no levels,
no XP, no attributes, and no meta-progression between runs.

The four pillars of character power:

- **Shrines** define playstyle (how you fight) — permanent upgrades purchased
  with essence at shrine stations found throughout the dungeon
- **Equipment** provides raw stats (weapon type, armor weight, accessories)
- **Staves** provide magical tools (charges, not mana)
- **Enchanting** is the core strategic decision (which item to invest in)

The player starts weak and becomes powerful through what they find in chests,
how they allocate their limited enchant scrolls, and which shrine upgrades they
invest in.

### Essence

Monsters drop **essence** on death. Essence is a currency spent at shrines for
permanent upgrades. The amount of essence dropped scales with monster difficulty
and floor depth. Essence is per-run — it does not persist between runs.

## Combat System

### Hit Check (d20)

```
Attacker rolls: d20 + hit_bonus
Target number:  4 + target_dodge_bonus

If roll >= target: hit
If roll < target:  miss (turn still consumed)
If natural 20:     critical hit (always hits, double damage dice)
```

Both player and monsters use this formula. The `hit_bonus` and
`dodge_bonus` values come from the same per-entity components (`HitBonus`,
`Dodge`) and the formula is identical for both — but **the inputs are
asymmetric:** the player's `HitBonus` is baked from attribute mods +
class_attack_bonus + equipment at spawn, while monsters' come from
authored RON values + their own equip flow. See
[CHARACTER.md](CHARACTER.md) §Combat Math Integration for which attribute
mod feeds which derived stat on the player side.

**Halfling Lucky:** if the attacker has `Race::Halfling` and rolls a
natural 1, the roll is replayed once and the second result is taken
(no cooldown). Implemented in `roll_d20_with_race`
([src/character/dice.rs](../../src/character/dice.rs)) — every player
d20 site routes through this helper.

### Damage Pipeline

```
AttackIntent
  -> hit_check (d20 + hit_bonus vs 4 + dodge_bonus)
                  ↑ Halfling Lucky reroll on nat-1
  -> damage_roll (weapon dice + damage_bonus; x2 on crit)
                                ↑ STR_mod for melee, DEX_mod for ranged
                                  (preview-only for ranged today — the
                                  baked HitBonus uses STR. See
                                  CHARACTER.md §Combat Math Integration.)
  -> damage_reduction:
       Physical: (raw - armor).max(0), then apply resistance %
                                              ↑ Dwarf Stoneblood: +50%
                                                poison resistance at spawn
       Poison/Fire/Lightning: skip armor, apply resistance % only
  -> apply_damage (HP change, death check)
```

**Staff zaps** (Lightning / Fire / Force) add `INT_mod.max(0)` from the
zapper's `Attributes` to each damage event. INT can never *reduce* a
staff's base damage — a low-INT Mage just zaps for normal damage.

### Damage Types

| Type | Armor Applied? | Resistance Applied? | Unique Property |
|------|---------------|---------------------|-----------------|
| Physical | Yes (flat subtraction) | Yes (%) | Standard melee and ranged attacks |
| Poison | No | Yes (%) | Stacks: re-application resets duration to full; multiple sources accumulate independently |
| Fire | No | Yes (%) | Destroys wooden doors; burning DoT is fire-based |
| Lightning | No | Yes (%) | Can chain jump to additional enemies |

Physical hits the full reduction chain: flat armor first, then percentage
resistance. Poison, Fire, and Lightning skip flat armor — only their
respective resistance applies.

### Resistances

Percentage-based, stored per damage type (default 0 for all):

| Value | Effect |
|-------|--------|
| 0 | No resistance (default) |
| 50 | 50% damage reduction |
| 100 | Immune (0 damage) |
| >100 | Heals (damage absorbed and converted to healing) |
| Negative | Vulnerability (takes extra damage) |

Applies symmetrically to player and monsters.

## Player Base Stats

| Stat | Starting Value | Increased By |
|------|---------------|--------------|
| HP | 25 | Equipment, enchanting |
| Hit Bonus | 0 | Equipment |
| Dodge Bonus | 0 | Equipment |
| Armor | 0 | Equipment |
| Damage | 1d2 (unarmed) | Weapon equipped |
| Action Delay | 1.0x (baseline) | Equipment |
| Vision Range | 8 tiles (min 4) | Equipment |

No mana, no spell slots. Magic is delivered via staves (see ITEMS.md).

## Health & Regen

- **Starting HP:** 25
- **Regen:** HP regenerates slowly over time
- **Regen suppression:** Regen is suppressed for 5 turns after taking damage.
  This creates a "recover between fights" pacing — the player heals up in
  corridors, not mid-combat.

## Equipment Slots

9 slots total:

| Slot | Examples |
|------|---------|
| Weapon | Sword, Dagger, Staff |
| Off-hand | Shield |
| Helm | Iron Helm, Leather Cap |
| Chest | Plate Armor, Robe, Chainmail |
| Gloves | Gauntlets, Leather Gloves |
| Boots | Iron Boots, Soft Boots |
| Ring (x2) | Two ring slots |
| Amulet | One amulet slot |

See ITEMS.md for full equipment details.

## Speed & Turn Order

```
action_delay = base_cost * delay_multiplier
delay_multiplier starts at 1.0
```

Lower delay = more turns per cycle. Feeds into the TurnManager queue where all
actors (player and monsters) are sorted by game time.

## Death Screen

On death, show:
- Floor reached
- Equipment carried
- Cause of death
- Enemies killed
- Turns survived

## Floor Structure

26 floors with escalating difficulty:

- Machines and encounters appear on **all floors**
- Floor difficulty scales via monster spawns, monster complexity, and liquid hazards
- **Floor 1** contains the Escape Portal (at the player's start tile). Inert without the Amulet.
- **Floor 26** contains the Amulet of Ascension. No down-stairs — the only exit is the climb back up.
- Visited floors are cached and restored on ascent: same layout, same surviving monsters, same liquid/fire/gas state
- All floors are explorable — no linear corridors to the exit
