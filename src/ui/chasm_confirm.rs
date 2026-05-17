//! Confirmation dialog for falling into a chasm.
//! Enter = fall (2d6 damage + floor transition), Esc = cancel.

use bevy::prelude::*;

use crate::game::InGameState;
use crate::game::actions::PendingChasmFall;
use crate::game::combat::{DamageEvent, DamageSource, DamageType};
use crate::map::dungeon::MapTransitionMessage;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;
use crate::ui::modal::{despawn_screen, spawn_modal, ModalConfig, GOLD};
use crate::ui::registry::UiScreen;

/// Registry entry for the chasm-fall confirmation dialog. Event-driven:
/// entered when the player steps on a chasm tile (gameplay code sets
/// `NextState<InGameState::ChasmConfirm>`); has no hotkey.
pub struct ChasmConfirmScreen;

impl UiScreen for ChasmConfirmScreen {
    const STATE: InGameState = InGameState::ChasmConfirm;
    const OPEN_KEY: Option<KeyCode> = None;
    // HELP is None — event-driven screens are not user-discoverable.

    fn build(app: &mut App) {
        app.add_systems(OnEnter(Self::STATE), spawn_chasm_confirm_ui)
            .add_systems(
                Update,
                chasm_confirm_input_system.run_if(in_state(Self::STATE)),
            )
            .add_systems(
                OnExit(Self::STATE),
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
    player_query: Query<Entity, With<Player>>,
    floor: Res<crate::map::dungeon::Floor>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
    mut damage_writer: MessageWriter<DamageEvent>,
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

        // Emit damage through the combat pipeline (environmental — bypasses armor)
        if let Ok(player_entity) = player_query.single() {
            damage_writer.write(DamageEvent {
                attacker: None,
                target: player_entity,
                amount: damage,
                damage_type: DamageType::Physical,
                source: DamageSource::Environment,
                armor: 0,
            });
        }

        // Trigger floor descent — the damage system handles death if HP <= 0.
        transition_writer.write(MapTransitionMessage {
            destination_floor: floor.0 + 1,
            destination_pos: None,
        });
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
