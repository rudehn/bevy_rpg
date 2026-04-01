//! Staff selection screen — shown when player presses Z to zap.
//! Player picks which staff to use, then enters targeting mode.

use bevy::prelude::*;

use crate::components::{Inventory, Name};
use crate::game::enchantment::{display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified};
use crate::game::items::{ItemKind, ItemProperties};
use crate::game::actions::PendingPlayerAction;
use crate::game::staves::{Rechargeable, StaffData, StaffEffect};
use crate::game::targeting::{TargetingContext, TargetingMode};
use crate::game::turns::TurnState;
use crate::game::{AppState, InGameState};
use crate::player::Player;

pub struct StaffSelectPlugin;

impl Plugin for StaffSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StaffSelection>()
            .add_systems(OnEnter(InGameState::StaffSelect), spawn_staff_select_ui)
            .add_systems(
                Update,
                update_staff_select_ui.run_if(in_state(InGameState::StaffSelect)),
            )
            .add_systems(OnExit(InGameState::StaffSelect), crate::ui::modal::despawn_screen::<OnStaffSelectScreen>);
    }
}

#[derive(Component)]
struct OnStaffSelectScreen;

#[derive(Component)]
struct StaffSlotText(usize);

#[derive(Resource, Default)]
struct StaffSelection {
    index: usize,
}

fn spawn_staff_select_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut selection: ResMut<StaffSelection>,
) {
    selection.index = 0;

    use crate::ui::modal::{spawn_modal, ModalConfig};
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    spawn_modal(&mut commands, OnStaffSelectScreen, &font, &ModalConfig {
        title: "ZAP WHICH STAFF?",
        title_color: Color::srgb(0.5, 0.8, 1.0),
        footer: "↑/↓ Navigate  |  Enter - Select  |  Esc - Cancel",
        ..Default::default()
    }, |panel, font| {
        panel.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            ..default()
        }).with_children(|list| {
            for i in 0..10 {
                list.spawn((
                    Text::new(""),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                    StaffSlotText(i),
                ));
            }
        });
    });
}

fn update_staff_select_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<StaffSelection>,
    inv_query: Query<(Entity, &Inventory), With<Player>>,
    item_query: Query<(
        &Name,
        &ItemProperties,
        &StaffData,
        &Rechargeable,
        Option<&Enchantment>,
        Option<&ItemWeaponRunic>,
        Option<&ItemArmorRunic>,
        Option<&RunicIdentified>,
    )>,
    mut slot_texts: Query<(&mut Text, &mut TextColor, &StaffSlotText)>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    mut next_turn_state: ResMut<NextState<TurnState>>,
    mut targeting_context: ResMut<TargetingContext>,
    mut pending: ResMut<PendingPlayerAction>,
) {
    let Ok((player_entity, inv)) = inv_query.single() else { return; };

    // Build list of staves in inventory
    let staves: Vec<Entity> = inv.items.iter()
        .filter(|&&e| item_query.get(e).is_ok())
        .copied()
        .collect();

    let count = staves.len();

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
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::KeyZ) {
        next_ingame.set(InGameState::Running);
        return;
    }

    // Confirm — select staff and enter targeting
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        if let Some(&staff_entity) = staves.get(selection.index) {
            if let Ok((_, _, staff_data, rech, enchant, _, _, _)) = item_query.get(staff_entity) {
                if rech.charges <= 0 {
                    // No charges — don't enter targeting
                    return;
                }
                let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
                let range = staff_data.effect.range(enchant_level);

                // Self-targeting staves (Healing) skip the targeting screen
                if !staff_data.effect.needs_target() {
                    pending.0 = Some(crate::game::actions::Action::ZapStaff {
                        staff_entity,
                        target: player_entity,
                        target_pos: None,
                    });
                    next_turn_state.set(TurnState::Processing);
                    next_ingame.set(InGameState::Running);
                    return;
                }

                // Set targeting mode based on staff effect
                match staff_data.effect {
                    StaffEffect::Blinking => {
                        targeting_context.mode = TargetingMode::Tile {
                            slot: 0, // unused for staves
                            range,
                            radius: 0,
                        };
                    }
                    _ => {
                        targeting_context.mode = TargetingMode::Staff {
                            staff_entity,
                        };
                    }
                }
                // Store the staff entity in the targeting context for later
                targeting_context.staff_entity = Some(staff_entity);
                next_ingame.set(InGameState::Targeting);
                return;
            }
        }
    }

    // Update slot display
    for (mut text, mut color, marker) in &mut slot_texts {
        let i = marker.0;
        if let Some(&item_entity) = staves.get(i) {
            if let Ok((name, _props, staff_data, rech, enchant, weapon_runic, armor_runic, runic_id)) = item_query.get(item_entity) {
                let dname = display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
                let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
                let desc = staff_data.effect.description(enchant_level);
                let prefix = if i == selection.index { "> " } else { "  " };
                text.0 = format!("{}{} [{}/{}]  {}", prefix, dname, rech.charges, rech.max_charges, desc);
                color.0 = if rech.charges <= 0 {
                    Color::srgb(0.5, 0.3, 0.3) // Depleted — dim red
                } else if i == selection.index {
                    Color::srgb(0.5, 0.8, 1.0) // Selected — bright blue
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
