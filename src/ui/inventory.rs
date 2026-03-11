use bevy::prelude::*;

use crate::components::{Equipped, Inventory, Name};
use crate::game::actions::Action;
use crate::game::items::{Equipment, ItemKind, ItemProperties, ItemStack, SelectedInventorySlot};
use crate::game::actions::PendingPlayerAction;
use crate::game::turns::TurnState;
use crate::game::{AppState, InGameState};
use crate::player::Player;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            // Only toggle inventory when it's the player's turn and the game is running.
            // This prevents opening the bag mid-NPC-turn and avoids double-input on close.
            inventory_input_system.run_if(
                in_state(AppState::InGame)
                    .and(in_state(TurnState::PlayerInput).or(in_state(InGameState::Inventory))),
            ),
        )
        .add_systems(
            OnEnter(InGameState::Inventory),
            (spawn_inventory_ui, reset_inventory_selection),
        )
        .add_systems(
            Update,
            update_inventory_ui.run_if(in_state(InGameState::Inventory)),
        )
        .add_systems(OnExit(InGameState::Inventory), despawn_inventory_ui);
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
            InGameState::CharacterInfo | InGameState::Spells | InGameState::Targeting => {}
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
                panel.spawn((
                    Text::new("INVENTORY"),
                    TextFont { font: font.clone(), font_size: 28.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.84, 0.0)),
                ));

                panel.spawn(Node { height: Val::Px(10.0), ..default() });

                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1.0,
                        column_gap: Val::Px(20.0),
                        ..default()
                    })
                    .with_children(|cols| {
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

                panel.spawn((
                    Text::new("↑/↓ Navigate  |  E - Equip/Unequip  |  U - Use  |  D - Drop  |  I/Esc - Close"),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                ));
            });
        });
}

/// Handles inventory navigation and item actions.
/// E and D both cost a turn: they set PendingPlayerAction and transition to Processing.
fn update_inventory_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut slot: ResMut<SelectedInventorySlot>,
    mut pending: ResMut<PendingPlayerAction>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    mut next_turn: ResMut<NextState<TurnState>>,
    inv_query: Query<(&Inventory, &Equipment), With<Player>>,
    item_query: Query<(&Name, &ItemProperties, Has<Equipped>, Option<&ItemStack>)>,
    mut slot_texts: Query<(&mut Text, &mut TextColor, &InventorySlotText)>,
    mut detail_text: Query<
        (&mut Text, &mut TextColor),
        (With<InventoryDetailText>, Without<InventorySlotText>),
    >,
) {
    let Ok((inv, _equipment)) = inv_query.single() else {
        return;
    };
    let item_count = inv.items.len();

    // Navigation (no turn cost, stays in inventory)
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

    // Equip / Unequip — costs a turn
    if keys.just_pressed(KeyCode::KeyE) {
        if let Some(&item_entity) = inv.items.get(slot.0) {
            if let Ok((_, props, is_equipped, _)) = item_query.get(item_entity) {
                if Equipment::slot_for(props).is_some() {
                    let action = if is_equipped {
                        Action::UnequipItem { item: item_entity }
                    } else {
                        Action::EquipItem { item: item_entity }
                    };
                    pending.0 = Some(action);
                    next_ingame.set(InGameState::Running);
                    next_turn.set(TurnState::Processing);
                    return;
                }
            }
        }
    }

    // Drop — costs a turn
    if keys.just_pressed(KeyCode::KeyD) {
        if let Some(&item_entity) = inv.items.get(slot.0) {
            if slot.0 > 0 && slot.0 >= item_count.saturating_sub(1) {
                slot.0 -= 1;
            }
            pending.0 = Some(Action::DropItem { item: item_entity });
            next_ingame.set(InGameState::Running);
            next_turn.set(TurnState::Processing);
            return;
        }
    }

    // Use / consume — costs a turn (consumables only)
    if keys.just_pressed(KeyCode::KeyU) {
        if let Some(&item_entity) = inv.items.get(slot.0) {
            if let Ok((_, props, _, _)) = item_query.get(item_entity) {
                if props.kind == ItemKind::Consumable || props.kind == ItemKind::Spellbook {
                    if slot.0 > 0 && slot.0 >= item_count.saturating_sub(1) {
                        slot.0 -= 1;
                    }
                    pending.0 = Some(Action::UseItem { item: item_entity });
                    next_ingame.set(InGameState::Running);
                    next_turn.set(TurnState::Processing);
                    return;
                }
            }
        }
    }

    // Update slot list
    for (mut text, mut color, slot_marker) in &mut slot_texts {
        let i = slot_marker.0;
        if let Some(&item_entity) = inv.items.get(i) {
            if let Ok((name, props, is_equipped, stack)) = item_query.get(item_entity) {
                let equipped_tag = if is_equipped { " [E]" } else { "" };
                let stack_tag = match stack {
                    Some(s) if s.count > 1 => format!(" (x{})", s.count),
                    _ => String::new(),
                };
                text.0 = format!("{:2}. {}{}{}", i + 1, name.0, stack_tag, equipped_tag);
                color.0 = props.rarity.color();
            }
        } else {
            text.0 = format!("{:2}. ---", i + 1);
            color.0 = Color::srgb(0.3, 0.3, 0.3);
        }

        if i == slot.0 && i < item_count {
            let existing = text.0.clone();
            if !existing.starts_with('>') {
                text.0 = format!("> {}", existing.trim_start_matches("> "));
            }
        } else if text.0.starts_with("> ") {
            text.0 = text.0[2..].to_string();
        }
    }

    // Update detail panel
    if let Ok((mut text, mut color)) = detail_text.single_mut() {
        if let Some(&item_entity) = inv.items.get(slot.0) {
            if let Ok((name, props, is_equipped, stack)) = item_query.get(item_entity) {
                let kind_str = match &props.armor_slot {
                    Some(s) => format!("{} ({})", props.kind, s),
                    None => props.kind.to_string(),
                };

                let mut lines = vec![
                    name.0.clone(),
                    format!("{} — {}", kind_str, props.rarity),
                ];

                if let Some(s) = stack {
                    if s.max_stack > 1 {
                        lines.push(format!("Quantity: {}/{}", s.count, s.max_stack));
                    }
                }

                if is_equipped {
                    lines.push("[ EQUIPPED ]".to_string());
                }
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
                let is_equippable = Equipment::slot_for(props).is_some()
                    || props.kind == ItemKind::Ring;
                if is_equippable {
                    lines.push(if is_equipped { "[E] Unequip" } else { "[E] Equip" }.to_string());
                }
                if props.kind == ItemKind::Consumable || props.kind == ItemKind::Spellbook {
                    lines.push("[U] Use item".to_string());
                }
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
