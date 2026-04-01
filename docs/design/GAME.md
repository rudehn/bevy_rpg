# The Veiled Tyrant — Game Design

## Vision

A Brogue-inspired roguelike where a lone hero descends a 10-floor dungeon to
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
Enter floor
  -> Explore map (FOV-based, procedurally generated)
  -> Fight enemies (turn-based, d20 combat)
  -> Loot chests (all items come from chests)
  -> Grow stronger (equipment, staves, enchanting)
  -> Discover machines & encounters
  -> Find the down-stairs
Descend to next floor
  -> Repeat through floor 9
Floor 10: The Amulet
  -> Find the Amulet of Ascension
  -> Reach the escape portal
  -> Leave the dungeon
Victory
```

## Win Condition

On **floor 10**, the player must find the **Amulet of Ascension** and reach the
**Escape Portal**. Floor 10 is a full dungeon floor with normal encounters,
monsters, and machines — not a boss arena. The amulet and portal are placed far
apart, forcing the player to navigate the entire floor.

Reaching the portal while carrying the amulet ends the run with a victory screen.

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

Both player and monsters use this formula (symmetric combat).

### Damage Pipeline

```
AttackIntent
  -> hit_check (d20 + hit_bonus vs 4 + dodge_bonus)
  -> damage_roll (weapon dice + damage_bonus; x2 on crit)
  -> damage_reduction:
       Physical: (raw - armor).max(0), then apply resistance %
       Poison/Fire/Lightning: skip armor, apply resistance % only
  -> apply_damage (HP change, death check)
```

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

10 floors with escalating difficulty:

- Machines and encounters appear on **all floors**
- Floor difficulty scales via monster spawns, monster complexity, and liquid hazards
- Floor 10 contains the Amulet of Ascension and the Escape Portal
- All floors are explorable — no linear corridors to the exit
