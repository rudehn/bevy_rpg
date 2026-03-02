use bevy::{prelude::*, window::Window};

use crate::components::{GameEntityMarker, Monster, Name};
use crate::constants::TILE_SIZE_X;
use crate::game::camera::{MainCamera, UiCamera};
use crate::game::{
    AppState,
    actions::ActionStats,
    combat::{Damage, Health},
};
use crate::player::Player;

pub mod game_log;

use game_log::{
    GameLog, GameLogMessage, GameLogSettings, add_log_message_system, game_log_input_system,
    spawn_game_log_ui, update_game_log_ui,
};

// --- Components ---

/// Marker component for the player's health text UI element.
#[derive(Component)]
pub struct PlayerHealthText;

/// Marker component for the player's health bar UI element.
#[derive(Component)]
pub struct PlayerHealthBar;

/// Marker component for the player's health bar UI element.
#[derive(Component)]
pub struct PlayerHealthBarBackground;

/// Marker component for the root UI node of the monster tooltip.
#[derive(Component)]
pub struct MonsterTooltip;

/// Marker component for the text displaying the monster's name in the tooltip.
#[derive(Component)]
pub struct MonsterTooltipName;

/// Marker component for the text displaying the monster's health in the tooltip.
#[derive(Component)]
pub struct MonsterTooltipHealth;

/// Marker component for the text displaying the monster's damage in the tooltip.
#[derive(Component)]
pub struct MonsterTooltipDamage;

/// Marker component for the text displaying the monster's move speed in the tooltip.
#[derive(Component)]
pub struct MonsterTooltipMoveSpeed;

/// Marker component for the text displaying the monster's action speed in the tooltip.
#[derive(Component)]
pub struct MonsterTooltipActionSpeed;

// --- Systems ---

/// System that spawns the player stats UI panel on the right side of the screen.
fn spawn_player_stats_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    q_ui_camera: Query<Entity, With<UiCamera>>,
) {
    let Ok(ui_camera) = q_ui_camera.single() else {
        return;
    };

    // Root UI node for the panel
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(200.0),
                height: Val::Percent(100.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                border: UiRect::left(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::FlexStart,
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            BorderColor::all(Color::WHITE),
            UiTargetCamera(ui_camera),
            GameEntityMarker,
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("PLAYER STATS"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));

            // Health Text
            parent.spawn((
                Text::new("Health: ??/??"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                PlayerHealthText,
            ));

            // Health Bar (Background)
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(20.0),
                        border: UiRect::all(Val::Px(1.0)),
                        margin: UiRect::top(Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.0, 0.0)), // Red
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    // Health Bar (Foreground - actual health)
                    parent.spawn((
                        Node {
                            width: Val::Percent(100.0), // Starts full
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), // Green
                        PlayerHealthBar,
                    ));
                });
        });
}

/// System that updates the player's health display in the UI.
fn update_player_stats_ui(
    player_query: Query<&Health, With<Player>>,
    mut health_text_query: Query<&mut Text, With<PlayerHealthText>>,
    mut health_bar_query: Query<&mut Node, With<PlayerHealthBar>>,
) {
    let Ok(player_health) = player_query.single() else {
        return;
    };

    let health_percentage = player_health.current as f32 / player_health.max as f32;

    if let Ok(mut text) = health_text_query.single_mut() {
        text.0 = format!("Health: {}/{}", player_health.current, player_health.max);
    }

    if let Ok(mut health_bar_node) = health_bar_query.single_mut() {
        health_bar_node.width = Val::Percent(health_percentage * 100.0);
    }
}

/// System that spawns the monster tooltip UI.
fn spawn_monster_tooltip_ui(
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
                display: Display::None, // Initially hidden
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
            BorderColor::all(Color::WHITE),
            ZIndex(100),        // Ensure it's on top
            Visibility::Hidden, // Use Visibility for intent
            UiTargetCamera(ui_camera),
            MonsterTooltip,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Name: "),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                MonsterTooltipName,
            ));
            parent.spawn((
                Text::new("Health: "),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                MonsterTooltipHealth,
            ));
            parent.spawn((
                Text::new("Damage: "),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                MonsterTooltipDamage,
            ));
            parent.spawn((
                Text::new("Move Speed: "),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                MonsterTooltipMoveSpeed,
            ));
            parent.spawn((
                Text::new("Action Speed: "),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                MonsterTooltipActionSpeed,
            ));
        });
}

/// System that detects mouse hover over monsters/player and updates the tooltip.
fn update_monster_tooltip_ui(
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_actors: Query<
        (
            Entity,
            &GlobalTransform,
            &Name,
            &Health,
            &Damage,
            Option<&ActionStats>,
            &InheritedVisibility,
        ),
        (Or<(With<Monster>, With<Player>)>, Without<MonsterTooltip>),
    >,
    mut q_tooltip_root: Query<
        (&mut Node, &mut Visibility),
        (With<MonsterTooltip>, Without<Monster>, Without<Player>),
    >,
    mut q_tooltip_name: Query<&mut Text, With<MonsterTooltipName>>,
    mut q_tooltip_health: Query<
        &mut Text,
        (With<MonsterTooltipHealth>, Without<MonsterTooltipName>),
    >,
    mut q_tooltip_damage: Query<
        &mut Text,
        (
            With<MonsterTooltipDamage>,
            Without<MonsterTooltipName>,
            Without<MonsterTooltipHealth>,
        ),
    >,
    mut q_tooltip_move_speed: Query<
        &mut Text,
        (
            With<MonsterTooltipMoveSpeed>,
            Without<MonsterTooltipName>,
            Without<MonsterTooltipHealth>,
            Without<MonsterTooltipDamage>,
        ),
    >,
    mut q_tooltip_action_speed: Query<
        &mut Text,
        (
            With<MonsterTooltipActionSpeed>,
            Without<MonsterTooltipName>,
            Without<MonsterTooltipHealth>,
            Without<MonsterTooltipDamage>,
            Without<MonsterTooltipMoveSpeed>,
        ),
    >,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = q_camera.single() else {
        return;
    };

    let mut hovered_actor_data = None;

    if let Some(screen_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            let mouse_world_x = world_pos.x;
            let mouse_world_y = world_pos.y;

            // Simple AABB check for hover using GlobalTransform
            for (entity, global_transform, name, health, damage, action_stats, visibility) in
                q_actors.iter()
            {
                if !visibility.get() {
                    continue;
                }

                let translation = global_transform.translation();
                let scale = global_transform.compute_transform().scale;

                // Effective size in world units
                let half_size_x = (TILE_SIZE_X as f32 * scale.x) / 2.0;
                let half_size_y = (TILE_SIZE_X as f32 * scale.y) / 2.0;

                let min_x = translation.x - half_size_x;
                let max_x = translation.x + half_size_x;
                let min_y = translation.y - half_size_y;
                let max_y = translation.y + half_size_y;

                if mouse_world_x >= min_x
                    && mouse_world_x <= max_x
                    && mouse_world_y >= min_y
                    && mouse_world_y <= max_y
                {
                    hovered_actor_data =
                        Some((entity, translation, name, health, damage, action_stats));
                    break;
                }
            }
        }
    }

    let Ok((mut tooltip_node, mut tooltip_visibility)) = q_tooltip_root.single_mut() else {
        return;
    };

    if let Some((_entity, actor_world_pos, name, health, damage, action_stats)) = hovered_actor_data
    {
        // Show tooltip
        *tooltip_visibility = Visibility::Visible;
        tooltip_node.display = Display::Flex;

        // Position tooltip near the actor
        if let Ok(screen_pos_actor) = camera.world_to_viewport(camera_transform, actor_world_pos) {
            // Viewport logical coordinates are same as window logical coordinates
            // since physical_position is (0,0) and logical offset is also (0,0).
            tooltip_node.left = Val::Px(screen_pos_actor.x + 15.0);
            tooltip_node.top = Val::Px(screen_pos_actor.y - 15.0);
        }

        // Update tooltip text
        if let Ok(mut name_text) = q_tooltip_name.single_mut() {
            name_text.0 = format!("Name: {}", name.0);
        }
        if let Ok(mut health_text) = q_tooltip_health.single_mut() {
            health_text.0 = format!("Health: {}/{}", health.current, health.max);
        }
        if let Ok(mut damage_text) = q_tooltip_damage.single_mut() {
            damage_text.0 = format!("Damage: {}", damage.0);
        }

        if let Some(stats) = action_stats {
            if let Ok(mut move_speed_text) = q_tooltip_move_speed.single_mut() {
                move_speed_text.0 = format!("Move Delay: {}x", stats.move_delay);
            }
            if let Ok(mut action_speed_text) = q_tooltip_action_speed.single_mut() {
                action_speed_text.0 = format!("Action Delay: {}x", stats.action_delay);
            }
        } else {
            // Hide speed info for player if ActionStats is missing
            if let Ok(mut move_speed_text) = q_tooltip_move_speed.single_mut() {
                move_speed_text.0 = String::new();
            }
            if let Ok(mut action_speed_text) = q_tooltip_action_speed.single_mut() {
                action_speed_text.0 = String::new();
            }
        }
    } else {
        // Hide tooltip
        *tooltip_visibility = Visibility::Hidden;
        tooltip_node.display = Display::None;
    }
}

// --- Plugin ---

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameLog>()
            .init_resource::<GameLogSettings>()
            .add_message::<GameLogMessage>()
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    spawn_player_stats_ui,
                    spawn_monster_tooltip_ui,
                    spawn_game_log_ui,
                ),
            )
            .add_systems(
                Update,
                (
                    update_player_stats_ui,
                    update_monster_tooltip_ui,
                    add_log_message_system,
                    update_game_log_ui,
                    game_log_input_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
