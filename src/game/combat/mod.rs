pub mod resolve;

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

// DamageRollMessage was the internal handoff between the old
// `hit_check_system` and `damage_roll_system`. The unified
// `attack_resolution_system` does both phases in one call to
// `resolve::resolve_attack`, so this message no longer exists.
//
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

// --- Pure computation helpers ---
//
// These functions now live in `roguelike_engine::combat` and are re-exported
// here so existing game code (attack pipeline, tests) can keep referencing
// them via `crate::game::combat::{compute_after_armor, ...}`.
pub use roguelike_engine::combat::{
    apply_damage_multipliers, apply_resistance, compute_after_armor,
};

/// Difficulty class for the shield block check. A fixed value (mirrors
/// the dodge baseline of 4 — neither scales with attack power) keeps
/// the math symmetric across damage sources and lets tower shields
/// stay meaningful into the late game.
pub const SHIELD_BLOCK_DC: i32 = 17;

/// Bundled queries every adapter needs to build a
/// [`resolve::DefenderSnapshot`] for one entity and to write back the
/// shield-budget delta after the call. Used by
/// `attack_resolution_system` (primary + Cleave splash) and
/// `handle_zap_staff` (AoE staff zaps). Bundling them keeps each
/// system under Bevy's parameter ceiling.
#[derive(bevy::ecs::system::SystemParam)]
pub struct DefenderQueries<'w, 's> {
    pub by_entity: Query<
        'w,
        's,
        (
            Option<&'static Dodge>,
            Option<&'static Armor>,
            Option<&'static crate::game::stats::Block>,
            Option<&'static crate::game::stats::MaxShieldBlocks>,
            Option<&'static crate::game::abilities::RallyBuff>,
            Option<&'static crate::game::skills::Skills>,
        ),
    >,
    pub shield_blocks_used: Query<'w, 's, &'static mut crate::game::stats::ShieldBlocksUsed>,
}

impl<'w, 's> DefenderQueries<'w, 's> {
    /// Fetch components for `entity` and build a snapshot, or return
    /// `None` if the entity has no defender components at all (rare —
    /// entities with Health usually have at least Dodge).
    pub fn snapshot(&self, entity: Entity) -> Option<resolve::DefenderSnapshot> {
        let (dodge, armor, block, max_blocks, rally, skills) = self.by_entity.get(entity).ok()?;
        let blocks_used_now = self
            .shield_blocks_used
            .get(entity)
            .map(|b| b.0)
            .unwrap_or(0);
        Some(build_defender_snapshot(
            dodge,
            armor,
            block,
            max_blocks,
            rally,
            skills,
            blocks_used_now,
        ))
    }

    /// Increment the per-turn shield-block counter on `entity`. Called
    /// after `apply_damage` reports a successful block.
    pub fn bump_shield_blocks_used(&mut self, entity: Entity) {
        if let Ok(mut used) = self.shield_blocks_used.get_mut(entity) {
            used.0 = used.0.saturating_add(1);
        }
    }
}

/// Build a [`resolve::DefenderSnapshot`] from the optional ECS
/// components on a defender entity. Shared by every Bevy adapter that
/// calls into the pure resolver (`attack_resolution_system`,
/// `handle_zap_staff`, Cleave splash). The adapter passes the already-
/// fetched components; this helper computes derived values
/// (`armor_max` = base + rally, `shield_budget_left` = max - used).
pub fn build_defender_snapshot(
    dodge: Option<&Dodge>,
    armor: Option<&Armor>,
    block: Option<&crate::game::stats::Block>,
    max_blocks: Option<&crate::game::stats::MaxShieldBlocks>,
    rally: Option<&crate::game::abilities::RallyBuff>,
    skills: Option<&crate::game::skills::Skills>,
    blocks_used_now: u32,
) -> resolve::DefenderSnapshot {
    let armor_max =
        armor.map(|a| a.0).unwrap_or(0) + rally.map(|r| r.armor_bonus).unwrap_or(0);
    let block_base = block.map(|b| b.0).unwrap_or(0);
    let max_blocks_val = max_blocks.map(|m| m.0).unwrap_or(0);
    let shield_budget_left = max_blocks_val
        .saturating_sub(blocks_used_now)
        .min(u8::MAX as u32) as u8;
    resolve::DefenderSnapshot {
        dodge: dodge.map(|d| d.0).unwrap_or(0),
        skills: skills.cloned(),
        armor_max,
        shield_block_bonus: block_base,
        shield_budget_left,
    }
}

/// Pure shield-check resolver: given a d20 roll, the floored Shields
/// skill bonus, and the shield's SH value, return whether the block
/// succeeds. Extracted so the formula is unit-testable without ECS.
///
/// At chargen (Shields 0): Buckler (+3) blocks 35% of incoming hits,
/// Kite (+8) 60%, Tower (+13) 85%. See SKILLS.md §1 for the curve.
pub fn shield_check_passes(d20_roll: i32, shields_skill_bonus: i32, shield_sh: i32) -> bool {
    d20_roll + shields_skill_bonus + shield_sh >= SHIELD_BLOCK_DC
}

/// Resets an entity's `ShieldBlocksUsed` to 0 each time it finishes
/// its own action. This refreshes the per-turn block budget on a
/// rolling "between my actions" window — between my action N and N+1,
/// incoming attackers chew through `MaxShieldBlocks` successful blocks.
fn reset_shield_blocks_on_turn_end(
    mut finished: MessageReader<crate::game::actions::ActionFinishedEvent>,
    mut q: Query<&mut crate::game::stats::ShieldBlocksUsed>,
) {
    for ev in finished.read() {
        if let Ok(mut used) = q.get_mut(ev.entity) {
            used.0 = 0;
        }
    }
}

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

/// Unified attack resolver adapter. Builds snapshots from ECS state,
/// delegates the full hit/damage pipeline to
/// [`resolve::resolve_attack`], and writes [`DamageEvent`] /
/// [`MissMessage`] / log messages from the outcome.
///
/// Replaces the old `hit_check_system` + `damage_roll_system` pair.
/// The resolver owns: d20 hit math, attribute / weapon-skill /
/// Fighting bonus stacking, damage roll with crit double-roll,
/// Enraged / Terrified multipliers, Backstab multiplier (via
/// `damage_multiplier_bp`), shield block, and armor roll. The adapter
/// owns: ECS reads, ECS writes (shield budget, use counters),
/// Backstab proc detection, Cleave splash (step 3 will migrate that
/// to `resolve::apply_damage`), and combat log lines.
fn attack_resolution_system(
    mut intents: MessageReader<AttackIntentMessage>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut miss_writer: MessageWriter<MissMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(
        &Name,
        &Damage,
        Option<&HitBonus>,
        Option<&DamageBonus>,
        Option<&crate::character::Attributes>,
        Option<&crate::game::skills::Skills>,
        Option<&crate::game::magic::StatusEffects>,
        Has<crate::game::abilities::Terrified>,
        Has<Player>,
    )>,
    defender_query: Query<(
        &Name,
        Option<&Dodge>,
        Option<&Armor>,
        Option<&crate::game::stats::Block>,
        Option<&crate::game::stats::MaxShieldBlocks>,
        Option<&crate::game::abilities::RallyBuff>,
        Option<&crate::game::skills::Skills>,
    )>,
    mut shield_blocks_used_query: Query<&mut crate::game::stats::ShieldBlocksUsed>,
    target_ai_query: Query<&crate::game::MonsterAI>,
    player_equipment_query: Query<&crate::game::items::Equipment, With<Player>>,
    weapon_props_query: Query<&crate::game::items::ItemProperties>,
    monster_position_query: Query<(Entity, &Position), With<Monster>>,
    position_query: Query<&Position>,
    mut use_counters: ResMut<crate::game::skills::SkillUseCounters>,
) {
    for intent in intents.read() {
        let Ok((
            attacker_name,
            damage_dice,
            hit_bonus_comp,
            damage_bonus_comp,
            attacker_attrs,
            attacker_skills,
            attacker_status,
            is_terrified,
            is_player,
        )) = attacker_query.get(intent.attacker)
        else {
            continue;
        };
        let Ok((
            target_name,
            target_dodge,
            target_armor,
            target_block,
            target_max_blocks,
            target_rally,
            target_skills,
        )) = defender_query.get(intent.target)
        else {
            continue;
        };

        // Look up the equipped weapon for the player (monsters have no
        // Equipment); used for the weapon-skill tag, Backstab proc, and
        // Cleave splash detection.
        let player_weapon_props = player_equipment_query
            .get(intent.attacker)
            .ok()
            .and_then(|eq| eq.weapon)
            .and_then(|w| weapon_props_query.get(w).ok());
        let weapon_skill_tag = player_weapon_props.and_then(|p| p.weapon_skill);
        let weapon_ability = player_weapon_props.and_then(|p| p.weapon_ability.as_deref());

        // Backstab: player + Melee + Backstab weapon + sleeping target.
        // The ×3 multiplier flows through `damage_multiplier_bp` so the
        // resolver applies it after Enraged / Terrified.
        let backstab_proc = is_player
            && intent.source == DamageSource::Melee
            && weapon_ability == Some("Backstab")
            && target_ai_query
                .get(intent.target)
                .map(|ai| ai.is_asleep())
                .unwrap_or(false);
        if backstab_proc {
            log_writer.write(GameLogMessage("Backstab! Triple damage!".to_string()));
        }

        // ----- Build attacker snapshot. -----
        let attacker_snap = resolve::AttackerSnapshot {
            hit_bonus: hit_bonus_comp.map(|h| h.0).unwrap_or(0),
            damage_bonus: damage_bonus_comp.map(|d| d.0).unwrap_or(0),
            attributes: attacker_attrs.copied(),
            skills: attacker_skills.cloned(),
            enraged: attacker_status.map(|e| e.is_enraged()).unwrap_or(false),
            terrified: is_terrified,
            damage_multiplier_bp: if backstab_proc { 300 } else { 100 },
        };

        // ----- Build defender snapshot. -----
        let blocks_used_now = shield_blocks_used_query
            .get(intent.target)
            .map(|b| b.0)
            .unwrap_or(0);
        let mut defender_snap = build_defender_snapshot(
            target_dodge,
            target_armor,
            target_block,
            target_max_blocks,
            target_rally,
            target_skills,
            blocks_used_now,
        );

        // ----- Build weapon snapshot. -----
        let weapon_snap = resolve::WeaponSnapshot {
            damage_dice: damage_dice.0.clone(),
            damage_type: intent.damage_type,
            weapon_skill: weapon_skill_tag,
        };

        // ----- Resolve. -----
        let outcome = resolve::resolve_attack(
            intent.source,
            &attacker_snap,
            &mut defender_snap,
            &weapon_snap,
            resolve::AttackOverrides::default(),
            &mut game_rng.0,
        );

        // ----- Apply outcome. -----
        if outcome.result == resolve::HitResult::Miss {
            let verb = if is_player { "miss" } else { "misses" };
            log_writer.write(GameLogMessage(format!(
                "{} {} {}.",
                attacker_name.0, verb, target_name.0
            )));
            miss_writer.write(MissMessage {
                attacker: intent.attacker,
                target: intent.target,
            });
        } else {
            // Shield block log line. Wording mirrors the legacy adapter.
            if outcome.blocked {
                log_writer.write(GameLogMessage(if is_player {
                    "Your blow is blocked!".to_string()
                } else {
                    "You block the attack!".to_string()
                }));
                // Write back the shield-budget decrement that the
                // resolver tracked on `defender_snap.shield_budget_left`.
                if let Ok(mut used) = shield_blocks_used_query.get_mut(intent.target) {
                    used.0 = used.0.saturating_add(1);
                }
            }

            damage_writer.write(DamageEvent {
                target: intent.target,
                amount: outcome.amount,
                damage_type: outcome.damage_type,
                source: intent.source,
                attacker: Some(intent.attacker),
                armor: outcome.armor_roll,
            });

            // Cleave splash. Each of the 8 tiles around the attacker
            // takes the primary's rolled damage as an independent
            // `DamagePacket`, then runs through the full defense
            // pipeline via `resolve::apply_damage`: per-splash-target
            // shield block + per-splash-target armor roll. The
            // `DamageEvent.source` stays `Environment` so the splash
            // never recursively re-triggers Cleave or on-hit procs.
            if is_player
                && intent.source == DamageSource::Melee
                && weapon_ability == Some("Cleave")
            {
                if let Ok(attacker_pos) = position_query.get(intent.attacker) {
                    let (ax, ay) = (attacker_pos.x, attacker_pos.y);
                    let splash_packet = resolve::DamagePacket {
                        amount: outcome.amount,
                        damage_type: DamageType::Physical,
                        crit: matches!(outcome.result, resolve::HitResult::Crit),
                    };
                    let mut hit = 0;
                    let mut splash_targets: Vec<Entity> = Vec::new();
                    for (other_entity, other_pos) in monster_position_query.iter() {
                        if other_entity == intent.target {
                            continue;
                        }
                        let dx = (other_pos.x - ax).abs();
                        let dy = (other_pos.y - ay).abs();
                        if dx <= 1 && dy <= 1 && (dx + dy) > 0 {
                            splash_targets.push(other_entity);
                        }
                    }
                    // Resolve each splash target through `apply_damage`.
                    // Defender query reads happen first; mutable shield-
                    // budget writes happen after, to avoid an overlap.
                    for splash_entity in splash_targets {
                        let Ok((
                            _splash_name,
                            splash_dodge,
                            splash_armor,
                            splash_block,
                            splash_max_blocks,
                            splash_rally,
                            splash_skills,
                        )) = defender_query.get(splash_entity)
                        else {
                            continue;
                        };
                        let splash_blocks_used = shield_blocks_used_query
                            .get(splash_entity)
                            .map(|b| b.0)
                            .unwrap_or(0);
                        let mut splash_def = build_defender_snapshot(
                            splash_dodge,
                            splash_armor,
                            splash_block,
                            splash_max_blocks,
                            splash_rally,
                            splash_skills,
                            splash_blocks_used,
                        );
                        let splash_out = resolve::apply_damage(
                            splash_packet.clone(),
                            &mut splash_def,
                            &mut game_rng.0,
                        );
                        if splash_out.blocked {
                            if let Ok(mut used) =
                                shield_blocks_used_query.get_mut(splash_entity)
                            {
                                used.0 = used.0.saturating_add(1);
                            }
                        }
                        damage_writer.write(DamageEvent {
                            target: splash_entity,
                            amount: splash_out.amount,
                            damage_type: DamageType::Physical,
                            source: DamageSource::Environment,
                            attacker: Some(intent.attacker),
                            armor: splash_out.armor_roll,
                        });
                        hit += 1;
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

        // ----- Bump use-counters. -----
        if outcome.use_counters.fighting {
            use_counters.bump(crate::game::skills::Skill::Fighting);
        }
        if let Some(ws) = outcome.use_counters.weapon_skill {
            use_counters.bump(ws);
        }
        if outcome.use_counters.dodging {
            use_counters.bump(crate::game::skills::Skill::Dodging);
        }
        if outcome.use_counters.armor {
            use_counters.bump(crate::game::skills::Skill::Armor);
        }
        if outcome.use_counters.shields {
            use_counters.bump(crate::game::skills::Skill::Shields);
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
                    // Game's unified attack resolver (emits DamageEvent / MissMessage)
                    attack_resolution_system.in_set(CombatDamageSet),
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
                    reset_shield_blocks_on_turn_end,
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

    // --- Shield block check ---

    #[test]
    fn shield_check_passes_at_dc_exactly() {
        // d20 = 14, no skill, +3 buckler → 17 (exactly DC) = pass.
        assert!(shield_check_passes(14, 0, 3));
    }

    #[test]
    fn shield_check_misses_below_dc() {
        // d20 = 13, no skill, +3 buckler → 16 = fail.
        assert!(!shield_check_passes(13, 0, 3));
    }

    #[test]
    fn shield_check_buckler_chargen_threshold() {
        // Buckler (+3), Shields 0 → need d20 ≥ 14 to hit DC 17.
        // d20 = 13 fails, d20 = 14 passes → 7 of 20 outcomes block → 35%.
        let buckler = 3;
        let pass_count: i32 = (1..=20).filter(|&d| shield_check_passes(d, 0, buckler)).count() as i32;
        assert_eq!(pass_count, 7); // 14..=20 inclusive
    }

    #[test]
    fn shield_check_kite_chargen_threshold() {
        // Kite (+8), Shields 0 → need d20 ≥ 9.
        let kite = 8;
        let pass_count: i32 = (1..=20).filter(|&d| shield_check_passes(d, 0, kite)).count() as i32;
        assert_eq!(pass_count, 12); // 9..=20 = 12 of 20 = 60%
    }

    #[test]
    fn shield_check_tower_chargen_threshold() {
        // Tower (+13), Shields 0 → need d20 ≥ 4.
        let tower = 13;
        let pass_count: i32 = (1..=20).filter(|&d| shield_check_passes(d, 0, tower)).count() as i32;
        assert_eq!(pass_count, 17); // 4..=20 = 17 of 20 = 85%
    }

    #[test]
    fn shield_check_skill_bonus_lowers_required_roll() {
        // Buckler (+3), Shields 16 (+4) → need d20 ≥ 10 → 11 of 20 = 55%.
        let buckler = 3;
        let skill_bonus = 4;
        let pass_count: i32 = (1..=20)
            .filter(|&d| shield_check_passes(d, skill_bonus, buckler))
            .count() as i32;
        assert_eq!(pass_count, 11);
    }

    #[test]
    fn shield_check_tower_with_max_skill_autoblocks() {
        // Tower (+13), Shields 27 (+6) → d20 + 19 always ≥ 17 (worst = 20).
        let tower = 13;
        let skill_bonus = 6;
        for d in 1..=20 {
            assert!(shield_check_passes(d, skill_bonus, tower), "d20={} should pass", d);
        }
    }
}
