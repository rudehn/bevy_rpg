use bevy::prelude::*;

use crate::components::GameEntityMarker;
use crate::game::camera::UiCamera;
use crate::game::{
    AppState,
    combat::{Health, HealthRegen},
    magic::{GameStatusEffectsExt, StatusEffects},
};
use crate::map::dungeon::Floor;
use crate::player::Player;

pub mod chasm_confirm;
pub mod character_creation;
pub mod cheat_menu;
pub mod enchant_select;
pub mod game_log;
pub mod help;
pub mod hover_info;
pub mod inventory;
pub mod log_history;
pub mod menu;
pub mod modal;
pub mod monster_info;
pub mod nearby;

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
            // Player name
            parent.spawn((
                Text::new("@: You"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));

            // Health text (hidden, used for accessibility/screen readers)
            parent.spawn((
                Text::new("Health: ??/??"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 1.0,
                    ..default()
                },
                TextColor(Color::NONE),
                PlayerHealthText,
            ));

            // Health Bar (Background)
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(10.0),
                        border: UiRect::all(Val::Px(1.0)),
                        margin: UiRect::top(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.5, 0.1, 0.1)),
                    BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.0, 0.8, 0.0)),
                        PlayerHealthBar,
                    ));
                });

            // Status effects container (dynamically populated)
            parent.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(3.0),
                    row_gap: Val::Px(2.0),
                    margin: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
                PlayerStatusEffectsContainer { last_count: 0, last_hash: 0 },
            ));

            // Floor depth
            parent.spawn((
                Text::new("Floor: 1"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.7, 0.4)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                FloorDepthText,
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
                Text::new("[I/C] Inventory"),
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

/// Collects active status effects with duration info: (label, color, turns_remaining, initial_duration).
pub fn collect_status_effects_with_duration(effects: Option<&StatusEffects>) -> Vec<(String, Color, u32, u32, String)> {
    effects
        .map(|e| e.display_entries_with_duration().into_iter().map(|(n, c, tr, id, desc)| (n.to_string(), c, tr, id, desc)).collect())
        .unwrap_or_default()
}

/// Spawns a status effect badge with a depleting progress bar.
pub fn spawn_status_badge(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    name: &str,
    color: Color,
    turns_remaining: u32,
    initial_duration: u32,
    description: &str,
) {
    let progress = if initial_duration > 0 {
        turns_remaining as f32 / initial_duration as f32
    } else {
        1.0
    };

    // Show description with remaining turns, e.g. "Burning: 2 fire dmg/turn, 3 turns"
    let label = format!("{} ({}t)", name, turns_remaining);

    parent.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            border: UiRect::all(Val::Px(1.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BorderColor::all(color),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        Interaction::None,
        Tooltip(description.to_string()),
    ))
    .with_children(|badge| {
        // Top row: label with turns
        badge.spawn((
            Text::new(&label),
            TextFont {
                font: font.clone(),
                font_size: 9.0,
                ..default()
            },
            TextColor(color),
            Node {
                padding: UiRect::horizontal(Val::Px(3.0)),
                ..default()
            },
        ));
        // Bottom: progress bar
        badge.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)),
        )).with_children(|bar_bg| {
            bar_bg.spawn((
                Node {
                    width: Val::Percent(progress * 100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(color.with_alpha(0.6)),
            ));
        });
    });
}

/// Marker for UI elements with tooltip text, displayed on mouse hover.
#[derive(Component)]
pub struct Tooltip(pub String);

/// Marker for the floating tooltip popup entity.
#[derive(Component)]
struct TooltipPopup;

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

    let effects = collect_status_effects_with_duration(status_effects);

    // Quick hash: combine effect labels + turns_remaining to detect changes
    use std::hash::Hash;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (label, _, turns_remaining, _, _) in &effects {
        label.hash(&mut hasher);
        turns_remaining.hash(&mut hasher);
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
        for (label, color, turns_remaining, initial_duration, desc) in &effects {
            spawn_status_badge(parent, &font, label, *color, *turns_remaining, *initial_duration, desc);
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

// --- Tooltip hover system ---

fn update_tooltip_popup(
    mut commands: Commands,
    windows: Query<&Window>,
    tooltip_query: Query<(&Interaction, &Tooltip)>,
    existing_popup: Query<Entity, With<TooltipPopup>>,
    asset_server: Res<AssetServer>,
) {
    // Find the hovered tooltip (if any)
    let mut hovered_text: Option<&str> = None;
    for (interaction, tooltip) in tooltip_query.iter() {
        if *interaction == Interaction::Hovered {
            hovered_text = Some(&tooltip.0);
            break;
        }
    }

    // Despawn old popup
    for entity in existing_popup.iter() {
        commands.entity(entity).despawn();
    }

    // Spawn new popup if hovering
    let Some(text) = hovered_text else { return };
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };

    let font = asset_server.load("fonts/SourceCodePro.ttf");
    let window_width = window.width();
    let window_height = window.height();

    // Estimate tooltip size (rough: 7px per char width, 18px height + padding)
    let estimated_width = text.len() as f32 * 7.0 + 16.0;
    let estimated_height = 22.0;
    let margin = 12.0;

    // Position horizontally: prefer right of cursor, flip to left if it would overflow
    let left = if cursor.x + margin + estimated_width < window_width {
        cursor.x + margin
    } else {
        (cursor.x - margin - estimated_width).max(0.0)
    };

    // Position vertically: prefer below cursor, flip above if it would overflow
    let top = if cursor.y + margin + estimated_height < window_height {
        cursor.y + margin
    } else {
        (cursor.y - margin - estimated_height).max(0.0)
    };

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(left),
            top: Val::Px(top),
            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)),
        BorderColor::all(Color::srgb(0.5, 0.5, 0.5)),
        ZIndex(300),
        TooltipPopup,
    )).with_children(|popup| {
        popup.spawn((
            Text::new(text),
            TextFont { font, font_size: 12.0, ..default() },
            TextColor(Color::WHITE),
        ));
    });
}

// --- Plugin ---

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameLog>()
            .init_resource::<GameLogSettings>()
            .add_message::<GameLogMessage>()
            .add_plugins((
                CheatMenuPlugin, InventoryPlugin, LogHistoryPlugin,
                MenuPlugin, NearbyPlugin, monster_info::MonsterInfoPlugin,
                hover_info::HoverInfoPlugin, enchant_select::EnchantSelectPlugin,
                help::HelpPlugin, chasm_confirm::ChasmConfirmPlugin,
                character_creation::CharacterCreationPlugin,
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
                    update_tooltip_popup,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
