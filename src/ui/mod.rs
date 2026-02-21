use bevy::{prelude::*, window::Window};

use crate::components::Monster;
use crate::constants::TILE_SIZE_X; // For rough monster size check
use crate::game::camera::MainCamera;
use crate::game::{
    AppState,
    combat::{Damage, Health},
};
use crate::player::Player;

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

// --- Systems ---

/// System that spawns the player stats UI panel on the right side of the screen.
fn spawn_player_stats_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Root UI node for the panel
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(200.0),
                height: Val::Percent(100.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                border: UiRect::left(Val::Px(2.0)), // Updated to use UiRect for border
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::FlexStart,
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            BorderColor::all(Color::WHITE),
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
    mut health_bar_query: Query<&mut Node, With<PlayerHealthBar>>, // Style merged into Node
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
fn spawn_monster_tooltip_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
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
            BackgroundColor(Color::BLACK),
            BorderColor::all(Color::WHITE),
            Visibility::Hidden, // Explicitly hide alongside display none
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
        });
}

/// System that detects mouse hover over monsters and updates the tooltip.
fn update_monster_tooltip_ui(
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_monsters: Query<(Entity, &Transform, &Name, &Health, &Damage), With<Monster>>,
    mut q_tooltip_root: Query<(&mut Node, &mut Visibility), With<MonsterTooltip>>, // Style replaced by Node
    mut q_tooltip_name: Query<
        &mut Text,
        (
            With<MonsterTooltipName>,
            Without<MonsterTooltipHealth>,
            Without<MonsterTooltipDamage>,
        ),
    >,
    mut q_tooltip_health: Query<
        &mut Text,
        (
            With<MonsterTooltipHealth>,
            Without<MonsterTooltipName>,
            Without<MonsterTooltipDamage>,
        ),
    >,
    mut q_tooltip_damage: Query<
        &mut Text,
        (
            With<MonsterTooltipDamage>,
            Without<MonsterTooltipName>,
            Without<MonsterTooltipHealth>,
        ),
    >,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = q_camera.single() else {
        return;
    };

    let mut hovered_monster_data = None;

    if let Some(screen_pos) = window.cursor_position() {
        // viewport_to_world_2d now returns a Result
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            let mouse_world_x = world_pos.x;
            let mouse_world_y = world_pos.y;

            // Simple AABB check for hover
            for (entity, monster_transform, name, health, damage) in q_monsters.iter() {
                let monster_size = TILE_SIZE_X as f32 * monster_transform.scale.x;
                let half_size = monster_size / 2.0;

                let min_x = monster_transform.translation.x - half_size;
                let max_x = monster_transform.translation.x + half_size;
                let min_y = monster_transform.translation.y - half_size;
                let max_y = monster_transform.translation.y + half_size;

                if mouse_world_x >= min_x
                    && mouse_world_x <= max_x
                    && mouse_world_y >= min_y
                    && mouse_world_y <= max_y
                {
                    hovered_monster_data = Some((entity, monster_transform, name, health, damage));
                    break;
                }
            }
        }
    }

    let Ok((mut tooltip_node, mut tooltip_visibility)) = q_tooltip_root.single_mut() else {
        return;
    };

    if let Some((_entity, monster_transform, name, health, damage)) = hovered_monster_data {
        // Show tooltip
        *tooltip_visibility = Visibility::Visible;
        tooltip_node.display = Display::Flex;

        // Position tooltip near the monster
        // world_to_viewport now returns a Result
        if let Ok(screen_pos_monster) =
            camera.world_to_viewport(camera_transform, monster_transform.translation)
        {
            tooltip_node.left = Val::Px(screen_pos_monster.x + 10.0);
            tooltip_node.top = Val::Px(screen_pos_monster.y - 10.0);
        } else {
            // Fallback: position near cursor if monster not visible on screen
            if let Some(cursor_pos) = window.cursor_position() {
                tooltip_node.left = Val::Px(cursor_pos.x + 10.0);
                tooltip_node.top = Val::Px(cursor_pos.y - 10.0);
            }
        }

        // Update tooltip text (Text is now directly mutated)
        if let Ok(mut name_text) = q_tooltip_name.single_mut() {
            name_text.0 = format!("Name: {}", name.as_str());
        }
        if let Ok(mut health_text) = q_tooltip_health.single_mut() {
            health_text.0 = format!("Health: {}/{}", health.current, health.max);
        }
        if let Ok(mut damage_text) = q_tooltip_damage.single_mut() {
            damage_text.0 = format!("Damage: {}", damage.0);
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
        app.add_systems(
            OnEnter(AppState::InGame),
            (spawn_player_stats_ui, spawn_monster_tooltip_ui),
        )
        .add_systems(
            Update,
            (update_player_stats_ui, update_monster_tooltip_ui).run_if(in_state(AppState::InGame)),
        );
    }
}
