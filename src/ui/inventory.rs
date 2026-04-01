use bevy::prelude::*;

use crate::components::{Equipped, Inventory, Name};
use crate::game::actions::Action;
use crate::game::enchantment::{display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified};
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
        .add_systems(OnExit(InGameState::Inventory), crate::ui::modal::despawn_screen::<OnInventoryScreen>);
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
    crate::ui::modal::toggle_screen(&keys, &state, &mut next_state, KeyCode::KeyI, InGameState::Inventory);
}

fn reset_inventory_selection(mut slot: ResMut<SelectedInventorySlot>) {
    slot.0 = 0;
}

fn spawn_inventory_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    use crate::ui::modal::{spawn_modal, ModalConfig};
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    spawn_modal(&mut commands, OnInventoryScreen, &font, &ModalConfig {
        title: "INVENTORY",
        title_color: Color::srgb(1.0, 0.84, 0.0),
        footer: "↑/↓ Navigate  |  PgUp/PgDn Jump  |  E - Equip  |  U - Use  |  D - Drop  |  I/Esc - Close",
        ..Default::default()
    }, |panel, font| {
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
    item_query: Query<(&Name, &ItemProperties, Has<Equipped>, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>)>,
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

    // Build display-order: equipped items first, then unequipped
    let mut display_order: Vec<Entity> = Vec::with_capacity(item_count);
    for &e in &inv.items {
        if item_query.get(e).is_ok_and(|(_, _, is_eq, _, _, _, _, _)| is_eq) {
            display_order.push(e);
        }
    }
    for &e in &inv.items {
        if item_query.get(e).is_ok_and(|(_, _, is_eq, _, _, _, _, _)| !is_eq) {
            display_order.push(e);
        }
    }

    // Navigation (no turn cost, stays in inventory)
    if (keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK))
        && item_count > 0 && slot.0 > 0 {
            slot.0 -= 1;
        }
    if (keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ))
        && item_count > 0 && slot.0 + 1 < item_count {
            slot.0 += 1;
        }
    // Page Up/Down — jump 5 items
    if keys.just_pressed(KeyCode::PageUp) && item_count > 0 {
        slot.0 = slot.0.saturating_sub(5);
    }
    if keys.just_pressed(KeyCode::PageDown) && item_count > 0 {
        slot.0 = (slot.0 + 5).min(item_count.saturating_sub(1));
    }
    // Home/End — jump to first/last
    if keys.just_pressed(KeyCode::Home) && item_count > 0 {
        slot.0 = 0;
    }
    if keys.just_pressed(KeyCode::End) && item_count > 0 {
        slot.0 = item_count.saturating_sub(1);
    }

    // Equip / Unequip — costs a turn, stays in inventory
    if keys.just_pressed(KeyCode::KeyE)
        && let Some(&item_entity) = display_order.get(slot.0)
            && let Ok((_, props, is_equipped, _, _, _, _, _)) = item_query.get(item_entity)
                && Equipment::slot_for(props).is_some() {
                    let action = if is_equipped {
                        Action::UnequipItem { item: item_entity }
                    } else {
                        Action::EquipItem { item: item_entity }
                    };
                    pending.0 = Some(action);
                    next_turn.set(TurnState::Processing);
                }

    // Drop — costs a turn, stays in inventory
    if keys.just_pressed(KeyCode::KeyD)
        && let Some(&item_entity) = display_order.get(slot.0) {
            if slot.0 > 0 && slot.0 >= item_count.saturating_sub(1) {
                slot.0 -= 1;
            }
            pending.0 = Some(Action::DropItem { item: item_entity });
            next_turn.set(TurnState::Processing);
        }

    // Use / consume — costs a turn, exits inventory (may trigger sub-screen like enchant)
    if keys.just_pressed(KeyCode::KeyU)
        && let Some(&item_entity) = display_order.get(slot.0)
            && let Ok((_, props, _, _, _, _, _, _)) = item_query.get(item_entity)
                && (props.kind == ItemKind::Consumable || props.kind == ItemKind::Spellbook) {
                    if slot.0 > 0 && slot.0 >= item_count.saturating_sub(1) {
                        slot.0 -= 1;
                    }
                    pending.0 = Some(Action::UseItem { item: item_entity });
                    next_ingame.set(InGameState::Running);
                    next_turn.set(TurnState::Processing);
                    return;
                }

    // Update slot list
    for (mut text, mut color, slot_marker) in &mut slot_texts {
        let i = slot_marker.0;
        if let Some(&item_entity) = display_order.get(i) {
            if let Ok((name, props, is_equipped, stack, enchant, weapon_runic, armor_runic, runic_id)) = item_query.get(item_entity) {
                let display_name = display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
                let equipped_tag = if is_equipped { " [E]" } else { "" };
                let stack_tag = match stack {
                    Some(s) if s.count > 1 => format!(" (x{})", s.count),
                    _ => String::new(),
                };
                text.0 = format!("{:2}. {}{}{}", i + 1, display_name, stack_tag, equipped_tag);
                color.0 = Color::WHITE;
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
        if let Some(&item_entity) = display_order.get(slot.0) {
            if let Ok((name, props, is_equipped, stack, enchant, weapon_runic, armor_runic, runic_id)) = item_query.get(item_entity) {
                let display_name = display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
                let kind_str = match &props.armor_slot {
                    Some(s) => format!("{} ({})", props.kind, s),
                    None => props.kind.to_string(),
                };

                let mut lines = vec![
                    display_name,
                    kind_str,
                ];

                if let Some(s) = stack
                    && s.max_stack > 1 {
                        lines.push(format!("Quantity: {}/{}", s.count, s.max_stack));
                    }

                if is_equipped {
                    lines.push("[ EQUIPPED ]".to_string());
                }
                let ench_level = enchant.map(|e| e.level).unwrap_or(0);
                if let Some(dmg) = &props.damage {
                    if ench_level > 0 {
                        lines.push(format!("Damage: {} +{}", dmg, ench_level));
                    } else {
                        lines.push(format!("Damage: {}", dmg));
                    }
                }
                if props.attack_speed != 0.0 && props.attack_speed != 1.0 {
                    if props.attack_speed < 1.0 {
                        lines.push(format!("Speed: Fast ({}x)", props.attack_speed));
                    } else {
                        lines.push(format!("Speed: Slow ({}x)", props.attack_speed));
                    }
                }
                if props.defense > 0 || ench_level > 0 {
                    let total_defense = props.defense + ench_level;
                    if ench_level > 0 && props.defense > 0 {
                        lines.push(format!("Defense: +{} ({}+{})", total_defense, props.defense, ench_level));
                    } else {
                        lines.push(format!("Defense: +{}", total_defense));
                    }
                }

                // Show runic effect description if identified, with proc chance
                let is_identified = runic_id.is_some_and(|r| r.0);
                if let Some(wr) = weapon_runic {
                    if is_identified {
                        let damage_dice = props.damage.as_deref().unwrap_or("1d4");
                        let chance = crate::game::enchantment::weapon_runic_proc_chance(&wr.0, ench_level, damage_dice);
                        let desc = wr.0.description();
                        lines.push(format!("Runic of {} ({}%): {}", wr.0.name(), chance, desc));
                    } else {
                        lines.push("Runic: ???".to_string());
                    }
                }
                if let Some(ar) = armor_runic {
                    if is_identified {
                        let chance = crate::game::enchantment::armor_runic_proc_chance(ar.0, ench_level);
                        let desc = ar.0.description();
                        lines.push(format!("Runic of {} ({}%): {}", ar.0.name(), chance, desc));
                    } else {
                        lines.push("Runic: ???".to_string());
                    }
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
                color.0 = Color::WHITE;
            }
        } else {
            text.0 = "Select an item to\nsee its details.".to_string();
            color.0 = Color::srgb(0.6, 0.6, 0.6);
        }
    }
}

// Despawn handled by modal::despawn_screen::<OnInventoryScreen>
