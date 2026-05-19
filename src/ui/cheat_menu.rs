//! Debug cheat menu — toggle with Backslash.
//!
//! Registered via the [`UiScreen`] trait so the central dispatcher
//! owns the open/close hotkey + collision detection. While open
//! (`InGameState::CheatMenu`), shortcut keys (R/H/G/O/N) fire the
//! same effects as their on-screen buttons.

use bevy::prelude::*;

use crate::game::combat::{HealEvent, ToggleGodModeMessage};
use crate::game::systems::Omniscient;
use crate::game::InGameState;
use crate::map::dungeon::MapTransitionMessage;
use crate::map::map::RevealMapMessage;
use crate::player::Player;
use crate::ui::registry::{close_on_toggle_or_escape, HelpEntry, UiScreen};

#[derive(Component)]
pub struct CheatMenuRoot;

#[derive(Component)]
pub enum CheatButton {
    RevealMap,
    HealPlayer,
    ToggleGodMode,
    ToggleOmniscient,
    NextLevel,
    Close,
}

pub struct CheatMenuScreen;

impl UiScreen for CheatMenuScreen {
    const STATE: InGameState = InGameState::CheatMenu;
    const OPEN_KEY: Option<KeyCode> = Some(KeyCode::Backslash);
    const HELP: Option<HelpEntry> = Some(HelpEntry {
        display: "\\",
        label: "Cheat menu (debug)",
    });

    fn build(app: &mut App) {
        app.add_systems(OnEnter(Self::STATE), spawn_cheat_menu)
            .add_systems(OnExit(Self::STATE), despawn_cheat_menu)
            .add_systems(
                Update,
                (
                    close_on_toggle_or_escape::<Self>,
                    cheat_shortcut_keys,
                    cheat_menu_button_system,
                )
                    .run_if(in_state(Self::STATE)),
            );
    }
}

fn cheat_shortcut_keys(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    player_query: Query<Entity, With<Player>>,
    mut reveal_writer: MessageWriter<RevealMapMessage>,
    mut heal_writer: MessageWriter<HealEvent>,
    mut god_mode_writer: MessageWriter<ToggleGodModeMessage>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
    floor: Res<crate::map::dungeon::Floor>,
    mut omniscient: ResMut<Omniscient>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    let Ok(player_entity) = player_query.single() else { return };

    if keyboard_input.just_pressed(KeyCode::KeyR) {
        reveal_writer.write(RevealMapMessage);
    }
    if keyboard_input.just_pressed(KeyCode::KeyH) {
        heal_writer.write(HealEvent {
            target: player_entity,
            amount: 999,
            source: None,
        });
    }
    if keyboard_input.just_pressed(KeyCode::KeyG) {
        god_mode_writer.write(ToggleGodModeMessage { entity: player_entity });
    }
    if keyboard_input.just_pressed(KeyCode::KeyN) {
        transition_writer.write(MapTransitionMessage {
            destination_floor: floor.0 + 1,
            destination_pos: None,
        });
        next_state.set(InGameState::Running);
    }
    if keyboard_input.just_pressed(KeyCode::KeyO) {
        omniscient.0 = !omniscient.0;
    }
}

fn spawn_cheat_menu(mut commands: Commands, asset_server: Res<AssetServer>) {
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, crate::ui::modal::MODAL_OVERLAY_OPACITY)),
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
                        TextColor(Color::srgb(1.0, 1.0, 0.0)),
                        Node {
                            margin: UiRect::bottom(Val::Px(20.0)),
                            ..default()
                        },
                    ));

                    spawn_button(parent, &asset_server, "(R)eveal Map", CheatButton::RevealMap);
                    spawn_button(parent, &asset_server, "(H)eal Player", CheatButton::HealPlayer);
                    spawn_button(parent, &asset_server, "(G)odmode Toggle", CheatButton::ToggleGodMode);
                    spawn_button(parent, &asset_server, "(O)mniscient Toggle", CheatButton::ToggleOmniscient);
                    spawn_button(parent, &asset_server, "(N)ext Level", CheatButton::NextLevel);
                    spawn_button(parent, &asset_server, r"Close (\ or ESC)", CheatButton::Close);
                });
        });
}

fn despawn_cheat_menu(mut commands: Commands, query: Query<Entity, With<CheatMenuRoot>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
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

fn cheat_menu_button_system(
    mut interaction_query: Query<
        (&Interaction, &CheatButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    player_query: Query<Entity, With<Player>>,
    mut reveal_writer: MessageWriter<RevealMapMessage>,
    mut heal_writer: MessageWriter<HealEvent>,
    mut god_mode_writer: MessageWriter<ToggleGodModeMessage>,
    mut transition_writer: MessageWriter<MapTransitionMessage>,
    floor: Res<crate::map::dungeon::Floor>,
    mut omniscient: ResMut<Omniscient>,
    mut next_state: ResMut<NextState<InGameState>>,
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
                            heal_writer.write(HealEvent { target: entity, amount: 999, source: None });
                        }
                    }
                    CheatButton::ToggleGodMode => {
                        if let Some(entity) = player_entity {
                            god_mode_writer.write(ToggleGodModeMessage { entity });
                        }
                    }
                    CheatButton::ToggleOmniscient => {
                        omniscient.0 = !omniscient.0;
                    }
                    CheatButton::NextLevel => {
                        transition_writer.write(MapTransitionMessage {
                            destination_floor: floor.0 + 1,
                            destination_pos: None,
                        });
                        next_state.set(InGameState::Running);
                    }
                    CheatButton::Close => {
                        next_state.set(InGameState::Running);
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
