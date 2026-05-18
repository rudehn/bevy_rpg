# Entity Patterns

Archetype templates for monster and item design. Pick one as the
scaffold for a new entity; deviate deliberately.

## Monster archetypes

### Swarm
Weak alone, dangerous in packs. Teaches positioning.

- HP: low end of the band (floor-appropriate)
- Damage: low, often 1dN with no bonus
- Speed: standard or fast
- Group size: 2–4 minimum
- Behavior: aggressive, flees when alone and wounded (`flee_at_hp_percent: 0.5`)
- Faction: usually its own (Rats, insects)
- Reference: Sewer Rat, Giant Rat

### Glass cannon
High damage, low HP. Priority target.

- HP: ≤ swarm tier at its floor
- Damage: upper-medium
- Dodge bonus: optional (+1–2) to make them annoying
- Speed: fast
- Behavior: aggressive, no fleeing, or `flee_at_hp_percent: 0.3`
- Reference: Plague Rat (poison version of swarm/glass blend)

### Brute
Slow, tanky, hits hard. Teaches kiting.

- HP: top of band (brute column in balance-targets)
- Damage: heavy dice
- Armor: 1–2 higher than standard
- Speed: slow (`movement_delay: 1.1–1.3`)
- Behavior: aggressive, never flees
- Reference: Ogre, Troll, Cave Troll

### Caster
Cooldown abilities instead of melee DPS. Forces closing or interrupts.

- HP: below brute, above glass cannon
- Damage: low melee, primary threat is `monster_abilities` (Bolt, Heal, etc.)
- Speed: standard
- Behavior: standard tactic list + `flee_at_hp_percent: 0.4`; `UseAbility` fires when any cooldown is up
- Reference: Goblin Conjurer, Ogre Mage, Goblin Shaman

### Summoner
Cooldown produces minions. Priority target; summons overwhelm if ignored.

- HP: caster range
- Use `SummonCapped { weights: [...], max_summons: N }` to prevent infinite
  spawn chains
- Reference: Rat Broodmother, Goblin Conjurer

### Ambusher / Sleeper
Invisible or disguised until triggered. Punishes inattention.

- Starts asleep (`MonsterAIMode::Asleep` — wakes via `Awareness` transition)
- Low HP, high first-strike damage
- Reference: Mimic (disguised), Shade (stealth-flavored)

### Support / Rally
Buffs allies, doesn't fight alone. Makes trash dangerous.

- Low damage, low HP
- Abilities: `WarCry`, `Rally`, `SelfBuff`, party-wide `Heal`
- Reference: Mycoid Sovereign, Goblin Warchief (via Enrage/Rally)

### Kiter / Skirmisher
Ranged damage, maintains distance. Frustrating to pin down.

- Standard tactic list + `kites: true, kite_distance: 3, ranged_range: 6` knobs (`RangedAttack` fires before `KiteRetreat` so archers shoot-then-back-off)
- Low HP, moderate ranged damage
- Reference: Goblin Firebomber, Imp, Bone Archer

### Leader
Commands a squad. Killing scatters the rest.

- Spawn in group entries with `on_leader_death: "scatter"`
- Higher HP + armor than minions
- Optional: `SelfBuff(effect: Enraged)` ability
- Reference: Rat Broodmother, Goblin Warchief, Orc Warlord

### Environmental hazard
0 damage, dangerous via side effect. Teaches terrain awareness.

- Low HP, 0 direct damage
- Primary mechanic is an `ExplodeOnHit` / `GasOnDeath` / ability
- Default `idle_movement: PathToRandomTile` gives them a meandering approach
- Reference: Pit Bloat (chasms), Bloat (gas), Fungal Spore (poison AoE)

### Stationary threat
Stays put, shoots from range.

- `stationary: true`
- `ranged_range` set
- High perception, low vision angle matters less
- Reference: Arrow Turret, Goblin Totem

### Apex / mini-boss
Slow, powerful, rare. Solo encounter.

- HP: top of mini-boss column
- Multiple abilities (on-hit + cooldown + passive)
- Standard tactic list + `chase_leash: 12, base_morale: 0.8+` (bosses resist routing)
- Reference: Hill Giant, Elder Drake, Stone Sentinel, Amulet Guardian

## Item archetypes

### Weapon identity
The Sword is the **balance baseline** — no active ability, the reference
point every other weapon is tuned against. Every other weapon kind
trades base damage or speed for a unique active ability (see `weapon_ability`):
- Sword: **None** (1d6, baseline)
- Dagger: **Backstab** (1d4 fast, triple damage vs. unaware)
- Bow: ranged fire via F key (no ability string needed; 1d4 melee fallback)
- Axe: **Cleave** (1d4, slow; rolled damage also hits every monster in the 8 tiles around the attacker)
- Spear: **Lunge** (planned) — 2-tile reach + reposition
- Mace: **Stun** (planned) — on-hit stun chance

New weapons without a unique ability are fine only as *typed-damage variants*
(Flameblade = fire Sword) — not as pure stat upgrades.

### Ring identity
Rings grant one primary effect:
- Protection: +armor
- Might: +damage_bonus
- Precision: +hit_bonus
- Evasion: +dodge_bonus
- Regeneration: +regen
- Speed: -delay
- Vitality: +max_hp
- Perception: +hit + vision radius (hybrid)

New rings should pick a stat lane and own it. Hybrid rings (Perception)
are fine but need deliberate design intent.

### Amulet identity
Amulets are rarer than rings, with bigger single-axis effects:
- Life: +15 max HP (double a Vitality ring)
- Warding: +25% physical resistance + 1 armor
- Swiftness: -0.15 delay (beats Ring of Speed's -0.1)
- Inferno / Grounding / Antivenom: +50% resistance to one element

### Staff identity
See `balance-targets.md`. Each staff owns one effect type. New staff ideas
must not duplicate an existing effect's mechanic; re-tune existing if close.

### Consumable identity
Potions are small, powerful, finite. Effect should be useful but not
run-winning. Scroll of Enchanting is the strategic center — never add
another item that fully duplicates its choice-of-target mechanic.

## Ability design patterns

### On-hit proc
Trigger when this monster successfully damages the target.
- Low per-hit chance (usually 30–100% for bosses, 20–40% for trash)
- Status effect (Burning, Poisoned, Slowed) with short duration (3 turns)

### On-being-hit
Trigger when this monster takes damage.
- `RoughBody` (reflect) — passive, always fires
- `Enrage` — threshold-based one-shot buff
- Good for tanks that punish slow DPS

### On-death
Trigger at DeathEvent.
- `ExplodeOnDeath`, `GasOnDeath`, `SummonOnDeath`
- Always guards itself with a `once` semantic (wrapped by DeathEvent)
- Watch for chain reactions (two bloats adjacent → gas chain)

### Cooldown (monster_abilities)
Cast on a cooldown like the player's staves.
- Mirror a staff effect when possible (Bolt → staff Lightning parallel)
- Cooldown 3–8 turns is the sweet spot

### Passive aura
Radius buff/debuff. Activates at first sighting, stays on.
- `WarCry`, `Rally`, `Terrify`
- Radius 2–3 is manageable; 4+ shifts encounter feel significantly
