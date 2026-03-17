use crate::game::AppState;
use crate::game::InGameState;
use crate::game::actions::SpeedStats;
use crate::game::combat::{Damage, Health};
use crate::game::essence::Essence;
use crate::game::stats::{Armor, Dodge, Mana};
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
                despawn_character_info_ui,
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
    if keys.just_pressed(KeyCode::KeyC) {
        match state.get() {
            InGameState::Running => next_state.set(InGameState::CharacterInfo),
            InGameState::CharacterInfo => next_state.set(InGameState::Running),
            _ => {} // C does nothing while another screen is open
        }
    }

    if keys.just_pressed(KeyCode::Escape) && *state.get() == InGameState::CharacterInfo {
        next_state.set(InGameState::Running);
    }
}

fn spawn_character_info_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player_query: Query<
        (
            &Health,
            &Mana,
            &Damage,
            &Armor,
            &Dodge,
            &Essence,
            Option<&SpeedStats>,
        ),
        With<Player>,
    >,
) {
    let Ok((health, mana, damage, armor, dodge, essence, speed_stats)) = player_query.single() else {
        return;
    };

    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    let speed_str = speed_stats
        .map(|s| format!("{:.2}x", s.delay))
        .unwrap_or_else(|| "1.00x".to_string());

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
            ZIndex(200),
            OnCharacterInfoScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(400.0),
                        padding: UiRect::all(Val::Px(20.0)),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    // Header
                    parent.spawn((
                        Text::new("CHARACTER INFO"),
                        TextFont {
                            font: font.clone(),
                            font_size: 32.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.84, 0.0)),
                    ));

                    parent.spawn(Node {
                        height: Val::Px(10.0),
                        ..default()
                    });

                    // Stats
                    let stats_text = format!(
                        "HP:       {}/{}\nMana:     {}/{}\nDamage:   {}\nArmor:    {}\nDodge:    {}\nSpeed:    {}\nEssence:  {} (lifetime: {})",
                        health.current, health.max,
                        mana.current, mana.max,
                        damage.0,
                        armor.0,
                        dodge.0,
                        speed_str,
                        essence.current, essence.lifetime,
                    );

                    parent.spawn((
                        Text::new(stats_text),
                        TextFont {
                            font: font.clone(),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    // Footer
                    parent.spawn(Node {
                        height: Val::Px(20.0),
                        ..default()
                    });
                    parent.spawn((
                        Text::new("Press (C) to Close"),
                        TextFont {
                            font: font.clone(),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.5, 0.5, 0.5)),
                    ));
                });
        });
}

fn despawn_character_info_ui(
    mut commands: Commands,
    query: Query<Entity, With<OnCharacterInfoScreen>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
}
