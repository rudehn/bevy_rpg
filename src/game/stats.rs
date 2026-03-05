use crate::components::{Monster, Viewshed};
use crate::game::AppState;
use crate::game::actions::SpeedStats;
use crate::game::combat::{Health, HealthRegen};
use crate::player::Player;
use bevy::prelude::*;

// --- Components ---

#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct Attributes {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub agility: i32,
}

#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct AttributeModifiers {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub agility: i32,
}

#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct Level {
    pub value: i32,
}

#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct CombatStats {
    pub strength_bonus: i32,
    pub dexterity_bonus: i32,
    pub constitution_bonus: i32,
    pub agility_bonus: i32,

    pub damage_bonus: i32,
    pub hit_chance: i32,
    pub dodge_chance: i32,
    pub armor: i32,
}

/// Component to store the sum of 1d4 HP rolls gained on level up.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct RolledHp(pub i32);

/// New component specifically for monsters to store their raw base health
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct MonsterBaseHealth {
    pub value: i32,
}

// --- Systems ---

pub fn stat_recalculation_system(
    mut query: Query<
        (
            &Attributes,
            &AttributeModifiers,
            &Level,
            &mut CombatStats,
            &mut Health,
            &mut Viewshed,
            Option<&mut HealthRegen>,
            Option<&MonsterBaseHealth>,
            Option<&RolledHp>,
            Option<&Player>,
            Option<&Monster>,
        ),
        Or<(
            Changed<Attributes>,
            Changed<AttributeModifiers>,
            Changed<Level>,
        )>,
    >,
) {
    for (
        attr,
        mods,
        level,
        mut stats,
        mut health,
        mut viewshed,
        regen,
        monster_base,
        rolled_hp,
        is_player,
        is_monster,
    ) in query.iter_mut()
    {
        // 1. Calculate Effective Scores
        let eff_str = attr.strength + mods.strength;
        let eff_dex = attr.dexterity + mods.dexterity;
        let eff_con = attr.constitution + mods.constitution;
        let eff_agi = attr.agility + mods.agility;

        // 2. Calculate Bonuses (+1 per 2 points above 10)
        stats.strength_bonus = (eff_str - 10) / 2;
        stats.dexterity_bonus = (eff_dex - 10) / 2;
        stats.constitution_bonus = (eff_con - 10) / 2;
        stats.agility_bonus = (eff_agi - 10) / 2;

        // 3. Update Health
        let old_max = health.max;
        if is_player.is_some() {
            // Player HP = 10 (base) + rolled_hp (sum of 1d4s) + (CON bonus * Level)
            let roll_sum = rolled_hp.map(|r| r.0).unwrap_or(0);
            health.max = 10 + roll_sum + (stats.constitution_bonus * level.value);
        } else if is_monster.is_some() {
            // Monster HP = base + (constitution_bonus * level)
            let base = monster_base.map(|b| b.value).unwrap_or(10);
            health.max = base + (stats.constitution_bonus * level.value);
        }

        // Adjust current health if max increased (simple level-up/gear heal)
        if health.max > old_max {
            health.current += health.max - old_max;
        }
        health.current = health.current.min(health.max);

        // 4. Update Secondary Combat Values
        stats.damage_bonus = stats.strength_bonus;
        stats.hit_chance = 10 + stats.strength_bonus; // Example: Base 10 + STR
        stats.dodge_chance = 5 + stats.dexterity_bonus; // Example: Base 5 + DEX
        // stats.armor = stats.constitution_bonus; // Example: CON provides armor

        // 5. Update Viewshed/Vision (based on DEX/Quickness)
        // Ensure range doesn't drop below 1
        // viewshed.range = (8 + stats.dexterity_bonus).max(1);

        // 6. Update Regeneration (based on CON bonus)
        if let Some(mut r) = regen {
            // e.g., 10 points per turn base + 5 per CON bonus
            // At 20 points, gain 1 HP every 5 turns.
            r.regen_rate = (20 + (stats.constitution_bonus * 5)).max(0);
        }
    }
}

/// Helper system to sync CombatStats.agility_bonus to ActionStats delay
pub fn sync_action_speed_system(
    mut query: Query<(&CombatStats, &mut SpeedStats), Changed<CombatStats>>,
) {
    for (stats, mut actions) in query.iter_mut() {
        // High AGI bonus = Low Delay (Faster)
        // 1.0 is default. Each point of AGI bonus reduces delay by 5%
        let multiplier = 1.0 - (stats.agility_bonus as f32 * 0.05);
        actions.delay = multiplier.clamp(0.5, 2.0);
    }
}

// --- Plugin ---

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Attributes>()
            .register_type::<AttributeModifiers>()
            .register_type::<CombatStats>()
            .register_type::<Level>()
            .register_type::<MonsterBaseHealth>()
            .add_systems(
                Update,
                (
                    stat_recalculation_system,
                    sync_action_speed_system.after(stat_recalculation_system),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
