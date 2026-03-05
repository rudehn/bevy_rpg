use bevy::prelude::*;
use crate::game::combat::{Health, GameRng, DeathEvent};
use crate::game::stats::{CombatStats, Level, RolledHp};
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;
use crate::components::Name;

// --- Components ---

#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct Experience {
    pub current: i32,
    pub next_level: i32,
}

#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct AvailableStatPoints(pub u32);

#[derive(Component, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct ExperienceReward(pub i32);

// --- Messages ---

#[derive(Message)]
pub struct LevelUpEvent {
    pub entity: Entity,
    pub new_level: i32,
}

// --- Systems ---

pub fn xp_award_system(
    mut death_events: MessageReader<DeathEvent>,
    mut player_query: Query<(&mut Experience, &Name), With<Player>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in death_events.read() {
        if let Ok((mut exp, name)) = player_query.get_mut(event.attacker) {
            if event.xp > 0 {
                exp.current += event.xp;
                log_writer.write(GameLogMessage(format!("{} gained {} XP.", name.0, event.xp)));
            }
        }
    }
}

pub fn level_up_check_system(
    mut query: Query<(Entity, &mut Level, &mut Experience, &mut RolledHp, &mut AvailableStatPoints, &CombatStats), (With<Player>, Changed<Experience>)>,
    mut level_up_writer: MessageWriter<LevelUpEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut game_rng: ResMut<GameRng>,
) {
    for (entity, mut level, mut exp, mut rolled_hp, mut points, stats) in query.iter_mut() {
        while exp.current >= exp.next_level {
            level.value += 1;
            exp.current -= exp.next_level;
            exp.next_level = (level.value as f32 * 100.0 * 1.2).round() as i32; // Scaling XP
            points.0 += 1;

            // Roll for HP: 1d4. CON bonus is handled retroactively by stat_recalculation_system.
            let hp_roll = game_rng.0.roll_dice(1, 4);
            rolled_hp.0 += hp_roll;

            level_up_writer.write(LevelUpEvent {
                entity,
                new_level: level.value,
            });

            log_writer.write(GameLogMessage(format!(
                "Welcome to Level {}! HP increased by {} (plus toughness bonus).",
                level.value, hp_roll
            )));
            log_writer.write(GameLogMessage("You have a stat point to allocate!".to_string()));
        }
    }
}

// --- Plugin ---

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LevelUpEvent>()
            .register_type::<Experience>()
            .register_type::<AvailableStatPoints>()
            .register_type::<ExperienceReward>()
            .add_systems(Update, (xp_award_system, level_up_check_system).chain());
    }
}
