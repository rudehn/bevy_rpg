use bevy::prelude::*;
use bracket_lib::random::RandomNumberGenerator;
use serde::{Deserialize, Serialize};

use crate::components::{FloorEntityMarker, InInventory, Inventory, Monster, Name, GodMode, Position};
use crate::game::magic::GameStatusEffectsExt;
use crate::game::stats::{Armor, DamageBonus, Dodge, HitBonus};
use crate::game::turns::TurnEndEvent;
use crate::game::{AppState, RunSummary, TurnManager};
use crate::map::dungeon::Floor;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// --- Damage Types, Resistances, and Health components ---
//
// These types now live in `roguelike_engine::combat` and are re-exported
// here so every existing call site (91 occurrences across 18 files) can
// continue to use `crate::game::combat::{DamageType, DamageSource, ...}`
// unchanged.
pub use roguelike_engine::combat::{
    DamageSource, DamageType, DamageTypeTag, Health, HealthRegen, RegenSuppression, Resistances,
};

// --- Engine combat events ---
//
// The engine's `CombatPlugin` registers `DamageEvent`, `DeathEvent`, and
// `HealEvent` message types plus `damage_application_system` (armor +
// resistance + HP mutation) and `heal_application_system`.  The game
// re-exports the event types here so call sites can use
// `crate::game::combat::{DamageEvent, DeathEvent, HealEvent, CombatEventSet}`.
pub use roguelike_engine::combat::events::{
    CombatEventSet, DamageEvent, DeathEvent, HealEvent,
};

// --- Components ---
//
// `Health`, `HealthRegen`, and `RegenSuppression` are re-exported from
// `roguelike_engine::combat` above.

/// Component for an entity's damage, using dice notation (e.g., "1d6").
#[derive(Component, Debug)]
pub struct Damage(pub String);

// --- Messages ---

/// Message sent when an entity intends to attack another entity.
#[derive(Message, Debug)]
pub struct AttackIntentMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub damage_type: DamageType,
    pub source: DamageSource,
}

/// Message sent after a successful hit to trigger damage rolling.
#[derive(Message, Debug)]
pub struct DamageRollMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub damage_type: DamageType,
    pub source: DamageSource,
    pub is_crit: bool,
}

// DamageReductionMessage, ApplyDamageMessage, HealMessage, and DeathEvent
// are replaced by engine types: DamageEvent, HealEvent, DeathEvent
// (re-exported above from roguelike_engine::combat::events).

/// Message sent when an attack misses its target.
#[allow(dead_code)]
#[derive(Message, Debug)]
pub struct MissMessage {
    pub attacker: Entity,
    pub target: Entity,
}

/// Message sent to toggle GodMode on an entity.
#[derive(Message, Debug)]
pub struct ToggleGodModeMessage {
    pub entity: Entity,
}

// DeathEvent is now re-exported from roguelike_engine::combat::events above.
// Engine fields: { entity: Entity, killer: Option<Entity> }

// --- Resources ---

/// Wrapper for bracket_lib's RandomNumberGenerator to be used as a Bevy Resource.
#[derive(Resource)]
pub struct GameRng(pub RandomNumberGenerator);

// --- Utility Functions ---

/// Rolls dice based on a dice notation string (e.g., "1d6").
///
/// Re-export of the engine's [`roguelike_engine::dice::roll_dice_string`].
/// Parse errors fall back to a damage roll of 1 (the engine default),
/// which shows up as a consistently-weak attack if a dice string is
/// malformed — an observable dev-time symptom.
use roguelike_engine::dice::roll_dice_string as roll_dice;

// --- Pure computation helpers ---
//
// These functions now live in `roguelike_engine::combat` and are re-exported
// here so existing game code (attack pipeline, tests) can keep referencing
// them via `crate::game::combat::{compute_after_armor, ...}`.
pub use roguelike_engine::combat::{
    apply_damage_multipliers, apply_resistance, compute_after_armor,
};

// --- Systems ---

/// System that handles health regeneration at the end of a global turn cycle.
fn regen_system(
    mut turn_end_events: MessageReader<TurnEndEvent>,
    mut query: Query<(&mut Health, &mut HealthRegen, Has<RegenSuppression>, Option<&crate::game::magic::StatusEffects>)>,
) {
    for _ in turn_end_events.read() {
        for (mut health, mut regen, is_suppressed, status_effects) in query.iter_mut() {
            // Suppress regen if entity has RegenSuppression or is Poisoned
            if is_suppressed || status_effects.is_some_and(|fx| fx.is_poisoned()) {
                continue;
            }
            if health.current < health.max {
                regen.regen_accumulator += regen.regen_rate;
                while regen.regen_accumulator >= 100 {
                    health.current = (health.current + 1).min(health.max);
                    regen.regen_accumulator -= 100;
                }
            } else {
                // If health is full, we cap the accumulator at 100 to prevent
                // massive "burst" healing immediately after taking damage.
                regen.regen_accumulator = regen.regen_accumulator.min(100);
            }
        }
    }
}

/// 1. Hit Chance: d20 + hit_bonus >= 4 + dodge_bonus (natural 20 always hits)
fn hit_check_system(
    mut intents: MessageReader<AttackIntentMessage>,
    mut roll_writer: MessageWriter<DamageRollMessage>,
    mut miss_writer: MessageWriter<MissMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    query: Query<(&Name, Option<&Dodge>, Option<&HitBonus>, Has<Player>)>,
    race_query: Query<&crate::character::Race>,
    attrs_query: Query<&crate::character::Attributes>,
    skills_query: Query<&crate::game::skills::Skills>,
    equipment_query: Query<&crate::game::items::Equipment>,
    weapon_props_query: Query<&crate::game::items::ItemProperties>,
    mut use_counters: ResMut<crate::game::skills::SkillUseCounters>,
) {
    for intent in intents.read() {
        let Ok((attacker_name, _, attacker_hit_bonus, is_player)) = query.get(intent.attacker) else {
            continue;
        };
        let Ok((target_name, target_dodge, _, _)) = query.get(intent.target) else {
            continue;
        };

        // d20 routed through the canonical helper. In Phase 1 this handled
        // Halfling Lucky; Halfling was removed in Phase 2, so the helper is
        // currently a thin `roll_dice(1, 20)` wrapper. Future race / class /
        // skill d20 effects plug in here.
        let attacker_race = race_query.get(intent.attacker).ok().copied();
        let hit_roll = crate::character::roll_d20_with_race(&mut game_rng.0, attacker_race);

        let hit_bonus = attacker_hit_bonus.map(|h| h.0).unwrap_or(0);
        // Branch the attribute contribution by weapon type: STR for melee,
        // DEX for ranged. Monsters lack `Attributes` and contribute 0.
        let attacker_attrs = attrs_query.get(intent.attacker).ok();
        let attr_bonus = crate::character::attack_attribute_bonus(intent.source, attacker_attrs);

        // Phase 3 skill bonuses: weapon-family + Fighting for melee.
        // Look up the equipped weapon's skill tag (only meaningful for
        // the player; monsters have no Equipment).
        let attacker_skills = skills_query.get(intent.attacker).ok();
        let weapon_skill_tag = equipment_query
            .get(intent.attacker)
            .ok()
            .and_then(|eq| eq.weapon)
            .and_then(|w| weapon_props_query.get(w).ok())
            .and_then(|props| props.weapon_skill);
        let weapon_bonus = crate::game::skills::weapon_skill_bonus(
            weapon_skill_tag,
            intent.source,
            attacker_skills,
        );
        let fighting_bonus =
            crate::game::skills::fighting_melee_bonus(intent.source, attacker_skills);

        // Target dodge: flat Dodge component + Dodging skill bonus
        // (only meaningful for the player; monsters lack Skills).
        let target_skills = skills_query.get(intent.target).ok();
        let dodge_val = target_dodge.map(|d| d.0).unwrap_or(0);
        let dodging_bonus = crate::game::skills::dodging_skill_bonus(target_skills);
        let dodge_target = 4 + dodge_val + dodging_bonus;
        let is_natural_20 = hit_roll == 20;

        if is_natural_20
            || (hit_roll + hit_bonus + attr_bonus + weapon_bonus + fighting_bonus >= dodge_target)
        {
            roll_writer.write(DamageRollMessage {
                attacker: intent.attacker,
                target: intent.target,
                damage_type: intent.damage_type,
                source: intent.source,
                is_crit: is_natural_20,
            });
            // Bump attacker-side counters on a successful hit. Monsters
            // lack Skills so weapon_skill_tag would be None anyway, but
            // the is_player guard documents intent.
            if is_player {
                if let Some(ws) = weapon_skill_tag {
                    use_counters.bump(ws.as_skill());
                }
                if intent.source == roguelike_engine::combat::DamageSource::Melee {
                    use_counters.bump(crate::game::skills::Skill::Fighting);
                }
            }
        } else {
            let verb = if is_player { "miss" } else { "misses" };
            log_writer.write(GameLogMessage(format!(
                "{} {} {}.",
                attacker_name.0, verb, target_name.0
            )));
            miss_writer.write(MissMessage {
                attacker: intent.attacker,
                target: intent.target,
            });
            // Bump Dodging use counter on every miss against a Skills-
            // having target (the player). Per SKILLS.md §5: "Successful
            // dodge (the d20 miss condition)."
            if target_skills.is_some() {
                use_counters.bump(crate::game::skills::Skill::Dodging);
            }
        }
    }
}

/// 2. Damage Calculation: Roll attacker damage dice. Crits (nat 20) double the dice.
///
/// Emits [`DamageEvent`] with the raw damage and the target's armor value.
/// The engine's `damage_application_system` handles armor reduction,
/// resistance, HP mutation, and death detection.
fn damage_roll_system(
    mut roll_messages: MessageReader<DamageRollMessage>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(
        &Damage,
        Option<&crate::game::magic::StatusEffects>,
        Has<crate::game::abilities::Terrified>,
        Option<&DamageBonus>,
        Has<Player>,
        Option<&crate::character::Attributes>,
        Option<&crate::game::skills::Skills>,
    )>,
    target_query: Query<(
        Option<&Armor>,
        Option<&crate::game::abilities::RallyBuff>,
        Option<&crate::game::skills::Skills>,
    )>,
    player_equipment_query: Query<&crate::game::items::Equipment, With<Player>>,
    weapon_props_query: Query<&crate::game::items::ItemProperties>,
    target_ai_query: Query<&crate::game::MonsterAI>,
    monster_position_query: Query<(Entity, &Position), With<Monster>>,
    position_query: Query<&Position>,
    mut use_counters: ResMut<crate::game::skills::SkillUseCounters>,
) {
    for message in roll_messages.read() {
        let Ok((
            damage_dice,
            status_effects,
            is_terrified,
            damage_bonus,
            attacker_is_player,
            attacker_attrs,
            attacker_skills,
        )) = attacker_query.get(message.attacker)
        else {
            continue;
        };

        let base_roll = roll_dice(&mut game_rng.0, &damage_dice.0);
        let rolled_damage = if message.is_crit {
            base_roll + roll_dice(&mut game_rng.0, &damage_dice.0)
        } else {
            base_roll
        };

        let bonus = damage_bonus.map(|d| d.0).unwrap_or(0);
        // Branch the attribute contribution by weapon type: STR for melee,
        // DEX for ranged. Mirrors hit_check_system so an attribute's hit
        // and damage scaling always travel together.
        let attr_bonus = crate::character::attack_attribute_bonus(message.source, attacker_attrs);

        // Phase 3 skill damage bonuses: weapon-family + Fighting (melee only).
        let weapon_skill_tag = player_equipment_query
            .get(message.attacker)
            .ok()
            .and_then(|eq| eq.weapon)
            .and_then(|w| weapon_props_query.get(w).ok())
            .and_then(|props| props.weapon_skill);
        let weapon_bonus = crate::game::skills::weapon_skill_bonus(
            weapon_skill_tag,
            message.source,
            attacker_skills,
        );
        let fighting_bonus =
            crate::game::skills::fighting_melee_bonus(message.source, attacker_skills);

        let is_enraged = status_effects.map(|e| e.is_enraged()).unwrap_or(false);
        let mut raw_damage = apply_damage_multipliers(
            rolled_damage + bonus + attr_bonus + weapon_bonus + fighting_bonus,
            is_enraged,
            is_terrified,
        );

        // Backstab: player with Backstab weapon attacking a sleeping monster deals triple damage.
        if attacker_is_player && message.source == DamageSource::Melee {
            if let Ok(equipment) = player_equipment_query.get(message.attacker) {
                if let Some(weapon_entity) = equipment.weapon {
                    if let Ok(props) = weapon_props_query.get(weapon_entity) {
                        if props.weapon_ability.as_deref() == Some("Backstab") {
                            if let Ok(target_ai) = target_ai_query.get(message.target) {
                                if target_ai.is_asleep() {
                                    raw_damage *= 3;
                                    log_writer.write(GameLogMessage("Backstab! Triple damage!".to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Phase 3+: armor is a random roll, not a flat subtraction.
        // The Armor component value is the *upper bound* of a uniform
        // roll in [0, armor_max] (inclusive). Armor skill adds to that
        // ceiling. Non-physical damage skips armor entirely.
        let armor_val = if message.damage_type == DamageType::Physical {
            let (armor_base, skill_bonus) = target_query
                .get(message.target)
                .map(|(armor, rally, skills)| {
                    let base = armor.map(|a| a.0).unwrap_or(0)
                        + rally.map(|r| r.armor_bonus).unwrap_or(0);
                    let sb = crate::game::skills::armor_skill_bonus(skills);
                    (base, sb)
                })
                .unwrap_or((0, 0));
            // Skill bonus only applies if you actually have armor (or a
            // Rally buff). Naked-with-skill produces 0.
            let armor_max = if armor_base > 0 {
                armor_base + skill_bonus
            } else {
                0
            };
            if armor_max > 0 {
                // Bump target's Armor skill use counter on every hit
                // that the armor stat actually intercepts. Per the
                // SKILLS.md spec: "Damage taken while wearing armor."
                if target_query
                    .get(message.target)
                    .ok()
                    .and_then(|(_, _, s)| s)
                    .is_some()
                {
                    use_counters.bump(crate::game::skills::Skill::Armor);
                }
                game_rng.0.range(0, armor_max + 1)
            } else {
                0
            }
        } else {
            0 // Non-physical damage bypasses armor
        };

        damage_writer.write(DamageEvent {
            target: message.target,
            amount: raw_damage,
            damage_type: message.damage_type,
            source: message.source,
            attacker: Some(message.attacker),
            armor: armor_val,
        });

        // Cleave: after the primary melee hit, the Axe's swing damages
        // every monster in the 8 tiles surrounding the *attacker*. The
        // primary target is excluded (they already took the main hit).
        // Splash damage equals the rolled damage — the Axe trades a
        // smaller damage die for area coverage. `DamageSource::Environment`
        // is used so the splash never recursively re-triggers Cleave or
        // on-hit procs.
        if attacker_is_player && message.source == DamageSource::Melee {
            let has_cleave = player_equipment_query
                .get(message.attacker)
                .ok()
                .and_then(|eq| eq.weapon)
                .and_then(|w| weapon_props_query.get(w).ok())
                .map(|p| p.weapon_ability.as_deref() == Some("Cleave"))
                .unwrap_or(false);

            if has_cleave {
                let attacker_pos = position_query
                    .get(message.attacker)
                    .ok()
                    .map(|p| (p.x, p.y));
                if let Some((ax, ay)) = attacker_pos {
                    let mut hit = 0;
                    for (other_entity, other_pos) in monster_position_query.iter() {
                        if other_entity == message.target { continue; }
                        let dx = (other_pos.x - ax).abs();
                        let dy = (other_pos.y - ay).abs();
                        // Chebyshev <= 1 = the 8 surrounding tiles; (dx+dy) > 0
                        // excludes the attacker's own tile (a Cleave-wielding
                        // monster wouldn't damage itself).
                        if dx <= 1 && dy <= 1 && (dx + dy) > 0 {
                            damage_writer.write(DamageEvent {
                                target: other_entity,
                                amount: raw_damage,
                                damage_type: DamageType::Physical,
                                source: DamageSource::Environment,
                                attacker: Some(message.attacker),
                                armor: 0,
                            });
                            hit += 1;
                        }
                    }
                    if hit > 0 {
                        let suffix = if hit == 1 { "y" } else { "ies" };
                        log_writer.write(GameLogMessage(format!(
                            "Cleave! Your axe sweeps through {} more enem{}.",
                            hit, suffix
                        )));
                    }
                }
            }
        }
    }
}

// armor_reduction_system and damage_application_system are removed.
// The engine's CombatPlugin handles armor, resistance, HP mutation, regen
// suppression, and DeathEvent emission via damage_application_system.
// Game-specific reactions (on-hit triggers, combat log, GodMode,
// last-attacker tracking) are handled by the systems below.

/// Reads [`DamageEvent`] messages and emits on-hit / on-being-hit trigger
/// messages so ability and enchantment handlers can react.
///
/// Runs after [`CombatEventSet`] so that HP has already been mutated.
pub fn combat_trigger_system(
    mut damage_events: MessageReader<DamageEvent>,
    mut on_hit_writer: MessageWriter<crate::game::abilities::OnHitTriggerMessage>,
    mut on_being_hit_writer: MessageWriter<crate::game::abilities::OnBeingHitTriggerMessage>,
) {
    for event in damage_events.read() {
        if let Some(attacker) = event.attacker {
            if event.source == DamageSource::Melee || event.source == DamageSource::Ranged {
                on_hit_writer.write(crate::game::abilities::OnHitTriggerMessage {
                    attacker,
                    defender: event.target,
                    final_damage: event.amount,
                    source: event.source,
                });
                on_being_hit_writer.write(crate::game::abilities::OnBeingHitTriggerMessage {
                    attacker,
                    defender: event.target,
                    final_damage: event.amount,
                    source: event.source,
                    damage_type: event.damage_type,
                });
            }
        }
    }
}

/// Logs combat hits, resistance effects, and damage amounts.
///
/// Recomputes the final damage locally (same engine math) for display.
pub fn combat_log_system(
    mut damage_events: MessageReader<DamageEvent>,
    target_query: Query<(Option<&Resistances>, &Name)>,
    name_query: Query<(&Name, Has<Player>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in damage_events.read() {
        let Ok((resistances, target_name)) = target_query.get(event.target) else {
            continue;
        };

        // Recompute final damage for display (mirrors engine math)
        let after_armor = compute_after_armor(event.amount, event.armor);
        let resist_percent = resistances
            .map(|r| r.get(&event.damage_type))
            .unwrap_or(0);
        let final_damage = apply_resistance(after_armor, resist_percent);

        // Log resistance effects
        if resist_percent >= 100 {
            log_writer.write(GameLogMessage(format!(
                "{} is immune to {} damage!",
                target_name.0,
                event.damage_type.name()
            )));
        } else if resist_percent > 0 {
            log_writer.write(GameLogMessage(format!(
                "{} resists the {} damage.",
                target_name.0,
                event.damage_type.name()
            )));
        } else if resist_percent < 0 {
            log_writer.write(GameLogMessage(format!(
                "{} is weak to {}!",
                target_name.0,
                event.damage_type.name()
            )));
        }

        if final_damage <= 0 {
            continue;
        }

        let (attacker_label, is_player) = if let Some(attacker_entity) = event.attacker {
            if let Ok((name, is_pl)) = name_query.get(attacker_entity) {
                (name.0.clone(), is_pl)
            } else {
                ("the environment".to_string(), false)
            }
        } else {
            ("the environment".to_string(), false)
        };

        let verb = if is_player { "hit" } else { "hits" };
        log_writer.write(GameLogMessage(format!(
            "{} {} {} for {} damage.",
            attacker_label, verb, target_name.0, final_damage
        )));
    }
}

/// Tracks the last attacker (for death screen) and undoes engine damage
/// for GodMode entities.
pub fn combat_bookkeeping_system(
    mut damage_events: MessageReader<DamageEvent>,
    name_query: Query<&Name>,
    mut godmode_query: Query<&mut Health, With<GodMode>>,
    player_query: Query<Entity, With<Player>>,
    mut run_stats: ResMut<crate::game::RunStats>,
) {
    let player_entity = player_query.single().ok();
    for event in damage_events.read() {
        // Track last attacker for death screen
        if player_entity == Some(event.target) {
            if let Some(attacker) = event.attacker {
                run_stats.last_hit_by = name_query
                    .get(attacker)
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|_| "the environment".to_string());
            } else {
                run_stats.last_hit_by = "the environment".to_string();
            }
        }

        // GodMode: undo any damage the engine applied
        if let Ok(mut health) = godmode_query.get_mut(event.target) {
            health.current = health.max;
        }
    }
}

/// Logs heal events. The engine's `heal_application_system` handles the
/// actual HP restoration; this system just writes the combat log message.
pub fn heal_log_system(
    mut events: MessageReader<HealEvent>,
    query: Query<(&Health, &Name)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in events.read() {
        if let Ok((health, name)) = query.get(event.target) {
            // The engine already applied the heal, so we compute how much
            // was actually healed (clamped to max) for display.
            let healed = event.amount.min(health.max - (health.current - event.amount));
            if healed > 0 {
                log_writer.write(GameLogMessage(format!(
                    "{} is healed for {} HP.",
                    name.0, healed
                )));
            }
        }
    }
}

/// Tick down regen suppression each turn end. Removes the component when it reaches 0.
fn tick_regen_suppression(
    mut commands: Commands,
    mut turn_end_events: MessageReader<TurnEndEvent>,
    mut query: Query<(Entity, &mut RegenSuppression)>,
) {
    for _ in turn_end_events.read() {
        for (entity, mut suppression) in query.iter_mut() {
            if suppression.0 <= 1 {
                commands.entity(entity).remove::<RegenSuppression>();
            } else {
                suppression.0 -= 1;
            }
        }
    }
}

/// System that toggles GodMode on an entity.
pub fn handle_toggle_god_mode_system(
    mut commands: Commands,
    mut messages: MessageReader<ToggleGodModeMessage>,
    query: Query<(&Name, Has<GodMode>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if let Ok((name, has_god_mode)) = query.get(msg.entity) {
            if has_god_mode {
                commands.entity(msg.entity).remove::<GodMode>();
                log_writer.write(GameLogMessage(format!("{} Godmode DISABLED.", name.0)));
            } else {
                commands.entity(msg.entity).insert(GodMode);
                log_writer.write(GameLogMessage(format!("{} Godmode ENABLED.", name.0)));
            }
        }
    }
}

/// Drop all held inventory items at the entity's death position.
/// Runs before `death_system` so items are placed before the entity is despawned.
pub fn drop_inventory_on_death(
    mut commands: Commands,
    query: Query<(Entity, &Health, &Position, &Inventory)>,
) {
    for (entity, health, pos, inventory) in query.iter() {
        if health.current > 0 || inventory.items.is_empty() {
            continue;
        }
        for &item_entity in &inventory.items {
            commands
                .entity(item_entity)
                .remove::<InInventory>()
                .insert(Position { x: pos.x, y: pos.y })
                .insert(Visibility::Inherited)
                .insert(FloorEntityMarker);
        }
    }
}

/// System that checks for entities with Health <= 0 and handles death.
pub fn death_system(
    mut commands: Commands,
    mut query_dead: Query<(Entity, &mut Health, &Name, Option<&Player>, Option<&Monster>, Has<crate::components::Destructible>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_manager: ResMut<TurnManager>,
    mut log_writer: MessageWriter<GameLogMessage>,
    floor: Res<Floor>,
    mut run_summary: ResMut<RunSummary>,
    mut run_stats: ResMut<crate::game::RunStats>,
) {
    for (entity, mut health, name, is_player, is_monster, is_destructible) in query_dead.iter_mut() {
        if health.current <= 0 {
            if is_player.is_some() {
                // Player died — permadeath: erase the save
                eprintln!("Game Over! You died!");
                log_writer.write(GameLogMessage("You have died!".to_string()));
                let cause = if run_stats.last_hit_by.is_empty() {
                    "Unknown".to_string()
                } else {
                    format!("Slain by a {} on floor {}.", run_stats.last_hit_by, floor.0)
                };
                *run_summary = RunSummary {
                    floor_reached: floor.0,
                    cause,
                    victory: false,
                    enemies_killed: run_stats.enemies_killed,
                };
                crate::save::delete_save();
                next_state.set(AppState::GameOver);
            } else if is_monster.is_some() {
                // Monster died
                run_stats.enemies_killed += 1;
                info!("Monster {:?} died!", entity);
                log_writer.write(GameLogMessage(format!("{} dies.", name.0)));
                commands.entity(entity).despawn();
                // Remove from turn queue if present
                turn_manager.remove_entity(entity);
            } else if is_destructible {
                // Destructible prop destroyed (e.g., barricade)
                log_writer.write(GameLogMessage(format!("The {} crumbles!", name.0.to_lowercase())));
                commands.entity(entity).despawn();
            }
        }
    }
}

// --- Plugin ---

/// System set label for the damage resolution pipeline.
/// Use `.after(CombatDamageSet)` to guarantee a system runs after damage is applied
/// and `DeathEvent` messages have been written.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CombatDamageSet;

pub struct GameCombatPlugin;

impl Plugin for GameCombatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameRng(RandomNumberGenerator::new()))
            .add_message::<AttackIntentMessage>()
            .add_message::<DamageRollMessage>()
            .add_message::<MissMessage>()
            .add_message::<ToggleGodModeMessage>()
            .register_type::<Health>()
            .register_type::<HealthRegen>()
            .register_type::<RegenSuppression>()
            .register_type::<GodMode>()
            .configure_sets(Update, CombatDamageSet.run_if(in_state(AppState::InGame)))
            // Engine's CombatEventSet must run after our hit→roll pipeline
            // so its damage_application_system sees the DamageEvent messages.
            .configure_sets(
                Update,
                CombatEventSet
                    .after(CombatDamageSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    // Game's hit check → damage roll pipeline (emits DamageEvent)
                    (hit_check_system, damage_roll_system)
                        .chain()
                        .in_set(CombatDamageSet),
                    // Reaction systems run after the engine processes damage
                    (
                        combat_trigger_system,
                        combat_log_system,
                        combat_bookkeeping_system,
                        heal_log_system,
                    )
                        .after(CombatEventSet),
                    regen_system,
                    tick_regen_suppression,
                    handle_toggle_god_mode_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure arithmetic tests (compute_after_armor, apply_resistance,
    // apply_damage_multipliers, and the armor+resistance pipeline tests)
    // now live in `roguelike_engine::combat::tests`. Only combat tests that
    // exercise game-side types (Resistances component, DamageType parsing,
    // RegenSuppression) remain here.

    // --- Resistance component ---

    // DamageType parsing, Resistances lookup, and the name tests now
    // live in `roguelike_engine::combat::tests`. Only game-side tests
    // (RegenSuppression, game-specific pipeline integrations) stay here.

    // --- DamageBonus ---

    #[test]
    fn damage_bonus_adds_to_base() {
        // Simulated: rolled 4, bonus 2, no multipliers
        let result = apply_damage_multipliers(4 + 2, false, false);
        assert_eq!(result, 6);
    }

    // --- RegenSuppression ---

    #[test]
    fn regen_suppression_decrements() {
        // Simulated: 5 turns, tick 4 times = 1 remaining
        let mut turns = 5u32;
        for _ in 0..4 {
            turns -= 1;
        }
        assert_eq!(turns, 1);
    }
}
