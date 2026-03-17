//! Monster ability system — passive, on-hit, on-death, and aura abilities.
//!
//! Abilities are data-driven: the `MonsterAsset` declares which abilities a monster
//! has, the spawner attaches the corresponding ECS components, and handler systems
//! react to combat events (trigger messages) to apply the effects.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Name, Position};
use crate::game::combat::{
    ApplyDamageMessage, CombatDamageSet, DamageSource, DamageType, DeathEvent, GameRng,
    HealMessage,
};
use crate::game::magic::{Burning, Slowed, Stunned};
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
    pub final_damage: i32,
    pub source: DamageSource,
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

/// +50% damage multiplier. Inserted by Enrage trigger or War Cry.
#[derive(Component, Debug, Clone)]
pub struct Enraged {
    pub turns_remaining: u32,
}

/// On death: deal AoE damage in a radius.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct ExplodeOnDeath {
    pub damage: i32,
    pub radius: i32,
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
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &BurningStrike)>,
    defender_query: Query<&Name>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, burning_strike)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= burning_strike.chance as i32 {
            commands.entity(msg.defender).insert(Burning {
                damage_per_turn: burning_strike.damage_per_turn,
                turns_remaining: burning_strike.duration,
            });
            log_writer.write(GameLogMessage(format!(
                "{}'s attack sets {} ablaze!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Stunning Blow: on hit, chance to stun target.
pub fn handle_stunning_blow(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &StunningBlow)>,
    defender_query: Query<&Name>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, stun)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= stun.chance as i32 {
            commands.entity(msg.defender).insert(Stunned {
                turns_remaining: stun.duration,
            });
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
    mut heal_writer: MessageWriter<HealMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        let Ok((attacker_name, drain)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let heal_amount = (msg.final_damage * drain.percent / 100).max(1);
        heal_writer.write(HealMessage {
            entity: msg.attacker,
            amount: heal_amount,
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
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &SlowStrike)>,
    defender_query: Query<&Name>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, slow)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        let roll = game_rng.0.roll_dice(1, 100);
        if roll <= slow.chance as i32 {
            commands.entity(msg.defender).insert(Slowed {
                turns_remaining: slow.duration,
            });
            log_writer.write(GameLogMessage(format!(
                "{}'s attack slows {}!",
                attacker_name.0, defender_name.0
            )));
        }
    }
}

/// Rough Body: when hit by melee, reflect flat damage back.
pub fn handle_rough_body(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    defender_query: Query<(&Name, &RoughBody)>,
    attacker_query: Query<&Name>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((defender_name, rough)) = defender_query.get(msg.defender) else { continue; };
        let Ok(attacker_name) = attacker_query.get(msg.attacker) else { continue; };

        damage_writer.write(ApplyDamageMessage {
            attacker: msg.defender,
            target: msg.attacker,
            final_damage: rough.damage,
            damage_type: DamageType::Physical,
            source: DamageSource::Environment,
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
    mut commands: Commands,
    query: Query<(&Name, &crate::game::combat::Health, &Enrage, Has<Enraged>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        let Ok((name, health, enrage, already_enraged)) = query.get(msg.defender) else { continue; };
        if already_enraged { continue; }

        let threshold_hp = health.max * enrage.threshold_percent as i32 / 100;
        if health.current <= threshold_hp && health.current > 0 {
            commands.entity(msg.defender).insert(Enraged {
                turns_remaining: 99,
            });
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
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in death_events.read() {
        let Ok((pos, name, explode)) = query.get(event.target) else { continue; };

        log_writer.write(GameLogMessage(format!("{} explodes!", name.0)));

        for (target_entity, target_pos) in targets.iter() {
            if target_entity == event.target { continue; }
            let dist = (target_pos.x - pos.x).abs() + (target_pos.y - pos.y).abs();
            if dist <= explode.radius {
                damage_writer.write(ApplyDamageMessage {
                    attacker: event.target,
                    target: target_entity,
                    final_damage: explode.damage,
                    damage_type: DamageType::Fire,
                    source: DamageSource::Environment,
                });
            }
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
        let Ok((pos, name, summon)) = query.get(event.target) else { continue; };

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
            );
        }

        log_writer.write(GameLogMessage(format!(
            "As {} falls, {} {} emerge!",
            name.0, spawned_count, summon.monster_name
        )));
    }
}

/// War Cry: when a monster with WarCry first enters combat, buff nearby allies.
/// "Enters combat" = the monster's AI has a target (checked by AI bridge, not here).
/// For simplicity, we activate on first OnHitTrigger where the monster is the attacker.
pub fn handle_war_cry(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut commands: Commands,
    mut query: Query<(&Name, &Position, &mut WarCry, &crate::components::Faction)>,
    ally_query: Query<(Entity, &Position, &crate::components::Faction), With<crate::components::Monster>>,
    mut log_writer: MessageWriter<GameLogMessage>,
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
            if !faction.0.is_allied_to(&ally_faction.0) { continue; }
            let dist = (ally_pos.x - pos.x).abs() + (ally_pos.y - pos.y).abs();
            if dist <= war_cry.radius {
                commands.entity(ally_entity).insert(Enraged {
                    turns_remaining: war_cry.duration,
                });
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
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee { continue; }
        let Ok((attacker_name, attacker_faction)) = attacker_query.get(msg.attacker) else { continue; };
        let Ok((defender_name, defender_pos)) = defender_query.get(msg.defender) else { continue; };

        // Check if any allied monster (not self) is adjacent to the defender
        let has_flanking_ally = ally_query.iter().any(|(ally_pos, ally_faction)| {
            if !attacker_faction.0.is_allied_to(&ally_faction.0) { return false; }
            let dist = (ally_pos.x - defender_pos.x).abs() + (ally_pos.y - defender_pos.y).abs();
            dist == 1
        });

        if has_flanking_ally {
            // Deal 50% bonus damage
            let bonus = (msg.final_damage / 2).max(1);
            damage_writer.write(ApplyDamageMessage {
                attacker: msg.attacker,
                target: msg.defender,
                final_damage: bonus,
                damage_type: DamageType::Physical,
                source: DamageSource::Environment,
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
) {
    for _ in turn_end.read() {
        // Clear old rally buffs
        for (entity, ..) in allies.iter() {
            commands.entity(entity).remove::<RallyBuff>();
        }

        // Apply new rally buffs
        for (leader_pos, rally, leader_faction) in leaders.iter() {
            for (ally_entity, ally_pos, ally_faction) in allies.iter() {
                if !leader_faction.0.is_allied_to(&ally_faction.0) { continue; }
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
) {
    for _ in turn_end.read() {
        // Clear old terrify markers
        for (entity, ..) in targets.iter() {
            commands.entity(entity).remove::<Terrified>();
        }

        // Apply terrify to enemies in range
        for (source_pos, terrify, source_faction) in sources.iter() {
            for (target_entity, target_pos, target_faction) in targets.iter() {
                if source_faction.0.is_allied_to(&target_faction.0) { continue; }
                let dist = (target_pos.x - source_pos.x).abs() + (target_pos.y - source_pos.y).abs();
                if dist <= terrify.radius {
                    commands.entity(target_entity).insert(Terrified);
                }
            }
        }
    }
}

/// Tick Enraged duration: decrement, remove when expired.
pub fn tick_enraged_system(
    mut turn_end: MessageReader<crate::game::turns::TurnEndEvent>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Enraged)>,
) {
    for _ in turn_end.read() {
        for (entity, mut enraged) in query.iter_mut() {
            enraged.turns_remaining = enraged.turns_remaining.saturating_sub(1);
            if enraged.turns_remaining == 0 {
                commands.entity(entity).remove::<Enraged>();
            }
        }
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OnHitTriggerMessage>()
            .add_message::<OnBeingHitTriggerMessage>()
            .add_systems(
                Update,
                (
                    // On-hit handlers
                    handle_burning_strike.after(CombatDamageSet),
                    handle_stunning_blow.after(CombatDamageSet),
                    handle_life_drain.after(CombatDamageSet),
                    handle_knockback.after(CombatDamageSet),
                    handle_slow_strike.after(CombatDamageSet),
                    handle_pack_tactics.after(CombatDamageSet),
                    handle_war_cry.after(CombatDamageSet),
                    // On-being-hit handlers
                    handle_rough_body.after(CombatDamageSet),
                    handle_enrage.after(CombatDamageSet),
                    // On-death handlers
                    handle_explode_on_death.after(CombatDamageSet),
                    handle_summon_on_death.after(CombatDamageSet),
                    // Aura systems (run on turn end)
                    rally_aura_system,
                    terrify_aura_system,
                    tick_enraged_system,
                )
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}
