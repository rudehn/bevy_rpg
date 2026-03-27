use bevy::prelude::*;

use crate::game::{AppState, InGameState};
use crate::game::turns::TurnState;
use crate::ui::game_log::GameLog;

pub struct LogHistoryPlugin;

impl Plugin for LogHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LogScrollOffset>()
            .insert_resource(LogScrollTimer(Timer::from_seconds(0.05, TimerMode::Repeating)))
            .add_systems(
                Update,
                log_history_input.run_if(
                    in_state(AppState::InGame)
                        .and(in_state(TurnState::PlayerInput).or(in_state(InGameState::LogHistory))),
                ),
            )
            .add_systems(OnEnter(InGameState::LogHistory), (reset_scroll_offset, spawn_log_history_ui).chain())
            .add_systems(
                Update,
                update_log_history_ui.run_if(in_state(InGameState::LogHistory)),
            )
            .add_systems(OnExit(InGameState::LogHistory), crate::ui::modal::despawn_screen::<OnLogHistoryScreen>);
    }
}

const VISIBLE_LINES: usize = 30;

/// Distance from the bottom of the log (0 = newest, max = oldest visible at top).
#[derive(Resource, Default)]
pub struct LogScrollOffset(pub usize);

/// Repeat timer for held scroll keys.
#[derive(Resource)]
struct LogScrollTimer(Timer);

#[derive(Component)]
struct OnLogHistoryScreen;

#[derive(Component)]
struct LogHistoryText;

#[derive(Component)]
struct LogHistoryScrollLabel;

fn reset_scroll_offset(mut offset: ResMut<LogScrollOffset>) {
    offset.0 = 0;
}

fn log_history_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
    mut offset: ResMut<LogScrollOffset>,
    mut scroll_timer: ResMut<LogScrollTimer>,
    game_log: Res<GameLog>,
) {
    // Toggle open/close.
    if crate::ui::modal::toggle_screen(&keys, &state, &mut next_state, KeyCode::KeyL, InGameState::LogHistory) {
        return;
    }

    if *state.get() != InGameState::LogHistory {
        return;
    }

    let count = game_log.entries.len();
    let max_offset = count.saturating_sub(VISIBLE_LINES);

    // Jump keys — instant, no timer.
    if keys.just_pressed(KeyCode::PageUp) || keys.just_pressed(KeyCode::Period) {
        offset.0 = max_offset;
        return;
    }
    if keys.just_pressed(KeyCode::PageDown) || keys.just_pressed(KeyCode::Slash) {
        offset.0 = 0;
        return;
    }

    // Held scroll — fire immediately on first press, then repeat on timer.
    let scroll_up = keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp);
    let scroll_down = keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown);

    if scroll_up || scroll_down {
        let just_started = keys.just_pressed(KeyCode::KeyW)
            || keys.just_pressed(KeyCode::ArrowUp)
            || keys.just_pressed(KeyCode::KeyS)
            || keys.just_pressed(KeyCode::ArrowDown);

        if just_started {
            scroll_timer.0.reset();
        }

        scroll_timer.0.tick(time.delta());

        if just_started || scroll_timer.0.just_finished() {
            if scroll_up {
                offset.0 = (offset.0 + 1).min(max_offset);
            } else {
                offset.0 = offset.0.saturating_sub(1);
            }
        }
    } else {
        // No scroll key held — keep timer reset so next press fires immediately.
        scroll_timer.0.reset();
    }
}

fn spawn_log_history_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(20.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.93)),
            ZIndex(200),
            OnLogHistoryScreen,
        ))
        .with_children(|root| {
            // Title row
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                margin: UiRect::bottom(Val::Px(10.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("MESSAGE LOG"),
                    TextFont { font: font.clone(), font_size: 22.0, ..default() },
                    TextColor(Color::srgb(0.8, 0.8, 0.4)),
                ));
                row.spawn((
                    Text::new(""),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                    LogHistoryScrollLabel,
                ));
            });

            // Log body
            root.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 15.0, ..default() },
                TextColor(Color::WHITE),
                LogHistoryText,
            ));

            // Push footer to bottom
            root.spawn(Node { flex_grow: 1.0, ..default() });

            // Footer hint
            root.spawn((
                Text::new("W/↑ Scroll up  |  S/↓ Scroll down  |  . Top  |  / Bottom  |  PgUp/PgDn Jump  |  L / Esc  Close"),
                TextFont { font: font.clone(), font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.4, 0.4, 0.4)),
            ));
        });
}

fn update_log_history_ui(
    game_log: Res<GameLog>,
    offset: Res<LogScrollOffset>,
    mut q_text: Query<&mut Text, (With<LogHistoryText>, Without<LogHistoryScrollLabel>)>,
    mut q_label: Query<&mut Text, (With<LogHistoryScrollLabel>, Without<LogHistoryText>)>,
) {
    if !game_log.is_changed() && !offset.is_changed() {
        return;
    }

    let entries = &game_log.entries;
    let count = entries.len();
    let end = count.saturating_sub(offset.0);
    let start = end.saturating_sub(VISIBLE_LINES);

    if let Ok(mut text) = q_text.single_mut() {
        if count == 0 {
            text.0 = "No messages yet.".to_string();
        } else {
            text.0 = entries[start..end]
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{:4}. {}", start + i + 1, s))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }

    if let Ok(mut label) = q_label.single_mut() {
        if count == 0 {
            label.0 = String::new();
        } else {
            label.0 = format!("{}-{} / {}", start + 1, end, count);
        }
    }
}

// Despawn handled by modal::despawn_screen::<OnLogHistoryScreen>
