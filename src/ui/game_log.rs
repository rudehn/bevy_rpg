use crate::components::GameEntityMarker;
use bevy::prelude::*;

#[derive(Message, Debug, Clone)]
pub struct GameLogMessage(pub String);

#[derive(Resource, Default)]
pub struct GameLog {
    pub entries: Vec<String>,
    /// Temporary prompt shown during targeting and other transient states.
    /// Displayed in the log panel but NOT stored in `entries`.
    /// Set this when entering a transient state; clear it on exit.
    pub status_message: Option<String>,
}

#[derive(Component)]
pub struct GameLogNode;

#[derive(Component)]
pub struct GameLogText;

#[allow(dead_code)]
#[derive(Resource)]
pub struct GameLogSettings {
    pub expanded: bool,
    pub scroll_offset: usize,
}

impl Default for GameLogSettings {
    fn default() -> Self {
        Self {
            expanded: false,
            scroll_offset: 0,
        }
    }
}

pub fn add_log_message_system(
    mut messages: MessageReader<GameLogMessage>,
    mut game_log: ResMut<GameLog>,
) {
    for message in messages.read() {
        game_log.entries.push(message.0.clone());
    }
}

use crate::game::camera::UiCamera;

pub fn spawn_game_log_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    q_ui_camera: Query<Entity, With<UiCamera>>,
) {
    let Ok(ui_camera) = q_ui_camera.single() else {
        return;
    };

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Px(150.0),
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                border: UiRect::top(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::BLACK.with_alpha(0.85)),
            BorderColor::all(Color::WHITE),
            UiTargetCamera(ui_camera),
            GameLogNode,
            GameEntityMarker,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(""),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                GameLogText,
            ));
        });
}

pub fn update_game_log_ui(game_log: Res<GameLog>, mut q_text: Query<&mut Text, With<GameLogText>>) {
    if !game_log.is_changed() {
        return;
    }
    if let Ok(mut text) = q_text.single_mut() {
        let max_lines = 5;
        let entries_len = game_log.entries.len();
        let start = entries_len.saturating_sub(max_lines);
        let mut lines: Vec<String> = game_log.entries[start..].to_vec();

        if let Some(status) = &game_log.status_message {
            lines.push(format!("> {}", status));
        }

        text.0 = lines.join("\n");
    }
}

pub fn game_log_input_system() {
    // Resizing and hotkeys are disabled for now.
}
