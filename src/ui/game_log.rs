use bevy::prelude::*;
use crate::components::GameEntityMarker;

#[derive(Message, Debug, Clone)]
pub struct GameLogMessage(pub String);

#[derive(Resource, Default)]
pub struct GameLog {
    pub entries: Vec<String>,
}

#[derive(Component)]
pub struct GameLogNode;

#[derive(Component)]
pub struct GameLogText;

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

pub fn spawn_game_log_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
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

pub fn update_game_log_ui(
    game_log: Res<GameLog>,
    mut q_text: Query<&mut Text, With<GameLogText>>,
) {
    if let Ok(mut text) = q_text.single_mut() {
        let max_lines = 5;
        let entries_len = game_log.entries.len();
        
        let start = if entries_len > max_lines {
            entries_len - max_lines
        } else {
            0
        };
        
        let end = entries_len;
        
        let displayed_messages: Vec<String> = game_log.entries[start..end].to_vec();
        text.0 = displayed_messages.join("\n");
    }
}

pub fn game_log_input_system() {
    // Resizing and hotkeys are disabled for now.
}
