use bevy::prelude::*;
use crate::game::AppState;
use crate::player::Player;
use crate::game::combat::{Health, Damage};
use crate::game::stats::{Attributes, CombatStats, Level};
use crate::game::InGameState;

pub struct CharacterInfoPlugin;

impl Plugin for CharacterInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hotkey_bar)
            .add_systems(Update, character_info_input_system.run_if(in_state(AppState::InGame)))
            .add_systems(OnEnter(InGameState::CharacterInfo), spawn_character_info_ui)
            .add_systems(OnExit(InGameState::CharacterInfo), despawn_character_info_ui);
    }
}

#[derive(Component)]
pub struct OnCharacterInfoScreen;

#[derive(Component)]
pub struct HotkeyBar;

fn spawn_hotkey_bar(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Px(30.0),
                bottom: Val::Px(150.0), // Above the game log
                left: Val::Px(0.0),
                padding: UiRect::left(Val::Px(10.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            HotkeyBar,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("(C)haracter Info"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn character_info_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    if keys.just_pressed(KeyCode::KeyC) {
        match state.get() {
            InGameState::Running => next_state.set(InGameState::CharacterInfo),
            InGameState::CharacterInfo => next_state.set(InGameState::Running),
        }
    }

    if keys.just_pressed(KeyCode::Escape) && *state.get() == InGameState::CharacterInfo {
        next_state.set(InGameState::Running);
    }
}

fn spawn_character_info_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player_query: Query<(&Health, &Attributes, &CombatStats, &Level, &Damage), With<Player>>,
) {
    let Ok((health, attrs, stats, level, damage)) = player_query.single() else {
        return;
    };

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
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 32.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.84, 0.0).into()), // Gold
                    ));

                    // Level & Health
                    parent.spawn((
                        Text::new(format!("Level: {} | HP: {}/{}", level.value, health.current, health.max)),
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn(Node {
                        height: Val::Px(10.0),
                        ..default()
                    });

                    // Attributes Section
                    parent.spawn((
                        Text::new("ATTRIBUTES"),
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.7, 0.7, 1.0).into()),
                    ));

                    let attr_style = (
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    );

                    parent.spawn((Text::new(format!("Strength:     {}", attrs.strength)), attr_style.0.clone(), attr_style.1.clone()));
                    parent.spawn((Text::new(format!("Dexterity:    {}", attrs.dexterity)), attr_style.0.clone(), attr_style.1.clone()));
                    parent.spawn((Text::new(format!("Constitution: {}", attrs.constitution)), attr_style.0.clone(), attr_style.1.clone()));
                    parent.spawn((Text::new(format!("Agility:      {}", attrs.agility)), attr_style.0.clone(), attr_style.1.clone()));

                    parent.spawn(Node {
                        height: Val::Px(10.0),
                        ..default()
                    });

                    // Combat Stats Section
                    parent.spawn((
                        Text::new("COMBAT STATS"),
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.7, 0.7).into()),
                    ));

                    parent.spawn((Text::new(format!("Damage:       {}", damage.0)), attr_style.0.clone(), attr_style.1.clone()));
                    parent.spawn((Text::new(format!("Hit Chance:   {}", stats.hit_chance)), attr_style.0.clone(), attr_style.1.clone()));
                    parent.spawn((Text::new(format!("Dodge Chance: {}", stats.dodge_chance)), attr_style.0.clone(), attr_style.1.clone()));
                    parent.spawn((Text::new(format!("Armor:        {}", stats.armor)), attr_style.0.clone(), attr_style.1.clone()));

                    // Footer
                    parent.spawn(Node {
                        height: Val::Px(20.0),
                        ..default()
                    });
                    parent.spawn((
                        Text::new("Press (C) to Close"),
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.5, 0.5, 0.5).into()), // Gray
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
