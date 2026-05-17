//! Enchant target selection screen — shown after using a Scroll of Enchanting.
//! Player picks which equipped/inventory weapon or armor to enchant by +1.

use bevy::prelude::*;

use crate::components::{Equipped, Inventory, Name};
use crate::constants::BASE_ACTION_COST;
use crate::game::actions::{finish_turn, ActionFinishedEvent, ActionKind};
use crate::game::enchantment::{display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified};
use crate::game::items::{ItemKind, ItemProperties};
use crate::game::staves::{Rechargeable, StaffData};
use crate::game::stats::{Armor, DamageBonus};
use crate::game::turns::TurnState;
use crate::game::InGameState;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;
use crate::ui::registry::UiScreen;

/// Registry entry for the enchant-target selection screen. Event-driven:
/// entered when the player uses a Scroll of Enchanting (gameplay code
/// sets `NextState`); has no hotkey.
pub struct EnchantSelectScreen;

impl UiScreen for EnchantSelectScreen {
    const STATE: InGameState = InGameState::EnchantSelect;
    const OPEN_KEY: Option<KeyCode> = None;

    fn build(app: &mut App) {
        app.init_resource::<EnchantSelection>()
            .add_systems(OnEnter(Self::STATE), spawn_enchant_ui)
            .add_systems(
                Update,
                update_enchant_ui.run_if(in_state(Self::STATE)),
            )
            .add_systems(
                OnExit(Self::STATE),
                crate::ui::modal::despawn_screen::<OnEnchantScreen>,
            );
    }
}

#[derive(Component)]
struct OnEnchantScreen;

#[derive(Component)]
struct EnchantSlotText(usize);

#[derive(Resource, Default)]
struct EnchantSelection {
    index: usize,
}

fn spawn_enchant_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut selection: ResMut<EnchantSelection>,
) {
    selection.index = 0;

    use crate::ui::modal::{spawn_modal, ModalConfig};
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    spawn_modal(&mut commands, OnEnchantScreen, &font, &ModalConfig {
        title: "ENCHANT WHICH ITEM?",
        title_color: Color::srgb(1.0, 0.84, 0.0),
        footer: "↑/↓ Navigate  |  Enter - Enchant  |  Esc - Cancel",
        ..Default::default()
    }, |panel, font| {
        panel.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            ..default()
        }).with_children(|list| {
            for i in 0..10 {
                list.spawn((
                    Text::new(format!("{:2}. ---", i + 1)),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                    EnchantSlotText(i),
                ));
            }
        });
    });
}

fn update_enchant_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<EnchantSelection>,
    mut commands: Commands,
    inv_query: Query<(Entity, &Inventory), With<Player>>,
    item_query: Query<(
        &Name,
        &ItemProperties,
        Option<&Enchantment>,
        Option<&ItemWeaponRunic>,
        Option<&ItemArmorRunic>,
        Option<&RunicIdentified>,
        Has<Equipped>,
        Option<&StaffData>,
        Option<&Rechargeable>,
    )>,
    mut player_stats: Query<(&mut Armor, &mut DamageBonus), With<Player>>,
    mut slot_texts: Query<(&mut Text, &mut TextColor, &EnchantSlotText)>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    mut next_turn: ResMut<NextState<TurnState>>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, inv)) = inv_query.single() else { return; };

    // Build list of enchantable items (weapons and armor in inventory)
    let enchantable: Vec<Entity> = inv.items.iter()
        .filter(|&&e| {
            item_query.get(e).is_ok_and(|(_, props, _, _, _, _, _, _, _)| {
                matches!(props.kind, ItemKind::Weapon | ItemKind::Armor | ItemKind::Staff)
            })
        })
        .copied()
        .collect();

    let count = enchantable.len();

    // Navigation
    if (keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK))
        && count > 0 && selection.index > 0 {
        selection.index -= 1;
    }
    if (keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ))
        && count > 0 && selection.index + 1 < count {
        selection.index += 1;
    }

    // Cancel
    if keys.just_pressed(KeyCode::Escape) {
        // Put the scroll back — for simplicity, just cancel without consuming
        // (the scroll was already consumed in handle_use_item, so this is a slight cheat;
        //  a proper fix would defer consumption, but that's a bigger refactor)
        next_ingame.set(InGameState::Running);
        return;
    }

    // Confirm enchant
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        if let Some(&target_entity) = enchantable.get(selection.index) {
            if let Ok((name, props, enchant, _, _, _, is_equipped, staff_data, rechargeable)) = item_query.get(target_entity) {
                let old_level = enchant.map(|e| e.level).unwrap_or(0);
                let new_level = old_level + 1;

                // Only update player stats if item is currently equipped
                if is_equipped {
                    if let Ok((mut armor, mut dmg_bonus)) = player_stats.single_mut() {
                        match props.kind {
                            ItemKind::Weapon => { dmg_bonus.0 += 1; }
                            ItemKind::Armor => { armor.0 += 1; }
                            _ => {}
                        }
                    }
                }

                // Update or insert enchantment
                if enchant.is_some() {
                    // Can't mutably get from item_query since we already borrowed immutably.
                    // Use commands instead.
                    commands.entity(target_entity).insert(Enchantment { level: new_level });
                } else {
                    commands.entity(target_entity).insert(Enchantment { level: 1 });
                }

                // Update staff rechargeable stats when enchanting a staff
                if let Some(sd) = staff_data {
                    if let Some(rech) = rechargeable {
                        let mut updated = rech.clone();
                        updated.update_from_enchantment(sd.base_recharge, new_level);
                        commands.entity(target_entity).insert(updated);
                    }
                }

                log_writer.write(GameLogMessage(format!(
                    "Your {} glows! It is now a +{} {}.",
                    name.0, new_level, name.0
                )));

                // Cost a turn and return to gameplay
                finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
                next_ingame.set(InGameState::Running);
                next_turn.set(TurnState::Processing);
                return;
            }
        }
    }

    // Update slot display
    for (mut text, mut color, marker) in &mut slot_texts {
        let i = marker.0;
        if let Some(&item_entity) = enchantable.get(i) {
            if let Ok((name, _props, enchant, weapon_runic, armor_runic, runic_id, _, _, _)) = item_query.get(item_entity) {
                let dname = display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
                let prefix = if i == selection.index { "> " } else { "  " };
                text.0 = format!("{}{:2}. {}", prefix, i + 1, dname);
                color.0 = if i == selection.index {
                    Color::srgb(1.0, 0.84, 0.0)
                } else {
                    Color::WHITE
                };
            }
        } else {
            text.0 = String::new();
            color.0 = Color::srgb(0.3, 0.3, 0.3);
        }
    }
}
