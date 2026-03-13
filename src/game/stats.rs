use crate::components::{Monster, Viewshed};
use crate::game::AppState;
use crate::game::abilities::BaseArmor;
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
    pub intelligence: i32,
    pub perception: i32,
}

#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct AttributeModifiers {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub agility: i32,
    pub intelligence: i32,
    pub perception: i32,
}

/// Mana pool — derived from Intelligence (max = INT × 5). Updated by stat_recalculation_system.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct Mana {
    pub current: i32,
    pub max: i32,
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
    pub intelligence_bonus: i32,
    pub perception_bonus: i32,

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
            &mut Viewshed,  // perception drives vision range
            Option<&mut HealthRegen>,
            Option<&mut Mana>,
            Option<&MonsterBaseHealth>,
            Option<&RolledHp>,
            Option<&Player>,
            Option<&Monster>,
            Option<&BaseArmor>,
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
        mana,
        monster_base,
        rolled_hp,
        is_player,
        is_monster,
        base_armor,
    ) in query.iter_mut()
    {
        // 1. Calculate Effective Scores
        let eff_str = attr.strength + mods.strength;
        let eff_dex = attr.dexterity + mods.dexterity;
        let eff_con = attr.constitution + mods.constitution;
        let eff_agi = attr.agility + mods.agility;
        let eff_int = attr.intelligence + mods.intelligence;
        let eff_per = attr.perception + mods.perception;

        // 2. Calculate Bonuses (+1 per point above 10 — every point is immediately impactful)
        stats.strength_bonus = eff_str - 10;
        stats.dexterity_bonus = eff_dex - 10;
        stats.constitution_bonus = eff_con - 10;
        stats.agility_bonus = eff_agi - 10;
        stats.intelligence_bonus = eff_int - 10;
        stats.perception_bonus = eff_per - 10;

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
        stats.hit_chance = 10 + stats.strength_bonus;
        stats.dodge_chance = 5 + stats.dexterity_bonus;
        // Base armor from monster definition (TimedModifiers "armor" adds on top via recalc_attribute_modifiers)
        stats.armor = base_armor.map(|a| a.0).unwrap_or(0);

        // 5. Update Mana (max = INT × 5)
        if let Some(mut m) = mana {
            let new_max = eff_int * 5;
            if new_max > m.max {
                m.current += new_max - m.max;
            }
            m.max = new_max;
            m.current = m.current.min(m.max).max(0);
        }

        // 6. Update Regeneration (based on CON bonus)
        if let Some(mut r) = regen {
            r.regen_rate = (20 + (stats.constitution_bonus * 5)).max(0);
        }

        // 7. Update Vision Range (PER drives Viewshed — base 8 tiles + PER bonus)
        viewshed.range = (8 + stats.perception_bonus).max(2);
        viewshed.dirty = true;
    }
}

/// Helper system to sync CombatStats.agility_bonus to ActionStats delay
pub fn sync_action_speed_system(
    mut query: Query<(&CombatStats, &mut SpeedStats), Changed<CombatStats>>,
) {
    for (stats, mut actions) in query.iter_mut() {
        // High AGI bonus = Low Delay (Faster)
        // 1.0 is default. Each point of AGI bonus (above 10) reduces delay by 2.5%
        let multiplier = 1.0 - (stats.agility_bonus as f32 * 0.025);
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
            .register_type::<Mana>()
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
