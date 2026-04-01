use crate::game::AppState;
use crate::game::InGameState;
use crate::game::actions::SpeedStats;
use crate::game::combat::{Damage, Health};
use crate::game::stats::{Armor, Dodge};
use crate::player::Player;
use bevy::prelude::*;

pub struct CharacterInfoPlugin;

impl Plugin for CharacterInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
                Update,
                character_info_input_system.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                OnEnter(InGameState::CharacterInfo),
                spawn_character_info_ui,
            )
            .add_systems(
                OnExit(InGameState::CharacterInfo),
                crate::ui::modal::despawn_screen::<OnCharacterInfoScreen>,
            );
    }
}

#[derive(Component)]
pub struct OnCharacterInfoScreen;

fn character_info_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    crate::ui::modal::toggle_screen(&keys, &state, &mut next_state, KeyCode::KeyC, InGameState::CharacterInfo);
}

fn spawn_character_info_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player_query: Query<
        (
            &Health,
            &Damage,
            &Armor,
            &Dodge,
            Option<&SpeedStats>,
        ),
        With<Player>,
    >,
) {
    let Ok((health, damage, armor, dodge, speed_stats)) = player_query.single() else {
        return;
    };

    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    let move_speed_str = speed_stats
        .map(|s| format!("{:.2}x", s.movement_delay))
        .unwrap_or_else(|| "1.00x".to_string());
    let atk_speed_str = speed_stats
        .map(|s| format!("{:.2}x", s.attack_delay))
        .unwrap_or_else(|| "1.00x".to_string());

    let stats_text = format!(
        "HP:       {}/{}\nDamage:   {}\nArmor:    {}\nDodge:    {}\nMove Spd: {}\nAtk Spd:  {}",
        health.current, health.max,
        damage.0,
        armor.0,
        dodge.0,
        move_speed_str,
        atk_speed_str,
    );

    use crate::ui::modal::{spawn_modal, ModalConfig};
    spawn_modal(&mut commands, OnCharacterInfoScreen, &font, &ModalConfig {
        title: "CHARACTER INFO",
        title_color: Color::srgb(1.0, 0.84, 0.0),
        footer: "Press (C) to Close",
        width: 400.0,
        height: 520.0,
        opacity: 0.8,
    }, |panel, font| {
        panel.spawn((
            Text::new(stats_text),
            TextFont { font: font.clone(), font_size: 18.0, ..default() },
            TextColor(Color::WHITE),
        ));
    });
}

// Despawn handled by modal::despawn_screen::<OnCharacterInfoScreen>
