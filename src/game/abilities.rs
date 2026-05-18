//! Monster ability system — passive, on-hit, on-death, and aura abilities.
//!
//! Abilities are data-driven: the `MonsterAsset` declares which abilities a monster
//! has, the spawner attaches the corresponding ECS components, and handler systems
//! react to combat events (trigger messages) to apply the effects.

use bevy::prelude::*;
use bracket_lib::prelude::Algorithm2D;
use serde::{Deserialize, Serialize};

use crate::components::{Name, Position};
use crate::game::combat::{
    DamageEvent, DamageSource, DamageType, DeathEvent, GameRng,
    HealEvent,
};
use crate::game::factions::FactionMatrix;
use crate::game::magic::{GameStatusEffectsExt, StatusEffectKind, StatusEffects};
use crate::game::gas::{self, GasTiles, GasType};
use crate::map::map::Map;
use crate::map::tile::is_walkable;
use crate::ui::game_log::GameLogMessage;

// =====================================================================
// Trigger Messages — emitted by the combat pipeline, consumed by handlers
// =====================================================================

/// Fired after a melee/ranged attack deals damage. Lets on-hit abilities trigger.
#[derive(Message, Debug)]
pub struct OnHitTriggerMessage {
    pub attacker: Entity,
    pub defender: Entity,
    pub final_damage: i32,
    pub source: DamageSource,
}

/// Fired after an entity takes damage. Lets on-being-hit abilities trigger.
#[derive(Message, Debug)]
pub struct OnBeingHitTriggerMessage {
    pub attacker: Entity,
    pub defender: Entity,
    #[allow(dead_code)]
    pub final_damage: i32,
    pub source: DamageSource,
    pub damage_type: DamageType,
}

// =====================================================================
// Ability Components — attached by the spawner from MonsterAsset data
// =====================================================================

/// On-hit effect: apply burning (fire DoT) to the target.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct BurningStrike {
    pub damage_per_turn: i32,
    pub duration: u32,
    pub chance: u32,
}

/// On-hit effect: chance to apply Poisoned DoT to the defender.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct PoisonStrike {
    pub damage_per_turn: i32,
    pub duration: u32,
    pub chance: u32,
}

/// On-hit effect: stun the target for N turns.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct StunningBlow {
    pub duration: u32,
    pub chance: u32,
}

/// On-hit effect: heal attacker for a percentage of damage dealt.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct LifeDrain {
    pub percent: i32,
}

/// On-hit effect: push the target N tiles away.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Knockback {
    pub distance: i32,
    pub chance: u32,
}

/// On-hit effect: slow the target.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct SlowStrike {
    pub duration: u32,
    pub chance: u32,
}

/// On-being-hit: reflect flat damage back to melee attackers.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct RoughBody {
    pub damage: i32,
}

/// When HP drops below threshold%, gain +50% damage.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Enrage {
    pub threshold_percent: u32,
}

/// On death: deal AoE damage in a radius.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct ExplodeOnDeath {
    pub damage: i32,
    pub radius: i32,
    #[serde(default = "default_fire_damage_type")]
    pub damage_type: DamageType,
}

fn default_fire_damage_type() -> DamageType {
    DamageType::Fire
}

/// On death: spawn a poison gas cloud in a Manhattan radius.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct GasOnDeath {
    pub radius: i32,
    pub volume: u16,
}

/// What happens when an `ExplodeOnHit` monster detonates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExplodeEffect {
    /// Crack nearby floor tiles; they collapse into chasms after a few turns.
    CrackFloor,
    /// Spawn a poison gas cloud in the radius.
    GasCloud { volume: u16 },
}

impl Default for ExplodeEffect {
    fn default() -> Self { Self::CrackFloor }
}

/// On melee hit: trigger an area effect (chasms, gas, etc.) and kill self.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct ExplodeOnHit {
    pub radius: i32,
    #[serde(default)]
    pub effect: ExplodeEffect,
}

/// On death: spawn N monsters at adjacent tiles.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct SummonOnDeath {
    pub monster_name: String,
    pub count: u32,
}

/// Passive aura: allies within radius gain +50% damage for N turns.
/// Activated once at start of combat (when first seeing player).
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct WarCry {
    pub radius: i32,
    pub duration: u32,
    pub activated: bool,
}

/// On-hit (conditional): +50% damage when a faction ally is adjacent to the target.
#[derive(Component, Debug, Clone)]
pub struct PackTactics;

/// Passive aura: allies within radius gain +N armor.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Rally {
    pub radius: i32,
    pub armor_bonus: i32,
}

/// Marker for entities receiving a Rally armor buff. Cleared each turn.
#[derive(Component, Debug, Clone)]
pub struct RallyBuff {
    pub armor_bonus: i32,
}

/// On-being-hit: split into two when hit, if HP is above threshold.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct SplitOnHit {
    pub min_hp: i32,
}

/// Marker: entity disguised as a chest. Removed when player is adjacent.
#[derive(Component, Debug, Clone)]
pub struct MimicDisguise;

/// Passive aura: enemies within radius deal -25% damage.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Terrify {
    pub radius: i32,
}

/// Marker for entities affected by Terrify aura. Cleared each turn.
#[derive(Component, Debug, Clone)]
pub struct Terrified;

// =====================================================================
// Handler Systems
// =====================================================================

/// Burning Strike: on hit, chance to apply Burning DoT.
pub fn handle_burning_strike(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &BurningStrike)>,
    defender_query: Query<&Name>,
    mut status_query: Query<&mut StatusEffects>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, burning_strike)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= burning_strike.chance as i32 {
            if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                effects.add_effect_with_magnitude(StatusEffectKind::Burning, burning_strike.duration, burning_strike.damage_per_turn, None);
            }
            log_writer.write(GameLogMessage(format!(
                "{}'s attack sets {} ablaze!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Poison Strike: on hit, chance to apply Poisoned DoT.
pub fn handle_poison_strike(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &PoisonStrike)>,
    defender_query: Query<&Name>,
    mut status_query: Query<&mut StatusEffects>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, poison_strike)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= poison_strike.chance as i32 {
            if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                effects.add_effect_with_magnitude(StatusEffectKind::Poisoned, poison_strike.duration, poison_strike.damage_per_turn, None);
            }
            log_writer.write(GameLogMessage(format!(
                "{}'s attack poisons {}!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Stunning Blow: on hit, chance to stun target.
pub fn handle_stunning_blow(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &StunningBlow)>,
    defender_query: Query<&Name>,
    mut status_query: Query<&mut StatusEffects>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, stun)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= stun.chance as i32 {
            if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                effects.add_effect(StatusEffectKind::Stunned, stun.duration);
            }
            log_writer.write(GameLogMessage(format!(
                "{}'s blow stuns {}!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Life Drain: on hit, heal attacker for % of damage dealt.
pub fn handle_life_drain(
    mut messages: MessageReader<OnHitTriggerMessage>,
    attacker_query: Query<(&Name, &LifeDrain)>,
    defender_query: Query<&Name>,
    mut heal_writer: MessageWriter<HealEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        let Ok((attacker_name, drain)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let heal_amount = (msg.final_damage * drain.percent / 100).max(1);
        heal_writer.write(HealEvent {
            target: msg.attacker,
            amount: heal_amount,
            source: None,
        });
        log_writer.write(GameLogMessage(format!(
            "{} drains life from {}! (+{} HP)",
            attacker_name.0, defender_name.0, heal_amount
        )));
    }
}

/// Knockback: on hit, push target away.
pub fn handle_knockback(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &Knockback, &Position)>,
    defender_query: Query<(&Name, &Position)>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
    map: Res<Map>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, kb, attacker_pos)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok((defender_name, defender_pos)) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll > kb.chance as i32 { continue; }

        let dx = (defender_pos.x - attacker_pos.x).signum();
        let dy = (defender_pos.y - attacker_pos.y).signum();
        if dx == 0 && dy == 0 { continue; }

        let occupied: std::collections::HashSet<(i32, i32)> = collider_query
            .iter()
            .map(|p| (p.x, p.y))
            .collect();

        let mut final_x = defender_pos.x;
        let mut final_y = defender_pos.y;
        for _ in 0..kb.distance {
            let nx = final_x + dx;
            let ny = final_y + dy;
            let idx = map.xy_idx(nx, ny);
            if !is_walkable(map.tiles[idx]) || occupied.contains(&(nx, ny)) {
                break;
            }
            final_x = nx;
            final_y = ny;
        }

        if final_x != defender_pos.x || final_y != defender_pos.y {
            commands.entity(msg.defender).insert(Position { x: final_x, y: final_y });
            log_writer.write(GameLogMessage(format!(
                "{} knocks {} back!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Slow Strike: on hit, chance to slow target.
pub fn handle_slow_strike(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &SlowStrike)>,
    defender_query: Query<&Name>,
    mut status_query: Query<&mut StatusEffects>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, slow)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= slow.chance as i32 {
            if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                effects.add_effect(StatusEffectKind::Slowed, slow.duration);
            }
            log_writer.write(GameLogMessage(format!(
                "{}'s attack slows {}!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Explode on Hit: when this monster lands a melee hit, detonate its configured
/// effect (floor-cracking, poison gas, etc.) and kill self.
pub fn handle_explode_on_hit(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut commands: Commands,
    query: Query<(&Position, &Name, &ExplodeOnHit)>,
    mut decoration_writer: MessageWriter<crate::map::tile::DecorationMutationMessage>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut gas_tiles: ResMut<GasTiles>,
    map: Res<Map>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((pos, name, explode)) = query.get(msg.attacker) else { continue; };

        match &explode.effect {
            ExplodeEffect::CrackFloor => {
                log_writer.write(GameLogMessage(format!(
                    "{} explodes! The floor begins to crack!",
                    name.0
                )));
                let r = explode.radius;
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() + dy.abs() > r { continue; }
                        let pt = bracket_lib::prelude::Point::new(pos.x + dx, pos.y + dy);
                        if !map.in_bounds(pt) { continue; }
                        decoration_writer.write(crate::map::tile::DecorationMutationMessage {
                            position: pt,
                            new_decoration: crate::map::tile::Decoration::CrackedFloor,
                        });
                    }
                }
            }
            ExplodeEffect::GasCloud { volume } => {
                log_writer.write(GameLogMessage(format!(
                    "{} bursts, releasing a cloud of poisonous gas!",
                    name.0
                )));
                for (x, y) in gas_positions_in_radius(pos.x, pos.y, explode.radius, &map) {
                    gas::spawn_gas(&mut commands, x, y, GasType::Poison, *volume, &mut gas_tiles);
                }
                // Prevent double-fire: if this entity also has GasOnDeath, strip it
                // so the imminent self-death doesn't spawn gas a second time.
                commands.entity(msg.attacker).remove::<GasOnDeath>();
            }
        }

        damage_writer.write(DamageEvent {
            attacker: None,
            target: msg.attacker,
            amount: 9999,
            damage_type: DamageType::Physical,
            source: DamageSource::Environment,
            armor: 0,
        });
    }
}

/// Rough Body: when hit by melee, reflect flat damage back.
pub fn handle_rough_body(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    defender_query: Query<(&Name, &RoughBody)>,
    attacker_query: Query<&Name>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((defender_name, rough)) = defender_query.get(msg.defender) else { continue; };
        let Ok(attacker_name) = attacker_query.get(msg.attacker) else { continue; };

        damage_writer.write(DamageEvent {
            attacker: Some(msg.defender),
            target: msg.attacker,
            amount: rough.damage,
            damage_type: DamageType::Physical,
            source: DamageSource::Environment,
            armor: 0,
        });
        log_writer.write(GameLogMessage(format!(
            "{}'s rough body deals {} damage to {}!",
            defender_name.0, rough.damage, attacker_name.0
        )));
    }
}

/// Enrage: when HP drops below threshold, gain Enraged (+50% damage).
pub fn handle_enrage(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    query: Query<(&Name, &crate::game::combat::Health, &Enrage)>,
    mut status_query: Query<&mut StatusEffects>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        let Ok((name, health, enrage)) = query.get(msg.defender) else { continue; };

        // Check if already enraged via StatusEffects
        if let Ok(effects) = status_query.get(msg.defender)
            && effects.is_enraged() { continue; }

        let threshold_hp = health.max * enrage.threshold_percent as i32 / 100;
        if health.current <= threshold_hp && health.current > 0 {
            if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                effects.add_effect(StatusEffectKind::Enraged, 99);
            }
            log_writer.write(GameLogMessage(format!(
                "{} flies into a rage!",
                name.0
            )));
        }
    }
}

/// Explode on Death: AoE damage when entity dies.
pub fn handle_explode_on_death(
    mut death_events: MessageReader<DeathEvent>,
    query: Query<(&Position, &Name, &ExplodeOnDeath)>,
    targets: Query<(Entity, &Position), With<crate::game::combat::Health>>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in death_events.read() {
        let Ok((pos, name, explode)) = query.get(event.entity) else { continue; };

        log_writer.write(GameLogMessage(format!("{} explodes!", name.0)));

        for (target_entity, target_pos) in targets.iter() {
            if target_entity == event.entity { continue; }
            let dist = (target_pos.x - pos.x).abs() + (target_pos.y - pos.y).abs();
            if dist <= explode.radius {
                damage_writer.write(DamageEvent {
                    attacker: Some(event.entity),
                    target: target_entity,
                    amount: explode.damage,
                    damage_type: explode.damage_type,
                    source: DamageSource::Environment,
                    armor: 0,
                });
            }
        }
    }
}

/// Returns all tile positions within Manhattan `radius` of `(cx, cy)` where gas can exist.
pub fn gas_positions_in_radius(cx: i32, cy: i32, radius: i32, map: &Map) -> Vec<(i32, i32)> {
    let mut positions = Vec::new();
    for dx in -radius..=radius {
        let remaining = radius - dx.abs();
        for dy in -remaining..=remaining {
            let (nx, ny) = (cx + dx, cy + dy);
            if !map.in_bounds(bracket_lib::prelude::Point::new(nx, ny)) {
                continue;
            }
            let idx = map.xy_idx(nx, ny);
            if gas::can_gas_occupy(map.tiles[idx]) {
                positions.push((nx, ny));
            }
        }
    }
    positions
}

/// Gas on Death: spawn poison gas cloud when entity dies.
pub fn handle_gas_on_death(
    mut death_events: MessageReader<DeathEvent>,
    mut commands: Commands,
    query: Query<(&Position, &Name, &GasOnDeath)>,
    map: Res<Map>,
    mut gas_tiles: ResMut<GasTiles>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in death_events.read() {
        let Ok((pos, name, gas_death)) = query.get(event.entity) else { continue; };

        log_writer.write(GameLogMessage(format!(
            "{} bursts, releasing a cloud of poisonous gas!",
            name.0
        )));

        for (x, y) in gas_positions_in_radius(pos.x, pos.y, gas_death.radius, &map) {
            gas::spawn_gas(
                &mut commands,
                x,
                y,
                GasType::Poison,
                gas_death.volume,
                &mut gas_tiles,
            );
        }
    }
}

/// Summon on Death: spawn monsters when entity dies.
pub fn handle_summon_on_death(
    mut death_events: MessageReader<DeathEvent>,
    mut commands: Commands,
    query: Query<(&Position, &Name, &SummonOnDeath)>,
    mut turn_manager: ResMut<crate::game::TurnManager>,
    monster_manifests: Res<Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    map: Res<Map>,
    mut log_writer: MessageWriter<GameLogMessage>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
) {
    for event in death_events.read() {
        let Ok((pos, name, summon)) = query.get(event.entity) else { continue; };

        let occupied: std::collections::HashSet<(i32, i32)> = collider_query
            .iter()
            .map(|p| (p.x, p.y))
            .collect();

        let directions = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
        let mut spawn_points = Vec::new();
        for (dx, dy) in &directions {
            let nx = pos.x + dx;
            let ny = pos.y + dy;
            let idx = map.xy_idx(nx, ny);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) && !occupied.contains(&(nx, ny)) {
                spawn_points.push(bracket_lib::prelude::Point::new(nx, ny));
                if spawn_points.len() >= summon.count as usize {
                    break;
                }
            }
        }

        if spawn_points.is_empty() { continue; }

        let spawned_count = spawn_points.len();
        for point in spawn_points {
            crate::game::spawner::spawn_monster_by_name(
                &mut commands,
                &summon.monster_name,
                &point,
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
                None,
            );
        }

        log_writer.write(GameLogMessage(format!(
            "As {} falls, {} {} emerge!",
            name.0, spawned_count, summon.monster_name
        )));
    }
}

/// Kill all summons when their summoner dies.
/// When a summoner (e.g. Goblin Conjurer) is killed, all entities with
/// `SummonedBy` pointing to it are despawned. Generic — works for any summoner.
pub fn handle_summoner_death(
    mut death_events: MessageReader<DeathEvent>,
    summons: Query<(Entity, &crate::components::SummonedBy, &Name)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut commands: Commands,
    mut turn_manager: ResMut<crate::game::TurnManager>,
) {
    for event in death_events.read() {
        let to_kill = collect_summons_to_kill(event.entity, &summons);
        for (summon_entity, name) in to_kill {
            log_writer.write(GameLogMessage(format!("{} dissipates!", name)));
            commands.entity(summon_entity).despawn();
            turn_manager.remove_entity(summon_entity);
        }
    }
}

/// Pure helper: collect summon entities that should die when `dead_summoner` dies.
fn collect_summons_to_kill(
    dead_summoner: Entity,
    summons: &Query<(Entity, &crate::components::SummonedBy, &Name)>,
) -> Vec<(Entity, String)> {
    summons
        .iter()
        .filter(|(_, sb, _)| sb.summoner == dead_summoner)
        .map(|(e, _, name)| (e, name.0.clone()))
        .collect()
}

/// War Cry: when a monster with WarCry first enters combat, buff nearby allies.
/// "Enters combat" = the monster's AI has a target (checked by AI bridge, not here).
/// For simplicity, we activate on first OnHitTrigger where the monster is the attacker.
pub fn handle_war_cry(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut query: Query<(&Name, &Position, &mut WarCry, &crate::components::Faction)>,
    ally_query: Query<(Entity, &Position, &crate::components::Faction), With<crate::components::Monster>>,
    mut status_query: Query<&mut StatusEffects>,
    mut log_writer: MessageWriter<GameLogMessage>,
    faction_matrix: Res<FactionMatrix>,
) {
    for msg in messages.read() {
        let Ok((name, pos, mut war_cry, faction)) = query.get_mut(msg.attacker) else { continue; };
        if war_cry.activated { continue; }
        war_cry.activated = true;

        log_writer.write(GameLogMessage(format!(
            "{} lets out a war cry!",
            name.0
        )));

        for (ally_entity, ally_pos, ally_faction) in ally_query.iter() {
            if ally_entity == msg.attacker { continue; }
            if !faction_matrix.is_allied_to(&faction.0.0, &ally_faction.0.0) { continue; }
            let dist = (ally_pos.x - pos.x).abs() + (ally_pos.y - pos.y).abs();
            if dist <= war_cry.radius
                && let Ok(mut effects) = status_query.get_mut(ally_entity) {
                    effects.add_effect(StatusEffectKind::Enraged, war_cry.duration);
                }
        }
    }
}

/// Pack Tactics: +50% damage when a faction ally is adjacent to the target.
/// This modifies the OnHitTrigger by issuing bonus damage.
pub fn handle_pack_tactics(
    mut messages: MessageReader<OnHitTriggerMessage>,
    attacker_query: Query<(&Name, &crate::components::Faction), With<PackTactics>>,
    defender_query: Query<(&Name, &Position)>,
    ally_query: Query<(&Position, &crate::components::Faction), With<crate::components::Monster>>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    faction_matrix: Res<FactionMatrix>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, attacker_faction)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok((defender_name, defender_pos)) = defender_query.get(msg.defender) else { continue; };

        // Check if any allied monster (not self) is adjacent to the defender
        let has_flanking_ally = ally_query.iter().any(|(ally_pos, ally_faction)| {
            if !faction_matrix.is_allied_to(&attacker_faction.0.0, &ally_faction.0.0) { return false; }
            let dist = (ally_pos.x - defender_pos.x).abs() + (ally_pos.y - defender_pos.y).abs();
            dist == 1
        });

        if has_flanking_ally {
            // Deal 50% bonus damage
            let bonus = (msg.final_damage / 2).max(1);
            damage_writer.write(DamageEvent {
                attacker: Some(msg.attacker),
                target: msg.defender,
                amount: bonus,
                damage_type: DamageType::Physical,
                source: DamageSource::Environment,
                armor: 0,
            });
            log_writer.write(GameLogMessage(format!(
                "{} exploits pack tactics against {}! (+{} damage)",
                attacker_name.0, defender_name.0, bonus
            )));
        }
    }
}

/// Rally aura: each turn, apply RallyBuff to allies within radius.
pub fn rally_aura_system(
    mut turn_end: MessageReader<crate::game::turns::TurnEndEvent>,
    mut commands: Commands,
    leaders: Query<(&Position, &Rally, &crate::components::Faction)>,
    allies: Query<(Entity, &Position, &crate::components::Faction), With<crate::components::Monster>>,
    faction_matrix: Res<FactionMatrix>,
) {
    for _ in turn_end.read() {
        // Clear old rally buffs
        for (entity, ..) in allies.iter() {
            commands.entity(entity).remove::<RallyBuff>();
        }

        // Apply new rally buffs
        for (leader_pos, rally, leader_faction) in leaders.iter() {
            for (ally_entity, ally_pos, ally_faction) in allies.iter() {
                if !faction_matrix.is_allied_to(&leader_faction.0.0, &ally_faction.0.0) { continue; }
                let dist = (ally_pos.x - leader_pos.x).abs() + (ally_pos.y - leader_pos.y).abs();
                if dist <= rally.radius {
                    commands.entity(ally_entity).insert(RallyBuff {
                        armor_bonus: rally.armor_bonus,
                    });
                }
            }
        }
    }
}

/// Terrify aura: each turn, apply Terrified marker to enemies within radius.
pub fn terrify_aura_system(
    mut turn_end: MessageReader<crate::game::turns::TurnEndEvent>,
    mut commands: Commands,
    sources: Query<(&Position, &Terrify, &crate::components::Faction)>,
    targets: Query<(Entity, &Position, &crate::components::Faction), With<crate::game::combat::Health>>,
    faction_matrix: Res<FactionMatrix>,
) {
    for _ in turn_end.read() {
        // Clear old terrify markers
        for (entity, ..) in targets.iter() {
            commands.entity(entity).remove::<Terrified>();
        }

        // Apply terrify to enemies in range
        for (source_pos, terrify, source_faction) in sources.iter() {
            for (target_entity, target_pos, target_faction) in targets.iter() {
                if faction_matrix.is_allied_to(&source_faction.0.0, &target_faction.0.0) { continue; }
                let dist = (target_pos.x - source_pos.x).abs() + (target_pos.y - source_pos.y).abs();
                if dist <= terrify.radius {
                    commands.entity(target_entity).insert(Terrified);
                }
            }
        }
    }
}

/// Split on Hit: when hit (non-fire), spawn a clone adjacent with half HP.
pub fn handle_split_on_hit(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    mut commands: Commands,
    query: Query<(&Name, &Position, &SplitOnHit, &crate::game::combat::Health, &StatusEffects)>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
    map: Res<Map>,
    mut turn_manager: ResMut<crate::game::TurnManager>,
    monster_manifests: Res<Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        let Ok((name, pos, split, health, status_effects)) = query.get(msg.defender) else { continue; };

        // Don't split from fire damage
        if msg.damage_type == DamageType::Fire { continue; }

        // Don't split if HP is too low
        if health.current < split.min_hp { continue; }

        // Find an adjacent walkable, unoccupied tile
        let occupied: std::collections::HashSet<(i32, i32)> = collider_query
            .iter()
            .map(|p| (p.x, p.y))
            .collect();

        let directions = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
        let mut spawn_point = None;
        for (dx, dy) in &directions {
            let nx = pos.x + dx;
            let ny = pos.y + dy;
            let idx = map.xy_idx(nx, ny);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) && !occupied.contains(&(nx, ny)) {
                spawn_point = Some(bracket_lib::prelude::Point::new(nx, ny));
                break;
            }
        }

        let Some(point) = spawn_point else { continue; };

        // Clone HP is floor(current / 2)
        let clone_hp = health.current / 2;

        // Spawn a clone using spawn_monster_by_name
        if let Some(clone_entity) = crate::game::spawner::spawn_monster_by_name(
            &mut commands,
            &name.0,
            &point,
            &mut turn_manager,
            &monster_manifests,
            &monster_manifest_handle,
            &monster_sprite_assets,
            None,
        ) {
            // Override clone's HP
            commands.entity(clone_entity).insert(crate::game::combat::Health {
                current: clone_hp,
                max: clone_hp,
            });
            // Inherit status effects from the original
            commands.entity(clone_entity).insert(status_effects.clone());
        }

        log_writer.write(GameLogMessage(format!("{} splits into two!", name.0)));
    }
}

/// Mimic reveal: if a MimicDisguise entity is adjacent to the player, wake it and remove disguise.
pub fn mimic_reveal_system(
    mut commands: Commands,
    mut turn_end: MessageReader<crate::game::turns::TurnEndEvent>,
    mimic_query: Query<(Entity, &Position, &Name), With<MimicDisguise>>,
    player_query: Query<&Position, With<crate::player::Player>>,
    mut ai_query: Query<&mut crate::game::MonsterAI>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in turn_end.read() {
        let Ok(player_pos) = player_query.single() else { continue; };

        for (entity, mimic_pos, _name) in mimic_query.iter() {
            let dist = (mimic_pos.x - player_pos.x).abs() + (mimic_pos.y - player_pos.y).abs();
            if dist <= 1 {
                commands.entity(entity).remove::<MimicDisguise>();
                if let Ok(mut ai) = ai_query.get_mut(entity) {
                    ai.alert_to_position(bracket_lib::prelude::Point::new(player_pos.x, player_pos.y));
                }
                log_writer.write(GameLogMessage("The chest was a mimic!".to_string()));
            }
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poison_strike_component_stores_fields() {
        let ps = PoisonStrike { damage_per_turn: 2, duration: 5, chance: 75 };
        assert_eq!(ps.damage_per_turn, 2);
        assert_eq!(ps.duration, 5);
        assert_eq!(ps.chance, 75);
    }

    #[test]
    fn collect_summons_finds_matching_summoner() {
        let mut world = World::new();
        let summoner = world.spawn_empty().id();
        let blade1 = world
            .spawn((
                crate::components::SummonedBy { summoner },
                Name("Spectral Blade".to_string()),
            ))
            .id();
        let blade2 = world
            .spawn((
                crate::components::SummonedBy { summoner },
                Name("Spectral Blade".to_string()),
            ))
            .id();

        let mut query_state =
            world.query::<(Entity, &crate::components::SummonedBy, &Name)>();
        let results: Vec<(Entity, String)> = query_state
            .iter(&world)
            .filter(|(_, sb, _)| sb.summoner == summoner)
            .map(|(e, _, name)| (e, name.0.clone()))
            .collect();

        assert_eq!(results.len(), 2);
        let entities: Vec<Entity> = results.iter().map(|(e, _)| *e).collect();
        assert!(entities.contains(&blade1));
        assert!(entities.contains(&blade2));
    }

    #[test]
    fn collect_summons_ignores_other_summoner() {
        let mut world = World::new();
        let summoner_a = world.spawn_empty().id();
        let summoner_b = world.spawn_empty().id();
        let _blade_a = world
            .spawn((
                crate::components::SummonedBy { summoner: summoner_a },
                Name("Blade A".to_string()),
            ))
            .id();
        let blade_b = world
            .spawn((
                crate::components::SummonedBy { summoner: summoner_b },
                Name("Blade B".to_string()),
            ))
            .id();

        // Kill summoner A — only blade A should be collected
        let mut query_state =
            world.query::<(Entity, &crate::components::SummonedBy, &Name)>();
        let results: Vec<(Entity, String)> = query_state
            .iter(&world)
            .filter(|(_, sb, _)| sb.summoner == summoner_a)
            .map(|(e, _, name)| (e, name.0.clone()))
            .collect();

        assert_eq!(results.len(), 1);
        assert_ne!(results[0].0, blade_b);
    }

    #[test]
    fn collect_summons_empty_when_no_summons() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        let mut query_state =
            world.query::<(Entity, &crate::components::SummonedBy, &Name)>();
        let results: Vec<(Entity, String)> = query_state
            .iter(&world)
            .filter(|(_, sb, _)| sb.summoner == entity)
            .map(|(e, _, name)| (e, name.0.clone()))
            .collect();

        assert!(results.is_empty());
    }

    // --- gas_positions_in_radius ---

    fn make_open_map(width: i32, height: i32) -> crate::map::map::Map {
        use crate::map::tile::{Tile, TerrainType, LiquidType, Decoration};
        let mut map = crate::map::map::Map::new(1, width, height, "test");
        // Set all tiles to Floor so gas can occupy them
        for tile in map.tiles.iter_mut() {
            *tile = Tile { terrain: TerrainType::Floor, liquid: LiquidType::None, decoration: Decoration::None };
        }
        map
    }

    #[test]
    fn gas_positions_radius_0_returns_center_only() {
        let map = make_open_map(10, 10);
        let positions = gas_positions_in_radius(5, 5, 0, &map);
        assert_eq!(positions.len(), 1);
        assert!(positions.contains(&(5, 5)));
    }

    #[test]
    fn gas_positions_radius_1_returns_5_tiles() {
        let map = make_open_map(10, 10);
        let positions = gas_positions_in_radius(5, 5, 1, &map);
        assert_eq!(positions.len(), 5);
        assert!(positions.contains(&(5, 5)));
        assert!(positions.contains(&(4, 5)));
        assert!(positions.contains(&(6, 5)));
        assert!(positions.contains(&(5, 4)));
        assert!(positions.contains(&(5, 6)));
    }

    #[test]
    fn gas_positions_radius_2_returns_13_tiles() {
        let map = make_open_map(10, 10);
        let positions = gas_positions_in_radius(5, 5, 2, &map);
        assert_eq!(positions.len(), 13);
    }

    #[test]
    fn gas_positions_skips_walls() {
        use crate::map::tile::{Tile, TerrainType, LiquidType, Decoration};
        let mut map = make_open_map(10, 10);
        // Place a wall at (6, 5)
        let idx = map.xy_idx(6, 5);
        map.tiles[idx] = Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None };
        let positions = gas_positions_in_radius(5, 5, 1, &map);
        assert_eq!(positions.len(), 4);
        assert!(!positions.contains(&(6, 5)));
    }

    #[test]
    fn gas_positions_clamps_to_map_bounds() {
        let map = make_open_map(10, 10);
        // Place at corner (0, 0) with radius 1 — only 3 valid positions
        let positions = gas_positions_in_radius(0, 0, 1, &map);
        assert_eq!(positions.len(), 3);
        assert!(positions.contains(&(0, 0)));
        assert!(positions.contains(&(1, 0)));
        assert!(positions.contains(&(0, 1)));
    }

    // --- ExplodeOnHit ---

    #[test]
    fn explode_effect_defaults_to_crack_floor() {
        // Backward-compat: RON entries that specify ExplodeOnHit without an
        // effect field (pre-refactor Pit Bloat) deserialize as CrackFloor.
        assert!(matches!(ExplodeEffect::default(), ExplodeEffect::CrackFloor));
    }

    #[test]
    fn explode_on_hit_holds_gas_cloud_effect() {
        let comp = ExplodeOnHit {
            radius: 2,
            effect: ExplodeEffect::GasCloud { volume: 500 },
        };
        match comp.effect {
            ExplodeEffect::GasCloud { volume } => assert_eq!(volume, 500),
            _ => panic!("expected GasCloud effect"),
        }
        assert_eq!(comp.radius, 2);
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        use crate::game::turns::CombatReactionSet;
        app.add_message::<OnHitTriggerMessage>()
            .add_message::<OnBeingHitTriggerMessage>()
            // On-hit / on-being-hit / on-death triggers + aura systems.
            // Run in CombatReactionSet → `.after(CombatDamageSet)` in-game only.
            .add_systems(
                Update,
                (
                    (
                        // On-hit ability triggers
                        handle_burning_strike,
                        handle_poison_strike,
                        handle_stunning_blow,
                        handle_life_drain,
                        handle_knockback,
                        handle_slow_strike,
                        handle_pack_tactics,
                        handle_war_cry,
                        handle_explode_on_hit,
                    ),
                    (
                        // On-being-hit ability triggers
                        handle_rough_body,
                        handle_enrage,
                        handle_split_on_hit,
                        // On-death triggers
                        handle_explode_on_death,
                        handle_summon_on_death,
                        handle_gas_on_death,
                        handle_summoner_death,
                        // Aura systems (run on turn end)
                        rally_aura_system,
                        terrify_aura_system,
                        // Mimic reveal (run on turn end)
                        mimic_reveal_system,
                    ),
                )
                    .in_set(CombatReactionSet),
            );
    }
}
