//! Runic enchantment system — Brogue-inspired weapon/armor runics and enchantment levels.
//!
//! Items can have:
//! - An `Enchantment` level (+N) that boosts damage (weapons) or defense (armor)
//! - A `WeaponRunic` or `ArmorRunic` that procs on hit/being-hit
//! - A `RunicIdentified` state (hidden until the runic triggers in combat)
//!
//! Proc chance scales with enchantment level and (for weapons) inversely with base damage,
//! following Brogue's design: low-damage weapons like daggers proc more often.

use bevy::prelude::*;
use bracket_lib::random::RandomNumberGenerator;
use serde::{Deserialize, Serialize};

use crate::components::{Name, Position};
use crate::game::abilities::OnHitTriggerMessage;
use crate::game::abilities::OnBeingHitTriggerMessage;
use crate::game::combat::{
    ApplyDamageMessage, CombatDamageSet, DamageSource, DamageType, GameRng, HealMessage, Health,
};
use crate::game::items::{Equipment, ItemKind, ItemProperties};
use crate::game::magic::{StatusEffectKind, StatusEffects};
use crate::map::map::Map;
use crate::map::tile::is_walkable;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// =====================================================================
// Enums
// =====================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum WeaponRunic {
    /// Next action costs 0 time (free turn).
    Speed,
    /// Apply Slowed status to the target.
    Slowing,
    /// Knockback with collision damage.
    Force,
    /// Stun the target for several turns.
    Paralysis,
    /// Instant kill when target HP is below a threshold.
    Quietus,
    /// +1d4 fire damage on hit.
    Flames,
    /// +1d4 poison damage on hit + Poisoned DoT.
    Venom,
    /// +1d4 lightning damage on hit + 25% chance to arc to a nearby enemy.
    Lightning,
    /// +50% damage vs a specific faction.
    Slaying { faction: String },
}

/// Factions that can appear on a Slaying runic.
pub const SLAYING_FACTIONS: &[&str] = &["Goblin", "Dragon", "Undead", "Kobold", "Monster"];

impl WeaponRunic {
    pub fn name(&self) -> String {
        match self {
            WeaponRunic::Speed => "Speed".to_string(),
            WeaponRunic::Slowing => "Slowing".to_string(),
            WeaponRunic::Force => "Force".to_string(),
            WeaponRunic::Paralysis => "Paralysis".to_string(),
            WeaponRunic::Quietus => "Quietus".to_string(),
            WeaponRunic::Flames => "Flames".to_string(),
            WeaponRunic::Venom => "Venom".to_string(),
            WeaponRunic::Lightning => "Lightning".to_string(),
            WeaponRunic::Slaying { faction } => format!("{} Slaying", faction),
        }
    }

    pub fn description(&self) -> String {
        match self {
            WeaponRunic::Speed => "grants a free turn".to_string(),
            WeaponRunic::Slowing => "slows the target".to_string(),
            WeaponRunic::Force => "knocks the target back".to_string(),
            WeaponRunic::Paralysis => "paralyzes the target".to_string(),
            WeaponRunic::Quietus => "instant kill on wounded targets".to_string(),
            WeaponRunic::Flames => "+1d4 fire damage on hit".to_string(),
            WeaponRunic::Venom => "+1d4 poison damage + poison DoT".to_string(),
            WeaponRunic::Lightning => "+1d4 lightning damage, may arc".to_string(),
            WeaponRunic::Slaying { faction } => format!("+50% damage vs {}", faction),
        }
    }

    /// Base proc rate per enchantment level (percentage points).
    /// Lower = rarer. Brogue-inspired: powerful effects proc less often.
    fn base_rate(&self) -> f32 {
        match self {
            WeaponRunic::Speed => 3.0,
            WeaponRunic::Slowing => 6.0,
            WeaponRunic::Force => 5.0,
            WeaponRunic::Paralysis => 4.0,
            WeaponRunic::Quietus => 7.0,
            WeaponRunic::Flames => 6.0,
            WeaponRunic::Venom => 5.0,
            WeaponRunic::Lightning => 4.0,
            WeaponRunic::Slaying { .. } => 7.0,
        }
    }

    /// Generate a random WeaponRunic. Slaying picks a random faction.
    pub fn random(rng: &mut RandomNumberGenerator) -> WeaponRunic {
        let variant = rng.range(0, 9);
        match variant {
            0 => WeaponRunic::Speed,
            1 => WeaponRunic::Slowing,
            2 => WeaponRunic::Force,
            3 => WeaponRunic::Paralysis,
            4 => WeaponRunic::Quietus,
            5 => WeaponRunic::Flames,
            6 => WeaponRunic::Venom,
            7 => WeaponRunic::Lightning,
            _ => {
                let idx = rng.range(0, SLAYING_FACTIONS.len() as i32) as usize;
                WeaponRunic::Slaying { faction: SLAYING_FACTIONS[idx].to_string() }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum ArmorRunic {
    /// Reflect a percentage of damage back at the attacker.
    Reprisal,
    /// Reduce incoming damage by a random amount.
    Absorption,
    /// 30% chance to reflect ranged projectiles back at the attacker.
    Reflection,
    /// +75% resistance to one damage type (fire, lightning, or poison).
    Immunity { damage_type: DamageType },
}

impl ArmorRunic {
    pub fn name(&self) -> String {
        match self {
            ArmorRunic::Reprisal => "Reprisal".to_string(),
            ArmorRunic::Absorption => "Absorption".to_string(),
            ArmorRunic::Reflection => "Reflection".to_string(),
            ArmorRunic::Immunity { damage_type } => format!("{} Immunity", damage_type.name()),
        }
    }

    pub fn description(&self) -> String {
        match self {
            ArmorRunic::Reprisal => "reflects damage back".to_string(),
            ArmorRunic::Absorption => "absorbs incoming damage".to_string(),
            ArmorRunic::Reflection => "reflects ranged projectiles".to_string(),
            ArmorRunic::Immunity { damage_type } => format!("+75% {} resistance", damage_type.name()),
        }
    }

    /// Base proc rate per enchantment level (percentage points).
    fn base_rate(&self) -> f32 {
        match self {
            ArmorRunic::Reprisal => 5.0,
            ArmorRunic::Absorption => 7.0,
            ArmorRunic::Reflection => 5.0,
            ArmorRunic::Immunity { .. } => 7.0,
        }
    }

    /// Generate a random ArmorRunic. Immunity picks a random element.
    pub fn random(rng: &mut RandomNumberGenerator) -> ArmorRunic {
        let variant = rng.range(0, 4);
        match variant {
            0 => ArmorRunic::Reprisal,
            1 => ArmorRunic::Absorption,
            2 => ArmorRunic::Reflection,
            _ => {
                let element = match rng.range(0, 3) {
                    0 => DamageType::Fire,
                    1 => DamageType::Lightning,
                    _ => DamageType::Poison,
                };
                ArmorRunic::Immunity { damage_type: element }
            }
        }
    }
}

// =====================================================================
// Components
// =====================================================================

/// Enchantment level on a weapon or armor piece. Affects damage/defense and runic proc chance.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Enchantment {
    pub level: i32,
}

/// A weapon runic attached to a weapon item entity.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ItemWeaponRunic(pub WeaponRunic);

/// An armor runic attached to an armor item entity.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ItemArmorRunic(pub ArmorRunic);

/// Whether this item's runic has been identified by the player.
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct RunicIdentified(pub bool);

/// Marker on the player entity: next action costs 0 time (consumed by resolve_turn_end).
#[derive(Component, Debug, Clone)]
pub struct SpeedRunicProc;

// =====================================================================
// Proc Chance Formulas
// =====================================================================

/// Compute the average damage from a dice string like "1d6" or "1d4+2".
fn avg_damage_from_dice(dice_str: &str) -> f32 {
    // Parse "NdM" or "NdM+B"
    let dice_str = dice_str.trim();
    let (dice_part, bonus) = if let Some(plus_idx) = dice_str.find('+') {
        let bonus: f32 = dice_str[plus_idx + 1..].trim().parse().unwrap_or(0.0);
        (&dice_str[..plus_idx], bonus)
    } else {
        (dice_str, 0.0)
    };

    if let Some(d_idx) = dice_part.find('d') {
        let n: f32 = dice_part[..d_idx].trim().parse().unwrap_or(1.0);
        let m: f32 = dice_part[d_idx + 1..].trim().parse().unwrap_or(4.0);
        n * (m + 1.0) / 2.0 + bonus
    } else {
        // Flat damage
        dice_str.parse::<f32>().unwrap_or(2.0)
    }
}

/// Weapon runic proc chance (percentage, 0-100).
/// Lower-damage weapons proc more often (Brogue design).
pub fn weapon_runic_proc_chance(runic: &WeaponRunic, enchant_level: i32, damage_dice: &str) -> u32 {
    let avg_dmg = avg_damage_from_dice(damage_dice);
    let modifier = 1.0 - (avg_dmg / 18.0).min(0.99);
    let chance = runic.base_rate() * enchant_level.max(0) as f32 * modifier + 5.0;
    (chance as u32).clamp(1, 75)
}

/// Armor runic proc chance (percentage, 0-100).
pub fn armor_runic_proc_chance(runic: ArmorRunic, enchant_level: i32) -> u32 {
    let chance = runic.base_rate() * enchant_level.max(0) as f32 + 10.0;
    (chance as u32).clamp(5, 50)
}

// =====================================================================
// Display Name Helper
// =====================================================================

/// Build a display name like "+3 Sword of Speed" from item components.
pub fn display_item_name(
    base_name: &str,
    enchantment: Option<&Enchantment>,
    weapon_runic: Option<&ItemWeaponRunic>,
    armor_runic: Option<&ItemArmorRunic>,
    runic_identified: Option<&RunicIdentified>,
) -> String {
    let mut name = String::new();

    // Enchantment prefix
    if let Some(ench) = enchantment {
        if ench.level != 0 {
            name.push_str(&format!("+{} ", ench.level));
        }
    }

    name.push_str(base_name);

    // Runic suffix
    let is_identified = runic_identified.is_some_and(|r| r.0);
    let has_runic = weapon_runic.is_some() || armor_runic.is_some();

    if has_runic {
        if is_identified {
            if let Some(wr) = weapon_runic {
                name.push_str(&format!(" of {}", wr.0.name()));
            } else if let Some(ar) = armor_runic {
                name.push_str(&format!(" of {}", ar.0.name()));
            }
        } else {
            name.push_str(" (runic)");
        }
    }

    name
}

// =====================================================================
// Handler Systems
// =====================================================================

/// Weapon runic proc: on hit, check attacker's equipped weapon for a runic and roll.
pub fn handle_weapon_runic_proc(
    mut commands: Commands,
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    player_query: Query<&Equipment, With<Player>>,
    weapon_query: Query<(
        &ItemProperties,
        &ItemWeaponRunic,
        Option<&Enchantment>,
        Option<&RunicIdentified>,
    )>,
    defender_query: Query<(&Name, &Health)>,
    mut status_query: Query<&mut StatusEffects>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    attacker_name_query: Query<&Name>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
    attacker_pos_query: Query<&Position>,
    defender_pos_query: Query<&Position>,
    map: Res<Map>,
    monster_query: Query<(Entity, &Position, &Name), With<crate::components::Monster>>,
    faction_query: Query<&crate::components::Faction>,
) {
    for msg in messages.read() {
        // Only player weapon runics for now
        let Ok(equipment) = player_query.get(msg.attacker) else { continue; };
        let Some(weapon_entity) = equipment.weapon else { continue; };
        let Ok((props, runic, enchant, identified)) = weapon_query.get(weapon_entity) else { continue; };

        let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
        let damage_dice = props.damage.as_deref().unwrap_or("1d4");
        let chance = weapon_runic_proc_chance(&runic.0, enchant_level, damage_dice);

        let roll = game_rng.0.roll_dice(1, 100) as u32;
        if roll > chance { continue; }

        // Reveal runic on first proc
        if !identified.is_some_and(|r| r.0) {
            commands.entity(weapon_entity).insert(RunicIdentified(true));
            let attacker_name = attacker_name_query.get(msg.attacker).map(|n| n.0.as_str()).unwrap_or("You");
            log_writer.write(GameLogMessage(format!(
                "{} discover your weapon is a weapon of {}!",
                attacker_name, runic.0.name()
            )));
        }

        // Apply effect
        match &runic.0 {
            WeaponRunic::Speed => {
                commands.entity(msg.attacker).insert(SpeedRunicProc);
                log_writer.write(GameLogMessage(
                    "Your weapon trembles and time freezes for a moment!".to_string(),
                ));
            }
            WeaponRunic::Slowing => {
                let duration = (3 + enchant_level).max(1) as u32;
                if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                    effects.add(StatusEffectKind::Slowed, duration);
                }
                if let Ok((defender_name, _)) = defender_query.get(msg.defender) {
                    log_writer.write(GameLogMessage(format!(
                        "Your weapon's runic slows {}!",
                        defender_name.0
                    )));
                }
            }
            WeaponRunic::Force => {
                // Knockback: reuse pattern from abilities.rs handle_knockback
                let Ok(attacker_pos) = attacker_pos_query.get(msg.attacker) else { continue; };
                let Ok(defender_pos) = defender_pos_query.get(msg.defender) else { continue; };

                let dx = (defender_pos.x - attacker_pos.x).signum();
                let dy = (defender_pos.y - attacker_pos.y).signum();
                if dx == 0 && dy == 0 { continue; }

                let distance = 2 + enchant_level / 3;
                let occupied: std::collections::HashSet<(i32, i32)> = collider_query
                    .iter()
                    .map(|p| (p.x, p.y))
                    .collect();

                let mut final_x = defender_pos.x;
                let mut final_y = defender_pos.y;
                for _ in 0..distance {
                    let nx = final_x + dx;
                    let ny = final_y + dy;
                    let idx = map.xy_idx(nx, ny);
                    if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) || occupied.contains(&(nx, ny)) {
                        break;
                    }
                    final_x = nx;
                    final_y = ny;
                }

                if final_x != defender_pos.x || final_y != defender_pos.y {
                    commands.entity(msg.defender).insert(Position { x: final_x, y: final_y });
                    if let Ok((defender_name, _)) = defender_query.get(msg.defender) {
                        log_writer.write(GameLogMessage(format!(
                            "Your weapon's force hurls {} backward!",
                            defender_name.0
                        )));
                    }
                }
            }
            WeaponRunic::Paralysis => {
                let duration = (2 + enchant_level / 2).max(1) as u32;
                if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                    effects.add(StatusEffectKind::Stunned, duration);
                }
                if let Ok((defender_name, _)) = defender_query.get(msg.defender) {
                    log_writer.write(GameLogMessage(format!(
                        "Your weapon's runic paralyzes {}!",
                        defender_name.0
                    )));
                }
            }
            WeaponRunic::Quietus => {
                let Ok((defender_name, defender_health)) = defender_query.get(msg.defender) else { continue; };
                let threshold_pct = (3 + enchant_level) * 5;
                let threshold_hp = defender_health.max * threshold_pct / 100;
                if defender_health.current <= threshold_hp {
                    damage_writer.write(ApplyDamageMessage {
                        attacker: msg.attacker,
                        target: msg.defender,
                        final_damage: defender_health.current,
                        damage_type: DamageType::Physical,
                        source: DamageSource::Melee,
                    });
                    log_writer.write(GameLogMessage(format!(
                        "Your weapon's runic strikes {} down!",
                        defender_name.0
                    )));
                }
            }
            WeaponRunic::Flames => {
                let fire_damage = game_rng.0.roll_dice(1, 4);
                damage_writer.write(ApplyDamageMessage {
                    attacker: msg.attacker,
                    target: msg.defender,
                    final_damage: fire_damage,
                    damage_type: DamageType::Fire,
                    source: DamageSource::Melee,
                });
                let attacker_name = attacker_name_query.get(msg.attacker).map(|n| n.0.as_str()).unwrap_or("Your");
                log_writer.write(GameLogMessage(format!(
                    "{}'s weapon erupts in flame!",
                    attacker_name
                )));
            }
            WeaponRunic::Venom => {
                let poison_damage = game_rng.0.roll_dice(1, 4);
                damage_writer.write(ApplyDamageMessage {
                    attacker: msg.attacker,
                    target: msg.defender,
                    final_damage: poison_damage,
                    damage_type: DamageType::Poison,
                    source: DamageSource::Melee,
                });
                if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                    effects.add(StatusEffectKind::Poisoned { damage_per_turn: 2 }, 3);
                }
                let attacker_name = attacker_name_query.get(msg.attacker).map(|n| n.0.as_str()).unwrap_or("Your");
                log_writer.write(GameLogMessage(format!(
                    "{}'s weapon drips with venom!",
                    attacker_name
                )));
            }
            WeaponRunic::Lightning => {
                let lightning_damage = game_rng.0.roll_dice(1, 4);
                damage_writer.write(ApplyDamageMessage {
                    attacker: msg.attacker,
                    target: msg.defender,
                    final_damage: lightning_damage,
                    damage_type: DamageType::Lightning,
                    source: DamageSource::Melee,
                });
                let attacker_name = attacker_name_query.get(msg.attacker).map(|n| n.0.as_str()).unwrap_or("Your");
                log_writer.write(GameLogMessage(format!(
                    "{}'s weapon crackles with lightning!",
                    attacker_name
                )));

                // 25% chance to arc to a nearby enemy within 3 tiles of the target
                let arc_roll = game_rng.0.roll_dice(1, 4);
                if arc_roll == 1 {
                    if let Ok(defender_pos) = defender_pos_query.get(msg.defender) {
                        // Find nearest enemy within 3 tiles of the defender (not the attacker)
                        let mut best: Option<(Entity, i32)> = None;
                        for (entity, pos, _name_comp) in monster_query.iter() {
                            if entity == msg.defender || entity == msg.attacker { continue; }
                            let dist = (pos.x - defender_pos.x).abs() + (pos.y - defender_pos.y).abs();
                            if dist <= 3 {
                                if best.is_none() || dist < best.unwrap().1 {
                                    best = Some((entity, dist));
                                }
                            }
                        }
                        if let Some((arc_target, _)) = best {
                            let arc_damage = game_rng.0.roll_dice(1, 4);
                            damage_writer.write(ApplyDamageMessage {
                                attacker: msg.attacker,
                                target: arc_target,
                                final_damage: arc_damage,
                                damage_type: DamageType::Lightning,
                                source: DamageSource::Melee,
                            });
                            if let Ok((_, arc_pos, arc_name)) = monster_query.get(arc_target) {
                                let _ = arc_pos; // suppress unused
                                log_writer.write(GameLogMessage(format!(
                                    "Lightning arcs to {}!",
                                    arc_name.0
                                )));
                            }
                        }
                    }
                }
            }
            WeaponRunic::Slaying { faction } => {
                if let Ok((_defender_name, _defender_health)) = defender_query.get(msg.defender) {
                    // Check if the defender's faction matches
                    let faction_matches = faction_query.get(msg.defender)
                        .map(|f| f.0.0 == *faction)
                        .unwrap_or(false);

                    if faction_matches {
                        // Deal 50% bonus damage (based on the hit's damage)
                        let bonus = (msg.final_damage / 2).max(1);
                        damage_writer.write(ApplyDamageMessage {
                            attacker: msg.attacker,
                            target: msg.defender,
                            final_damage: bonus,
                            damage_type: DamageType::Physical,
                            source: DamageSource::Melee,
                        });
                        let attacker_name = attacker_name_query.get(msg.attacker).map(|n| n.0.as_str()).unwrap_or("Your");
                        log_writer.write(GameLogMessage(format!(
                            "{}'s weapon glows against the {}!",
                            attacker_name, faction
                        )));
                    }
                }
            }
        }
    }
}

/// Armor runic proc: on being hit, check defender's equipped armor for runics and roll.
pub fn handle_armor_runic_proc(
    mut commands: Commands,
    mut messages: MessageReader<OnBeingHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    player_query: Query<&Equipment, With<Player>>,
    armor_query: Query<(
        &ItemArmorRunic,
        Option<&Enchantment>,
        Option<&RunicIdentified>,
    )>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    attacker_name_query: Query<&Name>,
) {
    for msg in messages.read() {
        // Only player armor runics for now
        let Ok(equipment) = player_query.get(msg.defender) else { continue; };

        // Check all armor slots
        let armor_slots = [
            equipment.chest,
            equipment.helm,
            equipment.gloves,
            equipment.boots,
            equipment.offhand,
        ];

        for slot in armor_slots.iter().flatten() {
            let Ok((runic, enchant, identified)) = armor_query.get(*slot) else { continue; };

            let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
            let chance = armor_runic_proc_chance(runic.0, enchant_level);

            let roll = game_rng.0.roll_dice(1, 100) as u32;
            if roll > chance { continue; }

            // Reveal runic on first proc
            if !identified.is_some_and(|r| r.0) {
                commands.entity(*slot).insert(RunicIdentified(true));
                log_writer.write(GameLogMessage(format!(
                    "You discover your armor has a runic of {}!",
                    runic.0.name()
                )));
            }

            match runic.0 {
                ArmorRunic::Reprisal => {
                    let reflect_pct = (10 + enchant_level * 5).max(5);
                    let reflect_damage = (msg.final_damage * reflect_pct / 100).max(1);
                    damage_writer.write(ApplyDamageMessage {
                        attacker: msg.defender,
                        target: msg.attacker,
                        final_damage: reflect_damage,
                        damage_type: DamageType::Physical,
                        source: DamageSource::Environment,
                    });
                    if let Ok(attacker_name) = attacker_name_query.get(msg.attacker) {
                        log_writer.write(GameLogMessage(format!(
                            "Your armor's reprisal deals {} damage to {}!",
                            reflect_damage, attacker_name.0
                        )));
                    }
                }
                ArmorRunic::Absorption => {
                    let max_absorb = (enchant_level * 3).max(1);
                    let absorb = if max_absorb > 1 {
                        game_rng.0.range(1, max_absorb + 1)
                    } else {
                        1
                    };
                    heal_writer.write(HealMessage {
                        entity: msg.defender,
                        amount: absorb,
                    });
                    log_writer.write(GameLogMessage(format!(
                        "Your armor absorbs {} damage!",
                        absorb
                    )));
                }
                ArmorRunic::Reflection => {
                    // Only reflects ranged attacks
                    if msg.source == DamageSource::Ranged {
                        // 30% chance to reflect
                        let reflect_roll = game_rng.0.roll_dice(1, 100);
                        if reflect_roll <= 30 {
                            damage_writer.write(ApplyDamageMessage {
                                attacker: msg.defender,
                                target: msg.attacker,
                                final_damage: msg.final_damage,
                                damage_type: msg.damage_type,
                                source: DamageSource::Ranged,
                            });
                            log_writer.write(GameLogMessage(
                                "Your armor reflects the projectile!".to_string(),
                            ));
                        }
                    }
                }
                ArmorRunic::Immunity { damage_type } => {
                    // Placeholder: log the protective effect
                    let element_name = damage_type.name();
                    log_writer.write(GameLogMessage(format!(
                        "You feel protected from {}.",
                        element_name
                    )));
                }
            }
        }
    }
}

// =====================================================================
// Random Enchantment Generation
// =====================================================================

/// Roll random enchantment level and optional runic for a weapon or armor item.
/// Called from spawner after item entity creation.
pub fn enchant_item(
    commands: &mut Commands,
    item_entity: Entity,
    item_kind: &ItemKind,
    floor_depth: u32,
    rng: &mut RandomNumberGenerator,
) {
    if !matches!(item_kind, ItemKind::Weapon | ItemKind::Armor | ItemKind::Staff) {
        return;
    }

    // Roll enchantment level: 0 to floor_depth/3 + 1
    let max_enchant = (floor_depth / 3 + 2) as i32;
    let enchant_level = rng.range(0, max_enchant);
    commands.entity(item_entity).insert(Enchantment { level: enchant_level });

    // Roll for runic
    let runic_chance = 15 + floor_depth * 2;
    let runic_roll = rng.range(0, 100);
    if runic_roll < runic_chance as i32 {
        match item_kind {
            ItemKind::Weapon => {
                commands.entity(item_entity).insert(ItemWeaponRunic(WeaponRunic::random(rng)));
            }
            ItemKind::Armor => {
                commands.entity(item_entity).insert(ItemArmorRunic(ArmorRunic::random(rng)));
            }
            _ => {}
        }
        commands.entity(item_entity).insert(RunicIdentified(false));
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct EnchantmentPlugin;

impl Plugin for EnchantmentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Enchantment>()
            .register_type::<ItemWeaponRunic>()
            .register_type::<ItemArmorRunic>()
            .register_type::<RunicIdentified>()
            .add_systems(
                Update,
                (
                    handle_weapon_runic_proc.after(CombatDamageSet),
                    handle_armor_runic_proc.after(CombatDamageSet),
                )
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avg_damage_parsing() {
        assert!((avg_damage_from_dice("1d4") - 2.5).abs() < 0.01);
        assert!((avg_damage_from_dice("1d6") - 3.5).abs() < 0.01);
        assert!((avg_damage_from_dice("2d6") - 7.0).abs() < 0.01);
        assert!((avg_damage_from_dice("1d4+2") - 4.5).abs() < 0.01);
    }

    #[test]
    fn weapon_proc_chance_dagger_vs_sword() {
        // Daggers (1d4) should proc more often than swords (1d6)
        let dagger = weapon_runic_proc_chance(&WeaponRunic::Speed, 3, "1d4");
        let sword = weapon_runic_proc_chance(&WeaponRunic::Speed, 3, "1d6");
        assert!(dagger >= sword, "Dagger ({}) should proc >= Sword ({})", dagger, sword);
    }

    #[test]
    fn weapon_proc_chance_floor_bonus() {
        // Even at +0 enchant, there should be a minimum chance
        let chance = weapon_runic_proc_chance(&WeaponRunic::Speed, 0, "1d4");
        assert!(chance >= 1, "Minimum proc chance should be at least 1%");
    }

    #[test]
    fn armor_proc_chance_scaling() {
        let c0 = armor_runic_proc_chance(ArmorRunic::Reprisal, 0);
        let c3 = armor_runic_proc_chance(ArmorRunic::Reprisal, 3);
        assert!(c3 > c0, "+3 should have higher proc chance than +0");
    }

    #[test]
    fn display_name_basic() {
        assert_eq!(
            display_item_name("Dagger", None, None, None, None),
            "Dagger"
        );
    }

    #[test]
    fn display_name_enchanted() {
        let ench = Enchantment { level: 3 };
        assert_eq!(
            display_item_name("Sword", Some(&ench), None, None, None),
            "+3 Sword"
        );
    }

    #[test]
    fn display_name_identified_runic() {
        let ench = Enchantment { level: 2 };
        let runic = ItemWeaponRunic(WeaponRunic::Speed);
        let id = RunicIdentified(true);
        assert_eq!(
            display_item_name("Dagger", Some(&ench), Some(&runic), None, Some(&id)),
            "+2 Dagger of Speed"
        );
    }

    #[test]
    fn display_name_unidentified_runic() {
        let ench = Enchantment { level: 1 };
        let runic = ItemWeaponRunic(WeaponRunic::Force);
        let id = RunicIdentified(false);
        assert_eq!(
            display_item_name("Sword", Some(&ench), Some(&runic), None, Some(&id)),
            "+1 Sword (runic)"
        );
    }
}
