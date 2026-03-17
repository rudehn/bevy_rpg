# Faction Design Guide

How to build a cohesive faction roster for The Veiled Tyrant.
Sourced from existing faction analysis in `assets/monsters.ron` and `docs/design/BESTIARY.md`.

## Roster Template

Every faction needs members across power tiers that span its floor range:

| Tier | Count | Purpose | Floor Position |
|------|-------|---------|----------------|
| **Fodder** | 1-2 | Weak alone, dangerous in groups. Tests AoE and resource management. | Bottom of range |
| **Standard** | 1-2 | Core encounter unit. The most common representative of the faction. | Middle of range |
| **Elite** | 1 | Dangerous solo or as squad leader. Forces tactical decisions. | Upper range |
| **Boss candidate** | 0-1 | Could anchor a boss encounter. Represents faction at peak power. | Top of range |

**Why this structure works:** Fodder introduces the faction's theme cheaply. Standard monsters establish the tactical identity. Elites force the player to engage with faction mechanics deliberately. Boss candidates create memorable encounters.

## Mechanical Identity

Every faction needs 1-2 signature mechanics that unify its members and create a distinct tactical challenge.

### Choosing Signature Mechanics

Good signature mechanics:
- **Affect player decision-making** — The player does something different against this faction
- **Scale across the roster** — Fodder has a mild version, elites have a strong version
- **Interact with other systems** — Synergize with items, spells, terrain, or other factions
- **Are readable** — The player can learn and adapt to the mechanic

Bad signature mechanics:
- Purely numerical (just "more HP" or "more damage")
- Require specific items to counter (hard gate, not a decision)
- Don't differentiate from existing factions

### Examples from Existing Factions

| Faction | Signature Mechanics | How It Affects Play |
|---------|--------------------|--------------------|
| **Vermin** | Fast + cowardly + poison | Player must chase fleeing enemies; poison creates resource pressure |
| **Goblinoid** | Numbers + scatter on leader death | Kill the leader to break the group; positioning matters in large fights |
| **Undead** | Reanimate + necrotic damage | Must over-kill reanimators; necrotic resistance becomes valuable |
| **Orcish** | Brute force + enrage on leader death | Killing the leader makes survivors MORE dangerous; timing matters |
| **Demonic** | Fire damage + high individual power | Fire resistance valuable; each demon is a serious threat alone |
| **Giant** | Massive HP + slow + explosive attacks | Kiting works; burst damage preferred over sustained |
| **Dark** | Life drain + high stats + death curse | Anti-sustain faction; punishes melee builds without preparation |

## Role Synergies

How monster roles interact within squads to create tactical puzzles:

### Squad Composition Patterns

**Frontline + Support** (most common):
- `melee_guard` blocks corridors while `caster` or `ranged` deals damage from behind
- Player must choose: fight through the guard or find a way to reach the backline
- Example: Goblin (melee_guard) + Goblin Archer (ranged)

**Leader + Followers**:
- `leader` provides passive benefits (buff aura, summons) while followers engage
- Killing the leader triggers `on_leader_death` behavior (scatter/enrage)
- Example: Goblin Warchief (leader) + Goblins + Goblin Archers

**Brute + Swarm**:
- Single `brute` absorbs attention while fodder surrounds the player
- Player must decide: focus the brute or clear the swarm first
- Example: Ogre (brute) + Imps

**Caster + Tank**:
- `caster` is the real threat but protected by high-armor `melee_guard` or `brute`
- Player needs to maneuver past the tank or use ranged/spell attacks
- Example: Orc Shaman (caster) + Orc (melee_guard)

### Squad Behaviors

**on_leader_death** options:
- `"scatter"` — Best for cowardly/organized factions (Goblins). Group breaks up, easier to pick off individually.
- `"enrage"` — Best for aggressive factions (Orcs). Killing the leader makes the fight harder temporarily. Forces the player to decide: kill leader first (trigger enrage) or last (leader keeps buffing)?
- `""` (fight_on) — Best for mindless/fanatic factions (Undead). Group continues fighting regardless.

**flee_threshold** guidelines:
- 0.25-0.30 — Brave (Orcs, Demons). Only flee when almost wiped.
- 0.35-0.45 — Normal (Goblins). Flee when group is clearly losing.
- 0.50+ — Cowardly (Vermin). Flee early. Requires `is_cowardly: true` on individual monsters.

## Ability Distribution

Two approaches for distributing faction mechanics across the roster:

### Shared Traits (all members get X)
Best when the mechanic IS the faction identity:
- All Undead deal necrotic damage
- All Vermin are cowardly
- All Demonic creatures deal fire damage

### Specialist Abilities (only some members get Y)
Best when abilities should escalate with power tier:
- Only Goblin Shaman casts spells (fodder Goblins are melee only)
- Only Lich Apprentice has death_curse (basic Skeletons don't)
- Only Orc Berserker has enrage_on_hit (regular Orcs don't)

### Recommended Pattern
Combine both: 1 shared trait + escalating specialist abilities.

Example (Undead):
- **Shared**: All deal necrotic damage, all immune to necrotic
- **Fodder (Skeleton)**: No special abilities, just fights
- **Standard (Zombie)**: Slow but high HP, harder to kill
- **Standard (Wraith)**: Fast, life drain on hit
- **Elite (Lich Apprentice)**: Casts necrotic spells, death curse on kill, reanimates

## Themed Loot Principles

Faction-themed items should:
1. **Reflect the faction's mechanics** — Undead items have necrotic bonuses, Goblin items are crude but fast
2. **Counter the faction** — Items that are effective against the faction they drop from (resistance gear)
3. **Be appropriate rarity** — Fodder drops common, elites drop uncommon/rare, boss candidates drop rare/legendary
4. **Appear on appropriate floors** — Match the faction's floor range

### Loot Design Checklist
- [ ] At least 1 weapon themed to the faction
- [ ] At least 1 defensive item (armor, ring, or amulet)
- [ ] Optional: consumable or spellbook if the faction has magical elements
- [ ] Items use only existing bonus types from `ron-schemas.md`

## Floor Range Sizing

A faction should span **6-10 floors** for adequate exposure. This ensures:
- Players encounter the faction across multiple floor generations
- Power tiers have room to spread out (fodder early, elite late)
- Faction overlap creates interesting mixed encounters in transition zones

### Current Faction Floor Ranges

| Faction    | Floor Range | Span | Overlap With                |
|------------|-------------|------|-----------------------------|
| Vermin     | 1-8         | 8    | Goblinoid (1-10)            |
| Goblinoid  | 1-10        | 10   | Vermin (1-8), Undead (6-16) |
| Undead     | 6-16        | 11   | Goblinoid (6-10), Orcish (9-17) |
| Orcish     | 9-17        | 9    | Undead (9-16), Demonic (13-20) |
| Demonic    | 13-20       | 8    | Orcish (13-17), Giant (14-20), Dark (17-20) |
| Giant      | 14-20       | 7    | Demonic (13-20), Dark (17-20) |
| Dark       | 17-20       | 4    | Demonic, Giant              |

### Overlap Zones
Where two factions share floors, the game creates mixed encounters that test the player's ability to handle both mechanics simultaneously. Plan overlap deliberately — 2-4 floors of overlap creates transition tension.

## Existing Faction Analysis

### Vermin (Floors 1-8)
**Roster:** Rat (fodder), Giant Bat (fodder), Spiderling (fodder), Giant Spider (standard), Plague Rat (standard)
**Signature mechanics:** Fast (high AGI), cowardly (flee when losing), poison (Spiderling/Giant Spider/Plague Rat)
**Squad behavior:** Packs with flee_threshold 0.3-0.5. No leader structure — they scatter naturally.
**Tactical identity:** Harassment faction. They poison and run. Player must chase or use AoE. Teaches new players about poison management and that not all fights are stand-and-trade.

### Goblinoid (Floors 1-10)
**Roster:** Goblin (fodder), Goblin Archer (standard/ranged), Goblin Shaman (standard/caster), Goblin Warchief (elite/leader)
**Signature mechanics:** Numbers + organized squads + scatter on leader death
**Squad behavior:** Mixed groups with Warchief leader. on_leader_death: scatter, flee_threshold: 0.3. Composite spawn groups combine melee + ranged + leader.
**Tactical identity:** The "organized enemy" faction. Teaches squad tactics — kill the leader to break cohesion, use corridors to negate numbers advantage. Goblin Shaman introduces spell-casting enemies.

### Undead (Floors 6-16)
**Roster:** Skeleton (fodder), Bone Archer (standard/ranged), Zombie (standard/brute), Wraith (standard/skirmisher), Lich Apprentice (elite/caster)
**Signature mechanics:** Necrotic damage + necrotic immunity + reanimate (Skeleton, Zombie) + death curse (Lich)
**Squad behavior:** fight_on (empty on_leader_death) — Undead don't care if their leader dies. No flee behavior.
**Tactical identity:** The attrition faction. Reanimating enemies extend fights. Death curses punish killing elites without preparation. Necrotic immunity means necrotic spells are useless against them.

### Orcish (Floors 9-17)
**Roster:** Orc (standard/melee_guard), Orc Berserker (elite/brute), Orc Shaman (standard/caster), Orc Warlord (elite/leader)
**Signature mechanics:** Raw power + enrage on leader death + enrage_on_hit (Berserker)
**Squad behavior:** on_leader_death: enrage, flee_threshold: 0.25. Orcs are brave and get angrier when losing.
**Tactical identity:** The brute-force faction. Unlike Goblins where killing the leader helps, killing the Orc Warlord makes survivors enrage — creating a tactical dilemma. Berserker gets stronger as it takes hits.

### Demonic (Floors 13-20)
**Roster:** Imp (fodder), Hell Hound (standard), Shadow Fiend (elite)
**Signature mechanics:** Fire damage + burning on-hit + high individual power
**Squad behavior:** Imps have small groups. Shadow Fiends are often solo or paired.
**Tactical identity:** Each demon is a serious individual threat. Fire resistance becomes valuable. Less about squad tactics, more about handling powerful individuals.

### Giant (Floors 14-20)
**Roster:** Ogre (brute), Ogre Mage (caster), Troll (brute)
**Signature mechanics:** Massive HP + slow speed + heavy hits. Troll has high regen.
**Squad behavior:** Usually solo or pairs. No complex squad dynamics.
**Tactical identity:** HP walls. Kiting works because they're slow. Troll's regen creates urgency — must burst it down or it heals back. Ogre Mage adds spell threat to an otherwise straightforward faction.

### Dark (Floors 17-20)
**Roster:** Vampire (elite), Dark Knight (elite)
**Signature mechanics:** Life drain (Vampire) + high armor (Dark Knight) + death curse
**Squad behavior:** Usually solo. Both are individually dangerous.
**Tactical identity:** Endgame gatekeepers. Vampire's life drain makes sustained fights dangerous — burst damage preferred. Dark Knight's high armor (5) demands armor penetration or magic damage.

### Boss: The Veiled Tyrant (Floor 20)
**Stats:** Level 20, 200 base_hp (~440 final), 2d8+4 necrotic, 5 armor, regen 6, 7 spells
**Signature mechanics:** Escalating power (gains new abilities every ~1000 player turns), spell variety, necrotic immune
**Tactical identity:** The ultimate test. Escalation mechanic punishes slow play. Diverse spell loadout means no single counter-strategy works.
