use bevy::prelude::*;

use crate::components::GameEntityMarker;
use crate::game::camera::UiCamera;
use crate::game::{
    AppState,
    combat::{Health, HealthRegen},
    magic::StatusEffects,
};
use crate::map::dungeon::Floor;
use crate::player::Player;

pub mod character_info;
pub mod cheat_menu;
pub mod enchant_select;
pub mod game_log;
pub mod staff_select;
pub mod hover_info;
pub mod inventory;
pub mod log_history;
pub mod menu;
pub mod modal;
pub mod monster_info;
pub mod nearby;

use character_info::CharacterInfoPlugin;
use cheat_menu::CheatMenuPlugin;
use inventory::InventoryPlugin;
use log_history::LogHistoryPlugin;
use menu::MenuPlugin;
use nearby::{NearbyListRoot, NearbyPlugin};
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
#[allow(dead_code)]
#[derive(Component)]
pub struct PlayerHealthBarBackground;

#[derive(Component)]
pub struct FloorDepthText;

/// Container for player status effect icons in the HUD.
/// Stores a snapshot of the last rendered effect count to avoid per-frame rebuilds.
#[derive(Component)]
struct PlayerStatusEffectsContainer {
    last_count: usize,
    last_hash: u64,
}

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
                        height: Val::Px(14.0),
                        border: UiRect::all(Val::Px(1.0)),
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.0, 0.0)), // Red
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), // Green
                        PlayerHealthBar,
                    ));
                });

            // Floor depth
            parent.spawn((
                Text::new("Floor: 1"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.7, 0.4)),
                Node { margin: UiRect::top(Val::Px(10.0)), ..default() },
                FloorDepthText,
            ));

            // Status effects container (dynamically populated)
            parent.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(2.0),
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                },
                PlayerStatusEffectsContainer { last_count: 0, last_hash: 0 },
            ));

            // Nearby entities container — populated each turn by NearbyPlugin
            parent.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    ..default()
                },
                NearbyListRoot,
            ));

            // Push hotkeys to the bottom
            parent.spawn(Node { flex_grow: 1.0, ..default() });

            parent.spawn((
                Text::new("[C] Character"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.55)),
            ));
            parent.spawn((
                Text::new("[I] Inventory"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.55)),
            ));
        });
}

/// System that updates the player's health display in the UI.
fn update_player_stats_ui(
    player_query: Query<(&Health, Option<&HealthRegen>), (With<Player>, Changed<Health>)>,
    mut health_text_query: Query<&mut Text, With<PlayerHealthText>>,
    mut health_bar_query: Query<&mut Node, With<PlayerHealthBar>>,
) {
    let Ok((player_health, player_regen)) = player_query.single() else {
        return;
    };

    let health_percentage = player_health.current as f32 / player_health.max as f32;

    if let Ok(mut text) = health_text_query.single_mut() {
        let mut health_str = format!("Health: {}/{}", player_health.current, player_health.max);
        if let Some(regen) = player_regen
            && regen.regen_rate > 0 {
                if regen.regen_rate >= 100 {
                    health_str.push_str(&format!(" (+{}/t)", regen.regen_rate / 100));
                } else {
                    let turns = 100 / regen.regen_rate;
                    health_str.push_str(&format!(" (+1/{}t)", turns));
                }
            }
        text.0 = health_str;
    }

    if let Ok(mut health_bar_node) = health_bar_query.single_mut() {
        health_bar_node.width = Val::Percent(health_percentage * 100.0);
    }
}

fn update_floor_ui(
    floor: Res<Floor>,
    mut text_query: Query<&mut Text, With<FloorDepthText>>,
) {
    if !floor.is_changed() { return }
    if let Ok(mut text) = text_query.single_mut() {
        text.0 = format!("Floor: {}", floor.0);
    }
}

/// Collects active status effects as (label, color) tuples.
pub fn collect_status_effects(effects: Option<&StatusEffects>) -> Vec<(String, Color)> {
    effects
        .map(|e| e.display_entries().into_iter().map(|(n, c)| (n.to_string(), c)).collect())
        .unwrap_or_default()
}

#[allow(clippy::type_complexity)]
fn update_player_status_effects_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut q_container: Query<(Entity, &mut PlayerStatusEffectsContainer)>,
    player_query: Query<
        Option<&StatusEffects>,
        With<Player>,
    >,
) {
    let Ok((container, mut tracker)) = q_container.single_mut() else {
        return;
    };
    let Ok(status_effects) = player_query.single() else {
        return;
    };

    let effects = collect_status_effects(status_effects);

    // Quick hash: combine effect count with sum of turns to detect changes
    use std::hash::Hash;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (label, _) in &effects {
        label.hash(&mut hasher);
    }
    let hash = std::hash::Hasher::finish(&hasher);

    if effects.len() == tracker.last_count && hash == tracker.last_hash {
        return;
    }
    tracker.last_count = effects.len();
    tracker.last_hash = hash;

    // Rebuild children
    commands.entity(container).despawn_related::<Children>();

    if effects.is_empty() {
        return;
    }

    let font: Handle<Font> = asset_server.load("fonts/Macondo-Regular.ttf");

    commands.entity(container).with_children(|parent| {
        for (label, color) in &effects {
            parent.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(*color),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            ))
            .with_children(|badge| {
                badge.spawn((
                    Text::new(label.clone()),
                    TextFont {
                        font: font.clone(),
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(*color),
                ));
            });
        }
    });
}

/// Helper to get semantic speed traits and their colors based on dangerousness.
/// Fast = Low Multiplier = RED (Dangerous)
/// Slow = High Multiplier = GREEN (Advantage)
pub fn get_speed_trait(multiplier: f32, category: &str) -> Option<(String, Color)> {
    if multiplier < 0.75 {
        Some((
            format!("Very Quick {}", category),
            Color::srgb(1.0, 0.2, 0.2),
        )) // Bright Red
    } else if multiplier < 0.95 {
        Some((format!("Fast {}", category), Color::srgb(1.0, 0.6, 0.2))) // Orange
    } else if multiplier > 1.3 {
        Some((format!("Sluggish {}", category), Color::srgb(0.2, 1.0, 0.2))) // Bright Green
    } else if multiplier > 1.05 {
        Some((format!("Slow {}", category), Color::srgb(0.5, 1.0, 0.5))) // Pale Green
    } else {
        None
    }
}

// --- Plugin ---

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameLog>()
            .init_resource::<GameLogSettings>()
            .add_message::<GameLogMessage>()
            .add_plugins((
                CharacterInfoPlugin, CheatMenuPlugin, InventoryPlugin, LogHistoryPlugin,
                MenuPlugin, NearbyPlugin, monster_info::MonsterInfoPlugin,
                hover_info::HoverInfoPlugin, enchant_select::EnchantSelectPlugin,
                staff_select::StaffSelectPlugin,
            ))
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    spawn_player_stats_ui,
                    spawn_game_log_ui,
                ),
            )
            .add_systems(
                Update,
                (
                    update_player_stats_ui,
                    update_floor_ui,
                    update_player_status_effects_ui,
                    add_log_message_system,
                    update_game_log_ui,
                    game_log_input_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
