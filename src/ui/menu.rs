use bevy::prelude::*;

use crate::game::{AppState, RunSummary};
use crate::map::dungeon::PendingGameLoad;
use crate::save::{GameSaveData, SaveExists};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuSelection>()
            .add_systems(OnEnter(AppState::Menu), menu_setup)
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

/// Tracks which menu button is highlighted by keyboard navigation.
#[derive(Resource, Default)]
struct MenuSelection(usize);

/// Index tag on menu buttons for keyboard selection matching.
#[derive(Component)]
struct MenuButtonIndex(usize);

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
    index: usize,
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
            MenuButtonIndex(index),
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
                Text::new("THE VEILED TYRANT"),
                TextFont { font: font.clone(), font_size: 80.0, ..default() },
                TextColor(GOLD),
            ));

            // Subtitle
            root.spawn((
                Text::new("A dungeon lies beneath. The Tyrant awaits."),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(Color::srgb(0.55, 0.55, 0.55)),
                Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
            ));

            // Spacer
            root.spawn(Node { height: Val::Px(48.0), ..default() });

            // Buttons
            spawn_button(root, font.clone(), "New Game", MenuButton::NewGame, true, 0);
            spawn_button(root, font.clone(), "Continue", MenuButton::Continue, has_save, 1);
            spawn_button(root, font.clone(), "Quit", MenuButton::Quit, true, 2);

            // Version hint
            root.spawn((
                Text::new("↑/↓ Navigate  |  Enter - Select"),
                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.3, 0.3, 0.3)),
                Node { margin: UiRect::top(Val::Px(40.0)), ..default() },
            ));
        });
}

fn menu_action(
    mut button_query: Query<
        (&Interaction, &MenuButton, &MenuButtonIndex, &mut BackgroundColor),
        With<Button>,
    >,
    mut next_state: ResMut<NextState<AppState>>,
    mut pending_game_load: ResMut<PendingGameLoad>,
    save_exists: Res<SaveExists>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut app_exit: MessageWriter<AppExit>,
    mut selection: ResMut<MenuSelection>,
) {
    let button_count = 3;

    // Keyboard navigation
    if keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyK) {
        if selection.0 > 0 {
            selection.0 -= 1;
        }
    }
    if keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyJ) {
        if selection.0 + 1 < button_count {
            selection.0 += 1;
        }
    }

    // Enter activates the selected button
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        match selection.0 {
            0 => {
                pending_game_load.0 = None;
                next_state.set(AppState::CharacterCreation);
            }
            1 => {
                if save_exists.0 {
                    match load_save_file() {
                        Some(data) => {
                            pending_game_load.0 = Some(Box::new(data));
                            next_state.set(AppState::InGame);
                        }
                        None => { warn!("Save file found but failed to load."); }
                    }
                }
            }
            2 => { app_exit.write(AppExit::Success); }
            _ => {}
        }
        return;
    }

    // Update button colors: keyboard selection OR mouse hover
    for (interaction, _button, idx, mut bg) in &mut button_query {
        let is_selected = idx.0 == selection.0;
        match *interaction {
            Interaction::Hovered => {
                *bg = BackgroundColor(BTN_HOVER);
                // Sync keyboard selection to mouse hover
                selection.0 = idx.0;
            }
            Interaction::Pressed => {
                *bg = BackgroundColor(BTN_PRESSED);
                // Activate on click
                match idx.0 {
                    0 => {
                        pending_game_load.0 = None;
                        next_state.set(AppState::CharacterCreation);
                    }
                    1 => {
                        if save_exists.0 {
                            if let Some(data) = load_save_file() {
                                pending_game_load.0 = Some(Box::new(data));
                                next_state.set(AppState::InGame);
                            }
                        }
                    }
                    2 => { app_exit.write(AppExit::Success); }
                    _ => {}
                }
            }
            Interaction::None => {
                *bg = BackgroundColor(if is_selected { BTN_HOVER } else { BTN_NORMAL });
            }
        }
    }
}

fn load_save_file() -> Option<GameSaveData> {
    let text = crate::save::read_save_data()
        .or_else(|| { warn!("No save data found."); None })?;
    ron::from_str::<GameSaveData>(&text)
        .map_err(|e| warn!("Could not parse save file: {}", e))
        .ok()
}

// ---- Game Over ----

fn game_over_setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    run_summary: Res<RunSummary>,
) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");
    let summary = run_summary.clone();

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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, crate::ui::modal::MODAL_OVERLAY_OPACITY)),
            OnGameOverScreen,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("YOU DIED"),
                TextFont { font: font.clone(), font_size: 72.0, ..default() },
                TextColor(Color::srgb(0.85, 0.1, 0.1)),
            ));
            root.spawn((
                Text::new(summary.cause.clone()),
                TextFont { font: font.clone(), font_size: 20.0, ..default() },
                TextColor(Color::srgb(0.5, 0.5, 0.5)),
                Node { margin: UiRect::top(Val::Px(10.0)), ..default() },
            ));

            // Stats panel
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(20.0)),
                    margin: UiRect::top(Val::Px(28.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.35, 0.1, 0.1)),
                BackgroundColor(Color::srgba(0.1, 0.0, 0.0, 0.6)),
            ))
            .with_children(|panel| {
                let stat_color = Color::srgb(0.75, 0.65, 0.65);
                let stat_font = TextFont { font: font.clone(), font_size: 18.0, ..default() };
                let stat_node = Node { margin: UiRect::vertical(Val::Px(3.0)), ..default() };

                panel.spawn((
                    Text::new(format!("Floor reached:  {}", summary.floor_reached)),
                    stat_font.clone(), TextColor(stat_color), stat_node.clone(),
                ));
                panel.spawn((
                    Text::new(format!("Enemies slain:  {}", summary.enemies_killed)),
                    stat_font.clone(), TextColor(stat_color), stat_node,
                ));
            });

            root.spawn(Node { height: Val::Px(32.0), ..default() });

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

            root.spawn((
                Text::new("[ Enter ] Return to Menu"),
                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.3, 0.3, 0.3)),
                Node { margin: UiRect::top(Val::Px(18.0)), ..default() },
            ));
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

fn victory_setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    run_summary: Res<RunSummary>,
) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");
    let summary = run_summary.clone();

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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, crate::ui::modal::MODAL_OVERLAY_OPACITY)),
            OnVictoryScreen,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("VICTORY"),
                TextFont { font: font.clone(), font_size: 72.0, ..default() },
                TextColor(GOLD),
            ));
            root.spawn((
                Text::new("You have escaped the depths."),
                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                TextColor(Color::WHITE),
                Node { margin: UiRect::top(Val::Px(10.0)), ..default() },
            ));
            root.spawn((
                Text::new("Freedom at last."),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                Node { margin: UiRect::top(Val::Px(6.0)), ..default() },
            ));

            // Stats panel
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(20.0)),
                    margin: UiRect::top(Val::Px(28.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.5, 0.4, 0.0)),
                BackgroundColor(Color::srgba(0.08, 0.06, 0.0, 0.7)),
            ))
            .with_children(|panel| {
                let stat_color = Color::srgb(0.9, 0.82, 0.5);
                let stat_font = TextFont { font: font.clone(), font_size: 18.0, ..default() };
                let stat_node = Node { margin: UiRect::vertical(Val::Px(3.0)), ..default() };

                panel.spawn((
                    Text::new(format!("Floor reached:  {}", summary.floor_reached)),
                    stat_font.clone(), TextColor(stat_color), stat_node.clone(),
                ));
                panel.spawn((
                    Text::new(format!("Enemies slain:  {}", summary.enemies_killed)),
                    stat_font.clone(), TextColor(stat_color), stat_node,
                ));
            });

            root.spawn(Node { height: Val::Px(32.0), ..default() });

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

            root.spawn((
                Text::new("[ Enter ] Return to Menu"),
                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.3, 0.3, 0.3)),
                Node { margin: UiRect::top(Val::Px(18.0)), ..default() },
            ));
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

/// Format an integer with comma-separated thousands (e.g. 3250 → "3,250").
fn format_number(n: i32) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    if n < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

fn despawn_screen<T: Component>(query: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
