use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use crate::game::combat::DeathEvent;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Essence {
    pub current: i32,
    pub lifetime: i32,
}

pub fn essence_award_system(
    mut death_events: MessageReader<DeathEvent>,
    mut player_query: Query<&mut Essence, With<Player>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for event in death_events.read() {
        if let Ok(mut essence) = player_query.get_mut(event.attacker) {
            let amount = event.xp;
            if amount > 0 {
                essence.current += amount;
                essence.lifetime += amount;
                log_writer.write(GameLogMessage(format!("Gained {} Essence.", amount)));
            }
        }
    }
}
