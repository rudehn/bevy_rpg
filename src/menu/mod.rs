use bevy::prelude::*;

use crate::game::AppState;
use crate::map::dungeon::PendingGameLoad;
use crate::save::{GameSaveData, SaveExists};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), menu_setup)
            .add_systems(Update, menu_action.run_if(in_state(AppState::Menu)))
            .add_systems(OnExit(AppState::Menu), despawn_screen::<OnMainMenuScreen>)
            .add_systems(OnEnter(AppState::GameOver), game_over_setup)
            .add_systems(Update, game_over_action.run_if(in_state(AppState::GameOver)))
            .add_systems(OnExit(AppState::GameOver), despawn_screen::<OnGameOverScreen>)
            .add_systems(OnEnter(AppState::Victory), victory_setup)
            .add_systems(Update, victory_action.run_if(in_state(AppState::Victory)))
            .add_systems(OnExit(AppState::Victory), despawn_screen::<OnVictoryScreen>);
    }
}

// ---- Marker components ----

#[derive(Component)]
pub struct OnMainMenuScreen;

#[derive(Component)]
pub struct OnGameOverScreen;

#[derive(Component)]
pub struct OnVictoryScreen;

// ---- Button tags ----

#[derive(Component)]
enum MenuButton {
    NewGame,
    Continue,
    Quit,
    ReturnToMenu,
}

// ---- Styling constants ----

const BTN_NORMAL: Color = Color::srgb(0.08, 0.08, 0.08);
const BTN_HOVER: Color = Color::srgb(0.18, 0.18, 0.18);
const BTN_PRESSED: Color = Color::srgb(0.05, 0.05, 0.05);
const GOLD: Color = Color::srgb(1.0, 0.84, 0.0);
const DIM: Color = Color::srgb(0.35, 0.35, 0.35);

fn button_style(width: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(54.0),
        border: UiRect::all(Val::Px(1.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        margin: UiRect::top(Val::Px(14.0)),
        ..default()
    }
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    font: Handle<Font>,
    label: &str,
    tag: MenuButton,
    enabled: bool,
) {
    let text_color = if enabled { Color::WHITE } else { DIM };
    let border_color = if enabled { Color::srgb(0.4, 0.4, 0.4) } else { DIM };

    parent
        .spawn((
            Button,
            button_style(260.0),
            BackgroundColor(BTN_NORMAL),
            BorderColor::all(border_color),
            tag,
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont { font, font_size: 22.0, ..default() },
                TextColor(text_color),
            ));
        });
}

// ---- Main menu ----

fn menu_setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    save_exists: Res<SaveExists>,
) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");
    let has_save = save_exists.0;

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.04, 0.04, 0.04)),
            OnMainMenuScreen,
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("IRONVEIL"),
                TextFont { font: font.clone(), font_size: 80.0, ..default() },
                TextColor(GOLD),
            ));

            // Subtitle
            root.spawn((
                Text::new("A dungeon lies beneath. The amulet awaits."),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(Color::srgb(0.55, 0.55, 0.55)),
                Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
            ));

            // Spacer
            root.spawn(Node { height: Val::Px(48.0), ..default() });

            // Buttons
            spawn_button(root, font.clone(), "New Game", MenuButton::NewGame, true);
            spawn_button(root, font.clone(), "Continue", MenuButton::Continue, has_save);
            spawn_button(root, font.clone(), "Quit", MenuButton::Quit, true);

            // Version hint
            root.spawn((
                Text::new("[ Enter ] New Game"),
                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.3, 0.3, 0.3)),
                Node { margin: UiRect::top(Val::Px(40.0)), ..default() },
            ));
        });
}

fn menu_action(
    mut interaction_query: Query<
        (&Interaction, &MenuButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    mut pending_game_load: ResMut<PendingGameLoad>,
    save_exists: Res<SaveExists>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    // Keyboard shortcut: Enter → new game
    if keyboard.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::InGame);
        return;
    }

    for (interaction, button, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Hovered => *bg = BackgroundColor(BTN_HOVER),
            Interaction::None => *bg = BackgroundColor(BTN_NORMAL),
            Interaction::Pressed => {
                *bg = BackgroundColor(BTN_PRESSED);
                match button {
                    MenuButton::NewGame => {
                        pending_game_load.0 = None;
                        next_state.set(AppState::InGame);
                    }
                    MenuButton::Continue => {
                        if save_exists.0 {
                            match load_save_file() {
                                Some(data) => {
                                    pending_game_load.0 = Some(Box::new(data));
                                    next_state.set(AppState::InGame);
                                }
                                None => {
                                    warn!("Save file found but failed to load.");
                                }
                            }
                        }
                    }
                    MenuButton::Quit => {
                        app_exit.write(AppExit::Success);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn load_save_file() -> Option<GameSaveData> {
    let path = crate::save::save_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| warn!("Could not read save file: {}", e))
        .ok()?;
    ron::from_str::<GameSaveData>(&text)
        .map_err(|e| warn!("Could not parse save file: {}", e))
        .ok()
}

// ---- Game Over ----

fn game_over_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.88)),
            OnGameOverScreen,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("YOU DIED"),
                TextFont { font: font.clone(), font_size: 72.0, ..default() },
                TextColor(Color::srgb(0.85, 0.1, 0.1)),
            ));
            root.spawn((
                Text::new("Your legend ends here."),
                TextFont { font: font.clone(), font_size: 20.0, ..default() },
                TextColor(Color::srgb(0.5, 0.5, 0.5)),
                Node { margin: UiRect::top(Val::Px(12.0)), ..default() },
            ));

            root.spawn(Node { height: Val::Px(40.0), ..default() });

            root.spawn((
                Button,
                button_style(280.0),
                BackgroundColor(BTN_NORMAL),
                BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
                MenuButton::ReturnToMenu,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Return to Menu"),
                    TextFont { font: font.clone(), font_size: 22.0, ..default() },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn game_over_action(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::Menu);
        return;
    }
    for (interaction, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Hovered => *bg = BackgroundColor(BTN_HOVER),
            Interaction::None => *bg = BackgroundColor(BTN_NORMAL),
            Interaction::Pressed => next_state.set(AppState::Menu),
        }
    }
}

// ---- Victory ----

fn victory_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.88)),
            OnVictoryScreen,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("VICTORY"),
                TextFont { font: font.clone(), font_size: 72.0, ..default() },
                TextColor(GOLD),
            ));
            root.spawn((
                Text::new("The Amulet of Bevy is yours."),
                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                TextColor(Color::WHITE),
                Node { margin: UiRect::top(Val::Px(12.0)), ..default() },
            ));
            root.spawn((
                Text::new("Ironveil will remember your name."),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                Node { margin: UiRect::top(Val::Px(6.0)), ..default() },
            ));

            root.spawn(Node { height: Val::Px(40.0), ..default() });

            root.spawn((
                Button,
                button_style(280.0),
                BackgroundColor(BTN_NORMAL),
                BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
                MenuButton::ReturnToMenu,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Return to Menu"),
                    TextFont { font: font.clone(), font_size: 22.0, ..default() },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn victory_action(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::Menu);
        return;
    }
    for (interaction, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Hovered => *bg = BackgroundColor(BTN_HOVER),
            Interaction::None => *bg = BackgroundColor(BTN_NORMAL),
            Interaction::Pressed => next_state.set(AppState::Menu),
        }
    }
}

// ---- Helper ----

fn despawn_screen<T: Component>(query: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
