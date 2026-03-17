# Encounter Design Principles

Tactical design guidance for creating prefabricated dungeon encounters in The Veiled Tyrant.

## Encounter Archetypes

| Archetype | Player Experience | Key Geometry | Typical Roles |
|-----------|-------------------|--------------|---------------|
| Sentinel gauntlet | Push through fortified position | Barricade lines, narrow approach | melee_guard + ranged |
| Trapped treasure | High reward behind danger | Enclosed room, single entry | brute or caster + melee_guard |
| Ambush corridor | Walk into kill zone, react fast | Long hall, side alcoves | ranged + melee flankers (guard: false) |
| Ritual disruption | Interrupt caster before spell completes | Open center, peripheral cover | caster (center) + melee_guard |
| Monster lair | Fight in creature's territory | Organic cave shape, debris | brute + any |
| Patrol checkpoint | Guards at chokepoint, sneak or fight | Corridor with pillars/barricades | melee_guard (guard: true) |
| Puzzle room | Layout rewards positioning over power | Unusual geometry, terrain features | varied, fewer monsters |

## Tactical Geometry Patterns

- **L-shaped cover** — Barricades form an L, ranged unit behind the corner. Player must approach from exposed angle or flank around.
- **Funnel chokepoint** — Barricades/pillars narrow approach to 1-2 tiles. Melee guards hold the gap, ranged fires over.
- **Split approach** — Two entrances force player to choose. Monsters positioned to cover both.
- **Barrel maze** — Barrels create winding path. Monsters at intersections create ambush points.
- **Room-within-room** — Wall-carved inner chamber with single door. Defenders inside, player must breach.
- **Diamond/ring formation** — Props arranged in diamond/ring around central high-value target (caster, chest, structure).
- **Elevated position** — Ranged unit behind cover with clear sight lines, melee guards at base blocking approach.

## Squad Composition Heuristics

| Composition | Tactical Dynamic |
|-------------|-----------------|
| ranged + melee_guard | Cover-and-fire: ranged behind barricade, melee blocks approach |
| caster + brute | Priority dilemma: kill the caster quickly, but brute is in the way |
| leader + melee_guard + ranged | Combined arms: killing leader triggers behavior change |
| brute alone | Simple but dangerous: high damage gatekeeper |
| multiple melee (guard: false) | Swarm/ambush: flankers close from multiple directions |

## Squad Behavior Selection Guide

| Encounter Drama | on_leader_death | flee_threshold | Reasoning |
|-----------------|-----------------|----------------|-----------|
| Desperate defenders | `fight_on` | 0.15-0.20 | They have nowhere to run |
| Disciplined soldiers | `fight_on` | 0.25-0.30 | Hold the line |
| Aggressive mob | `enrage` | 0.20-0.25 | Rage makes them dangerous when cornered |
| Raiders/bandits | `scatter` | 0.35-0.40 | Self-preservation over loyalty |
| Cowardly ambushers | `flee` | 0.40-0.50 | Only fight with advantage |

## Guard vs. Roam

- **`guard: true`** — Monster patrols near its spawn point (3-tile radius). Use for sentries, defenders, anything holding a position.
- **`guard: false`** — Monster roams freely. Use for flankers, ambushers, patrols that should chase the player.
- **Mix both** in a single prefab for interesting dynamics — guards hold position while flankers pursue.

## Orientation Guidance

- **Allow both rotate + flip** (default) — For symmetric designs or when approach direction doesn't matter. Most prefabs should use this.
- **Rotate only, no flip** — When the prefab has left/right asymmetry that matters tactically (e.g., cover is on one side only).
- **No rotate, no flip** — When the prefab depends on a specific directional relationship (rare — most designs work in any orientation).

## Reward Scaling

| Encounter Size | Reward Expectation |
|---------------|-------------------|
| 0-1 monsters | 0-1 props (candle, barrel), maybe a small_chest |
| 2 monsters | 1 chest or 1-2 useful props |
| 3+ monsters | 1-2 chests + structure or multiple props |
| Landmark (4-6 monsters) | Significant rewards — multiple chests, structure, item spawns |

Risk must match reward. Empty rooms with high danger feel unfair; easy rooms with rich loot feel unearned.

## Depth & Difficulty Tiers

| Tier | Floors | Monster Count | Tactical Complexity |
|------|--------|--------------|---------------------|
| Easy | 1-5 | 1-2 | Simple tactics, single role encounters |
| Medium | 3-10 | 2-3 | Combined roles, basic squad behavior |
| Hard | 6-15 | 3-4 | Complex tactics, multi-role squads |
| Landmark | 8-20 | 4-6 | Major encounters, full squad dynamics |
| Late game | 15-26 | Toughest | Most challenging configurations |

Overlap between tiers is intentional — depth ranges should blend so the player experiences gradual escalation rather than sharp jumps.

## Anti-Patterns

- **Decorative only** — Prefab has interesting geometry but no tactical purpose. Every wall, barrel, and prop should affect how the fight plays out.
- **Impossible approach** — No way to engage without taking guaranteed hits. Player should always have a decision to make.
- **Unwinnable odds** — Too many monsters for the depth range. Reference existing prefabs: floors 1-5 have 1-2 monsters, not 4.
- **Empty loot room** — Rich rewards with no challenge. Even "unguarded" caches should have nearby threats or trade-offs.
- **Redundant design** — Too similar to an existing prefab. Check the catalog before finalizing. If the tactical situation is the same, it's redundant even if the geometry differs.
- **Overly complex geometry** — Prefabs larger than necessary for the encounter. A 5x5 single-monster room doesn't need to be 10x10. Tight geometry creates more interesting decisions.
- **Ignoring terrain** — Never using doors, never varying tile types. Doors create information asymmetry (what's behind the door?). Mixed terrain creates movement decisions.

## Brogue Design Philosophy Applied to Prefabs

1. **Tactical depth from simple rules** — A barricade + ranged monster creates more interesting decisions than a room full of melee enemies. Prefer fewer monsters with terrain interaction over more monsters in open space.

2. **Environmental storytelling** — The prefab layout should suggest a story. A watchfire with barricades is a camp. Barrels arranged in a maze with monsters at intersections is an ambush. A caster surrounded by totems is a ritual.

3. **Transparent systems** — The player should be able to look at a prefab and understand the tactical situation. Sight lines, cover positions, and approach angles should be readable from FOV.

4. **Meaningful risk/reward** — Every prefab presents a choice: engage for the reward, or skip and conserve resources. The value of the reward should be proportional to the risk.

5. **Terrain as weapon** — Doors block sight. Barricades block movement. Chokepoints limit approach angles. Use these as design tools, not just decoration.
