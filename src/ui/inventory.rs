use bevy::prelude::*;

use crate::components::{Equipped, Inventory, Name};
use crate::game::actions::Action;
use crate::game::actions::SpeedStats;
use crate::game::combat::{Damage, Health};
use crate::game::enchantment::{
    display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified,
};
use crate::game::items::{Equipment, ItemKind, ItemProperties, ItemStack, SelectedInventorySlot};
use crate::game::actions::PendingPlayerAction;
use crate::game::staves::{Rechargeable, StaffData};
use crate::game::stats::{Armor, Dodge};
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
        .add_systems(
            OnExit(InGameState::Inventory),
            crate::ui::modal::despawn_screen::<OnInventoryScreen>,
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

#[derive(Component)]
struct EquipmentPaneText;

// --- Systems ---

fn inventory_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    // Both I and C toggle the inventory screen
    crate::ui::modal::toggle_screen(
        &keys,
        &state,
        &mut next_state,
        KeyCode::KeyI,
        InGameState::Inventory,
    );
    crate::ui::modal::toggle_screen(
        &keys,
        &state,
        &mut next_state,
        KeyCode::KeyC,
        InGameState::Inventory,
    );
}

fn reset_inventory_selection(mut slot: ResMut<SelectedInventorySlot>) {
    slot.0 = 0;
}

fn spawn_inventory_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    use crate::ui::modal::{spawn_modal, ModalConfig, GOLD};
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    spawn_modal(
        &mut commands,
        OnInventoryScreen,
        &font,
        &ModalConfig {
            title: "INVENTORY",
            title_color: GOLD,
            footer: "J/K Navigate | PgUp/PgDn Jump | E Equip | U Use | D Drop | I/C/Esc Close",
            width: 900.0,
            height: 520.0,
            ..Default::default()
        },
        |panel, font| {
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_grow: 1.0,
                    column_gap: Val::Px(12.0),
                    ..default()
                })
                .with_children(|cols| {
                    // --- Column 1: Equipment + Stats (200px) ---
                    cols.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        width: Val::Px(200.0),
                        overflow: Overflow::clip(),
                        ..default()
                    })
                    .with_children(|equip_col| {
                        equip_col.spawn((
                            Text::new("Loading..."),
                            TextFont {
                                font: font.clone(),
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.6, 0.6, 0.6)),
                            EquipmentPaneText,
                        ));
                    });

                    // Vertical separator
                    cols.spawn((
                        Node {
                            width: Val::Px(1.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
                    ));

                    // --- Column 2: Inventory List (280px) ---
                    cols.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        width: Val::Px(280.0),
                        overflow: Overflow::clip(),
                        ..default()
                    })
                    .with_children(|list| {
                        for i in 0..20 {
                            list.spawn((
                                Text::new(format!("{:2}. ---", i + 1)),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.5, 0.5, 0.5)),
                                InventorySlotText(i),
                            ));
                        }
                    });

                    // Vertical separator
                    cols.spawn((
                        Node {
                            width: Val::Px(1.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
                    ));

                    // --- Column 3: Item Detail + Comparison (370px) ---
                    cols.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        padding: UiRect::left(Val::Px(4.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    })
                    .with_children(|detail| {
                        detail.spawn((
                            Text::new("Select an item to\nsee its details."),
                            TextFont {
                                font: font.clone(),
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.6, 0.6, 0.6)),
                            InventoryDetailText,
                        ));
                    });
                });
        },
    );
}

/// Handles inventory navigation and item actions.
/// E and D both cost a turn: they set PendingPlayerAction and transition to Processing.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_inventory_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut slot: ResMut<SelectedInventorySlot>,
    mut pending: ResMut<PendingPlayerAction>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    mut next_turn: ResMut<NextState<TurnState>>,
    player_query: Query<
        (
            &Inventory,
            &Equipment,
            &Health,
            &Damage,
            &Armor,
            &Dodge,
            Option<&SpeedStats>,
        ),
        With<Player>,
    >,
    item_query: Query<(
        &Name,
        &ItemProperties,
        Has<Equipped>,
        Option<&ItemStack>,
        Option<&Enchantment>,
        Option<&ItemWeaponRunic>,
        Option<&ItemArmorRunic>,
        Option<&RunicIdentified>,
        Option<&StaffData>,
        Option<&Rechargeable>,
    )>,
    mut slot_texts: Query<(&mut Text, &mut TextColor, &InventorySlotText)>,
    mut detail_text: Query<
        (&mut Text, &mut TextColor),
        (
            With<InventoryDetailText>,
            Without<InventorySlotText>,
            Without<EquipmentPaneText>,
        ),
    >,
    mut equip_text: Query<
        (&mut Text, &mut TextColor),
        (
            With<EquipmentPaneText>,
            Without<InventorySlotText>,
            Without<InventoryDetailText>,
        ),
    >,
) {
    let Ok((inv, equipment, health, damage, armor, dodge, speed_stats)) =
        player_query.single()
    else {
        return;
    };
    let item_count = inv.items.len();

    // Build display-order: equipped items first, then unequipped
    let mut display_order: Vec<Entity> = Vec::with_capacity(item_count);
    for &e in &inv.items {
        if item_query.get(e).is_ok_and(|(_, _, is_eq, _, _, _, _, _, _, _)| is_eq) {
            display_order.push(e);
        }
    }
    for &e in &inv.items {
        if item_query.get(e).is_ok_and(|(_, _, is_eq, _, _, _, _, _, _, _)| !is_eq) {
            display_order.push(e);
        }
    }

    // Navigation (no turn cost, stays in inventory)
    if (keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK))
        && item_count > 0
        && slot.0 > 0
    {
        slot.0 -= 1;
    }
    if (keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ))
        && item_count > 0
        && slot.0 + 1 < item_count
    {
        slot.0 += 1;
    }
    // Page Up/Down -- jump 5 items
    if keys.just_pressed(KeyCode::PageUp) && item_count > 0 {
        slot.0 = slot.0.saturating_sub(5);
    }
    if keys.just_pressed(KeyCode::PageDown) && item_count > 0 {
        slot.0 = (slot.0 + 5).min(item_count.saturating_sub(1));
    }
    // Home/End -- jump to first/last
    if keys.just_pressed(KeyCode::Home) && item_count > 0 {
        slot.0 = 0;
    }
    if keys.just_pressed(KeyCode::End) && item_count > 0 {
        slot.0 = item_count.saturating_sub(1);
    }

    // Equip / Unequip -- costs a turn, stays in inventory
    if keys.just_pressed(KeyCode::KeyE)
        && let Some(&item_entity) = display_order.get(slot.0)
        && let Ok((_, props, is_equipped, _, _, _, _, _, _, _)) = item_query.get(item_entity)
        && Equipment::slot_for(props).is_some()
    {
        let action = if is_equipped {
            Action::UnequipItem { item: item_entity }
        } else {
            Action::EquipItem { item: item_entity }
        };
        pending.0 = Some(action);
        next_turn.set(TurnState::Processing);
    }

    // Drop -- costs a turn, stays in inventory
    if keys.just_pressed(KeyCode::KeyD)
        && let Some(&item_entity) = display_order.get(slot.0)
    {
        if slot.0 > 0 && slot.0 >= item_count.saturating_sub(1) {
            slot.0 -= 1;
        }
        pending.0 = Some(Action::DropItem { item: item_entity });
        next_turn.set(TurnState::Processing);
    }

    // Use -- generic action for any item. Downstream handlers decide behavior
    // (consumables apply effect, staves open targeting, etc.)
    if keys.just_pressed(KeyCode::KeyU)
        && let Some(&item_entity) = display_order.get(slot.0)
    {
        pending.0 = Some(Action::UseItem { item: item_entity });
        next_ingame.set(InGameState::Running);
        next_turn.set(TurnState::Processing);
        return;
    }

    // =========================================================================
    // Column 1: Equipment + Stats
    // =========================================================================
    if let Ok((mut text, mut color)) = equip_text.single_mut() {
        let mut lines: Vec<String> = Vec::new();

        // Slot display order and labels
        let slots: &[(&str, &str)] = &[
            ("weapon", "Weapon"),
            ("helm", "Helm"),
            ("chest", "Chest"),
            ("gloves", "Gloves"),
            ("boots", "Boots"),
            ("ring_l", "Ring L"),
            ("ring_r", "Ring R"),
            ("amulet", "Amulet"),
            ("offhand", "Offhand"),
        ];

        lines.push("EQUIPMENT".to_string());
        lines.push(String::new());

        for &(slot_id, label) in slots {
            let item_name = equipment
                .get_entity(slot_id)
                .and_then(|e| item_query.get(e).ok())
                .map(|(name, _props, _eq, _stack, enchant, w_runic, a_runic, runic_id, _staff, _rech)| {
                    display_item_name(&name.0, enchant, w_runic, a_runic, runic_id)
                })
                .unwrap_or_else(|| "---".to_string());
            lines.push(format!("{:<8} {}", label, item_name));
        }

        lines.push(String::new());
        lines.push("----------".to_string());
        lines.push(String::new());

        // Combat stats
        lines.push(format!("HP:     {}/{}", health.current, health.max));
        lines.push(format!("Damage: {}", damage.0));
        lines.push(format!("Armor:  {}", armor.0));
        lines.push(format!("Dodge:  {}", dodge.0));

        let move_str = speed_stats
            .map(|s| format!("{:.2}x", s.movement_delay))
            .unwrap_or_else(|| "1.00x".to_string());
        let atk_str = speed_stats
            .map(|s| format!("{:.2}x", s.attack_delay))
            .unwrap_or_else(|| "1.00x".to_string());
        lines.push(format!("Move:   {}", move_str));
        lines.push(format!("Atk:    {}", atk_str));

        text.0 = lines.join("\n");
        color.0 = Color::WHITE;
    }

    // =========================================================================
    // Column 2: Inventory list
    // =========================================================================
    for (mut text, mut color, slot_marker) in &mut slot_texts {
        let i = slot_marker.0;
        if let Some(&item_entity) = display_order.get(i) {
            if let Ok((name, _props, is_equipped, stack, enchant, weapon_runic, armor_runic, runic_id, _staff, _rech)) =
                item_query.get(item_entity)
            {
                let display_name =
                    display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
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

    // =========================================================================
    // Column 3: Item detail + comparison
    // =========================================================================
    if let Ok((mut text, mut color)) = detail_text.single_mut() {
        if let Some(&item_entity) = display_order.get(slot.0) {
            if let Ok((
                name,
                props,
                is_equipped,
                stack,
                enchant,
                weapon_runic,
                armor_runic,
                runic_id,
                staff_data,
                rechargeable,
            )) = item_query.get(item_entity)
            {
                let display_name =
                    display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
                let kind_str = match &props.armor_slot {
                    Some(s) => format!("{} ({})", props.kind, s),
                    None => props.kind.to_string(),
                };

                let mut lines = vec![display_name, kind_str];

                if let Some(s) = stack
                    && s.max_stack > 1
                {
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
                if props.defense > 0 || (ench_level > 0 && props.kind == ItemKind::Armor) {
                    let total_defense = props.defense + ench_level;
                    if ench_level > 0 && props.defense > 0 {
                        lines.push(format!(
                            "Defense: +{} ({}+{})",
                            total_defense, props.defense, ench_level
                        ));
                    } else {
                        lines.push(format!("Defense: +{}", total_defense));
                    }
                }
                if props.dodge_bonus != 0 {
                    lines.push(format!("Dodge: +{}", props.dodge_bonus));
                }
                if props.hit_bonus != 0 {
                    lines.push(format!("Hit: +{}", props.hit_bonus));
                }
                if props.damage_bonus != 0 {
                    lines.push(format!("Dmg Bonus: +{}", props.damage_bonus));
                }
                if props.max_hp_bonus != 0 {
                    lines.push(format!("Max HP: +{}", props.max_hp_bonus));
                }
                if props.regen_bonus != 0 {
                    lines.push(format!("Regen: +{}", props.regen_bonus));
                }
                if props.delay_modifier != 0.0 {
                    if props.delay_modifier < 0.0 {
                        lines.push(format!("Speed: +{:.0}% faster", -props.delay_modifier * 100.0));
                    } else {
                        lines.push(format!("Speed: {:.0}% slower", props.delay_modifier * 100.0));
                    }
                }

                // Staff charges
                if let Some(rech) = rechargeable {
                    lines.push(format!("Charges: {}/{}", rech.charges, rech.max_charges));
                }
                if let Some(sd) = staff_data {
                    let ench = ench_level;
                    lines.push(format!("Effect: {}", sd.effect.description(ench)));
                }

                // Show runic effect description if identified, with proc chance
                let is_identified = runic_id.is_some_and(|r| r.0);
                if let Some(wr) = weapon_runic {
                    if is_identified {
                        let damage_dice = props.damage.as_deref().unwrap_or("1d4");
                        let chance = crate::game::enchantment::weapon_runic_proc_chance(
                            &wr.0,
                            ench_level,
                            damage_dice,
                        );
                        let desc = wr.0.description();
                        lines.push(format!(
                            "Runic of {} ({}%): {}",
                            wr.0.name(),
                            chance,
                            desc
                        ));
                    } else {
                        lines.push("Runic: ???".to_string());
                    }
                }
                if let Some(ar) = armor_runic {
                    if is_identified {
                        let chance =
                            crate::game::enchantment::armor_runic_proc_chance(ar.0, ench_level);
                        let desc = ar.0.description();
                        lines.push(format!(
                            "Runic of {} ({}%): {}",
                            ar.0.name(),
                            chance,
                            desc
                        ));
                    } else {
                        lines.push("Runic: ???".to_string());
                    }
                }

                // =============================================================
                // Comparison section
                // =============================================================
                let target_slot = Equipment::slot_for(props);
                if let Some(slot_name) = target_slot {
                    lines.push(String::new());
                    if is_equipped {
                        lines.push("(currently equipped)".to_string());
                    } else {
                        // For rings, check both slots
                        let equipped_entity = if props.kind == ItemKind::Ring {
                            equipment
                                .get_entity("ring_l")
                                .or_else(|| equipment.get_entity("ring_r"))
                        } else {
                            equipment.get_entity(slot_name)
                        };

                        if let Some(eq_entity) = equipped_entity {
                            if let Ok((
                                eq_name,
                                eq_props,
                                _,
                                _,
                                eq_enchant,
                                eq_w_runic,
                                eq_a_runic,
                                eq_runic_id,
                                _,
                                _,
                            )) = item_query.get(eq_entity)
                            {
                                let eq_display = display_item_name(
                                    &eq_name.0,
                                    eq_enchant,
                                    eq_w_runic,
                                    eq_a_runic,
                                    eq_runic_id,
                                );
                                lines.push(format!("vs {}", eq_display));

                                let eq_ench = eq_enchant.map(|e| e.level).unwrap_or(0);

                                build_comparison_lines(
                                    &mut lines,
                                    props,
                                    ench_level,
                                    eq_props,
                                    eq_ench,
                                );
                            }
                        } else {
                            lines.push("(slot empty)".to_string());
                        }
                    }
                }

                // Action keys
                lines.push(String::new());
                let is_equippable =
                    Equipment::slot_for(props).is_some() || props.kind == ItemKind::Ring;
                if is_equippable {
                    lines.push(
                        if is_equipped {
                            "[E] Unequip"
                        } else {
                            "[E] Equip"
                        }
                        .to_string(),
                    );
                }
                if props.effect.is_some() || props.kind == ItemKind::Staff {
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

// ---------------------------------------------------------------------------
// Comparison helper
// ---------------------------------------------------------------------------

/// Format an integer delta as "+N" or "-N".
fn fmt_delta(delta: i32) -> String {
    if delta > 0 {
        format!("+{}", delta)
    } else {
        format!("{}", delta) // negative sign is already included
    }
}

/// Format a float delta as "+N.NNx" or "-N.NNx".
fn fmt_delta_f32(delta: f32, decimals: usize, suffix: &str) -> String {
    if delta > 0.0 {
        format!("+{:.prec$}{}", delta, suffix, prec = decimals)
    } else {
        format!("{:.prec$}{}", delta, suffix, prec = decimals)
    }
}

/// Builds stat comparison lines between `selected` and `equipped` items.
/// Positive deltas mean the selected item is better; negative means worse.
fn build_comparison_lines(
    lines: &mut Vec<String>,
    selected: &ItemProperties,
    selected_ench: i32,
    equipped: &ItemProperties,
    equipped_ench: i32,
) {
    // Defense (armor value + enchantment for armor items)
    let sel_def = if selected.kind == ItemKind::Armor {
        selected.defense + selected_ench
    } else {
        selected.defense
    };
    let eq_def = if equipped.kind == ItemKind::Armor {
        equipped.defense + equipped_ench
    } else {
        equipped.defense
    };
    if sel_def != 0 || eq_def != 0 {
        let delta = sel_def - eq_def;
        if delta != 0 {
            lines.push(format!("  Defense: {}", fmt_delta(delta)));
        }
    }

    // Dodge bonus
    let delta_dodge = selected.dodge_bonus - equipped.dodge_bonus;
    if delta_dodge != 0 {
        lines.push(format!("  Dodge: {}", fmt_delta(delta_dodge)));
    }

    // Hit bonus
    let delta_hit = selected.hit_bonus - equipped.hit_bonus;
    if delta_hit != 0 {
        lines.push(format!("  Hit: {}", fmt_delta(delta_hit)));
    }

    // Damage bonus
    let delta_dmg_bonus = selected.damage_bonus - equipped.damage_bonus;
    if delta_dmg_bonus != 0 {
        lines.push(format!("  Dmg Bonus: {}", fmt_delta(delta_dmg_bonus)));
    }

    // Max HP bonus
    let delta_hp = selected.max_hp_bonus - equipped.max_hp_bonus;
    if delta_hp != 0 {
        lines.push(format!("  Max HP: {}", fmt_delta(delta_hp)));
    }

    // Regen bonus
    let delta_regen = selected.regen_bonus - equipped.regen_bonus;
    if delta_regen != 0 {
        lines.push(format!("  Regen: {}", fmt_delta(delta_regen)));
    }

    // Attack speed (for weapons)
    if selected.attack_speed != 1.0 || equipped.attack_speed != 1.0 {
        let delta_speed = equipped.attack_speed - selected.attack_speed; // inverted: lower = faster = better
        if delta_speed.abs() > 0.001 {
            lines.push(format!("  Atk Speed: {}", fmt_delta_f32(delta_speed, 2, "x")));
        }
    }

    // Speed delay modifier
    if selected.delay_modifier != 0.0 || equipped.delay_modifier != 0.0 {
        let delta_delay = equipped.delay_modifier - selected.delay_modifier; // inverted: lower = better
        if delta_delay.abs() > 0.001 {
            lines.push(format!("  Speed Mod: {}", fmt_delta_f32(delta_delay * 100.0, 0, "%")));
        }
    }

    // Damage dice (just show both strings if they differ)
    if selected.damage != equipped.damage {
        let sel_dmg = selected
            .damage
            .as_deref()
            .unwrap_or("none");
        let eq_dmg = equipped
            .damage
            .as_deref()
            .unwrap_or("none");
        lines.push(format!("  Damage: {} vs {}", sel_dmg, eq_dmg));
    }
}

// Despawn handled by modal::despawn_screen::<OnInventoryScreen>

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_defense_delta() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            kind: ItemKind::Armor,
            defense: 5,
            ..Default::default()
        };
        let equipped = ItemProperties {
            kind: ItemKind::Armor,
            defense: 3,
            ..Default::default()
        };
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("Defense: +2")),
            "Expected defense delta of +2, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_defense_worse() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            kind: ItemKind::Armor,
            defense: 2,
            ..Default::default()
        };
        let equipped = ItemProperties {
            kind: ItemKind::Armor,
            defense: 5,
            ..Default::default()
        };
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("Defense: -3")),
            "Expected defense delta of -3, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_enchantment_affects_defense() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            kind: ItemKind::Armor,
            defense: 3,
            ..Default::default()
        };
        let equipped = ItemProperties {
            kind: ItemKind::Armor,
            defense: 3,
            ..Default::default()
        };
        // selected has +2 enchant, equipped has +0 => delta = (3+2)-(3+0) = +2
        build_comparison_lines(&mut lines, &selected, 2, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("Defense: +2")),
            "Expected defense delta of +2 from enchantment, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_dodge_bonus() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            dodge_bonus: 3,
            ..Default::default()
        };
        let equipped = ItemProperties {
            dodge_bonus: 1,
            ..Default::default()
        };
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("Dodge: +2")),
            "Expected dodge delta of +2, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_attack_speed() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            attack_speed: 0.5,
            ..Default::default()
        };
        let equipped = ItemProperties {
            attack_speed: 1.0,
            ..Default::default()
        };
        // delta = equipped - selected = 1.0 - 0.5 = 0.5 (positive = better for selected)
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("Atk Speed: +0.50x")),
            "Expected attack speed delta of +0.50x, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_no_deltas_when_equal() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            kind: ItemKind::Armor,
            defense: 3,
            dodge_bonus: 1,
            ..Default::default()
        };
        let equipped = ItemProperties {
            kind: ItemKind::Armor,
            defense: 3,
            dodge_bonus: 1,
            ..Default::default()
        };
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.is_empty(),
            "Expected no comparison lines for equal items, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_damage_dice_shown_when_different() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            kind: ItemKind::Weapon,
            damage: Some("2d6".to_string()),
            ..Default::default()
        };
        let equipped = ItemProperties {
            kind: ItemKind::Weapon,
            damage: Some("1d8".to_string()),
            ..Default::default()
        };
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("2d6 vs 1d8")),
            "Expected damage dice comparison, got: {:?}",
            lines
        );
    }

    #[test]
    fn comparison_damage_bonus_worse() {
        let mut lines = Vec::new();
        let selected = ItemProperties {
            damage_bonus: 1,
            ..Default::default()
        };
        let equipped = ItemProperties {
            damage_bonus: 3,
            ..Default::default()
        };
        build_comparison_lines(&mut lines, &selected, 0, &equipped, 0);
        assert!(
            lines.iter().any(|l| l.contains("Dmg Bonus: -2")),
            "Expected dmg bonus delta of -2, got: {:?}",
            lines
        );
    }
}
