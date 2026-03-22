use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::FinalBoss,
    game::{
        combat::{DamageType, Health, Resistances},
        stats::Armor,
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
/// tier boosts. Modifies Health and Armor directly.
/// When a FinalBoss entity is first spawned, apply all accumulated TyrantPower
/// tier boosts. Each tier adds +15 HP. Higher tiers add armor and resistances.
fn apply_tyrant_power_on_spawn(
    tyrant_power: Res<TyrantPower>,
    mut query: Query<(Entity, &mut Health, &mut Armor, Option<&mut Resistances>), Added<FinalBoss>>,
    mut commands: Commands,
) {
    for (entity, mut health, mut armor, resistances) in query.iter_mut() {
        if tyrant_power.tier == 0 {
            return;
        }

        // Each tier: +15 HP. Tier 3+: physical resistant. Tier 5+: +2 armor, necrotic resistant.
        let hp_boost = tyrant_power.tier as i32 * 15;
        health.max += hp_boost;
        health.current += hp_boost;

        if tyrant_power.tier >= 5 {
            armor.0 += 2;
        }

        if tyrant_power.tier >= 3 {
            let mut map = resistances
                .map(|r| r.0.clone())
                .unwrap_or_default();
            map.insert(DamageType::Physical, 50);
            if tyrant_power.tier >= 5 {
                map.insert(DamageType::Necrotic, 50);
            }
            commands.entity(entity).insert(Resistances(map));
        }

        info!(
            "Applied TyrantPower tier {} to The Veiled Tyrant: +{} HP, +{} armor",
            tyrant_power.tier, hp_boost, if tyrant_power.tier >= 5 { 2 } else { 0 },
        );
    }
}

/// Checks the global game time and increments the Tyrant's power tier
/// when the escalation interval is crossed. Logs warnings to the player.
/// Checks the global game time and increments the Tyrant's power tier
/// when the escalation interval is crossed. Logs warnings to the player.
/// If the boss is already spawned, applies the stat boost immediately.
fn tyrant_escalation_system(
    mut turn_end_events: MessageReader<TurnEndEvent>,
    turn_manager: Res<TurnManager>,
    mut tyrant_power: ResMut<TyrantPower>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut boss_query: Query<(&mut Health, &mut Armor), With<FinalBoss>>,
) {
    for _ in turn_end_events.read() {
        let current_time = turn_manager.current_time;
        let next_threshold = tyrant_power.last_escalation_time + ESCALATION_INTERVAL;

        if current_time < next_threshold {
            continue;
        }

        tyrant_power.tier += 1;
        tyrant_power.last_escalation_time = current_time;
        let tier = tyrant_power.tier;

        let warning = match tier {
            1 => "You feel a dark power growing in the depths below...",
            2 => "The dungeon trembles. The Tyrant grows stronger.",
            3 => "A cold dread washes over you. The Tyrant's might swells.",
            4 => "The air crackles with malice. The Tyrant draws upon dark forces.",
            5 => "Reality shudders. The Tyrant has become a force of nature.",
            _ => "The Tyrant's power continues to grow without bound...",
        };
        log_writer.write(GameLogMessage(warning.to_string()));

        // If the boss is already spawned, apply single-tier boost now
        if let Ok((mut health, mut armor)) = boss_query.single_mut() {
            health.max += 15;
            health.current += 15;
            if tier >= 5 {
                armor.0 += 1;
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

