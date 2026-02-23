use crate::game::{AppState, TurnManager};
use bevy::prelude::*;
use bevy::ui::{
    AlignItems, BackgroundColor, BorderColor, Interaction, JustifyContent, UiRect, Val,
};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), menu_setup)
            .add_systems(Update, menu_action.run_if(in_state(AppState::Menu)))
            .add_systems(OnExit(AppState::Menu), despawn_screen::<OnMainMenuScreen>)
            .add_systems(OnEnter(AppState::GameOver), game_over_setup)
            .add_systems(
                Update,
                game_over_action.run_if(in_state(AppState::GameOver)),
            )
            .add_systems(
                OnExit(AppState::GameOver),
                despawn_screen::<OnGameOverScreen>,
            );
    }
}

// Tag component to mark entities added by the setup_menu system.
#[derive(Component)]
struct OnMainMenuScreen;

// Tag component to mark entities added by the game_over_setup system.
#[derive(Component)]
struct OnGameOverScreen;

fn menu_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::NONE), // No background for the root Node
            OnMainMenuScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    // Spawn button with individual components
                    Button,
                    Node {
                        // Node for button styling
                        width: Val::Px(150.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)), // Background color for the button
                    BorderColor::all(Color::BLACK),                 // Border color for the button
                ))
                .with_children(|parent| {
                    parent.spawn((
                        // Spawn text with individual components
                        Text::new("Play"),
                        TextFont::from_font_size(40.0),
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn menu_action(
    interaction_query: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::InGame);
            return;
        }
    }

    if keyboard_input.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::InGame);
    }
}

fn game_over_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), // Semi-transparent black overlay
            OnGameOverScreen,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("YOU DIED!"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 60.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.0, 0.0)),
            ));

            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(250.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::top(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Return to Menu"),
                        TextFont::from_font_size(30.0),
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn game_over_action(
    interaction_query: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::Menu);
            return;
        }
    }
}

// Generic system that despawns all entities with a given component whenever
// a state is exited
fn despawn_screen<T: Component>(to_despawn: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &to_despawn {
        commands.entity(entity).despawn();
    }
}
