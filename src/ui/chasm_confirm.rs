//! Confirmation dialog for falling into a chasm.
//! Enter = fall (2d6 damage + floor transition), Esc = cancel.

use bevy::prelude::*;

use crate::game::{AppState, InGameState};
use crate::game::actions::PendingChasmFall;
use crate::game::combat::Health;
use crate::map::dungeon::MapTransitionMessage;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;
use crate::ui::modal::{despawn_screen, spawn_modal, ModalConfig, GOLD};

pub struct ChasmConfirmPlugin;

impl Plugin for ChasmConfirmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(InGameState::ChasmConfirm), spawn_chasm_confirm_ui)
            .add_systems(
                Update,
                chasm_confirm_input_system
                    .run_if(in_state(AppState::InGame).and(in_state(InGameState::ChasmConfirm))),
            )
            .add_systems(
                OnExit(InGameState::ChasmConfirm),
                despawn_screen::<OnChasmConfirmScreen>,
            );
    }
}

#[derive(Component)]
struct OnChasmConfirmScreen;

fn spawn_chasm_confirm_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/SourceCodePro.ttf");

    spawn_modal(
        &mut commands,
        OnChasmConfirmScreen,
        &font,
        &ModalConfig {
            title: "CHASM",
            title_color: GOLD,
            width: 420.0,
            height: 200.0,
            footer: "Enter - Fall  |  Esc - Cancel",
            ..default()
        },
        |panel, font| {
            panel.spawn((
                Text::new(
                    "A gaping chasm yawns before you.\nFall to the next floor? (2d6 damage)",
                ),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));
        },
    );
}

/// Roll 2d6 using bracket-lib's RNG and return the sum.
fn roll_2d6() -> i32 {
    let mut rng = bracket_lib::random::RandomNumberGenerator::new();
    rng.range(1, 7) + rng.range(1, 7)
}

fn chasm_confirm_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<InGameState>>,
    mut pending_fall: ResMut<PendingChasmFall>,
    mut player_query: Query<&mut Health, With<Player>>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        pending_fall.0 = None;
        next_state.set(InGameState::Running);
        return;
    }

    if keys.just_pressed(KeyCode::Enter) {
        let _target_pos = pending_fall.0.take();

        // Roll 2d6 fall damage
        let damage = roll_2d6();
        log_writer.write(GameLogMessage(format!(
            "You fall into the chasm! ({} damage)",
            damage
        )));

        // Apply damage directly (environmental — bypasses armor)
        if let Ok(mut health) = player_query.single_mut() {
            health.current -= damage;

            if health.current <= 0 {
                // Let the death system handle it — just transition to Running
                // so the normal game loop picks up the dead player.
                next_state.set(InGameState::Running);
                return;
            }
        }

        // Player survived — trigger floor descent
        transition_writer.write(MapTransitionMessage);
        next_state.set(InGameState::Running);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_2d6_in_valid_range() {
        // 2d6 should produce values in [2, 12]
        for _ in 0..100 {
            let result = roll_2d6();
            assert!(result >= 2 && result <= 12, "roll_2d6 returned {}", result);
        }
    }
}
