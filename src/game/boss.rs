use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::FinalBoss,
    game::{
        abilities::{BaseArmor, ThornAura},
        combat::{DamageType, Health, Resistances, ResistanceLevel},
        magic::{ActiveSpells, KnownSpells},
        stats::{Attributes, MonsterBaseHealth},
        turns::TurnEndEvent,
        TurnManager,
    },
    ui::game_log::GameLogMessage,
};

// --- Constants ---

/// Time units between Tyrant power tier increases.
/// Each player action costs BASE_ACTION_COST (100), so 100_000 = ~1000 player turns.
pub const ESCALATION_INTERVAL: u32 = 100_000;

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

// --- Resources ---

/// Tracks the Tyrant's power tier from the hunger clock.
/// Each tier adds new abilities and stat boosts.
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct TyrantPower {
    pub tier: u32,
    pub last_escalation_time: u32,
}

// --- Plugin ---

pub struct BossPlugin;

impl Plugin for BossPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TyrantPower>()
            .add_systems(
                Update,
                (
                    apply_tyrant_power_on_spawn,
                    tyrant_escalation_system,
                    boss_phase_system,
                )
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

// --- Systems ---

/// When a FinalBoss entity is first spawned, apply all accumulated TyrantPower
/// tier boosts. Modifies MonsterBaseHealth and Attributes so that the existing
/// stat_recalculation_system handles HP/mana properly.
fn apply_tyrant_power_on_spawn(
    tyrant_power: Res<TyrantPower>,
    mut query: Query<
        (
            Entity,
            &mut MonsterBaseHealth,
            &mut Attributes,
            Option<&mut Resistances>,
            Option<&mut KnownSpells>,
            Option<&mut ActiveSpells>,
        ),
        Added<FinalBoss>,
    >,
    mut commands: Commands,
) {
    for (entity, mut base_hp, mut attrs, resistances, mut known_spells, mut active_spells) in
        query.iter_mut()
    {
        if tyrant_power.tier == 0 {
            return;
        }

        let mut cumulative_hp = 0;
        let mut cumulative_armor_bonus = 0;
        let mut has_thorn = false;
        let mut spells_to_add: Vec<&str> = Vec::new();
        let mut resistance_map: std::collections::HashMap<DamageType, ResistanceLevel> =
            std::collections::HashMap::new();

        for tier in 1..=tyrant_power.tier {
            // +1 INT per tier
            attrs.intelligence += 1;

            // HP boost
            cumulative_hp += if tier == 5 { 20 } else { 15 };

            match tier {
                1 => {
                    has_thorn = true;
                }
                2 => {
                    spells_to_add.push("fireball");
                }
                3 => {
                    resistance_map.insert(DamageType::Physical, ResistanceLevel::Resistant);
                }
                4 => {
                    spells_to_add.push("haste");
                }
                5 => {
                    cumulative_armor_bonus = 2;
                    resistance_map.insert(DamageType::Necrotic, ResistanceLevel::Resistant);
                }
                t if t >= 6 => {
                    cumulative_armor_bonus = 2 + (t - 5) as i32;
                }
                _ => {}
            }
        }

        // Apply HP boost to base health (stat recalc will compute final max)
        base_hp.value += cumulative_hp;

        // Apply thorn aura
        if has_thorn {
            commands.entity(entity).insert(ThornAura { damage: 3 });
        }

        // Apply armor boost
        if cumulative_armor_bonus > 0 {
            // Base armor from monsters.ron is 3, add the tier bonus
            commands
                .entity(entity)
                .insert(BaseArmor(3 + cumulative_armor_bonus));
        }

        // Apply resistances
        if !resistance_map.is_empty() {
            if let Some(mut res) = resistances {
                for (dt, rl) in &resistance_map {
                    res.0.insert(*dt, *rl);
                }
            } else {
                commands.entity(entity).insert(Resistances(resistance_map));
            }
        }

        // Apply spells
        for spell_id in &spells_to_add {
            if let Some(ref mut known) = known_spells {
                if !known.spells.contains(&spell_id.to_string()) {
                    known.spells.push(spell_id.to_string());
                }
            }
            if let Some(ref mut active) = active_spells {
                for slot in active.slots.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(spell_id.to_string());
                        break;
                    }
                }
            }
        }

        info!(
            "Applied TyrantPower tier {} to The Veiled Tyrant: +{} base HP, +{} INT, +{} armor",
            tyrant_power.tier,
            cumulative_hp,
            tyrant_power.tier,
            cumulative_armor_bonus,
        );
    }
}

/// Checks the global game time and increments the Tyrant's power tier
/// when the escalation interval is crossed. Logs warnings to the player.
fn tyrant_escalation_system(
    mut turn_end_events: MessageReader<TurnEndEvent>,
    turn_manager: Res<TurnManager>,
    mut tyrant_power: ResMut<TyrantPower>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut boss_query: Query<
        (
            Entity,
            &mut Health,
            &mut Attributes,
            Option<&mut Resistances>,
            Option<&mut KnownSpells>,
            Option<&mut ActiveSpells>,
        ),
        With<FinalBoss>,
    >,
    mut commands: Commands,
) {
    for _ in turn_end_events.read() {
        let current_time = turn_manager.current_time;
        let next_threshold = tyrant_power.last_escalation_time + ESCALATION_INTERVAL;

        if current_time < next_threshold {
            continue;
        }

        // Increment tier
        tyrant_power.tier += 1;
        tyrant_power.last_escalation_time = current_time;
        let tier = tyrant_power.tier;

        // Log escalating warnings
        let warning = match tier {
            1 => "You feel a dark power growing in the depths below...",
            2 => "The dungeon trembles. The Tyrant grows stronger.",
            3 => "A cold dread washes over you. The Tyrant's might swells.",
            4 => "The air crackles with malice. The Tyrant draws upon dark forces.",
            5 => "Reality shudders. The Tyrant has become a force of nature.",
            _ => "The Tyrant's power continues to grow without bound...",
        };
        log_writer.write(GameLogMessage(warning.to_string()));

        // If the boss is already spawned on the current floor, apply the single tier now
        if let Ok((entity, mut health, mut attrs, resistances, known_spells, active_spells)) =
            boss_query.single_mut()
        {
            // +1 INT
            attrs.intelligence += 1;

            // HP boost (direct, since stat recalc already ran)
            let hp_boost = if tier == 5 { 20 } else { 15 };
            health.max += hp_boost;
            health.current += hp_boost;

            match tier {
                1 => {
                    commands.entity(entity).insert(ThornAura { damage: 3 });
                }
                2 => {
                    add_spell("fireball", known_spells, active_spells);
                }
                3 => {
                    if let Some(mut res) = resistances {
                        res.0.insert(DamageType::Physical, ResistanceLevel::Resistant);
                    } else {
                        let mut map = std::collections::HashMap::new();
                        map.insert(DamageType::Physical, ResistanceLevel::Resistant);
                        commands.entity(entity).insert(Resistances(map));
                    }
                }
                4 => {
                    add_spell("haste", known_spells, active_spells);
                }
                5 => {
                    commands.entity(entity).insert(BaseArmor(5));
                    if let Some(mut res) = resistances {
                        res.0.insert(DamageType::Necrotic, ResistanceLevel::Resistant);
                    } else {
                        let mut map = std::collections::HashMap::new();
                        map.insert(DamageType::Necrotic, ResistanceLevel::Resistant);
                        commands.entity(entity).insert(Resistances(map));
                    }
                }
                t if t >= 6 => {
                    commands
                        .entity(entity)
                        .insert(BaseArmor(3 + 2 + (t - 5) as i32));
                }
                _ => {}
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

// --- Helpers ---

fn add_spell(
    spell_id: &str,
    known_spells: Option<Mut<KnownSpells>>,
    active_spells: Option<Mut<ActiveSpells>>,
) {
    if let Some(mut known) = known_spells {
        if !known.spells.contains(&spell_id.to_string()) {
            known.spells.push(spell_id.to_string());
        }
    }
    if let Some(mut active) = active_spells {
        for slot in active.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(spell_id.to_string());
                return;
            }
        }
    }
}
