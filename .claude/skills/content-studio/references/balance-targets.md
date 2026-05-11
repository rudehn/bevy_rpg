# Balance Targets

Floor-by-floor stat ranges for proposing balanced content. These are
target bands, not hard rules — an outlier is fine if it has identity
(the Stone Sentinel's 200 HP is intentional).

## How to use

1. Identify the target floor range.
2. Pick the *band* from each table.
3. Propose specific numbers, cross-reference an existing neighbor.
4. Review against Formulas (bottom of file) for edge cases.

## Player baseline

- Starting HP: 20 (see `assets/player.ron`)
- Starting damage: 1d4 unarmed
- Starting armor/dodge: 0
- Base action delay: 1.0 (movement + attack)

## Monster HP curve

Target HP for a monster on its *introduction floor* (first floor it can
spawn). Solo mini-bosses can exceed by 1.5×; swarm fodder should sit
near the bottom of the band.

| Floor band | Swarm | Standard | Brute | Mini-boss |
|---|---|---|---|---|
| 1–3 | 3–5 | 5–10 | 10–20 | 25 |
| 4–7 | 5–10 | 10–20 | 20–35 | 40–60 |
| 8–12 | 10–15 | 20–40 | 35–60 | 60–100 |
| 13–18 | 15–25 | 40–70 | 60–100 | 100–160 |
| 19–25 | 25–40 | 70–120 | 100–180 | 160–260 |
| 26 | — | — | — | Boss-class (>200) |

## Monster damage

Per-hit damage should kill the player in 4–8 hits if they stand still
with baseline gear. Faster enemies should sit at the low end; slow
enemies at the high end.

| Floor band | Light | Medium | Heavy |
|---|---|---|---|
| 1–3 | 1d2 | 1d3–1d4 | 1d6 |
| 4–7 | 1d3–1d4 | 1d6 | 1d6+1 – 1d8 |
| 8–12 | 1d4 | 1d6+1 | 1d10 – 2d6 |
| 13–18 | 1d6 | 2d6 | 2d8 – 2d6+2 |
| 19–25 | 1d8 | 2d6+2 | 2d10 – 3d6 |
| 26 | Boss: 2d8+ or special |

Typed damage (fire, lightning, poison): keep base dice the same; the
tradeoff is that typed damage bypasses armor but is subject to resistance.

## Monster armor / dodge

Armor reduces physical damage by a flat amount. Dodge raises the target
number the attacker must roll.

| Floor band | Light-armored | Mid-armored | Heavy-armored | Dodge-based |
|---|---|---|---|---|
| 1–4 | 0 armor | 1 armor | 2 armor | 1–2 dodge |
| 5–10 | 0–1 | 2 | 3–4 | 2–3 dodge |
| 11–18 | 1–2 | 3 | 5–7 | 3–5 dodge |
| 19–26 | 2–3 | 4 | 8–12 | 4–6 dodge |

## Monster movement/attack delay

Default 1.0. Lower = faster.

- 0.5 — very fast (twice-per-turn)
- 0.75 — fast (skirmishers, kiters)
- 1.0 — normal
- 1.2 — slow (brutes, bloats)
- 1.5+ — very slow (set-piece obstacles)

Split delays for archetypes: fast chasers with heavy hits use
`movement_delay: 0.8, attack_delay: 1.1`. See
[docs/design/ENEMIES.md](../../../docs/design/ENEMIES.md) for rationale.

## Monster vision

| Role | Range |
|---|---|
| Blind (stationary, environmental) | 0 |
| Low perception (swarm) | 4–6 |
| Default | 6–8 |
| Alert (guards, archers) | 10–12 |
| Hawk-eyed (snipers, bosses) | 14+ |

## Item rarity floors

From `assets/item_spawns.ron`. New items should respect these bands:

| Rarity | Earliest min_floor | Typical weight |
|---|---|---|
| Common | 1 | 3–5 |
| Uncommon | 2–3 | 2 |
| Rare | 5–7 | 1 |
| Legendary | 9+ | rare/unique only |

## Weapon damage dice

| Weapon type | Base dice | Attack speed |
|---|---|---|
| Dagger (fast) | 1d4 | 0.5 |
| Sword (balanced) | 1d6 | 1.0 |
| Axe (planned) | 1d8 | 1.2 |
| Spear (planned) | 1d6 | 1.0 |
| Mace (planned) | 1d6 | 1.1 |
| Bow (ranged) | 1d4 | 1.0 |

## Staff damage / charges / recharge

| Staff | Base dice | Recharge (turns) | Notes |
|---|---|---|---|
| Lightning | 2d6 | 250 | Single target, range 8 |
| Fire | 2d6 | 250 | 3×3 AoE, range 6 |
| Poison | 1d4 + DoT | 250 | Range 6 |
| Healing | 3d6 | 200 | Self only |
| Blinking | — | 400 | Teleport, range 8 |
| Force | 1d6 + knockback | 250 | Range 6 |

Enchant scroll on a staff: +1 charge, +1d6 damage or +1 effect level.

## Runic proc rates

See `src/game/enchantment.rs::WeaponRunic::base_rate`. Target band:
3–7% at +0. Powerful effects (Speed, Paralysis) sit low; damage adders
(Flames, Venom) sit higher.

## Runic appearance curve

Live in `src/game/enchantment.rs::runic_chance_for_floor`:

- Floors 1–4: 0%
- Floor N (≥5): `min(50, (N - 4) * 5 / 2)`%
- Cap at 50% by floor 24+

## Formulas

### Hit check
`1d20 + attacker.hit_bonus >= 4 + defender.dodge`

### Damage application (physical)
`max(1, rolled_damage + attacker.damage_bonus - defender.armor)`

### Damage application (typed: fire / lightning / poison)
`max(1, rolled_damage + attacker.damage_bonus) * (100 - resistance) / 100`
Armor does not reduce typed damage; only resistance does.

### Fall damage (chasm)
`2d6`, physical, source Environment. Applies to player and monsters.

### Enchant level from floor
`0 to (floor_depth / 3 + 2)` — floor 1: 0–2, floor 26: 0–10.
Note: this curve was sized for 10 floors. At floor 26 it produces +10
weapons, which may be excessive. Flag if a user asks to tune enchant.

## Cross-references

- Runic proc math: `src/game/enchantment.rs`
- Hit / damage pipeline: `src/game/combat.rs`
- Staff mechanics: `src/game/staves.rs`
- Status effects: `src/game/magic.rs`
- Action / speed pipeline: `src/game/actions.rs`, `src/game/turns.rs`
