use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// --- Faction ---

/// Determines how this entity relates to others for AI targeting and spell scoring.
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub struct Faction(pub FactionKind);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FactionKind {
    Player,
    Monster,
}

impl FactionKind {
    /// Returns true if `other` is a valid hostile target for `self`.
    pub fn is_hostile_to(&self, other: &FactionKind) -> bool {
        self != other
    }

    /// Returns true if `other` is on the same side as `self`.
    pub fn is_allied_to(&self, other: &FactionKind) -> bool {
        self == other
    }
}

// --- Monster Ability Components ---

/// Base armor value from monster definition (flat damage reduction).
#[derive(Component, Debug, Clone)]
pub struct BaseArmor(pub i32);

/// Monster flees when below 50% HP.
#[derive(Component, Debug, Clone)]
pub struct Cowardly;

// --- On-Hit Effects ---

/// A single on-hit effect that can trigger when a monster lands a melee attack.
/// Chance is a flat percentage (1–100): roll 1–100, if ≤ chance, effect triggers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OnHitEffect {
    ApplyPoison {
        damage_per_turn: i32,
        duration: u32,
        chance: u32,
    },
    ApplySlow {
        duration: u32,
        chance: u32,
    },
    ApplyStun {
        duration: u32,
        chance: u32,
    },
    AttributeDrain {
        attribute: String,
        amount: i32,
        duration: u32,
        chance: u32,
    },
    LifeDrain {
        amount: i32,
        chance: u32,
    },
    Knockback {
        distance: i32,
        chance: u32,
    },
    ApplyBurning {
        damage_per_turn: i32,
        duration: u32,
        chance: u32,
    },
    Disarm {
        duration: u32,
        chance: u32,
    },
}

/// Collection of on-hit effects on an entity.
#[derive(Component, Clone, Debug)]
pub struct OnHitEffects(pub Vec<OnHitEffect>);

// --- Passive Ability Components (Stubs) ---
// Each passive ability is its own component. Monsters that have it just carry the component;
// dedicated systems react to game events and trigger the effect automatically.
// Abilities have no mana cost, no cooldown, and are never "cast."

/// Inflicts poison stacks on any entity that physically attacks this one.
#[derive(Component, Debug, Clone)]
pub struct PoisonBody {
    /// How many stacks of poison the attacker receives per hit.
    pub stacks: i32,
}

/// Deals area damage to nearby entities when this entity dies.
#[derive(Component, Debug, Clone)]
pub struct ExplodeOnDeath {
    /// Tile radius of the explosion.
    pub radius: i32,
    /// Flat damage dealt to each entity in range.
    pub damage: i32,
}

/// Can revive itself once after reaching 0 HP.
#[derive(Component, Debug, Clone)]
pub struct Reanimate {
    /// HP the entity revives with.
    pub revive_hp: i32,
}

// --- On-Hit Handler System ---

// --- New Passive Ability Components ---

/// Reflects flat damage back to melee attackers.
#[derive(Component, Debug, Clone)]
pub struct ThornAura {
    pub damage: i32,
}

/// When HP drops below threshold%, gain Enraged (+50% damage). Triggers once.
#[derive(Component, Debug, Clone)]
pub struct EnrageOnHit {
    pub threshold_percent: u32,
}

/// On death, applies a debuff to the killer.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct DeathCurse {
    pub effect: DeathCurseEffect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DeathCurseEffect {
    Slow { duration: u32 },
    Poison { damage_per_turn: i32, duration: u32 },
    WeakenStr { amount: i32, duration: u32 },
}

/// On death, spawns monsters at adjacent walkable tiles.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct SummonOnDeath {
    pub monster_name: String,
    pub count: u32,
}

// --- Imports ---

use crate::components::{Name, Position};
use crate::game::combat::{
    ApplyDamageMessage, DamageSource, DamageType, DeathEvent, GameRng, HealMessage,
    OnBeingHitTriggerMessage, OnHitTriggerMessage, ResistanceLevel, Resistances,
};
use crate::game::magic::{
    Burning, Disarmed, Enraged, Poisoned, Slowed, Stunned, TimedModifierEntry, TimedModifiers,
};
use crate::game::particles::grid_to_world;
use crate::game::stats::AttributeModifiers;
use crate::map::map::Map;
use crate::map::tile::is_walkable;
use crate::ui::game_log::GameLogMessage;

/// Processes on-hit effects after a melee attack deals damage.
/// Each effect has a flat chance (1–100). Rolls 1–100; if ≤ chance, triggers.
pub fn handle_on_hit_effects(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, Option<&OnHitEffects>, &Position)>,
    defender_query: Query<(&Name, Option<&Resistances>, &Position)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    map: Res<Map>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
) {
    for msg in messages.read() {
        let Ok((attacker_name, on_hit, attacker_pos)) = attacker_query.get(msg.attacker) else {
            continue;
        };
        let Some(on_hit) = on_hit else { continue };

        let Ok((defender_name, defender_resistances, defender_pos)) = defender_query.get(msg.defender) else {
            continue;
        };

        for effect in &on_hit.0 {
            match effect {
                OnHitEffect::ApplyPoison {
                    damage_per_turn,
                    duration,
                    chance,
                } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        // Check poison immunity via Resistances
                        let poison_resistance = defender_resistances
                            .map(|r| r.get(&DamageType::Poison))
                            .unwrap_or(ResistanceLevel::Normal);
                        if poison_resistance == ResistanceLevel::Immune {
                            log_writer.write(GameLogMessage(format!(
                                "{} resists the venom! (poison immune)",
                                defender_name.0
                            )));
                        } else {
                            commands.entity(msg.defender).insert(Poisoned {
                                damage_per_turn: *damage_per_turn,
                                turns_remaining: *duration,
                            });
                            log_writer.write(GameLogMessage(format!(
                                "{}'s attack poisons {}! ({} dmg/turn for {} turns)",
                                attacker_name.0, defender_name.0, damage_per_turn, duration
                            )));
                        }
                    }
                }
                OnHitEffect::ApplySlow { duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        commands.entity(msg.defender).insert(Slowed {
                            turns_remaining: *duration,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack slows {}!",
                            attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::ApplyStun { duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        commands.entity(msg.defender).insert(Stunned {
                            turns_remaining: *duration,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack stuns {}!",
                            attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::AttributeDrain {
                    attribute,
                    amount,
                    duration,
                    chance,
                } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        let defender_e = msg.defender;
                        let attr = attribute.clone();
                        let amt = -*amount; // negative for drain
                        let dur = *duration;
                        commands.queue(move |world: &mut World| {
                            let mut modifiers = world
                                .get_mut::<TimedModifiers>(defender_e)
                                .map(|m| m.clone())
                                .unwrap_or_default();
                            modifiers.entries.push(TimedModifierEntry {
                                attribute: attr.clone(),
                                amount: amt,
                                turns_remaining: dur,
                            });
                            // Recalculate attribute modifiers
                            let mut mods = AttributeModifiers::default();
                            for entry in &modifiers.entries {
                                match entry.attribute.as_str() {
                                    "strength" => mods.strength += entry.amount,
                                    "dexterity" => mods.dexterity += entry.amount,
                                    "constitution" => mods.constitution += entry.amount,
                                    "agility" => mods.agility += entry.amount,
                                    "intelligence" => mods.intelligence += entry.amount,
                                    "perception" => mods.perception += entry.amount,
                                    _ => {}
                                }
                            }
                            if let Ok(mut ec) = world.get_entity_mut(defender_e) {
                                ec.insert(modifiers);
                                ec.insert(mods);
                            }
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s touch drains {}'s {}! (-{} for {} turns)",
                            attacker_name.0, defender_name.0, attribute, amount, duration
                        )));
                    }
                }
                OnHitEffect::LifeDrain { amount, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        heal_writer.write(HealMessage {
                            entity: msg.attacker,
                            amount: *amount,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{} drains {}'s life force! (heals {} HP)",
                            attacker_name.0, defender_name.0, amount
                        )));
                    }
                }
                OnHitEffect::Knockback { distance, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        // Compute direction from attacker to defender (cardinal only)
                        let dx = (defender_pos.x - attacker_pos.x).signum();
                        let dy = (defender_pos.y - attacker_pos.y).signum();
                        if dx == 0 && dy == 0 { continue; }

                        // Build set of occupied tiles for collision
                        let occupied: std::collections::HashSet<(i32, i32)> = collider_query
                            .iter()
                            .map(|p| (p.x, p.y))
                            .collect();

                        let mut final_x = defender_pos.x;
                        let mut final_y = defender_pos.y;
                        for _ in 0..*distance {
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
                OnHitEffect::ApplyBurning {
                    damage_per_turn,
                    duration,
                    chance,
                } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        let fire_resistance = defender_resistances
                            .map(|r| r.get(&DamageType::Fire))
                            .unwrap_or(ResistanceLevel::Normal);
                        if fire_resistance == ResistanceLevel::Immune {
                            log_writer.write(GameLogMessage(format!(
                                "{} resists the flames! (fire immune)",
                                defender_name.0
                            )));
                        } else {
                            commands.entity(msg.defender).insert(Burning {
                                damage_per_turn: *damage_per_turn,
                                turns_remaining: *duration,
                            });
                            log_writer.write(GameLogMessage(format!(
                                "{}'s attack sets {} ablaze! ({} fire dmg/turn for {} turns)",
                                attacker_name.0, defender_name.0, damage_per_turn, duration
                            )));
                        }
                    }
                }
                OnHitEffect::Disarm { duration, chance } => {
                    let roll = game_rng.0.roll_dice(1, 100);
                    if roll <= *chance as i32 {
                        commands.entity(msg.defender).insert(Disarmed {
                            turns_remaining: *duration,
                        });
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack disarms {} for {} turns!",
                            attacker_name.0, defender_name.0, duration
                        )));
                    }
                }
            }
        }
    }
}

// --- On-Death Systems ---

/// ExplodeOnDeath: when an entity with this component reaches 0 HP, deal AoE fire damage.
/// Runs after CombatDamageSet but before death_system (which despawns entities).
pub fn handle_explode_on_death(
    mut death_events: MessageReader<DeathEvent>,
    query: Query<(&Position, &Name, &ExplodeOnDeath)>,
    targets: Query<(Entity, &Position), With<crate::game::combat::Health>>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut particle_writer: MessageWriter<crate::game::particles::ParticleRequest>,
) {
    for event in death_events.read() {
        let Ok((pos, name, explode)) = query.get(event.target) else {
            continue;
        };

        log_writer.write(GameLogMessage(format!("{} explodes!", name.0)));

        let world_pos = grid_to_world(pos.x, pos.y);
        particle_writer.write(crate::game::particles::ParticleRequest::FloatingText {
            world_pos,
            text: "BOOM".to_string(),
            color: Color::srgba(1.0, 0.5, 0.0, 1.0),
            font_size: 8.0,
        });

        for (target_entity, target_pos) in targets.iter() {
            if target_entity == event.target {
                continue;
            }
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

/// Reanimate: when an entity with this component reaches 0 HP, revive it once.
/// Must run after CombatDamageSet and before death_system so HP is positive when death_system checks.
pub fn handle_reanimate(
    mut death_events: MessageReader<DeathEvent>,
    mut commands: Commands,
    mut query: Query<(&mut crate::game::combat::Health, &Name, &Reanimate)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut particle_writer: MessageWriter<crate::game::particles::ParticleRequest>,
    pos_query: Query<&Position>,
) {
    for event in death_events.read() {
        let Ok((mut health, name, reanimate)) = query.get_mut(event.target) else {
            continue;
        };

        health.current = reanimate.revive_hp;
        commands.entity(event.target).remove::<Reanimate>();

        log_writer.write(GameLogMessage(format!("{} rises again!", name.0)));

        if let Ok(pos) = pos_query.get(event.target) {
            let world_pos = grid_to_world(pos.x, pos.y);
            particle_writer.write(crate::game::particles::ParticleRequest::FloatingText {
                world_pos,
                text: "REVIVE".to_string(),
                color: Color::srgba(0.6, 1.0, 0.6, 1.0),
                font_size: 6.0,
            });
        }
    }
}

/// DeathCurse: on death, apply a debuff to the killer.
pub fn handle_death_curse(
    mut death_events: MessageReader<DeathEvent>,
    mut commands: Commands,
    query: Query<(&Name, &DeathCurse)>,
    killer_query: Query<(&Name, Option<&Resistances>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in death_events.read() {
        let Ok((dead_name, curse)) = query.get(event.target) else {
            continue;
        };
        let Ok((killer_name, killer_resistances)) = killer_query.get(event.attacker) else {
            continue;
        };

        match &curse.effect {
            DeathCurseEffect::Slow { duration } => {
                // Check ice immunity
                let ice_resistance = killer_resistances
                    .map(|r| r.get(&DamageType::Ice))
                    .unwrap_or(ResistanceLevel::Normal);
                if ice_resistance == ResistanceLevel::Immune {
                    log_writer.write(GameLogMessage(format!(
                        "{}'s death curse fizzles — {} is immune!",
                        dead_name.0, killer_name.0
                    )));
                } else {
                    commands.entity(event.attacker).insert(Slowed {
                        turns_remaining: *duration,
                    });
                    log_writer.write(GameLogMessage(format!(
                        "{}'s death curse slows {} for {} turns!",
                        dead_name.0, killer_name.0, duration
                    )));
                }
            }
            DeathCurseEffect::Poison {
                damage_per_turn,
                duration,
            } => {
                let poison_resistance = killer_resistances
                    .map(|r| r.get(&DamageType::Poison))
                    .unwrap_or(ResistanceLevel::Normal);
                if poison_resistance == ResistanceLevel::Immune {
                    log_writer.write(GameLogMessage(format!(
                        "{}'s death curse fizzles — {} is immune!",
                        dead_name.0, killer_name.0
                    )));
                } else {
                    commands.entity(event.attacker).insert(Poisoned {
                        damage_per_turn: *damage_per_turn,
                        turns_remaining: *duration,
                    });
                    log_writer.write(GameLogMessage(format!(
                        "{}'s death curse poisons {}!",
                        dead_name.0, killer_name.0
                    )));
                }
            }
            DeathCurseEffect::WeakenStr { amount, duration } => {
                let attacker_e = event.attacker;
                let amt = -(*amount); // negative for debuff
                let dur = *duration;
                let dead_name_str = dead_name.0.clone();
                let killer_name_str = killer_name.0.clone();
                let amount_val = *amount;
                commands.queue(move |world: &mut World| {
                    let mut modifiers = world
                        .get_mut::<TimedModifiers>(attacker_e)
                        .map(|m| m.clone())
                        .unwrap_or_default();
                    modifiers.entries.push(TimedModifierEntry {
                        attribute: "strength".to_string(),
                        amount: amt,
                        turns_remaining: dur,
                    });
                    let mut mods = AttributeModifiers::default();
                    for entry in &modifiers.entries {
                        match entry.attribute.as_str() {
                            "strength" => mods.strength += entry.amount,
                            "dexterity" => mods.dexterity += entry.amount,
                            "constitution" => mods.constitution += entry.amount,
                            "agility" => mods.agility += entry.amount,
                            "intelligence" => mods.intelligence += entry.amount,
                            "perception" => mods.perception += entry.amount,
                            _ => {}
                        }
                    }
                    if let Ok(mut ec) = world.get_entity_mut(attacker_e) {
                        ec.insert(modifiers);
                        ec.insert(mods);
                    }
                });
                log_writer.write(GameLogMessage(format!(
                    "{}'s death curse weakens {}'s strength! (-{} for {} turns)",
                    dead_name_str, killer_name_str, amount_val, dur
                )));
            }
        }
    }
}

/// SummonOnDeath: on death, spawn monsters at adjacent walkable tiles.
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
        let Ok((pos, name, summon)) = query.get(event.target) else {
            continue;
        };

        let occupied: std::collections::HashSet<(i32, i32)> = collider_query
            .iter()
            .map(|p| (p.x, p.y))
            .collect();

        // Find adjacent walkable, unoccupied tiles
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

        if spawn_points.is_empty() {
            continue;
        }

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

// --- On-Being-Hit Systems ---

/// PoisonBody: when this entity takes melee damage, poison the attacker.
pub fn handle_poison_body(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    mut commands: Commands,
    defender_query: Query<(&Name, &PoisonBody)>,
    attacker_query: Query<(&Name, Option<&Resistances>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee {
            continue;
        }
        let Ok((defender_name, poison_body)) = defender_query.get(msg.defender) else {
            continue;
        };
        let Ok((attacker_name, attacker_resistances)) = attacker_query.get(msg.attacker) else {
            continue;
        };

        let poison_resistance = attacker_resistances
            .map(|r| r.get(&DamageType::Poison))
            .unwrap_or(ResistanceLevel::Normal);
        if poison_resistance == ResistanceLevel::Immune {
            log_writer.write(GameLogMessage(format!(
                "{}'s toxic body has no effect — {} is immune!",
                defender_name.0, attacker_name.0
            )));
        } else {
            commands.entity(msg.attacker).insert(Poisoned {
                damage_per_turn: poison_body.stacks,
                turns_remaining: 3,
            });
            log_writer.write(GameLogMessage(format!(
                "{}'s toxic body poisons {}! ({} dmg/turn)",
                defender_name.0, attacker_name.0, poison_body.stacks
            )));
        }
    }
}

/// ThornAura: when this entity takes melee damage, reflect flat damage back.
pub fn handle_thorn_aura(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    defender_query: Query<(&Name, &ThornAura)>,
    attacker_query: Query<&Name>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if msg.source != DamageSource::Melee {
            continue;
        }
        let Ok((defender_name, thorns)) = defender_query.get(msg.defender) else {
            continue;
        };
        let Ok(attacker_name) = attacker_query.get(msg.attacker) else {
            continue;
        };

        damage_writer.write(ApplyDamageMessage {
            attacker: msg.defender,
            target: msg.attacker,
            final_damage: thorns.damage,
            damage_type: DamageType::Physical,
            source: DamageSource::Environment,
        });
        log_writer.write(GameLogMessage(format!(
            "{}'s thorns deal {} damage to {}!",
            defender_name.0, thorns.damage, attacker_name.0
        )));
    }
}

/// EnrageOnHit: when this entity drops below threshold% HP, gain Enraged.
pub fn handle_enrage_on_hit(
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    mut commands: Commands,
    query: Query<(&Name, &crate::game::combat::Health, &EnrageOnHit, Has<Enraged>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut particle_writer: MessageWriter<crate::game::particles::ParticleRequest>,
    pos_query: Query<&Position>,
) {
    for msg in messages.read() {
        let Ok((name, health, enrage, already_enraged)) = query.get(msg.defender) else {
            continue;
        };

        if already_enraged {
            continue;
        }

        let threshold_hp = health.max * enrage.threshold_percent as i32 / 100;
        if health.current <= threshold_hp && health.current > 0 {
            commands.entity(msg.defender).insert(Enraged {
                turns_remaining: 99, // Essentially permanent once triggered
            });
            log_writer.write(GameLogMessage(format!(
                "{} flies into a rage! (+50% damage)",
                name.0
            )));

            if let Ok(pos) = pos_query.get(msg.defender) {
                let world_pos = grid_to_world(pos.x, pos.y);
                particle_writer.write(crate::game::particles::ParticleRequest::FloatingText {
                    world_pos,
                    text: "ENRAGED".to_string(),
                    color: Color::srgba(1.0, 0.2, 0.2, 1.0),
                    font_size: 6.0,
                });
            }
        }
    }
}

// --- Plugin ---

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_on_hit_effects
                    .after(crate::game::combat::CombatDamageSet),
                // On-being-hit systems
                handle_poison_body
                    .after(crate::game::combat::CombatDamageSet),
                handle_thorn_aura
                    .after(crate::game::combat::CombatDamageSet),
                handle_enrage_on_hit
                    .after(crate::game::combat::CombatDamageSet),
                // On-death systems (must run before death_system despawns entities)
                handle_explode_on_death
                    .after(crate::game::combat::CombatDamageSet),
                handle_reanimate
                    .after(crate::game::combat::CombatDamageSet),
                handle_death_curse
                    .after(crate::game::combat::CombatDamageSet),
                handle_summon_on_death
                    .after(crate::game::combat::CombatDamageSet),
            )
                .run_if(in_state(crate::game::AppState::InGame)),
        );
    }
}
