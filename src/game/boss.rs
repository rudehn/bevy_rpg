use bevy::prelude::*;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::{
    components::FinalBoss,
    game::{
        abilities::{BurningStrike, Knockback, RoughBody, StunningBlow},
        combat::{DamageType, Health, HealthRegen, Resistances},
        magic::{ActiveSpells, KnownSpells, MAX_SPELL_SLOTS},
        stats::{Armor, DamageBonus},
        turns::TurnEndEvent,
        TurnManager,
    },
    ui::game_log::GameLogMessage,
};

// --- Constants ---

/// Time thresholds for aspect stage advancement (game ticks).
const STAGE_1_TIME: u32 = 12_500; // ~125 player turns
const STAGE_2_TIME: u32 = 30_000; // ~300 turns
const STAGE_3_TIME: u32 = 50_000; // ~500 turns

/// Beyond Stage 3, the Tyrant gains +15 HP and +1 armor per tick of this interval.
const BEYOND_INTERVAL: u32 = 25_000;

// --- Components ---

/// Boss AI component that tracks the current fight phase.
/// Phase transitions are driven by HP thresholds.
#[derive(Component, Debug)]
pub struct BossAI {
    pub phase: u8, // 1, 2, or 3
}

impl Default for BossAI {
    fn default() -> Self {
        Self { phase: 1 }
    }
}

// --- Aspect types ---

/// The four possible Tyrant aspects. Each run selects 3 of 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectKind {
    Flame,
    Iron,
    Blood,
    Storm,
}

impl AspectKind {
    pub const ALL: [AspectKind; 4] = [
        AspectKind::Flame,
        AspectKind::Iron,
        AspectKind::Blood,
        AspectKind::Storm,
    ];
}

/// Tracks the growth stage of a single aspect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AspectState {
    pub kind: AspectKind,
    pub stage: u32, // 0 = dormant, 1-3 = active stages
}

// --- Resources ---

/// Tracks which 3 aspects the Tyrant has and their current growth stages.
/// Replaces the old tier-based TyrantPower system.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct TyrantAspects {
    pub aspects: Vec<AspectState>, // Always 3 once initialized
    pub beyond_ticks: u32,         // How many beyond-Stage-3 ticks have occurred
}

impl Default for TyrantAspects {
    fn default() -> Self {
        Self {
            aspects: Vec::new(),
            beyond_ticks: 0,
        }
    }
}

impl TyrantAspects {
    /// Pick 3 random aspects from the pool of 4.
    pub fn new_random() -> Self {
        let mut rng = rand::rng();
        let mut pool = AspectKind::ALL.to_vec();
        pool.shuffle(&mut rng);
        pool.truncate(3);

        Self {
            aspects: pool
                .into_iter()
                .map(|kind| AspectState { kind, stage: 0 })
                .collect(),
            beyond_ticks: 0,
        }
    }

    /// Returns true if all 3 aspects have been initialized.
    pub fn is_initialized(&self) -> bool {
        self.aspects.len() == 3
    }

    /// Returns true if the given aspect kind is present.
    pub fn has_aspect(&self, kind: AspectKind) -> bool {
        self.aspects.iter().any(|a| a.kind == kind)
    }

    /// Returns the stage of a given aspect kind, or None if not present.
    pub fn stage_of(&self, kind: AspectKind) -> Option<u32> {
        self.aspects.iter().find(|a| a.kind == kind).map(|a| a.stage)
    }
}

// --- Plugin ---

pub struct BossPlugin;

impl Plugin for BossPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TyrantAspects>()
            .add_systems(
                Update,
                (
                    apply_tyrant_aspects_on_spawn,
                    tyrant_escalation_system,
                    boss_phase_system,
                )
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

// --- Systems ---

/// When a FinalBoss entity is first spawned, apply all accumulated aspect
/// abilities based on each aspect's current stage.
fn apply_tyrant_aspects_on_spawn(
    tyrant_aspects: Res<TyrantAspects>,
    mut query: Query<
        (
            Entity,
            &mut Health,
            &mut Armor,
            Option<&mut Resistances>,
            Option<&mut HealthRegen>,
            Option<&mut KnownSpells>,
            Option<&mut ActiveSpells>,
            Option<&mut DamageBonus>,
        ),
        Added<FinalBoss>,
    >,
    mut commands: Commands,
) {
    for (entity, mut health, mut armor, resistances, mut health_regen, known_spells, active_spells, mut damage_bonus) in
        query.iter_mut()
    {
        if !tyrant_aspects.is_initialized() {
            return;
        }

        // Apply beyond-stage-3 bonuses first
        if tyrant_aspects.beyond_ticks > 0 {
            let hp_boost = tyrant_aspects.beyond_ticks as i32 * 15;
            let armor_boost = tyrant_aspects.beyond_ticks as i32;
            health.max += hp_boost;
            health.current += hp_boost;
            armor.0 += armor_boost;
            info!(
                "Applied {} beyond-ticks to Tyrant: +{} HP, +{} armor",
                tyrant_aspects.beyond_ticks, hp_boost, armor_boost,
            );
        }

        // Collect spells to add across all aspects
        let mut spells_to_add: Vec<String> = Vec::new();
        let mut resistance_map = resistances
            .map(|r| r.0.clone())
            .unwrap_or_default();
        let mut needs_resistances = false;

        for aspect in &tyrant_aspects.aspects {
            if aspect.stage == 0 {
                continue;
            }

            match aspect.kind {
                AspectKind::Flame => {
                    apply_flame_aspect(
                        aspect.stage,
                        entity,
                        &mut spells_to_add,
                        &mut resistance_map,
                        &mut needs_resistances,
                        &mut commands,
                    );
                }
                AspectKind::Iron => {
                    apply_iron_aspect(
                        aspect.stage,
                        entity,
                        &mut armor,
                        &mut resistance_map,
                        &mut needs_resistances,
                        &mut commands,
                    );
                }
                AspectKind::Blood => {
                    apply_blood_aspect(
                        aspect.stage,
                        &mut health,
                        health_regen.as_deref_mut(),
                        damage_bonus.as_deref_mut(),
                        entity,
                        &mut commands,
                    );
                }
                AspectKind::Storm => {
                    apply_storm_aspect(
                        aspect.stage,
                        entity,
                        &mut spells_to_add,
                        &mut commands,
                    );
                }
            }

            info!(
                "Applied {:?} aspect stage {} to The Veiled Tyrant",
                aspect.kind, aspect.stage,
            );
        }

        // Apply collected resistances
        if needs_resistances {
            commands.entity(entity).insert(Resistances(resistance_map));
        }

        // Apply collected spells
        if !spells_to_add.is_empty() {
            // Get mutable references or create new components
            if let Some(mut known) = known_spells {
                for spell_id in &spells_to_add {
                    if !known.spells.contains(spell_id) {
                        known.spells.push(spell_id.clone());
                    }
                }
                if let Some(mut active) = active_spells {
                    for spell_id in &spells_to_add {
                        // Find an empty slot
                        if active.slots.iter().any(|s| s.as_deref() == Some(spell_id.as_str())) {
                            continue;
                        }
                        if let Some(empty_slot) = active.slots.iter_mut().find(|s| s.is_none()) {
                            *empty_slot = Some(spell_id.clone());
                        }
                    }
                }
            } else {
                // Boss didn't have KnownSpells yet — create fresh
                let mut known = KnownSpells { spells: spells_to_add.clone() };
                let mut active = ActiveSpells {
                    slots: vec![None; MAX_SPELL_SLOTS],
                };
                for (i, spell_id) in spells_to_add.iter().enumerate() {
                    if i < MAX_SPELL_SLOTS {
                        active.slots[i] = Some(spell_id.clone());
                    }
                    known.spells.push(spell_id.clone());
                }
                commands.entity(entity).insert(known).insert(active);
            }
        }
    }
}

fn apply_flame_aspect(
    stage: u32,
    entity: Entity,
    spells: &mut Vec<String>,
    resistance_map: &mut std::collections::HashMap<DamageType, i32>,
    needs_resistances: &mut bool,
    commands: &mut Commands,
) {
    // Stage 1+: fire_dart spell
    spells.push("fire_dart".to_string());

    if stage >= 2 {
        // Stage 2+: fireball spell, 50% fire resistance
        spells.push("fireball".to_string());
        resistance_map.insert(DamageType::Fire, 50);
        *needs_resistances = true;
    }

    if stage >= 3 {
        // Stage 3: fire immune (100%), BurningStrike on melee
        resistance_map.insert(DamageType::Fire, 100);
        *needs_resistances = true;
        commands.entity(entity).insert(BurningStrike {
            damage_per_turn: 3,
            duration: 3,
            chance: 40,
        });
    }
}

fn apply_iron_aspect(
    stage: u32,
    entity: Entity,
    armor: &mut Armor,
    resistance_map: &mut std::collections::HashMap<DamageType, i32>,
    needs_resistances: &mut bool,
    commands: &mut Commands,
) {
    // Stage 1: +2 armor
    // Stage 2: +4 total armor, RoughBody 2
    // Stage 3: +6 total armor, RoughBody 3, 50% physical resistance
    let armor_bonus = match stage {
        1 => 2,
        2 => 4,
        _ => 6, // 3+
    };
    armor.0 += armor_bonus;

    if stage >= 2 {
        let rough_damage = if stage >= 3 { 3 } else { 2 };
        commands.entity(entity).insert(RoughBody {
            damage: rough_damage,
        });
    }

    if stage >= 3 {
        resistance_map.insert(DamageType::Physical, 50);
        *needs_resistances = true;
    }
}

fn apply_blood_aspect(
    stage: u32,
    health: &mut Health,
    health_regen: Option<&mut HealthRegen>,
    damage_bonus: Option<&mut DamageBonus>,
    entity: Entity,
    commands: &mut Commands,
) {
    // Stage 1: +15 HP, regen_rate 30
    // Stage 2: +30 HP, regen_rate 60, DamageBonus(3)
    // Stage 3: +45 HP, regen_rate 80, DamageBonus(6)
    let hp_bonus = match stage {
        1 => 15,
        2 => 30,
        _ => 45, // 3+
    };
    health.max += hp_bonus;
    health.current += hp_bonus;

    let regen_rate = match stage {
        1 => 30,
        2 => 60,
        _ => 80, // 3+
    };
    if let Some(regen) = health_regen {
        regen.regen_rate = regen_rate;
    } else {
        commands.entity(entity).insert(HealthRegen {
            regen_rate,
            regen_accumulator: 0,
        });
    }

    if stage >= 2 {
        let bonus = if stage >= 3 { 6 } else { 3 };
        if let Some(db) = damage_bonus {
            db.0 += bonus;
        } else {
            commands.entity(entity).insert(DamageBonus(bonus));
        }
    }
}

fn apply_storm_aspect(
    stage: u32,
    entity: Entity,
    spells: &mut Vec<String>,
    commands: &mut Commands,
) {
    // Stage 1: spark spell
    spells.push("spark".to_string());

    if stage >= 2 {
        // Stage 2: lightning_bolt spell (chain_lightning not in spells.ron), StunningBlow 15%
        spells.push("lightning_bolt".to_string());
        commands.entity(entity).insert(StunningBlow {
            duration: 2,
            chance: 15,
        });
    }

    if stage >= 3 {
        // Stage 3: StunningBlow 30%, Knockback 2
        commands.entity(entity).insert(StunningBlow {
            duration: 2,
            chance: 30,
        });
        commands.entity(entity).insert(Knockback {
            distance: 2,
            chance: 100,
        });
    }
}

/// Checks the global game time against stage thresholds and advances all
/// aspect stages together. Logs atmospheric warnings to the player.
/// If the boss is already spawned, applies beyond-tick bonuses immediately.
fn tyrant_escalation_system(
    mut turn_end_events: MessageReader<TurnEndEvent>,
    turn_manager: Res<TurnManager>,
    mut tyrant_aspects: ResMut<TyrantAspects>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut boss_query: Query<(&mut Health, &mut Armor), With<FinalBoss>>,
) {
    for _ in turn_end_events.read() {
        if !tyrant_aspects.is_initialized() {
            continue;
        }

        let current_time = turn_manager.current_time;

        // Check stage advancement (all aspects advance together)
        let mut advanced_to: Option<u32> = None;

        // Stage 1
        if current_time >= STAGE_1_TIME
            && tyrant_aspects.aspects.iter().any(|a| a.stage == 0)
        {
            for aspect in &mut tyrant_aspects.aspects {
                if aspect.stage == 0 {
                    aspect.stage = 1;
                }
            }
            advanced_to = Some(1);
        }

        // Stage 2
        if current_time >= STAGE_2_TIME
            && tyrant_aspects.aspects.iter().any(|a| a.stage == 1)
        {
            for aspect in &mut tyrant_aspects.aspects {
                if aspect.stage == 1 {
                    aspect.stage = 2;
                }
            }
            advanced_to = Some(2);
        }

        // Stage 3
        if current_time >= STAGE_3_TIME
            && tyrant_aspects.aspects.iter().any(|a| a.stage == 2)
        {
            for aspect in &mut tyrant_aspects.aspects {
                if aspect.stage == 2 {
                    aspect.stage = 3;
                }
            }
            advanced_to = Some(3);
        }

        // Whisper messages on stage advancement
        if let Some(stage) = advanced_to {
            let warning = match stage {
                1 => "You feel a dark power stirring in the depths below...",
                2 => "The dungeon trembles. The Tyrant grows stronger.",
                3 => "Reality shudders. The Tyrant has become a force of nature.",
                _ => unreachable!(),
            };
            log_writer.write(GameLogMessage(warning.to_string()));
        }

        // Beyond Stage 3: check if all aspects are at stage 3
        if tyrant_aspects.aspects.iter().all(|a| a.stage >= 3) && current_time >= STAGE_3_TIME {
            let elapsed_beyond = current_time - STAGE_3_TIME;
            let expected_ticks = elapsed_beyond / BEYOND_INTERVAL;
            if expected_ticks > tyrant_aspects.beyond_ticks {
                let new_ticks = expected_ticks - tyrant_aspects.beyond_ticks;
                tyrant_aspects.beyond_ticks = expected_ticks;

                log_writer.write(GameLogMessage(
                    "The Tyrant's power continues to grow without bound...".to_string(),
                ));

                // If the boss is already spawned, apply beyond bonuses immediately
                if let Ok((mut health, mut armor)) = boss_query.single_mut() {
                    let hp_boost = new_ticks as i32 * 15;
                    let armor_boost = new_ticks as i32;
                    health.max += hp_boost;
                    health.current += hp_boost;
                    armor.0 += armor_boost;
                }
            }
        }
    }
}

/// Updates the boss's phase based on current HP thresholds.
fn boss_phase_system(mut query: Query<(&Health, &mut BossAI), With<FinalBoss>>) {
    for (health, mut boss_ai) in query.iter_mut() {
        let hp_pct = health.current as f32 / health.max.max(1) as f32;
        let new_phase = if hp_pct > 0.6 {
            1
        } else if hp_pct > 0.3 {
            2
        } else {
            3
        };

        if new_phase != boss_ai.phase {
            boss_ai.phase = new_phase;
        }
    }
}
