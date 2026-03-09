use bevy::prelude::*;

use crate::components::{Inventory, Name};
use crate::game::items::{DropItemMessage, ItemProperties, SelectedInventorySlot};
use crate::game::{AppState, InGameState};
use crate::player::Player;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            inventory_input_system.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            OnEnter(InGameState::Inventory),
            (spawn_inventory_ui, reset_inventory_selection),
        )
        .add_systems(
            Update,
            update_inventory_ui.run_if(in_state(InGameState::Inventory)),
        )
        .add_systems(
            OnExit(InGameState::Inventory),
            despawn_inventory_ui,
        );
    }
}

// --- Marker components ---

#[derive(Component)]
struct OnInventoryScreen;

#[derive(Component)]
struct InventorySlotText(usize);

#[derive(Component)]
struct InventoryDetailText;

// --- Systems ---

fn inventory_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    if keys.just_pressed(KeyCode::KeyI) {
        match state.get() {
            InGameState::Running => next_state.set(InGameState::Inventory),
            InGameState::Inventory => next_state.set(InGameState::Running),
            InGameState::CharacterInfo => {}
        }
    }
    if keys.just_pressed(KeyCode::Escape) && *state.get() == InGameState::Inventory {
        next_state.set(InGameState::Running);
    }
}

fn reset_inventory_selection(mut slot: ResMut<SelectedInventorySlot>) {
    slot.0 = 0;
}

fn spawn_inventory_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            ZIndex(200),
            OnInventoryScreen,
        ))
        .with_children(|root| {
            // Main panel
            root.spawn((
                Node {
                    width: Val::Px(700.0),
                    height: Val::Px(520.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::BLACK),
                BorderColor::all(Color::WHITE),
            ))
            .with_children(|panel| {
                // Header
                panel.spawn((
                    Text::new("INVENTORY"),
                    TextFont { font: font.clone(), font_size: 28.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.84, 0.0)),
                ));

                panel.spawn(Node { height: Val::Px(10.0), ..default() });

                // Two-column layout: item list + detail panel
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1.0,
                        column_gap: Val::Px(20.0),
                        ..default()
                    })
                    .with_children(|cols| {
                        // Left: item list (20 slots)
                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            width: Val::Px(320.0),
                            ..default()
                        })
                        .with_children(|list| {
                            for i in 0..20 {
                                list.spawn((
                                    Text::new(format!("{:2}. ---", i + 1)),
                                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                                    InventorySlotText(i),
                                ));
                            }
                        });

                        // Right: item detail panel
                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            border: UiRect::all(Val::Px(1.0)),
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        })
                        .with_children(|detail| {
                            detail.spawn((
                                Text::new("Select an item to\nsee its details."),
                                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                                InventoryDetailText,
                            ));
                        });
                    });

                panel.spawn(Node { height: Val::Px(10.0), ..default() });

                // Footer hints
                panel.spawn((
                    Text::new("↑/↓ Navigate  |  D - Drop  |  I/Esc - Close"),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                ));
            });
        });
}

fn update_inventory_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut slot: ResMut<SelectedInventorySlot>,
    mut drop_writer: MessageWriter<DropItemMessage>,
    mut next_state: ResMut<NextState<InGameState>>,
    inv_query: Query<&Inventory, With<Player>>,
    item_query: Query<(&Name, &ItemProperties)>,
    mut slot_texts: Query<(&mut Text, &mut TextColor, &InventorySlotText)>,
    mut detail_text: Query<(&mut Text, &mut TextColor), (With<InventoryDetailText>, Without<InventorySlotText>)>,
) {
    let Ok(inv) = inv_query.single() else {
        return;
    };
    let item_count = inv.items.len();

    // Navigation
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
        if item_count > 0 && slot.0 > 0 {
            slot.0 -= 1;
        }
    }
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
        if item_count > 0 && slot.0 + 1 < item_count {
            slot.0 += 1;
        }
    }

    // Drop
    if keys.just_pressed(KeyCode::KeyD) {
        if let Some(&item_entity) = inv.items.get(slot.0) {
            drop_writer.write(DropItemMessage { item_entity });
            if slot.0 > 0 && slot.0 >= item_count.saturating_sub(1) {
                slot.0 -= 1;
            }
            next_state.set(InGameState::Running);
            return;
        }
    }

    // Update slot list
    for (mut text, mut color, slot_marker) in &mut slot_texts {
        let i = slot_marker.0;
        if let Some(&item_entity) = inv.items.get(i) {
            if let Ok((name, props)) = item_query.get(item_entity) {
                text.0 = format!("{:2}. {}", i + 1, name.0);
                color.0 = props.rarity.color();
            }
        } else {
            text.0 = format!("{:2}. ---", i + 1);
            color.0 = Color::srgb(0.3, 0.3, 0.3);
        }

        // Highlight selected slot
        if i == slot.0 && i < item_count {
            // Prefix with arrow
            let existing = text.0.clone();
            if !existing.starts_with('>') {
                text.0 = format!("> {}", existing.trim_start_matches("> "));
            }
        } else {
            // Remove arrow if present
            if text.0.starts_with("> ") {
                text.0 = text.0[2..].to_string();
            }
        }
    }

    // Update detail panel for selected item
    if let Ok((mut text, mut color)) = detail_text.single_mut() {
        if let Some(&item_entity) = inv.items.get(slot.0) {
            if let Ok((name, props)) = item_query.get(item_entity) {
                let kind_str = match &props.armor_slot {
                    Some(slot) => format!("{} ({})", props.kind, slot),
                    None => props.kind.to_string(),
                };

                let mut lines = vec![
                    format!("{}", name.0),
                    format!("{} — {}", kind_str, props.rarity),
                ];

                if let Some(dmg) = &props.damage {
                    lines.push(format!("Damage: {}", dmg));
                }
                if props.defense > 0 {
                    lines.push(format!("Defense: +{}", props.defense));
                }
                let bonuses = props.bonus_summary();
                if !bonuses.is_empty() {
                    lines.push(bonuses);
                }

                lines.push(String::new());
                lines.push("[D] Drop item".to_string());

                text.0 = lines.join("\n");
                color.0 = props.rarity.color();
            }
        } else {
            text.0 = "Select an item to\nsee its details.".to_string();
            color.0 = Color::srgb(0.6, 0.6, 0.6);
        }
    }
}

fn despawn_inventory_ui(
    mut commands: Commands,
    query: Query<Entity, With<OnInventoryScreen>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
}
