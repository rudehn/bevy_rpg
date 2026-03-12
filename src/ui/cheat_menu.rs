use crate::game::AppState;
use crate::game::combat::{HealMessage, ToggleGodModeMessage};
use crate::map::dungeon::MapTransitionMessage;
use crate::map::map::RevealMapMessage;
use crate::player::Player;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct CheatMenu {
    pub is_open: bool,
}

#[derive(Component)]
pub struct CheatMenuRoot;

#[derive(Component)]
pub enum CheatButton {
    RevealMap,
    HealPlayer,
    ToggleGodMode,
    NextLevel,
    Close,
}

pub fn toggle_cheat_menu_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut cheat_menu: ResMut<CheatMenu>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    root_query: Query<Entity, With<CheatMenuRoot>>,
    player_query: Query<Entity, With<Player>>,
    mut reveal_writer: MessageWriter<RevealMapMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut god_mode_writer: MessageWriter<ToggleGodModeMessage>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
) {
    // Open/Close toggle with Backslash
    if keyboard_input.just_pressed(KeyCode::Backslash) {
        cheat_menu.is_open = !cheat_menu.is_open;
        if cheat_menu.is_open {
            spawn_cheat_menu(&mut commands, &asset_server);
        } else {
            for entity in root_query.iter() {
                commands.entity(entity).despawn();
            }
        }
    }

    // Close only with Escape
    if keyboard_input.just_pressed(KeyCode::Escape) && cheat_menu.is_open {
        cheat_menu.is_open = false;
        for entity in root_query.iter() {
            commands.entity(entity).despawn();
        }
    }

    // Shortcut keys when open
    if cheat_menu.is_open {
        if let Ok(player_entity) = player_query.single() {
            if keyboard_input.just_pressed(KeyCode::KeyR) {
                reveal_writer.write(RevealMapMessage);
            }
            if keyboard_input.just_pressed(KeyCode::KeyH) {
                heal_writer.write(HealMessage {
                    entity: player_entity,
                    amount: 999,
                });
            }
            if keyboard_input.just_pressed(KeyCode::KeyG) {
                god_mode_writer.write(ToggleGodModeMessage {
                    entity: player_entity,
                });
            }
            if keyboard_input.just_pressed(KeyCode::KeyN) {
                transition_writer.write(MapTransitionMessage);
            }
        }
    }
}

fn spawn_cheat_menu(commands: &mut Commands, asset_server: &Res<AssetServer>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            ZIndex(200),
            CheatMenuRoot,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(350.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(20.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("CHEAT MENU"),
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 32.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 1.0, 0.0)), // Yellow
                        Node {
                            margin: UiRect::bottom(Val::Px(20.0)),
                            ..default()
                        },
                    ));

                    spawn_button(parent, asset_server, "(R)eveal Map", CheatButton::RevealMap);
                    spawn_button(
                        parent,
                        asset_server,
                        "(H)eal Player",
                        CheatButton::HealPlayer,
                    );
                    spawn_button(
                        parent,
                        asset_server,
                        "(G)odmode Toggle",
                        CheatButton::ToggleGodMode,
                    );
                    spawn_button(parent, asset_server, "(N)ext Level", CheatButton::NextLevel);
                    spawn_button(
                        parent,
                        asset_server,
                        r"Close (\ or ESC)",
                        CheatButton::Close,
                    );
                });
        });
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    asset_server: &Res<AssetServer>,
    label: &str,
    cheat: CheatButton,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(40.0),
                margin: UiRect::vertical(Val::Px(5.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
            cheat,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(label),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

pub fn cheat_menu_button_system(
    mut interaction_query: Query<
        (&Interaction, &CheatButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut cheat_menu: ResMut<CheatMenu>,
    mut commands: Commands,
    root_query: Query<Entity, With<CheatMenuRoot>>,
    player_query: Query<Entity, With<Player>>,
    mut reveal_writer: MessageWriter<RevealMapMessage>,
    mut heal_writer: MessageWriter<HealMessage>,
    mut god_mode_writer: MessageWriter<ToggleGodModeMessage>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
) {
    for (interaction, cheat_button, mut color) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                *color = BackgroundColor(Color::srgb(0.35, 0.35, 0.35));
                let player_entity = player_query.single().ok();

                match cheat_button {
                    CheatButton::RevealMap => {
                        reveal_writer.write(RevealMapMessage);
                    }
                    CheatButton::HealPlayer => {
                        if let Some(entity) = player_entity {
                            heal_writer.write(HealMessage {
                                entity,
                                amount: 999,
                            });
                        }
                    }
                    CheatButton::ToggleGodMode => {
                        if let Some(entity) = player_entity {
                            god_mode_writer.write(ToggleGodModeMessage { entity });
                        }
                    }
                    CheatButton::NextLevel => {
                        transition_writer.write(MapTransitionMessage);
                        cheat_menu.is_open = false;
                        for entity in root_query.iter() {
                            commands.entity(entity).despawn();
                        }
                    }
                    CheatButton::Close => {
                        cheat_menu.is_open = false;
                        for entity in root_query.iter() {
                            commands.entity(entity).despawn();
                        }
                    }
                }
            }
            Interaction::Hovered => {
                *color = BackgroundColor(Color::srgb(0.25, 0.25, 0.25));
            }
            Interaction::None => {
                *color = BackgroundColor(Color::srgb(0.2, 0.2, 0.2));
            }
        }
    }
}

pub struct CheatMenuPlugin;

impl Plugin for CheatMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CheatMenu>().add_systems(
            Update,
            (toggle_cheat_menu_system, cheat_menu_button_system).run_if(in_state(AppState::InGame)),
        );
    }
}
